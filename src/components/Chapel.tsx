// The main screen: the cloud platform, a handful of praying clerics standing on
// it, and a prayer floating above whoever is currently speaking. Rows are
// symmetric (three at the back, two per row after) so the composition stays
// balanced whatever the number of clerics or the aspect ratio.
import type { Translate } from "../i18n";
import type { Dictionary } from "../i18n/locales/en";
import type { Bubble } from "../hooks/useChapel";
import type {
  ChapelStatus,
  ProviderId,
  ProviderView,
  ReligionProfile,
  Tempo,
  ThemeId,
} from "../lib/types";
import { CloudPlatform } from "./CloudPlatform";
import { PrayerBubble } from "./PrayerBubble";
import { Robot } from "./Robot";

interface ChapelProps {
  t: Translate;
  language: string;
  religion: ReligionProfile | null;
  clerics: number;
  /** Configured providers; their looks are handed out to the clerics in order. */
  providers: ProviderView[];
  bubbles: Record<number, Bubble>;
  activeIndex: number | null;
  status: ChapelStatus | null;
  enabled: boolean;
  onOpenSettings: () => void;
}

/** Splits the clerics into symmetric rows of three, one platform tier each. */
function layoutRows(clerics: number): number[] {
  const rows: number[] = [];
  let remaining = Math.max(clerics, 1);
  while (remaining > 0) {
    const size = Math.min(remaining, 3);
    rows.push(size);
    remaining -= size;
  }
  return rows;
}

/** Height of the front tier in the scene, in percent from the bottom. */
const ROW_BASE = 35;
/** How much higher every row behind the front one stands. Matches the tiers. */
const ROW_STEP = 10;

const THEME_KEYS: Record<ThemeId, keyof Dictionary> = {
  guilt: "themeGuilt",
  grief: "themeGrief",
  illness: "themeIllness",
  gratitude: "themeGratitude",
  family: "themeFamily",
  habit: "themeHabit",
  work: "themeWork",
  anger: "themeAnger",
  fear: "themeFear",
  loneliness: "themeLoneliness",
  doubt: "themeDoubt",
  unnamed: "themeUnnamed",
};

const TEMPO_KEYS: Record<Tempo, keyof Dictionary> = {
  steady: "tempoSteady",
  cautious: "tempoCautious",
  resting: "tempoResting",
  offline: "tempoOffline",
};

/** The counter, the theme and the source, when there is anything to say. */
function bubbleMeta(bubble: Bubble, t: Translate): string | null {
  const parts: string[] = [];
  if (bubble.ordinal !== null && bubble.target !== null) {
    parts.push(t("confessionalPraying", { done: bubble.ordinal, target: bubble.target }));
  }
  if (bubble.theme) {
    parts.push(t(THEME_KEYS[bubble.theme]));
  }
  if (bubble.source === "quiet") {
    parts.push(t("chapelQuiet"));
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}

function StatusBar({ t, status }: { t: Translate; status: ChapelStatus | null }) {
  if (!status || !status.configured) {
    return null;
  }
  return (
    <div className="chapel__status" role="status">
      <span className={`status-chip status-chip--${status.tempo}`}>
        <span className="status-chip__dot" aria-hidden="true" />
        {t(TEMPO_KEYS[status.tempo])}
      </span>
      {status.pending > 0 ? (
        <span className="status-chip status-chip--muted">
          {t("chapelPending", { count: status.pending })}
        </span>
      ) : null}
      <span className="status-chip status-chip--muted">
        {t("chapelBudget", {
          done: status.budget.done,
          limit: status.budget.limit,
        })}
      </span>
    </div>
  );
}

/** Quiet hours: the chapel keeps praying its canon while the networks rest. */
function QuietNotice({ t, status }: { t: Translate; status: ChapelStatus | null }) {
  if (!status || !status.configured) {
    return null;
  }
  if (status.tempo !== "resting" && status.tempo !== "offline") {
    return null;
  }
  return (
    <div className="chapel__notice" role="status">
      <span className="chapel__notice-title">{t("chapelQuietTitle")}</span>
      <p className="chapel__notice-text">
        {status.error ??
          (status.tempo === "resting" ? t("tempoRestingHint") : t("tempoOfflineHint"))}
      </p>
    </div>
  );
}

export function Chapel({
  t,
  language,
  religion,
  clerics,
  providers,
  bubbles,
  activeIndex,
  status,
  enabled,
  onOpenSettings,
}: ChapelProps) {
  const rows = layoutRows(clerics);
  const traditionName = religion
    ? religion.identity.names[language] ??
      religion.identity.names.en ??
      religion.id
    : "";

  // Until the roster binds each cleric to a network, the looks go round the
  // configured providers, which still shows every house on the cloud.
  const deck = providers
    .filter((provider) => provider.configured)
    .map((provider) => provider.id);
  const lookAt = (index: number): ProviderId | undefined =>
    deck.length > 0 ? deck[index % deck.length] : undefined;

  let robotIndex = 0;

  return (
    <main className="chapel">
      <CloudPlatform rows={rows.length} />

      <header className="chapel__caption">
        <h1 className="chapel__tradition">{traditionName}</h1>
        <p className="chapel__meta">{t("chapelMeta", { count: clerics })}</p>
      </header>

      <StatusBar t={t} status={status} />

      <div className="chapel__stacks">
        {rows.map((size, rowIndex) => (
          <div
            className="chapel__row"
            key={`row-${rowIndex}`}
            style={{
              bottom: `${ROW_BASE + (rows.length - 1 - rowIndex) * ROW_STEP}%`,
              zIndex: 2 + rowIndex,
            }}
          >
            {Array.from({ length: size }, () => {
              const index = robotIndex;
              robotIndex += 1;
              const bubble = bubbles[index];
              return (
                <div
                  className={`chapel__cell ${
                    activeIndex === index ? "is-praying" : ""
                  }`}
                  key={`cleric-${index}`}
                >
                  <div className="chapel__bubble-slot">
                    {bubble ? (
                      <PrayerBubble
                        key={bubble.at}
                        text={bubble.text}
                        meta={bubbleMeta(bubble, t)}
                        quiet={bubble.source === "quiet"}
                      />
                    ) : null}
                  </div>
                  <Robot
                    praying={activeIndex === index}
                    seed={index + 1}
                    scale={0.86 + rowIndex * 0.11}
                    variant={lookAt(index)}
                  />
                </div>
              );
            })}
          </div>
        ))}
      </div>

      <QuietNotice t={t} status={status} />

      {!enabled ? (
        <div className="empty-card">
          <h2 className="empty-card__title">{t("chapelEmptyTitle")}</h2>
          <p className="empty-card__body">{t("chapelEmptyBody")}</p>
          <button
            type="button"
            className="button button--primary"
            onClick={onOpenSettings}
          >
            {t("chapelEmptyAction")}
          </button>
        </div>
      ) : null}
    </main>
  );
}
