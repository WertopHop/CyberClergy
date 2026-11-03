// Drives the chapel until the real scheduler exists: one sequential loop asks
// for a short prayer, hands it to the next cleric and lets the bubble expire.
// Sequential on purpose, so free-tier limits are not hammered. Only the newest
// bubble is kept: one clear voice at a time keeps the scene readable.
import { useCallback, useEffect, useState } from "react";

import { generatePrayer } from "../lib/bridge";

const BUBBLE_TTL_MS = 22_000;
const GAP_MS = 2_600;
const RETRY_MS = 9_000;
const PRUNE_INTERVAL_MS = 2_000;

export interface Bubble {
  text: string;
  createdAt: number;
  provider: string;
}

export interface ChapelState {
  bubbles: Record<number, Bubble>;
  activeIndex: number | null;
  error: string | null;
  reload: () => void;
}

interface ChapelOptions {
  clerics: number;
  religion: string;
  language: string;
  enabled: boolean;
}

export function useChapel({
  clerics,
  religion,
  language,
  enabled,
}: ChapelOptions): ChapelState {
  const [bubbles, setBubbles] = useState<Record<number, Bubble>>({});
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!enabled || !religion) {
      setActiveIndex(null);
      return;
    }

    let cancelled = false;
    let cursor = 0;
    const sleep = (milliseconds: number) =>
      new Promise<void>((resolve) => {
        window.setTimeout(resolve, milliseconds);
      });

    const loop = async () => {
      while (!cancelled) {
        const index = cursor % Math.max(clerics, 1);
        cursor += 1;
        setActiveIndex(index);
        try {
          const prayer = await generatePrayer({
            religion,
            short: true,
            language,
          });
          if (cancelled) {
            return;
          }
          setError(null);
          setBubbles({
            [index]: {
              text: prayer.text,
              createdAt: Date.now(),
              provider: prayer.provider,
            },
          });
          await sleep(GAP_MS);
        } catch (cause) {
          if (cancelled) {
            return;
          }
          setError(String(cause));
          await sleep(RETRY_MS);
        }
      }
    };

    setBubbles({});
    void loop();

    return () => {
      cancelled = true;
      setActiveIndex(null);
    };
  }, [enabled, religion, clerics, language, attempt]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const now = Date.now();
      setBubbles((current) => {
        let changed = false;
        const next: Record<number, Bubble> = {};
        for (const [key, bubble] of Object.entries(current)) {
          if (now - bubble.createdAt < BUBBLE_TTL_MS) {
            next[Number(key)] = bubble;
          } else {
            changed = true;
          }
        }
        return changed ? next : current;
      });
    }, PRUNE_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, []);

  const reload = useCallback(() => setAttempt((value) => value + 1), []);

  return { bubbles, activeIndex, error, reload };
}
