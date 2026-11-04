//! Core LLM layer for CyberClergy.
//!
//! The crate exposes one provider-agnostic request/response API over several
//! free-tier inference services. Every provider is optional: a provider whose
//! credentials are absent is never registered, so the application keeps
//! running with whatever subset the user has configured.
//!
//! Prayer composition lives in [`generate`] and always goes through the single
//! editable prompt in `resources/prompts/system_prompt.md`, so every provider
//! receives identical instructions.

pub mod clergy;
pub mod config;
pub mod errors;
pub mod generate;
pub mod prompts;
pub mod provider;
pub mod providers;
pub mod redact;
pub mod types;

pub use clergy::intercession::{target_for, MAX_TEXT_CHARS};
pub use clergy::{
    Direction, Intercession, IntercessionQueue, IntercessionStatus, IntercessionTask, ProfileCatalog,
    ProfileError, QueueError, RecordOutcome, ReligionProfile, Severity, SubmitOutcome, Theme,
};
pub use config::{
    ProviderConfig, ProviderSpec, ProviderStatus, Protocol, Settings, SharedSettings,
    PROVIDER_SPECS,
};
pub use errors::LlmError;
pub use generate::{build_prayer_request, PrayerGenerator, USER_INSTRUCTION};
pub use prompts::{
    PrayerSpec, PromptContext, PromptError, PromptTemplate, SYSTEM_PROMPT_TEMPLATE,
};
pub use provider::{Provider, Registry};
pub use types::{ChatMessage, ChatRequest, ChatResponse, ProviderId, Role, Usage};
