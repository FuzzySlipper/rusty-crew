import assert from "node:assert/strict";
import type {
  AgentId,
  BodyState,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  createDebugApiClient,
  DebugApiClientError,
  handleAdminDiagnosticsRequest,
  inspectDirectDebugSession,
} from "./index.js";
import type { AdminApiEnvelope, DebugApiFetch } from "./index.js";
import { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";

const session = {
  handle: 1 as SessionHandle,
  sessionId: "debug-client-session" as SessionId,
  agentId: "debug-client-agent" as AgentId,
  profileId: "debug-client-profile" as ProfileId,
  kind: "full",
  resourceLimits: {
    workdir: "/home/dev/rusty-crew",
  },
  toolProfile: {
    tools: [
      {
        name: "read_file",
        description: "Read files.",
      },
    ],
  },
  status: "active",
  brainTurnCount: 1,
  createdAt: "2026-06-20T00:00:00Z",
  lastActiveAt: "2026-06-20T00:00:30Z",
} satisfies SessionState;
const bodyState = {
  session,
  pendingMessages: [],
  recentEvents: [],
  childCompletions: [],
  fanOutGroups: [],
  deltaPolicy: {
    mode: "frozen_snapshot_next_wake",
    queueOwner: "body",
    queuedMessageTtlMs: 5_000,
    maxQueuedMessages: 10,
  },
} satisfies BodyState;
const diagnostics = buildRuntimeDiagnosticsProjection({
  now: "2026-06-20T00:01:00Z",
  sessions: [session],
  observation: {
    enabled: true,
    writerAvailable: true,
  },
  adapters: {
    generatedAt: "2026-06-20T00:01:00Z",
    degraded: true,
    channels: {
      totalBindings: 1,
      activeBindings: 1,
      degradedBindings: 0,
      droppedProjections: 0,
      bindings: [
        {
          bindingId: "channel-debug",
          bindingSource: "configured",
          adapterId: "den",
          agentId: session.agentId,
          sessionId: session.sessionId,
          profileId: session.profileId,
          provider: "den",
          status: "active",
          membershipStatus: "joined",
          presenceStatus: "online",
          subscriptionStatus: "active",
          stalePresence: false,
          droppedProjections: 0,
        },
      ],
    },
    mcp: {
      totalSurfaces: 1,
      activeSurfaces: 0,
      degradedSurfaces: 1,
      collisionCount: 0,
      reloadCount: 0,
      surfaces: [
        {
          bindingId: "mcp-debug",
          adapterId: "mcp",
          agentId: session.agentId,
          sessionId: session.sessionId,
          profileId: session.profileId,
          status: "degraded",
          transport: "stdio",
          serverNames: ["debug"],
          toolProfileKey: "debug-client-profile:debug",
          reconnectAttempts: 1,
          collisionCount: 0,
          discoveryIssueCount: 1,
          optionalServerFailures: ["debug"],
          lastError: "debug mcp unavailable",
        },
      ],
    },
    issues: ["mcp mcp-debug: debug mcp unavailable"],
  },
});
const recentEvents = [
  {
    id: "event-debug",
    createdAt: "2026-06-20T00:02:00Z",
    source: "smoke",
    eventType: "debug.event",
    summary: "Debug event.",
    workRef: {
      sessionId: session.sessionId,
    },
  },
];
const calls: Array<{
  method: string;
  url: string;
  authorization?: string;
  body?: unknown;
}> = [];
let overviewAttempts = 0;
const fakeFetch: DebugApiFetch = async (input, init) => {
  const url = new URL(String(input));
  const method = init?.method ?? "GET";
  const body =
    typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
  calls.push({
    method,
    url: url.toString(),
    authorization: header(init?.headers, "authorization"),
    body,
  });

  if (url.pathname === "/v1/admin/diagnostics/overview") {
    overviewAttempts += 1;
    if (overviewAttempts === 1) {
      return jsonResponse(
        {
          ok: false,
          error: {
            code: "internal_error",
            reason_code: "temporary_local_failure",
            message: "temporary local failure",
            retryable: true,
          },
          meta: {
            request_id: "retry",
            schema_version: 1,
          },
        } satisfies AdminApiEnvelope<never>,
        503,
      );
    }
  }

  if (url.pathname.startsWith("/v1/admin/")) {
    return routeAdmin(url, method);
  }
  if (url.pathname === `/v1/debug/sessions/${session.sessionId}/context`) {
    const view = inspectDirectDebugSession(
      {
        sessionId: session.sessionId,
        includeMessageBodies:
          url.searchParams.get("include_message_bodies") === "true",
      },
      {
        diagnostics,
        sessions: [
          {
            session,
            bodyState,
          },
        ],
        recentEvents,
        now: () => "2026-06-20T00:03:00Z",
      },
    );
    assert.equal(view.ok, true);
    return jsonResponse({
      ok: true,
      data: view.data,
      meta: {
        request_id: "debug-context",
        schema_version: 1,
      },
    } satisfies AdminApiEnvelope<unknown>);
  }
  if (url.pathname === `/v1/debug/sessions/${session.sessionId}/turn`) {
    return jsonResponse({
      ok: true,
      data: {
        status: "accepted",
        summary: `accepted ${body?.actorId}`,
        wakeId: "wake-client-debug",
      },
      meta: {
        request_id: "debug-turn",
        schema_version: 1,
      },
    } satisfies AdminApiEnvelope<unknown>);
  }
  return jsonResponse(
    {
      ok: false,
      error: {
        code: "not_found",
        reason_code: "fake_not_found",
        message: "fake server route not found",
        retryable: false,
      },
      meta: {
        request_id: "missing",
        schema_version: 1,
      },
    } satisfies AdminApiEnvelope<never>,
    404,
  );
};

const client = createDebugApiClient({
  baseUrl: "http://rusty-crew.local",
  bearerToken: "debug-token",
  fetchImpl: fakeFetch,
  timeoutMs: 500,
  retries: 1,
});

const overview = await client.overview();
assert.equal(overviewAttempts, 2);
assert.equal(overview.summary.sessions, 1);
assert.equal(calls[0]?.authorization, "Bearer debug-token");

const bundle = await client.diagnostics();
assert.equal(bundle.overview.summary.activeSessions, 1);

const sessions = await client.sessions({ profileId: session.profileId });
assert.equal(sessions.items[0]?.sessionId, session.sessionId);

const mcp = await client.mcpSurfaces({ status: "degraded" });
assert.equal(mcp.items[0]?.bindingId, "mcp-debug");

const channels = await client.channelBindings({ status: "active" });
assert.equal(channels.items[0]?.bindingId, "channel-debug");

const observation = await client.observation();
assert.equal(observation?.enabled, true);

const metrics = await client.metrics({ limit: 10 });
assert.ok(metrics.items.length > 0);

const events = await client.recentEvents();
assert.equal(events.items[0]?.id, "event-debug");

const context = await client.directDebugContext({
  sessionId: session.sessionId,
  includeMessageBodies: true,
});
assert.equal(context.source, "direct_debug");
assert.equal(context.session.sessionId, session.sessionId);

const turn = await client.requestDirectDebugTurn({
  sessionId: session.sessionId,
  actorId: "operator",
  body: "Run direct debug turn.",
});
assert.equal(turn.wakeId, "wake-client-debug");
assert.equal(calls.find((call) => call.url.endsWith("/turn"))?.method, "POST");

let missingError: unknown;
try {
  await client.directDebugContext({
    sessionId: "missing-session",
  });
} catch (error) {
  missingError = error;
}
assert.ok(missingError instanceof DebugApiClientError);
assert.equal(missingError.options.status, 404);
assert.equal(missingError.options.reasonCode, "fake_not_found");

console.log(
  JSON.stringify(
    {
      overviewAttempts,
      sessions: sessions.total,
      mcp: mcp.items.length,
      channels: channels.items.length,
      directDebugSource: context.source,
      directTurnWake: turn.wakeId,
      calls: calls.length,
    },
    null,
    2,
  ),
);

function routeAdmin(url: URL, method: string): Response {
  const routeResult = handleAdminDiagnosticsRequest(
    {
      method,
      url: `${url.pathname}${url.search}`,
      requestId: "fake-admin",
    },
    {
      diagnostics,
      recentEvents,
    },
  );
  return jsonResponse(routeResult.body, routeResult.status);
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
    },
  });
}

function header(
  headers: HeadersInit | undefined,
  name: string,
): string | undefined {
  if (!headers) {
    return undefined;
  }
  if (headers instanceof Headers) {
    return headers.get(name) ?? undefined;
  }
  if (Array.isArray(headers)) {
    return headers.find(([key]) => key.toLowerCase() === name)?.[1];
  }
  return headers[name];
}
