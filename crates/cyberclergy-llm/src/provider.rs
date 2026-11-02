//! Provider abstraction and the registry that performs rotation and retries.
//!
//! The registry is the only place that knows about "many providers at once".
//! Adapters stay dumb: one request in, one normalised response out.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

use crate::config::{ProviderStatus, Settings, SharedSettings};
use crate::errors::LlmError;
use crate::providers::cloudflare::CloudflareProvider;
use crate::providers::gemini::GeminiProvider;
use crate::providers::openai_compat::OpenAiCompatProvider;
use crate::types::{ChatRequest, ChatResponse, ProviderId};

/// Upper bound for a locally computed backoff delay. Providers that ask for a
/// longer pause still get the delay from `Retry-After`.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const BASE_BACKOFF: Duration = Duration::from_millis(500);

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Default model configured for this provider.
    fn model(&self) -> &str;

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError>;
}

/// Holds every configured provider and the shared call policy.
pub struct Registry {
    providers: BTreeMap<ProviderId, Arc<dyn Provider>>,
    shared: SharedSettings,
    statuses: Vec<ProviderStatus>,
}

impl Registry {
    /// Builds the registry from settings, using a client with the shared timeout.
    pub fn new(settings: &Settings) -> Result<Self, LlmError> {
        let client = Client::builder()
            .timeout(settings.shared.timeout)
            .build()
            .map_err(|error| LlmError::Internal {
                detail: format!("could not build the HTTP client: {error}"),
            })?;
        Self::with_client(settings, client)
    }

    /// Builds the registry with a caller-provided HTTP client (tests, proxies).
    pub fn with_client(settings: &Settings, client: Client) -> Result<Self, LlmError> {
        let timeout = settings.shared.timeout;
        let mut providers: BTreeMap<ProviderId, Arc<dyn Provider>> = BTreeMap::new();

        for id in [
            ProviderId::OpenRouter,
            ProviderId::Groq,
            ProviderId::Mistral,
            ProviderId::Nvidia,
        ] {
            let endpoint = settings
                .endpoint(id)
                .expect("openai-compatible providers always have an endpoint");
            if endpoint.is_configured() {
                providers.insert(
                    id,
                    Arc::new(OpenAiCompatProvider::new(
                        id,
                        endpoint.clone(),
                        client.clone(),
                        timeout,
                    )),
                );
            }
        }

        if settings.gemini.is_configured() {
            providers.insert(
                ProviderId::Gemini,
                Arc::new(GeminiProvider::new(
                    settings.gemini.clone(),
                    client.clone(),
                    timeout,
                )),
            );
        }

        if settings.cloudflare.is_configured() {
            providers.insert(
                ProviderId::Cloudflare,
                Arc::new(CloudflareProvider::new(
                    settings.cloudflare.clone(),
                    client.clone(),
                    timeout,
                )),
            );
        }

        Ok(Self {
            providers,
            shared: settings.shared.clone(),
            statuses: settings.statuses(),
        })
    }

    /// Convenience constructor reading the process environment.
    pub fn from_env() -> Result<Self, LlmError> {
        Self::new(&Settings::from_env())
    }

    pub fn available(&self) -> Vec<ProviderId> {
        self.providers.keys().copied().collect()
    }

    pub fn is_available(&self, id: ProviderId) -> bool {
        self.providers.contains_key(&id)
    }

    pub fn shared(&self) -> &SharedSettings {
        &self.shared
    }

    /// Configuration state of every known provider, including unavailable ones.
    pub fn statuses(&self) -> &[ProviderStatus] {
        &self.statuses
    }

    /// Calls one specific provider, retrying transient failures.
    pub async fn chat(
        &self,
        id: ProviderId,
        request: &ChatRequest,
    ) -> Result<ChatResponse, LlmError> {
        let provider = self
            .providers
            .get(&id)
            .ok_or(LlmError::NotConfigured(id))?;
        self.call_with_retries(provider.as_ref(), request).await
    }

    /// Tries providers in the configured `CC_PROVIDER_ORDER`, skipping those
    /// that are not configured.
    pub async fn chat_with_fallback(
        &self,
        request: &ChatRequest,
    ) -> Result<ChatResponse, LlmError> {
        if self.providers.is_empty() {
            return Err(LlmError::NoProvidersConfigured);
        }
        let order: Vec<ProviderId> = self
            .shared
            .provider_order
            .iter()
            .copied()
            .filter(|id| self.providers.contains_key(id))
            .collect();
        if order.is_empty() {
            // Configured providers exist, but none of them appear in the order.
            return self.chat_with_order(request, &self.available()).await;
        }
        self.chat_with_order(request, &order).await
    }

    /// Tries providers in an explicit order and aggregates the failures.
    pub async fn chat_with_order(
        &self,
        request: &ChatRequest,
        order: &[ProviderId],
    ) -> Result<ChatResponse, LlmError> {
        let mut attempts: Vec<String> = Vec::new();
        for id in order {
            let Some(provider) = self.providers.get(id) else {
                attempts.push(format!("{id}: not configured"));
                continue;
            };
            match self.call_with_retries(provider.as_ref(), request).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    tracing::warn!(provider = %id, error = %error, "provider call failed, trying the next one");
                    attempts.push(format!("{id}: {error}"));
                }
            }
        }
        if attempts.is_empty() {
            return Err(LlmError::NoProvidersConfigured);
        }
        Err(LlmError::AllProvidersFailed {
            attempts: attempts.join("; "),
        })
    }

    async fn call_with_retries(
        &self,
        provider: &dyn Provider,
        request: &ChatRequest,
    ) -> Result<ChatResponse, LlmError> {
        let effective = self.apply_defaults(request);
        let mut attempt: u32 = 0;
        loop {
            match provider.chat(&effective).await {
                Ok(response) => return Ok(response),
                Err(error) if error.is_retryable() && attempt < self.shared.max_retries => {
                    let delay = error
                        .retry_after()
                        .unwrap_or_else(|| backoff_delay(attempt))
                        .min(MAX_BACKOFF);
                    tracing::debug!(
                        provider = %provider.id(),
                        attempt = attempt + 1,
                        delay_ms = delay.as_millis() as u64,
                        error = %error,
                        "retrying after a transient failure"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Fills in the global temperature and token budget when the caller omits them.
    fn apply_defaults(&self, request: &ChatRequest) -> ChatRequest {
        let mut effective = request.clone();
        if effective.temperature.is_none() {
            effective.temperature = Some(self.shared.temperature);
        }
        if effective.max_tokens.is_none() {
            effective.max_tokens = Some(self.shared.max_tokens);
        }
        effective
    }
}

fn backoff_delay(attempt: u32) -> Duration {
    let factor = 1u32 << attempt.min(5);
    BASE_BACKOFF.saturating_mul(factor)
}
