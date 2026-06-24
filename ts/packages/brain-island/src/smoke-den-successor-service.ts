import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { BrainEventEnvelope } from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-den-successor-service-"));
const adminPort = await openPort();
const gatewayRequests: {
  method: string;
  path: string;
  auth?: string;
  migrated?: string;
  body?: unknown;
}[] = [];
const finalCompletionText = "FINAL_COMPLETION_TEXT_FROM_PACKET";
let deliveryListCount = 0;
const gateway = createHttpServer((request, response) => {
  let body = "";
  request.setEncoding("utf8");
  request.on("data", (chunk) => {
    body += chunk;
  });
  request.on("end", () => {
    gatewayRequests.push({
      method: request.method ?? "GET",
      path: request.url ?? "/",
      auth: request.headers.authorization,
      migrated: request.headers["x-den-migrated-functions"]?.toString(),
      body: body ? (JSON.parse(body) as unknown) : undefined,
    });
    response.setHeader("Content-Type", "application/json");
    if (request.url === "/health") {
      response.end(JSON.stringify({ status: "ok", service_name: "gateway" }));
      return;
    }
    if (request.url === "/v1/runtime/instances") {
      response.statusCode = 201;
      response.end(
        JSON.stringify({
          instance_id: "field-agent@rusty-crew",
          profile_identity: "field-profile",
          host: "test-host",
          state: "starting",
          started_at: "2026-06-21T12:00:00Z",
        }),
      );
      return;
    }
    if (
      request.url === "/v1/runtime/instances/field-agent%40rusty-crew/heartbeat"
    ) {
      response.end(
        JSON.stringify({
          instance_id: "field-agent@rusty-crew",
          profile_identity: "field-profile",
          host: "test-host",
          state: "active",
          started_at: "2026-06-21T12:00:00Z",
          last_heartbeat_at: "2026-06-21T12:00:01Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents?state=pending") {
      deliveryListCount += 1;
      response.end(
        JSON.stringify(
          deliveryListCount === 1
            ? [
                {
                  id: 89,
                  target_identity: {
                    profile: "missing-profile",
                    instance_id: "missing-agent@rusty-crew",
                    session_key: "missing-session",
                  },
                  state: "pending",
                  idempotency_key: "wake:ch42:missing-profile:unmatched",
                  source_ref:
                    "wake://missing-profile?body=unmatched&channel_id=42",
                  channel_message_id: 5,
                  created_at: "2026-06-21T12:00:00Z",
                  expires_at: "2026-06-21T12:05:00Z",
                },
                {
                  id: 90,
                  target_identity: {
                    profile: "field-profile",
                    instance_id: "field-agent@rusty-crew",
                    session_key: "field-session",
                  },
                  state: "pending",
                  idempotency_key: "wake:ch42:field-profile:expired",
                  source_ref:
                    "wake://field-profile?body=stale%20message&channel_id=42",
                  channel_message_id: 6,
                  created_at: "2026-06-21T11:00:00Z",
                  expires_at: "2026-06-21T11:59:00Z",
                },
                {
                  id: 91,
                  target_identity: {
                    profile: "field-profile",
                    instance_id: "field-agent@rusty-crew",
                    session_key: "field-session",
                  },
                  state: "pending",
                  idempotency_key: "wake:ch42:field-profile:nonce",
                  source_ref: "/api/v1/conversation/channels/42/messages/7",
                  channel_message_id: 7,
                  created_at: "2026-06-21T12:00:00Z",
                  expires_at: "2026-06-21T12:05:00Z",
                },
              ]
            : [],
        ),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents/91/claim") {
      response.end(
        JSON.stringify({
          id: 91,
          target_identity: {
            profile: "field-profile",
            instance_id: "field-agent@rusty-crew",
            session_key: "field-session",
          },
          state: "claimed",
          idempotency_key: "wake:ch42:field-profile:nonce",
          created_at: "2026-06-21T12:00:00Z",
          expires_at: "2026-06-21T12:05:00Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents/91/events") {
      response.end(JSON.stringify({ accepted: true }));
      return;
    }
    if (
      request.url === "/v1/conversation/channels/42/messages?limit=5&after_id=6"
    ) {
      response.end(
        JSON.stringify([
          {
            id: 7,
            channel_id: 42,
            body: "hello from den",
            sender_type: "user",
            sender_identity: "Patch",
            created_at: "2026-06-21T12:00:00Z",
          },
        ]),
      );
      return;
    }
    if (request.url === "/v1/conversation/channels/42/messages") {
      response.statusCode = 201;
      response.end(JSON.stringify({ id: 92, channel_id: 42 }));
      return;
    }
    if (
      request.url ===
      "/v1/conversation/memberships?project_id=field-project&include_left=true&limit=100"
    ) {
      response.end(
        JSON.stringify([
          {
            id: 93,
            channel_id: 42,
            member_type: "agent",
            member_identity: "field-agent",
            profile_identity: "field-profile",
            membership_status: "active",
            wake_policy: "subscription",
            can_send: true,
            can_react: true,
            can_invite: false,
            membership_purpose: "normal",
            settings: {},
            created_at: "2026-06-21T12:00:00Z",
            updated_at: "2026-06-21T12:00:00Z",
          },
          {
            id: 94,
            channel_id: 42,
            member_type: "agent",
            member_identity: "field-agent",
            profile_identity: "field-profile",
            membership_status: "left",
            wake_policy: "never",
            can_send: false,
            can_react: false,
            can_invite: false,
            membership_purpose: "ordinary",
            settings: {},
            created_at: "2026-06-21T11:00:00Z",
            updated_at: "2026-06-21T11:30:00Z",
            left_at: "2026-06-21T11:30:00Z",
          },
        ]),
      );
      return;
    }
    response.end(JSON.stringify({ accepted: true }));
  });
});
gateway.listen(0, "127.0.0.1");
await once(gateway, "listening");
const gatewayAddress = gateway.address();
assert.equal(typeof gatewayAddress, "object");
if (typeof gatewayAddress !== "object" || gatewayAddress === null) {
  throw new Error("expected fake Gateway address");
}

writeRuntimeConfig(root);
const bridge = withToolEventBridge(await loadNativeBridge());
const host = await startRustyCrewServiceHost({
  bridge,
  env: {
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_PORT: String(adminPort),
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    DEN_SUCCESSOR_GATEWAY_URL: `http://127.0.0.1:${gatewayAddress.port}`,
    DEN_SUCCESSOR_RUNTIME_TOKEN: "runtime-token",
    DEN_SUCCESSOR_OBSERVATION_WRITE_TOKEN: "observation-write-token",
    DEN_SUCCESSOR_DELIVERY_TOKEN: "delivery-token",
    DEN_SUCCESSOR_CONVERSATION_WRITE_TOKEN: "conversation-write-token",
    DEN_SUCCESSOR_CONVERSATION_READ_TOKEN: "conversation-read-token",
    RUSTY_CREW_DEN_RUNTIME_HEARTBEAT_INTERVAL_MS: "0",
    RUSTY_CREW_DEN_DELIVERY_POLL_INTERVAL_MS: "50",
  },
  now: () => "2026-06-21T12:01:00.000Z",
});

try {
  assert.deepEqual(gatewayRequests.slice(0, 4).map(requestKey), [
    "GET /health",
    "POST /v1/runtime/instances",
    "POST /v1/runtime/instances/field-agent%40rusty-crew/heartbeat",
    "POST /v1/observation/activity-events",
  ]);
  assert.equal(gatewayRequests[1]?.auth, "Bearer runtime-token");
  assert.equal(gatewayRequests[2]?.auth, "Bearer runtime-token");
  assert.equal(gatewayRequests[3]?.auth, "Bearer observation-write-token");
  assert.equal(gatewayRequests[1]?.migrated, "true");
  assert.equal(gatewayRequests[2]?.migrated, "true");
  assert.equal(gatewayRequests[3]?.migrated, "true");
  assert.equal(
    (
      gatewayRequests.find(
        (request) => request.path === "/v1/observation/activity-events",
      )?.body as { event_type?: string } | undefined
    )?.event_type,
    "adapter_connected",
  );

  await waitUntil(
    () =>
      gatewayRequests.some(
        (request) =>
          request.method === "POST" &&
          request.path === "/v1/conversation/channels/42/messages",
      ),
    "delivery intent completion was projected to Conversation",
  );
  assert.equal(
    gatewayRequests.some((request) =>
      request.path.startsWith("/v1/conversation/channels?"),
    ),
    false,
  );
  assert.equal(
    gatewayRequests.find(
      (request) =>
        request.path ===
        "/v1/conversation/memberships?project_id=field-project&include_left=true&limit=100",
    )?.auth,
    "Bearer conversation-read-token",
  );
  assert.equal(
    gatewayRequests.some(
      (request) => request.path === "/v1/delivery/intents/90/claim",
    ),
    false,
  );
  assert.equal(
    gatewayRequests.find(
      (request) => request.path === "/v1/delivery/intents/91/claim",
    )?.auth,
    "Bearer delivery-token",
  );
  assert.deepEqual(
    (
      gatewayRequests.find(
        (request) => request.path === "/v1/delivery/intents/91/claim",
      )?.body as { claimed_by?: unknown } | undefined
    )?.claimed_by,
    {
      profile: "field-profile",
      instance_id: "field-agent@rusty-crew",
      session_key: "field-session",
    },
  );
  assert.equal(
    gatewayRequests.find(
      (request) => request.path === "/v1/conversation/channels/42/messages",
    )?.auth,
    "Bearer conversation-write-token",
  );
  assert.match(
    (
      gatewayRequests.find(
        (request) => request.path === "/v1/conversation/channels/42/messages",
      )?.body as { body?: string } | undefined
    )?.body ?? "",
    new RegExp(finalCompletionText),
  );
  const lifecycleEvents = gatewayRequests
    .filter((request) => request.path === "/v1/delivery/intents/91/events")
    .map(
      (request) =>
        (request.body as { event_type?: string } | undefined)?.event_type,
    );
  assert.deepEqual(lifecycleEvents, ["running", "completed"]);
  const activityEvents = gatewayRequests
    .filter((request) => request.path === "/v1/observation/activity-events")
    .map((request) => request.body as ObservationActivityRequest);
  assert.deepEqual(
    activityEvents.map((event) => event.event_type),
    ["adapter_connected", "tool_call_started", "tool_call_completed"],
  );
  assert.deepEqual(activityEvents[1]?.agent_identity, {
    profile: "field-profile",
    instance_id: "field-agent@rusty-crew",
    session_key: "field-session",
  });
  assert.equal(
    activityEvents[1]?.runtime_instance_id,
    "field-agent@rusty-crew",
  );
  assert.equal(activityEvents[1]?.payload.kind, "agent_activity.v1");
  assert.equal(activityEvents[1]?.payload.tool_name, "den_memory_recall");
  assert.equal(activityEvents[1]?.payload.adapter, "rusty-crew");
  assert.equal(activityEvents[1]?.payload.visibility, "channel");
  const toolWorkRef = activityEvents[1]?.payload.work_ref as
    | {
        session_id?: string;
        run_id?: string;
        channel_id?: number;
        channel_message_id?: number;
      }
    | undefined;
  assert.deepEqual(
    {
      session_id: toolWorkRef?.session_id,
      channel_id: toolWorkRef?.channel_id,
      channel_message_id: toolWorkRef?.channel_message_id,
    },
    {
      session_id: "field-session",
      channel_id: 42,
      channel_message_id: 7,
    },
  );
  assert.match(
    toolWorkRef?.run_id ?? "",
    /^delivery_intent:91;wake:service-field-session-[0-9]+-1$/,
  );

  const diagnostics = await fetch(
    `http://127.0.0.1:${adminPort}/v1/admin/events/recent`,
  );
  const body = (await diagnostics.json()) as {
    data: { items: { eventType: string; summary: string }[] };
  };
  assert.equal(
    body.data.items.some(
      (event) => event.eventType === "den_successor_gateway_connected",
    ),
    true,
  );
  assert.equal(
    body.data.items.some(
      (event) => event.eventType === "den_delivery_intent_expired",
    ),
    true,
  );
  assert.equal(
    body.data.items.some(
      (event) => event.eventType === "den_delivery_intent_unmatched",
    ),
    true,
  );
  const channelDiagnostics = await fetch(
    `http://127.0.0.1:${adminPort}/v1/admin/diagnostics/channels`,
  );
  const channelBody = (await channelDiagnostics.json()) as {
    data: {
      items: {
        bindingSource?: string;
        status?: string;
        conversationChannelId?: number;
        deliveryIntentId?: number;
      }[];
    };
  };
  const configuredChannel = channelBody.data.items.find(
    (item) => item.bindingSource === "configured",
  );
  assert.deepEqual(configuredChannel, {
    bindingId: "field-channel",
    bindingSource: "configured",
    adapterId: "den-channel-main",
    agentId: "field-agent",
    sessionId: "field-session",
    profileId: "field-profile",
    provider: "den_channels",
    externalChannelId: "field-channel",
    conversationProjectId: "field-project",
    wakePolicy: "subscription",
    status: "active",
    membershipStatus: "joined",
    presenceStatus: "online",
    subscriptionStatus: "active",
    stalePresence: false,
    droppedProjections: 0,
    conversationChannelId: 42,
  });
  const dynamicChannel = channelBody.data.items.find(
    (item) => item.bindingSource === "gateway_delivery",
  );
  assert.deepEqual(dynamicChannel, {
    bindingId: "gateway-delivery:field-session:42",
    bindingSource: "gateway_delivery",
    adapterId: "den-successor-gateway",
    agentId: "field-agent",
    sessionId: "field-session",
    profileId: "field-profile",
    provider: "den_successor_gateway",
    externalChannelId: "conversation:42",
    conversationChannelId: 42,
    sourceMessageId: 7,
    deliveryIntentId: 91,
    lastObservedAt: "2026-06-21T12:01:00.000Z",
    wakePolicy: "subscription",
    status: "active",
    membershipStatus: "dynamic",
    presenceStatus: "delivery_intent",
    subscriptionStatus: "active",
    stalePresence: false,
    droppedProjections: 0,
  });

  console.log(
    JSON.stringify(
      {
        gatewayRequests: gatewayRequests.length,
        startupEvent: body.data.items[0]?.eventType,
        lifecycleEvents,
        channelDiagnostics: channelBody.data.items.length,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop();
  gateway.close();
  rmSync(root, { recursive: true, force: true });
}

interface ObservationActivityRequest {
  event_type: string;
  agent_identity?: unknown;
  runtime_instance_id?: string;
  payload: {
    kind?: string;
    adapter?: string;
    tool_name?: string;
    visibility?: string;
    work_ref?: unknown;
  };
}

function requestKey(request: { method: string; path: string }): string {
  return `${request.method} ${request.path}`;
}

function withToolEventBridge(bridge: NativeBridgeModule): NativeBridgeModule {
  return {
    ...bridge,
    registerBrainRuntime: async (registration, executor) => {
      const wrappedExecutor: Parameters<
        NativeBridgeModule["registerBrainRuntime"]
      >[1] = {
        wake: async (request, buffers) => {
          const result = await executor.wake(request, buffers);
          return {
            ...result,
            events: withToolAndStreamingEvents(result.events),
            actions: result.actions.map((action) =>
              action.type === "deliver_completion"
                ? {
                    ...action,
                    packet: {
                      ...action.packet,
                      summary: finalCompletionText,
                    },
                  }
                : action,
            ),
          };
        },
      };
      return bridge.registerBrainRuntime(registration, wrappedExecutor);
    },
  };
}

function withToolAndStreamingEvents(
  events: readonly BrainEventEnvelope[],
): BrainEventEnvelope[] {
  const started = events[0];
  if (started === undefined) return [...events];
  const inserted: BrainEventEnvelope[] = [
    {
      wakeId: started.wakeId,
      sessionId: started.sessionId,
      event: { type: "tool_call_started", toolName: "den_memory_recall" },
    },
    {
      wakeId: started.wakeId,
      sessionId: started.sessionId,
      event: {
        type: "tool_call_finished",
        toolName: "den_memory_recall",
        isError: false,
      },
    },
  ];
  const textEvents = Array.from({ length: 70 }, (_, index) => ({
    wakeId: started.wakeId,
    sessionId: started.sessionId,
    event: {
      type: "text_delta" as const,
      text: index === 69 ? finalCompletionText : "x",
    },
  }));
  const finishedIndex = events.findIndex(
    (event) => event.event.type === "finished",
  );
  if (finishedIndex < 0) return [...events, ...inserted, ...textEvents];
  return [
    ...events.slice(0, finishedIndex),
    ...inserted,
    ...textEvents,
    ...events.slice(finishedIndex),
  ];
}

async function waitUntil(
  predicate: () => boolean,
  description: string,
  timeoutMs = 3_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.fail(`timed out waiting for ${description}`);
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createTcpServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const port = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(port);
      });
    });
  });
}

function writeRuntimeConfig(targetRoot: string): void {
  const configDir = join(targetRoot, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "field-profile" }],
        sessions: [
          {
            sessionId: "field-session",
            agentId: "field-agent",
            profileId: "field-profile",
            kind: "full",
          },
        ],
        channelBindings: [
          {
            bindingId: "field-channel",
            adapterId: "den-channel-main",
            provider: "den_channels",
            agentId: "field-agent",
            sessionId: "field-session",
            profileId: "field-profile",
            externalChannelId: "field-channel",
            conversationProjectId: "field-project",
            conversationChannelId: 42,
            status: "active",
          },
        ],
        mcpBindings: [],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "field-profile.json"),
    JSON.stringify(
      {
        profileId: "field-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        toolPolicy: { requestedTools: [] },
      },
      null,
      2,
    ),
  );
}
