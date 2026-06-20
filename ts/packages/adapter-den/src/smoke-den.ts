import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  DenDataUpdate,
  EventReceipt,
  ExternalEvent,
  ProjectId,
  RunId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createDenAdapter,
  createMemoryDenProjectionSink,
  createSimulatedDenChannelsTransport,
  denChannelsInboundToExternalEvent,
  DenChannelsTransportController,
  InMemoryDenChannelsCursorStore,
  isExpiredChannelInboundMessage,
  normalizeDenChannelsInboundEvent,
  toDenChannelsActivityRequest,
  toDenChannelsPostMessageRequest,
} from "./index.js";

const adapterId = "den" as AdapterId;
let sequence = 0;
const denUpdates: DenDataUpdate[] = [];
const externalEvents: ExternalEvent[] = [];

const ingress = {
  injectDenDataUpdate(update: DenDataUpdate): EventReceipt {
    denUpdates.push(update);
    sequence += 1;
    return { accepted: true, sequence };
  },
  injectExternalEvent(event: ExternalEvent): EventReceipt {
    externalEvents.push(event);
    sequence += 1;
    return { accepted: true, sequence };
  },
};

const projectionSink = createMemoryDenProjectionSink();
const adapter = createDenAdapter({
  adapterId,
  ingress,
  projectionSink,
});

const updateReceipt = await adapter.injectDataUpdate({
  projectId: "pi-crew" as ProjectId,
  entityKind: "task",
  entityId: "2767",
  revision: "smoke-revision",
});

assert.equal(updateReceipt.accepted, true);
assert.equal(denUpdates.length, 1);

await adapter.injectExternalEventPayload("den", {
  type: "adapter_status",
  status: "connected",
});

assert.equal(externalEvents.length, 1);

const projectionResult = await adapter.projectEvent({
  type: "agent_message_routed",
  message: {
    from: "planner" as AgentId,
    to: "worker" as AgentId,
    body: "internal routing is observed, not delegated to Den",
  },
});

assert.equal(projectionResult.accepted, true);
assert.equal(projectionSink.projections.length, 1);

const lifecycleProjection = await adapter.projectEvent({
  type: "delegation_lifecycle_observed",
  lifecycle: {
    parentSessionId: "planner-session" as SessionId,
    delegatedSessionId: "planner-session:delegated:wake:0" as SessionId,
    runId: "wake:0" as RunId,
    phase: "cancelled",
  },
});

assert.equal(lifecycleProjection.accepted, true);
const lifecycleSummary = projectionSink.projections.at(-1)?.summary;
assert.ok(lifecycleSummary);
assert.match(lifecycleSummary, /delegation cancelled/);

projectionSink.failNext(new Error("simulated Den outage"));
const droppedProjection = await adapter.projectEvent({
  type: "den_data_updated",
  update: denUpdates[0]!,
});

assert.equal(droppedProjection.dropped, true);
assert.equal(adapter.status().state, "degraded");
assert.equal(adapter.status().droppedProjections, 1);

const postOutageReceipt = await adapter.injectExternalEventPayload("den", {
  type: "adapter_status",
  status: "disconnected",
  detail: "observability projection unavailable",
});

assert.equal(postOutageReceipt.accepted, true);
assert.equal(externalEvents.length, 2);

const legacyChannelMessage = normalizeDenChannelsInboundEvent(
  {
    type: "message",
    channelId: "crew-room",
    threadId: "thread-1",
    messageId: "legacy-message-1",
    userId: "den-user-alpha",
    userLabel: "Ada",
    text: "legacy hello",
    receivedAt: "2026-06-20T05:00:00.000Z",
    cursor: "legacy-cursor-1",
    mentions: ["agent-alpha"],
  },
  {
    adapterId,
    bindingId: "binding-alpha",
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
    ttlMs: 10_000,
  },
);

assert.equal(legacyChannelMessage.kind, "channel_inbound_message.v1");
assert.equal(legacyChannelMessage.providerRefs.provider, "den_channels");
assert.equal(legacyChannelMessage.providerRefs.externalChannelId, "crew-room");
assert.equal(
  legacyChannelMessage.idempotencyKey,
  "den_channels:crew-room:thread-1:legacy-message-1",
);
assert.equal(
  isExpiredChannelInboundMessage(
    legacyChannelMessage,
    "2026-06-20T05:00:09.000Z",
  ),
  false,
);

const currentChannelMessage = normalizeDenChannelsInboundEvent(
  {
    kind: "channel.message.created",
    channel: { id: "crew-room" },
    thread: { id: "thread-1" },
    message: {
      id: "current-message-1",
      body: "current hello",
      createdAt: "2026-06-20T05:00:00.000Z",
    },
    author: { id: "den-user-beta", displayName: "Grace" },
    cursor: "current-cursor-1",
    mentions: [{ id: "agent-beta" }],
    attachments: [{ id: "att-1", mediaType: "text/plain" }],
  },
  {
    adapterId,
    bindingId: "binding-beta",
    agentId: "agent-beta" as AgentId,
    sessionId: "session-beta" as SessionId,
    ttlMs: 5_000,
  },
);

assert.equal(currentChannelMessage.body, "current hello");
assert.equal(currentChannelMessage.mentions[0], "agent-beta");
assert.equal(currentChannelMessage.attachments[0]?.ref, "att-1");
assert.equal(
  isExpiredChannelInboundMessage(
    currentChannelMessage,
    "2026-06-20T05:00:06.000Z",
  ),
  true,
);

const bridgeEvent = denChannelsInboundToExternalEvent(
  legacyChannelMessage,
  "2026-06-20T05:00:09.000Z",
);
assert.equal(bridgeEvent.source, "den_channels:binding-alpha");
assert.equal(bridgeEvent.payload.type, "human_message");
assert.equal(bridgeEvent.payload.text, "legacy hello");

const outboundRequest = toDenChannelsPostMessageRequest({
  kind: "channel_outbound_message.v1",
  adapterId,
  bindingId: "binding-alpha",
  runtime: {
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
  },
  providerRefs: {
    provider: "den_channels",
    externalChannelId: "crew-room",
    externalThreadId: "thread-1",
  },
  body: "projection reply",
  correlationId: "corr-channel-1",
  idempotencyKey: "outbound-1",
  visibility: "conversation",
  deliveryPolicy: "best_effort",
  workRef: "work:2929",
});

assert.equal(outboundRequest.channelId, "crew-room");
assert.equal(outboundRequest.metadata.bindingId, "binding-alpha");
assert.equal(outboundRequest.metadata.correlationId, "corr-channel-1");

const activityRequest = toDenChannelsActivityRequest({
  kind: "channel_activity_projection.v1",
  adapterId,
  bindingId: "binding-alpha",
  runtime: {
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
  },
  providerRefs: {
    provider: "den_channels",
    externalChannelId: "crew-room",
    externalThreadId: "thread-1",
  },
  eventType: "adapter_degraded",
  summary: "projection degraded",
  severity: "warning",
  createdAt: "2026-06-20T05:01:00.000Z",
});

assert.equal(activityRequest.eventType, "adapter_degraded");
assert.equal(activityRequest.severity, "warning");

const staleLocalCursorStore = new InMemoryDenChannelsCursorStore();
staleLocalCursorStore.write("binding-alpha", "2");
const primaryTransport = createSimulatedDenChannelsTransport("ws-primary");
const fallbackTransport = createSimulatedDenChannelsTransport("http-fallback");
primaryTransport.failNextOpen(new Error("ws unavailable"));

const transportController = new DenChannelsTransportController({
  binding: {
    adapterId,
    bindingId: "binding-alpha",
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
    ttlMs: 5_000,
  },
  cursorStore: staleLocalCursorStore,
  cursorKey: "binding-alpha",
  transports: [primaryTransport, fallbackTransport],
  retryPolicy: {
    maxAttempts: 2,
    backoffMs: [100, 500],
  },
  subscriptionCursor: () => "10",
});

const attempts = await transportController.connect();
assert.equal(attempts.length, 2);
assert.equal(attempts[0]?.connected, false);
assert.equal(attempts[0]?.delayMs, 0);
assert.equal(attempts[0]?.cursor, "10");
assert.equal(attempts[1]?.connected, true);
assert.equal(attempts[1]?.transport, "http-fallback");
assert.equal(transportController.status().state, "connected");
assert.deepEqual(fallbackTransport.openedWithCursors, ["10"]);

const acceptedInbound = await transportController.acceptInbound(
  {
    type: "message",
    channelId: "crew-room",
    threadId: "thread-1",
    messageId: "cursor-11",
    userId: "den-user-alpha",
    text: "cursor accepted",
    receivedAt: "2026-06-20T05:01:00.000Z",
    cursor: "11",
  },
  "2026-06-20T05:01:01.000Z",
);

assert.equal(acceptedInbound.accepted, true);
assert.equal(await staleLocalCursorStore.read("binding-alpha"), "11");

const duplicateInbound = await transportController.acceptInbound(
  {
    type: "message",
    channelId: "crew-room",
    threadId: "thread-1",
    messageId: "cursor-11",
    userId: "den-user-alpha",
    text: "cursor accepted again",
    receivedAt: "2026-06-20T05:01:01.000Z",
    cursor: "12",
  },
  "2026-06-20T05:01:02.000Z",
);

assert.equal(duplicateInbound.accepted, false);
assert.equal(duplicateInbound.reason, "duplicate");

const staleInbound = await transportController.acceptInbound(
  {
    type: "message",
    channelId: "crew-room",
    threadId: "thread-1",
    messageId: "cursor-09",
    userId: "den-user-alpha",
    text: "old cursor",
    receivedAt: "2026-06-20T05:01:02.000Z",
    cursor: "9",
  },
  "2026-06-20T05:01:03.000Z",
);

assert.equal(staleInbound.accepted, false);
assert.equal(staleInbound.reason, "stale_cursor");

const expiredInbound = await transportController.acceptInbound(
  {
    type: "message",
    channelId: "crew-room",
    threadId: "thread-1",
    messageId: "cursor-13",
    userId: "den-user-alpha",
    text: "expired",
    receivedAt: "2026-06-20T05:01:00.000Z",
    cursor: "13",
  },
  "2026-06-20T05:01:06.000Z",
);

assert.equal(expiredInbound.accepted, false);
assert.equal(expiredInbound.reason, "expired");

await transportController.send({
  kind: "channel_outbound_message.v1",
  adapterId,
  bindingId: "binding-alpha",
  runtime: {
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
  },
  providerRefs: {
    provider: "den_channels",
    externalChannelId: "crew-room",
    externalThreadId: "thread-1",
  },
  body: "transport send",
  idempotencyKey: "transport-send-1",
  visibility: "conversation",
  deliveryPolicy: "best_effort",
});

assert.equal(fallbackTransport.sent.length, 1);

console.log(
  JSON.stringify(
    {
      registration: adapter.registration(),
      denUpdates: denUpdates.length,
      externalEvents: externalEvents.length,
      projectedEvents: adapter.status().projectedEvents,
      droppedProjections: adapter.status().droppedProjections,
      degradedWithoutBlockingIngress: postOutageReceipt.accepted,
      normalizedChannelShapes: ["legacy", "current"],
      channelBridgePayload: bridgeEvent.payload.type,
      outboundChannelRequest: outboundRequest.channelId,
      transportState: transportController.status().state,
      transportAttempts: attempts.length,
      activeTransport: transportController.status().activeTransport,
      droppedTransportMessages:
        transportController.status().droppedDuplicates +
        transportController.status().droppedStaleCursor +
        transportController.status().droppedExpired,
    },
    null,
    2,
  ),
);
