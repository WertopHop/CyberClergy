// The chapel listens to the scheduler instead of driving it.
//
// The Rust loop decides which cleric prays and when; this hook only collects the
// `chapel://prayer` events into bubbles, tracks who is currently speaking and
// keeps the latest status. Bubbles expire on their own, length-aware, so a long
// prayer is not cut off and a short one does not linger.
import { useCallback, useEffect, useRef, useState } from "react";

import { getStatus, onPrayer, onStatus } from "../lib/bridge";
import type {
  ChapelStatus,
  PrayerEvent,
  PrayerSource,
  ThemeId,
} from "../lib/types";

/** A bubble lives longer when there is more to read, within reason. */
const MIN_TTL_MS = 14_000;
const TTL_PER_CHAR_MS = 45;
const MAX_TTL_MS = 48_000;
/** How long a cleric keeps the praying pose after speaking. */
const ACTIVE_MS = 8_000;
const PRUNE_MS = 1_500;

export interface Bubble {
  text: string;
  /** Unix timestamp in milliseconds, used as the animation key. */
  at: number;
  source: PrayerSource;
  provider: string | null;
  model: string | null;
  theme: ThemeId | null;
  ordinal: number | null;
  target: number | null;
}

export interface ChapelState {
  bubbles: Record<number, Bubble>;
  activeIndex: number | null;
  status: ChapelStatus | null;
  /** True once the first status arrived, so the UI can avoid a flash. */
  ready: boolean;
  reload: () => void;
}

function bubbleTtl(bubble: Bubble): number {
  return Math.min(
    MAX_TTL_MS,
    MIN_TTL_MS + bubble.text.length * TTL_PER_CHAR_MS,
  );
}

function toBubble(event: PrayerEvent): Bubble {
  return {
    text: event.text,
    at: event.at,
    source: event.source,
    provider: event.provider,
    model: event.model,
    theme: event.theme,
    ordinal: event.ordinal,
    target: event.target,
  };
}

export function useChapel({ enabled }: { enabled: boolean }): ChapelState {
  const [bubbles, setBubbles] = useState<Record<number, Bubble>>({});
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [status, setStatus] = useState<ChapelStatus | null>(null);
  const [ready, setReady] = useState(false);
  const activeTimer = useRef<number | null>(null);

  useEffect(() => {
    if (!enabled) {
      setBubbles({});
      setActiveIndex(null);
      return;
    }

    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void onPrayer((event) => {
      if (cancelled) {
        return;
      }
      setBubbles((current) => ({
        ...current,
        [event.cleric]: toBubble(event),
      }));
      setActiveIndex(event.cleric);
      if (activeTimer.current !== null) {
        window.clearTimeout(activeTimer.current);
      }
      activeTimer.current = window.setTimeout(() => {
        setActiveIndex(null);
        activeTimer.current = null;
      }, ACTIVE_MS);
    }).then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisteners.push(stop);
      }
    });

    void onStatus((next) => {
      if (!cancelled) {
        setStatus(next);
        setReady(true);
      }
    }).then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisteners.push(stop);
      }
    });

    // The event listener misses a status emitted before the window was ready.
    void getStatus()
      .then((current) => {
        if (!cancelled) {
          setStatus(current);
          setReady(true);
        }
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
      unlisteners.forEach((stop) => stop());
      if (activeTimer.current !== null) {
        window.clearTimeout(activeTimer.current);
        activeTimer.current = null;
      }
    };
  }, [enabled]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const now = Date.now();
      setBubbles((current) => {
        let changed = false;
        const next: Record<number, Bubble> = {};
        for (const [key, bubble] of Object.entries(current)) {
          if (now - bubble.at < bubbleTtl(bubble)) {
            next[Number(key)] = bubble;
          } else {
            changed = true;
          }
        }
        return changed ? next : current;
      });
    }, PRUNE_MS);
    return () => window.clearInterval(timer);
  }, []);

  const reload = useCallback(() => {
    void getStatus()
      .then(setStatus)
      .catch(() => undefined);
  }, []);

  return { bubbles, activeIndex, status, ready, reload };
}
