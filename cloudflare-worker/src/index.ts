const SNAPSHOT_KEY = "current-snapshot";
const SUPPORTED_SCHEMA_VERSION = 1;

export interface Env {
  SNAPSHOTS: KVNamespace;
  PUBLISH_TOKEN: string;
}

interface SnapshotEnvelope {
  schema_version: number;
  revision: number;
  published_at_unix_ms: number;
  tournament_name: string;
  tournament: {
    phase: string;
  };
}

const corsHeaders = {
  "Access-Control-Allow-Headers": "Authorization, Content-Type",
  "Access-Control-Allow-Methods": "GET, PUT, OPTIONS",
  "Access-Control-Allow-Origin": "*",
};

function response(body: BodyInit | null, status: number, contentType = "text/plain"): Response {
  return new Response(body, {
    status,
    headers: {
      ...corsHeaders,
      "Cache-Control": "no-store",
      "Content-Type": contentType,
    },
  });
}

function isSnapshot(value: unknown): value is SnapshotEnvelope {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const snapshot = value as Partial<SnapshotEnvelope>;
  return (
    snapshot.schema_version === SUPPORTED_SCHEMA_VERSION &&
    Number.isSafeInteger(snapshot.revision) &&
    snapshot.revision !== undefined &&
    snapshot.revision >= 0 &&
    Number.isSafeInteger(snapshot.published_at_unix_ms) &&
    typeof snapshot.tournament_name === "string" &&
    snapshot.tournament_name.trim().length > 0 &&
    typeof snapshot.tournament === "object" &&
    snapshot.tournament !== null &&
    typeof snapshot.tournament.phase === "string"
  );
}

async function getSnapshot(env: Env): Promise<Response> {
  const snapshot = await env.SNAPSHOTS.get(SNAPSHOT_KEY);
  return snapshot === null
    ? response("Snapshot not found", 404)
    : response(snapshot, 200, "application/json");
}

async function putSnapshot(request: Request, env: Env): Promise<Response> {
  if (
    !env.PUBLISH_TOKEN ||
    request.headers.get("Authorization") !== `Bearer ${env.PUBLISH_TOKEN}`
  ) {
    return response("Unauthorized", 401);
  }

  let snapshot: unknown;
  try {
    snapshot = await request.json();
  } catch {
    return response("Request body must be valid JSON", 400);
  }

  if (!isSnapshot(snapshot)) {
    return response("Invalid snapshot", 400);
  }

  const current = await env.SNAPSHOTS.get<SnapshotEnvelope>(SNAPSHOT_KEY, "json");
  if (current !== null && snapshot.revision < current.revision) {
    return response("Snapshot revision is stale", 409);
  }

  await env.SNAPSHOTS.put(SNAPSHOT_KEY, JSON.stringify(snapshot));
  return response(null, 204);
}

export async function handleRequest(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname !== "/snapshot") {
    return response("Not found", 404);
  }

  switch (request.method) {
    case "GET":
      return getSnapshot(env);
    case "PUT":
      return putSnapshot(request, env);
    case "OPTIONS":
      return response(null, 204);
    default:
      return response("Method not allowed", 405);
  }
}

export default {
  fetch: handleRequest,
} satisfies ExportedHandler<Env>;