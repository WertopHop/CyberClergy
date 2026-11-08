//! Turning one provider response into the individual prayers it contains.
//!
//! Free tiers bill per request far more often than per token, so a cleric asks
//! for a small batch of prayers in one call and the chapel spends them one at a
//! time, at the ordinary rhythm. The separator contract lives in
//! `resources/prompts/system_prompt.md`; this module is the only place that has
//! to understand it, and it is deliberately forgiving: models add numbering,
//! fences and preambles even when told not to.

/// Line the prompt asks the model to put between prayers.
pub const SEPARATOR: &str = "---";
/// Largest batch the chapel will ask for in a single request. Free models start
/// losing the thread (and count) past a handful of paragraphs.
pub const MAX_BATCH: u32 = 5;
/// Batch a religion profile asks for when its JSON says nothing.
pub const DEFAULT_PROFILE_BATCH: u32 = 3;

/// A chunk shorter than this is a leftover heading or a note, not a prayer.
const MIN_PRAYER_CHARS: usize = 24;
const MIN_PRAYER_WORDS: usize = 4;
/// Guard against a document that happens to start every line with a number.
const MAX_NUMBERED_CHUNKS: usize = 12;

/// Splits one response into prayers. Junk and empty chunks are dropped, and the
/// number of prayers is whatever the model actually produced — asking for three
/// and getting two is normal and must not break the chapel.
pub fn split_prayers(text: &str) -> Vec<String> {
    let cleaned = drop_fences(text);

    let mut chunks = split_on_separators(&cleaned);
    if chunks.len() < 2 {
        // Some models ignore the separator and number the prayers instead.
        let numbered = split_on_numbering(&cleaned);
        if numbered.len() > 1 {
            chunks = numbered;
        }
    }

    chunks
        .iter()
        .map(|chunk| tidy(chunk))
        .filter(|prayer| is_prayer(prayer))
        .collect()
}

/// Removes fenced-code markers, which some models wrap prose in.
fn drop_fences(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with("```") || trimmed.starts_with("~~~"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn split_on_separators(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if is_separator(line) {
            chunks.push(std::mem::take(&mut current));
            continue;
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    chunks.push(current);
    chunks
}

/// A line made only of rule characters: `---`, `***`, `___`, `===`.
fn is_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= 3
        && trimmed
            .chars()
            .all(|character| matches!(character, '-' | '—' | '–' | '*' | '_' | '='))
}

fn split_on_numbering(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut starts: Vec<usize> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        // Only a run that counts 1, 2, 3... is a prayer list; anything else is
        // prose that happens to begin with a digit.
        if numbered_prefix(line).is_some_and(|number| number == starts.len() + 1) {
            starts.push(index);
            if starts.len() >= MAX_NUMBERED_CHUNKS {
                break;
            }
        }
    }
    if starts.len() < 2 {
        return Vec::new();
    }
    starts
        .iter()
        .enumerate()
        .map(|(position, start)| {
            let end = starts.get(position + 1).copied().unwrap_or(lines.len());
            lines[*start..end].join("\n")
        })
        .collect()
}

/// Number of an ordered-list line (`1.`, `2)`, `3:`, and the bold variants).
fn numbered_prefix(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let trimmed = trimmed
        .trim_start_matches('*')
        .trim_start_matches('_')
        .trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 2 {
        return None;
    }
    let rest = trim_list_marker(&trimmed[digits..])?;
    // `1.5 hours of silence` is text, `1. Holy One,` is a list item.
    if rest.starts_with(|character: char| character.is_ascii_digit()) {
        return None;
    }
    trimmed[..digits].parse().ok()
}

fn trim_list_marker(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix('.')
        .or_else(|| text.strip_prefix(')'))
        .or_else(|| text.strip_prefix(':'))?;
    Some(rest.trim_start())
}

/// Strips the decorative wrapping models add around a prayer.
fn tidy(chunk: &str) -> String {
    let mut text = chunk.trim();
    loop {
        let shorter = strip_prefix_noise(text);
        if shorter.len() == text.len() {
            break;
        }
        text = shorter.trim_start();
    }
    let text = text.trim_end().trim_end_matches(['*', '_']).trim_end();
    trim_quotes(text).trim().to_string()
}

fn strip_prefix_noise(text: &str) -> &str {
    if let Some(rest) = text
        .strip_prefix("- ")
        .or_else(|| text.strip_prefix("• "))
    {
        return rest.trim_start();
    }
    // Emphasis and quote markers come one character at a time: `*2)*` is a bold
    // list item, not an emphatic prayer.
    if let Some(rest) = text.strip_prefix(['*', '_', '#', '>']) {
        return rest.trim_start();
    }
    if let Some(rest) = strip_numbering(text) {
        return rest;
    }
    text
}

fn strip_numbering(text: &str) -> Option<&str> {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 2 {
        return None;
    }
    let rest = trim_list_marker(&text[digits..])?;
    if rest.starts_with(|character: char| character.is_ascii_digit()) {
        return None;
    }
    Some(rest)
}

fn trim_quotes(text: &str) -> &str {
    let pairs = [('"', '"'), ('«', '»'), ('“', '”'), ('„', '“')];
    for (open, close) in pairs {
        if text.starts_with(open) && text.ends_with(close) && text.chars().count() >= 2 {
            return &text[open.len_utf8()..text.len() - close.len_utf8()];
        }
    }
    text
}

fn is_prayer(text: &str) -> bool {
    text.chars().count() >= MIN_PRAYER_CHARS && text.split_whitespace().count() >= MIN_PRAYER_WORDS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_splits_the_batch() {
        let text = "Holy One, hold us in the quiet of this morning.\n\n---\n\n\
                    Source of mercy, steady the hands that tremble today.\n\n---\n\n\
                    Giver of peace, watch over the ones we cannot reach.";
        let prayers = split_prayers(text);
        assert_eq!(prayers.len(), 3);
        assert!(prayers[0].starts_with("Holy One"));
        assert!(prayers[1].starts_with("Source of mercy"));
        assert!(prayers[2].starts_with("Giver of peace"));
        assert!(prayers.iter().all(|prayer| !prayer.contains("---")));
    }

    #[test]
    fn a_single_prayer_needs_no_marker() {
        let text = "  Holy One, hold us in the quiet of this morning.  ";
        assert_eq!(
            split_prayers(text),
            vec!["Holy One, hold us in the quiet of this morning."]
        );
    }

    #[test]
    fn empty_and_junk_chunks_are_dropped() {
        let text = "Here are three prayers:\n\n---\n\n\
                    Holy One, hold us in the quiet of this morning.\n\n---\n\n\
                    \n\n---\n\n\
                    Amen.\n\n---\n\n\
                    Source of mercy, steady the hands that tremble today.";
        let prayers = split_prayers(text);
        assert_eq!(prayers.len(), 2);
        assert!(prayers[0].starts_with("Holy One"));
        assert!(prayers[1].starts_with("Source of mercy"));
    }

    #[test]
    fn numbered_lists_are_split_too() {
        let text = "1. Holy One, hold us in the quiet of this morning.\n\
                    2. Source of mercy, steady the hands that tremble today.\n\
                    3. Giver of peace, watch over the ones we cannot reach.";
        let prayers = split_prayers(text);
        assert_eq!(prayers.len(), 3);
        assert!(prayers[0].starts_with("Holy One"));
        assert!(prayers[2].starts_with("Giver of peace"));
    }

    #[test]
    fn list_numbering_bold_and_quotes_are_removed() {
        let text = "**1.** \"Holy One, hold us in the quiet of this morning.\"\n\n---\n\n\
                    *2)* «Source of mercy, steady the hands that tremble today.»";
        let prayers = split_prayers(text);
        assert_eq!(
            prayers[0], "Holy One, hold us in the quiet of this morning.",
            "numbering, bold and quotes are wrapping, not text"
        );
        assert_eq!(
            prayers[1], "Source of mercy, steady the hands that tremble today."
        );
    }

    #[test]
    fn fence_lines_do_not_leak_into_the_text() {
        let text = "```\nHoly One, hold us in the quiet of this morning.\n---\n\
                    Source of mercy, steady the hands that tremble today.\n```";
        let prayers = split_prayers(text);
        assert_eq!(prayers.len(), 2);
        assert!(prayers.iter().all(|prayer| !prayer.contains("```")));
    }

    #[test]
    fn paragraphs_inside_a_prayer_are_kept() {
        let text = "Holy One, hold us in the quiet of this morning.\n\n\
                    Let the day begin gently.\n\n---\n\n\
                    Source of mercy, steady the hands that tremble today.";
        let prayers = split_prayers(text);
        assert_eq!(prayers.len(), 2);
        assert!(prayers[0].contains("Let the day begin gently."));
    }

    #[test]
    fn a_separator_only_response_yields_nothing() {
        assert!(split_prayers("---\n---\n").is_empty());
        assert!(split_prayers("   ").is_empty());
    }
}
