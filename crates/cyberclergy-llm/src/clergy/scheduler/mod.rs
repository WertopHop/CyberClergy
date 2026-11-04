//! Cadence of continuous prayer.
//!
//! The chapel prays on a schedule, not on demand: every cleric has its own turn,
//! and the turns are staggered so the cloud never fires seven requests at once.
//! This module owns the *decision* half of that loop and nothing else — no
//! network, no clock of its own (the caller passes `now` in Unix seconds), no
//! Tauri. That keeps every rule below testable and lets the shell stay a thin
//! driver.
//!
//! Three pressures shape the rhythm:
//!
//! * **pace** — how often the person wants to hear a prayer ([`Pace`]);
//! * **health** — consecutive failures slow the rotation and, after
//!   [`OFFLINE_AFTER_FAILURES`], switch the chapel to its built-in canon while
//!   the networks recover ([`Tempo`], [`Health`]);
//! * **budget** — a hard ceiling of calls per hour ([`Meter`]), so free tiers are
//!   never drained by an enthusiastic cloud.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Length of the budget window: an hour of wall-clock time.
pub const BUDGET_WINDOW_SECS: i64 = 3600;
/// Calls allowed per window when the user configured nothing else.
pub const DEFAULT_BUDGET_PER_HOUR: u32 = 60;
/// Consecutive failures that make the chapel stop calling and pray its canon.
pub const OFFLINE_AFTER_FAILURES: u32 = 4;
/// While offline, how often a single probe is allowed through.
pub const OFFLINE_PROBE_SECS: i64 = 180;
/// Never wait out a provider cooldown longer than this.
pub const MAX_BACKOFF_SECS: i64 = 600;
/// First retry delay after a failure without a `Retry-After` header.
const BASE_BACKOFF_SECS: i64 = 30;
/// The prayer counter grows with the person's patience, not their free time:
/// three presses of the livelier pace still respect the profile floor.
pub const MAX_SLOTS: usize = 12;

/// How often the cloud prays, relative to the profile interval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    /// Fewer prayers, longer silences. Good for a background companion.
    Calm,
    /// The profile interval, as written by the tradition.
    #[default]
    Normal,
    /// Twice as often as normal, still clamped to the profile minimum.
    Lively,
}

impl Pace {
    pub const ALL: [Pace; 3] = [Pace::Calm, Pace::Normal, Pace::Lively];

    pub fn as_str(self) -> &'static str {
        match self {
            Pace::Calm => "calm",
            Pace::Normal => "normal",
            Pace::Lively => "lively",
        }
    }

    /// Accepts the persisted spelling and any case variant of it.
    pub fn parse(value: &str) -> Option<Self> {
        Pace::ALL
            .into_iter()
            .find(|pace| pace.as_str() == value.trim().to_ascii_lowercase())
    }

    /// Multiplier applied to `schedule.interval_seconds`.
    pub fn scale(self) -> f64 {
        match self {
            Pace::Calm => 1.5,
            Pace::Normal => 1.0,
            Pace::Lively => 0.5,
        }
    }
}

/// The mood of the chapel right now, derived from [`Health`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tempo {
    /// Everything is fine.
    Steady,
    /// A recent failure: slower and more careful.
    Cautious,
    /// Waiting out a cooldown or a spent budget. The canon keeps the cloud alive.
    Resting,
    /// Too many failures: no calls at all except an occasional probe.
    Offline,
}

impl Tempo {
    pub fn as_str(self) -> &'static str {
        match self {
            Tempo::Steady => "steady",
            Tempo::Cautious => "cautious",
            Tempo::Resting => "resting",
            Tempo::Offline => "offline",
        }
    }

    /// True when the chapel may compose a prayer through a provider.
    pub fn is_live(self) -> bool {
        matches!(self, Tempo::Steady | Tempo::Cautious)
    }
}

/// Request budget as the interface shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meter {
    pub done: u32,
    pub limit: u32,
    /// Seconds until the window slides and the counter starts over.
    pub resets_in_secs: u64,
}

impl Meter {
    pub fn exhausted(&self) -> bool {
        self.done >= self.limit
    }
}

/// When each cleric is due, derived from the tradition's schedule and the pace.
///
/// Slots are spread across the interval, so with seven clerics and a 15-minute
/// interval a prayer arrives roughly every two minutes instead of seven at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cadence {
    pace: Pace,
    interval_secs: u64,
    floor_secs: u64,
}

impl Cadence {
    /// `floor_secs` is the profile's `min_interval_seconds`: the livelier pace
    /// may shorten the interval but never below this.
    pub fn new(pace: Pace, interval_secs: u64, floor_secs: u64) -> Self {
        let floor = floor_secs.max(1);
        Self {
            pace,
            interval_secs: interval_secs.max(floor),
            floor_secs: floor,
        }
    }

    pub fn pace(&self) -> Pace {
        self.pace
    }

    pub fn interval_secs(&self) -> u64 {
        self.interval_secs
    }

    pub fn floor_secs(&self) -> u64 {
        self.floor_secs
    }

    /// Gap between two prayers of the *whole* cloud: `interval / clerics`.
    pub fn slot_gap_secs(&self, clerics: usize) -> u64 {
        let slots = clerics.clamp(1, MAX_SLOTS) as u64;
        (self.gap_secs() / slots).max(1)
    }

    /// Gap between two prayers of one cleric.
    pub fn gap_secs(&self) -> u64 {
        let scaled = (self.interval_secs as f64 * self.pace.scale()).round() as u64;
        scaled.max(self.floor_secs)
    }

    /// Cold-start time for one slot, staggered across the cloud.
    pub fn first_due(&self, slot: usize, clerics: usize, now: i64) -> i64 {
        let slots = clerics.clamp(1, MAX_SLOTS);
        let step = self.slot_gap_secs(slots);
        now + (slot.min(slots - 1) as u64 * step) as i64
    }

    /// Next turn of a cleric. `multiplier` comes from [`Health::gap_multiplier`].
    pub fn next_due(&self, now: i64, multiplier: u64) -> i64 {
        now + (self.gap_secs() * multiplier.max(1)) as i64
    }
}

/// How the shell configures [`Health`]; sourced from the environment once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub budget_per_hour: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            budget_per_hour: DEFAULT_BUDGET_PER_HOUR,
        }
    }
}

impl Policy {
    /// A zero budget would silence the chapel forever, so it is raised to one.
    pub fn with_budget(mut self, per_hour: u32) -> Self {
        self.budget_per_hour = per_hour.max(1);
        self
    }
}

/// Failure counting, cooldowns and the request budget.
///
/// The state is small on purpose: the shell rebuilds it on startup, because a
/// cooldown that survived a restart would be a lie — the provider's window may
/// already have slid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    policy: Policy,
    failures: u32,
    cooldown_until: Option<i64>,
    probe_at: Option<i64>,
    budget_started_at: i64,
    budget_used: u32,
    last_success_at: Option<i64>,
}

impl Health {
    pub fn new(policy: Policy, now: i64) -> Self {
        Self {
            policy,
            failures: 0,
            cooldown_until: None,
            probe_at: None,
            budget_started_at: now,
            budget_used: 0,
            last_success_at: None,
        }
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn last_success_at(&self) -> Option<i64> {
        self.last_success_at
    }

    /// Where the chapel stands right now. Offline wins over the other states so
    /// the interface can say plainly that the networks are gone.
    pub fn tempo(&self, now: i64) -> Tempo {
        if self.failures >= OFFLINE_AFTER_FAILURES {
            return Tempo::Offline;
        }
        if self.cooldown_remaining(now) > 0 {
            return Tempo::Resting;
        }
        if self.meter(now).exhausted() {
            return Tempo::Resting;
        }
        if self.failures > 0 {
            return Tempo::Cautious;
        }
        Tempo::Steady
    }

    /// Whether a network call may start now.
    pub fn can_call(&self, now: i64) -> bool {
        match self.tempo(now) {
            Tempo::Steady | Tempo::Cautious => true,
            Tempo::Resting => false,
            // While offline a single prayer every few minutes is enough to find
            // out whether the network came back.
            Tempo::Offline => self.probe_due(now),
        }
    }

    pub fn probe_due(&self, now: i64) -> bool {
        self.probe_at.is_none_or(|at| now >= at)
    }

    pub fn cooldown_remaining(&self, now: i64) -> u64 {
        self.cooldown_until
            .map(|until| (until - now).max(0) as u64)
            .unwrap_or(0)
    }

    pub fn meter(&self, now: i64) -> Meter {
        let elapsed = now - self.budget_started_at;
        // A window that has already slid reports a fresh meter even before the
        // next attempt rolls the counters, so the interface never shows an
        // exhausted budget that has in fact reopened.
        if !(0..BUDGET_WINDOW_SECS).contains(&elapsed) {
            return Meter {
                done: 0,
                limit: self.policy.budget_per_hour,
                resets_in_secs: BUDGET_WINDOW_SECS as u64,
            };
        }
        Meter {
            done: self.budget_used,
            limit: self.policy.budget_per_hour,
            resets_in_secs: (BUDGET_WINDOW_SECS - elapsed) as u64,
        }
    }

    /// How much longer a cleric waits between prayers after failures.
    pub fn gap_multiplier(&self) -> u64 {
        match self.failures {
            0 | 1 => 1,
            2 => 2,
            _ => 3,
        }
    }

    /// Charges one call against the hourly budget. Called before the request.
    pub fn note_attempt(&mut self, now: i64) {
        self.roll_window(now);
        self.budget_used = self.budget_used.saturating_add(1);
    }

    /// The provider answered: the chapel is healthy again.
    pub fn note_success(&mut self, now: i64) {
        self.failures = 0;
        self.cooldown_until = None;
        self.probe_at = None;
        self.last_success_at = Some(now);
    }

    /// Records a failed call and returns the moment the next call may start.
    ///
    /// `retry_after` is the provider's own hint (429 responses carry one); when
    /// it is absent the delay doubles with every consecutive failure, up to
    /// [`MAX_BACKOFF_SECS`], so a flaky endpoint is not hammered.
    pub fn note_failure(&mut self, now: i64, retry_after: Option<Duration>) -> i64 {
        self.failures = self.failures.saturating_add(1);
        let hinted = retry_after
            .map(|delay| delay.as_secs() as i64)
            .unwrap_or_else(|| BASE_BACKOFF_SECS << (self.failures.saturating_sub(1).min(5)));
        let delay = hinted.clamp(5, MAX_BACKOFF_SECS);
        let until = now + delay;
        self.cooldown_until = Some(until);
        if self.failures >= OFFLINE_AFTER_FAILURES {
            // The probe is measured from the end of the cooldown, not from the
            // failure itself, so a long 429 does not cause an early probe.
            self.probe_at = Some(until + OFFLINE_PROBE_SECS);
        }
        until
    }

    fn roll_window(&mut self, now: i64) {
        if now - self.budget_started_at >= BUDGET_WINDOW_SECS {
            self.budget_started_at = now;
            self.budget_used = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    fn health() -> Health {
        Health::new(Policy::default(), NOW)
    }

    #[test]
    fn pace_round_trips_through_its_string_form() {
        for pace in Pace::ALL {
            assert_eq!(Pace::parse(pace.as_str()), Some(pace));
        }
        assert_eq!(Pace::parse("  LIVELY "), Some(Pace::Lively));
        assert_eq!(Pace::parse("frantic"), None);
    }

    #[test]
    fn a_lively_pace_never_breaks_the_profile_floor() {
        let lively = Cadence::new(Pace::Lively, 900, 300);
        assert_eq!(lively.gap_secs(), 450);

        // A profile whose interval is already at the floor stays there.
        let pinned = Cadence::new(Pace::Lively, 300, 300);
        assert_eq!(pinned.gap_secs(), 300);

        let calm = Cadence::new(Pace::Calm, 900, 300);
        assert_eq!(calm.gap_secs(), 1350);
    }

    #[test]
    fn slots_are_staggered_across_the_cloud() {
        let cadence = Cadence::new(Pace::Normal, 900, 300);
        assert_eq!(cadence.slot_gap_secs(7), 128);
        assert_eq!(cadence.first_due(0, 7, NOW), NOW);
        assert_eq!(cadence.first_due(3, 7, NOW), NOW + 384);
        // The last slot is inside the interval, never beyond it.
        assert!(cadence.first_due(6, 7, NOW) < NOW + cadence.gap_secs() as i64);
    }

    #[test]
    fn a_single_cleric_still_gets_a_turn() {
        let cadence = Cadence::new(Pace::Normal, 900, 300);
        assert_eq!(cadence.slot_gap_secs(0), 900);
        assert_eq!(cadence.first_due(5, 0, NOW), NOW);
    }

    #[test]
    fn cooldown_blocks_calls_until_it_ends() {
        let mut health = health();
        let until = health.note_failure(NOW, Some(Duration::from_secs(42)));
        assert_eq!(until, NOW + 42);
        assert_eq!(health.tempo(NOW), Tempo::Resting);
        assert!(!health.can_call(NOW));
        assert_eq!(health.cooldown_remaining(NOW), 42);
        assert_eq!(health.tempo(NOW + 42), Tempo::Cautious);
        assert!(health.can_call(NOW + 42));
    }

    #[test]
    fn failures_back_off_exponentially_without_a_hint() {
        let mut health = health();
        assert_eq!(health.note_failure(NOW, None), NOW + 30);
        assert_eq!(health.note_failure(NOW, None), NOW + 60);
        assert_eq!(health.note_failure(NOW, None), NOW + 120);
    }

    #[test]
    fn a_huge_hint_is_capped() {
        let mut health = health();
        let until = health.note_failure(NOW, Some(Duration::from_secs(86_400)));
        assert_eq!(until, NOW + MAX_BACKOFF_SECS);
    }

    #[test]
    fn the_chapel_goes_offline_after_four_failures_and_probes_later() {
        let mut health = health();
        for _ in 0..OFFLINE_AFTER_FAILURES {
            health.note_failure(NOW, Some(Duration::from_secs(10)));
        }
        assert_eq!(health.tempo(NOW), Tempo::Offline);
        assert!(!health.can_call(NOW));

        let probe = NOW + 10 + OFFLINE_PROBE_SECS;
        assert!(!health.can_call(probe - 1));
        assert!(health.can_call(probe));
    }

    #[test]
    fn a_success_heals_everything() {
        let mut health = health();
        for _ in 0..OFFLINE_AFTER_FAILURES {
            health.note_failure(NOW, Some(Duration::from_secs(10)));
        }
        health.note_success(NOW + 60);
        assert_eq!(health.failures(), 0);
        assert_eq!(health.tempo(NOW + 60), Tempo::Steady);
        assert_eq!(health.last_success_at(), Some(NOW + 60));
    }

    #[test]
    fn failures_slow_the_rotation() {
        let mut health = health();
        assert_eq!(health.gap_multiplier(), 1);
        health.note_failure(NOW, None);
        assert_eq!(health.gap_multiplier(), 1);
        health.note_failure(NOW, None);
        assert_eq!(health.gap_multiplier(), 2);
        health.note_failure(NOW, None);
        assert_eq!(health.gap_multiplier(), 3);
        health.note_success(NOW);
        assert_eq!(health.gap_multiplier(), 1);
    }

    #[test]
    fn the_budget_stops_the_cloud_and_reopens_an_hour_later() {
        let mut health = Health::new(Policy::default().with_budget(2), NOW);
        health.note_attempt(NOW);
        assert!(!health.meter(NOW).exhausted());
        health.note_attempt(NOW + 10);
        assert!(health.meter(NOW + 10).exhausted());
        assert_eq!(health.tempo(NOW + 10), Tempo::Resting);
        assert!(!health.can_call(NOW + 10));
        assert_eq!(health.meter(NOW + 10).resets_in_secs, 3590);

        // A new window opens exactly one hour after the first call.
        assert_eq!(health.tempo(NOW + BUDGET_WINDOW_SECS), Tempo::Steady);
        assert_eq!(health.meter(NOW + BUDGET_WINDOW_SECS).done, 0);
    }

    #[test]
    fn a_zero_budget_still_allows_one_call() {
        let health = Health::new(Policy::default().with_budget(0), NOW);
        assert_eq!(health.meter(NOW).limit, 1);
    }
}
