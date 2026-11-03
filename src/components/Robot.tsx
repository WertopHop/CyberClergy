// Little praying cleric. Everything is inline SVG plus CSS keyframes, so the
// figure stays sharp at any window size, needs no sprite sheets or runtime
// animation library, and inherits the palette from the active theme.
import type { CSSProperties } from "react";

interface RobotProps {
  praying: boolean;
  /** Index used to desynchronise the idle loops. */
  seed: number;
  scale?: number;
}

export function Robot({ praying, seed, scale = 1 }: RobotProps) {
  const phase = 0.7 + (seed % 5) * 0.31;

  return (
    <div
      className={`robot ${praying ? "robot--praying" : ""}`}
      style={{ "--robot-scale": scale } as CSSProperties}
    >
      <svg viewBox="0 0 120 150" className="robot__art" aria-hidden="true">
        <ellipse className="robot__shadow" cx="60" cy="139" rx="31" ry="7" />

        <g
          className="robot__bob"
          style={{ animationDelay: `${(seed % 6) * -0.9}s` }}
        >
          <g className="robot__antenna">
            <line className="robot__wire" x1="60" y1="30" x2="60" y2="16" />
            <circle className="robot__tip" cx="60" cy="13" r="2.8" />
          </g>

          <circle className="robot__shell" cx="60" cy="48" r="21" />
          <circle className="robot__edge" cx="60" cy="48" r="21" />
          <rect
            className="robot__visor"
            x="44"
            y="42"
            width="32"
            height="11"
            rx="5.5"
          />
          <circle className="robot__eye" cx="53" cy="47.5" r="2.1" />
          <circle className="robot__eye" cx="67" cy="47.5" r="2.1" />

          <rect
            className="robot__joint"
            x="54"
            y="65"
            width="12"
            height="7"
            rx="3.2"
          />

          <rect
            className="robot__shell"
            x="35"
            y="70"
            width="50"
            height="37"
            rx="15"
          />
          <rect
            className="robot__edge"
            x="35"
            y="70"
            width="50"
            height="37"
            rx="15"
          />
          <circle className="robot__emblem" cx="60" cy="88" r="6.4" />
          <path className="robot__emblem-mark" d="M60 84.6v6.8M56.6 88h6.8" />

          <g className="robot__arm robot__arm--left" style={{ animationDelay: `${phase}s` }}>
            <g transform="rotate(-16 32.5 90)">
              <rect
                className="robot__joint"
                x="28"
                y="62"
                width="9"
                height="30"
                rx="4.5"
              />
              <circle className="robot__shell" cx="32.5" cy="59" r="5.6" />
              <circle className="robot__edge" cx="32.5" cy="59" r="5.6" />
            </g>
          </g>
          <g className="robot__arm robot__arm--right" style={{ animationDelay: `${phase + 0.6}s` }}>
            <g transform="rotate(16 87.5 90)">
              <rect
                className="robot__joint"
                x="83"
                y="62"
                width="9"
                height="30"
                rx="4.5"
              />
              <circle className="robot__shell" cx="87.5" cy="59" r="5.6" />
              <circle className="robot__edge" cx="87.5" cy="59" r="5.6" />
            </g>
          </g>

          <rect
            className="robot__joint"
            x="43"
            y="104"
            width="10"
            height="22"
            rx="5"
            transform="rotate(9 48 104)"
          />
          <rect
            className="robot__joint"
            x="67"
            y="104"
            width="10"
            height="22"
            rx="5"
            transform="rotate(-9 72 104)"
          />
          <ellipse className="robot__shell" cx="47" cy="129" rx="9.5" ry="4.6" />
          <ellipse className="robot__edge" cx="47" cy="129" rx="9.5" ry="4.6" />
          <ellipse className="robot__shell" cx="73" cy="129" rx="9.5" ry="4.6" />
          <ellipse className="robot__edge" cx="73" cy="129" rx="9.5" ry="4.6" />
        </g>
      </svg>
    </div>
  );
}
