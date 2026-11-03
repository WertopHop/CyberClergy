// Local, device-only storage for confessional entries. They are never sent
// anywhere yet; the scheduler stage will feed them to the clergy as prayer
// intents.
import type { Confession } from "./types";

const STORAGE_KEY = "cyberclergy.confessions";
const MAX_ENTRIES = 200;
const MAX_LENGTH = 400;

export function loadConfessions(): Confession[] {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .filter((entry): entry is Confession => {
        const candidate = entry as Partial<Confession>;
        return (
          typeof candidate.id === "string" &&
          typeof candidate.text === "string" &&
          typeof candidate.createdAt === "number"
        );
      })
      .slice(0, MAX_ENTRIES);
  } catch {
    return [];
  }
}

export function saveConfessions(confessions: Confession[]): void {
  try {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(confessions.slice(0, MAX_ENTRIES)),
    );
  } catch {
    // A full or disabled storage area must not break the chapel.
  }
}

export function createConfession(text: string): Confession | null {
  const trimmed = text.trim().slice(0, MAX_LENGTH);
  if (!trimmed) {
    return null;
  }
  return {
    id: `confession-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    text: trimmed,
    createdAt: Date.now(),
  };
}
