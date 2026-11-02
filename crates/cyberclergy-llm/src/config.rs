//! Environment-driven configuration for every supported provider.
//!
//! Design goals: every provider is optional, secrets never leak through `Debug`
//! output, and malformed numeric settings degrade to defaults with a warning
//! instead of blocking application startup.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::Serialize;

use crate::redact::redact_secret;
use crate::types::ProviderId;

// Base URLs can be overridden through `*_BASE_URL` variables, which keeps the
// tests hermetic and lets users fall back to a proxy without changing code.
pub const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const DEFAULT_GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";
pub const DEFAULT_MISTRAL_BASE_URL: &str = "https://api.mistral.ai/v1";
pub const DEFAULT_CLOUDFLARE_BASE_URL: &str = "https://api.cloudflare.com/client/v4";
pub const DEFAULT_NVIDIA_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";

pub const DEFAULT_OPENROUTER_MODEL: &str = "meta-llama/llama-3.3-70b-instruct:free";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
pub const DEFAULT_GROQ_MODEL: &str = "llama-3.3-70b-versatile";
pub const DEFAULT_MISTRAL_MODEL: &str = "mistral-small-latest";
pub const DEFAULT_CLOUDFLARE_MODEL: &str = "@cf/meta/llama-3.1-8b-instruct";
pub const DEFAULT_NVIDIA_MODEL: &str = "meta/llama-3.3-70b-instruct";

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
pub const DEFAULT_MAX_RETRIES: u32 = 2;
pub const DEFAULT_TEMPERATURE: f32 = 0.8;
pub const DEFAULT_MAX_TOKENS: u32 = 512;

const ENV_OPENROUTER_API_KEY: &str = "OPENROUTER_API_KEY";
const ENV_GEMINI_API_KEY: &str = "GEMINI_API_KEY";
const ENV_GROQ_API_KEY: &str = "GROQ_API_KEY";
const ENV_MISTRAL_API_KEY: &str = "MISTRAL_API_KEY";
const ENV_NVIDIA_API_KEY: &str = "NVIDIA_API_KEY";
const ENV_CLOUDFLARE_ACCOUNT_ID: &str = "CLOUDFLARE_ACCOUNT_ID";
const ENV_CLOUDFLARE_API_TOKEN: &str = "CLOUDFLARE_API_TOKEN";

/// A single endpoint that speaks a chat-completion style protocol.
#[derive(Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub api_key_var: &'static str,
    pub extra_headers: Vec<(String, String)>,
}

impl Endpoint {
    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Environment variables the user still has to provide, if any.
    pub fn missing_vars(&self) -> Vec<&'static str> {
        if self.is_configured() {
            Vec::new()
        } else {
            vec![self.api_key_var]
        }
    }
}

// Manual implementation: a derived Debug would print the API key verbatim in
// panics, logs and bug reports.
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_deref().map(redact_secret))
            .field(
                "extra_headers",
                &self
                    .extra_headers
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Cloudflare Workers AI needs an account id in addition to the token.
#[derive(Clone)]
pub struct CloudflareConfig {
    pub base_url: String,
    pub model: String,
    pub account_id: Option<String>,
    pub api_token: Option<String>,
}

impl CloudflareConfig {
    pub fn is_configured(&self) -> bool {
        self.missing_vars().is_empty()
    }

    pub fn missing_vars(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.account_id.is_none() {
            missing.push(ENV_CLOUDFLARE_ACCOUNT_ID);
        }
        if self.api_token.is_none() {
            missing.push(ENV_CLOUDFLARE_API_TOKEN);
        }
        missing
    }
}

impl fmt::Debug for CloudflareConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudflareConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("account_id", &self.account_id.as_deref().map(redact_secret))
            .field("api_token", &self.api_token.as_deref().map(redact_secret))
            .finish()
    }
}

/// Settings shared by all providers.
#[derive(Debug, Clone)]
pub struct SharedSettings {
    pub timeout: Duration,
    pub max_retries: u32,
    pub temperature: f32,
    pub max_tokens: u32,
    pub provider_order: Vec<ProviderId>,
}

/// Read-only snapshot of the whole configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    pub openrouter: Endpoint,
    pub gemini: Endpoint,
    pub groq: Endpoint,
    pub mistral: Endpoint,
    pub nvidia: Endpoint,
    pub cloudflare: CloudflareConfig,
    pub shared: SharedSettings,
}

/// Per-provider configuration state, suitable for showing in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub id: ProviderId,
    pub configured: bool,
    pub model: String,
    pub missing_vars: Vec<&'static str>,
}

impl Settings {
    /// Reads the process environment.
    pub fn from_env() -> Self {
        let vars: BTreeMap<String, String> = env::vars().collect();
        Self::from_vars(&vars)
    }

    /// Builds settings from an explicit map; never fails.
    pub fn from_vars(vars: &BTreeMap<String, String>) -> Self {
        let openrouter = Endpoint {
            base_url: string_var(vars, "OPENROUTER_BASE_URL")
                .unwrap_or_else(|| DEFAULT_OPENROUTER_BASE_URL.to_string()),
            model: string_var(vars, "OPENROUTER_MODEL")
                .unwrap_or_else(|| DEFAULT_OPENROUTER_MODEL.to_string()),
            api_key: string_var(vars, ENV_OPENROUTER_API_KEY),
            api_key_var: ENV_OPENROUTER_API_KEY,
            extra_headers: openrouter_headers(vars),
        };
        let gemini = Endpoint {
            base_url: string_var(vars, "GEMINI_BASE_URL")
                .unwrap_or_else(|| DEFAULT_GEMINI_BASE_URL.to_string()),
            model: string_var(vars, "GEMINI_MODEL")
                .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string()),
            api_key: string_var(vars, ENV_GEMINI_API_KEY),
            api_key_var: ENV_GEMINI_API_KEY,
            extra_headers: Vec::new(),
        };
        let groq = Endpoint {
            base_url: string_var(vars, "GROQ_BASE_URL")
                .unwrap_or_else(|| DEFAULT_GROQ_BASE_URL.to_string()),
            model: string_var(vars, "GROQ_MODEL").unwrap_or_else(|| DEFAULT_GROQ_MODEL.to_string()),
            api_key: string_var(vars, ENV_GROQ_API_KEY),
            api_key_var: ENV_GROQ_API_KEY,
            extra_headers: Vec::new(),
        };
        let mistral = Endpoint {
            base_url: string_var(vars, "MISTRAL_BASE_URL")
                .unwrap_or_else(|| DEFAULT_MISTRAL_BASE_URL.to_string()),
            model: string_var(vars, "MISTRAL_MODEL")
                .unwrap_or_else(|| DEFAULT_MISTRAL_MODEL.to_string()),
            api_key: string_var(vars, ENV_MISTRAL_API_KEY),
            api_key_var: ENV_MISTRAL_API_KEY,
            extra_headers: Vec::new(),
        };
        let nvidia = Endpoint {
            base_url: string_var(vars, "NVIDIA_BASE_URL")
                .unwrap_or_else(|| DEFAULT_NVIDIA_BASE_URL.to_string()),
            model: string_var(vars, "NVIDIA_MODEL")
                .unwrap_or_else(|| DEFAULT_NVIDIA_MODEL.to_string()),
            api_key: string_var(vars, ENV_NVIDIA_API_KEY),
            api_key_var: ENV_NVIDIA_API_KEY,
            extra_headers: Vec::new(),
        };
        let cloudflare = CloudflareConfig {
            base_url: string_var(vars, "CLOUDFLARE_BASE_URL")
                .unwrap_or_else(|| DEFAULT_CLOUDFLARE_BASE_URL.to_string()),
            model: string_var(vars, "CLOUDFLARE_MODEL")
                .unwrap_or_else(|| DEFAULT_CLOUDFLARE_MODEL.to_string()),
            account_id: string_var(vars, ENV_CLOUDFLARE_ACCOUNT_ID),
            api_token: string_var(vars, ENV_CLOUDFLARE_API_TOKEN),
        };
        let shared = SharedSettings {
            timeout: Duration::from_secs(number_var(
                vars,
                "CC_HTTP_TIMEOUT_SECS",
                DEFAULT_TIMEOUT.as_secs(),
                5,
                600,
            )),
            max_retries: number_var(vars, "CC_MAX_RETRIES", DEFAULT_MAX_RETRIES, 0, 10),
            temperature: number_var(
                vars,
                "CC_DEFAULT_TEMPERATURE",
                DEFAULT_TEMPERATURE,
                0.0,
                2.0,
            ),
            max_tokens: number_var(vars, "CC_DEFAULT_MAX_TOKENS", DEFAULT_MAX_TOKENS, 16, 32_768),
            provider_order: provider_order_var(vars),
        };

        Self {
            openrouter,
            gemini,
            groq,
            mistral,
            nvidia,
            cloudflare,
            shared,
        }
    }

    /// Chat-completion style endpoint for a provider, if the provider uses one.
    pub fn endpoint(&self, id: ProviderId) -> Option<&Endpoint> {
        match id {
            ProviderId::OpenRouter => Some(&self.openrouter),
            ProviderId::Gemini => Some(&self.gemini),
            ProviderId::Groq => Some(&self.groq),
            ProviderId::Mistral => Some(&self.mistral),
            ProviderId::Nvidia => Some(&self.nvidia),
            ProviderId::Cloudflare => None,
        }
    }

    /// Configuration state of every known provider, always in `ProviderId::ALL` order.
    pub fn statuses(&self) -> Vec<ProviderStatus> {
        ProviderId::ALL
            .iter()
            .map(|id| match id {
                ProviderId::Cloudflare => ProviderStatus {
                    id: *id,
                    configured: self.cloudflare.is_configured(),
                    model: self.cloudflare.model.clone(),
                    missing_vars: self.cloudflare.missing_vars(),
                },
                other => {
                    let endpoint = self.endpoint(*other).expect(
                        "every provider except Cloudflare is backed by a chat-completion endpoint",
                    );
                    ProviderStatus {
                        id: *other,
                        configured: endpoint.is_configured(),
                        model: endpoint.model.clone(),
                        missing_vars: endpoint.missing_vars(),
                    }
                }
            })
            .collect()
    }
}

/// Reads a variable, trimming surrounding whitespace and treating blank values
/// as absent. This mirrors how users edit `.env` files by hand.
fn string_var(vars: &BTreeMap<String, String>, key: &str) -> Option<String> {
    vars.get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn number_var<T>(vars: &BTreeMap<String, String>, key: &str, fallback: T, min: T, max: T) -> T
where
    T: FromStr + PartialOrd + Copy + fmt::Display,
{
    match string_var(vars, key) {
        None => fallback,
        Some(raw) => match raw.parse::<T>() {
            Ok(value) if value >= min && value <= max => value,
            Ok(_) => {
                tracing::warn!(
                    variable = key,
                    value = %raw,
                    "value is out of range, falling back to the default"
                );
                fallback
            }
            Err(_) => {
                tracing::warn!(
                    variable = key,
                    value = %raw,
                    "value is not a valid number, falling back to the default"
                );
                fallback
            }
        },
    }
}

fn provider_order_var(vars: &BTreeMap<String, String>) -> Vec<ProviderId> {
    let Some(raw) = string_var(vars, "CC_PROVIDER_ORDER") else {
        return ProviderId::ALL.to_vec();
    };
    let parsed: Vec<ProviderId> = raw
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            match ProviderId::from_str(part) {
                Ok(id) => Some(id),
                Err(error) => {
                    tracing::warn!(value = part, error = %error, "ignoring unknown provider id");
                    None
                }
            }
        })
        .collect();
    if parsed.is_empty() {
        ProviderId::ALL.to_vec()
    } else {
        parsed
    }
}

fn openrouter_headers(vars: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(url) = string_var(vars, "OPENROUTER_APP_URL") {
        headers.push(("HTTP-Referer".to_string(), url));
    }
    if let Some(title) = string_var(vars, "OPENROUTER_APP_TITLE") {
        headers.push(("X-Title".to_string(), title));
    }
    headers
}
