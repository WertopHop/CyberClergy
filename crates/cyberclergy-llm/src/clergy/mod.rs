//! Religion profiles and the confessional queue.
//!
//! Profiles describe identity, style defaults, schedule and a tradition-specific
//! addendum to the shared system prompt. Built-ins are embedded with
//! `include_str!`, so a packaged application never depends on external resource
//! files; a user profile in the application data directory replaces the built-in
//! profile with the same id.
//!
//! The intercession module owns the entries a person writes down and the limited
//! number of prayers owed to each of them; the scheduler module owns the rhythm
//! the chapel prays to.

pub mod custom_religion;
pub mod intercession;
mod profile;
pub mod scheduler;

pub use custom_religion::{parse_answer, ReligionDraft, MAX_DESCRIPTION_CHARS, MAX_TITLE_CHARS};

pub use intercession::{
    Direction, Intercession, IntercessionQueue, IntercessionRequest, IntercessionStatus,
    IntercessionTask, QueueError, RecordOutcome, Severity, SubmitOutcome, Theme,
};
pub use profile::{
    Identity, OfflineCanon, ProfileCatalog, ProfileError, ReligionProfile, Schedule, Style,
    ADDENDUM_FILE, OFFLINE_FILE, PROFILE_FILE,
};
pub use scheduler::{Cadence, Health, Meter, Pace, Policy, Tempo};
