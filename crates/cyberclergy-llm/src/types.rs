//! Provider-agnostic request, response and identifier types.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Stable identifier of a supported inference backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    OpenRouter,
    Gemini,
    Groq,
    Mistral,
    Cloudflare,
    Nvidia,
}

impl ProviderId {
    /// Every provider known to the crate, in the default rotation order.
    pub const ALL: [ProviderId; 6] = [
        ProviderId::OpenRouter,
        ProviderId::Gemini,
        ProviderId::Groq,
        ProviderId::Mistral,
        ProviderId::Cloudflare,
        ProviderId::Nvidia,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::OpenRouter => "openrouter",
            ProviderId::Gemini => "gemini",
            ProviderId::Groq => "groq",
            ProviderId::Mistral => "mistral",
            ProviderId::Cloudflare => "cloudflare",
            ProviderId::Nvidia => "nvidia",
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a provider identifier cannot be recognised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseProviderIdError(pub String);

impl fmt::Display for ParseProviderIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let known = ProviderId::ALL
            .iter()
            .map(|provider| provider.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "unknown provider id `{}` (expected one of: {known})", self.0)
    }
}

impl std::error::Error for ParseProviderIdError {}

impl FromStr for ProviderId {
    type Err = ParseProviderIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "openrouter" | "open-router" => Ok(ProviderId::OpenRouter),
            "gemini" | "google" | "google-ai-studio" | "googleai" => Ok(ProviderId::Gemini),
            "groq" => Ok(ProviderId::Groq),
            "mistral" | "mistralai" | "mistral-ai" => Ok(ProviderId::Mistral),
            "cloudflare" | "cloudflare-workers-ai" | "workers-ai" => Ok(ProviderId::Cloudflare),
            "nvidia" | "nim" | "nvidia-nim" => Ok(ProviderId::Nvidia),
            _ => Err(ParseProviderIdError(value.to_string())),
        }
    }
}

/// Role of a single conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One message of a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// A provider-agnostic completion request.
///
/// `system` is kept separate from the conversation because provider APIs model
/// system instructions differently (a message on OpenAI-compatible endpoints,
/// a dedicated field on Gemini).
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl ChatRequest {
    /// A single user turn without a system prompt.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            system: None,
            messages: vec![ChatMessage::user(text)],
            model: None,
            temperature: None,
            max_tokens: None,
        }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_message(mut self, message: ChatMessage) -> Self {
        self.messages.push(message);
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// Token accounting reported by a provider, when available.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// A normalised completion returned by a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub provider: ProviderId,
    pub model: String,
    pub text: String,
    pub usage: Option<Usage>,
    pub latency: Duration,
}
