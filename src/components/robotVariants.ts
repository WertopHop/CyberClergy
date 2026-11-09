// Per-provider looks for the praying mascot.
//
// Every cleric shares one silhouette; only the accent trim, one characteristic
// detail and the glyph on the chest hatch differ, so adding a network stays a
// single entry here. Accents are deliberately kept inside the warm palette of
// the rest of the interface instead of copying brand colours, which would drag
// blue and purple back into a scene that is meant to read as candle light.
import type { ProviderId } from "../lib/types";

export type RobotAccessory = "halo" | "wings" | "headset" | "hood" | "scarf" | "crest";

export interface RobotVariant {
  /** Trim colour: collar, stole, hands, antenna tips and the accessory. */
  accent: string;
  /** The one detail that makes this cleric recognisable at a glance. */
  accessory: RobotAccessory;
  /** Abstract mark inside the hatch, drawn in a 10x10 box around the origin. */
  glyph: string;
}

export const PROVIDER_VARIANTS: Record<ProviderId, RobotVariant> = {
  openrouter: {
    accent: "#c9955c",
    accessory: "halo",
    glyph: "M -4.5 0 L 0 -4.5 L 4.5 0 L 0 4.5 Z",
  },
  gemini: {
    accent: "#8fadc4",
    accessory: "wings",
    glyph: "M -4 3 L 4 -3 M -4 -3 L 4 3",
  },
  groq: {
    accent: "#d57f5c",
    accessory: "headset",
    glyph: "M -4.5 -2.5 L 0 3 L 4.5 -2.5",
  },
  mistral: {
    accent: "#cbb26a",
    accessory: "hood",
    glyph: "M -4.5 -4 L -0.5 0 L -4.5 4 M 0.5 -4 L 4.5 0 L 0.5 4",
  },
  cloudflare: {
    accent: "#dca94e",
    accessory: "scarf",
    glyph: "M -5 3 L -1 -4 L 3 2 L 5 0",
  },
  nvidia: {
    accent: "#9cbb8a",
    accessory: "crest",
    glyph: "M -4.5 -3 L 4.5 0 L -4.5 3 Z",
  },
  together: {
    accent: "#c9a27a",
    accessory: "crest",
    glyph: "M -4.5 -3.5 H 4.5 L 0 3.5 Z",
  },
  cerebras: {
    accent: "#b98f6a",
    accessory: "halo",
    glyph: "M 0 -4.5 L 4 -1 L 0 4.5 L -4 -1 Z",
  },
  deepinfra: {
    accent: "#c7a98f",
    accessory: "hood",
    glyph: "M -4.5 3 L -4.5 -3 L 4.5 -3 M -4.5 0 H 3",
  },
  "github-models": {
    accent: "#a7b391",
    accessory: "scarf",
    glyph: "M 0 -4.5 A 4.5 4.5 0 1 1 -3.2 1.9",
  },
  ollama: {
    accent: "#adb79b",
    accessory: "wings",
    glyph: "M -4.5 2.5 A 4.5 4.5 0 0 1 4.5 2.5 Z",
  },
  custom: {
    accent: "#d0b088",
    accessory: "headset",
    glyph: "M -4 0 H 4 M 0 -4 V 4 M -2.8 -2.8 L 2.8 2.8",
  },
};

/** Used before the cleric is bound to a network, and for unknown ids. */
export const DEFAULT_VARIANT: RobotVariant = {
  accent: "var(--accent)",
  accessory: "halo",
  glyph: "M -4 0 H 4 M 0 -4 V 4",
};

export function variantFor(provider: ProviderId | null | undefined): RobotVariant {
  if (provider && Object.prototype.hasOwnProperty.call(PROVIDER_VARIANTS, provider)) {
    return PROVIDER_VARIANTS[provider];
  }
  return DEFAULT_VARIANT;
}
