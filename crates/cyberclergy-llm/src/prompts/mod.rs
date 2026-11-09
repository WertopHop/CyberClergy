//! Prayer prompt templates shared by every provider.
//!
//! The editable prompt lives at `resources/prompts/system_prompt.md` and starts
//! on the first line of that file, so it can be tuned without touching code.
//! Placeholders use the `{{name}}` form and rendering is strict: a typo in the
//! template fails the request instead of silently sending a half-filled prompt
//! to a provider.

use std::collections::BTreeMap;
use std::fmt;

/// The system prompt that every provider receives.
pub const SYSTEM_PROMPT_TEMPLATE: &str =
    include_str!("../../resources/prompts/system_prompt.md");

/// The prompt that turns a person's description of a tradition into a profile.
/// Kept apart from the prayer prompt: this one asks for a document, not a prayer.
pub const RELIGION_PROFILE_TEMPLATE: &str =
    include_str!("../../resources/prompts/religion_profile.md");

pub const KEY_TRADITION: &str = "tradition";
pub const KEY_BATCH: &str = "batch";
pub const KEY_LANGUAGE: &str = "language";
pub const KEY_TONE: &str = "tone";
pub const KEY_PRAYER_LENGTH: &str = "prayer_length";
pub const KEY_INTENT: &str = "intent";
pub const KEY_USER_NAME: &str = "user_name";
pub const KEY_TRADITION_NOTES: &str = "tradition_notes";

pub const DEFAULT_TRADITION: &str =
    "an unspecified spiritual tradition, addressing the divine in inclusive and respectful terms";
pub const DEFAULT_LANGUAGE: &str = "English";
pub const DEFAULT_TONE: &str = "reverent and warm";
pub const DEFAULT_PRAYER_LENGTH: &str = "one or two short paragraphs of roughly 120 to 200 words";
/// Prayers per response when the caller says nothing. One, because a caller that
/// does not know about batches (the `pray` binary, tests) must get one prayer;
/// the scheduler asks for the profile's batch explicitly.
pub const DEFAULT_BATCH: u32 = 1;
pub const DEFAULT_INTENT: &str = "general wellbeing, protection and peace";
pub const DEFAULT_USER_NAME: &str = "the person you serve";
pub const DEFAULT_TRADITION_NOTES: &str =
    "No additional tradition-specific notes; follow the general guidance above.";

/// Problems that make a prompt unusable. They are always reported, never
/// papered over, because a malformed prompt looks like bad model output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptError {
    EmptyTemplate {
        template: String,
    },
    UnknownPlaceholder {
        template: String,
        key: String,
    },
    UnterminatedPlaceholder {
        template: String,
        rest: String,
    },
}

impl fmt::Display for PromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PromptError::EmptyTemplate { template } => {
                write!(f, "template `{template}` is empty")
            }
            PromptError::UnknownPlaceholder { template, key } => write!(
                f,
                "template `{template}` uses placeholder `{{{{{key}}}}}`, which has no value"
            ),
            PromptError::UnterminatedPlaceholder { template, rest } => write!(
                f,
                "template `{template}` has an unterminated placeholder near `{rest}`"
            ),
        }
    }
}

impl std::error::Error for PromptError {}

/// Values substituted into a template. Keys are matched without braces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptContext {
    values: BTreeMap<String, String>,
}

impl PromptContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// A named prompt template with `{{placeholder}}` slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    name: String,
    source: String,
}

impl PromptTemplate {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
        }
    }

    /// The embedded system prompt that all providers share.
    pub fn system_prompt() -> Self {
        Self::new("system_prompt", SYSTEM_PROMPT_TEMPLATE)
    }

    /// The embedded prompt the religion builder renders a draft with.
    pub fn religion_profile() -> Self {
        Self::new("religion_profile", RELIGION_PROFILE_TEMPLATE)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Placeholder keys in order of first appearance, without duplicates.
    pub fn placeholders(&self) -> Result<Vec<String>, PromptError> {
        let mut placeholders: Vec<String> = Vec::new();
        for key in scan_placeholders(&self.source, &self.name)? {
            if !placeholders.iter().any(|known| known == key) {
                placeholders.push(key.to_string());
            }
        }
        Ok(placeholders)
    }

    /// Substitutes every placeholder. Unknown keys and malformed braces are errors.
    pub fn render(&self, context: &PromptContext) -> Result<String, PromptError> {
        if self.source.trim().is_empty() {
            return Err(PromptError::EmptyTemplate {
                template: self.name.clone(),
            });
        }

        let mut output = String::with_capacity(self.source.len() + 256);
        let mut rest = self.source.as_str();
        while let Some(start) = rest.find("{{") {
            output.push_str(&rest[..start]);
            let after_open = &rest[start + 2..];
            let Some(end) = after_open.find("}}") else {
                return Err(PromptError::UnterminatedPlaceholder {
                    template: self.name.clone(),
                    rest: after_open.trim().to_string(),
                });
            };
            let key = after_open[..end].trim();
            let value = context.get(key).ok_or_else(|| PromptError::UnknownPlaceholder {
                template: self.name.clone(),
                key: key.to_string(),
            })?;
            output.push_str(value);
            rest = &after_open[end + 2..];
        }
        output.push_str(rest);
        Ok(output)
    }
}

/// Everything that shapes a single prayer request. Empty values fall back to
/// the defaults declared above, so callers only pass what they actually know.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrayerSpec {
    pub tradition: Option<String>,
    pub language: Option<String>,
    pub tone: Option<String>,
    pub length: Option<String>,
    pub intent: Option<String>,
    pub user_name: Option<String>,
    /// Free-form notes injected into the `Tradition-specific notes` section.
    /// Religion profiles use this for their addendum.
    pub tradition_notes: Option<String>,
    /// Prayers the response should contain, separated by the `---` line. Free
    /// tiers bill per request, so one call answering several prayers is the
    /// cheapest way to pray more often.
    pub batch: Option<u32>,
}

impl PrayerSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tradition(mut self, tradition: impl Into<String>) -> Self {
        self.tradition = Some(tradition.into());
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn with_tone(mut self, tone: impl Into<String>) -> Self {
        self.tone = Some(tone.into());
        self
    }

    pub fn with_length(mut self, length: impl Into<String>) -> Self {
        self.length = Some(length.into());
        self
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }

    pub fn with_user_name(mut self, name: impl Into<String>) -> Self {
        self.user_name = Some(name.into());
        self
    }

    pub fn with_tradition_notes(mut self, notes: impl Into<String>) -> Self {
        self.tradition_notes = Some(notes.into());
        self
    }

    pub fn with_batch(mut self, batch: u32) -> Self {
        self.batch = Some(batch.clamp(1, crate::batch::MAX_BATCH));
        self
    }

    /// Prayers this request should yield, clamped to what the prompt contract
    /// allows.
    pub fn batch(&self) -> u32 {
        self.batch
            .unwrap_or(DEFAULT_BATCH)
            .clamp(1, crate::batch::MAX_BATCH)
    }

    /// Builds the complete placeholder context, applying defaults for anything
    /// the caller left out or left blank.
    pub fn context(&self) -> PromptContext {
        PromptContext::new()
            .set(
                KEY_TRADITION,
                normalized(&self.tradition).unwrap_or_else(|| DEFAULT_TRADITION.to_string()),
            )
            .set(
                KEY_LANGUAGE,
                normalized(&self.language).unwrap_or_else(|| DEFAULT_LANGUAGE.to_string()),
            )
            .set(KEY_TONE, normalized(&self.tone).unwrap_or_else(|| DEFAULT_TONE.to_string()))
            .set(
                KEY_PRAYER_LENGTH,
                normalized(&self.length).unwrap_or_else(|| DEFAULT_PRAYER_LENGTH.to_string()),
            )
            .set(
                KEY_INTENT,
                normalized(&self.intent).unwrap_or_else(|| DEFAULT_INTENT.to_string()),
            )
            .set(
                KEY_USER_NAME,
                normalized(&self.user_name).unwrap_or_else(|| DEFAULT_USER_NAME.to_string()),
            )
            .set(
                KEY_TRADITION_NOTES,
                normalized(&self.tradition_notes)
                    .unwrap_or_else(|| DEFAULT_TRADITION_NOTES.to_string()),
            )
            .set(KEY_BATCH, self.batch().to_string())
    }
}

fn normalized(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn scan_placeholders<'a>(source: &'a str, template: &str) -> Result<Vec<&'a str>, PromptError> {
    let mut keys = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}}") else {
            return Err(PromptError::UnterminatedPlaceholder {
                template: template.to_string(),
                rest: after_open.trim().to_string(),
            });
        };
        keys.push(after_open[..end].trim());
        rest = &after_open[end + 2..];
    }
    Ok(keys)
}
