//! The continuous prayer loop.
//!
//! One background task, owned by Tauri, drives the whole chapel:
//!
//! * it wakes when a cleric's turn comes, when a confession needs praying, or
//!   when the settings change ([`tokio::sync::Notify`]);
//! * it runs up to [`MAX_IN_FLIGHT`] requests concurrently, because free models
//!   take 8–30 seconds and a strictly sequential loop would look dead;
//! * it records every attempt in the journal and publishes two events the
//!   interface listens to, [`EVENT_PRAYER`] and [`EVENT_STATUS`].
//!
//! The rules of the rhythm are in `cyberclergy_llm::clergy::scheduler`; this
//! module is the glue: state, network, journal and events. It deliberately owns
//! no timer of its own — it checks the clock, computes the next wait and sleeps
//! until something interesting can happen.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cyberclergy_llm::{
    Cadence, ChatResponse, IntercessionTask, LlmError, Meter, Policy, PrayerGenerator, PrayerSpec,
    DEFAULT_BUDGET_PER_HOUR,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::task::JoinSet;

use crate::history::{now_millis, now_secs, PrayerRecord, PrayerSource};
use crate::AppState;

pub const EVENT_PRAYER: &str = "chapel://prayer";
pub const EVENT_STATUS: &str = "chapel://status";

/// Free tiers dislike bursts; two prayers in flight keeps the cloud lively
/// without looking like an attack.
const MAX_IN_FLIGHT: usize = 2;
/// Upper bound for a single sleep. Notifications wake the loop earlier, so this
/// only matters when nothing else happens.
const IDLE_WAIT_SECS: u64 = 10;

/// Payload of [`EVENT_PRAYER`].
#[derive(Debug, Clone, Serialize)]
pub struct PrayerEvent {
    pub cleric: usize,
    pub text: String,
    pub source: PrayerSource,
    pub provider: Option<cyberclergy_llm::ProviderId>,
    pub model: Option<String>,
    pub theme: Option<String>,
    pub severity: Option<String>,
    pub ordinal: Option<u32>,
    pub target: Option<u32>,
    pub intercession_id: Option<String>,
    pub elapsed_ms: u64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Unix timestamp in milliseconds; the interface uses it to key bubbles.
    pub at: i64,
}

/// Payload of [`EVENT_STATUS`], also returned by the `scheduler_status` command.
#[derive(Debug, Clone, Serialize)]
pub struct ChapelStatus {
    /// `steady`, `cautious`, `resting` or `offline`.
    pub tempo: &'static str,
    pub pending: usize,
    pub fulfilled: usize,
    pub budget: Meter,
    pub cooldown_secs: u64,
    pub failures: u32,
    pub configured: bool,
    /// `keychain` or `file`.
    pub storage: &'static str,
    /// Most recent failure, if the chapel has not recovered since.
    pub error: Option<String>,
    /// Seconds until any cleric's next ordinary turn.
    pub next_in_secs: Option<u64>,
}

/// Reads the hourly call budget from the environment, falling back to the
/// documented default.
pub fn policy_from_env() -> Policy {
    let budget = std::env::var("CC_REQUESTS_PER_HOUR")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(DEFAULT_BUDGET_PER_HOUR);
    Policy::default().with_budget(budget)
}

/// Starts the loop. The returned handle is kept inside the application state, so
/// commands can wake the scheduler after a change.
pub fn spawn(app: AppHandle, shared: Arc<Mutex<AppState>>) {
    tauri::async_runtime::spawn(async move { run(app, shared).await });
}

async fn run(app: AppHandle, shared: Arc<Mutex<AppState>>) {
    let wake = shared
        .lock()
        .map(|state| state.wake.clone())
        .unwrap_or_else(|_| Arc::new(Notify::new()));

    let mut set: JoinSet<Finished> = JoinSet::new();
    let mut busy: BTreeSet<usize> = BTreeSet::new();

    loop {
        // Collect everything that finished since the last pass.
        while let Some(joined) = set.try_join_next() {
            if let Ok(finished) = joined {
                busy.remove(&finished.job.cleric);
                complete(&app, &shared, finished);
            }
        }
        // A panicking job loses its slot; once nothing is in flight, no slot can
        // still be busy.
        if set.is_empty() {
            busy.clear();
        }

        while set.len() < MAX_IN_FLIGHT {
            let Some(job) = plan(&shared, &busy) else {
                break;
            };
            busy.insert(job.cleric);
            set.spawn(run_job(job));
        }

        emit_status(&app, &shared, now_secs());

        tokio::select! {
            _ = wake.notified() => {}
            joined = set.join_next(), if !set.is_empty() => {
                if let Some(Ok(finished)) = joined {
                    busy.remove(&finished.job.cleric);
                    complete(&app, &shared, finished);
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(IDLE_WAIT_SECS)) => {}
        }
    }
}

struct Job {
    cleric: usize,
    task: Option<IntercessionTask>,
    kind: Kind,
    religion: String,
    source: PrayerSource,
}

enum Kind {
    Live {
        generator: Arc<PrayerGenerator>,
        spec: PrayerSpec,
    },
    Quiet {
        text: String,
    },
}

struct Finished {
    job: Job,
    elapsed_ms: u64,
    outcome: Outcome,
}

enum Outcome {
    Live(Result<ChatResponse, LlmError>),
    Quiet,
}

async fn run_job(job: Job) -> Finished {
    let started = Instant::now();
    let outcome = match &job.kind {
        Kind::Live { generator, spec } => Outcome::Live(generator.generate(spec).await),
        Kind::Quiet { .. } => Outcome::Quiet,
    };
    Finished {
        job,
        elapsed_ms: started.elapsed().as_millis() as u64,
        outcome,
    }
}

/// Decides the next prayer. Returns `None` when no cleric is free or nothing is
/// due, which is the common case between turns.
fn plan(shared: &Arc<Mutex<AppState>>, busy: &BTreeSet<usize>) -> Option<Job> {
    let now = now_secs();
    let mut state = shared.lock().ok()?;
    // Without a single configured network there is nothing to pray *for* yet:
    // the empty-state card explains what to do instead of faking a chapel.
    state.generator.as_ref()?;

    let preferences = state.preferences();
    let profile = state.profiles.get(&preferences.religion)?.clone();
    state.ensure_schedule(now);

    let busy_slots: Vec<usize> = busy.iter().copied().collect();
    let intercession = state.confessions.queue().next_task(&busy_slots);
    let (cleric, task) = match intercession {
        Some(task) => (task.cleric, Some(task)),
        None => (next_due_slot(&state.schedule, busy, now)?, None),
    };
    let cadence = Cadence::new(
        preferences.pace(),
        profile.schedule.interval_seconds,
        profile.schedule.min_interval_seconds,
    );
    let multiplier = state.health.gap_multiplier();
    if let Some(due) = state.schedule.get_mut(cleric) {
        *due = cadence.next_due(now, multiplier);
    }

    let can_call = state.health.can_call(now);
    let language = state.prayer_language();
    let kind = match (can_call, state.generator.clone()) {
        (true, Some(generator)) => {
            state.health.note_attempt(now);
            let mut spec = profile
                .short_prayer_spec()
                .with_language(language.to_string());
            if let Some(task) = &task {
                spec = spec.with_intent(task.direction.clone());
            }
            Kind::Live { generator, spec }
        }
        _ => {
            // Quiet hours: the tradition's own canon, rotated so it does not
            // repeat itself.
            let rotation = state.quiet_rotation;
            state.quiet_rotation = rotation.wrapping_add(1);
            let text = profile
                .quiet_prayer(&preferences.language, rotation)?
                .to_string();
            Kind::Quiet { text }
        }
    };

    let source = match (&kind, &task) {
        (Kind::Quiet { .. }, _) => PrayerSource::Quiet,
        (Kind::Live { .. }, Some(_)) => PrayerSource::Intercession,
        (Kind::Live { .. }, None) => PrayerSource::Schedule,
    };

    Some(Job {
        cleric,
        task,
        kind,
        religion: preferences.religion.clone(),
        source,
    })
}

/// Earliest slot whose turn has come and which is not already praying.
fn next_due_slot(schedule: &[i64], busy: &BTreeSet<usize>, now: i64) -> Option<usize> {
    schedule
        .iter()
        .enumerate()
        .filter(|(slot, due)| **due <= now && !busy.contains(slot))
        .min_by_key(|(_, due)| **due)
        .map(|(slot, _)| slot)
}

fn complete(app: &AppHandle, shared: &Arc<Mutex<AppState>>, finished: Finished) {
    let now = now_millis();
    let seconds = now / 1000;
    let Ok(mut state) = shared.lock() else {
        return;
    };

    let mut prayer: Option<PrayerEvent> = None;
    let record: Option<PrayerRecord>;
    let task = finished.job.task.clone();
    let mut counted = false;

    match finished.outcome {
        Outcome::Quiet => {
            let text = match &finished.job.kind {
                Kind::Quiet { text } => text.clone(),
                Kind::Live { .. } => return,
            };
            record = Some(PrayerRecord {
                created_at: now,
                religion: finished.job.religion.clone(),
                provider: None,
                model: None,
                cleric: finished.job.cleric,
                source: PrayerSource::Quiet,
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target: task.as_ref().map(|task| task.target),
                text: Some(text.clone()),
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: finished.elapsed_ms,
                ok: true,
                error: None,
            });
            counted = task.is_some();
            prayer = Some(PrayerEvent {
                cleric: finished.job.cleric,
                text,
                source: PrayerSource::Quiet,
                provider: None,
                model: None,
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                severity: task.as_ref().map(|task| task.severity.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target: task.as_ref().map(|task| task.target),
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                elapsed_ms: finished.elapsed_ms,
                prompt_tokens: 0,
                completion_tokens: 0,
                at: now,
            });
        }
        Outcome::Live(Ok(response)) => {
            let elapsed_ms = finished.elapsed_ms;
            let usage = response.usage.unwrap_or_default();
            let prompt_tokens = usage.prompt_tokens.unwrap_or(0);
            let completion_tokens = usage.completion_tokens.unwrap_or(0);
            state.health.note_success(seconds);
            state.last_error = None;
            counted = task.is_some();

            record = Some(PrayerRecord {
                created_at: now,
                religion: finished.job.religion.clone(),
                provider: Some(response.provider),
                model: Some(response.model.clone()),
                cleric: finished.job.cleric,
                source: finished.job.source,
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target: task.as_ref().map(|task| task.target),
                text: Some(response.text.clone()),
                prompt_tokens,
                completion_tokens,
                elapsed_ms,
                ok: true,
                error: None,
            });
            prayer = Some(PrayerEvent {
                cleric: finished.job.cleric,
                text: response.text,
                source: finished.job.source,
                provider: Some(response.provider),
                model: Some(response.model),
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                severity: task.as_ref().map(|task| task.severity.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target: task.as_ref().map(|task| task.target),
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                elapsed_ms,
                prompt_tokens,
                completion_tokens,
                at: now,
            });
        }
        Outcome::Live(Err(error)) => {
            state.health.note_failure(seconds, error.retry_after());
            let message = error.to_string();
            state.last_error = Some(message.clone());
            record = Some(PrayerRecord::failure(
                now,
                &finished.job.religion,
                finished.job.cleric,
                finished.job.source,
                finished.elapsed_ms,
                message,
            ));
        }
    }

    // A failed call never consumes the person's quota: only a prayer that was
    // actually composed counts.
    if counted {
        if let Some(task) = &task {
            if state.confessions.queue_mut().record_prayer(&task.id, now).is_some() {
                if let Err(error) = state.confessions.persist() {
                    eprintln!("cannot persist the confessional counter: {error}");
                }
            }
        }
    }

    if let Some(record) = record {
        if let Err(error) = state.history.record(&record) {
            eprintln!("cannot record a prayer: {error}");
        }
    }

    #[cfg(debug_assertions)]
    if let Some(event) = &prayer {
        eprintln!(
            "cyberclergy: {} prayer from cleric {} via {}{} in {} ms",
            event.source.as_str(),
            event.cleric,
            event
                .provider
                .map(|provider| provider.as_str())
                .unwrap_or("canon"),
            match (event.ordinal, event.target) {
                (Some(ordinal), Some(target)) => format!(" ({ordinal}/{target})"),
                _ => String::new(),
            },
            event.elapsed_ms
        );
    }

    let status = status_of(&state, seconds);
    drop(state);

    if let Some(prayer) = prayer {
        if let Err(error) = app.emit(EVENT_PRAYER, &prayer) {
            eprintln!("cannot emit a prayer: {error}");
        }
    }
    if let Err(error) = app.emit(EVENT_STATUS, &status) {
        eprintln!("cannot emit the chapel status: {error}");
    }
}

fn emit_status(app: &AppHandle, shared: &Arc<Mutex<AppState>>, now: i64) {
    let Ok(state) = shared.lock() else {
        return;
    };
    let status = status_of(&state, now);
    drop(state);
    if let Err(error) = app.emit(EVENT_STATUS, &status) {
        eprintln!("cannot emit the chapel status: {error}");
    }
}

/// Snapshot the interface renders. Shared by the event and the command.
pub fn status_of(state: &AppState, now: i64) -> ChapelStatus {
    let queue = state.confessions.queue();
    let fulfilled = queue
        .entries()
        .iter()
        .filter(|entry| !entry.is_active())
        .count();
    let next_in_secs = state
        .schedule
        .iter()
        .filter(|due| **due > now)
        .min()
        .map(|due| (*due - now) as u64);

    ChapelStatus {
        tempo: state.health.tempo(now).as_str(),
        pending: queue.active_count(),
        fulfilled,
        budget: state.health.meter(now),
        cooldown_secs: state.health.cooldown_remaining(now),
        failures: state.health.failures(),
        configured: state.generator.is_some(),
        storage: state.secrets.backend().as_str(),
        error: state.last_error.clone(),
        next_in_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_slots_ignore_busy_clerics_and_the_future() {
        let schedule = [10, 20, 5];
        assert_eq!(next_due_slot(&schedule, &BTreeSet::new(), 25), Some(2));
        assert_eq!(
            next_due_slot(&schedule, &BTreeSet::from([2]), 25),
            Some(0)
        );
        assert_eq!(next_due_slot(&schedule, &BTreeSet::new(), 0), None);
    }
}
