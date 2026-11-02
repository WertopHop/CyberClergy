//! Adapter for endpoints that implement the OpenAI chat-completions schema:
//! OpenRouter, Groq, Mistral AI and NVIDIA NIM.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::Endpoint;
use crate::errors::LlmError;
use crate::provider::Provider;
use crate::providers::{map_status, parse_retry_after, transport_error};
use crate::types::{ChatRequest, ChatResponse, ProviderId, Usage};

pub struct OpenAiCompatProvider {
    id: ProviderId,
    endpoint: Endpoint,
    client: reqwest::Client,
    timeout: Duration,
}

impl OpenAiCompatProvider {
    pub fn new(
        id: ProviderId,
        endpoint: Endpoint,
        client: reqwest::Client,
        timeout: Duration,
    ) -> Self {
        Self {
            id,
            endpoint,
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
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: Option<String>,
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
        self.id
    }

    fn model(&self) -> &str {
        &self.endpoint.model
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let api_key = self
            .endpoint
            .api_key
            .clone()
            .ok_or(LlmError::NotConfigured(self.id))?;

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
                provider: self.id,
                detail: "the request contains no messages".to_string(),
            });
        }

        let body = ChatCompletionBody {
            model: request
                .model
                .clone()
                .unwrap_or_else(|| self.endpoint.model.clone()),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let url = format!(
            "{}/chat/completions",
            self.endpoint.base_url.trim_end_matches('/')
        );
        let mut builder = self.client.post(&url).bearer_auth(api_key).json(&body);
        for (name, value) in &self.endpoint.extra_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let started = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|error| transport_error(self.id, error, self.timeout))?;
        let latency = started.elapsed();

        let status = response.status();
        if !status.is_success() {
            let retry_after = parse_retry_after(response.headers());
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status(self.id, status, retry_after, &body_text));
        }

        let payload: ChatCompletionResponse =
            response.json().await.map_err(|error| LlmError::Parse {
                provider: self.id,
                detail: format!("could not decode the completion: {error}"),
            })?;

        let text = payload
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .unwrap_or_default();
        let text = text.trim();
        if text.is_empty() {
            return Err(LlmError::Parse {
                provider: self.id,
                detail: "the provider returned an empty completion".to_string(),
            });
        }

        Ok(ChatResponse {
            provider: self.id,
            model: payload
                .model
                .unwrap_or_else(|| self.endpoint.model.clone()),
            text: text.to_string(),
            usage: payload.usage.map(Usage::from),
            latency,
        })
    }
}
