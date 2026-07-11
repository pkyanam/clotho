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
      expect.objectContaining({ headers: {} }),
    );
  });

  it("sends Authorization when token is set", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ user: { id: "u1", name: "clotho", email: "", display_name: "clotho", created_at: "" }, token_id: null }));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      token: "clotho_tok_secret",
      fetch: fetchMock as unknown as typeof fetch,
    });
    await client.me();
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/me",
      expect.objectContaining({
        headers: expect.objectContaining({
          authorization: "Bearer clotho_tok_secret",
        }),
      }),
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
      expect.objectContaining({ headers: {} }),
    );
    await client.artifacts("weave", "deadbeef");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/artifacts?commit_id=deadbeef",
      expect.objectContaining({ headers: {} }),
    );
    await client.artifactPreview("weave", "data/train rows.jsonl", {
      commitId: "deadbeef",
      limit: 25,
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/artifacts/preview?path=data%2Ftrain+rows.jsonl&commit_id=deadbeef&limit=25",
      expect.objectContaining({ headers: {} }),
    );
    await client.file("weave", "src/a b.rs", "deadbeef");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/file?path=src%2Fa+b.rs&commit_id=deadbeef",
      expect.objectContaining({ headers: {} }),
    );
  });

  it("posts Clotho repo kind and artifact policy metadata", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ name: "weave" }));
    await client.createRepo("weave", {
      description: "a woven repo",
      visibility: "private",
      defaultBranch: "main",
      ownerOrg: "clotho",
      kind: "model",
      largeFileThresholdBytes: 524288,
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
          kind: "model",
          large_file_threshold_bytes: 524288,
          network_mode: "public",
          network_tags: [],
        }),
      }),
    );
  });

  it("imports a pinned Hugging Face snapshot through Clotho", async () => {
    const { client, fetchMock } = clientWith(
      jsonResponse({ provider: "huggingface", commit_id: "c1" }),
    );
    await client.importHuggingFace("weave", "openai/gpt-oss", {
      revision: "abc123",
      paths: ["README.md", "model.safetensors"],
      maxFiles: 10,
      maxTotalBytes: 1_000_000,
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos/weave/imports/huggingface",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          repo_id: "openai/gpt-oss",
          revision: "abc123",
          paths: ["README.md", "model.safetensors"],
          max_files: 10,
          max_total_bytes: 1_000_000,
          allow_unsafe: false,
        }),
      }),
    );
  });

  it("queues and monitors durable Hub imports", async () => {
    const job = { id: "j1", status: "queued", source_repo_id: "openai/gpt-oss" };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(job, 202))
      .mockResolvedValueOnce(jsonResponse({ jobs: [job] }))
      .mockResolvedValueOnce(jsonResponse({ ...job, status: "running" }));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });
    await client.startHuggingFaceImport("weave", "openai/gpt-oss");
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://gateway.test/api/v1/repos/weave/hub-imports",
      expect.objectContaining({ method: "POST" }),
    );
    expect(await client.hubImportJobs("weave")).toHaveLength(1);
    await client.hubImportJob("weave", "j1");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/hub-imports/j1",
      expect.objectContaining({ headers: {} }),
    );
  });

  it("creates and verifies immutable repository releases", async () => {
    const release = { version: "v1.0.0", commit_id: "c1", verified: true };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(release, 201))
      .mockResolvedValueOnce(jsonResponse({ releases: [release] }))
      .mockResolvedValueOnce(jsonResponse(release));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });
    await client.createRelease("weave", "v1.0.0", { commitId: "c1" });
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://gateway.test/api/v1/repos/weave/releases",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          version: "v1.0.0",
          commit_id: "c1",
          require_ready: true,
        }),
      }),
    );
    expect(await client.releases("weave")).toHaveLength(1);
    await client.release("weave", "v1.0.0");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/releases/v1.0.0",
      expect.objectContaining({ headers: {} }),
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
      expect.objectContaining({ headers: {} }),
    );
  });

  it("lists and mints agent identities", async () => {
    const agent = {
      id: "a1",
      name: "weaver",
      description: "demo",
      created_at: "2026-01-01T00:00:00Z",
    };
    const minted = {
      token: "clotho_agt_deadbeef",
      token_id: "t1",
      agent: "weaver",
      allowed_repos: ["*"],
      allowed_tools: ["*"],
      expires_at: null,
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ agents: [agent] }))
      .mockResolvedValueOnce(jsonResponse(agent, 201))
      .mockResolvedValueOnce(jsonResponse({ agent, tokens: [] }))
      .mockResolvedValueOnce(jsonResponse(minted, 201))
      .mockResolvedValueOnce(jsonResponse({ tokens: [{ id: "t1", token_prefix: "clotho_agt_", allowed_repos: ["*"], allowed_tools: ["*"], created_at: "2026-01-01T00:00:00Z", expires_at: null, revoked_at: null }] }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(jsonResponse({ entries: [] }));
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });
    const agents = await client.listAgents();
    expect(agents).toHaveLength(1);
    await client.createAgent({ name: "weaver", description: "demo" });
    await client.getAgent("weaver");
    const token = await client.mintAgentToken("weaver", {
      allowedRepos: ["*"],
      allowedTools: ["*"],
    });
    expect(token.token).toContain("clotho_agt_");
    const tokens = await client.listAgentTokens("weaver");
    expect(tokens).toHaveLength(1);
    await client.revokeAgentToken("weaver", "t1");
    const audit = await client.agentAudit("weaver");
    expect(audit).toEqual([]);
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
      expect.objectContaining({ headers: {} }),
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
      expect.objectContaining({ headers: {} }),
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
          client_id: "",
          client_secret: "",
          org: "clotho",
          upstream: "",
          credentials: {},
          modal_token_id: "",
          modal_token_secret: "",
        }),
      }),
    );
  });

  it("sends Tailscale OAuth credentials only to the provider connect route", async () => {
    const { client, fetchMock } = clientWith(
      jsonResponse({ name: "TAILSCALE_OAUTH_CLIENT_SECRET", value_last4: "tail" }),
    );
    await client.connectProvider("tailscale", {
      clientId: "client-id",
      clientSecret: "tskey-client-tail",
      org: "clotho",
    });
    const body = JSON.parse(fetchMock.mock.calls[0]?.[1]?.body as string);
    expect(body).toMatchObject({
      api_key: "",
      client_id: "client-id",
      client_secret: "tskey-client-tail",
      org: "clotho",
    });
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
      expect.objectContaining({ headers: {} }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://gateway.test/api/v1/repos/weave/issues",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          title: "race",
          body: "found",
          labels: [],
          assignees: [],
        }),
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

  it("lists pull comments, reviews, and merge policy", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({
          comments: [{ id: 1, body: "root", in_reply_to: null }],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          reviews: [{ id: 2, state: "APPROVED", user: { login: "alice" } }],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          require_passing_actions: true,
          block_merge_when_conflicted: true,
          require_review_approvals: 1,
          protect_default_branch: false,
          updated_at: "2026-01-01T00:00:00Z",
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          require_passing_actions: false,
          block_merge_when_conflicted: true,
          require_review_approvals: 0,
          protect_default_branch: false,
          updated_at: "2026-01-02T00:00:00Z",
        }),
      );
    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.listPullComments("weave", 3)).resolves.toEqual([
      { id: 1, body: "root", in_reply_to: null },
    ]);
    await expect(client.listPullReviews("weave", 3)).resolves.toEqual([
      { id: 2, state: "APPROVED", user: { login: "alice" } },
    ]);
    await expect(client.getMergePolicy("weave")).resolves.toMatchObject({
      require_passing_actions: true,
      require_review_approvals: 1,
    });
    await client.updateMergePolicy("weave", { require_passing_actions: false });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/repos/weave/merge-policy",
      expect.objectContaining({
        method: "PUT",
        body: JSON.stringify({ require_passing_actions: false }),
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
      expect.objectContaining({ headers: {} }),
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
      expect.objectContaining({ headers: {} }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      7,
      "http://gateway.test/api/v1/providers",
      expect.objectContaining({ headers: {} }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      8,
      "http://gateway.test/api/v1/providers/daytona",
      expect.objectContaining({ headers: {} }),
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
      expect.objectContaining({ headers: {} }),
    );
  });

  it("creates an org with optional display name and git owner", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ id: "o1", name: "weavers", display_name: "Weavers", created_by: "u1", created_at: "" }));
    await client.createOrg("weavers", {
      displayName: "Weavers",
      gitOwner: "weavers",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/orgs",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          name: "weavers",
          display_name: "Weavers",
          git_owner: "weavers",
        }),
      }),
    );
  });

  it("creates issues with labels and assignees", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ number: 1, title: "t" }));
    await client.createIssue("weave", {
      title: "t",
      body: "b",
      labels: ["bug"],
      assignees: ["clotho"],
      milestone: 3,
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos/weave/issues",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          title: "t",
          body: "b",
          labels: ["bug"],
          assignees: ["clotho"],
          milestone: 3,
        }),
      }),
    );
  });

  it("updates issues and lists labels, milestones, notifications", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ number: 1, state: "closed" }))
      .mockResolvedValueOnce(jsonResponse({ labels: [{ id: 1, name: "bug" }] }))
      .mockResolvedValueOnce(jsonResponse({ milestones: [{ id: 2, title: "v1" }] }))
      .mockResolvedValueOnce(
        jsonResponse({ notifications: [], unread_count: 0 }),
      );

    const client = new ClothoClient({
      baseUrl: "http://gateway.test",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await client.updateIssue("weave", 1, { state: "closed" });
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://gateway.test/api/v1/repos/weave/issues/1",
      expect.objectContaining({ method: "PATCH" }),
    );

    await expect(client.listLabels("weave")).resolves.toEqual([
      { id: 1, name: "bug" },
    ]);
    await expect(client.listMilestones("weave")).resolves.toEqual([
      { id: 2, title: "v1" },
    ]);
    await expect(client.notifications({ unread: true })).resolves.toEqual({
      notifications: [],
      unread_count: 0,
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://gateway.test/api/v1/notifications?unread=true",
      expect.objectContaining({ headers: {} }),
    );
  });

  it("lists fabric providers by layer", async () => {
    const { client, fetchMock } = clientWith(
      jsonResponse({
        providers: [
          {
            id: "bootstrap",
            name: "Bootstrap",
            layer: "auth",
            kind: "auth",
            enabled: true,
            configured: true,
            capabilities: ["human-api-tokens"],
          },
        ],
        default_provider_id: "bootstrap",
        layer: "auth",
      }),
    );
    const list = await client.listProviders({ layer: "auth" });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/providers?layer=auth",
      expect.objectContaining({ headers: {} }),
    );
    expect(list.default_provider_id).toBe("bootstrap");
    expect(list.providers[0]?.id).toBe("bootstrap");
  });
});
