"use client";

import { useState, useTransition } from "react";
import { Badge, Button } from "@cloudflare/kumo";
import type { Agent, AgentAuditEntry, AgentDetail, MintedAgentToken } from "@clotho/sdk-js";

import { timeAgo } from "src/lib/api-shared";
import { SectionHeader } from "src/components/ui/page-frame";
import {
  createAgentAction,
  mintAgentTokenAction,
  revokeAgentTokenAction,
  updateAgentTokenScopesAction,
} from "./actions";

function isoToMillis(iso: string): number {
  const t = Date.parse(iso);
  return Number.isFinite(t) ? t : 0;
}

function scopeSummary(values: string[]): string {
  if (values.length === 0) return "none";
  if (values.length === 1 && values[0] === "*") return "*";
  return values.join(", ");
}

export function AgentManagement({
  agents,
  details,
  auditByAgent,
  loadError,
}: {
  agents: Agent[];
  details: Record<string, AgentDetail>;
  auditByAgent: Record<string, AgentAuditEntry[]>;
  loadError: string | null;
}) {
  const [pending, startTransition] = useTransition();
  const [minted, setMinted] = useState<MintedAgentToken | null>(null);
  const [expanded, setExpanded] = useState<string | null>(
    agents[0]?.name ?? null,
  );
  const [actionError, setActionError] = useState<string | null>(null);

  const copyToken = async (token: string) => {
    try {
      await navigator.clipboard.writeText(token);
    } catch {
      // ignore — user can select manually
    }
  };

  return (
    <section className="mt-8">
      <SectionHeader
        title="management"
        meta={`${agents.length} ${agents.length === 1 ? "identity" : "identities"}`}
      />

      {loadError ? (
        <p className="mt-4 text-[0.8125rem] text-kumo-inactive">{loadError}</p>
      ) : null}

      {minted ? (
        <div className="mt-4 border border-kumo-contrast bg-kumo-elevated px-4 py-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <p className="text-[0.9375rem] text-kumo-default">
                token minted for {minted.agent}
              </p>
              <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                copy this value now — it is shown once and never stored in
                plaintext.
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              onClick={() => setMinted(null)}
            >
              dismiss
            </Button>
          </div>
          <pre className="mt-3 overflow-x-auto border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.8125rem] text-kumo-default">
            {minted.token}
          </pre>
          <Button
            type="button"
            variant="outline"
            className="mt-3"
            onClick={() => copyToken(minted.token)}
          >
            copy token
          </Button>
        </div>
      ) : null}

      {actionError ? (
        <p className="mt-4 text-[0.8125rem] text-kumo-inactive">{actionError}</p>
      ) : null}

      <div className="mt-6 grid gap-8 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="min-w-0 space-y-4">
          {agents.length === 0 ? (
            <p className="text-[0.8125rem] text-kumo-inactive">
              no agents yet — create one to mint a scoped token.
            </p>
          ) : (
            agents.map((agent) => {
              const detail = details[agent.name];
              const tokens = detail?.tokens ?? [];
              const audit = auditByAgent[agent.name] ?? [];
              const open = expanded === agent.name;

              return (
                <div
                  key={agent.id}
                  className="border border-kumo-hairline bg-kumo-base"
                >
                  <button
                    type="button"
                    className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left hover:bg-kumo-elevated"
                    onClick={() => setExpanded(open ? null : agent.name)}
                  >
                    <div>
                      <span className="text-[0.9375rem] text-kumo-default">
                        {agent.name}
                      </span>
                      {agent.description ? (
                        <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                          {agent.description}
                        </p>
                      ) : null}
                    </div>
                    <Badge variant="outline">
                      {tokens.filter((t) => !t.revoked_at).length} active tokens
                    </Badge>
                  </button>

                  {open ? (
                    <div className="border-t border-kumo-hairline px-4 py-4">
                      <h3 className="text-[0.8125rem] uppercase tracking-wide text-kumo-inactive">
                        tokens
                      </h3>
                      {tokens.length === 0 ? (
                        <p className="mt-2 text-[0.8125rem] text-kumo-inactive">
                          no tokens yet.
                        </p>
                      ) : (
                        <ul className="mt-3 divide-y divide-kumo-hairline border border-kumo-hairline">
                          {tokens.map((token) => (
                            <li key={token.id} className="px-3 py-3">
                              <div className="flex flex-wrap items-center gap-2">
                                <code className="text-[0.8125rem]">
                                  {token.token_prefix}…
                                </code>
                                {token.revoked_at ? (
                                  <Badge variant="outline">revoked</Badge>
                                ) : (
                                  <Badge variant="outline">active</Badge>
                                )}
                              </div>
                              <p className="mt-1 text-[0.75rem] text-kumo-inactive">
                                repos: {scopeSummary(token.allowed_repos)} ·
                                tools: {scopeSummary(token.allowed_tools)}
                              </p>
                              <p className="mt-1 text-[0.75rem] text-kumo-inactive">
                                created {timeAgo(isoToMillis(token.created_at))}
                              </p>
                              {!token.revoked_at ? (
                                <div className="mt-3 flex flex-wrap gap-2">
                                  <form
                                    action={(fd) => {
                                      setActionError(null);
                                      startTransition(async () => {
                                        try {
                                          await revokeAgentTokenAction(
                                            agent.name,
                                            token.id,
                                          );
                                        } catch (e) {
                                          setActionError(
                                            e instanceof Error
                                              ? e.message
                                              : "revoke failed",
                                          );
                                        }
                                      });
                                    }}
                                  >
                                    <Button
                                      type="submit"
                                      variant="outline"
                                      disabled={pending}
                                    >
                                      revoke
                                    </Button>
                                  </form>
                                </div>
                              ) : null}
                              {!token.revoked_at ? (
                                <form
                                  className="mt-4 space-y-2 border-t border-kumo-hairline pt-3"
                                  action={(fd) => {
                                    setActionError(null);
                                    startTransition(async () => {
                                      try {
                                        await updateAgentTokenScopesAction(
                                          agent.name,
                                          token.id,
                                          fd,
                                        );
                                      } catch (e) {
                                        setActionError(
                                          e instanceof Error
                                            ? e.message
                                            : "update failed",
                                        );
                                      }
                                    });
                                  }}
                                >
                                  <label className="block text-[0.75rem] text-kumo-inactive">
                                    repos (comma or JSON array)
                                    <input
                                      name="allowed_repos"
                                      defaultValue={token.allowed_repos.join(
                                        ", ",
                                      )}
                                      className="mt-1 block w-full border border-kumo-hairline bg-kumo-canvas px-2 py-1.5 text-[0.8125rem]"
                                    />
                                  </label>
                                  <label className="block text-[0.75rem] text-kumo-inactive">
                                    tools (comma or JSON array)
                                    <input
                                      name="allowed_tools"
                                      defaultValue={token.allowed_tools.join(
                                        ", ",
                                      )}
                                      className="mt-1 block w-full border border-kumo-hairline bg-kumo-canvas px-2 py-1.5 text-[0.8125rem]"
                                    />
                                  </label>
                                  <Button
                                    type="submit"
                                    variant="outline"
                                    disabled={pending}
                                  >
                                    save scopes
                                  </Button>
                                </form>
                              ) : null}
                            </li>
                          ))}
                        </ul>
                      )}

                      <form
                        className="mt-4 space-y-3 border border-kumo-hairline p-3"
                        action={(fd) => {
                          setActionError(null);
                          startTransition(async () => {
                            try {
                              const result = await mintAgentTokenAction(
                                agent.name,
                                fd,
                              );
                              setMinted(result);
                            } catch (e) {
                              setActionError(
                                e instanceof Error
                                  ? e.message
                                  : "mint failed",
                              );
                            }
                          });
                        }}
                      >
                        <p className="text-[0.8125rem] text-kumo-default">
                          mint token
                        </p>
                        <label className="block text-[0.75rem] text-kumo-inactive">
                          allowed repos
                          <input
                            name="allowed_repos"
                            defaultValue="*"
                            required
                            className="mt-1 block w-full border border-kumo-hairline bg-kumo-canvas px-2 py-1.5 text-[0.8125rem]"
                          />
                        </label>
                        <label className="block text-[0.75rem] text-kumo-inactive">
                          allowed tools
                          <input
                            name="allowed_tools"
                            defaultValue="*"
                            required
                            className="mt-1 block w-full border border-kumo-hairline bg-kumo-canvas px-2 py-1.5 text-[0.8125rem]"
                          />
                        </label>
                        <label className="block text-[0.75rem] text-kumo-inactive">
                          expires in seconds (optional)
                          <input
                            name="expires_in_secs"
                            type="number"
                            min={1}
                            placeholder="omit for no expiry"
                            className="mt-1 block w-full border border-kumo-hairline bg-kumo-canvas px-2 py-1.5 text-[0.8125rem]"
                          />
                        </label>
                        <Button type="submit" disabled={pending}>
                          mint token
                        </Button>
                      </form>

                      {audit.length > 0 ? (
                        <div className="mt-6">
                          <h3 className="text-[0.8125rem] uppercase tracking-wide text-kumo-inactive">
                            recent audit
                          </h3>
                          <ul className="mt-2 divide-y divide-kumo-hairline border border-kumo-hairline">
                            {audit.slice(0, 12).map((entry) => (
                              <li
                                key={entry.id}
                                className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-[0.8125rem]"
                              >
                                <span>
                                  {entry.tool} on {entry.repo}
                                </span>
                                <span className="text-kumo-inactive">
                                  <Badge variant="outline">{entry.status}</Badge>{" "}
                                  {timeAgo(isoToMillis(entry.occurred_at))}
                                </span>
                              </li>
                            ))}
                          </ul>
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              );
            })
          )}
        </div>

        <aside>
          <div className="border border-kumo-hairline bg-kumo-base p-5">
            <h2 className="text-[0.9375rem]">create agent</h2>
            <p className="mt-2 text-[0.8125rem] leading-relaxed text-kumo-inactive">
              agents are non-human identities with scoped tokens. each tool call
              is audited and attributed.
            </p>
            <form
              action={(fd) => {
                setActionError(null);
                startTransition(async () => {
                  try {
                    await createAgentAction(fd);
                  } catch (e) {
                    setActionError(
                      e instanceof Error ? e.message : "create failed",
                    );
                  }
                });
              }}
              className="mt-5 space-y-4"
            >
              <label className="block text-[0.8125rem] text-kumo-inactive">
                name
                <input
                  name="name"
                  required
                  pattern="[a-z0-9][a-z0-9_-]*"
                  placeholder="weaver"
                  className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                />
              </label>
              <label className="block text-[0.8125rem] text-kumo-inactive">
                description
                <input
                  name="description"
                  placeholder="optional"
                  className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                />
              </label>
              <Button type="submit" disabled={pending}>
                create agent
              </Button>
            </form>
          </div>
        </aside>
      </div>
    </section>
  );
}
