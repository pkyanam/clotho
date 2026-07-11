# Stage 13 — Web console design note

## Goals

Ship Clotho web as a **premium platform console** (Vercel dashboard density +
Cloudflare control-plane modularity + agent-native presence), not a scaffold.

## Information architecture

| Route                                     | Role                                                   |
| ----------------------------------------- | ------------------------------------------------------ |
| `/`                                       | Ops dashboard: repos, compute health, agents, activity |
| `/repos`, `/repos/new`, `/repos/[name]/*` | Repo workspace                                         |
| `/settings`                               | Settings hub                                           |
| `/settings/compute`                       | Provider registry + connect                            |
| `/settings/secrets`                       | Org-scoped secrets (masked)                            |
| `/orgs`, `/agents`, `/activity`           | Progressive: stubs → full pages                        |

Global nav: logo · dashboard · repos · agents · activity · settings · ⌘K · status.

## Visual rules

- **Type scale:** body 14–15px (`text-sm` / `text-[0.9375rem]`), secondary 13px,
  titles 1.5–1.875rem. Stop using `text-xs` as default body.
- **Hierarchy:** page title → section → row → meta via size/weight/spacing.
- **Surfaces:** quiet, visibly distinct canvas / panel / raised tiers in both
  themes. Dark mode may use near-black surfaces; light mode must not collapse
  cards, page canvas, borders, and disabled controls into the same white/gray.
- **Kumo:** PageHeader, Badge, Button, Banner, Empty, Dialog, Input, Tabs;
  Clotho wrappers for AppShell, DataTable, EmptyState, SettingsSection.
- **Status color:** keep Kumo success/warning/danger for badges only.
- **Copy:** product language only — ban Forgejo, Gitea, docker hostnames, CCI
  jargon, “stage N”, env-var-as-only-instruction.

## Secrets UX

- Settings → Secrets: table name / scope / updated / rotate / delete.
- Compute → Connect Daytona: write-once key → org secret `DAYTONA_API_KEY`.
- UI shows `configured · ···last4`, never the raw value after save.

## Layout

- Content max-width ~1280px (`max-w-7xl`) with optional side rail.
- Dashboard: 12-col grid — main list + right activity/status rail.
- Repo: code/commits primary; collab/agents/activity as composed side panels.
- Mobile: drawer nav, full-width tables, no horizontal crumb soup.

## Contrast remediation — public-release blocker

The July 11 dashboard capture exposes a system-wide semantic-token problem,
not a one-page styling bug. In light mode, secondary copy, card labels, borders,
metadata, placeholders, and inactive controls are too close to the canvas;
cards and rows have insufficient surface separation. The same opacity-based
tokens become too dim in dark mode.

Required design-system behavior:

- Define theme-specific semantic roles—`canvas`, `surface`, `surface-raised`,
  `border`, `border-strong`, `text`, `text-secondary`, `text-muted`,
  `text-disabled`, `focus`, and status roles. Components must not select raw
  grayscale values or stack opacity on semantic text tokens.
- Target WCAG 2.2 AA: at least **4.5:1** for all meaningful normal-size text,
  **3:1** for large text, borders/control boundaries, icons that communicate
  state, and focus indicators. Aim for **7:1** on primary reading text.
- “Muted” means lower emphasis, not low legibility. Counts, branch names,
  ownership, timestamps, input placeholders, table metadata, empty-state text,
  and card labels remain meaningful and must meet 4.5:1.
- Disabled controls may be dimmer, but must remain identifiable and must not be
  the only way state/reason is communicated. Pair disabled Actions with visible
  explanatory copy.
- Separate adjacent surfaces with a combination of tone and border. Hairlines
  alone are insufficient on common displays; selected/hovered rows need an
  additional non-color-only cue.
- Focus rings are at least 2 CSS pixels, never clipped, visible against both the
  component and surrounding surface, and consistent across links, buttons,
  menus, fields, dialogs, tabs, and command-palette results.
- Status colors are not used as text gray replacements and never communicate
  state without a label/icon. Test normal, hover, active, selected, disabled,
  loading, error, and destructive states in both themes.
- Long generated repository and organization names wrap or truncate with an
  accessible full-value affordance; they must not distort the dashboard rail.

Verification must include automated contrast/accessibility checks plus visual
review on dashboard, repo tree, artifacts, import progress, Actions, issues,
pulls, agents, provider settings, secrets, and dialogs at desktop and 320 px.
Capture matching light/dark screenshots for the release evidence bundle.

### Implemented semantic direction

The Stage 22 console uses an operational control-room palette rather than the
original black/white inversion: deep ink and navy surfaces in dark mode, soft
slate and white tiers in light mode, and indigo only for interaction and
selection. `packages/ui/styles/tokens.css` owns the semantic roles; the Kumo
overlay maps to those roles without opacity-stacking meaningful text.

`packages/ui/test/contrast.test.mjs` enforces 4.5:1 for primary, secondary, and
muted text on canvas and working surfaces, plus 3:1 for control boundaries and
focus indicators. This closes the palette-level blocker, not the broader
journey-level accessibility gate: critical routes, keyboard flows, async
states, and 320 px layouts still require continuing browser evidence.
The first matching dashboard captures and verification notes are stored in
[`docs/evidence/stage22-web`](../evidence/stage22-web/README.md).

## Non-goals

GitHub pixel clone, Forgejo UI, localStorage secrets, billing/federation polish.
