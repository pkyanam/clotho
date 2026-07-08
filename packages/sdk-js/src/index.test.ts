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

  it("posts repo creation as JSON", async () => {
    const { client, fetchMock } = clientWith(jsonResponse({ name: "weave" }));
    await client.createRepo("weave");
    expect(fetchMock).toHaveBeenCalledWith(
      "http://gateway.test/api/v1/repos",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ name: "weave" }),
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
});
