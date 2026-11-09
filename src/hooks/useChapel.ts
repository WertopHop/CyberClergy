// The chapel listens to the scheduler instead of driving it.
//
// The Rust loop decides which cleric prays and when; this hook only collects the
// `chapel://prayer` events into bubbles, tracks who is currently speaking and
// keeps the latest status.
//
// A prayer arrives as finished text, but it is revealed as if it were being
// written: a batch of three would otherwise flash three paragraphs at once. The
// typing pace, the blinking cursor and the queue for prayers that arrive while
// the previous one is still being typed all live here, so the components stay
// presentational.
import { useCallback, useEffect, useRef, useState } from "react";

import { getStatus, onPrayer, onStatus } from "../lib/bridge";
import type {
  ChapelStatus,
  PrayerEvent,
  PrayerSource,
  ThemeId,
} from "../lib/types";

/**
 * Reveal speed of a prayer. Deliberately slow: ten characters a second reads
 * like a hand still writing, and a finished prayer never flashes by unread.
 * The reveal is computed from the elapsed time rather than per tick, so a
 * throttled background window cannot swallow half of a prayer.
 *
 * The number itself belongs to the shell: it schedules the next prayer from the
 * last letter of this one, so both sides must read the same speed. It arrives in
 * the status snapshot; this is only what is used before the first one lands.
 */
const FALLBACK_CHARS_PER_SECOND = 10;
const TICK_MS = 60;
/** A bubble lives longer when there is more to read, within reason. */
const MIN_TTL_MS = 14_000;
const TTL_PER_CHAR_MS = 45;
const MAX_TTL_MS = 60_000;
/** How long a cleric keeps the praying pose after speaking. */
const ACTIVE_MS = 8_000;

export interface Bubble {
  text: string;
  /** Characters revealed so far; equals `text.length` once fully typed. */
  shown: number;
  /** Unix timestamp in milliseconds, used as the animation key. */
  at: number;
  /** Moment the bubble leaves the screen. */
  expires: number;
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

function typingDuration(text: string, charsPerSecond: number): number {
  return Math.ceil((text.length / charsPerSecond) * 1000);
}

/**
 * How long the finished prayer stays on screen. A chosen rhythm is a pause
 * *after* the last word, so it replaces the reading time; without one the
 * bubble is sized to its own length.
 */
function bubbleLifetime(text: string, pauseSecs: number | null): number {
  return pauseSecs !== null ? pauseSecs * 1000 : bubbleTtl(text);
}

/** Characters that should be on screen by `now`. */
function revealed(bubble: Bubble, now: number, charsPerSecond: number): number {
  const elapsed = Math.max(0, now - bubble.at);
  return Math.min(
    bubble.text.length,
    Math.floor((elapsed / 1000) * charsPerSecond),
  );
}

function bubbleTtl(text: string): number {
  return Math.min(MAX_TTL_MS, MIN_TTL_MS + text.length * TTL_PER_CHAR_MS);
}

/** Created when typing starts, so the bubble lives through its own typing. */
function toBubble(
  event: PrayerEvent,
  now: number,
  pauseSecs: number | null,
  charsPerSecond: number,
): Bubble {
  return {
    text: event.text,
    shown: 0,
    at: now,
    expires:
      now +
      typingDuration(event.text, charsPerSecond) +
      bubbleLifetime(event.text, pauseSecs),
    source: event.source,
    provider: event.provider,
    model: event.model,
    theme: event.theme,
    ordinal: event.ordinal,
    target: event.target,
  };
}

/** Takes the next prayer waiting for a slot, if there is one. */
function shiftQueued(
  queued: { current: Record<number, PrayerEvent[]> },
  index: number,
): PrayerEvent | null {
  const pending = queued.current[index];
  if (!pending || pending.length === 0) {
    return null;
  }
  const event = pending.shift() ?? null;
  if (pending.length === 0) {
    delete queued.current[index];
  }
  return event;
}

export function useChapel({
  enabled,
  pauseSecs,
}: {
  enabled: boolean;
  /** Pause after a prayer is fully typed; `null` sizes the bubble by its text. */
  pauseSecs: number | null;
}): ChapelState {
  const [bubbles, setBubbles] = useState<Record<number, Bubble>>({});
  /** Latest bubbles, so the timers can read them without waiting for a render. */
  const visible = useRef(bubbles);
  visible.current = bubbles;
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [status, setStatus] = useState<ChapelStatus | null>(null);
  const [ready, setReady] = useState(false);
  const activeTimer = useRef<number | null>(null);
  /** Prayers that arrived while the same cleric was still typing. */
  const queued = useRef<Record<number, PrayerEvent[]>>({});
  /** Keeps the tick from doing any work on an empty chapel. */
  const lively = useRef(false);
  /** The tick is created once, so the rhythm reaches it through a ref. */
  const pause = useRef<number | null>(pauseSecs);
  pause.current = pauseSecs;
  /** Typing speed as reported by the shell, replaced by every status snapshot. */
  const cps = useRef(FALLBACK_CHARS_PER_SECOND);

  const touchActive = useCallback((cleric: number) => {
    setActiveIndex(cleric);
    if (activeTimer.current !== null) {
      window.clearTimeout(activeTimer.current);
    }
    activeTimer.current = window.setTimeout(() => {
      setActiveIndex(null);
      activeTimer.current = null;
    }, ACTIVE_MS);
  }, []);

  useEffect(() => {
    if (!enabled) {
      setBubbles({});
      setActiveIndex(null);
      queued.current = {};
      lively.current = false;
      return;
    }

    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void onPrayer((event) => {
      if (cancelled) {
        return;
      }
      const awaited = !visible.current[event.cleric];
      setBubbles((current) => {
        // The shell asks for the next prayer slightly ahead of time so it is
        // ready the moment the pause ends. A prayer that arrives while the same
        // cleric still has something on screen — being typed or waiting out the
        // pause — takes its place in the queue and is shown exactly on time.
        if (current[event.cleric]) {
          const pending = queued.current[event.cleric] ?? [];
          // A slow reveal makes a long queue pointless: the oldest waiting
          // prayer would only be shown stale, so the two newest are kept.
          if (pending.length >= 2) {
            pending.shift();
          }
          pending.push(event);
          queued.current[event.cleric] = pending;
          return current;
        }
        lively.current = true;
        return {
          ...current,
          [event.cleric]: toBubble(event, Date.now(), pause.current, cps.current),
        };
      });
      // The robot lights up when it actually starts speaking.
      if (awaited) {
        touchActive(event.cleric);
      }
    }).then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisteners.push(stop);
      }
    });

    void onStatus((next) => {
      if (!cancelled) {
        cps.current = next.reveal_cps || FALLBACK_CHARS_PER_SECOND;
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
          cps.current = current.reveal_cps || FALLBACK_CHARS_PER_SECOND;
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
  }, [enabled, touchActive]);

  // One tick drives everything time-based: reveal, expiry and the queue.
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!lively.current) {
        return;
      }
      const now = Date.now();
      // Read from the rendered bubbles rather than from inside the updater:
      // React may run an updater later, after the glow would have been missed.
      const waking = Object.entries(visible.current)
        .filter(([key, bubble]) => {
          const waiting = queued.current[Number(key)];
          return now >= bubble.expires && waiting && waiting.length > 0;
        })
        .map(([key]) => Number(key));
      setBubbles((current) => {
        let changed = false;
        const next: Record<number, Bubble> = {};
        for (const [key, bubble] of Object.entries(current)) {
          const index = Number(key);
          // Nothing replaces a bubble before its own time is up, so a prayer
          // that arrived early waits out the full pause.
          if (now >= bubble.expires) {
            changed = true;
            const waiting = shiftQueued(queued, index);
            if (waiting) {
              next[index] = toBubble(waiting, now, pause.current, cps.current);
            }
            continue;
          }
          if (bubble.shown < bubble.text.length) {
            const shown = revealed(bubble, now, cps.current);
            if (shown !== bubble.shown) {
              changed = true;
              next[index] = { ...bubble, shown };
            } else {
              next[index] = bubble;
            }
            continue;
          }
          next[index] = bubble;
        }
        if (!changed) {
          return current;
        }
        lively.current = Object.keys(next).length > 0;
        return next;
      });
      waking.forEach(touchActive);
    }, TICK_MS);
    return () => window.clearInterval(timer);
  }, []);

  const reload = useCallback(() => {
    void getStatus()
      .then(setStatus)
      .catch(() => undefined);
  }, []);

  return { bubbles, activeIndex, status, ready, reload };
}
