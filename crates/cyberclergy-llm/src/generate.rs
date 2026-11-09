//! The single funnel every provider goes through when composing a prayer.
//!
//! Keeping this in one place guarantees that all networks receive the exact
//! same system prompt and user turn; only the transport differs.

use crate::errors::LlmError;
use crate::prompts::{PrayerSpec, PromptError, PromptTemplate};
use crate::provider::Registry;
use crate::types::{ChatMessage, ChatRequest, ChatResponse, ProviderId};

/// The user turn is intentionally minimal: all instructions live in the
/// editable system prompt under `resources/prompts/system_prompt.md`.
pub const USER_INSTRUCTION: &str = "Compose the prayers now.";
/// Ceiling for a batched response. Free tiers start rejecting the request
/// outright well before this, and a batch that dies on a 400 saves nothing.
pub const BATCH_TOKEN_CAP: u32 = 4096;

/// Renders the system prompt and wraps it into a provider-agnostic request.
pub fn build_prayer_request(
    template: &PromptTemplate,
    spec: &PrayerSpec,
) -> Result<ChatRequest, PromptError> {
    let system = template.render(&spec.context())?;
    Ok(ChatRequest {
        system: Some(system),
        messages: vec![ChatMessage::user(USER_INSTRUCTION)],
        model: None,
        temperature: None,
        max_tokens: None,
    })
}

/// Owns the provider registry together with the prayer template.
pub struct PrayerGenerator {
    registry: Registry,
    template: PromptTemplate,
}

impl PrayerGenerator {
    /// Uses the embedded system prompt.
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            template: PromptTemplate::system_prompt(),
        }
    }

    /// Uses a custom template (useful for religion-specific overrides later).
    pub fn with_template(registry: Registry, template: PromptTemplate) -> Self {
        Self { registry, template }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn template(&self) -> &PromptTemplate {
        &self.template
    }

    pub fn build_request(&self, spec: &PrayerSpec) -> Result<ChatRequest, PromptError> {
        build_prayer_request(&self.template, spec)
    }

    /// Composes one prayer, walking the configured provider order on failure.
    pub async fn generate(&self, spec: &PrayerSpec) -> Result<ChatResponse, LlmError> {
        let request = self.prepare(spec)?;
        self.registry.chat_with_fallback(&request).await
    }

    /// Composes one prayer through the cleric's own network, falling back to
    /// the rest of the rotation so an exhausted free tier never silences a
    /// robot. When nothing else answers, the slot's own failure is reported.
    pub async fn generate_for(
        &self,
        provider: Option<ProviderId>,
        spec: &PrayerSpec,
    ) -> Result<ChatResponse, LlmError> {
        let Some(provider) = provider else {
            return self.generate(spec).await;
        };
        let request = self.prepare(spec)?;
        match self.registry.chat(provider, &request).await {
            Ok(response) => Ok(response),
            Err(primary) => {
                tracing::warn!(
                    provider = %provider,
                    error = %primary,
                    "the cleric's own network failed, falling back to the rotation"
                );
                match self
                    .registry
                    .chat_with_fallback_except(provider, &request)
                    .await
                {
                    Ok(response) => Ok(response),
                    Err(_) => Err(primary),
                }
            }
        }
    }

    /// Sends an arbitrary request through the shared rotation. Used by the
    /// religion builder, which needs a structured answer rather than a prayer.
    pub async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.registry.chat_with_fallback(request).await
    }

    /// Composes one prayer through a specific provider.
    pub async fn generate_with_provider(
        &self,
        provider: ProviderId,
        spec: &PrayerSpec,
    ) -> Result<ChatResponse, LlmError> {
        let request = self.prepare(spec)?;
        self.registry.chat(provider, &request).await
    }

    fn prepare(&self, spec: &PrayerSpec) -> Result<ChatRequest, LlmError> {
        let mut request =
            self.build_request(spec)
                .map_err(|error| LlmError::PromptInvalid {
                    detail: error.to_string(),
                })?;
        request.max_tokens = Some(self.batch_budget(spec.batch()));
        Ok(request)
    }

    /// One request now answers several prayers, so the token budget grows with
    /// the batch; otherwise the last prayers of a batch are cut mid-sentence.
    fn batch_budget(&self, batch: u32) -> u32 {
        self.registry
            .shared()
            .max_tokens
            .saturating_mul(batch)
            .min(BATCH_TOKEN_CAP)
    }
}
