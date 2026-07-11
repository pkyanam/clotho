import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import type { Agent, AgentAuditEntry, AgentDetail } from "@clotho/sdk-js";

import { loadAgentSummary } from "src/lib/agents-summary";
import { api, timeAgo } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";
import { AgentManagement } from "./agent-management";

export const dynamic = "force-dynamic";

export default async function AgentsPage() {
  const repos = await (await api())
    .listRepos()
    .catch(() => []);

  let agents: Agent[] = [];
  let loadError: string | null = null;
  const details: Record<string, AgentDetail> = {};
  const auditByAgent: Record<string, AgentAuditEntry[]> = {};

  try {
    agents = await (await api()).listAgents();
    await Promise.all(
      agents.map(async (agent) => {
        try {
          details[agent.name] = await (await api()).getAgent(agent.name);
          auditByAgent[agent.name] = await (await api()).agentAudit(agent.name, {
            limit: 20,
          });
        } catch {
          // per-agent load errors are non-fatal
        }
      }),
    );
  } catch (e) {
    loadError =
      e instanceof Error ? e.message : "agent management is not available";
  }

  const { identities, sessionCount, recentRows: rows } = await loadAgentSummary(
    repos ?? [],
  );

  const activeRepos = new Set(rows.map((r) => r.repo));
  const totalCalls = rows.reduce((t, r) => t + r.tool_calls, 0);

  return (
    <PageFrame>
      <PageTitle
        title="agents"
        description="agent identities and recent sessions across repositories — every tool call is audited and attributed."
      />

      <AgentManagement
        agents={agents}
        details={details}
        auditByAgent={auditByAgent}
        loadError={loadError}
      />

      <section className="mt-12">
        <SectionHeader
          title="recent activity"
          meta={rows.length > 0 ? `${sessionCount} sessions · 7d` : undefined}
        />

        {rows.length === 0 ? (
          <div className="mt-6">
            <EmptyState
              title="no agent sessions yet"
              description="agents connect to clotho with scoped tokens and work through the same repos, pull requests, and issues you see here. when one starts working, its presence appears here and on each repo workspace."
              action={
                <Link href="/repos">
                  <Button type="button" variant="outline">
                    browse repositories
                  </Button>
                </Link>
              }
            />
          </div>
        ) : (
          <>
            <div className="mt-6 grid gap-3 sm:grid-cols-3">
              <StatCell label="identities · 7d" value={identities} />
              <StatCell label="active repos" value={activeRepos.size} />
              <StatCell label="tool calls" value={totalCalls} />
            </div>

            <div className="mt-6 overflow-x-auto border border-kumo-hairline">
              <table className="w-full min-w-[640px] text-left">
                <thead>
                  <tr className="border-b border-kumo-hairline text-[0.75rem] uppercase tracking-wide text-kumo-inactive">
                    <th className="px-4 py-2.5 font-normal">agent</th>
                    <th className="px-4 py-2.5 font-normal">repository</th>
                    <th className="px-4 py-2.5 font-normal">last tool</th>
                    <th className="px-4 py-2.5 font-normal">status</th>
                    <th className="px-4 py-2.5 text-right font-normal">calls</th>
                    <th className="px-4 py-2.5 text-right font-normal">
                      last seen
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-kumo-hairline">
                  {rows.map((row) => (
                    <tr
                      key={`${row.repo}-${row.agent_id}-${row.token_id}`}
                      className="transition-colors hover:bg-kumo-elevated"
                    >
                      <td className="px-4 py-3 text-[0.9375rem] text-kumo-default">
                        {row.agent}
                      </td>
                      <td className="px-4 py-3">
                        <Link
                          href={`/repos/${row.repo}/agents`}
                          className="text-[0.875rem] text-kumo-inactive hover:text-kumo-default hover:underline"
                        >
                          {row.repo}
                        </Link>
                      </td>
                      <td className="px-4 py-3 text-[0.875rem] text-kumo-inactive">
                        {row.last_tool}
                      </td>
                      <td className="px-4 py-3">
                        <Badge variant="outline">{row.last_status}</Badge>
                      </td>
                      <td className="px-4 py-3 text-right text-[0.875rem] text-kumo-inactive">
                        {row.tool_calls}
                      </td>
                      <td className="px-4 py-3 text-right text-[0.875rem] text-kumo-inactive">
                        {timeAgo(Date.parse(row.last_seen))}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </section>
    </PageFrame>
  );
}
