import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  DenChannelsTransportController,
  dispatchChannelActivityProjection,
  dispatchChannelMessageProjection,
  ingestAcceptedChannelDecision,
  ingestChannelInboundMessage,
  InMemoryDenChannelsCursorStore,
  projectAgentMessageToChannel,
  projectCoreEventToChannelActivity,
} from "@rusty-crew/adapter-den";
import { createSimulatedDenChannelsTransport } from "@rusty-crew/adapter-den/test-support";
import type {
  AdapterId,
  AgentId,
  ChannelBindingRecord,
  CoreEvent,
  NormalizedChannelInboundMessage,
  NormalizedChannelActivityProjection,
  NormalizedChannelOutboundMessage,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { ensureConfiguredSessionForChannelBinding } from "./service-runtime-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

const adapterId = "den-channel-main" as AdapterId;
const bindings: ChannelBindingRecord[] = [
  channelBinding("binding-alpha", "agent-alpha", "session-alpha", "prime"),
  channelBinding("binding-beta", "agent-beta", "session-beta", "review"),
  channelBinding(
    "binding-alpha-telegram",
    "agent-alpha",
    "session-alpha",
    "prime",
    {
      provider: "telegram",
      externalChannelId: "telegram-room",
      externalThreadId: "telegram-thread",
      externalUserId: "telegram-user-alpha",
    },
  ),
];
const alphaTransport = createSimulatedDenChannelsTransport("alpha-ws");
const betaTransport = createSimulatedDenChannelsTransport("beta-ws");
const alphaController = channelController(bindings[0]!, alphaTransport);
const betaController = channelController(bindings[1]!, betaTransport);

await alphaController.connect();
await betaController.connect();

const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-den-channels-e2e-engine-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T11:30:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const runtimeConfig: RustyCrewRuntimeConfig = {
    profilesDir: engineDataDir,
    brains: [],
    sessions: [
      {
        sessionId: "session-alpha" as SessionId,
        agentId: "agent-alpha" as AgentId,
        profileId: "prime" as ProfileId,
        kind: "full",
      },
      {
        sessionId: "session-beta" as SessionId,
        agentId: "agent-beta" as AgentId,
        profileId: "review" as ProfileId,
        kind: "full",
      },
    ],
    scheduledJobs: [],
    channelBindings: bindings,
    mcpBindings: [],
  };
  const events = await native.subscribeEvents({
    eventKinds: ["agent_message_routed", "brain_wake_requested"],
  });
  const bridge = {
    injectExternalEvent: native.injectExternalEvent,
    routeAgentMessage(message: {
      from: AgentId;
      to: AgentId;
      body: string;
      correlationId?: string;
    }) {
      return native.routeAgentMessage(
        message.from,
        message.to,
        message.body,
        message.correlationId,
      );
    },
  };
  const ensureSessionForRoute = async (request: {
    binding: ChannelBindingRecord;
  }) =>
    ensureConfiguredSessionForChannelBinding({
      bridge: native,
      runtimeConfig,
      binding: request.binding,
    });

  const alphaInbound = await alphaController.acceptInbound(
    {
      type: "message",
      channelId: "crew-room",
      threadId: "thread-alpha",
      messageId: "alpha-1",
      userId: "den-user-alpha",
      text: "alpha asks for status",
      receivedAt: "2026-06-20T11:30:01.000Z",
      cursor: "1",
    },
    "2026-06-20T11:30:02.000Z",
  );
  const betaInbound = await betaController.acceptInbound(
    {
      kind: "channel.message.created",
      channel: { id: "crew-room" },
      thread: { id: "thread-beta" },
      message: {
        id: "beta-1",
        text: "beta asks for review",
        createdAt: "2026-06-20T11:30:03.000Z",
      },
      author: { id: "den-user-beta", displayName: "Bea" },
      cursor: "1",
    },
    "2026-06-20T11:30:04.000Z",
  );

  const alphaIngress = await ingestAcceptedChannelDecision(alphaInbound, {
    bridge,
    bindings,
    ensureSessionForRoute,
    now: "2026-06-20T11:30:02.000Z",
  });
  const betaIngress = await ingestAcceptedChannelDecision(betaInbound, {
    bridge,
    bindings,
    ensureSessionForRoute,
    now: "2026-06-20T11:30:04.000Z",
  });
  const alphaTelegramIngress = await ingestChannelInboundMessage(
    {
      kind: "channel_inbound_message.v1",
      adapterId,
      bindingId: "binding-alpha-telegram",
      runtime: {
        agentId: "agent-alpha" as AgentId,
        sessionId: "session-alpha" as SessionId,
        profileId: "prime" as ProfileId,
      },
      providerRefs: {
        provider: "telegram",
        externalChannelId: "telegram-room",
        externalThreadId: "telegram-thread",
        externalMessageId: "telegram-alpha-1",
        externalUserId: "telegram-user-alpha",
      },
      author: { externalUserId: "telegram-user-alpha", displayLabel: "Ada" },
      body: "alpha asks from telegram too",
      attachments: [],
      mentions: [],
      receivedAt: "2026-06-20T11:30:05.000Z",
      ttlMs: 5_000,
      expiresAt: "2026-06-20T11:30:10.000Z",
      cursor: "telegram-1",
      idempotencyKey: "telegram-alpha-1",
      visibility: "conversation",
      provenance: {},
    } satisfies NormalizedChannelInboundMessage,
    {
      bridge,
      bindings,
      ensureSessionForRoute,
      now: "2026-06-20T11:30:06.000Z",
    },
  );

  assert.equal(alphaIngress.status, "routed");
  assert.equal(betaIngress.status, "routed");
  assert.equal(alphaTelegramIngress.status, "routed");
  if (
    alphaIngress.status !== "routed" ||
    betaIngress.status !== "routed" ||
    alphaTelegramIngress.status !== "routed"
  ) {
    throw new Error("expected channel messages to route");
  }
  assert.equal(alphaIngress.session?.sessionId, "session-alpha");
  assert.equal(betaIngress.session?.sessionId, "session-beta");
  assert.equal(alphaTelegramIngress.session?.sessionId, "session-alpha");
  assert.equal(alphaIngress.routedMessage.to, "agent-alpha");
  assert.equal(betaIngress.routedMessage.to, "agent-beta");
  assert.equal(alphaTelegramIngress.routedMessage.to, "agent-alpha");
  assert.match(alphaIngress.routedMessage.correlationId ?? "", /^channel:/);
  assert.match(betaIngress.routedMessage.correlationId ?? "", /^channel:/);
  assert.match(
    alphaTelegramIngress.routedMessage.correlationId ?? "",
    /^channel:binding-alpha-telegram:/,
  );

  const activeSessions = (await native.listSessions()).filter(
    (session) => session.status !== "archived",
  );
  assert.deepEqual(activeSessions.map((session) => session.sessionId).sort(), [
    "session-alpha",
    "session-beta",
  ]);

  const routedEvents = await native.drainSubscriptionEvents(events, 12);
  assert.equal(
    routedEvents.some(
      (event) =>
        event.type === "brain_wake_requested" &&
        event.sessionId === "session-alpha",
    ),
    true,
  );
  assert.equal(
    routedEvents.some(
      (event) =>
        event.type === "brain_wake_requested" &&
        event.sessionId === "session-beta",
    ),
    true,
  );
  assert.equal(
    routedEvents.filter(
      (event) =>
        event.type === "brain_wake_requested" &&
        event.sessionId === "session-alpha",
    ).length >= 2,
    true,
  );

  const duplicateAlpha = await alphaController.acceptInbound(
    {
      type: "message",
      channelId: "crew-room",
      threadId: "thread-alpha",
      messageId: "alpha-1",
      userId: "den-user-alpha",
      text: "duplicate alpha asks for status",
      receivedAt: "2026-06-20T11:30:05.000Z",
      cursor: "1",
    },
    "2026-06-20T11:30:06.000Z",
  );
  assert.equal(duplicateAlpha.accepted, false);
  if (!duplicateAlpha.accepted)
    assert.equal(duplicateAlpha.reason, "duplicate");

  const staleBeta = await betaController.acceptInbound(
    {
      type: "message",
      channelId: "crew-room",
      threadId: "thread-beta",
      messageId: "beta-stale",
      userId: "den-user-beta",
      text: "old beta cursor",
      receivedAt: "2026-06-20T11:30:07.000Z",
      cursor: "0",
    },
    "2026-06-20T11:30:08.000Z",
  );
  assert.equal(staleBeta.accepted, false);
  if (!staleBeta.accepted) assert.equal(staleBeta.reason, "stale_cursor");

  betaTransport.failNextOpen(new Error("temporary Den Channels outage"));
  const reconnectAttempts = await betaController.reconnect();
  assert.equal(reconnectAttempts[0]?.connected, false);
  assert.equal(reconnectAttempts[1]?.connected, true);

  const projectedMessages: NormalizedChannelOutboundMessage[] = [];
  const projectedActivities: NormalizedChannelActivityProjection[] = [];
  let failNextProjection = true;
  const sink = {
    sendMessage(message: NormalizedChannelOutboundMessage): void {
      if (failNextProjection) {
        failNextProjection = false;
        throw new Error("projection sink unavailable");
      }
      projectedMessages.push(message);
    },
    sendActivity(activity: NormalizedChannelActivityProjection): void {
      projectedActivities.push(activity);
    },
  };

  const alphaReply = projectAgentMessageToChannel(
    {
      from: "agent-alpha" as AgentId,
      to: "agent-beta" as AgentId,
      body: "alpha reply",
      correlationId: alphaIngress.routedMessage.correlationId,
    },
    bindings,
  );
  assert.equal(alphaReply.status, "projected");
  if (alphaReply.status !== "projected") {
    throw new Error("expected alpha reply to project");
  }
  assert.equal(alphaReply.message.bindingId, "binding-alpha");

  const droppedProjection = await dispatchChannelMessageProjection(
    sink,
    alphaReply.message,
  );
  assert.equal(droppedProjection.accepted, false);

  const internalRoute = await native.routeAgentMessage(
    "agent-alpha",
    "agent-beta",
    "internal route after Den projection degradation",
  );
  assert.equal(internalRoute.accepted, true);

  const acceptedProjection = await dispatchChannelMessageProjection(
    sink,
    alphaReply.message,
  );
  assert.equal(acceptedProjection.accepted, true);
  assert.equal(projectedMessages[0]?.bindingId, "binding-alpha");

  const betaActivity = projectCoreEventToChannelActivity(
    {
      type: "completion_packet_delivered",
      packet: {
        sessionId: "session-beta" as SessionId,
        status: "completed",
        summary: "review complete",
      },
    },
    bindings[1]!,
    { now: "2026-06-20T11:30:10.000Z" },
  );
  const activityDispatch = await dispatchChannelActivityProjection(
    sink,
    betaActivity,
  );
  assert.equal(activityDispatch.accepted, true);
  assert.equal(projectedActivities[0]?.bindingId, "binding-beta");
  assert.equal(
    projectedActivities[0]?.resultRef,
    "completion:session-beta:completed",
  );

  await native.unsubscribeEvents(events);

  console.log(
    JSON.stringify(
      {
        alphaRoute: alphaIngress.routedMessage.to,
        betaRoute: betaIngress.routedMessage.to,
        routedEventTypes: eventTypes(routedEvents),
        duplicateDropped: !duplicateAlpha.accepted,
        staleDropped: !staleBeta.accepted,
        reconnectAttempts: reconnectAttempts.length,
        projectedMessages: projectedMessages.length,
        projectedActivities: projectedActivities.length,
        internalRoute: internalRoute.accepted,
        sessions: activeSessions.length,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

function channelController(
  binding: ChannelBindingRecord,
  transport: ReturnType<typeof createSimulatedDenChannelsTransport>,
): DenChannelsTransportController {
  return new DenChannelsTransportController({
    binding: {
      adapterId: binding.adapterId,
      bindingId: binding.bindingId,
      agentId: binding.agentId,
      sessionId: binding.sessionId,
      profileId: binding.profileId,
      ttlMs: 5_000,
    },
    cursorStore: new InMemoryDenChannelsCursorStore(),
    cursorKey: binding.bindingId,
    transports: [transport, createSimulatedDenChannelsTransport("fallback")],
    retryPolicy: { maxAttempts: 2, backoffMs: [0, 0] },
  });
}

function channelBinding(
  bindingId: string,
  agentId: string,
  sessionId: string,
  profileId: string,
  options: {
    provider?: string;
    externalChannelId?: string;
    externalThreadId?: string;
    externalUserId?: string;
  } = {},
): ChannelBindingRecord {
  return {
    bindingId,
    adapterId,
    provider: options.provider ?? "den_channels",
    agentId: agentId as AgentId,
    sessionId: sessionId as SessionId,
    profileId: profileId as ProfileId,
    externalChannelId: options.externalChannelId ?? "crew-room",
    externalThreadId:
      options.externalThreadId ?? bindingId.replace("binding-", "thread-"),
    externalUserId: options.externalUserId ?? `${agentId}-external`,
    status: "active",
  };
}

function eventTypes(events: readonly CoreEvent[]): CoreEvent["type"][] {
  return events.map((event) => event.type);
}
