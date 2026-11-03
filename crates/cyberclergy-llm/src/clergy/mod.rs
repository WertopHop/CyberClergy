//! Religion profiles: identity, style defaults, schedule and a tradition-specific
//! addendum to the shared system prompt.
//!
//! Built-ins are embedded with `include_str!`, so a packaged application never
//! depends on external resource files. A user profile placed in the application
//! data directory replaces the built-in profile with the same id.

mod profile;

pub use profile::{
    Identity, ProfileCatalog, ProfileError, ReligionProfile, Schedule, Style, ADDENDUM_FILE,
    PROFILE_FILE,
};
