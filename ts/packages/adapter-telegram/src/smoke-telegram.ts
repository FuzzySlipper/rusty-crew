import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  NormalizedChannelOutboundMessage,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createTelegramAdapterRegistration,
  createTelegramChannelAdapter,
  MemoryTelegramUpdateOffsetStore,
  normalizeTelegramUpdate,
  TelegramChannelConnector,
  telegramBindingFromChat,
  toTelegramSendMessageRequest,
  type TelegramGetUpdatesRequest,
  type TelegramSendMessageRequest,
  type TelegramUpdate,
} from "./index.js";

const adapterId = "telegram-main" as AdapterId;
const binding = telegramBindingFromChat({
  adapterId,
  bindingId: "telegram-alpha",
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  profileId: "prime" as ProfileId,
  chat: {
    id: -100123,
    type: "supergroup",
    title: "Crew Room",
  },
  threadId: 42,
  externalUserId: "1001",
  createdAt: "2026-06-20T12:00:00.000Z",
});

assert.deepEqual(createTelegramAdapterRegistration(adapterId), {
  adapterId,
  kind: "telegram",
  displayName: "Telegram",
});
assert.equal(binding.provider, "telegram");
assert.equal(binding.externalChannelId, "-100123");
assert.equal(binding.externalThreadId, "42");

const inbound = normalizeTelegramUpdate(
  {
    update_id: 777,
    message: {
      message_id: 55,
      message_thread_id: 42,
      date: 1_781_976_000,
      chat: {
        id: -100123,
        type: "supergroup",
        title: "Crew Room",
      },
      from: {
        id: 1001,
        first_name: "Ada",
        last_name: "Lovelace",
        username: "ada",
      },
      text: "@agent_alpha please check the build",
      document: {
        file_id: "file-doc",
        file_name: "report.txt",
        mime_type: "text/plain",
      },
    },
  },
  { binding, ttlMs: 5_000 },
);

assert.ok(inbound);
assert.equal(inbound.providerRefs.provider, "telegram");
assert.equal(inbound.providerRefs.externalChannelId, "-100123");
assert.equal(inbound.providerRefs.externalThreadId, "42");
assert.equal(inbound.providerRefs.externalMessageId, "55");
assert.equal(inbound.runtime.agentId, "agent-alpha");
assert.equal(inbound.cursor, "777");
assert.equal(inbound.idempotencyKey, "telegram:-100123:42:55");
assert.deepEqual(inbound.mentions, ["agent_alpha"]);
assert.equal(inbound.attachments[0]?.ref, "telegram:file:file-doc");

const outbound: NormalizedChannelOutboundMessage = {
  kind: "channel_outbound_message.v1",
  adapterId,
  bindingId: binding.bindingId,
  runtime: {
    agentId: binding.agentId,
    sessionId: binding.sessionId,
    profileId: binding.profileId,
  },
  providerRefs: {
    provider: "telegram",
    externalChannelId: "-100123",
    externalThreadId: "42",
  },
  body: "Build is green.",
  replyToExternalMessageId: "55",
  correlationId: "telegram-alpha:55",
  idempotencyKey: "outbound:telegram-alpha:55",
  visibility: "conversation",
  deliveryPolicy: "best_effort",
};

const request = toTelegramSendMessageRequest(outbound);
assert.deepEqual(request, {
  chat_id: -100123,
  message_thread_id: 42,
  reply_to_message_id: 55,
  text: "Build is green.",
  disable_web_page_preview: true,
});

const sent: TelegramSendMessageRequest[] = [];
const adapter = createTelegramChannelAdapter({
  adapterId,
  bot: {
    sendMessage(message) {
      sent.push(message);
      return { ok: true };
    },
  },
});

assert.equal(adapter.registration().kind, "telegram");
assert.equal(
  adapter.normalizeUpdate({ update_id: 778 }, { binding, ttlMs: 5_000 }),
  undefined,
);
await adapter.sendOutbound(outbound);
assert.deepEqual(sent, [request]);

const connectorUpdates: TelegramUpdate[] = [
  {
    update_id: 10,
    message: {
      message_id: 70,
      message_thread_id: 42,
      date: 1_781_976_001,
      chat: {
        id: -100123,
        type: "supergroup",
        title: "Crew Room",
      },
      from: {
        id: 1001,
        first_name: "Ada",
      },
      text: "first live connector message",
    },
  },
  {
    update_id: 11,
    message: {
      message_id: 71,
      date: 1_781_976_002,
      chat: {
        id: -999,
        type: "supergroup",
        title: "Unbound Room",
      },
      from: {
        id: 1002,
        first_name: "Grace",
      },
      text: "unbound should not route",
    },
  },
  {
    update_id: 12,
    message: {
      message_id: 72,
      message_thread_id: 42,
      date: 1_781_976_003,
      chat: {
        id: -100123,
        type: "supergroup",
        title: "Crew Room",
      },
      from: {
        id: 1001,
        first_name: "Ada",
      },
      text: "second live connector message",
    },
  },
];

const getUpdatesRequests: TelegramGetUpdatesRequest[] = [];
const connectorSent: TelegramSendMessageRequest[] = [];
const bot = {
  getUpdates(request: TelegramGetUpdatesRequest = {}) {
    getUpdatesRequests.push({ ...request });
    const offset = request.offset ?? 0;
    return connectorUpdates.filter((update) => update.update_id >= offset);
  },
  sendMessage(message: TelegramSendMessageRequest) {
    connectorSent.push(message);
    return { ok: true };
  },
};

const routedBodies: string[] = [];
const offsetStore = new MemoryTelegramUpdateOffsetStore();
const connector = new TelegramChannelConnector({
  adapterId,
  bot,
  offsetStore,
  bindings: () => [binding],
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
  ingest(message) {
    routedBodies.push(message.body);
    return { status: "routed" };
  },
});

await connector.pollOnce();
assert.deepEqual(routedBodies, [
  "first live connector message",
  "second live connector message",
]);
assert.equal(await offsetStore.read(), 13);
assert.equal(connector.diagnostics().inbound.routed, 2);
assert.equal(connector.diagnostics().inbound.unbound, 1);
assert.equal(connector.diagnostics().lastUpdateId, 12);

connectorUpdates.push({
  update_id: 13,
  message: {
    message_id: 73,
    message_thread_id: 42,
    date: 1_781_976_004,
    chat: {
      id: -100123,
      type: "supergroup",
      title: "Crew Room",
    },
    from: {
      id: 1001,
      first_name: "Ada",
    },
    text: "post restart message",
  },
});

const restartedConnector = new TelegramChannelConnector({
  adapterId,
  bot,
  offsetStore,
  bindings: () => [binding],
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
  ingest(message) {
    routedBodies.push(message.body);
    return { status: "routed" };
  },
});
await restartedConnector.pollOnce();
assert.equal(getUpdatesRequests.at(-1)?.offset, 13);
assert.deepEqual(routedBodies, [
  "first live connector message",
  "second live connector message",
  "post restart message",
]);
assert.equal(await offsetStore.read(), 14);

await restartedConnector.sendOutbound(outbound);
assert.deepEqual(connectorSent, [request]);
assert.equal(restartedConnector.diagnostics().outbound.sent, 1);

console.log(
  JSON.stringify(
    {
      bindingId: binding.bindingId,
      provider: inbound.providerRefs.provider,
      mention: inbound.mentions[0],
      chatId: request.chat_id,
      sent: sent.length,
      connectorRouted: routedBodies.length,
      connectorOffset: await offsetStore.read(),
    },
    null,
    2,
  ),
);
