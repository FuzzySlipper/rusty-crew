import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentMessage,
  AgentId,
  ChannelBindingRecord,
  CoreEventKind,
  DenDataUpdate,
  EventSubscription,
  EventReceipt,
  ExternalEvent,
  NormalizedChannelActivityProjection,
  NormalizedChannelOutboundMessage,
  ProfileId,
  ProjectId,
  RunId,
  SessionId,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import {
  ChannelBindingActivityTracker,
  createDenAdapter,
  denChannelsInboundToExternalEvent,
  DenChannelsTransportController,
  InMemoryDenChannelsCursorStore,
  dispatchChannelActivityProjection,
  dispatchChannelMessageProjection,
  ingestAcceptedChannelDecision,
  isExpiredChannelInboundMessage,
  normalizeDenChannelsInboundEvent,
  providerRefsFromBinding,
  projectAgentMessageToChannel,
  projectCoreEventToChannelActivity,
  resolveChannelRoute,
  routeRequestToBridgeArgs,
  toDenChannelsActivityRequest,
  toDenChannelsPostMessageRequest,
} from "./index.js";
import {
  createMemoryDenProjectionSink,
  createSimulatedDenChannelsTransport,
} from "./test-support.js";

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

const sharedChannelBindings: ChannelBindingRecord[] = [
  {
    bindingId: "binding-alpha",
    adapterId,
    provider: "den_channels",
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
    profileId: "prime-profile" as ProfileId,
    externalChannelId: "crew-room",
    externalThreadId: "thread-1",
    status: "active",
  },
  {
    bindingId: "binding-beta",
    adapterId,
    provider: "den_channels",
    agentId: "agent-beta" as AgentId,
    sessionId: "session-beta" as SessionId,
    profileId: "review-profile" as ProfileId,
    externalChannelId: "crew-room",
    externalThreadId: "thread-1",
    status: "active",
  },
];

const mentionedRoute = resolveChannelRoute(
  currentChannelMessage,
  sharedChannelBindings,
);

assert.equal(mentionedRoute.status, "routed");
if (mentionedRoute.status === "routed") {
  assert.equal(mentionedRoute.route.to, "agent-beta");
  assert.deepEqual(routeRequestToBridgeArgs(mentionedRoute.route), [
    "channel:den_channels:den-user-beta",
    "agent-beta",
    "current hello",
  ]);
}

const explicitBindingRoute = resolveChannelRoute(
  legacyChannelMessage,
  sharedChannelBindings,
);
assert.equal(explicitBindingRoute.status, "routed");
if (explicitBindingRoute.status === "routed") {
  assert.equal(explicitBindingRoute.route.to, "agent-alpha");
  assert.equal(explicitBindingRoute.route.bindingId, "binding-alpha");
}

const ambiguousMessage = {
  ...legacyChannelMessage,
  bindingId: "unresolved-binding",
  runtime: {},
  mentions: [],
};
const ambiguousRoute = resolveChannelRoute(
  ambiguousMessage,
  sharedChannelBindings,
);
assert.equal(ambiguousRoute.status, "ambiguous");
assert.equal(ambiguousRoute.candidates.length, 2);

const inactiveRoute = resolveChannelRoute(legacyChannelMessage, [
  {
    ...sharedChannelBindings[0]!,
    status: "degraded",
    degradedReason: "transport disconnected",
  },
]);
assert.equal(inactiveRoute.status, "inactive_binding");

const bindingActivity = new ChannelBindingActivityTracker();
const alphaProviderRefs = providerRefsFromBinding(sharedChannelBindings[0]!);
const alphaMembership = bindingActivity.upsertMembership({
  bindingId: "binding-alpha",
  adapterId,
  providerRefs: alphaProviderRefs,
  externalUserId: "den-user-alpha",
  displayLabel: "Ada",
  agentId: "agent-alpha" as AgentId,
  profileId: "prime-profile" as ProfileId,
  roleLabels: ["prime", "coder"],
  status: "joined",
  observedAt: "2026-06-20T05:01:00.000Z",
});

assert.equal(alphaMembership.status, "joined");
assert.deepEqual(alphaMembership.roleLabels, ["prime", "coder"]);

const alphaPresence = bindingActivity.observePresence({
  bindingId: "binding-alpha",
  adapterId,
  providerRefs: alphaProviderRefs,
  externalUserId: "den-user-alpha",
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  status: "idle",
  observedAt: "2026-06-20T05:01:00.000Z",
  expiresAt: "2026-06-20T05:02:00.000Z",
});

assert.equal(alphaPresence.status, "idle");
assert.equal(sharedChannelBindings[0]!.sessionId, "session-alpha");

const rustSubscriptions: EventSubscription[] = [];
const unsubscribedRustHandles: SubscriptionHandle[] = [];
const rustEventSubscriptionClient = {
  async subscribeEvents(
    subscription: EventSubscription,
  ): Promise<SubscriptionHandle> {
    rustSubscriptions.push(subscription);
    return 7 as SubscriptionHandle;
  },
  async unsubscribeEvents(handle: SubscriptionHandle): Promise<void> {
    unsubscribedRustHandles.push(handle);
  },
};

const alphaSubscription = await bindingActivity.subscribeRustEvents(
  rustEventSubscriptionClient,
  {
    binding: sharedChannelBindings[0]!,
    adapterId,
    eventKinds: [
      "agent_message_routed",
      "brain_wake_requested",
    ] as CoreEventKind[],
    observedAt: "2026-06-20T05:01:10.000Z",
  },
);

assert.equal(alphaSubscription.status, "active");
assert.equal(alphaSubscription.transportKind, "rust_event_subscription");
assert.equal(alphaSubscription.rustSubscriptionHandle, 7);
assert.equal(rustSubscriptions[0]?.sessionId, "session-alpha");

const healthyDiagnostics = bindingActivity.diagnostics(
  "2026-06-20T05:01:30.000Z",
);
assert.equal(healthyDiagnostics[0]?.membershipStatus, "joined");
assert.equal(healthyDiagnostics[0]?.presenceStatus, "idle");
assert.equal(healthyDiagnostics[0]?.subscriptionStatus, "active");
assert.equal(healthyDiagnostics[0]?.stale, false);

const degradedSubscription = bindingActivity.markSubscriptionDegraded(
  "binding-alpha",
  "subscription cursor stale",
  "2026-06-20T05:01:40.000Z",
);

assert.equal(degradedSubscription?.status, "degraded");
assert.equal(degradedSubscription?.degradedReason, "subscription cursor stale");

const staleDiagnostics = bindingActivity.diagnostics(
  "2026-06-20T05:02:01.000Z",
);
assert.equal(staleDiagnostics[0]?.stale, true);
assert.equal(staleDiagnostics[0]?.degradedReason, "subscription cursor stale");

const archivedSubscription = await bindingActivity.unsubscribeRustEvents(
  rustEventSubscriptionClient,
  "binding-alpha",
  "2026-06-20T05:02:05.000Z",
);

assert.equal(archivedSubscription?.status, "archived");
assert.deepEqual(unsubscribedRustHandles, [7]);

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

const channelIngressEvents: ExternalEvent[] = [];
const channelIngressRoutes: AgentMessage[] = [];
const channelIngressBootstraps: string[] = [];
const channelIngressOrder: string[] = [];
let channelIngressSequence = 100;
const channelIngressBridge = {
  injectExternalEvent(event: ExternalEvent): EventReceipt {
    channelIngressEvents.push(event);
    channelIngressOrder.push("inject");
    channelIngressSequence += 1;
    return { accepted: true, sequence: channelIngressSequence };
  },
  routeAgentMessage(message: AgentMessage): EventReceipt {
    channelIngressRoutes.push(message);
    channelIngressOrder.push("route");
    channelIngressSequence += 1;
    return { accepted: true, sequence: channelIngressSequence };
  },
};

const acceptedIngress = await ingestAcceptedChannelDecision(acceptedInbound, {
  bridge: channelIngressBridge,
  bindings: sharedChannelBindings,
  ensureSessionForRoute(request) {
    channelIngressBootstraps.push(request.binding.bindingId);
    channelIngressOrder.push("bootstrap");
    return {
      sessionId: request.binding.sessionId ?? "session-alpha",
      agentId: request.binding.agentId,
      profileId: request.binding.profileId,
      status: "active",
    };
  },
  now: "2026-06-20T05:01:01.000Z",
});
assert.equal(acceptedIngress.status, "routed");
if (acceptedIngress.status === "routed") {
  assert.equal(acceptedIngress.session?.sessionId, "session-alpha");
  assert.equal(acceptedIngress.externalEvent.payload.type, "channel_message");
  assert.equal(acceptedIngress.routedMessage.to, "agent-alpha");
  assert.match(
    acceptedIngress.routedMessage.correlationId ?? "",
    /^channel:binding-alpha:/,
  );
}
assert.equal(channelIngressEvents.length, 1);
assert.equal(channelIngressRoutes.length, 1);
assert.deepEqual(channelIngressBootstraps, ["binding-alpha"]);
assert.deepEqual(channelIngressOrder, ["bootstrap", "inject", "route"]);

const duplicateIngress = await ingestAcceptedChannelDecision(duplicateInbound, {
  bridge: channelIngressBridge,
  bindings: sharedChannelBindings,
  now: "2026-06-20T05:01:02.000Z",
});
assert.equal(duplicateIngress.status, "duplicate");
assert.equal(channelIngressEvents.length, 1);
assert.equal(channelIngressRoutes.length, 1);
assert.equal(channelIngressBootstraps.length, 1);

const staleIngress = await ingestAcceptedChannelDecision(staleInbound, {
  bridge: channelIngressBridge,
  bindings: sharedChannelBindings,
  now: "2026-06-20T05:01:03.000Z",
});
assert.equal(staleIngress.status, "stale_cursor");
assert.equal(channelIngressEvents.length, 1);
assert.equal(channelIngressRoutes.length, 1);
assert.equal(channelIngressBootstraps.length, 1);

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

const expiredIngress = await ingestAcceptedChannelDecision(expiredInbound, {
  bridge: channelIngressBridge,
  bindings: sharedChannelBindings,
  now: "2026-06-20T05:01:06.000Z",
});
assert.equal(expiredIngress.status, "expired");
assert.equal(channelIngressEvents.length, 1);
assert.equal(channelIngressRoutes.length, 1);
assert.equal(channelIngressBootstraps.length, 1);

const replyCorrelation =
  acceptedIngress.status === "routed"
    ? acceptedIngress.routedMessage.correlationId
    : undefined;
const outboundProjection = projectAgentMessageToChannel(
  {
    from: "agent-alpha" as AgentId,
    to: "channel:den_channels:den-user-alpha" as AgentId,
    body: "Thanks for the update. This reply is intentionally long enough to prove channel projection text bounds.",
    correlationId: replyCorrelation,
  },
  sharedChannelBindings,
  { maxBodyChars: 48 },
);

assert.equal(outboundProjection.status, "projected");
if (outboundProjection.status === "projected") {
  assert.equal(outboundProjection.message.bindingId, "binding-alpha");
  assert.equal(outboundProjection.message.correlationId, replyCorrelation);
  assert.match(outboundProjection.message.body, /\[truncated\]$/);
  assert.equal(
    toDenChannelsPostMessageRequest(outboundProjection.message).metadata
      .correlationId,
    replyCorrelation,
  );
}

const channelActivity = projectCoreEventToChannelActivity(
  {
    type: "completion_packet_delivered",
    packet: {
      sessionId: "session-alpha" as SessionId,
      status: "completed",
      summary: "large result payload should live behind result refs",
    },
  },
  sharedChannelBindings[0]!,
  { now: "2026-06-20T05:03:00.000Z", maxSummaryChars: 80 },
);
assert.equal(channelActivity.severity, "success");
assert.equal(channelActivity.resultRef, "completion:session-alpha:completed");
assert.equal(
  toDenChannelsActivityRequest(channelActivity).metadata.bindingId,
  "binding-alpha",
);

const projectedChannelMessages: string[] = [];
const projectedChannelActivities: string[] = [];
let failNextActivityProjection = true;
const channelProjectionSink = {
  sendMessage(message: NormalizedChannelOutboundMessage): void {
    projectedChannelMessages.push(message.body);
  },
  sendActivity(activity: NormalizedChannelActivityProjection): void {
    if (failNextActivityProjection) {
      failNextActivityProjection = false;
      throw new Error("channel activity sink unavailable");
    }
    projectedChannelActivities.push(activity.summary);
  },
};

if (outboundProjection.status === "projected") {
  const messageDispatch = await dispatchChannelMessageProjection(
    channelProjectionSink,
    outboundProjection.message,
  );
  assert.equal(messageDispatch.accepted, true);
}

const droppedActivityDispatch = await dispatchChannelActivityProjection(
  channelProjectionSink,
  channelActivity,
);
assert.equal(droppedActivityDispatch.accepted, false);
const acceptedActivityDispatch = await dispatchChannelActivityProjection(
  channelProjectionSink,
  channelActivity,
);
assert.equal(acceptedActivityDispatch.accepted, true);
assert.equal(projectedChannelMessages.length, 1);
assert.equal(projectedChannelActivities.length, 1);

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
      channelMembershipStatus: alphaMembership.status,
      channelPresenceStale: staleDiagnostics[0]?.stale,
      channelSubscriptionStatus: archivedSubscription?.status,
      channelIngressStatus: acceptedIngress.status,
      channelIngressEvents: channelIngressEvents.length,
      channelIngressRoutes: channelIngressRoutes.length,
      outboundProjectionStatus: outboundProjection.status,
      channelActivitySeverity: channelActivity.severity,
      droppedActivityProjection:
        droppedActivityDispatch.accepted === false
          ? droppedActivityDispatch.degradedReason
          : undefined,
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
