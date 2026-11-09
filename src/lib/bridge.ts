// Single boundary between the interface and the Rust shell.
//
// Every command and every event is wrapped here so components never touch
// `invoke` or `listen` directly, and so the whole interface can be developed in
// a plain browser. When the Tauri internals are missing, the same functions
// answer with demo data: a small scheduler broadcasts prayers and status on the
// same cadence the Rust loop would.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { dictionaryFor } from "../i18n";
import {
  previewAdvance,
  previewCancel,
  previewForget,
  previewList,
  previewSubmit,
} from "./confessional";
import { DEMO_RELIGIONS, PREVIEW_KEYS, type PreviewKey } from "./demo";
import { cloudRoster, isProviderId } from "./providers";
import type {
  ChapelStatus,
  CheckOutcome,
  ConfessionEntry,
  ConfessionOutcome,
  PrayerEvent,
  PrayerRecord,
  PrayerStats,
  Preferences,
  ProviderField,
  ProviderId,
  ProviderView,
  ReligionDraft,
  ReligionProfile,
} from "./types";

export const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Event names emitted by the Rust scheduler; the preview uses the same ones. */
export const PRAYER_EVENT = "chapel://prayer";
export const STATUS_EVENT = "chapel://status";

export type Unlisten = () => void;

export const DEFAULT_PREFERENCES: Preferences = {
  language: "ru",
  theme: "night",
  religion: "christianity",
  // No roster yet: the chapel stands every configured network up.
  roster: null,
  pace: "normal",
  // The tradition keeps its own rhythm until the slider is touched.
  interval_secs: null,
};

const PREFS_KEY = "cyberclergy.preview.preferences";

const DEMO_PROVIDERS: ProviderView[] = [
  {
    id: "openrouter",
    model: "meta-llama/llama-3.3-70b-instruct:free",
    default_model: "meta-llama/llama-3.3-70b-instruct:free",
    model_var: "OPENROUTER_MODEL",
    configured: true,
    fields: [{ name: "OPENROUTER_API_KEY", set: true, secret: true }],
  },
  {
    id: "gemini",
    model: "gemini-2.5-flash",
    default_model: "gemini-2.5-flash",
    model_var: "GEMINI_MODEL",
    configured: false,
    fields: [{ name: "GEMINI_API_KEY", set: false, secret: true }],
  },
  {
    id: "groq",
    model: "llama-3.3-70b-versatile",
    default_model: "llama-3.3-70b-versatile",
    model_var: "GROQ_MODEL",
    configured: false,
    fields: [{ name: "GROQ_API_KEY", set: false, secret: true }],
  },
  {
    id: "mistral",
    model: "mistral-small-latest",
    default_model: "mistral-small-latest",
    model_var: "MISTRAL_MODEL",
    configured: false,
    fields: [{ name: "MISTRAL_API_KEY", set: false, secret: true }],
  },
  {
    id: "cloudflare",
    model: "@cf/meta/llama-3.1-8b-instruct",
    default_model: "@cf/meta/llama-3.1-8b-instruct",
    model_var: "CLOUDFLARE_MODEL",
    configured: false,
    fields: [
      { name: "CLOUDFLARE_API_TOKEN", set: false, secret: true },
      { name: "CLOUDFLARE_ACCOUNT_ID", set: false, secret: false },
    ],
  },
  {
    id: "nvidia",
    model: "meta/llama-3.3-70b-instruct",
    default_model: "meta/llama-3.3-70b-instruct",
    model_var: "NVIDIA_MODEL",
    configured: false,
    fields: [{ name: "NVIDIA_API_KEY", set: false, secret: true }],
  },
  {
    id: "together",
    model: "meta-llama/Llama-3.3-70B-Instruct-Turbo-Free",
    default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo-Free",
    model_var: "TOGETHER_MODEL",
    configured: false,
    fields: [{ name: "TOGETHER_API_KEY", set: false, secret: true }],
  },
  {
    id: "cerebras",
    model: "llama-3.3-70b",
    default_model: "llama-3.3-70b",
    model_var: "CEREBRAS_MODEL",
    configured: false,
    fields: [{ name: "CEREBRAS_API_KEY", set: false, secret: true }],
  },
  {
    id: "deepinfra",
    model: "meta-llama/Meta-Llama-3.1-8B-Instruct",
    default_model: "meta-llama/Meta-Llama-3.1-8B-Instruct",
    model_var: "DEEPINFRA_MODEL",
    configured: false,
    fields: [{ name: "DEEPINFRA_API_KEY", set: false, secret: true }],
  },
  {
    id: "github-models",
    model: "openai/gpt-4o-mini",
    default_model: "openai/gpt-4o-mini",
    model_var: "GITHUB_MODELS_MODEL",
    configured: false,
    fields: [{ name: "GITHUB_MODELS_TOKEN", set: false, secret: true }],
  },
  {
    id: "ollama",
    model: "",
    default_model: "",
    model_var: "OLLAMA_MODEL",
    configured: false,
    fields: [{ name: "OLLAMA_API_KEY", set: false, secret: true }],
  },
  {
    id: "custom",
    model: "",
    default_model: "",
    model_var: "CUSTOM_MODEL",
    configured: false,
    fields: [
      { name: "CUSTOM_API_KEY", set: false, secret: true },
      { name: "CUSTOM_BASE_URL", set: false, secret: false },
    ],
  },
];

let demoProviders: ProviderView[] = DEMO_PROVIDERS.map((provider) => ({
  ...provider,
  fields: provider.fields.map((field) => ({ ...field })),
}));

function wait(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, milliseconds);
  });
}

function normalizePreferences(value: Partial<Preferences>): Preferences {
  const pace =
    value.pace === "calm" || value.pace === "lively" ? value.pace : "normal";
  const roster = Array.isArray(value.roster)
    ? value.roster.filter(
        (id): id is ProviderId =>
          typeof id === "string" && isProviderId(id),
      )
    : null;
  const interval =
    typeof value.interval_secs === "number" && value.interval_secs > 0
      ? Math.round(value.interval_secs)
      : null;
  return {
    ...DEFAULT_PREFERENCES,
    ...value,
    pace,
    roster,
    interval_secs: interval,
  };
}

/** Robots the preview can show, mirroring the backend's roster resolution. */
async function previewRoster(): Promise<ProviderId[]> {
  const preferences = await loadPreferences();
  return cloudRoster(
    preferences.roster,
    demoProviders.filter((provider) => provider.configured).map((provider) => provider.id),
  );
}

function previewPrayer(religion: string, language: string): string {
  const key: PreviewKey =
    PREVIEW_KEYS[religion as keyof typeof PREVIEW_KEYS] ?? "previewChristianity";
  return dictionaryFor(language)[key];
}

/* ------------------------------------------------------------------ settings */

export async function loadPreferences(): Promise<Preferences> {
  if (isDesktop) {
    return invoke<Preferences>("load_preferences");
  }
  try {
    const raw = window.localStorage.getItem(PREFS_KEY);
    if (raw) {
      return normalizePreferences(JSON.parse(raw) as Partial<Preferences>);
    }
  } catch {
    // Fall through to the defaults.
  }
  return DEFAULT_PREFERENCES;
}

export async function savePreferences(
  preferences: Preferences,
): Promise<Preferences> {
  if (isDesktop) {
    return invoke<Preferences>("save_preferences", { preferences });
  }
  try {
    window.localStorage.setItem(PREFS_KEY, JSON.stringify(preferences));
  } catch {
    // Preview mode only; ignoring is fine.
  }
  return preferences;
}

export async function listProviders(): Promise<ProviderView[]> {
  if (isDesktop) {
    return invoke<ProviderView[]>("list_providers");
  }
  return demoProviders.map((provider) => ({
    ...provider,
    fields: provider.fields.map((field) => ({ ...field })),
  }));
}

/** Traditions created in the preview, so the form has somewhere to land. */
let demoReligions: ReligionProfile[] = DEMO_RELIGIONS.map((religion) => ({
  ...religion,
}));

export async function listReligions(): Promise<ReligionProfile[]> {
  if (isDesktop) {
    return invoke<ReligionProfile[]>("list_religions");
  }
  return demoReligions;
}

export async function createReligion(
  draft: ReligionDraft,
): Promise<ReligionProfile[]> {
  if (isDesktop) {
    return invoke<ReligionProfile[]>("create_religion", { draft });
  }
  await wait(900);
  const base =
    draft.title
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "tradition";
  const taken = new Set(demoReligions.map((religion) => religion.id));
  let id = base;
  for (let index = 2; taken.has(id); index += 1) {
    id = `${base}-${index}`;
  }
  const profile: ReligionProfile = {
    id,
    version: 1,
    identity: {
      names: { en: draft.title, ru: draft.title },
      tradition: draft.description.slice(0, 200),
      prayer_term: "prayer",
      closing_formula: null,
    },
    style: {
      tone: draft.tone || "reverent and warm",
      length: "one short paragraph",
      short_length: "one short paragraph",
    },
    schedule: { interval_seconds: 900, min_interval_seconds: 300 },
  };
  demoReligions = [...demoReligions, profile];
  return demoReligions;
}

/** Preview mirror of the shell's custom gateways: each filled one reveals the
 * next empty one, so any number of OpenAI-compatible servers can be added. */
function gatewayFields(slot: number): ProviderField[] {
  const names =
    slot === 1
      ? ["CUSTOM_BASE_URL", "CUSTOM_API_KEY"]
      : [`CUSTOM_${slot}_MODEL`, `CUSTOM_${slot}_BASE_URL`, `CUSTOM_${slot}_API_KEY`];
  return names.map((name) => ({
    name,
    set: false,
    secret: name.endsWith("API_KEY"),
  }));
}

function growGateways(
  entry: ProviderView,
  fields: ProviderField[],
): ProviderField[] {
  if (entry.id !== "custom" || fields.length === 0) {
    return fields;
  }
  const lastSlot = fields.reduce((highest, item) => {
    const match = /^CUSTOM_(\d+)_/.exec(item.name);
    return match ? Math.max(highest, Number(match[1])) : highest;
  }, 1);
  // The tail of the list is the newest gateway; once it holds a value, the
  // next one is appended.
  const newest = fields.slice(-3);
  if (!newest.some((item) => item.set) || lastSlot >= 6) {
    return fields;
  }
  return [...fields, ...gatewayFields(lastSlot + 1)];
}

export async function saveProviderField(
  provider: ProviderId,
  field: string,
  value: string,
): Promise<ProviderView[]> {
  if (isDesktop) {
    return invoke<ProviderView[]>("save_provider_field", {
      provider,
      field,
      value,
    });
  }
  demoProviders = demoProviders.map((entry) => {
    if (entry.id !== provider) {
      return entry;
    }
    const fields = growGateways(
      entry,
      entry.fields.map((item) =>
        item.name === field ? { ...item, set: value.trim().length > 0 } : item,
      ),
    );
    // The model has its own editor: an empty value falls back to the default,
    // and a network without a built-in default (local, custom) then stays off.
    const model =
      field === entry.model_var
        ? value.trim() || entry.default_model
        : entry.model;
    // A custom network is ready as soon as one of its gateways is; the trailing
    // empty one is an invitation, not a missing credential.
    const ready =
      entry.id === "custom"
        ? model !== "" &&
          fields.some((item) => item.name.endsWith("_BASE_URL") && item.set)
        : fields.every((item) => item.set) &&
          (entry.default_model !== "" || model !== "");
    return {
      ...entry,
      model,
      fields,
      configured: ready,
    };
  });
  await wait(150);
  return demoProviders;
}

export async function clearProvider(
  provider: ProviderId,
): Promise<ProviderView[]> {
  if (isDesktop) {
    return invoke<ProviderView[]>("clear_provider", { provider });
  }
  demoProviders = demoProviders.map((entry) =>
    entry.id === provider
      ? {
          ...entry,
          model: entry.default_model,
          configured: false,
          // Forgetting a custom network also folds the extra gateways away.
          fields:
            entry.id === "custom"
              ? gatewayFields(1)
              : entry.fields.map((field) => ({ ...field, set: false })),
        }
      : entry,
  );
  await wait(150);
  return demoProviders;
}

export async function checkProvider(
  provider: ProviderId,
): Promise<CheckOutcome> {
  if (isDesktop) {
    return invoke<CheckOutcome>("check_provider", { provider });
  }
  await wait(700);
  return {
    ok: true,
    detail: "ready (preview)",
    model: "preview",
    elapsed_ms: 700,
  };
}

/* ------------------------------------------------------------------- chapel */

export function onPrayer(handler: (event: PrayerEvent) => void): Promise<Unlisten> {
  if (isDesktop) {
    return listen<PrayerEvent>(PRAYER_EVENT, (event) => handler(event.payload));
  }
  ensureDemoRuntime();
  demoRuntime.prayers.add(handler);
  return Promise.resolve(() => {
    demoRuntime.prayers.delete(handler);
  });
}

export function onStatus(
  handler: (status: ChapelStatus) => void,
): Promise<Unlisten> {
  if (isDesktop) {
    return listen<ChapelStatus>(STATUS_EVENT, (event) => handler(event.payload));
  }
  ensureDemoRuntime();
  demoRuntime.statuses.add(handler);
  handler(demoStatus());
  return Promise.resolve(() => {
    demoRuntime.statuses.delete(handler);
  });
}

export async function getStatus(): Promise<ChapelStatus> {
  if (isDesktop) {
    return invoke<ChapelStatus>("scheduler_status");
  }
  return demoStatus();
}

export async function prayerStats(): Promise<PrayerStats> {
  if (isDesktop) {
    return invoke<PrayerStats>("prayer_stats");
  }
  return {
    total: demoRuntime.total,
    today: demoRuntime.total,
    // The preview makes one request per step, so calls and prayers match.
    requests: demoRuntime.total,
    failures: 0,
    prompt_tokens: 0,
    completion_tokens: 0,
    average_ms: demoRuntime.total > 0 ? 900 : 0,
    providers:
      demoRuntime.total > 0
        ? [{ provider: "openrouter", prayers: demoRuntime.total, tokens: 0 }]
        : [],
  };
}

export async function prayerJournal(limit = 12): Promise<PrayerRecord[]> {
  if (isDesktop) {
    return invoke<PrayerRecord[]>("prayer_journal", { limit });
  }
  return [];
}

/* -------------------------------------------------------------- confessional */

export async function listConfessions(): Promise<ConfessionEntry[]> {
  if (isDesktop) {
    return invoke<ConfessionEntry[]>("list_confessions");
  }
  return previewList();
}

/** What the person asked for when writing the entry down. */
export interface ConfessionRequest {
  /** Prayers to recite; ignored when `endless` is set. */
  repeat: number | null;
  endless: boolean;
  provider: ProviderId | null;
}

export async function submitConfession(
  text: string,
  request: ConfessionRequest,
): Promise<ConfessionOutcome> {
  if (isDesktop) {
    return invoke<ConfessionOutcome>("submit_confession", {
      text,
      repeat: request.repeat,
      endless: request.endless,
      provider: request.provider,
    });
  }
  const robots = await previewRoster();
  const outcome = previewSubmit(text, request, robots.length, robots);
  emitDemoStatus();
  return outcome;
}

export async function cancelConfession(id: string): Promise<ConfessionEntry[]> {
  if (isDesktop) {
    return invoke<ConfessionEntry[]>("cancel_confession", { id });
  }
  const entries = previewCancel(id);
  emitDemoStatus();
  return entries;
}

export async function forgetFinishedConfessions(): Promise<ConfessionEntry[]> {
  if (isDesktop) {
    return invoke<ConfessionEntry[]>("forget_finished_confessions");
  }
  const entries = previewForget();
  emitDemoStatus();
  return entries;
}

/* ------------------------------------------------------------ demo runtime */

const DEMO_STEP_MS = 6_500;
const DEMO_FIRST_STEP_MS = 900;

interface DemoRuntime {
  prayers: Set<(event: PrayerEvent) => void>;
  statuses: Set<(status: ChapelStatus) => void>;
  timer: number | null;
  cursor: number;
  total: number;
}

const demoRuntime: DemoRuntime = {
  prayers: new Set(),
  statuses: new Set(),
  timer: null,
  cursor: 0,
  total: 0,
};

function demoStatus(): ChapelStatus {
  const entries = previewList();
  return {
    tempo: "steady",
    pending: entries.filter((entry) => entry.status === "queued").length,
    fulfilled: entries.filter((entry) => entry.status === "fulfilled").length,
    budget: { done: demoRuntime.total, limit: 60, resets_in_secs: 3600 },
    cooldown_secs: 0,
    failures: 0,
    configured: true,
    storage: "file",
    error: null,
    next_in_secs: null,
    waiting_for: null,
    buffered: 0,
    // The showcase runs without the shell, so it borrows the shell's speed.
    reveal_cps: 10,
  };
}

function emitDemoStatus(): void {
  const status = demoStatus();
  demoRuntime.statuses.forEach((handler) => handler(status));
}

async function demoStep(): Promise<void> {
  const preferences = await loadPreferences();
  const robots = await previewRoster();
  const now = Date.now();
  const intercession = previewAdvance(now);
  const clerics = Math.max(robots.length, 1);
  const cleric = intercession ? intercession.cleric : demoRuntime.cursor % clerics;
  if (!intercession) {
    demoRuntime.cursor += 1;
  }
  demoRuntime.total += 1;

  const event: PrayerEvent = {
    cleric,
    text: previewPrayer(preferences.religion, preferences.language),
    source: intercession ? "intercession" : "schedule",
    provider: robots[cleric % robots.length] ?? "openrouter",
    model: "preview",
    theme: intercession?.theme ?? null,
    severity: intercession?.severity ?? null,
    ordinal: intercession?.prayers_done ?? null,
    target:
      intercession && !intercession.endless ? intercession.prayers_target : null,
    intercession_id: intercession?.id ?? null,
    elapsed_ms: 900,
    prompt_tokens: 0,
    completion_tokens: 0,
    at: now,
  };
  demoRuntime.prayers.forEach((handler) => handler(event));
  emitDemoStatus();
}

/** Starts the preview loop on first subscription, not on import. */
function ensureDemoRuntime(): void {
  if (demoRuntime.timer !== null) {
    return;
  }
  demoRuntime.timer = window.setInterval(() => {
    void demoStep();
  }, DEMO_STEP_MS);
  window.setTimeout(() => {
    void demoStep();
  }, DEMO_FIRST_STEP_MS);
}
