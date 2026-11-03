//! Typed failures shared by every provider adapter.
//!
//! Errors are deliberately structured rather than stringly typed: the fallback
//! scheduler needs to distinguish "wrong key, never retry" from "rate limited,
//! try again later" without parsing messages.

use std::fmt;
use std::time::Duration;

use crate::types::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// The provider has no credentials, therefore it is not registered.
    NotConfigured(ProviderId),
    /// The registry has no configured provider at all.
    NoProvidersConfigured,
    /// The request itself is malformed (no messages, unknown model, ...).
    InvalidRequest {
        provider: ProviderId,
        detail: String,
    },
    /// The provider is configured but its settings are inconsistent.
    InvalidConfig {
        provider: ProviderId,
        detail: String,
    },
    /// The prayer prompt could not be rendered (bad placeholder, empty file).
    PromptInvalid { detail: String },
    /// The provider rejected the credentials.
    Auth {
        provider: ProviderId,
        status: u16,
        detail: String,
    },
    /// Too many requests; `retry_after` mirrors the provider hint when present.
    RateLimited {
        provider: ProviderId,
        retry_after: Option<Duration>,
        detail: String,
    },
    /// The free quota or credit balance is exhausted.
    QuotaExceeded {
        provider: ProviderId,
        detail: String,
    },
    /// The payload was rejected by the provider.
    BadRequest {
        provider: ProviderId,
        status: u16,
        detail: String,
    },
    /// The provider failed on its side.
    Server {
        provider: ProviderId,
        status: u16,
        detail: String,
    },
    /// The request never reached the provider.
    Network {
        provider: ProviderId,
        detail: String,
    },
    /// The request exceeded the configured timeout.
    Timeout {
        provider: ProviderId,
        timeout: Duration,
    },
    /// The provider answered with something that could not be understood.
    Parse {
        provider: ProviderId,
        detail: String,
    },
    /// A local, non-recoverable problem (for example an HTTP client that could
    /// not be constructed).
    Internal { detail: String },
    /// Every provider in the rotation failed; holds a pre-joined summary.
    AllProvidersFailed { attempts: String },
}

impl LlmError {
    /// Provider the failure belongs to, when it is provider-specific.
    pub fn provider(&self) -> Option<ProviderId> {
        match self {
            LlmError::NotConfigured(provider) => Some(*provider),
            LlmError::InvalidRequest { provider, .. }
            | LlmError::InvalidConfig { provider, .. }
            | LlmError::Auth { provider, .. }
            | LlmError::RateLimited { provider, .. }
            | LlmError::QuotaExceeded { provider, .. }
            | LlmError::BadRequest { provider, .. }
            | LlmError::Server { provider, .. }
            | LlmError::Network { provider, .. }
            | LlmError::Timeout { provider, .. }
            | LlmError::Parse { provider, .. } => Some(*provider),
            LlmError::NoProvidersConfigured
            | LlmError::PromptInvalid { .. }
            | LlmError::Internal { .. }
            | LlmError::AllProvidersFailed { .. } => None,
        }
    }

    /// Server-provided delay hint for retries.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            LlmError::RateLimited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// Whether repeating the same call may succeed after a short delay.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimited { .. }
                | LlmError::Server { .. }
                | LlmError::Network { .. }
                | LlmError::Timeout { .. }
        )
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::NotConfigured(provider) => {
                write!(f, "{provider}: credentials are not configured")
            }
            LlmError::NoProvidersConfigured => f.write_str("no LLM providers are configured"),
            LlmError::InvalidRequest { provider, detail } => {
                write!(f, "{provider}: invalid request: {detail}")
            }
            LlmError::InvalidConfig { provider, detail } => {
                write!(f, "{provider}: invalid configuration: {detail}")
            }
            LlmError::PromptInvalid { detail } => {
                write!(f, "the prayer prompt could not be rendered: {detail}")
            }
            LlmError::Auth {
                provider,
                status,
                detail,
            } => write!(
                f,
                "{provider}: authentication failed (HTTP {status}): {detail}"
            ),
            LlmError::RateLimited {
                provider,
                retry_after,
                detail,
            } => {
                write!(f, "{provider}: rate limited")?;
                if let Some(after) = retry_after {
                    write!(f, " (retry after {after:?})")?;
                }
                write!(f, ": {detail}")
            }
            LlmError::QuotaExceeded { provider, detail } => {
                write!(f, "{provider}: quota exhausted: {detail}")
            }
            LlmError::BadRequest {
                provider,
                status,
                detail,
            } => write!(
                f,
                "{provider}: request rejected (HTTP {status}): {detail}"
            ),
            LlmError::Server {
                provider,
                status,
                detail,
            } => write!(f, "{provider}: provider error (HTTP {status}): {detail}"),
            LlmError::Network { provider, detail } => {
                write!(f, "{provider}: network error: {detail}")
            }
            LlmError::Timeout { provider, timeout } => {
                write!(f, "{provider}: request timed out after {timeout:?}")
            }
            LlmError::Parse { provider, detail } => {
                write!(f, "{provider}: could not parse response: {detail}")
            }
            LlmError::Internal { detail } => write!(f, "internal error: {detail}"),
            LlmError::AllProvidersFailed { attempts } => {
                write!(f, "all providers failed: {attempts}")
            }
        }
    }
}

impl std::error::Error for LlmError {}
