import { describe, expect, it, vi } from "vitest";

import { ClothoApiError, ClothoClient } from "./index";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function clientWith(response: Response) {
  const fetchMock = vi.fn().mockResolvedValue(response);
  const client = new ClothoClient({
    baseUrl: "http://gateway.test/",
    fetch: fetchMock as unknown as typeof fetch,
  });
  return { client, fetchMock };
}

describe("ClothoClient", () => {
  it("strips trailing slashes and hits the right paths", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ repos: [] }));
    await client.listRepos();
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos",
      undefined,
    );
  });

  it("unwraps list envelopes", async () => {
    const { client } = clientWith(
      jsonResponse({ commits: [{ commit_id: "abc" }] }),
    );
    const commits = await client.commits("weave");
    expect(commits).toEqual([{ commit_id: "abc" }]);
  });

  it("encodes query parameters and skips empty ones", async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(jsonResponse({ commit_id: "c", files: [] })),
      );
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });
    await client.tree("weave");
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos/weave/tree",
      undefined,
    );
    await client.file("weave", "src/a b.rs", "deadbeef");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/file?path=src%2Fa+b.rs&commit_id=deadbeef",
      undefined,
    );
  });

  it("posts repo creation as JSON with Stage 11 metadata", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ name: "weave" }));
    await client.createRepo("weave", {
      description: "a woven repo",
      visibility: "private",
      defaultBranch: "main",
      ownerOrg: "clotho",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          name: "weave",
          description: "a woven repo",
          visibility: "private",
          default_branch: "main",
          owner_org: "clotho",
        }),
      }),
    );
  });

  it("surfaces gateway error bodies as ClothoApiError", async () => {
    const { client } = clientWith(
      jsonResponse({ error: 'repo "weave" already exists' }, 409),
    );
    await expect(client.createRepo("weave")).rejects.toMatchObject({
      name: "ClothoApiError",
      status: 409,
      message: 'repo "weave" already exists',
    });
  });

  it("keeps a generic message for non-JSON error bodies", async () => {
    const { client } = clientWith(new Response("boom", { status: 502 }));
    const error = await client.pulls("weave").catch((e) => e);
    expect(error).toBeInstanceOf(ClothoApiError);
    expect(error.message).toContain("502");
  });

  it("polls agent sessions with a look-back window", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ sessions: [] }));
    await client.agentSessions("weave", { limit: 5, withinSecs: 3600 });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos/weave/agent-sessions?limit=5&within_secs=3600",
      undefined,
    );
  });

  it("lists and creates org secrets without returning raw values", async () => {
    const meta = {
      id: "s1",
      scope: "org",
      name: "DAYTONA_API_KEY",
      value_last4: "x7k2",
      description: "",
      org_id: "clotho",
      repo_id: null,
      created_by: "clotho",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ secrets: [meta] }))
      .mockResolvedValueOnce(jsonResponse(meta, 201))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.orgSecrets("clotho")).resolves.toEqual([meta]);
    await client.createOrgSecret("clotho", {
      name: "DAYTONA_API_KEY",
      value: "super-secret-x7k2",
    });
    await client.deleteOrgSecret("clotho", "DAYTONA_API_KEY");

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://gateway.test/api/v1/orgs/clotho/secrets",
      undefined,
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://gateway.test/api/v1/orgs/clotho/secrets",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          name: "DAYTONA_API_KEY",
          value: "super-secret-x7k2",
          description: "",
        }),
      }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "http://gateway.test/api/v1/orgs/clotho/secrets/DAYTONA_API_KEY",
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("reads repo secret metadata without a raw value", async () => {
    const meta = {
      id: "s2",
      scope: "repo",
      name: "CI_TOKEN",
      value_last4: "zz99",
      description: "",
      org_id: null,
      repo_id: "weave",
      created_by: "clotho",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };
    const { client, fetchMock } = clientWith(jsonResponse(meta));
    await expect(client.getRepoSecret("weave", "CI_TOKEN")).resolves.toEqual(
      meta,
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos/weave/secrets/CI_TOKEN",
      undefined,
    );
  });

  it("connects a provider via the secrets convenience route", async () => {
    const { client, fetchMock } = clientWith(
      jsonResponse({ name: "DAYTONA_API_KEY", value_last4: "abcd" }),
    );
    await client.connectProvider("daytona", {
      apiKey: "key-abcd",
      org: "clotho",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/providers/daytona/connect",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          api_key: "key-abcd",
          org: "clotho",
          upstream: "",
          credentials: {},
          modal_token_id: "",
          modal_token_secret: "",
        }),
      }),
    );
  });

  it("disconnects a provider via DELETE connect", async () => {
    const { client, fetchMock } = clientWith(
      jsonResponse({ provider: "daytona", deleted_secrets: ["DAYTONA_API_KEY"] }),
    );
    const res = await client.disconnectProvider("daytona", { org: "clotho" });
    expect(res.deleted_secrets).toEqual(["DAYTONA_API_KEY"]);
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/providers/daytona/connect?org=clotho",
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("wraps native issue list and creation", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ issues: [{ number: 1 }] }))
      .mockResolvedValueOnce(jsonResponse({ number: 2 }));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.issues("weave", "all")).resolves.toEqual([
      { number: 1 },
    ]);
    await client.createIssue("weave", { title: "race", body: "found" });

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://gateway.test/api/v1/repos/weave/issues?state=all",
      undefined,
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://gateway.test/api/v1/repos/weave/issues",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ title: "race", body: "found" }),
      }),
    );
  });

  it("posts PR collaboration actions through Clotho routes", async () => {
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(jsonResponse({})));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await client.commentOnPull("weave", 7, "looks good");
    await client.reviewPull("weave", 7, { event: "APPROVE" });
    await client.mergePull("weave", 7, { method: "squash" });

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://gateway.test/api/v1/repos/weave/pulls/7/comments",
      expect.objectContaining({ body: JSON.stringify({ body: "looks good" }) }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://gateway.test/api/v1/repos/weave/pulls/7/reviews",
      expect.objectContaining({
        body: JSON.stringify({ body: "", event: "APPROVE" }),
      }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "http://gateway.test/api/v1/repos/weave/pulls/7/merge",
      expect.objectContaining({
        body: JSON.stringify({
          method: "squash",
          title: undefined,
          message: undefined,
        }),
      }),
    );
  });

  it("reads branches and statuses from the facade", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ branches: [{ name: "main" }] }))
      .mockResolvedValueOnce(jsonResponse({ statuses: [{ state: "success" }] }));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.branches("weave")).resolves.toEqual([{ name: "main" }]);
    await expect(client.commitStatuses("weave", "abc/def")).resolves.toEqual([
      { state: "success" },
    ]);

    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/commits/abc%2Fdef/statuses",
      undefined,
    );
  });

  it("wraps actions runs, logs, config, and provider metadata", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ runs: [{ id: "run-1" }] }))
      .mockResolvedValueOnce(jsonResponse({ id: "run-2" }))
      .mockResolvedValueOnce(jsonResponse({ id: "run-1" }))
      .mockResolvedValueOnce(jsonResponse({ run_id: "run-1", text: "ok" }))
      .mockResolvedValueOnce(jsonResponse({ enabled: true, provider: "daytona" }))
      .mockResolvedValueOnce(jsonResponse({ enabled: false, provider: "daytona" }))
      .mockResolvedValueOnce(
        jsonResponse({
          providers: [
            {
              id: "daytona",
              name: "Daytona",
              kind: "direct",
              configured: true,
              capabilities: ["one-shot-jobs"],
              capability_detail: { one_shot_jobs: true, regions: [] },
            },
            { id: "box", name: "Box", kind: "stub", configured: false, capabilities: ["ssh"] },
          ],
          default_provider_id: "daytona",
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({ id: "daytona", name: "Daytona", kind: "direct", configured: true }),
      );
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.actionRuns("weave")).resolves.toEqual([{ id: "run-1" }]);
    await client.createActionRun("weave", { commitId: "abc", actor: "preetham" });
    await client.actionRun("weave", "run-1");
    await client.actionLogs("weave", "run-1");
    await client.actionsConfig("weave");
    await client.updateActionsConfig("weave", { enabled: false });
    const providers = await client.computeProviders();
    expect(providers.map((p) => p.id)).toEqual(["daytona", "box"]);
    await client.computeProvider("daytona");

    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://gateway.test/api/v1/repos/weave/actions/runs",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          commit_id: "abc",
          branch: "main",
          actor: "preetham",
        }),
      }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      4,
      "http://gateway.test/api/v1/repos/weave/actions/runs/run-1/logs",
      undefined,
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      7,
      "http://gateway.test/api/v1/providers",
      undefined,
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      8,
      "http://gateway.test/api/v1/providers/daytona",
      undefined,
    );
  });

  it("queries users, orgs, org detail, org repos, and the activity feed", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ users: [{ id: "clotho" }] }))
      .mockResolvedValueOnce(jsonResponse({ orgs: [{ name: "clotho" }] }))
      .mockResolvedValueOnce(jsonResponse({ org: { name: "clotho" }, members: [] }))
      .mockResolvedValueOnce(jsonResponse({ repos: [{ name: "weave" }] }))
      .mockResolvedValueOnce(jsonResponse({ events: [{ event_type: "repo.created" }] }));

    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.users()).resolves.toEqual([{ id: "clotho" }]);
    await expect(client.orgs()).resolves.toEqual([{ name: "clotho" }]);
    await expect(client.getOrg("clotho")).resolves.toEqual({
      org: { name: "clotho" },
      members: [],
    });
    await expect(client.getOrgRepos("clotho")).resolves.toEqual([{ name: "weave" }]);
    await expect(client.activity({ limit: 10 })).resolves.toEqual([
      { event_type: "repo.created" },
    ]);

    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/activity?limit=10",
      undefined,
    );
  });

  it("creates an org with optional display name and forgejo owner", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ name: "weavers" }));
    await client.createOrg("weavers", {
      displayName: "Weavers",
      forgejoOwner: "weavers",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/orgs",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          name: "weavers",
          display_name: "Weavers",
          forgejo_owner: "weavers",
        }),
      }),
    );
  });
});
