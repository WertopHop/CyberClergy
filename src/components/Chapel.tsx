// The main screen: a cloud, a handful of praying clerics, and a prayer floating
// above whoever is currently speaking. The cloud is a symmetric stack of blurred
// SVG blobs, which keeps the composition even at any aspect ratio.
import type { Translate } from "../i18n";
import type { Bubble } from "../hooks/useChapel";
import type { ReligionProfile } from "../lib/types";
import { PrayerBubble } from "./PrayerBubble";
import { Robot } from "./Robot";

interface ChapelProps {
  t: Translate;
  language: string;
  religion: ReligionProfile | null;
  clerics: number;
  bubbles: Record<number, Bubble>;
  activeIndex: number | null;
  error: string | null;
  enabled: boolean;
  onOpenSettings: () => void;
  onRetry: () => void;
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
      <div className="clouds__glow" />
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

export function Chapel({
  t,
  language,
  religion,
  clerics,
  bubbles,
  activeIndex,
  error,
  enabled,
  onOpenSettings,
  onRetry,
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
                      <PrayerBubble key={bubble.createdAt} text={bubble.text} />
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

      {error ? (
        <div className="chapel__error" role="status">
          <div className="chapel__error-copy">
            <span className="chapel__error-title">{t("chapelError")}</span>
            <p className="chapel__error-text">{error}</p>
          </div>
          <button type="button" className="button" onClick={onRetry}>
            {t("chapelRetry")}
          </button>
        </div>
      ) : null}
    </main>
  );
}
