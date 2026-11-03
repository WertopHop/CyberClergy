// Preferences are owned by the Rust shell. The hook keeps a local mirror so the
// interface reacts instantly, then adopts whatever the backend saved after it
// sanitized the values.
import { useCallback, useEffect, useRef, useState } from "react";

import {
  DEFAULT_PREFERENCES,
  loadPreferences,
  savePreferences,
} from "../lib/bridge";
import type { Preferences } from "../lib/types";

export interface PreferencesState {
  preferences: Preferences;
  ready: boolean;
  update: (patch: Partial<Preferences>) => void;
}

export function usePreferences(): PreferencesState {
  const [preferences, setPreferences] = useState<Preferences>(
    DEFAULT_PREFERENCES,
  );
  const [ready, setReady] = useState(false);
  const latest = useRef(preferences);

  useEffect(() => {
    let active = true;
    loadPreferences()
      .then((loaded) => {
        if (!active) {
          return;
        }
        latest.current = loaded;
        setPreferences(loaded);
      })
      .catch(() => undefined)
      .finally(() => {
        if (active) {
          setReady(true);
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const update = useCallback((patch: Partial<Preferences>) => {
    const next = { ...latest.current, ...patch };
    latest.current = next;
    setPreferences(next);
    void savePreferences(next)
      .then((stored) => {
        latest.current = stored;
        setPreferences(stored);
      })
      .catch(() => undefined);
  }, []);

  return { preferences, ready, update };
}
