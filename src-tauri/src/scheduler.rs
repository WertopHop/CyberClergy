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

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cyberclergy_llm::{
    split_prayers, Cadence, ChatResponse, IntercessionTask, LlmError, Meter, Policy,
    PrayerGenerator, PrayerSpec, ProviderId, ReligionProfile, DEFAULT_BUDGET_PER_HOUR,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::task::JoinSet;

use crate::history::{now_millis, now_secs, PrayerRecord, PrayerSource};
use crate::prefs::Preferences;
use crate::AppState;

/// The rhythm in force: the person's own interval when they set one, otherwise
/// the tradition's interval modulated by the older pace setting.
pub fn cadence_for(
    preferences: &Preferences,
    profile: &ReligionProfile,
    clerics: usize,
) -> Cadence {
    match preferences.interval_secs {
        Some(secs) => Cadence::every(secs, clerics),
        None => Cadence::new(
            preferences.pace(),
            profile.schedule.interval_seconds,
            profile.schedule.min_interval_seconds,
        ),
    }
}

pub const EVENT_PRAYER: &str = "chapel://prayer";
pub const EVENT_STATUS: &str = "chapel://status";

/// Free tiers dislike bursts; two prayers in flight keeps the cloud lively
/// without looking like an attack.
const MAX_IN_FLIGHT: usize = 2;
/// Prayer turns are due on whole seconds, so the loop never sleeps longer than
/// this: a turn starts on the second it was scheduled for, not up to ten
/// seconds later. Notifications wake it earlier.
const MAX_WAIT_MS: u64 = 2_000;
/// And never spins: below this there is nothing to do anyway.
const MIN_WAIT_MS: u64 = 100;
/// Shortest gap between two planning passes, so a turn that finishes instantly
/// cannot outrun the clock.
const PLAN_FLOOR_MS: u64 = 500;
/// Prayers kept per slot from one batch. Beyond this the leftovers would go
/// stale before their turn came.
const MAX_BUFFERED: usize = 4;
/// Characters the interface types per second. A prayer counts as finished when
/// its last letter lands, so the chosen rhythm starts counting there. This is
/// the single source of that number: the status snapshot carries it to the
/// interface, which must type at exactly this speed.
pub const REVEAL_CHARS_PER_SECOND: u64 = 10;
/// Weight of a fresh measurement in the running average of reply time.
const LATENCY_WEIGHT: f64 = 0.35;
/// Before the first reply is measured: free models usually answer within a few
/// seconds, so the first request already starts slightly early.
pub(crate) const DEFAULT_LATENCY_MS: u64 = 6_000;

/// One prayer a request already paid for.
#[derive(Debug, Clone)]
pub struct BufferedPrayer {
    pub text: String,
    pub provider: Option<cyberclergy_llm::ProviderId>,
    pub model: Option<String>,
}

#[derive(Debug)]
struct SlotBuffer {
    key: String,
    texts: VecDeque<BufferedPrayer>,
}

/// Leftovers of the last batch, one queue per cleric.
///
/// A batch is composed for one intention, religion and language; a leftover is
/// only spent on a turn that asks for exactly the same, so a prayer written for
/// one confession is never handed to another.
#[derive(Debug, Default)]
pub struct PrayerBuffers {
    slots: BTreeMap<usize, SlotBuffer>,
}

impl PrayerBuffers {
    pub fn take(&mut self, slot: usize, key: &str) -> Option<BufferedPrayer> {
        let buffer = self.slots.get_mut(&slot)?;
        if buffer.key != key {
            // The intention, religion or language moved on: drop, never
            // misapply.
            self.slots.remove(&slot);
            return None;
        }
        let next = buffer.texts.pop_front();
        if buffer.texts.is_empty() {
            self.slots.remove(&slot);
        }
        next
    }

    pub fn store(&mut self, slot: usize, key: &str, prayers: Vec<BufferedPrayer>) {
        let texts: VecDeque<BufferedPrayer> = prayers.into_iter().take(MAX_BUFFERED).collect();
        if texts.is_empty() {
            self.slots.remove(&slot);
            return;
        }
        self.slots.insert(
            slot,
            SlotBuffer {
                key: key.to_string(),
                texts,
            },
        );
    }

    pub fn clear_slot(&mut self, slot: usize) {
        self.slots.remove(&slot);
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// Prayers waiting for their turn, for the status snapshot.
    pub fn len(&self) -> usize {
        self.slots.values().map(|buffer| buffer.texts.len()).sum()
    }
}

/// What a request was composed for. It travels with the job so the buffer can
/// refuse to spend a prayer on a different question.
fn prayer_key(religion: &str, language: &str, intent: Option<&str>) -> String {
    format!("{religion}|{language}|{}", intent.unwrap_or(""))
}

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
    /// Network an entry is pinned to but which no robot on the cloud runs.
    pub waiting_for: Option<cyberclergy_llm::ProviderId>,
    /// Prayers a batch already paid for, waiting for their turn.
    pub buffered: usize,
    /// Characters per second the interface types. Reported here so the typing
    /// speed exists in exactly one place.
    pub reveal_cps: u64,
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
    // A turn that costs nothing — the canon, a prayer from a batch — finishes
    // in a blink. This keeps such a run from turning into a busy spin.
    let mut last_plan: Option<Instant> = None;

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

        let floor = Duration::from_millis(PLAN_FLOOR_MS);
        if last_plan.is_none_or(|started| started.elapsed() >= floor) {
            let mut planned = false;
            while set.len() < MAX_IN_FLIGHT {
                let Some(job) = plan(&shared, &busy) else {
                    break;
                };
                busy.insert(job.cleric);
                planned = true;
                set.spawn(run_job(job));
            }
            if planned {
                last_plan = Some(Instant::now());
            }
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
            _ = tokio::time::sleep(wait_until_next_due(&shared)) => {}
        }
    }
}

/// How long the loop may sleep: until the nearest due turn, so a prayer starts
/// on its second, and never longer than [`MAX_WAIT_MS`] so the status snapshot
/// stays fresh.
fn wait_until_next_due(shared: &Arc<Mutex<AppState>>) -> Duration {
    let now = now_secs();
    let upcoming = shared.lock().ok().and_then(|state| {
        state
            .schedule
            .iter()
            .filter(|due| **due > now)
            .min()
            .map(|due| (due - now) as u64 * 1000)
    });
    Duration::from_millis(
        upcoming
            .unwrap_or(MAX_WAIT_MS)
            .clamp(MIN_WAIT_MS, MAX_WAIT_MS),
    )
}

struct Job {
    cleric: usize,
    task: Option<IntercessionTask>,
    kind: Kind,
    religion: String,
    source: PrayerSource,
    /// See [`prayer_key`].
    key: String,
}

enum Kind {
    Live {
        generator: Arc<PrayerGenerator>,
        spec: PrayerSpec,
        /// Network of the slot; `None` falls back to the ordinary rotation.
        provider: Option<cyberclergy_llm::ProviderId>,
    },
    /// A prayer from an earlier batch: no request, no tokens, no budget.
    Replay(BufferedPrayer),
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
    Replay,
    Quiet,
}

async fn run_job(job: Job) -> Finished {
    let started = Instant::now();
    let outcome = match &job.kind {
        Kind::Live {
            generator,
            spec,
            provider,
        } => Outcome::Live(generator.generate_for(*provider, spec).await),
        Kind::Replay(_) => Outcome::Replay,
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

    let roster = state.roster();
    // An emptied cloud really is empty: entries keep their counters and wait
    // until the person adds a robot back.
    if roster.is_empty() {
        return None;
    }

    // A turn belongs to a slot's own clock, whichever prayer it carries. The
    // confessional queue used to borrow any cleric, so a quiet canon — which
    // costs nothing and finishes instantly — was replayed thousands of times a
    // minute while an entry waited.
    let unavailable = unavailable_slots(&state.schedule, busy, now);
    let intercession = state.confessions.queue().next_task(&unavailable);
    let (cleric, task) = match intercession {
        Some(task) => (task.cleric, Some(task)),
        None => (next_due_slot(&state.schedule, busy, now)?, None),
    };
    let cadence = cadence_for(&preferences, &profile, roster.len());
    let multiplier = state.health.gap_multiplier();
    if let Some(due) = state.schedule.get_mut(cleric) {
        *due = cadence.next_due(now, multiplier);
    }

    let language = state.prayer_language();
    let religion = preferences.religion.clone();
    let key = prayer_key(
        &religion,
        language,
        task.as_ref().map(|task| task.direction.as_str()),
    );
    let source = match &task {
        Some(_) => PrayerSource::Intercession,
        None => PrayerSource::Schedule,
    };

    // A prayer paid for by an earlier request costs nothing, so it is spent
    // before the budget is even consulted: batching also bridges a spent hour.
    if let Some(buffered) = state.buffers.take(cleric, &key) {
        return Some(Job {
            cleric,
            task,
            kind: Kind::Replay(buffered),
            religion,
            source,
            key,
        });
    }

    let provider = roster.get(cleric).copied();
    // A model running on this machine costs no quota, so it prays even when
    // the hourly budget is spent and is not counted against it.
    let local = provider.map(ProviderId::is_local).unwrap_or(false);
    let can_call = local || state.health.can_call(now);
    let kind = match (can_call, state.generator.clone()) {
        (true, Some(generator)) => {
            if !local {
                state.health.note_attempt(now);
            }
            let batch = state.batch(profile.batch);
            let mut spec = profile
                .short_prayer_spec()
                .with_language(language.to_string())
                .with_batch(batch);
            if let Some(task) = &task {
                spec = spec.with_intent(task.direction.clone());
            }
            Kind::Live {
                generator,
                spec,
                provider,
            }
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
        (Kind::Replay(_), _) => source,
    };

    Some(Job {
        cleric,
        task,
        kind,
        religion,
        source,
        key,
    })
}

/// Slots that may not take a turn now: already praying, or still waiting out
/// the pause counted from their last prayer.
fn unavailable_slots(schedule: &[i64], busy: &BTreeSet<usize>, now: i64) -> Vec<usize> {
    let mut slots: Vec<usize> = busy.iter().copied().collect();
    slots.extend(
        schedule
            .iter()
            .enumerate()
            .filter(|(_, due)| **due > now)
            .map(|(slot, _)| slot),
    );
    slots.sort_unstable();
    slots.dedup();
    slots
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
    // An endless entry has no target to report: `None` keeps the bubble from
    // printing "3 of 0".
    let target = task.as_ref().and_then(|task| {
        if task.endless {
            None
        } else {
            Some(task.target)
        }
    });

    match finished.outcome {
        Outcome::Quiet => {
            let text = match &finished.job.kind {
                Kind::Quiet { text } => text.clone(),
                _ => return,
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
                target,
                text: Some(text.clone()),
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: finished.elapsed_ms,
                prayers_per_request: 0,
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
                target,
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                elapsed_ms: finished.elapsed_ms,
                prompt_tokens: 0,
                completion_tokens: 0,
                at: now,
            });
        }
        Outcome::Replay => {
            let buffered = match &finished.job.kind {
                Kind::Replay(buffered) => buffered.clone(),
                _ => return,
            };
            // The request that produced this prayer is already in the journal,
            // so the replay row carries no tokens and no tokens budget.
            counted = task.is_some();
            record = Some(PrayerRecord {
                created_at: now,
                religion: finished.job.religion.clone(),
                provider: buffered.provider,
                model: buffered.model.clone(),
                cleric: finished.job.cleric,
                source: finished.job.source,
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target,
                text: Some(buffered.text.clone()),
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: 0,
                prayers_per_request: 0,
                ok: true,
                error: None,
            });
            prayer = Some(PrayerEvent {
                cleric: finished.job.cleric,
                text: buffered.text,
                source: finished.job.source,
                provider: buffered.provider,
                model: buffered.model,
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                severity: task.as_ref().map(|task| task.severity.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target,
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                elapsed_ms: 0,
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
            state.latency_ms = smooth_latency(state.latency_ms, elapsed_ms);
            state.last_error = None;
            counted = task.is_some();

            // One response may hold several prayers: the first is spoken now,
            // the rest wait for this slot's next turns, so the rhythm stays the
            // same while the number of requests drops.
            let mut texts = split_prayers(&response.text);
            if texts.is_empty() {
                // The network answered but the text survived no filter; keep it
                // rather than swallowing the words.
                texts.push(response.text.trim().to_string());
            }
            let prayers_per_request = texts.len() as u32;
            let text = texts.remove(0);
            let leftovers = texts
                .into_iter()
                .map(|text| BufferedPrayer {
                    text,
                    provider: Some(response.provider),
                    model: Some(response.model.clone()),
                })
                .collect();
            state
                .buffers
                .store(finished.job.cleric, &finished.job.key, leftovers);

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
                target,
                text: Some(text.clone()),
                prompt_tokens,
                completion_tokens,
                elapsed_ms,
                prayers_per_request,
                ok: true,
                error: None,
            });
            prayer = Some(PrayerEvent {
                cleric: finished.job.cleric,
                text,
                source: finished.job.source,
                provider: Some(response.provider),
                model: Some(response.model),
                theme: task.as_ref().map(|task| task.theme.as_str().to_string()),
                severity: task.as_ref().map(|task| task.severity.as_str().to_string()),
                ordinal: task.as_ref().map(|task| task.ordinal),
                target,
                intercession_id: task.as_ref().map(|task| task.id.clone()),
                elapsed_ms,
                prompt_tokens,
                completion_tokens,
                at: now,
            });
        }
        Outcome::Live(Err(error)) => {
            state.health.note_failure(seconds, error.retry_after());
            // Whatever this slot still held was composed by a network that is
            // now refusing: drop it instead of praying stale words.
            state.buffers.clear_slot(finished.job.cleric);
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

    // A chosen rhythm means "pause after a prayer", not "a prayer every N": the
    // words take time to be typed out, so the next prayer is spoken `pause`
    // seconds after the last letter of this one. Waiting to *ask* until then
    // would add the whole network round-trip to the gap, which is how a five
    // second pause turned into half a minute. The request therefore starts
    // early — by the average time a reply has taken — so the words are already
    // in hand when the pause ends. A prayer that arrives early waits in the
    // interface, which shows it exactly on time.
    let pause = state.preferences().interval_secs;
    if let Some(pause) = pause {
        if let Some(event) = &prayer {
            let reveal = reveal_secs(&event.text);
            let lead = (state.latency_ms / 1000) as i64;
            if let Some(due) = state.schedule.get_mut(event.cleric) {
                *due = next_start(seconds, reveal, pause, lead);
            }
        }
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

/// Seconds the interface needs to reveal a prayer at its typing speed.
fn reveal_secs(text: &str) -> i64 {
    let characters = text.chars().count() as u64;
    characters.div_ceil(REVEAL_CHARS_PER_SECOND) as i64
}

/// When to start composing the next prayer.
///
/// The prayer on screen finishes typing at `now + reveal`; the next one is due
/// `pause` seconds later. A reply takes `lead` seconds on average, so the
/// request leaves `lead` seconds before that moment — during the typing when the
/// pause is short, just before it when the pause is long. It never starts
/// before `now`, and never waits longer than the whole turn.
fn next_start(now: i64, reveal: i64, pause: u64, lead: i64) -> i64 {
    let turn = reveal + pause as i64;
    (now + turn - lead.clamp(0, turn)).max(now)
}

/// Running average of how long a reply takes, in milliseconds.
fn smooth_latency(current: u64, sample: u64) -> u64 {
    (sample as f64 * LATENCY_WEIGHT + current as f64 * (1.0 - LATENCY_WEIGHT)).round() as u64
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
    // An entry pinned to a network the cloud no longer runs waits for that
    // robot to come back, and the interface says so instead of looking stuck.
    let waiting_for = queue
        .active()
        .filter_map(|entry| entry.provider)
        .find(|provider| !queue.roster().contains(provider));

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
        waiting_for,
        buffered: state.buffers.len(),
        reveal_cps: REVEAL_CHARS_PER_SECOND,
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

    #[test]
    fn a_slot_waiting_out_its_pause_is_unavailable() {
        let schedule = [10, 20, 5];
        // Slot 2 is due, slot 0 still waits, slot 1 has already prayed.
        assert_eq!(
            unavailable_slots(&schedule, &BTreeSet::new(), 8),
            vec![0, 1]
        );
        assert_eq!(
            unavailable_slots(&schedule, &BTreeSet::from([2]), 8),
            vec![0, 1, 2]
        );
        // Everything due and free: a quiet canon cannot borrow another slot.
        assert!(unavailable_slots(&[0, 0, 0], &BTreeSet::new(), 8).is_empty());
    }

    fn prayer(text: &str) -> BufferedPrayer {
        BufferedPrayer {
            text: text.to_string(),
            provider: Some(cyberclergy_llm::ProviderId::Groq),
            model: None,
        }
    }

    #[test]
    fn a_batch_is_spent_one_prayer_per_turn() {
        let mut buffers = PrayerBuffers::default();
        buffers.store(
            2,
            "christianity|English|",
            vec![prayer("second"), prayer("third")],
        );
        assert_eq!(buffers.len(), 2);
        assert_eq!(
            buffers.take(2, "christianity|English|").unwrap().text,
            "second"
        );
        assert_eq!(
            buffers.take(2, "christianity|English|").unwrap().text,
            "third"
        );
        assert!(buffers.take(2, "christianity|English|").is_none());
        assert_eq!(buffers.len(), 0);
    }

    #[test]
    fn leftovers_for_another_intention_are_dropped() {
        let mut buffers = PrayerBuffers::default();
        buffers.store(0, "islam|English|grief", vec![prayer("left over")]);
        assert!(
            buffers.take(0, "islam|English|guilt").is_none(),
            "a prayer for grief must not be spent on guilt"
        );
        assert_eq!(buffers.len(), 0, "the mismatched leftovers are gone");
    }

    #[test]
    fn a_failed_slot_forgets_its_batch() {
        let mut buffers = PrayerBuffers::default();
        buffers.store(1, "judaism|Russian|", vec![prayer("one"), prayer("two")]);
        buffers.clear_slot(1);
        assert!(buffers.take(1, "judaism|Russian|").is_none());
    }

    #[test]
    fn the_buffer_never_grows_without_bound() {
        let mut buffers = PrayerBuffers::default();
        let many: Vec<BufferedPrayer> = (0..MAX_BUFFERED + 3)
            .map(|index| prayer(&format!("prayer {index}")))
            .collect();
        buffers.store(0, "islam|English|", many);
        assert_eq!(buffers.len(), MAX_BUFFERED);
    }

    #[test]
    fn a_prayer_is_revealed_at_the_interface_typing_speed() {
        assert_eq!(reveal_secs("love"), 1);
        assert_eq!(reveal_secs(&"x".repeat(250)), 25);
        assert_eq!(reveal_secs(""), 0);
    }

    #[test]
    fn the_next_request_starts_before_the_pause_ends() {
        // No measurement yet: the request leaves when the pause ends.
        assert_eq!(next_start(100, 20, 5, 0), 125);
        // A short pause: the request overlaps the typing of the prayer on screen.
        assert_eq!(next_start(100, 20, 5, 8), 117);
        // A long pause: it still leaves only eight seconds before the turn.
        assert_eq!(next_start(100, 20, 120, 8), 232);
        // A hopelessly slow network never pulls the start before the reply.
        assert_eq!(next_start(100, 20, 5, 999), 100);
        // A silent canon is spoken while the previous words are still typing.
        assert_eq!(next_start(100, 0, 5, 8), 100);
    }

    #[test]
    fn the_measured_latency_moves_towards_the_latest_reply() {
        let once = smooth_latency(DEFAULT_LATENCY_MS, 20_000);
        assert!(once > DEFAULT_LATENCY_MS && once < 20_000);
        let settled = (0..40).fold(once, |current, _| smooth_latency(current, 2_000));
        assert!(settled < 2_500, "a fast network is learned: {settled}");
    }
}
