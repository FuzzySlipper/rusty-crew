import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  AgentMessage,
  ChannelBindingRecord,
  EventReceipt,
  ExternalEvent,
  NormalizedChannelOutboundMessage,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  dispatchChannelMessageProjection,
  ingestChannelInboundMessage,
  projectAgentMessageToChannel,
  type ChannelIngressRoutePlannerInput,
  type ChannelRouteResolution,
} from "@rusty-crew/adapter-den";
import {
  MemoryTelegramUpdateOffsetStore,
  MemoryTelegramUpdateTerminalStore,
  TelegramChannelConnector,
  type TelegramGetUpdatesRequest,
  type TelegramSendMessageRequest,
  type TelegramUpdate,
} from "@rusty-crew/adapter-telegram";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const adapterId = "telegram-main" as AdapterId;
const binding: ChannelBindingRecord = {
  bindingId: "telegram-alpha",
  adapterId,
  provider: "telegram",
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  profileId: "prime" as ProfileId,
  externalChannelId: "-100123",
  externalThreadId: "42",
  status: "active",
};
const alternateBinding: ChannelBindingRecord = {
  ...binding,
  bindingId: "telegram-beta",
  agentId: "agent-beta" as AgentId,
  sessionId: "session-beta" as SessionId,
  profileId: "review" as ProfileId,
  externalThreadId: "99",
};

const updates: TelegramUpdate[] = [
  {
    update_id: 50,
    message: {
      message_id: 90,
      message_thread_id: 42,
      date: 1_781_976_010,
      chat: { id: -100123, type: "supergroup", title: "Crew Room" },
      from: { id: 1001, first_name: "Ada" },
      text: "route me through ingress",
    },
  },
  {
    update_id: 51,
    message: {
      message_id: 91,
      date: 1_781_976_011,
      chat: { id: -999, type: "supergroup", title: "Unbound" },
      from: { id: 1002, first_name: "Grace" },
      text: "do not route me",
    },
  },
  {
    update_id: 52,
    message: {
      message_id: 92,
      date: 1_781_976_012,
      chat: { id: -100123, type: "supergroup", title: "Crew Room" },
      from: { id: 1003, first_name: "Lin" },
      text: "ambiguous threadless message",
    },
  },
  {
    update_id: 53,
    message: {
      message_id: 93,
      message_thread_id: 42,
      date: 1_781_975_900,
      chat: { id: -100123, type: "supergroup", title: "Crew Room" },
      from: { id: 1004, first_name: "Old" },
      text: "expired telegram message",
    },
  },
];
const getUpdatesRequests: TelegramGetUpdatesRequest[] = [];
const sent: TelegramSendMessageRequest[] = [];
const bot = {
  getUpdates(request: TelegramGetUpdatesRequest = {}) {
    getUpdatesRequests.push({ ...request });
    const offset = request.offset ?? 0;
    return updates.filter((update) => update.update_id >= offset);
  },
  sendMessage(request: TelegramSendMessageRequest) {
    sent.push(request);
    return { ok: true };
  },
};

const injectedExternalEvents: ExternalEvent[] = [];
const routedMessages: AgentMessage[] = [];
const bridge = {
  injectExternalEvent(event: ExternalEvent): EventReceipt {
    injectedExternalEvents.push(event);
    return { accepted: true, sequence: injectedExternalEvents.length };
  },
  routeAgentMessage(message: AgentMessage): EventReceipt {
    routedMessages.push(message);
    return { accepted: true, sequence: routedMessages.length };
  },
};

const offsetStore = new MemoryTelegramUpdateOffsetStore();
const native = await loadNativeBridge();
let plannerCalls = 0;
const bindings = [binding, alternateBinding];
const routePlanner = async (
  input: ChannelIngressRoutePlannerInput,
): Promise<ChannelRouteResolution> => {
  plannerCalls += 1;
  const plan = await native.planChannelIngressRoute({
    message: {
      adapterId: input.message.adapterId,
      bindingId: input.message.bindingId,
      provider: String(input.message.providerRefs.provider),
      externalChannelId: input.message.providerRefs.externalChannelId,
      externalThreadId: input.message.providerRefs.externalThreadId,
      externalUserId: input.message.author.externalUserId,
      body: input.message.body,
      mentions: input.message.mentions,
      expiresAt: input.message.expiresAt,
      idempotencyKey: input.message.idempotencyKey,
      runtimeAgentId: input.message.runtime.agentId,
    },
    bindings: input.bindings.map((binding) => ({
      bindingId: binding.bindingId,
      adapterId: binding.adapterId,
      provider: String(binding.provider),
      agentId: binding.agentId,
      instanceId: binding.instanceId,
      sessionId: binding.sessionId,
      profileId: binding.profileId,
      externalChannelId: binding.externalChannelId,
      externalThreadId: binding.externalThreadId,
      externalUserId: binding.externalUserId,
      conversationProjectId: binding.conversationProjectId,
      conversationChannelId: binding.conversationChannelId,
      providerSubscriptionId: binding.providerSubscriptionId,
      status: binding.status,
    })),
    mentionAliases: input.routing?.mentionAliases,
    systemAgentId: input.routing?.systemAgentId,
    now: input.now,
  });
  if (plan.status === "routed") {
    assert.ok(plan.binding);
    assert.ok(plan.route);
    return {
      status: "routed",
      binding: channelBindingFromPlanner(plan.binding),
      route: {
        from: plan.route.from,
        to: plan.route.to,
        body: plan.route.body,
        correlationId: plan.route.correlationId,
        bindingId: plan.route.bindingId,
        sessionId: plan.route.sessionId,
      },
    };
  }
  return {
    status: plan.status,
    reason: plan.reason,
    reasonCode: plan.reasonCode,
    correlationId: plan.correlationId,
    candidates: plan.candidates.map(channelBindingFromPlanner),
    message: input.message,
  };
};
const connector = new TelegramChannelConnector({
  adapterId,
  bot,
  offsetStore,
  terminalStore: new MemoryTelegramUpdateTerminalStore(),
  bindings: () => bindings,
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
  ingest: (message) =>
    ingestChannelInboundMessage(message, {
      bridge,
      bindings,
      ensureSessionForRoute: ({ binding }) => ({
        handle: 1,
        sessionId: binding.sessionId ?? ("session-alpha" as SessionId),
        agentId: binding.agentId,
        profileId: binding.profileId,
        kind: "full",
        status: "active",
      }),
      routePlanner,
      now: "2026-06-20T17:20:20.000Z",
    }),
});

await connector.pollOnce();

assert.equal(await offsetStore.read(), 54);
assert.equal(connector.diagnostics().inbound.routed, 1);
assert.equal(connector.diagnostics().inbound.unbound, 1);
assert.equal(connector.diagnostics().inbound.ambiguous, 1);
assert.equal(connector.diagnostics().inbound.expired, 1);
assert.equal(plannerCalls, 4);
assert.equal(injectedExternalEvents.length, 1);
assert.equal(injectedExternalEvents[0]?.source, "telegram:telegram-alpha");
assert.equal(routedMessages.length, 1);
assert.equal(routedMessages[0]?.to, "agent-alpha");
assert.match(
  routedMessages[0]?.correlationId ?? "",
  /^channel:telegram-alpha:/,
);

const outboundProjection = projectAgentMessageToChannel(
  {
    from: "agent-alpha" as AgentId,
    to: "channel:binding:telegram-alpha" as AgentId,
    body: "reply from rusty crew",
    correlationId: routedMessages[0]?.correlationId,
  },
  [binding],
);
assert.equal(outboundProjection.status, "projected");
if (outboundProjection.status !== "projected") {
  throw new Error("expected outbound projection to be projected");
}
const dispatch = await dispatchChannelMessageProjection(
  {
    sendMessage(message: NormalizedChannelOutboundMessage) {
      return connector.sendOutbound(message);
    },
    sendActivity() {
      return undefined;
    },
  },
  outboundProjection.message,
);

assert.deepEqual(dispatch, { accepted: true, kind: "message" });
assert.deepEqual(sent, [
  {
    chat_id: -100123,
    message_thread_id: 42,
    reply_parameters: undefined,
    text: "reply from rusty crew",
    link_preview_options: { is_disabled: true },
  },
]);

console.log(
  JSON.stringify(
    {
      offset: await offsetStore.read(),
      routed: routedMessages.length,
      externalEvents: injectedExternalEvents.length,
      sent: sent.length,
      plannerCalls,
      firstRequestOffset: getUpdatesRequests[0]?.offset,
    },
    null,
    2,
  ),
);

function channelBindingFromPlanner(binding: {
  bindingId: string;
  adapterId: string;
  provider: string;
  agentId: string;
  instanceId?: string;
  sessionId?: string;
  profileId: string;
  externalChannelId: string;
  externalThreadId?: string;
  externalUserId?: string;
  conversationProjectId?: string;
  conversationChannelId?: number;
  providerSubscriptionId?: string;
  status: ChannelBindingRecord["status"];
}): ChannelBindingRecord {
  return {
    bindingId: binding.bindingId,
    adapterId: binding.adapterId as AdapterId,
    provider: binding.provider,
    agentId: binding.agentId as AgentId,
    sessionId: binding.sessionId as SessionId | undefined,
    profileId: binding.profileId as ProfileId,
    externalChannelId: binding.externalChannelId,
    externalThreadId: binding.externalThreadId,
    externalUserId: binding.externalUserId,
    conversationProjectId: binding.conversationProjectId,
    conversationChannelId: binding.conversationChannelId,
    providerSubscriptionId: binding.providerSubscriptionId,
    status: binding.status,
  };
}
