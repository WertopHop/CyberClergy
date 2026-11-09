// Mirrors of the payloads the Rust commands and events use. Keeping them in one
// file makes a shape change on the backend a single place to update here.

export type ProviderId =
  | "openrouter"
  | "gemini"
  | "groq"
  | "mistral"
  | "cloudflare"
  | "nvidia"
  | "together"
  | "cerebras"
  | "deepinfra"
  | "github-models"
  | "ollama"
  | "custom";

export interface ProviderField {
  /** Environment-style variable name, for example `OPENROUTER_API_KEY`. */
  name: string;
  /** Whether a value is already known (from `.env` or the secret store). */
  set: boolean;
  /** True for keys and tokens, which are typed into a masked input. */
  secret: boolean;
}

export interface ProviderView {
  id: ProviderId;
  /** Model in effect: the person's choice, or the built-in default. */
  model: string;
  /** Built-in default, shown as the placeholder of the model editor. */
  default_model: string;
  /** Variable the model editor writes to, for example `GROQ_MODEL`. */
  model_var: string;
  configured: boolean;
  fields: ProviderField[];
}

/** Legacy rhythm spelling, still stored so an older settings file keeps its shape. */
export type Pace = "calm" | "normal" | "lively";

export interface Preferences {
  language: string;
  theme: string;
  religion: string;
  /**
   * Networks standing on the cloud, front slot first. `null` means the person
   * never chose: every configured network then stands up.
   */
  roster: ProviderId[] | null;
  pace: Pace;
  /**
   * One prayer of the whole cloud every this many seconds. `null` follows the
   * tradition's own interval.
   */
  interval_secs: number | null;
}

export interface PrayerOutcome {
  text: string;
  religion: string;
  provider: ProviderId;
  model: string;
  elapsed_ms: number;
}

export interface CheckOutcome {
  ok: boolean;
  detail: string;
  model: string;
  elapsed_ms: number;
}

/** What the "new tradition" form sends off to be shaped by a model. */
export interface ReligionDraft {
  title: string;
  description: string;
  /** Wish about the tone; empty lets the model choose one. */
  tone: string;
  /** Interface language of the person. */
  language: string;
}

export interface ReligionProfile {
  id: string;
  version: number;
  identity: {
    names: Record<string, string>;
    tradition: string;
    prayer_term: string;
    closing_formula: string | null;
  };
  style: {
    tone: string;
    length: string;
    short_length: string;
  };
  schedule: {
    interval_seconds: number;
    min_interval_seconds: number;
  };
  addendum?: string;
}

/** Mood of the chapel, derived from failures, cooldowns and the hourly budget. */
export type Tempo = "steady" | "cautious" | "resting" | "offline";

export interface Meter {
  done: number;
  limit: number;
  /** Seconds until the hourly window slides and the counter starts over. */
  resets_in_secs: number;
}

export interface ChapelStatus {
  tempo: Tempo;
  /** Active confessional entries still owed prayers. */
  pending: number;
  fulfilled: number;
  budget: Meter;
  cooldown_secs: number;
  failures: number;
  configured: boolean;
  storage: "keychain" | "file";
  error: string | null;
  next_in_secs: number | null;
  /** Network an entry is pinned to but which no robot on the cloud runs. */
  waiting_for: ProviderId | null;
  /** Prayers a batch already paid for, waiting for their turn. */
  buffered: number;
}

export type PrayerSource = "schedule" | "intercession" | "quiet";

export type ThemeId =
  | "guilt"
  | "grief"
  | "illness"
  | "gratitude"
  | "family"
  | "habit"
  | "work"
  | "anger"
  | "fear"
  | "loneliness"
  | "doubt"
  | "unnamed";

export type Severity = "urgent" | "steady" | "gentle";

/** Payload of `chapel://prayer`. */
export interface PrayerEvent {
  cleric: number;
  text: string;
  source: PrayerSource;
  provider: ProviderId | null;
  model: string | null;
  theme: ThemeId | null;
  severity: Severity | null;
  ordinal: number | null;
  target: number | null;
  intercession_id: string | null;
  elapsed_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  /** Unix timestamp in milliseconds. */
  at: number;
}

export type IntercessionStatus = "queued" | "fulfilled" | "cancelled";

/** One entry of the confessional queue, persisted in `confessions.json`. */
export interface ConfessionEntry {
  id: string;
  text: string;
  created_at: number;
  theme: ThemeId;
  severity: Severity;
  /** English direction handed to the prompt; the entry text never leaves. */
  direction: string;
  cleric: number;
  prayers_done: number;
  /** Zero for an endless entry: the counter never reaches a target. */
  prayers_target: number;
  /** Recites until dismissed instead of counting down to a target. */
  endless: boolean;
  /** Network the person pinned the entry to; `null` lets the queue choose. */
  provider: ProviderId | null;
  status: IntercessionStatus;
  last_prayed_at: number | null;
}

export interface ConfessionOutcome {
  entry: ConfessionEntry;
  duplicate: boolean;
}

export interface ProviderShare {
  provider: string;
  prayers: number;
  tokens: number;
}

export interface PrayerStats {
  total: number;
  /** Successful prayers in the last 24 hours. */
  today: number;
  /** Network calls that produced at least one prayer. */
  requests: number;
  failures: number;
  prompt_tokens: number;
  completion_tokens: number;
  average_ms: number;
  providers: ProviderShare[];
}

/** One row of the local prayer journal. */
export interface PrayerRecord {
  created_at: number;
  religion: string;
  provider: ProviderId | null;
  model: string | null;
  cleric: number;
  source: PrayerSource;
  intercession_id: string | null;
  theme: ThemeId | null;
  ordinal: number | null;
  target: number | null;
  text: string | null;
  prompt_tokens: number;
  completion_tokens: number;
  elapsed_ms: number;
  /** Prayers the call yielded; zero for prayers replayed from an earlier batch. */
  prayers_per_request: number;
  ok: boolean;
  error: string | null;
}
