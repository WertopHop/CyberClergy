//! Environment-driven configuration for every supported provider.
//!
//! Providers are described by one static table ([`PROVIDER_SPECS`]). Adding
//! another OpenAI-compatible backend therefore means: a `ProviderId` variant
//! plus a single spec row. Protocol differences live in the adapters under
//! `providers/`, never in this file.
//!
//! Design goals: every provider is optional, secrets never leak through
//! `Debug` output, and malformed numeric settings degrade to defaults with a
//! warning instead of blocking application startup.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::Serialize;

use crate::redact::redact_secret;
use crate::types::ProviderId;

pub const VAR_OPENROUTER_API_KEY: &str = "OPENROUTER_API_KEY";
pub const VAR_OPENROUTER_BASE_URL: &str = "OPENROUTER_BASE_URL";
pub const VAR_OPENROUTER_MODEL: &str = "OPENROUTER_MODEL";
pub const VAR_OPENROUTER_APP_URL: &str = "OPENROUTER_APP_URL";
pub const VAR_OPENROUTER_APP_TITLE: &str = "OPENROUTER_APP_TITLE";

pub const VAR_GEMINI_API_KEY: &str = "GEMINI_API_KEY";
pub const VAR_GEMINI_BASE_URL: &str = "GEMINI_BASE_URL";
pub const VAR_GEMINI_MODEL: &str = "GEMINI_MODEL";

pub const VAR_GROQ_API_KEY: &str = "GROQ_API_KEY";
pub const VAR_GROQ_BASE_URL: &str = "GROQ_BASE_URL";
pub const VAR_GROQ_MODEL: &str = "GROQ_MODEL";

pub const VAR_MISTRAL_API_KEY: &str = "MISTRAL_API_KEY";
pub const VAR_MISTRAL_BASE_URL: &str = "MISTRAL_BASE_URL";
pub const VAR_MISTRAL_MODEL: &str = "MISTRAL_MODEL";

pub const VAR_CLOUDFLARE_API_TOKEN: &str = "CLOUDFLARE_API_TOKEN";
pub const VAR_CLOUDFLARE_ACCOUNT_ID: &str = "CLOUDFLARE_ACCOUNT_ID";
pub const VAR_CLOUDFLARE_BASE_URL: &str = "CLOUDFLARE_BASE_URL";
pub const VAR_CLOUDFLARE_MODEL: &str = "CLOUDFLARE_MODEL";

pub const VAR_NVIDIA_API_KEY: &str = "NVIDIA_API_KEY";
pub const VAR_NVIDIA_BASE_URL: &str = "NVIDIA_BASE_URL";
pub const VAR_NVIDIA_MODEL: &str = "NVIDIA_MODEL";

pub const VAR_TOGETHER_API_KEY: &str = "TOGETHER_API_KEY";
pub const VAR_TOGETHER_BASE_URL: &str = "TOGETHER_BASE_URL";
pub const VAR_TOGETHER_MODEL: &str = "TOGETHER_MODEL";

pub const VAR_CEREBRAS_API_KEY: &str = "CEREBRAS_API_KEY";
pub const VAR_CEREBRAS_BASE_URL: &str = "CEREBRAS_BASE_URL";
pub const VAR_CEREBRAS_MODEL: &str = "CEREBRAS_MODEL";

pub const VAR_DEEPINFRA_API_KEY: &str = "DEEPINFRA_API_KEY";
pub const VAR_DEEPINFRA_BASE_URL: &str = "DEEPINFRA_BASE_URL";
pub const VAR_DEEPINFRA_MODEL: &str = "DEEPINFRA_MODEL";

pub const VAR_GITHUB_MODELS_TOKEN: &str = "GITHUB_MODELS_TOKEN";
pub const VAR_GITHUB_MODELS_BASE_URL: &str = "GITHUB_MODELS_BASE_URL";
pub const VAR_GITHUB_MODELS_MODEL: &str = "GITHUB_MODELS_MODEL";

pub const VAR_OLLAMA_API_KEY: &str = "OLLAMA_API_KEY";
pub const VAR_OLLAMA_BASE_URL: &str = "OLLAMA_BASE_URL";
pub const VAR_OLLAMA_MODEL: &str = "OLLAMA_MODEL";

pub const VAR_CUSTOM_API_KEY: &str = "CUSTOM_API_KEY";
pub const VAR_CUSTOM_BASE_URL: &str = "CUSTOM_BASE_URL";
pub const VAR_CUSTOM_MODEL: &str = "CUSTOM_MODEL";

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
pub const DEFAULT_MAX_RETRIES: u32 = 2;
pub const DEFAULT_TEMPERATURE: f32 = 0.8;
// Reasoning models spend part of the budget on hidden reasoning before writing
// a single word, so the default leaves room for both.
pub const DEFAULT_MAX_TOKENS: u32 = 2048;

/// Wire protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// OpenAI chat-completions schema: OpenRouter, Groq, Mistral AI, NVIDIA NIM.
    OpenAiChat,
    /// Google `POST /models/{model}:generateContent`.
    Gemini,
    /// Cloudflare `POST /accounts/{account}/ai/run/{model}`.
    CloudflareWorkersAi,
}

/// Static description of a provider: everything that does not depend on the
/// environment. Environment variable names are declared here so status
/// reporting and error messages can never drift from the loader.
#[derive(Debug, Clone, Copy)]
pub struct ProviderSpec {
    pub id: ProviderId,
    pub protocol: Protocol,
    pub api_key_var: &'static str,
    pub base_url_var: &'static str,
    pub default_base_url: &'static str,
    pub model_var: &'static str,
    pub default_model: &'static str,
    /// Variables required in addition to the API key (for example an account id).
    pub required_vars: &'static [&'static str],
    /// Optional attribution headers as `(url_var, title_var)`.
    pub attribution_vars: Option<(&'static str, &'static str)>,
    /// True for a backend that needs no credential at all, such as a model
    /// running on this machine. The key variable stays declared, so a
    /// password-protected local gateway can still be reached.
    pub key_optional: bool,
}

pub const PROVIDER_SPECS: &[ProviderSpec] = &[
    ProviderSpec {
        id: ProviderId::OpenRouter,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_OPENROUTER_API_KEY,
        base_url_var: VAR_OPENROUTER_BASE_URL,
        default_base_url: "https://openrouter.ai/api/v1",
        model_var: VAR_OPENROUTER_MODEL,
        default_model: "meta-llama/llama-3.3-70b-instruct:free",
        required_vars: &[],
        attribution_vars: Some((VAR_OPENROUTER_APP_URL, VAR_OPENROUTER_APP_TITLE)),
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Gemini,
        protocol: Protocol::Gemini,
        api_key_var: VAR_GEMINI_API_KEY,
        base_url_var: VAR_GEMINI_BASE_URL,
        default_base_url: "https://generativelanguage.googleapis.com/v1beta",
        model_var: VAR_GEMINI_MODEL,
        default_model: "gemini-2.5-flash",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Groq,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_GROQ_API_KEY,
        base_url_var: VAR_GROQ_BASE_URL,
        default_base_url: "https://api.groq.com/openai/v1",
        model_var: VAR_GROQ_MODEL,
        default_model: "llama-3.3-70b-versatile",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Mistral,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_MISTRAL_API_KEY,
        base_url_var: VAR_MISTRAL_BASE_URL,
        default_base_url: "https://api.mistral.ai/v1",
        model_var: VAR_MISTRAL_MODEL,
        default_model: "mistral-small-latest",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Cloudflare,
        protocol: Protocol::CloudflareWorkersAi,
        api_key_var: VAR_CLOUDFLARE_API_TOKEN,
        base_url_var: VAR_CLOUDFLARE_BASE_URL,
        default_base_url: "https://api.cloudflare.com/client/v4",
        model_var: VAR_CLOUDFLARE_MODEL,
        default_model: "@cf/meta/llama-3.1-8b-instruct",
        required_vars: &[VAR_CLOUDFLARE_ACCOUNT_ID],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Nvidia,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_NVIDIA_API_KEY,
        base_url_var: VAR_NVIDIA_BASE_URL,
        default_base_url: "https://integrate.api.nvidia.com/v1",
        model_var: VAR_NVIDIA_MODEL,
        default_model: "meta/llama-3.3-70b-instruct",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Together,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_TOGETHER_API_KEY,
        base_url_var: VAR_TOGETHER_BASE_URL,
        default_base_url: "https://api.together.xyz/v1",
        model_var: VAR_TOGETHER_MODEL,
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo-Free",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::Cerebras,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_CEREBRAS_API_KEY,
        base_url_var: VAR_CEREBRAS_BASE_URL,
        default_base_url: "https://api.cerebras.ai/v1",
        model_var: VAR_CEREBRAS_MODEL,
        default_model: "llama-3.3-70b",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::DeepInfra,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_DEEPINFRA_API_KEY,
        base_url_var: VAR_DEEPINFRA_BASE_URL,
        default_base_url: "https://api.deepinfra.com/v1/openai",
        model_var: VAR_DEEPINFRA_MODEL,
        default_model: "meta-llama/Meta-Llama-3.1-8B-Instruct",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    ProviderSpec {
        id: ProviderId::GithubModels,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_GITHUB_MODELS_TOKEN,
        base_url_var: VAR_GITHUB_MODELS_BASE_URL,
        default_base_url: "https://models.github.ai/inference",
        model_var: VAR_GITHUB_MODELS_MODEL,
        default_model: "openai/gpt-4o-mini",
        required_vars: &[],
        attribution_vars: None,
        key_optional: false,
    },
    // A local runtime is enabled by naming a model, not by a key: a fresh
    // install must not put a robot on the cloud that cannot answer.
    ProviderSpec {
        id: ProviderId::Ollama,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_OLLAMA_API_KEY,
        base_url_var: VAR_OLLAMA_BASE_URL,
        default_base_url: "http://localhost:11434/v1",
        model_var: VAR_OLLAMA_MODEL,
        default_model: "",
        required_vars: &[VAR_OLLAMA_MODEL],
        attribution_vars: None,
        key_optional: true,
    },
    // The universal escape hatch: any OpenAI-compatible gateway, reached by
    // pointing the app at its base URL and naming a model.
    ProviderSpec {
        id: ProviderId::Custom,
        protocol: Protocol::OpenAiChat,
        api_key_var: VAR_CUSTOM_API_KEY,
        base_url_var: VAR_CUSTOM_BASE_URL,
        default_base_url: "",
        model_var: VAR_CUSTOM_MODEL,
        default_model: "",
        required_vars: &[VAR_CUSTOM_BASE_URL, VAR_CUSTOM_MODEL],
        attribution_vars: None,
        key_optional: true,
    },
];

impl ProviderSpec {
    /// Looks up the spec of a provider. The table is exhaustive by contract,
    /// which is enforced by a unit test.
    pub fn from_id(id: ProviderId) -> &'static ProviderSpec {
        PROVIDER_SPECS
            .iter()
            .find(|spec| spec.id == id)
            .expect("every ProviderId must have a ProviderSpec entry")
    }
}

/// A provider resolved against the environment.
#[derive(Clone)]
pub struct ProviderConfig {
    pub spec: &'static ProviderSpec,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Values of `spec.required_vars`, keyed by variable name.
    pub required: BTreeMap<&'static str, String>,
    pub headers: Vec<(String, String)>,
}

impl ProviderConfig {
    pub fn id(&self) -> ProviderId {
        self.spec.id
    }

    pub fn protocol(&self) -> Protocol {
        self.spec.protocol
    }

    /// Environment variables the user still has to provide, if any.
    ///
    /// A provider with `key_optional` (a local runtime, a keyless gateway) is
    /// missing nothing just because no key was given; its required variables
    /// are what opt it in.
    pub fn missing_vars(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.api_key.is_none() && !self.spec.key_optional {
            missing.push(self.spec.api_key_var);
        }
        for variable in self.spec.required_vars {
            if !self.required.contains_key(variable) {
                missing.push(*variable);
            }
        }
        missing
    }

    pub fn is_configured(&self) -> bool {
        self.missing_vars().is_empty()
    }

    pub fn required_value(&self, variable: &str) -> Option<&str> {
        self.required.get(variable).map(String::as_str)
    }
}

// Manual implementation: a derived Debug would print the API key verbatim in
// panics, logs and bug reports.
impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("id", &self.spec.id)
            .field("protocol", &self.spec.protocol)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_deref().map(redact_secret))
            .field(
                "required",
                &self
                    .required
                    .iter()
                    .map(|(name, value)| (*name, redact_secret(value)))
                    .collect::<BTreeMap<_, _>>(),
            )
            .field(
                "headers",
                &self
                    .headers
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
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
    providers: BTreeMap<ProviderId, ProviderConfig>,
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
        let providers = PROVIDER_SPECS
            .iter()
            .map(|spec| {
                let config = ProviderConfig {
                    spec,
                    base_url: string_var(vars, spec.base_url_var)
                        .unwrap_or_else(|| spec.default_base_url.to_string()),
                    model: string_var(vars, spec.model_var)
                        .unwrap_or_else(|| spec.default_model.to_string()),
                    api_key: string_var(vars, spec.api_key_var),
                    required: spec
                        .required_vars
                        .iter()
                        .filter_map(|variable| {
                            string_var(vars, variable).map(|value| (*variable, value))
                        })
                        .collect(),
                    headers: attribution_headers(vars, spec),
                };
                (spec.id, config)
            })
            .collect();

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

        Self { providers, shared }
    }

    pub fn provider(&self, id: ProviderId) -> Option<&ProviderConfig> {
        self.providers.get(&id)
    }

    /// All providers in the canonical `ProviderId::ALL` order.
    pub fn providers(&self) -> impl Iterator<Item = &ProviderConfig> {
        self.providers.values()
    }

    /// Configuration state of every known provider.
    pub fn statuses(&self) -> Vec<ProviderStatus> {
        PROVIDER_SPECS
            .iter()
            .filter_map(|spec| self.providers.get(&spec.id))
            .map(|config| ProviderStatus {
                id: config.id(),
                configured: config.is_configured(),
                model: config.model.clone(),
                missing_vars: config.missing_vars(),
            })
            .collect()
    }
}

fn attribution_headers(
    vars: &BTreeMap<String, String>,
    spec: &ProviderSpec,
) -> Vec<(String, String)> {
    let Some((url_var, title_var)) = spec.attribution_vars else {
        return Vec::new();
    };
    let mut headers = Vec::new();
    if let Some(url) = string_var(vars, url_var) {
        headers.push(("HTTP-Referer".to_string(), url));
    }
    if let Some(title) = string_var(vars, title_var) {
        headers.push(("X-Title".to_string(), title));
    }
    headers
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

#[cfg(test)]
mod tests {
    use super::PROVIDER_SPECS;
    use crate::types::ProviderId;

    #[test]
    fn every_provider_has_exactly_one_spec() {
        assert_eq!(PROVIDER_SPECS.len(), ProviderId::ALL.len());
        for id in ProviderId::ALL {
            let matches = PROVIDER_SPECS
                .iter()
                .filter(|spec| spec.id == id)
                .count();
            assert_eq!(matches, 1, "provider {id} must have one spec entry");
        }
    }
}
