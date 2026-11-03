// Mirrors of the payloads the Rust commands return. Keeping them in one file
// makes a shape change on the backend a single place to update here.

export type ProviderId =
  | "openrouter"
  | "gemini"
  | "groq"
  | "mistral"
  | "cloudflare"
  | "nvidia";

export interface ProviderField {
  /** Environment-style variable name, for example `OPENROUTER_API_KEY`. */
  name: string;
  /** Whether a value is already known (from `.env` or the settings file). */
  set: boolean;
}

export interface ProviderView {
  id: ProviderId;
  model: string;
  configured: boolean;
  fields: ProviderField[];
}

export interface Preferences {
  language: string;
  theme: string;
  religion: string;
  clerics: number;
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

export interface Confession {
  id: string;
  text: string;
  createdAt: number;
}
