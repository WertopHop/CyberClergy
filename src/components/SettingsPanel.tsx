// Right-hand settings sheet. Providers are edited one at a time: each row can be
// expanded, its credentials replaced, verified with a live request, or forgotten.
import { useState } from "react";

import type { Translate } from "../i18n";
import { checkProvider, clearProvider, saveProviderField } from "../lib/bridge";
import type {
  Preferences,
  ProviderField,
  ProviderId,
  ProviderView,
  ReligionProfile,
} from "../lib/types";
import { IconChevron, IconClose } from "./Icons";

const MIN_CLERICS = 3;
const MAX_CLERICS = 9;

interface SettingsPanelProps {
  t: Translate;
  open: boolean;
  onClose: () => void;
  preferences: Preferences;
  onChange: (patch: Partial<Preferences>) => void;
  providers: ProviderView[];
  religions: ReligionProfile[];
  onProviders: (providers: ProviderView[]) => void;
  onResetPrayers: () => void;
}

function providerLabel(id: ProviderId, t: Translate): string {
  switch (id) {
    case "openrouter":
      return t("providerOpenRouter");
    case "gemini":
      return t("providerGemini");
    case "groq":
      return t("providerGroq");
    case "mistral":
      return t("providerMistral");
    case "cloudflare":
      return t("providerCloudflare");
    case "nvidia":
      return t("providerNvidia");
    default:
      return id;
  }
}

function fieldLabel(name: string, t: Translate): string {
  if (name.endsWith("_API_KEY")) {
    return t("fieldApiKey");
  }
  if (name.endsWith("_API_TOKEN")) {
    return t("fieldToken");
  }
  if (name.endsWith("_ACCOUNT_ID")) {
    return t("fieldAccountId");
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
  onResetPrayers: () => void;
}

function ProviderRow({
  t,
  provider,
  expanded,
  onToggle,
  onProviders,
  onResetPrayers,
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
      onResetPrayers();
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
      onResetPrayers();
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
            {t("modelLine", { model: provider.model })}
          </p>

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
                    type="password"
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

export function SettingsPanel({
  t,
  open,
  onClose,
  preferences,
  onChange,
  providers,
  religions,
  onProviders,
  onResetPrayers,
}: SettingsPanelProps) {
  const [expandedProvider, setExpandedProvider] = useState<ProviderId | null>(
    null,
  );

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
                  onResetPrayers={onResetPrayers}
                />
              ))}
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
              <Segmented
                value={preferences.language}
                options={[
                  { value: "ru", label: t("languageRussian") },
                  { value: "en", label: t("languageEnglish") },
                ]}
                onChange={(language) => onChange({ language })}
              />
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
              <span className="row__label">{t("clerics")}</span>
              <div className="stepper">
                <button
                  type="button"
                  className="button button--small"
                  aria-label="-"
                  onClick={() =>
                    onChange({
                      clerics: Math.max(MIN_CLERICS, preferences.clerics - 1),
                    })
                  }
                >
                  −
                </button>
                <span className="stepper__value">{preferences.clerics}</span>
                <button
                  type="button"
                  className="button button--small"
                  aria-label="+"
                  onClick={() =>
                    onChange({
                      clerics: Math.min(MAX_CLERICS, preferences.clerics + 1),
                    })
                  }
                >
                  +
                </button>
              </div>
            </div>
          </section>
        </div>
      </section>
    </>
  );
}
