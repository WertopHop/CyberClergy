//! Confessional queue: the entries a person writes down on their way to prayer.
//!
//! An entry is read locally into a *direction* (see [`direction`]), assigned to
//! exactly one cleric, and prayed for a limited number of times. The counter is
//! the whole point: a request is not a subscription, so after
//! [`Intercession::prayers_target`] prayers the entry is fulfilled and the
//! cleric returns to the ordinary rotation.
//!
//! The queue is deliberately independent of the network layer and of Tauri. The
//! scheduler only asks [`IntercessionQueue::next_task`] what to pray for and
//! reports back with [`IntercessionQueue::record_prayer`]. That keeps the state
//! machine testable and lets the whole queue be persisted as JSON.

mod direction;

pub use direction::{Direction, Severity, Theme};

use std::fmt;

use serde::{Deserialize, Serialize};

/// Longest entry the confessional accepts. Anything longer is a sign that the
/// person needs a conversation, not a longer queue item.
pub const MAX_TEXT_CHARS: usize = 1000;

pub const MIN_CLERICS: usize = 1;
pub const DEFAULT_CLERICS: usize = 7;

/// Hard ceiling for the prayer counter, whatever the severity.
pub const MAX_TARGET: u32 = 9;

/// How many times an entry is prayed for, by severity. A pressing entry gets
/// more rounds; a long entry gets one extra round because it carries more.
pub fn target_for(severity: Severity, characters: usize) -> u32 {
    let base = match severity {
        Severity::Urgent => 7,
        Severity::Steady => 5,
        Severity::Gentle => 3,
    };
    if characters > 400 {
        (base + 1).min(MAX_TARGET)
    } else {
        base
    }
}

/// Lifecycle of one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntercessionStatus {
    /// Waiting or in progress: the clergy still owe this entry prayers.
    Queued,
    /// The counter reached its target.
    Fulfilled,
    /// Dismissed by the person who wrote it.
    Cancelled,
}

/// One confessional entry, persisted verbatim on the device.
///
/// `text` never leaves the machine; only `direction` is handed to a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intercession {
    pub id: String,
    pub text: String,
    /// Unix timestamp in milliseconds.
    pub created_at: i64,
    pub theme: Theme,
    pub severity: Severity,
    /// English phrase substituted into `{{intent}}`.
    pub direction: String,
    /// Slot of the cleric who owns this entry from beginning to end.
    pub cleric: usize,
    pub prayers_done: u32,
    pub prayers_target: u32,
    pub status: IntercessionStatus,
    pub last_prayed_at: Option<i64>,
}

impl Intercession {
    pub fn is_active(&self) -> bool {
        self.status == IntercessionStatus::Queued
    }

    pub fn remaining(&self) -> u32 {
        self.prayers_target.saturating_sub(self.prayers_done)
    }
}

/// A single prayer the scheduler should compose next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntercessionTask {
    pub id: String,
    pub cleric: usize,
    pub direction: String,
    pub theme: Theme,
    pub severity: Severity,
    /// 1-based index of this prayer within the entry.
    pub ordinal: u32,
    pub target: u32,
}

/// Outcome of reporting a finished prayer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordOutcome {
    pub done: u32,
    pub target: u32,
    pub fulfilled: bool,
}

/// Result of submitting an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitOutcome {
    pub entry: Intercession,
    /// The same text is already waiting; nothing new was queued.
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    EmptyEntry,
    TooLong { limit: usize },
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueueError::EmptyEntry => f.write_str("the entry is empty"),
            QueueError::TooLong { limit } => {
                write!(f, "the entry is longer than {limit} characters")
            }
        }
    }
}

impl std::error::Error for QueueError {}

fn default_clerics() -> usize {
    DEFAULT_CLERICS
}

/// The queue itself: what is waiting, who owns it and how much is left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntercessionQueue {
    /// Oldest first, so the JSON stays readable and ordering is explicit.
    entries: Vec<Intercession>,
    #[serde(default = "default_clerics")]
    clerics: usize,
    next_serial: u64,
}

impl Default for IntercessionQueue {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            clerics: DEFAULT_CLERICS,
            next_serial: 0,
        }
    }
}

impl IntercessionQueue {
    pub fn new(clerics: usize) -> Self {
        Self {
            clerics: clerics.max(MIN_CLERICS),
            ..Self::default()
        }
    }

    pub fn clerics(&self) -> usize {
        self.clerics
    }

    /// Keeps assignments valid after the user changes how many clerics stand on
    /// the cloud. Entries keep their cleric unless that slot disappeared.
    pub fn set_clerics(&mut self, clerics: usize) {
        let count = clerics.max(MIN_CLERICS);
        if count == self.clerics {
            return;
        }
        self.clerics = count;
        for entry in self.entries.iter_mut().filter(|entry| entry.is_active()) {
            entry.cleric %= count;
        }
    }

    pub fn entries(&self) -> &[Intercession] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&Intercession> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn active(&self) -> impl Iterator<Item = &Intercession> {
        self.entries.iter().filter(|entry| entry.is_active())
    }

    pub fn active_count(&self) -> usize {
        self.active().count()
    }

    /// Number of active entries owned by one cleric.
    pub fn pending_count(&self, cleric: usize) -> usize {
        self.active().filter(|entry| entry.cleric == cleric).count()
    }

    /// Reads the text, assigns a cleric and queues the entry. Submitting the
    /// same text twice while the first copy is still pending returns the
    /// existing entry instead of piling up duplicates.
    pub fn submit(&mut self, text: &str, now: i64) -> Result<SubmitOutcome, QueueError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(QueueError::EmptyEntry);
        }
        let characters = trimmed.chars().count();
        if characters > MAX_TEXT_CHARS {
            return Err(QueueError::TooLong {
                limit: MAX_TEXT_CHARS,
            });
        }

        let normalized = normalize(trimmed);
        if let Some(existing) = self
            .entries
            .iter()
            .find(|entry| entry.is_active() && normalize(&entry.text) == normalized)
        {
            return Ok(SubmitOutcome {
                entry: existing.clone(),
                duplicate: true,
            });
        }

        let direction = Direction::derive(trimmed);
        let cleric = self.assign_cleric();
        let id = format!("ic-{now}-{}", self.next_serial);
        self.next_serial += 1;

        let entry = Intercession {
            id,
            text: trimmed.to_string(),
            created_at: now,
            theme: direction.theme,
            severity: direction.severity,
            direction: direction.intent,
            cleric,
            prayers_done: 0,
            prayers_target: target_for(direction.severity, characters),
            status: IntercessionStatus::Queued,
            last_prayed_at: None,
        };
        self.entries.push(entry.clone());

        Ok(SubmitOutcome {
            entry,
            duplicate: false,
        })
    }

    /// Dismisses an entry. The record is kept for the archive; its text can be
    /// cleared by the caller if privacy requires it.
    pub fn cancel(&mut self, id: &str) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.id == id && entry.is_active())
        else {
            return false;
        };
        entry.status = IntercessionStatus::Cancelled;
        true
    }

    /// The next prayer to compose, preferring pressing entries and leaving
    /// alone the clerics that are busy right now.
    pub fn next_task(&self, busy: &[usize]) -> Option<IntercessionTask> {
        self.entries
            .iter()
            .filter(|entry| entry.is_active() && !busy.contains(&entry.cleric))
            .min_by_key(|entry| (entry.severity, entry.created_at, entry.cleric))
            .map(|entry| IntercessionTask {
                id: entry.id.clone(),
                cleric: entry.cleric,
                direction: entry.direction.clone(),
                theme: entry.theme,
                severity: entry.severity,
                ordinal: entry.prayers_done + 1,
                target: entry.prayers_target,
            })
    }

    /// Counts one finished prayer. Only successful calls may be recorded, so a
    /// failed request never consumes the person's quota.
    pub fn record_prayer(&mut self, id: &str, now: i64) -> Option<RecordOutcome> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.id == id && entry.is_active())?;
        entry.prayers_done += 1;
        entry.last_prayed_at = Some(now);
        let fulfilled = entry.prayers_done >= entry.prayers_target;
        if fulfilled {
            entry.status = IntercessionStatus::Fulfilled;
        }
        Some(RecordOutcome {
            done: entry.prayers_done,
            target: entry.prayers_target,
            fulfilled,
        })
    }

    /// Deletes every finished entry, text included. This is the explicit
    /// "forget" action: the person asked for their words to be gone, not just
    /// archived.
    pub fn forget_finished(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(Intercession::is_active);
        before - self.entries.len()
    }

    /// Drops finished entries beyond `keep`, oldest first. Keeps the persisted
    /// file bounded without touching anything still owed.
    pub fn prune_finished(&mut self, keep: usize) -> usize {
        let finished_total = self.entries.iter().filter(|entry| !entry.is_active()).count();
        if finished_total <= keep {
            return 0;
        }
        let mut to_drop = finished_total - keep;
        let mut kept = Vec::with_capacity(self.entries.len());
        for entry in self.entries.drain(..) {
            if to_drop > 0 && !entry.is_active() {
                to_drop -= 1;
                continue;
            }
            kept.push(entry);
        }
        self.entries = kept;
        finished_total - keep
    }

    /// Picks the cleric with the fewest pending entries, rotating the starting
    /// point so two equally loaded clerics take turns.
    fn assign_cleric(&self) -> usize {
        let count = self.clerics.max(MIN_CLERICS);
        let start = (self.next_serial as usize) % count;
        (0..count)
            .map(|offset| (start + offset) % count)
            .min_by_key(|cleric| self.pending_count(*cleric))
            .unwrap_or(0)
    }
}

/// Case and whitespace insensitive form used for duplicate detection.
fn normalize(text: &str) -> String {
    text.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}
