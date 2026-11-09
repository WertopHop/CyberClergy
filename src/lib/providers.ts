// Provider labels and the roster rules shared by the settings sheet, the
// confessional and the chapel. A new network is one line in `PROVIDER_LABELS`
// here and one spec row in the Rust crate.
import type { Translate } from "../i18n";
import type { Dictionary } from "../i18n/locales/en";
import type { ProviderId } from "./types";

/** Longest recital a person may ask for; mirrors the crate's `MAX_REPEAT`. */
export const MAX_REPEAT = 999;

/**
 * Localisation key of every network, kept in the crate's spec order.
 *
 * `Record<ProviderId, ...>` makes the compiler refuse a provider added to the
 * union without a label, so the map replaces both the old switch and a separate
 * id list.
 */
const PROVIDER_LABELS: Record<ProviderId, keyof Dictionary> = {
  openrouter: "providerOpenRouter",
  gemini: "providerGemini",
  groq: "providerGroq",
  mistral: "providerMistral",
  cloudflare: "providerCloudflare",
  nvidia: "providerNvidia",
  together: "providerTogether",
  cerebras: "providerCerebras",
  deepinfra: "providerDeepinfra",
  "github-models": "providerGithub",
  ollama: "providerOllama",
  custom: "providerCustom",
};

/** Every network the interface can name, in the crate's spec order. */
export const PROVIDER_IDS = Object.keys(PROVIDER_LABELS) as ProviderId[];

/** Highest number of robots the cloud holds: four tiers of three, one per network. */
export const MAX_ROBOTS = PROVIDER_IDS.length;

export function isProviderId(value: string): value is ProviderId {
  return Object.prototype.hasOwnProperty.call(PROVIDER_LABELS, value);
}

export function providerLabel(id: ProviderId, t: Translate): string {
  const key = PROVIDER_LABELS[id];
  return key ? t(key) : id;
}

/** Journal rows name networks by their id; the quiet canon is not a network. */
export function providerName(id: string, t: Translate): string {
  if (id === "quiet") {
    return t("statsProviderQuiet");
  }
  return isProviderId(id) ? providerLabel(id, t) : id;
}

/**
 * Robots actually standing on the cloud.
 *
 * A stored roster keeps its order and loses only networks whose credentials are
 * gone; `null` means the person never picked one, so every configured network
 * stands up. The chapel and the scheduler resolve the same rule on both sides.
 */
export function cloudRoster(
  stored: ProviderId[] | null,
  configured: ProviderId[],
): ProviderId[] {
  if (stored === null) {
    return configured.slice(0, MAX_ROBOTS);
  }
  return stored.filter((id) => configured.includes(id));
}
