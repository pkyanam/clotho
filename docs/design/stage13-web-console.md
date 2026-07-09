# Stage 13 — Web console design note

## Goals

Ship Clotho web as a **premium platform console** (Vercel dashboard density +
Cloudflare control-plane modularity + agent-native presence), not a scaffold.

## Information architecture

| Route | Role |
|---|---|
| `/` | Ops dashboard: repos, compute health, agents, activity |
| `/repos`, `/repos/new`, `/repos/[name]/*` | Repo workspace |
| `/settings` | Settings hub |
| `/settings/compute` | Provider registry + connect |
| `/settings/secrets` | Org-scoped secrets (masked) |
| `/orgs`, `/agents`, `/activity` | Progressive: stubs → full pages |

Global nav: logo · dashboard · repos · agents · activity · settings · ⌘K · status.

## Visual rules

- **Type scale:** body 14–15px (`text-sm` / `text-[0.9375rem]`), secondary 13px,
  titles 1.5–1.875rem. Stop using `text-xs` as default body.
- **Hierarchy:** page title → section → row → meta via size/weight/spacing.
- **Surfaces:** pure black canvas, hairline borders, flat elevated `#0c0c0c`.
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

## Non-goals

GitHub pixel clone, Forgejo UI, localStorage secrets, billing/federation polish.
