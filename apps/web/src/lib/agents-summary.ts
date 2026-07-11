import type { AgentSession, RepoInfo } from "@clotho/sdk-js";

import { api } from "src/lib/api";

export type AgentSummaryRow = AgentSession & { repo: string };

export interface AgentSummary {
  identities: number;
  sessionCount: number;
  lastSeenMs: number | null;
  recentRows: AgentSummaryRow[];
}

const DEFAULT_SAMPLE = 12;
const DEFAULT_LIMIT = 8;
const DEFAULT_WITHIN_SECS = 86400 * 7;

/** Sample recent agent sessions across top repos (same pattern as /agents). */
export async function loadAgentSummary(
  repos: RepoInfo[],
  options?: { sampleSize?: number; limit?: number; withinSecs?: number },
): Promise<AgentSummary> {
  const sampleSize = options?.sampleSize ?? DEFAULT_SAMPLE;
  const limit = options?.limit ?? DEFAULT_LIMIT;
  const withinSecs = options?.withinSecs ?? DEFAULT_WITHIN_SECS;

  const sessionsByRepo = await Promise.all(
    repos.slice(0, sampleSize).map(async (repo) => {
      const sessions = await (await api())
        .agentSessions(repo.name, { limit, withinSecs })
        .catch(() => [] as AgentSession[]);
      return { repo: repo.name, sessions };
    }),
  );

  const rows: AgentSummaryRow[] = sessionsByRepo
    .flatMap(({ repo, sessions }) => sessions.map((s) => ({ repo, ...s })))
    .sort((a, b) => Date.parse(b.last_seen) - Date.parse(a.last_seen));

  const identities = new Set(rows.map((r) => r.agent_id));
  const lastSeenMs =
    rows.length > 0 ? Date.parse(rows[0]!.last_seen) : null;

  return {
    identities: identities.size,
    sessionCount: rows.length,
    lastSeenMs,
    recentRows: rows,
  };
}
