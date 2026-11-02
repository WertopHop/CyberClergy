//! Adapter for Google AI Studio (Gemini API).
//!
//! Gemini differs from the OpenAI schema in three ways that matter here: system
//! instructions live in their own field, assistant turns are called `model`,
//! and the key is passed through the `x-goog-api-key` header (never as a query
//! parameter, so URLs stay safe to log).

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::Endpoint;
use crate::errors::LlmError;
use crate::provider::Provider;
use crate::providers::{map_status, parse_retry_after, transport_error};
use crate::types::{ChatRequest, ChatResponse, ProviderId, Role, Usage};

const API_KEY_HEADER: &str = "x-goog-api-key";

pub struct GeminiProvider {
    endpoint: Endpoint,
    client: reqwest::Client,
    timeout: Duration,
}

impl GeminiProvider {
    pub fn new(endpoint: Endpoint, client: reqwest::Client, timeout: Duration) -> Self {
        Self {
            endpoint,
            client,
            timeout,
        }
    }
}

#[derive(Serialize)]
struct GenerateContentBody {
    contents: Vec<Content>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize)]
struct Content {
    role: &'static str,
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<PromptFeedback>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
    #[serde(rename = "modelVersion")]
    model_version: Option<String>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct CandidateContent {
    #[serde(default)]
    parts: Vec<ResponsePart>,
}

#[derive(Deserialize)]
struct ResponsePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct PromptFeedback {
    #[serde(rename = "blockReason")]
    block_reason: Option<String>,
}

#[derive(Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

#[async_trait]
impl Provider for GeminiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn model(&self) -> &str {
        &self.endpoint.model
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let api_key = self
            .endpoint
            .api_key
            .clone()
            .ok_or(LlmError::NotConfigured(ProviderId::Gemini))?;

        let contents: Vec<Content> = request
            .messages
            .iter()
            .filter(|message| message.role != Role::System)
            .map(|message| Content {
                role: match message.role {
                    Role::Assistant => "model",
                    _ => "user",
                },
                parts: vec![Part {
                    text: message.content.clone(),
                }],
            })
            .collect();
        if contents.is_empty() {
            return Err(LlmError::InvalidRequest {
                provider: ProviderId::Gemini,
                detail: "the request contains no user or assistant messages".to_string(),
            });
        }

        let generation_config = if request.temperature.is_some() || request.max_tokens.is_some() {
            Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            })
        } else {
            None
        };

        let body = GenerateContentBody {
            contents,
            system_instruction: request
                .system
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .map(|text| SystemInstruction {
                    parts: vec![Part {
                        text: text.to_string(),
                    }],
                }),
            generation_config,
        };

        let model = request
            .model
            .clone()
            .unwrap_or_else(|| self.endpoint.model.clone());
        let url = format!(
            "{}/models/{model}:generateContent",
            self.endpoint.base_url.trim_end_matches('/')
        );

        let started = Instant::now();
        let response = self
            .client
            .post(&url)
            .header(API_KEY_HEADER, api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| transport_error(ProviderId::Gemini, error, self.timeout))?;
        let latency = started.elapsed();

        let status = response.status();
        if !status.is_success() {
            let retry_after = parse_retry_after(response.headers());
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status(ProviderId::Gemini, status, retry_after, &body_text));
        }

        let payload: GenerateContentResponse =
            response.json().await.map_err(|error| LlmError::Parse {
                provider: ProviderId::Gemini,
                detail: format!("could not decode the completion: {error}"),
            })?;

        if let Some(reason) = payload
            .prompt_feedback
            .and_then(|feedback| feedback.block_reason)
        {
            return Err(LlmError::BadRequest {
                provider: ProviderId::Gemini,
                status: status.as_u16(),
                detail: format!("prompt blocked by safety filters ({reason})"),
            });
        }

        let candidate = payload.candidates.into_iter().next().ok_or_else(|| {
            LlmError::Parse {
                provider: ProviderId::Gemini,
                detail: "the response contains no candidates".to_string(),
            }
        })?;

        let text = candidate
            .content
            .map(|content| {
                content
                    .parts
                    .into_iter()
                    .filter_map(|part| part.text)
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        let text = text.trim();
        if text.is_empty() {
            return Err(LlmError::BadRequest {
                provider: ProviderId::Gemini,
                status: status.as_u16(),
                detail: format!(
                    "the provider returned no text (finish reason: {})",
                    candidate
                        .finish_reason
                        .unwrap_or_else(|| "unknown".to_string())
                ),
            });
        }

        Ok(ChatResponse {
            provider: ProviderId::Gemini,
            model: payload
                .model_version
                .unwrap_or_else(|| self.endpoint.model.clone()),
            text: text.to_string(),
            usage: payload.usage_metadata.map(|usage| Usage {
                prompt_tokens: usage.prompt_token_count,
                completion_tokens: usage.candidates_token_count,
                total_tokens: usage.total_token_count,
            }),
            latency,
        })
    }
}
