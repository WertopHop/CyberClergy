// The main screen: a cloud, a handful of praying clerics, and a prayer floating
// above whoever is currently speaking. The cloud is a symmetric stack of blurred
// SVG blobs, which keeps the composition even at any aspect ratio.
import type { Translate } from "../i18n";
import type { Dictionary } from "../i18n/locales/en";
import type { Bubble } from "../hooks/useChapel";
import type { ChapelStatus, ReligionProfile, Tempo, ThemeId } from "../lib/types";
import { PrayerBubble } from "./PrayerBubble";
import { Robot } from "./Robot";

interface ChapelProps {
  t: Translate;
  language: string;
  religion: ReligionProfile | null;
  clerics: number;
  bubbles: Record<number, Bubble>;
  activeIndex: number | null;
  status: ChapelStatus | null;
  enabled: boolean;
  onOpenSettings: () => void;
}

interface Puff {
  cx: number;
  cy: number;
  r: number;
}

// Mirrored around x = 600: three strata plus a base, so the mound reads as one
// soft bank rather than scattered dots.
const FAR: Puff[] = [
  { cx: 300, cy: 362, r: 132 },
  { cx: 600, cy: 332, r: 158 },
  { cx: 900, cy: 362, r: 132 },
];

const MID: Puff[] = [
  { cx: 180, cy: 470, r: 150 },
  { cx: 400, cy: 432, r: 170 },
  { cx: 600, cy: 414, r: 176 },
  { cx: 800, cy: 432, r: 170 },
  { cx: 1020, cy: 470, r: 150 },
];

const NEAR: Puff[] = [
  { cx: 150, cy: 620, r: 160 },
  { cx: 380, cy: 582, r: 190 },
  { cx: 600, cy: 566, r: 200 },
  { cx: 820, cy: 582, r: 190 },
  { cx: 1050, cy: 620, r: 160 },
];

const BASE: Puff[] = [
  { cx: 300, cy: 702, r: 170 },
  { cx: 600, cy: 692, r: 200 },
  { cx: 900, cy: 702, r: 170 },
];

function CloudBackdrop() {
  return (
    <div className="clouds" aria-hidden="true">
      <div className="clouds__sky" />
      <svg
        className="clouds__art"
        viewBox="0 0 1200 800"
        preserveAspectRatio="xMidYMid slice"
      >
        <defs>
          <filter id="cloud-veil" x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur stdDeviation="52" />
          </filter>
          <filter id="cloud-soft" x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur stdDeviation="30" />
          </filter>
        </defs>

        <g filter="url(#cloud-veil)" className="clouds__layer clouds__layer--far">
          {FAR.map((puff) => (
            <circle key={`${puff.cx}-${puff.cy}`} cx={puff.cx} cy={puff.cy} r={puff.r} />
          ))}
        </g>

        <g filter="url(#cloud-soft)" className="clouds__layer clouds__layer--mid">
          {MID.map((puff) => (
            <circle key={`${puff.cx}-${puff.cy}`} cx={puff.cx} cy={puff.cy} r={puff.r} />
          ))}
        </g>

        <g filter="url(#cloud-soft)" className="clouds__layer clouds__layer--near">
          {NEAR.map((puff) => (
            <circle key={`${puff.cx}-${puff.cy}`} cx={puff.cx} cy={puff.cy} r={puff.r} />
          ))}
        </g>

        <g filter="url(#cloud-soft)" className="clouds__layer clouds__layer--base">
          {BASE.map((puff) => (
            <circle key={`${puff.cx}-${puff.cy}`} cx={puff.cx} cy={puff.cy} r={puff.r} />
          ))}
        </g>

        <g filter="url(#cloud-veil)" className="clouds__layer clouds__layer--lit">
          <ellipse cx="830" cy="360" rx="210" ry="120" />
          <ellipse cx="560" cy="430" rx="150" ry="80" />
        </g>
      </svg>
    </div>
  );
}

/** Splits the clerics into symmetric rows: three at the back, two per row after. */
function layoutRows(clerics: number): number[] {
  const rows: number[] = [];
  let remaining = Math.max(clerics, 1);
  while (remaining > 0) {
    const size = Math.min(remaining, rows.length === 0 ? 3 : 2);
    rows.push(size);
    remaining -= size;
  }
  return rows;
}

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

  let robotIndex = 0;

  return (
    <main className="chapel">
      <CloudBackdrop />

      <header className="chapel__caption">
        <h1 className="chapel__tradition">{traditionName}</h1>
        <p className="chapel__meta">{t("chapelMeta", { count: clerics })}</p>
      </header>

      <StatusBar t={t} status={status} />

      <div className="chapel__stacks">
        {rows.map((size, rowIndex) => (
          <div className="chapel__row" key={`row-${rowIndex}`}>
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
                  style={{ zIndex: rowIndex + 2 }}
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
                    scale={0.94 + rowIndex * 0.12}
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
