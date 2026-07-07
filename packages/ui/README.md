# @clotho/ui

Clotho's design system: tokens today, components as the product surface grows.

## Design language

Pure black (`#000000`) and white (`#ffffff`) — no grey fills, no accent color.
Geist Pixel as the single typeface. All copy lowercase via CSS transform
(semantic casing preserved for accessibility). Hairline white borders are the
only structural lines; surfaces are flat (`#070707` lift, no shadows).
Hierarchy comes from size, weight, and spacing — never from color.

## Exports

- `@clotho/ui/tokens` — typed TS token objects (colors, typography, radii, spacing)
- `@clotho/ui/tokens.css` — CSS custom properties + base styles + utilities
- `@clotho/ui/fonts/*` — Geist Pixel (SIL OFL 1.1, © Vercel)

Consumers are Next.js apps using Tailwind 4; add `@clotho/ui` to
`transpilePackages`.
