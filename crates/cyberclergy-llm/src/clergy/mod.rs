//! Religion profiles and the confessional queue.
//!
//! Profiles describe identity, style defaults, schedule and a tradition-specific
//! addendum to the shared system prompt. Built-ins are embedded with
//! `include_str!`, so a packaged application never depends on external resource
//! files; a user profile in the application data directory replaces the built-in
//! profile with the same id.
//!
//! The intercession module owns the entries a person writes down and the limited
//! number of prayers owed to each of them.

pub mod intercession;
mod profile;

pub use intercession::{
    Direction, Intercession, IntercessionQueue, IntercessionStatus, IntercessionTask, QueueError,
    RecordOutcome, Severity, SubmitOutcome, Theme,
};
pub use profile::{
    Identity, ProfileCatalog, ProfileError, ReligionProfile, Schedule, Style, ADDENDUM_FILE,
    PROFILE_FILE,
};
