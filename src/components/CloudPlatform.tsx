// The cloud the clerics stand on.
//
// It is drawn as a stepped platform instead of a pile of blurred circles: up to
// four tiers, each a flat slab with a lit top edge over a body that sinks to the
// bottom of the scene. The tiers are stacked back to front so every row of
// clerics has its own shelf, and the whole scene layer keeps the rows in line at
// any window size because the SVG is stretched to the scene box.
//
// Geometry is shared with the row offsets in Chapel.tsx: the front slab sits at
// 35% of the scene height, each further tier is 10% higher and 120 units in the
// viewBox, which is 1200 units tall. Four tiers hold twelve robots.
const TIER_TOPS = [780, 660, 540, 420];
const TIER_INSETS = [24, 200, 360, 520];
const SLAB_HEIGHT = 80;
const SCENE_UNITS = 1200;

interface CloudPlatformProps {
  /** How many rows of clerics stand on it; each row gets its own tier. */
  rows: number;
}

export function CloudPlatform({ rows }: CloudPlatformProps) {
  const depth = Math.min(Math.max(rows, 1), TIER_TOPS.length);
  const tiers: number[] = [];
  for (let index = depth - 1; index >= 0; index -= 1) {
    tiers.push(index);
  }

  return (
    <div className="platform" aria-hidden="true">
      <svg
        className="platform__art"
        viewBox={`0 0 ${SCENE_UNITS} ${SCENE_UNITS}`}
        preserveAspectRatio="none"
      >
        <g className="platform__breathe">
          {tiers.map((index) => {
            const top = TIER_TOPS[index];
            const inset = TIER_INSETS[index];
            const width = SCENE_UNITS - inset * 2;

            return (
              <g key={index}>
                <rect
                  className="platform__body"
                  x={inset - 22}
                  y={top + 46}
                  width={width + 44}
                  height={SCENE_UNITS - top - 46}
                  rx="52"
                />
                <rect
                  className="platform__slab"
                  x={inset}
                  y={top}
                  width={width}
                  height={SLAB_HEIGHT}
                  rx={SLAB_HEIGHT / 2}
                />
                <rect
                  className="platform__line"
                  x={inset + 54}
                  y={top + 7}
                  width={width - 108}
                  height="4"
                  rx="2"
                />
                <rect
                  className="platform__sheen"
                  x={inset + 96}
                  y={top + 28}
                  width={width * 0.22}
                  height="10"
                  rx="5"
                />
                <rect
                  className="platform__sheen"
                  x={inset + width * 0.68}
                  y={top + 28}
                  width={width * 0.14}
                  height="10"
                  rx="5"
                />
              </g>
            );
          })}
        </g>
      </svg>
    </div>
  );
}
