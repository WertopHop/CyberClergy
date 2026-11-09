// Provider labels and the roster rules shared by the settings sheet, the
// confessional and the chapel. Keeping them here means a new network is added
// in one place on the interface side, next to its entry in the Rust spec.
import type { Translate } from "../i18n";
import type { ProviderId } from "./types";

/** Longest recital a person may ask for; mirrors the crate's `MAX_REPEAT`. */
export const MAX_REPEAT = 999;

/** Every network the interface can name, in the crate's spec order. */
export const PROVIDER_IDS: ProviderId[] = [
  "openrouter",
  "gemini",
  "groq",
  "mistral",
  "cloudflare",
  "nvidia",
  "together",
  "cerebras",
  "deepinfra",
  "github-models",
  "ollama",
  "custom",
];

/** Highest number of robots the cloud holds: four tiers of three, one per network. */
export const MAX_ROBOTS = PROVIDER_IDS.length;

export function isProviderId(value: string): value is ProviderId {
  return PROVIDER_IDS.some((id) => id === value);
}

export function providerLabel(id: ProviderId, t: Translate): string {
  switch (id) {
    case "openrouter":
      return t("providerOpenRouter");
    case "gemini":
      return t("providerGemini");
    case "groq":
      return t("providerGroq");
    case "mistral":
      return t("providerMistral");
    case "cloudflare":
      return t("providerCloudflare");
    case "nvidia":
      return t("providerNvidia");
    case "together":
      return t("providerTogether");
    case "cerebras":
      return t("providerCerebras");
    case "deepinfra":
      return t("providerDeepinfra");
    case "github-models":
      return t("providerGithub");
    case "ollama":
      return t("providerOllama");
    case "custom":
      return t("providerCustom");
    default:
      return id;
  }
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
