import { describe, expect, it } from "vitest";

import { type Env, handleRequest } from "../src/index";

class MemoryKv {
  private readonly values = new Map<string, string>();

  async get(key: string, type?: string): Promise<unknown> {
    const value = this.values.get(key) ?? null;
    return type === "json" && value !== null ? JSON.parse(value) : value;
  }

  async put(key: string, value: string): Promise<void> {
    this.values.set(key, value);
  }
}

function environment(): Env {
  return {
    SNAPSHOTS: new MemoryKv() as unknown as KVNamespace,
    PUBLISH_TOKEN: "test-token",
  };
}

function snapshot(revision: number) {
  return {
    schema_version: 1,
    revision,
    published_at_unix_ms: 1_800_000_000_000,
    tournament_name: "Invitational",
    active_race: null,
    tournament: {
      phase: "registration",
      state: { participants: [] },
    },
  };
}

function publishRequest(revision: number, token = "test-token"): Request {
  return new Request("https://example.test/snapshot", {
    method: "PUT",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(snapshot(revision)),
  });
}

describe("snapshot API", () => {
  it("returns 404 with CORS headers before a snapshot is published", async () => {
    const result = await handleRequest(
      new Request("https://example.test/snapshot"),
      environment(),
    );

    expect(result.status).toBe(404);
    expect(result.headers.get("Access-Control-Allow-Origin")).toBe("*");
  });

  it("rejects an unauthorized publication", async () => {
    const result = await handleRequest(publishRequest(1, "wrong-token"), environment());
    expect(result.status).toBe(401);
  });

  it("publishes and reads a valid snapshot", async () => {
    const env = environment();
    expect((await handleRequest(publishRequest(2), env)).status).toBe(204);

    const result = await handleRequest(new Request("https://example.test/snapshot"), env);
    expect(result.status).toBe(200);
    expect(result.headers.get("Cache-Control")).toBe("no-store");
    expect(await result.json()).toEqual(snapshot(2));
  });

  it("rejects an older revision", async () => {
    const env = environment();
    expect((await handleRequest(publishRequest(10), env)).status).toBe(204);
    expect((await handleRequest(publishRequest(9), env)).status).toBe(409);
  });

  it("rejects an unsupported schema version", async () => {
    const env = environment();
    const invalid = snapshot(1);
    invalid.schema_version = 2;
    const request = new Request("https://example.test/snapshot", {
      method: "PUT",
      headers: {
        Authorization: "Bearer test-token",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(invalid),
    });

    expect((await handleRequest(request, env)).status).toBe(400);
  });
});