// Fixtures for the browser preview (`npm run dev` without Tauri). The desktop
// build always talks to Rust; these only keep the layout inspectable. Religion
// data is imported from the real profile files so the preview cannot drift from
// the shipped clergy.
import christianityProfile from "../../crates/cyberclergy-llm/resources/clergy/christianity/profile.json";
import islamProfile from "../../crates/cyberclergy-llm/resources/clergy/islam/profile.json";
import judaismProfile from "../../crates/cyberclergy-llm/resources/clergy/judaism/profile.json";
import type { ReligionProfile } from "./types";

export const DEMO_RELIGIONS = [
  judaismProfile,
  christianityProfile,
  islamProfile,
] as unknown as ReligionProfile[];

/** Dictionary key holding a sample prayer for each bundled tradition. */
export const PREVIEW_KEYS = {
  judaism: "previewJudaism",
  christianity: "previewChristianity",
  islam: "previewIslam",
} as const;

export type PreviewKey = (typeof PREVIEW_KEYS)[keyof typeof PREVIEW_KEYS];
