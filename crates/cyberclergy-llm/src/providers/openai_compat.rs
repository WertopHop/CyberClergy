//! Adapter for endpoints that implement the OpenAI chat-completions schema:
//! OpenRouter, Groq, Mistral AI and NVIDIA NIM.
//!
//! These providers differ only in base URL, credentials, model and optional
//! attribution headers, all of which come from [`ProviderConfig`]. A new
//! provider with the same protocol needs no code here.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::ProviderConfig;
use crate::errors::LlmError;
use crate::provider::Provider;
use crate::providers::{map_status, parse_retry_after, transport_error, truncate};
use crate::types::{ChatRequest, ChatResponse, ProviderId, Usage};

pub struct OpenAiCompatProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    timeout: Duration,
}

impl OpenAiCompatProvider {
    pub fn new(config: ProviderConfig, client: reqwest::Client, timeout: Duration) -> Self {
        Self {
            config,
            client,
            timeout,
        }
    }
}

#[derive(Serialize)]
struct WireMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionBody {
    model: String,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    model: Option<String>,
    #[serde(default)]
    choices: Vec<Choice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: AssistantMessage,
    #[serde(rename = "finish_reason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct AssistantMessage {
    /// A string for plain completions, or an array of parts for multimodal
    /// endpoints; both shapes are handled by [`message_text`].
    content: Option<Value>,
    /// Reasoning models emit their private chain of thought here. It is never
    /// treated as prayer text, but its presence explains empty completions.
    #[serde(rename = "reasoning_content")]
    reasoning_content: Option<String>,
}

/// Extracts plain text from either a string or an array of text parts.
fn message_text(message: &AssistantMessage) -> String {
    match &message.content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

impl From<OpenAiUsage> for Usage {
    fn from(usage: OpenAiUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}

#[async_trait]
impl Provider for OpenAiCompatProvider {
    fn id(&self) -> ProviderId {
        self.config.id()
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let id = self.config.id();
        let api_key = self
            .config
            .api_key
            .clone()
            .ok_or(LlmError::NotConfigured(id))?;

        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        if let Some(system) = request.system.as_deref().filter(|text| !text.trim().is_empty()) {
            messages.push(WireMessage {
                role: "system",
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
                provider: id,
                detail: "the request contains no messages".to_string(),
            });
        }

        let body = ChatCompletionBody {
            model: request
                .model
                .clone()
                .unwrap_or_else(|| self.config.model.clone()),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
        let mut builder = self.client.post(&url).bearer_auth(api_key).json(&body);
        for (name, value) in &self.config.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let started = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|error| transport_error(id, error, self.timeout))?;
        let latency = started.elapsed();

        let status = response.status();
        if !status.is_success() {
            let retry_after = parse_retry_after(response.headers());
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status(id, status, retry_after, &body_text));
        }

        // Reading the body first keeps a snippet for the error message; providers
        // change response shapes without notice, and a bare "decoding failed" is
        // impossible to act on.
        let body_text = response
            .text()
            .await
            .map_err(|error| transport_error(id, error, self.timeout))?;
        let payload: ChatCompletionResponse =
            serde_json::from_str(&body_text).map_err(|error| LlmError::Parse {
                provider: id,
                detail: format!(
                    "could not decode the completion: {error}; body: {}",
                    truncate(&body_text, 300)
                ),
            })?;

        let choice = payload.choices.into_iter().next().ok_or_else(|| LlmError::Parse {
            provider: id,
            detail: "the provider returned no choices".to_string(),
        })?;
        let finished_by_length = choice.finish_reason.as_deref() == Some("length");
        let had_reasoning = choice
            .message
            .reasoning_content
            .as_deref()
            .map(|reasoning| !reasoning.trim().is_empty())
            .unwrap_or(false);

        let text = message_text(&choice.message);
        let text = text.trim();
        if text.is_empty() {
            // Reasoning models can burn the whole budget before writing an
            // answer; say so explicitly instead of reporting a generic failure.
            let hint = if finished_by_length {
                "the token budget was exhausted before any text was produced; raise CC_DEFAULT_MAX_TOKENS"
            } else if had_reasoning {
                "only reasoning content was returned, no prayer text"
            } else {
                "the provider returned an empty completion"
            };
            return Err(LlmError::Parse {
                provider: id,
                detail: format!(
                    "{hint} (finish reason: {})",
                    choice.finish_reason.as_deref().unwrap_or("unknown")
                ),
            });
        }

        Ok(ChatResponse {
            provider: id,
            model: payload
                .model
                .unwrap_or_else(|| self.config.model.clone()),
            text: text.to_string(),
            usage: payload.usage.map(Usage::from),
            latency,
        })
    }
}
