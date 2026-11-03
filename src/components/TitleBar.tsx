// Custom window chrome: the window is decorationless, so dragging, minimizing,
// maximizing and closing are provided here through the Tauri window API.
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { Translate } from "../i18n";
import { isDesktop } from "../lib/bridge";
import {
  IconClose,
  IconMenu,
  IconMinimize,
  IconRestore,
  IconSquare,
} from "./Icons";

interface TitleBarProps {
  t: Translate;
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
}

export function TitleBar({ t, sidebarOpen, onToggleSidebar }: TitleBarProps) {
  const [maximized, setMaximized] = useState(false);
  const appWindow = isDesktop ? getCurrentWindow() : null;

  useEffect(() => {
    if (!appWindow) {
      return;
    }
    let active = true;
    let unlisten: (() => void) | undefined;

    const sync = () => {
      appWindow
        .isMaximized()
        .then((value) => {
          if (active) {
            setMaximized(value);
          }
        })
        .catch(() => undefined);
    };

    sync();
    appWindow
      .onResized(sync)
      .then((stop) => {
        if (active) {
          unlisten = stop;
        } else {
          stop();
        }
      })
      .catch(() => undefined);

    return () => {
      active = false;
      unlisten?.();
    };
  }, [appWindow]);

  const toggleMaximize = () => {
    appWindow?.toggleMaximize().catch(() => undefined);
  };

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar__side">
        <button
          type="button"
          className="icon-button"
          onClick={onToggleSidebar}
          aria-label={t("menu")}
          aria-expanded={sidebarOpen}
          title={t("menu")}
        >
          <IconMenu />
        </button>
      </div>

      <div
        className="titlebar__title"
        data-tauri-drag-region
        onDoubleClick={toggleMaximize}
      >
        <span className="titlebar__mark" aria-hidden="true" />
        <span data-tauri-drag-region>{t("appName")}</span>
      </div>

      <div className="titlebar__side titlebar__side--end">
        <button
          type="button"
          className="winbutton"
          onClick={() => appWindow?.minimize().catch(() => undefined)}
          disabled={!appWindow}
          aria-label={t("minimize")}
          title={appWindow ? t("minimize") : t("desktopOnly")}
        >
          <IconMinimize />
        </button>
        <button
          type="button"
          className="winbutton"
          onClick={toggleMaximize}
          disabled={!appWindow}
          aria-label={maximized ? t("restore") : t("maximize")}
          title={maximized ? t("restore") : t("maximize")}
        >
          {maximized ? <IconRestore /> : <IconSquare />}
        </button>
        <button
          type="button"
          className="winbutton winbutton--close"
          onClick={() => appWindow?.close().catch(() => undefined)}
          disabled={!appWindow}
          aria-label={t("close")}
          title={appWindow ? t("close") : t("desktopOnly")}
        >
          <IconClose />
        </button>
      </div>
    </header>
  );
}
