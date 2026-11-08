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

pub mod batch;
pub mod clergy;
pub mod config;
pub mod errors;
pub mod generate;
pub mod prompts;
pub mod provider;
pub mod providers;
pub mod redact;
pub mod types;

pub use batch::{split_prayers, DEFAULT_PROFILE_BATCH, MAX_BATCH, SEPARATOR};
pub use clergy::intercession::{target_for, MAX_REPEAT, MAX_TEXT_CHARS};
pub use clergy::scheduler::{DEFAULT_BUDGET_PER_HOUR, OFFLINE_AFTER_FAILURES};
pub use clergy::{
    Cadence, Direction, Health, Intercession, IntercessionQueue, IntercessionRequest,
    IntercessionStatus, IntercessionTask, Meter, Pace, Policy, ProfileCatalog, ProfileError,
    QueueError, RecordOutcome, ReligionProfile, Severity, SubmitOutcome, Tempo, Theme,
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
