//! The local journal of prayers.
//!
//! SQLite, because the journal has to survive restarts and answer questions
//! ("how many prayers today, how many tokens, which network carried them")
//! without loading everything into memory. The table is capped at
//! [`MAX_ROWS`], so a year of continuous prayer stays a few megabytes on disk.
//!
//! Failed calls are recorded too: they never count against a confessional
//! entry, but they are exactly what the user needs to see when a network starts
//! refusing requests.

use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use cyberclergy_llm::ProviderId;
use rusqlite::{params, Connection};
use serde::Serialize;

pub const HISTORY_FILE: &str = "history.sqlite";
/// Rows kept on disk. Older prayers are pruned on insert.
pub const MAX_ROWS: i64 = 5000;
/// How often the insert path prunes, so it is not a delete per prayer.
const PRUNE_EVERY: u32 = 50;
const SCHEMA_VERSION: i64 = 1;

/// Where a prayer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrayerSource {
    /// The ordinary rotation of a cleric.
    Schedule,
    /// A confessional entry being prayed over.
    Intercession,
    /// A prayer from the tradition's own canon while the networks rest.
    Quiet,
}

impl PrayerSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PrayerSource::Schedule => "schedule",
            PrayerSource::Intercession => "intercession",
            PrayerSource::Quiet => "quiet",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "intercession" => PrayerSource::Intercession,
            "quiet" => PrayerSource::Quiet,
            _ => PrayerSource::Schedule,
        }
    }
}

/// One row of the journal.
#[derive(Debug, Clone, Serialize)]
pub struct PrayerRecord {
    /// Unix timestamp in milliseconds.
    pub created_at: i64,
    pub religion: String,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub cleric: usize,
    pub source: PrayerSource,
    pub intercession_id: Option<String>,
    pub theme: Option<String>,
    pub ordinal: Option<u32>,
    pub target: Option<u32>,
    pub text: Option<String>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub elapsed_ms: u64,
    pub ok: bool,
    pub error: Option<String>,
}

impl PrayerRecord {
    /// A failed attempt: no text, no tokens, but a timestamp and a reason.
    pub fn failure(
        created_at: i64,
        religion: &str,
        cleric: usize,
        source: PrayerSource,
        elapsed_ms: u64,
        error: String,
    ) -> Self {
        Self {
            created_at,
            religion: religion.to_string(),
            provider: None,
            model: None,
            cleric,
            source,
            intercession_id: None,
            theme: None,
            ordinal: None,
            target: None,
            text: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            elapsed_ms,
            ok: false,
            error: Some(error),
        }
    }
}

/// Prayers served by one network, for the statistics block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderShare {
    pub provider: String,
    pub prayers: u64,
    pub tokens: u64,
}

/// Aggregate view rendered by the settings sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrayerStats {
    pub total: u64,
    pub today: u64,
    pub failures: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub average_ms: u64,
    pub providers: Vec<ProviderShare>,
}

/// SQLite-backed journal behind a mutex: every method takes `&self`, which keeps
/// it usable from both commands and the scheduler task.
#[derive(Debug)]
pub struct History {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    conn: Connection,
    inserts: u32,
}

impl History {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    format!("cannot create {}: {error}", parent.display())
                })?;
            }
        }
        let conn = Connection::open(path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        Self::from_connection(conn)
    }

    /// Journal that lives only in memory. Used when the data directory is not
    /// writable: losing the journal is annoying, losing the chapel is not.
    pub fn in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|error| format!("cannot open the in-memory journal: {error}"))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self, String> {
        migrate(&conn)?;
        Ok(Self {
            inner: Mutex::new(Inner { conn, inserts: 0 }),
        })
    }

    pub fn record(&self, record: &PrayerRecord) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "the prayer journal is unavailable".to_string())?;

        inner
            .conn
            .execute(
                "INSERT INTO prayers (
                    created_at, religion, provider, model, cleric, source,
                    intercession_id, theme, ordinal, target, text,
                    prompt_tokens, completion_tokens, elapsed_ms, ok, error
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    record.created_at,
                    record.religion,
                    record.provider.map(|provider| provider.as_str()),
                    record.model,
                    record.cleric as i64,
                    record.source.as_str(),
                    record.intercession_id,
                    record.theme,
                    record.ordinal.map(i64::from),
                    record.target.map(i64::from),
                    record.text,
                    i64::from(record.prompt_tokens),
                    i64::from(record.completion_tokens),
                    record.elapsed_ms as i64,
                    i64::from(record.ok),
                    record.error,
                ],
            )
            .map_err(|error| format!("cannot record a prayer: {error}"))?;

        inner.inserts = inner.inserts.wrapping_add(1);
        if inner.inserts % PRUNE_EVERY == 0 {
            prune(&inner.conn, MAX_ROWS)?;
        }
        Ok(())
    }

    /// Aggregates for the statistics block. `today_start` is a Unix timestamp in
    /// milliseconds, computed by the caller so the day boundary stays local.
    pub fn stats(&self, today_start: i64) -> Result<PrayerStats, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "the prayer journal is unavailable".to_string())?;

        let (total, prompt_tokens, completion_tokens, average_ms, failures): (
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = inner
            .conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(prompt_tokens), 0),
                        COALESCE(SUM(completion_tokens), 0),
                        CAST(ROUND(COALESCE(AVG(elapsed_ms), 0)) AS INTEGER),
                        COALESCE(SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END), 0)
                 FROM prayers",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(|error| format!("cannot read the prayer statistics: {error}"))?;

        let today: i64 = inner
            .conn
            .query_row(
                "SELECT COUNT(*) FROM prayers WHERE ok = 1 AND created_at >= ?1",
                params![today_start],
                |row| row.get(0),
            )
            .map_err(|error| format!("cannot count today's prayers: {error}"))?;

        let mut statement = inner
            .conn
            .prepare(
                "SELECT COALESCE(provider, 'quiet') AS source,
                        COUNT(*),
                        COALESCE(SUM(prompt_tokens + completion_tokens), 0)
                 FROM prayers
                 WHERE ok = 1
                 GROUP BY source
                 ORDER BY 2 DESC, 1 ASC",
            )
            .map_err(|error| format!("cannot prepare the provider query: {error}"))?;
        let providers = statement
            .query_map([], |row| {
                Ok(ProviderShare {
                    provider: row.get(0)?,
                    prayers: row.get::<_, i64>(1)?.max(0) as u64,
                    tokens: row.get::<_, i64>(2)?.max(0) as u64,
                })
            })
            .map_err(|error| format!("cannot read the provider breakdown: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot read the provider breakdown: {error}"))?;

        Ok(PrayerStats {
            total: total.max(0) as u64,
            today: today.max(0) as u64,
            failures: failures.max(0) as u64,
            prompt_tokens: prompt_tokens.max(0) as u64,
            completion_tokens: completion_tokens.max(0) as u64,
            average_ms: average_ms.max(0) as u64,
            providers,
        })
    }

    /// Newest first.
    pub fn recent(&self, limit: u32) -> Result<Vec<PrayerRecord>, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "the prayer journal is unavailable".to_string())?;
        let mut statement = inner
            .conn
            .prepare(
                "SELECT created_at, religion, provider, model, cleric, source,
                        intercession_id, theme, ordinal, target, text,
                        prompt_tokens, completion_tokens, elapsed_ms, ok, error
                 FROM prayers
                 ORDER BY id DESC
                 LIMIT ?1",
            )
            .map_err(|error| format!("cannot prepare the journal query: {error}"))?;

        let records = statement
            .query_map(params![i64::from(limit.clamp(1, 200))], |row| {
                let provider: Option<String> = row.get(2)?;
                Ok(PrayerRecord {
                    created_at: row.get(0)?,
                    religion: row.get(1)?,
                    provider: provider.as_deref().and_then(|value| ProviderId::from_str(value).ok()),
                    model: row.get(3)?,
                    cleric: row.get::<_, i64>(4)?.max(0) as usize,
                    source: PrayerSource::parse(&row.get::<_, String>(5)?),
                    intercession_id: row.get(6)?,
                    theme: row.get(7)?,
                    ordinal: row.get::<_, Option<i64>>(8)?.map(|value| value.max(0) as u32),
                    target: row.get::<_, Option<i64>>(9)?.map(|value| value.max(0) as u32),
                    text: row.get(10)?,
                    prompt_tokens: row.get::<_, i64>(11)?.max(0) as u32,
                    completion_tokens: row.get::<_, i64>(12)?.max(0) as u32,
                    elapsed_ms: row.get::<_, i64>(13)?.max(0) as u64,
                    ok: row.get::<_, i64>(14)? != 0,
                    error: row.get(15)?,
                })
            })
            .map_err(|error| format!("cannot read the journal: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot read the journal: {error}"))?;
        Ok(records)
    }
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS prayers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at INTEGER NOT NULL,
            religion TEXT NOT NULL,
            provider TEXT,
            model TEXT,
            cleric INTEGER NOT NULL,
            source TEXT NOT NULL,
            intercession_id TEXT,
            theme TEXT,
            ordinal INTEGER,
            target INTEGER,
            text TEXT,
            prompt_tokens INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            elapsed_ms INTEGER NOT NULL DEFAULT 0,
            ok INTEGER NOT NULL,
            error TEXT
        );
        CREATE INDEX IF NOT EXISTS prayers_created_at ON prayers(created_at);
        CREATE INDEX IF NOT EXISTS prayers_provider ON prayers(provider);",
    )
    .map_err(|error| format!("cannot prepare the journal: {error}"))?;

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| format!("cannot read the journal version: {error}"))?;
    if version != SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|error| format!("cannot set the journal version: {error}"))?;
    }
    Ok(())
}

fn prune(conn: &Connection, keep: i64) -> Result<(), String> {
    conn.execute(
        "DELETE FROM prayers WHERE id NOT IN (SELECT id FROM prayers ORDER BY id DESC LIMIT ?1)",
        params![keep.max(1)],
    )
    .map_err(|error| format!("cannot prune the journal: {error}"))?;
    Ok(())
}

/// Unix time in milliseconds, the unit every record and event uses.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Unix time in whole seconds, the unit the scheduler uses.
pub fn now_secs() -> i64 {
    now_millis() / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(created_at: i64, provider: ProviderId, tokens: u32) -> PrayerRecord {
        PrayerRecord {
            created_at,
            religion: "christianity".to_string(),
            provider: Some(provider),
            model: Some("test-model".to_string()),
            cleric: 2,
            source: PrayerSource::Schedule,
            intercession_id: None,
            theme: None,
            ordinal: None,
            target: None,
            text: Some("A quiet prayer.".to_string()),
            prompt_tokens: tokens,
            completion_tokens: tokens,
            elapsed_ms: 700,
            ok: true,
            error: None,
        }
    }

    #[test]
    fn records_are_ordered_newest_first() {
        let history = History::in_memory().expect("in-memory journal opens");
        history
            .record(&live(1_000, ProviderId::OpenRouter, 10))
            .expect("first record");
        history
            .record(&live(2_000, ProviderId::Groq, 20))
            .expect("second record");

        let recent = history.recent(10).expect("journal is readable");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].created_at, 2_000);
        assert_eq!(recent[0].provider, Some(ProviderId::Groq));
        assert_eq!(recent[1].provider, Some(ProviderId::OpenRouter));
    }

    #[test]
    fn statistics_count_tokens_and_failures_separately() {
        let history = History::in_memory().expect("in-memory journal opens");
        history
            .record(&live(1_000, ProviderId::OpenRouter, 10))
            .expect("success");
        history
            .record(&live(2_000, ProviderId::OpenRouter, 5))
            .expect("success");
        history
            .record(&PrayerRecord::failure(
                3_000,
                "christianity",
                4,
                PrayerSource::Schedule,
                120,
                "rate limited".to_string(),
            ))
            .expect("failure");

        let stats = history.stats(1_500).expect("statistics are readable");
        assert_eq!(stats.total, 3);
        assert_eq!(stats.today, 1, "only successes after the day boundary");
        assert_eq!(stats.failures, 1);
        assert_eq!(stats.prompt_tokens, 15);
        assert_eq!(stats.completion_tokens, 15);
        assert_eq!(stats.average_ms, 507);
        assert_eq!(
            stats.providers,
            vec![ProviderShare {
                provider: "openrouter".to_string(),
                prayers: 2,
                tokens: 30,
            }]
        );
    }

    #[test]
    fn quiet_prayers_report_as_their_own_source() {
        let history = History::in_memory().expect("in-memory journal opens");
        let mut record = live(1_000, ProviderId::Mistral, 0);
        record.provider = None;
        record.model = None;
        record.source = PrayerSource::Quiet;
        history.record(&record).expect("quiet record");

        let stats = history.stats(0).expect("statistics are readable");
        assert_eq!(stats.providers[0].provider, "quiet");
        assert_eq!(stats.providers[0].prayers, 1);
    }

    #[test]
    fn pruning_keeps_the_newest_rows() {
        let conn = Connection::open_in_memory().expect("connection opens");
        migrate(&conn).expect("schema is ready");
        for created_at in 0..5 {
            conn.execute(
                "INSERT INTO prayers (created_at, religion, cleric, source, ok)
                 VALUES (?1, 'islam', 0, 'quiet', 1)",
                params![created_at],
            )
            .expect("row is inserted");
        }

        prune(&conn, 2).expect("pruning works");
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM prayers", [], |row| row.get(0))
            .expect("count is readable");
        assert_eq!(remaining, 2);
    }
}
