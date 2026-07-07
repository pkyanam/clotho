/**
 * Clotho design tokens.
 *
 * The design language: a precision instrument photographed in a dark room.
 * Pure black & white — no grey fills, no accent color. Geist Pixel as the one
 * typeface. All copy lowercase (via CSS transform; semantics preserved).
 * Hairline white borders are the only structural lines; surfaces are flat.
 * Hierarchy comes from size, weight, and spacing — never from color.
 */

export const colors = {
  background: "#000000",
  foreground: "#ffffff",
  /** "muted"/"faint" deliberately map to white — hierarchy is not grey. */
  muted: "#ffffff",
  faint: "#ffffff",
  surface: "#070707",
  surfaceHover: "#0c0c0c",
  panel: "#0a0a0a",
  border: "rgba(255, 255, 255, 0.16)",
  borderStrong: "rgba(255, 255, 255, 0.32)",
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
  md: "8px",
  lg: "12px",
  xl: "16px",
  pill: "9999px",
} as const;

export const spacing = {
  sm: "8px",
  md: "16px",
  lg: "24px",
  xl: "48px",
} as const;
