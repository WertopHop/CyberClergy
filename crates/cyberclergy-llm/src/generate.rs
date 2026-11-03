//! The single funnel every provider goes through when composing a prayer.
//!
//! Keeping this in one place guarantees that all networks receive the exact
//! same system prompt and user turn; only the transport differs.

use crate::errors::LlmError;
use crate::prompts::{PrayerSpec, PromptError, PromptTemplate};
use crate::provider::Registry;
use crate::types::{ChatMessage, ChatRequest, ChatResponse, ProviderId};

/// The user turn is intentionally minimal: all instructions live in the
/// editable system prompt under `resources/prompts/system_prompt.md`.
pub const USER_INSTRUCTION: &str = "Compose the prayer now.";

/// Renders the system prompt and wraps it into a provider-agnostic request.
pub fn build_prayer_request(
    template: &PromptTemplate,
    spec: &PrayerSpec,
) -> Result<ChatRequest, PromptError> {
    let system = template.render(&spec.context())?;
    Ok(ChatRequest {
        system: Some(system),
        messages: vec![ChatMessage::user(USER_INSTRUCTION)],
        model: None,
        temperature: None,
        max_tokens: None,
    })
}

/// Owns the provider registry together with the prayer template.
pub struct PrayerGenerator {
    registry: Registry,
    template: PromptTemplate,
}

impl PrayerGenerator {
    /// Uses the embedded system prompt.
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            template: PromptTemplate::system_prompt(),
        }
    }

    /// Uses a custom template (useful for religion-specific overrides later).
    pub fn with_template(registry: Registry, template: PromptTemplate) -> Self {
        Self { registry, template }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn template(&self) -> &PromptTemplate {
        &self.template
    }

    pub fn build_request(&self, spec: &PrayerSpec) -> Result<ChatRequest, PromptError> {
        build_prayer_request(&self.template, spec)
    }

    /// Composes one prayer, walking the configured provider order on failure.
    pub async fn generate(&self, spec: &PrayerSpec) -> Result<ChatResponse, LlmError> {
        let request = self.prepare(spec)?;
        self.registry.chat_with_fallback(&request).await
    }

    /// Composes one prayer through a specific provider.
    pub async fn generate_with_provider(
        &self,
        provider: ProviderId,
        spec: &PrayerSpec,
    ) -> Result<ChatResponse, LlmError> {
        let request = self.prepare(spec)?;
        self.registry.chat(provider, &request).await
    }

    fn prepare(&self, spec: &PrayerSpec) -> Result<ChatRequest, LlmError> {
        self.build_request(spec)
            .map_err(|error| LlmError::PromptInvalid {
                detail: error.to_string(),
            })
    }
}
