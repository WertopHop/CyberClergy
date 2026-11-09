// Root screen: the frame (title bar, confessional drawer, settings sheet) plus
// the chapel scene. Everything stateful lives in Rust; this component wires the
// hooks together and keeps the window chrome honest.
import { useEffect, useMemo, useState } from "react";

import { Chapel } from "./components/Chapel";
import { SettingsPanel } from "./components/SettingsPanel";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { useChapel } from "./hooks/useChapel";
import { useConfessions } from "./hooks/useConfessions";
import { usePreferences } from "./hooks/usePreferences";
import { localeFor, useTranslate } from "./i18n";
import { listProviders, listReligions } from "./lib/bridge";
import { cloudRoster } from "./lib/providers";
import type { ProviderView, ReligionProfile } from "./lib/types";

export default function App() {
  const { preferences, update } = usePreferences();
  const t = useTranslate(preferences.language);

  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [religions, setReligions] = useState<ReligionProfile[]>([]);
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
    // Hebrew flips the whole shell; the flex layouts follow `dir` on their own.
    root.dir = localeFor(preferences.language).dir;
  }, [preferences.theme, preferences.language]);

  const religion = useMemo(
    () =>
      religions.find((item) => item.id === preferences.religion) ??
      religions[0] ??
      null,
    [religions, preferences.religion],
  );

  // Robots standing on the cloud. The backend resolves the same rule, so slot
  // `i` here is slot `i` in the scheduler and the confessional queue.
  const configured = useMemo(
    () =>
      providers
        .filter((provider) => provider.configured)
        .map((provider) => provider.id),
    [providers],
  );
  const roster = useMemo(
    () => cloudRoster(preferences.roster, configured),
    [preferences.roster, configured],
  );

  const enabled = configured.length > 0;

  const chapel = useChapel({
    enabled,
    // The chosen rhythm is a pause after a prayer, and the hook owns the typing.
    pauseSecs: preferences.interval_secs,
  });
  const confessions = useConfessions();
  const reloadConfessions = confessions.reload;

  // Counters move while the drawer is open: refresh as soon as it appears.
  useEffect(() => {
    if (sidebarOpen) {
      reloadConfessions();
    }
  }, [sidebarOpen, reloadConfessions]);

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
          robots={roster}
          entries={confessions.entries}
          busy={confessions.busy}
          notice={confessions.notice}
          error={confessions.error}
          onAdd={confessions.add}
          onCancel={(id) => void confessions.cancel(id)}
          onForget={() => void confessions.forget()}
          onOpenSettings={() => {
            setSidebarOpen(false);
            setSettingsOpen(true);
          }}
        />

        <Chapel
          t={t}
          language={preferences.language}
          religion={religion}
          roster={roster}
          bubbles={chapel.bubbles}
          activeIndex={chapel.activeIndex}
          status={chapel.status}
          enabled={enabled}
          onOpenSettings={() => setSettingsOpen(true)}
        />

        <SettingsPanel
          t={t}
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          preferences={preferences}
          onChange={update}
          providers={providers}
          roster={roster}
          religions={religions}
          status={chapel.status}
          onProviders={setProviders}
          onReligions={setReligions}
        />
      </div>
    </div>
  );
}
