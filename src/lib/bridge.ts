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
import type {
  ChapelStatus,
  CheckOutcome,
  ConfessionEntry,
  ConfessionOutcome,
  PrayerEvent,
  PrayerRecord,
  PrayerStats,
  Preferences,
  ProviderId,
  ProviderView,
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
  clerics: 7,
  pace: "normal",
};

const PREFS_KEY = "cyberclergy.preview.preferences";

const DEMO_PROVIDERS: ProviderView[] = [
  {
    id: "openrouter",
    model: "meta-llama/llama-3.3-70b-instruct:free",
    configured: true,
    fields: [{ name: "OPENROUTER_API_KEY", set: true }],
  },
  {
    id: "gemini",
    model: "gemini-2.5-flash",
    configured: false,
    fields: [{ name: "GEMINI_API_KEY", set: false }],
  },
  {
    id: "groq",
    model: "llama-3.3-70b-versatile",
    configured: false,
    fields: [{ name: "GROQ_API_KEY", set: false }],
  },
  {
    id: "mistral",
    model: "mistral-small-latest",
    configured: false,
    fields: [{ name: "MISTRAL_API_KEY", set: false }],
  },
  {
    id: "cloudflare",
    model: "@cf/meta/llama-3.1-8b-instruct",
    configured: false,
    fields: [
      { name: "CLOUDFLARE_API_TOKEN", set: false },
      { name: "CLOUDFLARE_ACCOUNT_ID", set: false },
    ],
  },
  {
    id: "nvidia",
    model: "meta/llama-3.3-70b-instruct",
    configured: false,
    fields: [{ name: "NVIDIA_API_KEY", set: false }],
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
  return { ...DEFAULT_PREFERENCES, ...value, pace };
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

export async function listReligions(): Promise<ReligionProfile[]> {
  if (isDesktop) {
    return invoke<ReligionProfile[]>("list_religions");
  }
  return DEMO_RELIGIONS;
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
    const fields = entry.fields.map((item) =>
      item.name === field ? { ...item, set: value.trim().length > 0 } : item,
    );
    return {
      ...entry,
      fields,
      configured: fields.every((item) => item.set),
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
          configured: false,
          fields: entry.fields.map((field) => ({ ...field, set: false })),
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

export async function submitConfession(
  text: string,
): Promise<ConfessionOutcome> {
  if (isDesktop) {
    return invoke<ConfessionOutcome>("submit_confession", { text });
  }
  const preferences = await loadPreferences();
  const outcome = previewSubmit(text, preferences.clerics);
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
  };
}

function emitDemoStatus(): void {
  const status = demoStatus();
  demoRuntime.statuses.forEach((handler) => handler(status));
}

async function demoStep(): Promise<void> {
  const preferences = await loadPreferences();
  const now = Date.now();
  const intercession = previewAdvance(now);
  const clerics = Math.max(preferences.clerics, 1);
  const cleric = intercession ? intercession.cleric : demoRuntime.cursor % clerics;
  if (!intercession) {
    demoRuntime.cursor += 1;
  }
  demoRuntime.total += 1;

  const event: PrayerEvent = {
    cleric,
    text: previewPrayer(preferences.religion, preferences.language),
    source: intercession ? "intercession" : "schedule",
    provider: "openrouter",
    model: "preview",
    theme: intercession?.theme ?? null,
    severity: intercession?.severity ?? null,
    ordinal: intercession?.prayers_done ?? null,
    target: intercession?.prayers_target ?? null,
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
