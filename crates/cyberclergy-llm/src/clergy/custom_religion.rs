//! Turning a person's own description of a tradition into a chapel profile.
//!
//! The model is asked for a small JSON document, but nothing it says is trusted:
//! every field is cleaned, capped and re-assembled here, and the finished
//! profile must pass [`ReligionProfile::validate`] before it reaches the disk.
//! A confused or hostile answer can therefore only make the draft fail, never
//! leave the chapel with a broken tradition.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::batch::DEFAULT_PROFILE_BATCH;
use crate::prompts::{PromptContext, PromptTemplate};
use crate::types::ChatRequest;

use super::profile::{Identity, OfflineCanon, ReligionProfile, Schedule, Style};

/// Longest name of a tradition; the settings column is narrow.
pub const MAX_TITLE_CHARS: usize = 60;
/// Longest description accepted; anything longer is a document, not a draft.
pub const MAX_DESCRIPTION_CHARS: usize = 1_500;
pub const MAX_NAME_CHARS: usize = 48;
pub const MAX_OFFLINE_PRAYERS: usize = 4;
const MAX_OFFLINE_PRAYER_CHARS: usize = 380;

/// What the person typed into the "new tradition" form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReligionDraft {
    pub title: String,
    pub description: String,
    /// Optional wish about the tone; empty lets the model choose.
    pub tone: String,
    /// Interface language of the person, used for the canon and the prompt.
    pub language: String,
}

impl ReligionDraft {
    /// Rejects a draft before a single request is spent on it.
    pub fn validate(&self) -> Result<(), String> {
        let title = self.title.trim();
        let length = title.chars().count();
        if length < 2 {
            return Err("the name of the tradition is too short".to_string());
        }
        if length > MAX_TITLE_CHARS {
            return Err(format!(
                "the name of the tradition is longer than {MAX_TITLE_CHARS} characters"
            ));
        }
        if self.description.split_whitespace().count() < 6 {
            return Err("describe the tradition in a few sentences first".to_string());
        }
        if self.description.chars().count() > MAX_DESCRIPTION_CHARS {
            return Err(format!(
                "the description is longer than {MAX_DESCRIPTION_CHARS} characters"
            ));
        }
        Ok(())
    }

    /// Language code the canon is asked for, clamped to what the app speaks.
    pub fn language(&self) -> String {
        let code = self.language.trim().to_ascii_lowercase();
        match code.as_str() {
            "ru" | "uk" | "he" | "de" | "es" | "en" => code,
            _ => "en".to_string(),
        }
    }

    /// The request the religion builder sends to whichever network answers.
    pub fn request(&self, template: &PromptTemplate) -> Result<ChatRequest, String> {
        let tone = clean(&self.tone);
        let context = PromptContext::new()
            .set("title", clean(&self.title))
            .set("description", clean(&self.description))
            .set(
                "tone",
                if tone.is_empty() {
                    "no particular preference".to_string()
                } else {
                    tone
                },
            )
            .set("language", self.language());
        let system = template.render(&context).map_err(|error| error.to_string())?;
        Ok(ChatRequest::user(
            "Write the JSON document now, and nothing else.",
        )
        .with_system(system)
        .with_max_tokens(1_200))
    }
}

/// The subset of the answer that is used; extra keys are ignored on purpose,
/// because a chatty model should not break a usable profile.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Answer {
    names: BTreeMap<String, String>,
    tradition: Option<String>,
    prayer_term: Option<String>,
    closing_formula: Option<String>,
    tone: Option<String>,
    length: Option<String>,
    short_length: Option<String>,
    offline: BTreeMap<String, Vec<String>>,
}

/// Parses the model's answer into a profile that is ready to be written.
///
/// `taken` holds the ids already in use; a duplicate title becomes `name-2`.
pub fn parse_answer(
    raw: &str,
    draft: &ReligionDraft,
    taken: &[&str],
) -> Result<ReligionProfile, String> {
    let json = extract_json(raw)?;
    let answer: Answer = serde_json::from_str(json)
        .map_err(|error| format!("the model returned a profile that does not parse: {error}"))?;

    let title = truncate(&clean(&draft.title), MAX_NAME_CHARS);
    let mut names = BTreeMap::new();
    for (code, value) in answer.names {
        let name = truncate(&clean(&value), MAX_NAME_CHARS);
        if !name.is_empty() {
            names.insert(code, name);
        }
    }
    // A missing name is the original one, never the raw id.
    for code in ["en", "ru", "uk", "he", "de", "es"] {
        names
            .entry(code.to_string())
            .or_insert_with(|| title.clone());
    }

    let prayers = canon(answer.offline);
    if prayers.is_empty() {
        return Err("the model returned no short prayers; try again".to_string());
    }

    let tone = text(answer.tone, 200)
        .or_else(|| text(Some(draft.tone.clone()), 200))
        .unwrap_or_else(|| "reverent and warm".to_string());
    let tradition = text(answer.tradition, 300).unwrap_or_else(|| {
        format!("{title}, described by the person using the application")
    });

    let profile = ReligionProfile {
        id: unique_id(slug(&draft.title), taken),
        version: 1,
        identity: Identity {
            names,
            tradition,
            prayer_term: text(answer.prayer_term, 40).unwrap_or_else(|| "prayer".to_string()),
            closing_formula: text(answer.closing_formula, 60),
        },
        style: Style {
            tone,
            length: text(answer.length, 200).unwrap_or_else(|| {
                "one or two short paragraphs of roughly 120 to 200 words".to_string()
            }),
            short_length: text(answer.short_length, 200)
                .unwrap_or_else(|| "one short paragraph of roughly 40 to 60 words".to_string()),
        },
        batch: DEFAULT_PROFILE_BATCH,
        schedule: Schedule {
            interval_seconds: 900,
            min_interval_seconds: 300,
        },
        addendum: addendum(draft, &title),
        offline: OfflineCanon {
            version: 1,
            prayers,
        },
    };

    profile.validate().map_err(|error| error.to_string())?;
    Ok(profile)
}

/// The person's words are the only source the tradition-notes may point at.
fn addendum(draft: &ReligionDraft, title: &str) -> String {
    format!(
        "- This tradition was described by the person using the application as: {title}.\n\
         - Their own words, treated as the only source of its beliefs and practices:\n\
         {}\n\
         - Follow the tone they asked for, and keep the prayers gentle.\n\
         - Never invent history, scriptures, clergy, organisations or claims the description does not support.\n\
         - Never present invented words as a quotation of any real text.\n",
        clean(&draft.description)
    )
}

/// Keeps only the useful, harmless part of a model string.
fn text(value: Option<String>, max: usize) -> Option<String> {
    value
        .map(|value| truncate(&clean(&value), max))
        .filter(|value| !value.is_empty())
}

/// Folds whitespace and drops control characters, so a pasted document cannot
/// smuggle tabs, newlines or terminal escapes into a single-line field.
fn clean(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_control() || character.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(character);
    }
    out.trim().to_string()
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Reads the first JSON object out of an answer that may be wrapped in fences.
fn extract_json(raw: &str) -> Result<&str, String> {
    let start = raw
        .find('{')
        .ok_or_else(|| "the model did not return a profile".to_string())?;
    let end = raw
        .rfind('}')
        .filter(|end| *end > start)
        .ok_or_else(|| "the model returned an unfinished profile".to_string())?;
    Ok(&raw[start..=end])
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for character in title.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(character);
        } else {
            pending_dash = !out.is_empty();
        }
        if out.chars().count() >= 24 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "tradition".to_string()
    } else {
        trimmed
    }
}

fn unique_id(base: String, taken: &[&str]) -> String {
    if !taken.contains(&base.as_str()) {
        return base;
    }
    for index in 2..100 {
        let candidate = format!("{base}-{index}");
        if !taken.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    format!("{base}-{}", taken.len() + 1)
}

/// Cleans the canon and guarantees an English list, because validation and the
/// quiet hours both start from English.
fn canon(prayers: BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for (language, list) in prayers {
        let cleaned: Vec<String> = list
            .into_iter()
            .map(|prayer| truncate(&clean(&prayer), MAX_OFFLINE_PRAYER_CHARS))
            .filter(|prayer| !prayer.is_empty())
            .take(MAX_OFFLINE_PRAYERS)
            .collect();
        if !cleaned.is_empty() {
            out.insert(language, cleaned);
        }
    }
    if !out.contains_key("en") {
        if let Some((_, list)) = out.iter().next() {
            out.insert("en".to_string(), list.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_answer, ReligionDraft, MAX_DESCRIPTION_CHARS};

    fn draft() -> ReligionDraft {
        ReligionDraft {
            title: "Shinto".to_string(),
            description: "A Japanese way of honouring the kami of mountains, rivers and the home."
                .to_string(),
            tone: "quiet and grateful".to_string(),
            language: "ru".to_string(),
        }
    }

    const ANSWER: &str = r#"```json
{
  "names": {"en": "Shinto", "ru": "Синто", "he": "שינטו"},
  "tradition": "An indigenous Japanese tradition honouring the kami.",
  "prayer_term": "norito",
  "closing_formula": null,
  "tone": "calm and respectful",
  "offline": {
    "en": ["We thank the kami of this place."],
    "ru": ["Благодарим ками этого места."]
  }
}
```"#;

    #[test]
    fn a_fenced_answer_becomes_a_valid_profile() {
        let profile = parse_answer(ANSWER, &draft(), &[]).expect("the answer is usable");
        assert_eq!(profile.id, "shinto");
        assert_eq!(profile.localized_name("ru"), "Синто");
        // Missing translations fall back to the name the person typed.
        assert_eq!(profile.localized_name("de"), "Shinto");
        assert_eq!(profile.identity.prayer_term, "norito");
        assert_eq!(profile.identity.closing_formula, None);
        assert_eq!(profile.offline.len("ru"), 1);
        profile.validate().expect("the profile is valid");
    }

    #[test]
    fn a_duplicate_name_gets_a_number() {
        let profile = parse_answer(ANSWER, &draft(), &["shinto", "shinto-2"]).expect("usable");
        assert_eq!(profile.id, "shinto-3");
    }

    #[test]
    fn a_canon_without_english_still_validates() {
        let answer = r#"{"offline": {"ru": ["Тихая молитва."]}}"#;
        let profile = parse_answer(answer, &draft(), &[]).expect("usable");
        assert_eq!(profile.offline.pick("de", 0), Some("Тихая молитва."));
    }

    #[test]
    fn garbage_and_injection_are_rejected() {
        assert!(parse_answer("I cannot help with that.", &draft(), &[]).is_err());
        let broken = r#"{"offline": {"en": ["ok"]}, "tradition": 42}"#;
        assert!(parse_answer(broken, &draft(), &[]).is_err());

        // A description of nothing, or one that is far too long, never costs a request.
        let mut thin = draft();
        thin.description = "too short".to_string();
        assert!(thin.validate().is_err());
        let mut long = draft();
        long.description = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
        assert!(long.validate().is_err());
    }

    #[test]
    fn the_prompt_is_rendered_with_the_persons_own_words() {
        let template = cyberclergy_llm_prompts();
        let request = draft()
            .request(&template)
            .expect("the template renders");
        let system = request.system.clone().unwrap_or_default();
        assert!(system.contains("Shinto"));
        assert!(system.contains("kami of mountains"));
        assert!(system.contains("quiet and grateful"));
    }

    fn cyberclergy_llm_prompts() -> crate::prompts::PromptTemplate {
        crate::prompts::PromptTemplate::religion_profile()
    }
}
