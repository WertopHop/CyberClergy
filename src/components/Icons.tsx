// Hairline icon set drawn on a 24px grid. Inline SVG keeps the bundle free of
// an icon dependency and lets every glyph inherit `currentColor`.
interface IconProps {
  size?: number;
}

function svgProps(size: number) {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    focusable: false,
  };
}

export function IconMenu({ size = 18 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M4 7h16M4 12h11M4 17h16" />
    </svg>
  );
}

export function IconGear({ size = 18 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M12 3.5v2M12 18.5v2M4.9 7.8l1.7 1M17.4 15.2l1.7 1M4.9 16.2l1.7-1M17.4 8.8l1.7-1" />
    </svg>
  );
}

export function IconFlame({ size = 18 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M12 3.5c2.9 3 4.6 5.2 4.6 7.7A4.6 4.6 0 0 1 12 15.8a4.6 4.6 0 0 1-4.6-4.6c0-1.3.5-2.4 1.4-3.6.5 1 1.4 1.6 2.3 1.6.2-1.9.9-3.5 2.9-5.7Z" />
      <path d="M8.5 17.4h7" />
    </svg>
  );
}

export function IconMinimize({ size = 16 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M5 12h14" />
    </svg>
  );
}

export function IconSquare({ size = 16 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <rect x="5.5" y="5.5" width="13" height="13" rx="1.8" />
    </svg>
  );
}

export function IconRestore({ size = 16 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <rect x="4.5" y="7.5" width="11" height="11" rx="1.6" />
      <path d="M8 7.5V5.5h10v10h-2" />
    </svg>
  );
}

export function IconClose({ size = 16 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />
    </svg>
  );
}

export function IconTrash({ size = 15 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M5 7h14M9.5 7V5.2h5V7M7 7l.8 12h8.4L17 7" />
    </svg>
  );
}

export function IconChevron({ size = 16 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M8 10l4 4 4-4" />
    </svg>
  );
}

export function IconCheck({ size = 15 }: IconProps) {
  return (
    <svg {...svgProps(size)}>
      <path d="M5 12.5l4.5 4.5L19 7.5" />
    </svg>
  );
}
