//! Core LLM provider layer for CyberClergy.
//!
//! The crate exposes one provider-agnostic request/response API over several
//! free-tier inference services. Every provider is optional: a provider whose
//! credentials are absent is never registered, so the application keeps
//! running with whatever subset the user has configured.

pub mod config;
pub mod errors;
pub mod provider;
pub mod providers;
pub mod redact;
pub mod types;

pub use config::{ProviderStatus, Settings, SharedSettings};
pub use errors::LlmError;
pub use provider::{Provider, Registry};
pub use types::{ChatMessage, ChatRequest, ChatResponse, ProviderId, Role, Usage};
