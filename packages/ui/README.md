# @clotho/ui

Clotho's shared semantic design system.

## Design language

Clotho is an operational control room: cool ink/navy foundations, clearly
separated surface tiers, high-legibility text, and an indigo interaction
accent. Status colors are reserved for labelled status communication.

Both themes expose semantic roles rather than raw grayscale choices. Meaningful
primary, secondary, and muted text meets WCAG 2.2 AA on canvas and surface;
control boundaries and focus indicators meet the 3:1 non-text threshold. Run
`pnpm --filter @clotho/ui test` to verify the palette contract.

## Exports

- `@clotho/ui/tokens` — typed theme token objects
- `@clotho/ui/tokens.css` — CSS custom properties, base styles, and utilities
- `@clotho/ui/fonts/*` — Geist Pixel (SIL OFL 1.1, © Vercel)
