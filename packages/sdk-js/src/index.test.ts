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
});
