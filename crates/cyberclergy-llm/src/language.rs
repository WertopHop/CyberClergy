//! The languages the chapel can speak.
//!
//! One table drives three things that used to be separate lists: which values
//! are accepted from the settings file, which name the prompt is given, and
//! which entries the settings screen offers. Adding a language is one row here
//! plus one dictionary file on the interface side.

/// A language the application knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// Short tag stored in the settings and in the profile `names` map.
    pub code: &'static str,
    /// Full name handed to the model: models follow a name far more reliably
    /// than a two-letter code.
    pub english_name: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language {
        code: "en",
        english_name: "English",
    },
    Language {
        code: "ru",
        english_name: "Russian",
    },
    Language {
        code: "uk",
        english_name: "Ukrainian",
    },
    Language {
        code: "he",
        english_name: "Hebrew",
    },
    Language {
        code: "de",
        english_name: "German",
    },
    Language {
        code: "es",
        english_name: "Spanish",
    },
];

/// Looks a language up, ignoring case and surrounding whitespace.
pub fn find(code: &str) -> Option<&'static Language> {
    let code = code.trim();
    LANGUAGES
        .iter()
        .find(|language| language.code.eq_ignore_ascii_case(code))
}

pub fn is_supported(code: &str) -> bool {
    find(code).is_some()
}

/// Name for the prompt; an unknown code still prays, in English.
pub fn english_name(code: &str) -> &'static str {
    find(code)
        .map(|language| language.english_name)
        .unwrap_or("English")
}

#[cfg(test)]
mod tests {
    use super::{english_name, is_supported, LANGUAGES};

    #[test]
    fn the_table_is_unique_and_covers_the_interface() {
        let mut codes: Vec<&str> = LANGUAGES.iter().map(|language| language.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), LANGUAGES.len());
        assert_eq!(LANGUAGES.len(), 6);
        assert!(LANGUAGES
            .iter()
            .all(|language| !language.english_name.trim().is_empty()));
    }

    #[test]
    fn a_code_is_matched_loosely_and_unknown_ones_fall_back_to_english() {
        assert!(is_supported(" RU "));
        assert!(!is_supported("fr"));
        assert_eq!(english_name("he"), "Hebrew");
        assert_eq!(english_name("klingon"), "English");
    }
}
