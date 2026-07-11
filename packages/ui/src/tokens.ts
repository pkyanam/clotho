/** Clotho's theme-specific semantic design tokens. */
export const colors = {
  background: "#080d1a",
  foreground: "#f8fafc",
  muted: "#b7c2d6",
  faint: "#91a0b8",
  disabled: "#77859d",
  surface: "#11192b",
  surfaceHover: "#18243a",
  surfaceRaised: "#18243a",
  panel: "#0d1424",
  border: "#607291",
  borderStrong: "#9ca8ff",
  accent: "#9ca8ff",
  accentStrong: "#c3caff",
  accentSurface: "#1d2850",
  action: "#6554c0",
  actionHover: "#5847ae",
  focus: "#c3caff",
  success: "#5ee6a8",
  warning: "#ffd166",
  danger: "#ff8f9b",
} as const;

export const colorsLight = {
  background: "#f4f6fb",
  foreground: "#172033",
  muted: "#475569",
  faint: "#5b6475",
  disabled: "#687386",
  surface: "#ffffff",
  surfaceHover: "#eef1f8",
  surfaceRaised: "#e7eaf5",
  panel: "#edf0f7",
  border: "#7f8da5",
  borderStrong: "#6554c0",
  accent: "#6554c0",
  accentStrong: "#49379b",
  accentSurface: "#e8e5fb",
  action: "#6554c0",
  actionHover: "#5847ae",
  focus: "#49379b",
  success: "#16794b",
  warning: "#8a5700",
  danger: "#b4233b",
} as const;

export const typography = {
  fontFamily:
    'var(--font-geist-pixel), ui-monospace, "SFMono-Regular", monospace',
  display: {
    fontSize: "clamp(2.25rem, 6vw, 3.75rem)",
    fontWeight: 400,
    lineHeight: 1.1,
  },
  headline: {
    fontSize: "clamp(1.5rem, 3vw, 1.875rem)",
    fontWeight: 400,
    lineHeight: 1.2,
  },
  title: { fontSize: "1.25rem", fontWeight: 400, lineHeight: 1.3 },
  body: {
    fontSize: "1rem",
    fontWeight: 400,
    lineHeight: 1.6,
    letterSpacing: "0.01em",
  },
  label: {
    fontSize: "0.75rem",
    fontWeight: 400,
    lineHeight: 1.4,
    letterSpacing: "0.01em",
  },
} as const;

export const radii = {
  sm: "6px",
  md: "10px",
  lg: "14px",
  xl: "18px",
  pill: "9999px",
} as const;

export const spacing = {
  sm: "8px",
  md: "16px",
  lg: "24px",
  xl: "48px",
} as const;
