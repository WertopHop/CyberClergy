// Browser-only stand-in for `confessions.json`.
//
// The desktop build keeps the queue in Rust (`IntercessionQueue`), including the
// local classification of an entry into a direction. This module exists only so
// the interface can be inspected with `npm run dev`, without Tauri: it stores
// entries in memory, spreads them over the clerics and counts prayers down.
import type { ConfessionEntry, ConfessionOutcome } from "./types";

const MAX_ENTRIES = 50;

let entries: ConfessionEntry[] = [];
let serial = 0;

/** A direction that could have come from the local classifier. */
const PREVIEW_DIRECTION =
  "a prayer for an unnamed need: ask for steadiness, gentleness and peace of heart";

function clone(entry: ConfessionEntry): ConfessionEntry {
  return { ...entry };
}

export function previewList(): ConfessionEntry[] {
  return entries.map(clone);
}

export function previewSubmit(text: string, clerics: number): ConfessionOutcome {
  const trimmed = text.trim();
  const normalized = trimmed.toLocaleLowerCase();
  const existing = entries.find(
    (entry) => entry.status === "queued" && entry.text.toLocaleLowerCase() === normalized,
  );
  if (existing) {
    return { entry: clone(existing), duplicate: true };
  }

  serial += 1;
  const slot = Math.max(clerics, 1);
  const entry: ConfessionEntry = {
    id: `preview-${Date.now()}-${serial}`,
    text: trimmed,
    created_at: Date.now(),
    theme: "unnamed",
    severity: "steady",
    direction: PREVIEW_DIRECTION,
    cleric: serial % slot,
    prayers_done: 0,
    prayers_target: 5,
    status: "queued",
    last_prayed_at: null,
  };
  entries = [...entries, entry].slice(-MAX_ENTRIES);
  return { entry: clone(entry), duplicate: false };
}

export function previewCancel(id: string): ConfessionEntry[] {
  entries = entries.map((entry) =>
    entry.id === id && entry.status === "queued"
      ? { ...entry, status: "cancelled" }
      : entry,
  );
  return previewList();
}

export function previewForget(): ConfessionEntry[] {
  entries = entries.filter((entry) => entry.status === "queued");
  return previewList();
}

/**
 * Counts one prayer down on the oldest waiting entry and returns it. The
 * preview scheduler calls this before emitting its next prayer.
 */
export function previewAdvance(now: number): ConfessionEntry | null {
  const target = entries.find((entry) => entry.status === "queued");
  if (!target) {
    return null;
  }
  const done = target.prayers_done + 1;
  const fulfilled = done >= target.prayers_target;
  entries = entries.map((entry) =>
    entry.id === target.id
      ? {
          ...entry,
          prayers_done: done,
          last_prayed_at: now,
          status: fulfilled ? "fulfilled" : "queued",
        }
      : entry,
  );
  return {
    ...target,
    prayers_done: done,
    last_prayed_at: now,
    status: fulfilled ? "fulfilled" : "queued",
  };
}
