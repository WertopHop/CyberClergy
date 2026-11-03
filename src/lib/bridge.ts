// Single boundary between the interface and the Rust shell.
//
// Every command is wrapped here so components never touch `invoke` directly and
// so the whole interface can be developed in a plain browser: when the Tauri
// internals are missing the same functions answer with demo data.
import { invoke } from "@tauri-apps/api/core";

import { dictionaryFor } from "../i18n";
import { DEMO_RELIGIONS, PREVIEW_KEYS, type PreviewKey } from "./demo";
import type {
  CheckOutcome,
  Preferences,
  PrayerOutcome,
  ProviderId,
  ProviderView,
  ReligionProfile,
} from "./types";

export const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const DEFAULT_PREFERENCES: Preferences = {
  language: "ru",
  theme: "night",
  religion: "christianity",
  clerics: 7,
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

function previewPrayer(religion: string, language: string): string {
  const key: PreviewKey =
    PREVIEW_KEYS[religion as keyof typeof PREVIEW_KEYS] ?? "previewChristianity";
  return dictionaryFor(language)[key];
}

export async function loadPreferences(): Promise<Preferences> {
  if (isDesktop) {
    return invoke<Preferences>("load_preferences");
  }
  try {
    const raw = window.localStorage.getItem(PREFS_KEY);
    if (raw) {
      return { ...DEFAULT_PREFERENCES, ...(JSON.parse(raw) as Preferences) };
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

export interface PrayerRequest {
  religion: string;
  short: boolean;
  intent?: string;
  /** Used by the preview only; the Rust command follows the saved language. */
  language: string;
}

export async function generatePrayer(
  request: PrayerRequest,
): Promise<PrayerOutcome> {
  if (isDesktop) {
    return invoke<PrayerOutcome>("generate_prayer", {
      religion: request.religion,
      short: request.short,
      intent: request.intent ?? null,
    });
  }
  await wait(900);
  return {
    text: previewPrayer(request.religion, request.language),
    religion: request.religion,
    provider: "openrouter",
    model: "preview",
    elapsed_ms: 900,
  };
}
