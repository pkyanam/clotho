/** Clotho's theme-specific semantic design tokens. */
export const colors = {
  background: "#090909",
  foreground: "#f4f0e6",
  muted: "#c5c0b6",
  faint: "#aaa59d",
  disabled: "#8d8982",
  surface: "#121212",
  surfaceHover: "#20201f",
  surfaceRaised: "#1a1a19",
  panel: "#0e0e0e",
  border: "#686662",
  borderStrong: "#d8d2c7",
  accent: "#e3ded3",
  accentStrong: "#fffaf0",
  accentSurface: "#2b2a28",
  action: "#514f4b",
  actionHover: "#686560",
  focus: "#f4f0e6",
  success: "#5ee6a8",
  warning: "#ffd166",
  danger: "#ff8f9b",
} as const;

export const colorsLight = {
  background: "#f1eee7",
  foreground: "#171715",
  muted: "#514f4a",
  faint: "#615f5a",
  disabled: "#716e68",
  surface: "#fffdf8",
  surfaceHover: "#e8e4dc",
  surfaceRaised: "#e4e0d8",
  panel: "#ebe7df",
  border: "#85817a",
  borderStrong: "#242421",
  accent: "#343330",
  accentStrong: "#11110f",
  accentSurface: "#dcd8cf",
  action: "#343330",
  actionHover: "#1f1f1d",
  focus: "#171715",
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
