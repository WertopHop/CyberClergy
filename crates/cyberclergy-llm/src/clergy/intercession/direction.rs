//! Turns a confessional entry into a prayer *direction*.
//!
//! The entry is classified locally: nothing from the user's text leaves the
//! device. The model only receives a short English phrase describing the kind
//! of need, appended to the shared system prompt through `{{intent}}`. That is
//! enough to shape the prayer without shipping private text to a free-tier
//! provider, and it keeps the prompt contractual instead of templating every
//! possible situation.
//!
//! Classification is a keyword rubric over Russian and English stems. It is
//! deterministic, instant, testable and works offline; a model call is not
//! needed to know that "я виноват перед мамой" is about guilt within a family.

use serde::{Deserialize, Serialize};

/// Recognised families of need, ordered from the most specific to the most
/// general. The order also breaks ties between equally matching themes, so a
/// text that reads like guilt inside a family becomes guilt, not family, and a
/// thank-you that happens to mention work stays gratitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Guilt,
    Grief,
    Illness,
    Family,
    Habit,
    Work,
    Anger,
    Fear,
    Loneliness,
    Doubt,
    Gratitude,
    Unnamed,
}

impl Theme {
    pub const ALL: [Theme; 12] = [
        Theme::Guilt,
        Theme::Grief,
        Theme::Illness,
        Theme::Gratitude,
        Theme::Family,
        Theme::Habit,
        Theme::Work,
        Theme::Anger,
        Theme::Fear,
        Theme::Loneliness,
        Theme::Doubt,
        Theme::Unnamed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Guilt => "guilt",
            Theme::Grief => "grief",
            Theme::Illness => "illness",
            Theme::Family => "family",
            Theme::Habit => "habit",
            Theme::Work => "work",
            Theme::Anger => "anger",
            Theme::Fear => "fear",
            Theme::Loneliness => "loneliness",
            Theme::Doubt => "doubt",
            Theme::Gratitude => "gratitude",
            Theme::Unnamed => "unnamed",
        }
    }

    /// One sentence describing the need. It is deliberately impersonal: no
    /// names, no specifics, no verdicts, only what the prayer should hold.
    pub fn direction(self) -> &'static str {
        match self {
            Theme::Guilt => {
                "Someone carrying guilt over something they cannot undo, who needs a way back to a clear heart."
            }
            Theme::Grief => {
                "Someone grieving a loss that is still close, and the silence that arrived with it."
            }
            Theme::Illness => {
                "Someone waiting on news about their own body or the body of someone they love, and the fear that comes with the waiting."
            }
            Theme::Family => {
                "Someone carrying a family bond that has gone cold or strained, and the wish to make it whole again."
            }
            Theme::Habit => {
                "Someone fighting a habit that keeps reaching back for them, taking one day at a time."
            }
            Theme::Work => {
                "Someone under pressure from work, money or study, afraid that they will not hold on."
            }
            Theme::Anger => {
                "Someone holding anger they no longer want to carry, who needs it to soften without losing what is just."
            }
            Theme::Fear => {
                "Someone whose fear has grown louder than their days, and who needs a steady place to stand."
            }
            Theme::Loneliness => {
                "Someone who feels alone even among people, and needs to be reminded that they are seen."
            }
            Theme::Doubt => {
                "Someone whose faith has grown quiet, and who still wants to pray without pretending to certainty."
            }
            Theme::Gratitude => {
                "Someone who wants to give thanks and does not know where to put the words."
            }
            Theme::Unnamed => {
                "Someone who brought a weight here without naming it, and needs to know that it was heard."
            }
        }
    }
}

/// How much room the prayer needs. Ordering matters: `Urgent` sorts first so
/// the queue can prefer pressing entries without extra bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Urgent,
    Steady,
    Gentle,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Urgent => "urgent",
            Severity::Steady => "steady",
            Severity::Gentle => "gentle",
        }
    }

    /// Extra instruction appended to the direction, if the situation asks for
    /// one.
    fn clause(self) -> &'static str {
        match self {
            Severity::Urgent => {
                "The need is pressing: name the fear plainly, ask for protection and endurance, and keep the prayer close to the ground."
            }
            Severity::Steady => "",
            Severity::Gentle => {
                "Keep the prayer light; quiet hope is welcome here."
            }
        }
    }
}

/// Result of reading one entry: what it is about and how heavy it feels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Direction {
    pub theme: Theme,
    pub severity: Severity,
    /// Ready to be substituted into `{{intent}}`.
    pub intent: String,
}

impl Direction {
    /// Reads an entry and produces its direction. Never fails: an unrecognised
    /// text still yields a respectful, unnamed direction.
    pub fn derive(text: &str) -> Self {
        let normalized = text.to_lowercase();
        let theme = detect_theme(&normalized);
        let severity = detect_severity(theme, &normalized, text.chars().count());
        let intent = compose(theme, severity);
        Self {
            theme,
            severity,
            intent,
        }
    }
}

fn detect_theme(normalized: &str) -> Theme {
    let words = split_words(normalized);
    let mut best: Option<(Theme, usize)> = None;
    for theme in Theme::ALL {
        let score = score(theme, normalized, &words);
        if score == 0 {
            continue;
        }
        // Strictly greater keeps the first theme on a tie, which is the order
        // of `Theme::ALL`.
        let improves = match best {
            None => true,
            Some((_, best_score)) => score > best_score,
        };
        if improves {
            best = Some((theme, score));
        }
    }
    best.map_or(Theme::Unnamed, |(theme, _)| theme)
}

fn detect_severity(theme: Theme, normalized: &str, characters: usize) -> Severity {
    if describes_acute_distress(normalized) {
        return Severity::Urgent;
    }
    if theme == Theme::Gratitude {
        return Severity::Gentle;
    }
    // A short confession of guilt, grief or illness still weighs: brevity is
    // not lightness there, so those themes keep the steady quota.
    let heavy = matches!(theme, Theme::Guilt | Theme::Grief | Theme::Illness);
    if !heavy && characters <= 80 {
        return Severity::Gentle;
    }
    Severity::Steady
}

fn compose(theme: Theme, severity: Severity) -> String {
    let clause = severity.clause();
    if clause.is_empty() {
        theme.direction().to_string()
    } else {
        format!("{} {clause}", theme.direction())
    }
}

fn score(theme: Theme, normalized: &str, words: &[&str]) -> usize {
    keywords(theme)
        .iter()
        .filter(|keyword| matches_keyword(keyword, normalized, words))
        .count()
}

/// Single-word stems match the beginning of a whole word, so "рак" (cancer)
/// never matches "брак" (marriage) while "раком" still does. Phrases are rare
/// and are matched as plain substrings.
fn matches_keyword(keyword: &str, normalized: &str, words: &[&str]) -> bool {
    if keyword.contains(' ') {
        normalized.contains(keyword)
    } else {
        words.iter().any(|word| word.starts_with(keyword))
    }
}

fn split_words(normalized: &str) -> Vec<&str> {
    normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn keywords(theme: Theme) -> &'static [&'static str] {
    THEME_KEYWORDS
        .iter()
        .find(|(candidate, _)| *candidate == theme)
        .map(|(_, keywords)| *keywords)
        .unwrap_or(&[])
}

fn contains_any(normalized: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| normalized.contains(marker))
}

/// Word prefixes are matched per word so "брак" (marriage) is not read as "рак"
/// (cancer), while "раком" and "смерти" still match their stems.
fn describes_acute_distress(normalized: &str) -> bool {
    if contains_any(normalized, URGENT_PHRASES) {
        return true;
    }
    normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| URGENT_WORD_STEMS.iter().any(|stem| word.starts_with(stem)))
}

// Stems instead of whole words, because Russian inflects heavily and the input
// is informal. Matching is a plain substring test on the lowercased text.
const THEME_KEYWORDS: &[(Theme, &[&str])] = &[
    (
        Theme::Guilt,
        &[
            "вин", "стыд", "прости", "прощен", "ошиб", "совест", "предал", "обманул",
            "грех", "раскаива", "guilt", "guilty", "shame", "ashame", "forgive",
            "mistake", "betray", "lied", "sinful", "remorse",
        ],
    ),
    (
        Theme::Grief,
        &[
            "умер", "смерт", "похорон", "потерял", "потеря", "скорб", "горе", "утрат",
            "отошёл", "отошел", "не стало", "вдов", "died", "death", "funeral", "grief",
            "griev", "loss", "lost", "widow", "passed away",
        ],
    ),
    (
        Theme::Illness,
        &[
            "болезн", "болит", "болею", "больниц", "диагноз", "рак", "онколог",
            "опухол", "операц", "врач", "лечен", "температур", "больно", "ill",
            "sick", "pain", "hospital", "diagnos", "cancer", "surgery",
            "treatment", "recover", "symptom",
        ],
    ),
    (
        Theme::Family,
        &[
            "семь", "мам", "пап", "мать", "отец", "сын", "дочь", "ребен", "муж", "жен",
            "брат", "сестр", "развод", "брак", "ссор", "родн", "mother", "father", "son",
            "daughter", "child", "wife", "husband", "brother", "sister", "family",
            "divorce", "marriage", "parents", "quarrel",
        ],
    ),
    (
        Theme::Habit,
        &[
            "пью", "пить", "алкогол", "курю", "сигарет", "нарко", "зависим", "срыва",
            "перееда", "drink", "alcohol", "smok", "drug", "addict", "relapse",
            "gambl", "binge",
        ],
    ),
    (
        Theme::Work,
        &[
            "работ", "увол", "начальник", "коллег", "зарплат", "денег", "деньг", "долг",
            "кредит", "учеб", "экзамен", "сесси", "проект", "дедлайн", "карьер",
            "work", "job", "boss", "colleague", "money", "debt", "loan", "exam",
            "study", "deadline", "fired", "salary", "career",
        ],
    ),
    (
        Theme::Anger,
        &[
            "злюсь", "злост", "злоба", "гнев", "ненав", "обид", "раздраж", "ярост",
            "anger", "angry", "hate", "resent", "furious", "rage", "bitter", "grudge",
        ],
    ),
    (
        Theme::Fear,
        &[
            "страш", "боюсь", "боит", "тревог", "паник", "беспоко", "переживаю",
            "fear", "afraid", "scared", "anxiety", "anxious", "panic", "worry",
            "dread",
        ],
    ),
    (
        Theme::Loneliness,
        &[
            "одинок", "никому не нужен", "нет друз", "покинут", "никого нет",
            "lonely", "loneliness", "alone", "isolated", "no friends", "abandoned",
        ],
    ),
    (
        Theme::Doubt,
        &[
            "верую", "верю", "веры", "вера", "верит", "верил", "сомнева", "надежд",
            "церк", "храм", "молит", "faith", "believe", "doubt", "church", "temple",
            "pray", "hope",
        ],
    ),
    (
        Theme::Gratitude,
        &[
            "спасибо", "благодар", "радост", "счаст", "повезло", "выздоровел",
            "thankful", "grateful", "gratitude", "joy", "blessed", "happy", "lucky",
        ],
    ),
];

// Markers of acute distress. They raise the severity, never the theme, so a
// pressing fear keeps its own direction. Stems match the beginning of a whole
// word; phrases are matched as substrings.
const URGENT_WORD_STEMS: &[&str] = &[
    "умер",
    "смерт",
    "умира",
    "рак",
    "онколог",
    "суицид",
    "похорон",
    "инсульт",
    "инфаркт",
    "реанимац",
    "терминальн",
    "died",
    "death",
    "dying",
    "dead",
    "suicide",
    "terminal",
    "cancer",
    "overdose",
];

const URGENT_PHRASES: &[&str] = &[
    "не хочу жить",
    "не могу больше жить",
    "тяжело болен",
    "kill myself",
    "intensive care",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_guilt_is_recognised() {
        let direction = Direction::derive("я виноват перед мамой, прости меня, я соврал");
        assert_eq!(direction.theme, Theme::Guilt);
        assert!(direction.intent.contains("guilt"));
        // A short confession is not a light one.
        assert_eq!(direction.severity, Severity::Steady);
        assert!(!direction.intent.contains("light"));
    }

    #[test]
    fn marriage_is_family_not_cancer() {
        let direction = Direction::derive(
            "в моём браке всё стало холодно, мы почти не говорим, и я не знаю, \
             как вернуть то, что было между нами раньше, хотя очень хочу",
        );
        assert_eq!(direction.theme, Theme::Family);
        assert_eq!(direction.severity, Severity::Steady);
    }

    #[test]
    fn illness_with_cancer_is_urgent() {
        let direction = Direction::derive("вчера услышал диагноз, у мамы рак, я не знаю как жить дальше");
        assert_eq!(direction.theme, Theme::Illness);
        assert_eq!(direction.severity, Severity::Urgent);
        assert!(direction.intent.contains("pressing"));
    }

    #[test]
    fn english_grief_is_recognised() {
        let direction = Direction::derive("my father died last month and I cannot stop crying");
        assert_eq!(direction.theme, Theme::Grief);
        assert_eq!(direction.severity, Severity::Urgent);
    }

    #[test]
    fn gratitude_stays_gentle_even_when_long() {
        let direction = Direction::derive(
            "хочу просто сказать спасибо за этот год, за то что дети рядом, за работу, \
             за то что удалось пережить всё это без потерь и остаться собой",
        );
        assert_eq!(direction.theme, Theme::Gratitude);
        assert_eq!(direction.severity, Severity::Gentle);
    }

    #[test]
    fn unrecognised_entry_is_still_respectful() {
        let direction = Direction::derive("не знаю как это назвать");
        assert_eq!(direction.theme, Theme::Unnamed);
        assert!(direction.intent.contains("without naming it"));
    }
    #[test]
    fn every_theme_has_a_direction() {
        for theme in Theme::ALL {
            assert!(!theme.direction().trim().is_empty(), "{theme:?} is empty");
        }
        // `Unnamed` is the fallback and deliberately has no keywords.
        for theme in Theme::ALL.iter().filter(|theme| **theme != Theme::Unnamed) {
            assert!(!keywords(*theme).is_empty(), "{theme:?} has no keywords");
        }
    }
}
