// Little praying cleric. Everything is inline SVG plus CSS keyframes, so the
// figure stays sharp at any window size, needs no sprite sheets or runtime
// animation library, and inherits the palette from the active theme.
//
// One mascot silhouette is shared by the whole clergy: oversized head with
// antenna ears, visor with two eyes, collar, hatch on the chest, hands meeting in
// a prayer gesture and a short cloak. The provider only recolours the trim and
// adds one detail, so the cloud reads as "same order, different houses".
import type { CSSProperties } from "react";

import type { ProviderId } from "../lib/types";
import { variantFor } from "./robotVariants";

interface RobotProps {
  praying: boolean;
  /** Index used to desynchronise the idle loops. */
  seed: number;
  scale?: number;
  /** Network this cleric prays with; picks the accent and the detail. */
  variant?: ProviderId;
}

export function Robot({ praying, seed, scale = 1, variant }: RobotProps) {
  const look = variantFor(variant);
  const phase = 0.7 + (seed % 5) * 0.31;

  return (
    <div
      className={`robot ${praying ? "robot--praying" : ""}`}
      style={
        {
          "--robot-scale": scale,
          "--robot-accent": look.accent,
        } as CSSProperties
      }
    >
      {/* The box ends at the feet: rows are positioned by their bottom edge, which
          has to be the surface the cleric stands on, not empty space below it. */}
      <svg viewBox="0 0 120 153" className="robot__art" aria-hidden="true">
        <ellipse className="robot__shadow" cx="60" cy="149" rx="30" ry="4.4" />

        <g
          className="robot__bob"
          style={{ animationDelay: `${(seed % 6) * -0.9}s` }}
        >
          {look.accessory === "wings" ? (
            <g className="robot__wings">
              <path d="M 41 101 C 27 89 14 91 10 105 C 21 105 27 113 39 115 Z" />
              <path d="M 79 101 C 93 89 106 91 110 105 C 99 105 93 113 81 115 Z" />
            </g>
          ) : null}

          <path
            className="robot__cloak"
            d="M 41 96 C 31 118 33 135 43 145 L 77 145 C 87 135 89 118 79 96 Z"
          />

          <g className="robot__legs">
            <rect
              className="robot__joint"
              x="45"
              y="126"
              width="9"
              height="20"
              rx="4.5"
              transform="rotate(6 49.5 126)"
            />
            <rect
              className="robot__joint"
              x="66"
              y="126"
              width="9"
              height="20"
              rx="4.5"
              transform="rotate(-6 70.5 126)"
            />
            <ellipse className="robot__foot" cx="47" cy="148" rx="9.5" ry="4.4" />
            <ellipse className="robot__foot" cx="73" cy="148" rx="9.5" ry="4.4" />
          </g>

          <rect className="robot__shell" x="38" y="88" width="44" height="44" rx="17" />
          <rect className="robot__edge" x="38" y="88" width="44" height="44" rx="17" />
          <rect className="robot__stole" x="57" y="92" width="6" height="36" rx="3" />
          {/* The hatch sits low on the body so the praying hands stay clear of it. */}
          <circle className="robot__hatch" cx="60" cy="117" r="8.5" />
          <circle className="robot__hatch-rim" cx="60" cy="117" r="8.5" />
          <g transform="translate(60 117)">
            <path className="robot__glyph" d={look.glyph} />
          </g>

          {/* Sleeves run from the shoulders inward, palms meet above the hatch. */}
          <g
            className="robot__arm robot__arm--left"
            style={{ animationDelay: `${phase}s` }}
          >
            <circle className="robot__shoulder" cx="39" cy="96" r="5.6" />
            <rect
              className="robot__joint"
              x="39"
              y="97"
              width="7.5"
              height="16"
              rx="3.7"
              transform="rotate(-25 42.75 97)"
            />
            <rect
              className="robot__hand"
              x="51.5"
              y="92"
              width="7.5"
              height="19"
              rx="3.7"
              transform="rotate(10 55.25 101.5)"
            />
          </g>
          <g
            className="robot__arm robot__arm--right"
            style={{ animationDelay: `${phase + 0.6}s` }}
          >
            <circle className="robot__shoulder" cx="81" cy="96" r="5.6" />
            <rect
              className="robot__joint"
              x="73.5"
              y="97"
              width="7.5"
              height="16"
              rx="3.7"
              transform="rotate(25 77.25 97)"
            />
            <rect
              className="robot__hand"
              x="61"
              y="92"
              width="7.5"
              height="19"
              rx="3.7"
              transform="rotate(-10 64.75 101.5)"
            />
          </g>

          {/* Neck first, collar over it: the collar then hides the seam between
              the oversized head and the short body. */}
          <rect className="robot__joint" x="54" y="70" width="12" height="14" rx="6" />
          <rect className="robot__collar" x="40" y="76" width="40" height="11" rx="5.5" />

          {look.accessory === "hood" ? (
            <path
              className="robot__hood"
              d="M 32 62 C 32 26 44 16 60 16 C 76 16 88 26 88 62 C 88 76 76 82 60 82 C 44 82 32 76 32 62 Z"
            />
          ) : null}

          <g className="robot__antenna">
            <path className="robot__wire" d="M 38 31 C 34 26 33 20 32 15" />
            <path className="robot__wire" d="M 82 31 C 86 26 87 20 88 15" />
            <circle className="robot__tip" cx="31" cy="13" r="3.4" />
            <circle className="robot__tip" cx="89" cy="13" r="3.4" />
          </g>

          <rect className="robot__head" x="34" y="28" width="52" height="46" rx="21" />
          <rect className="robot__head-edge" x="34" y="28" width="52" height="46" rx="21" />

          {look.accessory === "crest" ? (
            <path className="robot__crest" d="M 54 30 C 55 18 63 12 70 9 C 67 19 66 25 63 30 Z" />
          ) : null}

          <rect className="robot__visor" x="42" y="42" width="36" height="18" rx="9" />
          <rect className="robot__visor-rim" x="42" y="42" width="36" height="18" rx="9" />
          <g className="robot__eyes">
            <circle className="robot__eye" cx="53" cy="51" r="2.7" />
            <circle className="robot__eye" cx="67" cy="51" r="2.7" />
          </g>

          {look.accessory === "halo" ? (
            <ellipse className="robot__halo" cx="60" cy="19" rx="17" ry="4.6" />
          ) : null}

          {look.accessory === "headset" ? (
            <g className="robot__headset">
              <path d="M 33 49 C 33 27 45 20 60 20 C 75 20 87 27 87 49" />
              <rect x="29" y="46" width="8" height="15" rx="4" />
              <rect x="83" y="46" width="8" height="15" rx="4" />
              <path d="M 37 61 C 43 67 48 67 52 65" />
              <circle cx="53.5" cy="65" r="2.2" />
            </g>
          ) : null}

          {look.accessory === "scarf" ? (
            <g className="robot__scarf">
              <rect x="40" y="79" width="40" height="8" rx="4" />
              <path d="M 76 84 C 85 92 84 104 76 110" />
            </g>
          ) : null}
        </g>
      </svg>
    </div>
  );
}
