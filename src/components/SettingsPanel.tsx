// Right-hand settings sheet. Providers are edited one at a time: each row can be
// expanded, its credentials replaced, verified with a live request, or forgotten.
// The sheet also shows how the chapel keeps its rhythm and what the journal has
// counted so far.
import { useEffect, useState } from "react";

import { LOCALES, type Translate } from "../i18n";
import { checkProvider, clearProvider, prayerStats, saveProviderField } from "../lib/bridge";
import { MAX_ROBOTS, providerLabel, providerName } from "../lib/providers";
import type {
  ChapelStatus,
  Pace,
  PrayerStats,
  Preferences,
  ProviderField,
  ProviderId,
  ProviderView,
  ReligionProfile,
} from "../lib/types";
import { IconChevron, IconClose } from "./Icons";

interface SettingsPanelProps {
  t: Translate;
  open: boolean;
  onClose: () => void;
  preferences: Preferences;
  onChange: (patch: Partial<Preferences>) => void;
  providers: ProviderView[];
  /** Effective roster: configured networks currently standing on the cloud. */
  roster: ProviderId[];
  religions: ReligionProfile[];
  status: ChapelStatus | null;
  onProviders: (providers: ProviderView[]) => void;
}

function fieldLabel(name: string, t: Translate): string {
  if (name.endsWith("_API_KEY")) {
    return t("fieldApiKey");
  }
  if (name.endsWith("_API_TOKEN") || name.endsWith("_TOKEN")) {
    return t("fieldToken");
  }
  if (name.endsWith("_ACCOUNT_ID")) {
    return t("fieldAccountId");
  }
  if (name.endsWith("_BASE_URL")) {
    return t("fieldBaseUrl");
  }
  if (name.endsWith("_MODEL")) {
    return t("fieldModel");
  }
  return name;
}

interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: Array<SegmentedOption<T>>;
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className={`segmented__item ${
            option.value === value ? "is-active" : ""
          }`}
          aria-pressed={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

interface ProviderRowProps {
  t: Translate;
  provider: ProviderView;
  expanded: boolean;
  onToggle: () => void;
  onProviders: (providers: ProviderView[]) => void;
}

function ProviderRow({
  t,
  provider,
  expanded,
  onToggle,
  onProviders,
}: ProviderRowProps) {
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const updateDraft = (name: string, value: string) => {
    setDrafts((current) => ({ ...current, [name]: value }));
  };

  const save = async (field: ProviderField) => {
    setBusy(true);
    try {
      const updated = await saveProviderField(
        provider.id,
        field.name,
        drafts[field.name] ?? "",
      );
      onProviders(updated);
      updateDraft(field.name, "");
      setNotice(t("saved"));
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  };

  const forget = async () => {
    setBusy(true);
    try {
      const updated = await clearProvider(provider.id);
      onProviders(updated);
      setNotice(null);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  };

  const saveModel = async () => {
    setBusy(true);
    try {
      const updated = await saveProviderField(
        provider.id,
        provider.model_var,
        drafts[provider.model_var] ?? "",
      );
      onProviders(updated);
      updateDraft(provider.model_var, "");
      setNotice(t("saved"));
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  };

  // An empty value removes the override, so the built-in default returns.
  const resetModel = async () => {
    setBusy(true);
    try {
      const updated = await saveProviderField(provider.id, provider.model_var, "");
      onProviders(updated);
      setNotice(null);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  };

  const check = async () => {
    setBusy(true);
    setNotice(t("checking"));
    try {
      const outcome = await checkProvider(provider.id);
      setNotice(t("checkOk", { ms: outcome.elapsed_ms }));
    } catch (error) {
      setNotice(t("checkFailed", { message: String(error) }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={`provider ${expanded ? "is-open" : ""}`}>
      <button
        type="button"
        className="provider__head"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <span
          className={`provider__dot ${provider.configured ? "is-on" : ""}`}
          aria-hidden="true"
        />
        <span className="provider__name">{providerLabel(provider.id, t)}</span>
        <span className="provider__state">
          {provider.configured ? t("configured") : t("notConfigured")}
        </span>
        <span className="provider__chevron" aria-hidden="true">
          <IconChevron />
        </span>
      </button>

      {expanded ? (
        <div className="provider__body">
          <p className="provider__model">
            {provider.model && provider.model !== provider.default_model
              ? t("modelCustom", {
                  model: provider.model,
                  default: provider.default_model,
                })
              : t("modelLine", {
                  model: provider.model || provider.default_model || "—",
                })}
          </p>

          <label className="field">
            <span className="field__label">{t("fieldModel")}</span>
            <span className="field__row">
              <input
                className="field__input"
                type="text"
                autoComplete="off"
                spellCheck={false}
                value={drafts[provider.model_var] ?? ""}
                placeholder={provider.default_model || t("modelPlaceholder")}
                onChange={(event) =>
                  updateDraft(provider.model_var, event.target.value)
                }
              />
              <button
                type="button"
                className="button button--small"
                disabled={busy || !(drafts[provider.model_var] ?? "").trim()}
                onClick={() => void saveModel()}
              >
                {t("save")}
              </button>
            </span>
          </label>
          <div className="provider__actions">
            {provider.model !== provider.default_model ? (
              <button
                type="button"
                className="button button--small button--ghost"
                disabled={busy}
                onClick={() => void resetModel()}
              >
                {t("modelReset")}
              </button>
            ) : null}
            <span className="provider__notice">
              {t("modelDefault", { model: provider.default_model || "—" })}
            </span>
          </div>

          {provider.fields.map((field) => {
            const draft = drafts[field.name] ?? "";
            return (
              <label className="field" key={field.name}>
                <span className="field__label">
                  {fieldLabel(field.name, t)}
                </span>
                <span className="field__row">
                  <input
                    className="field__input"
                    type={field.secret ? "password" : "text"}
                    autoComplete="off"
                    spellCheck={false}
                    value={draft}
                    placeholder={
                      field.set ? t("placeholderSet") : t("placeholderEmpty")
                    }
                    onChange={(event) =>
                      updateDraft(field.name, event.target.value)
                    }
                  />
                  <button
                    type="button"
                    className="button button--small"
                    disabled={busy || !draft.trim()}
                    onClick={() => void save(field)}
                  >
                    {t("save")}
                  </button>
                </span>
              </label>
            );
          })}

          <div className="provider__actions">
            <button
              type="button"
              className="button button--small"
              disabled={busy}
              onClick={() => void check()}
            >
              {t("check")}
            </button>
            <button
              type="button"
              className="button button--small button--ghost"
              disabled={busy}
              onClick={() => void forget()}
            >
              {t("forget")}
            </button>
            {notice ? <span className="provider__notice">{notice}</span> : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function StatsGroup({ t, open }: { t: Translate; open: boolean }) {
  const [stats, setStats] = useState<PrayerStats | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    prayerStats()
      .then((loaded) => {
        if (active) {
          setStats(loaded);
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [open]);

  return (
    <section className="group">
      <h3 className="group__title">{t("settingsStats")}</h3>
      <p className="group__hint">{t("settingsStatsHint")}</p>

      {!stats || stats.total === 0 ? (
        <p className="stats__empty">{t("statsEmpty")}</p>
      ) : (
        <>
          <div className="stats">
            <div className="stats__cell">
              <span className="stats__value">{stats.today}</span>
              <span className="stats__label">{t("statsLastDay")}</span>
            </div>
            <div className="stats__cell">
              <span className="stats__value">{stats.total}</span>
              <span className="stats__label">{t("statsTotal")}</span>
            </div>
            <div className="stats__cell">
              <span className="stats__value">
                {stats.prompt_tokens + stats.completion_tokens}
              </span>
              <span className="stats__label">{t("statsTokens")}</span>
            </div>
            <div className="stats__cell">
              <span className="stats__value">
                {t("statsMs", { ms: stats.average_ms })}
              </span>
              <span className="stats__label">{t("statsLatency")}</span>
            </div>
          </div>
          {stats.requests > 0 ? (
            <p className="stats__requests">
              {t("statsBatch", {
                prayers: stats.total,
                requests: stats.requests,
              })}
            </p>
          ) : null}

          {stats.failures > 0 ? (
            <p className="stats__failures">
              {t("statsFailures")}: {stats.failures}
            </p>
          ) : null}

          <ul className="stats__providers">
            {stats.providers.map((share) => (
              <li className="stats__provider" key={share.provider}>
                <span>{providerName(share.provider, t)}</span>
                <span className="stats__provider-count">{share.prayers}</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

export function SettingsPanel({
  t,
  open,
  onClose,
  preferences,
  onChange,
  providers,
  roster,
  religions,
  status,
  onProviders,
}: SettingsPanelProps) {
  const [expandedProvider, setExpandedProvider] = useState<ProviderId | null>(
    null,
  );

  // The stored roster is written back whole: after the first toggle `null`
  // ("never chosen") never returns, so an emptied cloud stays empty.
  const toggleRobot = (id: ProviderId) => {
    const next = roster.includes(id)
      ? roster.filter((candidate) => candidate !== id)
      : [...roster, id].slice(0, MAX_ROBOTS);
    onChange({ roster: next });
  };

  return (
    <>
      <div
        className={`scrim scrim--right ${open ? "is-open" : ""}`}
        onClick={onClose}
        aria-hidden="true"
      />
      <section
        className={`sheet ${open ? "is-open" : ""}`}
        aria-hidden={!open}
        aria-label={t("settingsTitle")}
      >
        <header className="sheet__header">
          <h2 className="sheet__title">{t("settingsTitle")}</h2>
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            aria-label={t("settingsClose")}
            title={t("settingsClose")}
          >
            <IconClose />
          </button>
        </header>

        <div className="sheet__body">
          <section className="group">
            <h3 className="group__title">{t("settingsNetworks")}</h3>
            <p className="group__hint">{t("settingsNetworksHint")}</p>
            <div className="providers">
              {providers.map((provider) => (
                <ProviderRow
                  key={provider.id}
                  t={t}
                  provider={provider}
                  expanded={expandedProvider === provider.id}
                  onToggle={() =>
                    setExpandedProvider((current) =>
                      current === provider.id ? null : provider.id,
                    )
                  }
                  onProviders={onProviders}
                />
              ))}
            </div>
            <div className="row">
              <span className="row__label">{t("settingsStorage")}</span>
              <span className="row__value">
                {status?.storage === "keychain"
                  ? t("storageKeychain")
                  : t("storageFile")}
              </span>
            </div>
          </section>

          <section className="group">
            <h3 className="group__title">{t("settingsRoster")}</h3>
            <p className="group__hint">{t("settingsRosterHint")}</p>
            <div className="roster">
              {providers.map((provider) => {
                const onCloud = roster.includes(provider.id);
                return (
                  <div
                    className={`roster__row ${
                      onCloud ? "is-on" : ""
                    } ${provider.configured ? "" : "is-locked"}`}
                    key={provider.id}
                  >
                    <label className="roster__pick">
                      <input
                        type="checkbox"
                        className="roster__check"
                        checked={onCloud}
                        disabled={!provider.configured}
                        onChange={() => toggleRobot(provider.id)}
                      />
                      <span className="roster__name">
                        {providerLabel(provider.id, t)}
                      </span>
                    </label>
                    <span className="roster__state">
                      {provider.configured
                        ? onCloud
                          ? t("rosterOnCloud")
                          : t("rosterBench")
                        : t("settingsRosterNeedsKey")}
                    </span>
                    {provider.configured ? null : (
                      <button
                        type="button"
                        className="roster__link"
                        onClick={() => setExpandedProvider(provider.id)}
                      >
                        {t("rosterAddKey")}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
            <div className="row">
              <span className="row__label">{t("rosterCountLabel")}</span>
              <span className="row__value">
                {t("settingsRosterCount", {
                  count: roster.length,
                  // Count networks, not cloud capacity: "1 of 6" answers the
                  // question the person is actually asking.
                  max: providers.length,
                })}
              </span>
            </div>
          </section>

          <section className="group">
            <h3 className="group__title">{t("settingsAppearance")}</h3>
            <div className="row">
              <span className="row__label">{t("theme")}</span>
              <Segmented
                value={preferences.theme}
                options={[
                  { value: "night", label: t("themeNight") },
                  { value: "dawn", label: t("themeDawn") },
                ]}
                onChange={(theme) => onChange({ theme })}
              />
            </div>
            <div className="row">
              <span className="row__label">{t("language")}</span>
              <select
                className="select"
                value={preferences.language}
                onChange={(event) =>
                  onChange({ language: event.target.value })
                }
              >
                {LOCALES.map((locale) => (
                  <option key={locale.code} value={locale.code}>
                    {locale.label}
                  </option>
                ))}
              </select>
            </div>
          </section>

          <section className="group">
            <h3 className="group__title">{t("settingsChapel")}</h3>
            <div className="row">
              <span className="row__label">{t("religion")}</span>
              <select
                className="select"
                value={preferences.religion}
                onChange={(event) =>
                  onChange({ religion: event.target.value })
                }
              >
                {religions.map((religion) => (
                  <option key={religion.id} value={religion.id}>
                    {religion.identity.names[preferences.language] ??
                      religion.identity.names.en ??
                      religion.id}
                  </option>
                ))}
              </select>
            </div>
            <div className="row">
              <span className="row__label">{t("pace")}</span>
              <Segmented<Pace>
                value={preferences.pace}
                options={[
                  { value: "calm", label: t("paceCalm") },
                  { value: "normal", label: t("paceNormal") },
                  { value: "lively", label: t("paceLively") },
                ]}
                onChange={(pace) => onChange({ pace })}
              />
            </div>
            <p className="row__hint">{t("paceHint")}</p>
          </section>

          <StatsGroup t={t} open={open} />
        </div>
      </section>
    </>
  );
}
