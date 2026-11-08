// Slide-out left panel: the confessional. Entries are owned by Rust: the queue
// assigns each one to a cleric, the scheduler counts prayers down, and the list
// is re-read on a slow poll so the counters move while the drawer is open.
import { useState } from "react";

import type { Translate } from "../i18n";
import type { ConfessionRequest } from "../lib/bridge";
import { MAX_REPEAT, providerLabel } from "../lib/providers";
import type { ConfessionEntry, IntercessionStatus, ProviderId } from "../lib/types";
import { IconClose, IconFlame, IconGear, IconTrash } from "./Icons";

const MAX_LENGTH = 1000;
/** Repeat count offered before the person touches the field. */
const DEFAULT_REPEAT = 5;

interface SidebarProps {
  t: Translate;
  open: boolean;
  onClose: () => void;
  /** Robots on the cloud: only they can be asked for by name. */
  robots: ProviderId[];
  entries: ConfessionEntry[];
  busy: boolean;
  notice: "duplicate" | null;
  error: string | null;
  onAdd: (text: string, request: ConfessionRequest) => Promise<boolean>;
  onCancel: (id: string) => void;
  onForget: () => void;
  onOpenSettings: () => void;
}

function clampRepeat(value: string): number {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    return 1;
  }
  return Math.min(parsed, MAX_REPEAT);
}

function statusLabel(status: IntercessionStatus, t: Translate): string {
  switch (status) {
    case "fulfilled":
      return t("confessionalDone");
    case "cancelled":
      return t("confessionalCancelled");
    default:
      return t("confessionalPending");
  }
}

function progress(entry: ConfessionEntry): number {
  if (entry.prayers_target <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((entry.prayers_done / entry.prayers_target) * 100));
}

export function Sidebar({
  t,
  open,
  onClose,
  robots,
  entries,
  busy,
  notice,
  error,
  onAdd,
  onCancel,
  onForget,
  onOpenSettings,
}: SidebarProps) {
  const [draft, setDraft] = useState("");
  const [repeat, setRepeat] = useState(DEFAULT_REPEAT);
  const [endless, setEndless] = useState(false);
  const [platform, setPlatform] = useState<ProviderId | "">("");

  // A robot that left the cloud cannot be asked for by name any more.
  const pinned = robots.includes(platform as ProviderId) ? platform : "";

  const submit = async () => {
    if (!draft.trim() || busy) {
      return;
    }
    const accepted = await onAdd(draft, {
      repeat: endless ? null : repeat,
      endless,
      provider: pinned === "" ? null : pinned,
    });
    if (accepted) {
      setDraft("");
    }
  };

  // Newest first: the last thing written is the one still on the person's mind.
  const ordered = [...entries].reverse();
  const hasFinished = entries.some((entry) => entry.status !== "queued");

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

        {hasFinished ? (
          <div className="sidebar__toolbar">
            <button
              type="button"
              className="button button--small button--ghost"
              disabled={busy}
              onClick={onForget}
            >
              {t("confessionalForget")}
            </button>
          </div>
        ) : null}

        <div className="sidebar__list">
          {ordered.length === 0 ? (
            <p className="sidebar__empty">{t("confessionalEmpty")}</p>
          ) : (
            ordered.map((entry) => {
              const active = entry.status === "queued";
              return (
                <article
                  className={`confession ${active ? "" : "confession--done"}`}
                  key={entry.id}
                >
                  <p className="confession__text">{entry.text}</p>
                  <div className="confession__meta">
                    <span
                      className={`confession__badge confession__badge--${entry.status}`}
                    >
                      {statusLabel(entry.status, t)}
                    </span>
                    {active ? (
                      <span className="confession__count">
                        {entry.endless
                          ? t("confessionalEndlessCount", {
                              done: entry.prayers_done,
                            })
                          : t("confessionalPraying", {
                              done: entry.prayers_done,
                              target: entry.prayers_target,
                            })}
                      </span>
                    ) : null}
                    {active && entry.provider ? (
                      <span
                        className={`confession__robot ${
                          robots.includes(entry.provider) ? "" : "is-waiting"
                        }`}
                      >
                        {robots.includes(entry.provider)
                          ? providerLabel(entry.provider, t)
                          : t("confessionalWaiting", {
                              provider: providerLabel(entry.provider, t),
                            })}
                      </span>
                    ) : null}
                    {active ? (
                      <button
                        type="button"
                        className="icon-button icon-button--small"
                        onClick={() => onCancel(entry.id)}
                        disabled={busy}
                        aria-label={t("confessionalCancel")}
                        title={t("confessionalCancel")}
                      >
                        <IconTrash />
                      </button>
                    ) : null}
                  </div>
                  {active && !entry.endless ? (
                    <div
                      className="confession__bar"
                      role="progressbar"
                      aria-valuemin={0}
                      aria-valuemax={entry.prayers_target}
                      aria-valuenow={entry.prayers_done}
                    >
                      <span style={{ width: `${progress(entry)}%` }} />
                    </div>
                  ) : null}
                </article>
              );
            })
          )}
        </div>

        <div className="sidebar__composer">
          <textarea
            className="sidebar__input"
            value={draft}
            rows={3}
            maxLength={MAX_LENGTH}
            placeholder={t("confessionalPlaceholder")}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void submit();
              }
            }}
          />
          <div className="composer-options">
            <label className="composer-option">
              <span className="composer-option__label">{t("confessionalRepeat")}</span>
              <input
                className="composer-option__number"
                type="number"
                min={1}
                max={MAX_REPEAT}
                value={repeat}
                disabled={endless}
                onChange={(event) => setRepeat(clampRepeat(event.target.value))}
              />
            </label>
            <button
              type="button"
              className={`chip-toggle ${endless ? "is-on" : ""}`}
              aria-pressed={endless}
              onClick={() => setEndless((value) => !value)}
            >
              {t("confessionalEndless")}
            </button>
            <label className="composer-option composer-option--grow">
              <span className="composer-option__label">
                {t("confessionalPlatform")}
              </span>
              <select
                className="composer-option__select"
                value={pinned}
                onChange={(event) =>
                  setPlatform(event.target.value as ProviderId | "")
                }
              >
                <option value="">{t("confessionalAnyRobot")}</option>
                {robots.map((id) => (
                  <option key={id} value={id}>
                    {providerLabel(id, t)}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {notice ? <p className="sidebar__notice">{t("confessionalDuplicate")}</p> : null}
          {error ? <p className="sidebar__notice sidebar__notice--error">{error}</p> : null}
          <button
            type="button"
            className="button button--primary"
            onClick={() => void submit()}
            disabled={busy || !draft.trim()}
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
