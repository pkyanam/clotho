"use client";

import { useEffect, useState } from "react";
import { Badge } from "@cloudflare/kumo";
import { RobotIcon } from "@phosphor-icons/react";
import { ClothoClient, type AgentSession } from "@clotho/sdk-js";

import { browserApiUrl, timeAgo } from "src/lib/api";

const POLL_INTERVAL_MS = 10_000;

/**
 * Agent-session presence: which agent identities have touched this repo
 * recently, straight from the agent gateway's audit log (polled — the PRD
 * explicitly allows polling over real-time infra for the prototype).
 */
export function PresencePanel({ repo }: { repo: string }) {
  const [sessions, setSessions] = useState<AgentSession[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    const client = new ClothoClient({ baseUrl: browserApiUrl });
    let cancelled = false;
    const poll = async () => {
      try {
        const result = await client.agentSessions(repo, { limit: 12 });
        if (!cancelled) {
          setSessions(result);
          setError(false);
        }
      } catch {
        if (!cancelled) setError(true);
      }
    };
    poll();
    const timer = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [repo]);

  return (
    <section className="border border-kumo-hairline p-4">
      <h2 className="flex items-center gap-2 text-xs text-kumo-subtle">
        <RobotIcon size={14} />
        agent sessions
        <span className="ml-auto text-kumo-inactive">
          {error ? "offline" : "polling"}
        </span>
      </h2>
      {sessions === null ? (
        <p className="mt-3 text-xs text-kumo-inactive">…</p>
      ) : sessions.length === 0 ? (
        <p className="mt-3 text-xs text-kumo-inactive">
          no agent has touched this repo in the last 7 days.
        </p>
      ) : (
        <ul className="mt-3 space-y-3">
          {sessions.map((session) => (
            <li
              key={session.token_id}
              className="flex items-baseline justify-between gap-2 text-xs"
            >
              <span className="flex items-baseline gap-2">
                <span>{session.agent}</span>
                <span className="text-kumo-inactive">
                  {session.last_tool} · {session.tool_calls} calls
                </span>
                {session.last_status !== "ok" && (
                  <Badge variant="outline">{session.last_status}</Badge>
                )}
              </span>
              <span
                className="shrink-0 text-kumo-inactive"
                title={session.last_seen}
              >
                {timeAgo(Date.parse(session.last_seen))}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
