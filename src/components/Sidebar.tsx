// Slide-out left panel: the confessional. Entries are stored locally and the
// scheduling of prayer over them arrives with the scheduler stage.
import { useState } from "react";

import type { Translate } from "../i18n";
import type { Confession } from "../lib/types";
import { IconClose, IconFlame, IconGear, IconTrash } from "./Icons";

interface SidebarProps {
  t: Translate;
  open: boolean;
  onClose: () => void;
  confessions: Confession[];
  onAdd: (text: string) => void;
  onRemove: (id: string) => void;
  onOpenSettings: () => void;
}

export function Sidebar({
  t,
  open,
  onClose,
  confessions,
  onAdd,
  onRemove,
  onOpenSettings,
}: SidebarProps) {
  const [draft, setDraft] = useState("");

  const submit = () => {
    if (!draft.trim()) {
      return;
    }
    onAdd(draft);
    setDraft("");
  };

  return (
    <>
      <div
        className={`scrim ${open ? "is-open" : ""}`}
        onClick={onClose}
        aria-hidden="true"
      />
      <aside
        className={`sidebar ${open ? "is-open" : ""}`}
        aria-hidden={!open}
        aria-label={t("confessionalTitle")}
      >
        <header className="sidebar__header">
          <div className="sidebar__heading">
            <span className="sidebar__glyph" aria-hidden="true">
              <IconFlame />
            </span>
            <div>
              <h2 className="sidebar__title">{t("confessionalTitle")}</h2>
              <p className="sidebar__hint">{t("confessionalHint")}</p>
            </div>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            aria-label={t("close")}
            title={t("close")}
          >
            <IconClose />
          </button>
        </header>

        <div className="sidebar__list">
          {confessions.length === 0 ? (
            <p className="sidebar__empty">{t("confessionalEmpty")}</p>
          ) : (
            confessions.map((entry) => (
              <article className="confession" key={entry.id}>
                <p className="confession__text">{entry.text}</p>
                <div className="confession__meta">
                  <span className="confession__badge">
                    {t("confessionalPending")}
                  </span>
                  <button
                    type="button"
                    className="icon-button icon-button--small"
                    onClick={() => onRemove(entry.id)}
                    aria-label={t("confessionalRemove")}
                    title={t("confessionalRemove")}
                  >
                    <IconTrash />
                  </button>
                </div>
              </article>
            ))
          )}
        </div>

        <div className="sidebar__composer">
          <textarea
            className="sidebar__input"
            value={draft}
            rows={3}
            maxLength={400}
            placeholder={t("confessionalPlaceholder")}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                submit();
              }
            }}
          />
          <button
            type="button"
            className="button button--primary"
            onClick={submit}
            disabled={!draft.trim()}
          >
            {t("confessionalAdd")}
          </button>
        </div>

        <p className="sidebar__note">{t("confessionalNote")}</p>

        <footer className="sidebar__footer">
          <button type="button" className="sidebar__settings" onClick={onOpenSettings}>
            <IconGear />
            <span>{t("settings")}</span>
          </button>
        </footer>
      </aside>
    </>
  );
}
