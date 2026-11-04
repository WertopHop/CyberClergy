// Confessional entries live in `confessions.json` (Rust) or in memory (browser
// preview). Counters move on their own while the scheduler prays, so the hook
// re-reads the list on a slow poll; writes refresh it immediately.
import { useCallback, useEffect, useRef, useState } from "react";

import {
  cancelConfession,
  forgetFinishedConfessions,
  listConfessions,
  submitConfession,
} from "../lib/bridge";
import type { ConfessionEntry } from "../lib/types";

const POLL_MS = 8_000;
const NOTICE_MS = 4_000;

export type ConfessionNotice = "duplicate" | null;

export interface ConfessionsState {
  entries: ConfessionEntry[];
  busy: boolean;
  notice: ConfessionNotice;
  error: string | null;
  /** Resolves to true when the entry is stored or already waiting. */
  add: (text: string) => Promise<boolean>;
  cancel: (id: string) => Promise<void>;
  forget: () => Promise<void>;
  reload: () => void;
}

export function useConfessions(): ConfessionsState {
  const [entries, setEntries] = useState<ConfessionEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<ConfessionNotice>(null);
  const [error, setError] = useState<string | null>(null);
  const noticeTimer = useRef<number | null>(null);

  const showNotice = useCallback((value: ConfessionNotice) => {
    setNotice(value);
    if (noticeTimer.current !== null) {
      window.clearTimeout(noticeTimer.current);
    }
    if (value === null) {
      return;
    }
    noticeTimer.current = window.setTimeout(() => {
      setNotice(null);
      noticeTimer.current = null;
    }, NOTICE_MS);
  }, []);

  const reload = useCallback(() => {
    void listConfessions()
      .then((loaded) => {
        setEntries(loaded);
      })
      .catch((cause) => setError(String(cause)));
  }, []);

  useEffect(() => {
    reload();
    const timer = window.setInterval(reload, POLL_MS);
    return () => {
      window.clearInterval(timer);
      if (noticeTimer.current !== null) {
        window.clearTimeout(noticeTimer.current);
      }
    };
  }, [reload]);

  const add = useCallback(
    async (text: string): Promise<boolean> => {
      setBusy(true);
      setError(null);
      try {
        const outcome = await submitConfession(text);
        const loaded = await listConfessions();
        setEntries(loaded);
        showNotice(outcome.duplicate ? "duplicate" : null);
        return true;
      } catch (cause) {
        setError(String(cause));
        return false;
      } finally {
        setBusy(false);
      }
    },
    [showNotice],
  );

  const cancel = useCallback(async (id: string) => {
    setBusy(true);
    try {
      setEntries(await cancelConfession(id));
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const forget = useCallback(async () => {
    setBusy(true);
    try {
      setEntries(await forgetFinishedConfessions());
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  return { entries, busy, notice, error, add, cancel, forget, reload };
}
