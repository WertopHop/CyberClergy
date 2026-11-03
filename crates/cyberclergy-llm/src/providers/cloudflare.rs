//! Adapter for Cloudflare Workers AI.
//!
//! Cloudflare authenticates with a token but also needs the account id in the
//! URL path, and it wraps payloads in `{"success":…,"result":…,"errors":…}`.
//! Both text-generation shapes are accepted: the classic `result.response`
//! string and the newer OpenAI-like `result.choices`.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::config::{ProviderConfig, VAR_CLOUDFLARE_ACCOUNT_ID};
use crate::errors::LlmError;
use crate::provider::Provider;
use crate::providers::{
    extract_error_message, map_status, parse_retry_after, transport_error, truncate,
};
use crate::types::{ChatRequest, ChatResponse, ProviderId, Role, Usage};

pub struct CloudflareProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    timeout: Duration,
}

impl CloudflareProvider {
    pub fn new(config: ProviderConfig, client: reqwest::Client, timeout: Duration) -> Self {
        Self {
            config,
            client,
            timeout,
        }
    }

    fn endpoint_url(&self) -> Result<String, LlmError> {
        let account_id = self.config.required_value(VAR_CLOUDFLARE_ACCOUNT_ID).ok_or_else(
            || LlmError::InvalidConfig {
                provider: ProviderId::Cloudflare,
                detail: format!("{VAR_CLOUDFLARE_ACCOUNT_ID} is not set"),
            },
        )?;
        Ok(format!(
            "{}/accounts/{account_id}/ai/run/{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.model
        ))
    }
}

#[derive(Serialize)]
struct WireMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct WorkersAiBody {
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[async_trait]
impl Provider for CloudflareProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Cloudflare
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let api_token = self
            .config
            .api_key
            .clone()
            .ok_or(LlmError::NotConfigured(ProviderId::Cloudflare))?;

        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        if let Some(system) = request.system.as_deref().filter(|text| !text.trim().is_empty()) {
            messages.push(WireMessage {
                role: Role::System.as_str(),
                content: system.to_string(),
            });
        }
        for message in &request.messages {
            messages.push(WireMessage {
                role: message.role.as_str(),
                content: message.content.clone(),
            });
        }
        if messages.is_empty() {
            return Err(LlmError::InvalidRequest {
                provider: ProviderId::Cloudflare,
                detail: "the request contains no messages".to_string(),
            });
        }

        let body = WorkersAiBody {
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };
        let url = self.endpoint_url()?;

        let started = Instant::now();
        let response = self
            .client
            .post(&url)
            .bearer_auth(api_token)
            .json(&body)
            .send()
            .await
            .map_err(|error| transport_error(ProviderId::Cloudflare, error, self.timeout))?;
        let latency = started.elapsed();

        let status = response.status();
        if !status.is_success() {
            let retry_after = parse_retry_after(response.headers());
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status(
                ProviderId::Cloudflare,
                status,
                retry_after,
                &body_text,
            ));
        }

        let body_text = response
            .text()
            .await
            .map_err(|error| transport_error(ProviderId::Cloudflare, error, self.timeout))?;
        let payload: Value =
            serde_json::from_str(&body_text).map_err(|error| LlmError::Parse {
                provider: ProviderId::Cloudflare,
                detail: format!(
                    "could not decode the completion: {error}; body: {}",
                    truncate(&body_text, 300)
                ),
            })?;

        if payload.get("success").and_then(Value::as_bool) == Some(false) {
            return Err(LlmError::Server {
                provider: ProviderId::Cloudflare,
                status: status.as_u16(),
                detail: extract_error_message(&payload.to_string()),
            });
        }

        let result = payload.get("result").cloned().unwrap_or(Value::Null);
        let text = result
            .get("response")
            .and_then(Value::as_str)
            .or_else(|| {
                result
                    .pointer("/choices/0/message/content")
                    .and_then(Value::as_str)
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(LlmError::Parse {
                provider: ProviderId::Cloudflare,
                detail: "the provider returned an empty completion".to_string(),
            });
        }

        let usage = result.get("usage").map(|usage| Usage {
            prompt_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .map(|value| value as u32),
            completion_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .map(|value| value as u32),
            total_tokens: usage
                .get("total_tokens")
                .and_then(Value::as_u64)
                .map(|value| value as u32),
        });

        Ok(ChatResponse {
            provider: ProviderId::Cloudflare,
            model: self.config.model.clone(),
            text,
            usage,
            latency,
        })
    }
}
