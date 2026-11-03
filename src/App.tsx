// Root screen: the frame (title bar, confessional drawer, settings sheet) plus
// the chapel scene. All state that the Rust shell owns is loaded once on mount.
import { useCallback, useEffect, useMemo, useState } from "react";

import { Chapel } from "./components/Chapel";
import { SettingsPanel } from "./components/SettingsPanel";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { useChapel } from "./hooks/useChapel";
import { usePreferences } from "./hooks/usePreferences";
import { useTranslate } from "./i18n";
import { listProviders, listReligions } from "./lib/bridge";
import {
  createConfession,
  loadConfessions,
  saveConfessions,
} from "./lib/confessional";
import type { Confession, ProviderView, ReligionProfile } from "./lib/types";

export default function App() {
  const { preferences, update } = usePreferences();
  const t = useTranslate(preferences.language);

  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [religions, setReligions] = useState<ReligionProfile[]>([]);
  const [confessions, setConfessions] = useState<Confession[]>(() =>
    loadConfessions(),
  );
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    let active = true;
    listReligions()
      .then((loaded) => {
        if (active) {
          setReligions(loaded);
        }
      })
      .catch(() => undefined);
    listProviders()
      .then((loaded) => {
        if (active) {
          setProviders(loaded);
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = preferences.theme;
    root.lang = preferences.language;
  }, [preferences.theme, preferences.language]);

  const religion = useMemo(
    () =>
      religions.find((item) => item.id === preferences.religion) ??
      religions[0] ??
      null,
    [religions, preferences.religion],
  );

  const enabled = providers.some((provider) => provider.configured);

  const chapel = useChapel({
    clerics: preferences.clerics,
    religion: religion?.id ?? "",
    language: preferences.language,
    enabled,
  });

  const addConfession = useCallback((text: string) => {
    const entry = createConfession(text);
    if (!entry) {
      return;
    }
    setConfessions((current) => {
      const next = [entry, ...current];
      saveConfessions(next);
      return next;
    });
  }, []);

  const removeConfession = useCallback((id: string) => {
    setConfessions((current) => {
      const next = current.filter((entry) => entry.id !== id);
      saveConfessions(next);
      return next;
    });
  }, []);

  // Escape closes the topmost layer, as expected from a chat-like application.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      if (settingsOpen) {
        setSettingsOpen(false);
      } else if (sidebarOpen) {
        setSidebarOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [settingsOpen, sidebarOpen]);

  const openSettings = useCallback(() => {
    setSidebarOpen(false);
    setSettingsOpen(true);
  }, []);

  return (
    <div className="app">
      <TitleBar
        t={t}
        sidebarOpen={sidebarOpen}
        onToggleSidebar={() => setSidebarOpen((value) => !value)}
      />

      <div className="app__body">
        <Sidebar
          t={t}
          open={sidebarOpen}
          onClose={() => setSidebarOpen(false)}
          confessions={confessions}
          onAdd={addConfession}
          onRemove={removeConfession}
          onOpenSettings={openSettings}
        />

        <Chapel
          t={t}
          language={preferences.language}
          religion={religion}
          clerics={preferences.clerics}
          bubbles={chapel.bubbles}
          activeIndex={chapel.activeIndex}
          error={chapel.error}
          enabled={enabled}
          onOpenSettings={openSettings}
          onRetry={chapel.reload}
        />

        <SettingsPanel
          t={t}
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          preferences={preferences}
          onChange={update}
          providers={providers}
          religions={religions}
          onProviders={setProviders}
          onResetPrayers={chapel.reload}
        />
      </div>
    </div>
  );
}
