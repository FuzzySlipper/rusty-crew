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
} from "@rusty-crew/adapter-den";
import {
  MemoryTelegramUpdateOffsetStore,
  TelegramChannelConnector,
  type TelegramGetUpdatesRequest,
  type TelegramSendMessageRequest,
  type TelegramUpdate,
} from "@rusty-crew/adapter-telegram";

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
const connector = new TelegramChannelConnector({
  adapterId,
  bot,
  offsetStore,
  bindings: () => [binding],
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
  ingest: (message) =>
    ingestChannelInboundMessage(message, {
      bridge,
      bindings: [binding],
      ensureSessionForRoute: ({ binding }) => ({
        handle: 1,
        sessionId: binding.sessionId ?? ("session-alpha" as SessionId),
        agentId: binding.agentId,
        profileId: binding.profileId,
        kind: "full",
        status: "active",
      }),
      now: "2026-06-20T12:00:20.000Z",
    }),
});

await connector.pollOnce();

assert.equal(await offsetStore.read(), 52);
assert.equal(connector.diagnostics().inbound.routed, 1);
assert.equal(connector.diagnostics().inbound.unbound, 1);
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
    reply_to_message_id: undefined,
    text: "reply from rusty crew",
    disable_web_page_preview: true,
  },
]);

console.log(
  JSON.stringify(
    {
      offset: await offsetStore.read(),
      routed: routedMessages.length,
      externalEvents: injectedExternalEvents.length,
      sent: sent.length,
      firstRequestOffset: getUpdatesRequests[0]?.offset,
    },
    null,
    2,
  ),
);
