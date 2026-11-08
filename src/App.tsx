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
import { useTranslate } from "./i18n";
import { listProviders, listReligions } from "./lib/bridge";
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
  }, [preferences.theme, preferences.language]);

  const religion = useMemo(
    () =>
      religions.find((item) => item.id === preferences.religion) ??
      religions[0] ??
      null,
    [religions, preferences.religion],
  );

  const enabled = providers.some((provider) => provider.configured);

  const chapel = useChapel({ enabled });
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
          clerics={preferences.clerics}
          providers={providers}
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
          religions={religions}
          status={chapel.status}
          onProviders={setProviders}
        />
      </div>
    </div>
  );
}
