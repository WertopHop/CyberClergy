// Custom window chrome: the window is decorationless, so dragging, minimizing,
// maximizing and closing are provided here through the Tauri window API.
// Dragging is bound to the whole bar through an explicit handler: the attribute
// based drag region only covers the element it sits on, which left most of the
// bar dead between the menu button and the window controls.
import { useEffect, useState } from "react";
import type { MouseEvent } from "react";
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

  // Controls keep their own behaviour; everything else in the bar drags the
  // window. A double click toggles maximize like the native title bar does.
  const onBarMouseDown = (event: MouseEvent<HTMLElement>) => {
    if (!appWindow || event.button !== 0) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a, input, textarea, select, [data-no-drag]")) {
      return;
    }
    if (event.detail === 2) {
      toggleMaximize();
      return;
    }
    appWindow.startDragging().catch(() => undefined);
  };

  return (
    <header className="titlebar" onMouseDown={onBarMouseDown}>
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

      <div className="titlebar__title">
        <span className="titlebar__mark" aria-hidden="true" />
        <span>{t("appName")}</span>
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
