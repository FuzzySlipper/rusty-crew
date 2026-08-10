import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  ChannelBindingRecord,
  NormalizedChannelInboundMessage,
  NormalizedChannelOutboundMessage,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  MemoryTelegramUpdateOffsetStore,
  MemoryTelegramUpdateTerminalStore,
  normalizeTelegramUpdate,
  TelegramBotApiError,
  TelegramChannelConnector,
  type TelegramGetUpdatesRequest,
  type TelegramNonExecutableUpdate,
  type TelegramQuarantinedUpdate,
  type TelegramSendMessageRequest,
  type TelegramUpdate,
} from "./index.js";

const adapterId = "telegram-main" as AdapterId;
const chatId = "-1006763";

function binding(input: {
  bindingId: string;
  agentId: string;
  sessionId: string;
}): ChannelBindingRecord {
  return {
    bindingId: input.bindingId,
    adapterId,
    provider: "telegram",
    agentId: input.agentId as AgentId,
    sessionId: input.sessionId as SessionId,
    profileId: "diplomat" as ProfileId,
    externalChannelId: chatId,
    externalThreadId: "42",
    status: "active",
  };
}

const alphaBinding = binding({
  bindingId: "telegram-alpha",
  agentId: "diplomat-alpha",
  sessionId: "session-alpha",
});
const betaBinding = binding({
  bindingId: "telegram-beta",
  agentId: "diplomat-beta",
  sessionId: "session-beta",
});

const humanMentionAlpha: TelegramUpdate = {
  update_id: 1,
  message: {
    message_id: 101,
    message_thread_id: 42,
    date: 1_786_000_000,
    chat: { id: chatId, type: "supergroup", title: "Install Diplomats" },
    from: {
      id: 1001,
      username: "patch",
      first_name: "Patch",
      is_bot: false,
    },
    text: "@InstallAlphaBot please inspect the service",
    entities: [{ type: "mention", offset: 0, length: 16 }],
  },
};

const normalizedHuman = normalizeTelegramUpdate(humanMentionAlpha, {
  binding: alphaBinding,
  ttlMs: 60_000,
});
assert.ok(normalizedHuman);
assert.deepEqual(normalizedHuman.author, {
  externalUserId: "1001",
  displayLabel: "Patch",
  username: "patch",
  kind: "human",
  isBot: false,
});
assert.deepEqual(normalizedHuman.mentions, ["installalphabot"]);
assert.equal(normalizedHuman.messageMutation, "original");

const botReply: TelegramUpdate = {
  update_id: 2,
  message: {
    message_id: 102,
    message_thread_id: 42,
    date: 1_786_000_001,
    chat: { id: chatId, type: "supergroup", title: "Install Diplomats" },
    from: {
      id: 2002,
      username: "InstallBetaBot",
      first_name: "Install Beta",
      is_bot: true,
    },
    reply_to_message: {
      message_id: 99,
      from: {
        id: 2001,
        username: "InstallAlphaBot",
        first_name: "Install Alpha",
        is_bot: true,
      },
    },
    text: "Beta sees the same provider failure.",
  },
};

const normalizedBot = normalizeTelegramUpdate(botReply, {
  binding: alphaBinding,
  ttlMs: 60_000,
});
assert.ok(normalizedBot);
assert.equal(normalizedBot.author.kind, "bot");
assert.equal(normalizedBot.author.isBot, true);
assert.equal(normalizedBot.author.username, "InstallBetaBot");
assert.equal(normalizedBot.replyToExternalMessageId, "99");
assert.equal(normalizedBot.provenance.replyToAuthorExternalUserId, "2001");
assert.equal(normalizedBot.provenance.replyToAuthorIsBot, true);

const edited = normalizeTelegramUpdate(
  {
    update_id: 3,
    edited_message: {
      ...humanMentionAlpha.message!,
      edit_date: 1_786_000_010,
      text: "@InstallAlphaBot inspect the debug service",
    },
  },
  { binding: alphaBinding, ttlMs: 60_000 },
);
assert.ok(edited);
assert.equal(edited.messageMutation, "edited");
assert.match(edited.idempotencyKey, /:edited:1786000010$/);

const sharedUpdates: TelegramUpdate[] = [
  humanMentionAlpha,
  {
    update_id: 4,
    message: {
      ...humanMentionAlpha.message!,
      message_id: 104,
      text: "ordinary group chatter",
      entities: [],
    },
  },
  botReply,
];

async function addressedBodiesFor(input: {
  localBinding: ChannelBindingRecord;
  botUserId: string;
  botUsername: string;
}): Promise<{ bodies: string[]; ignored: number; botMessages: number }> {
  const bodies: string[] = [];
  const connector = new TelegramChannelConnector({
    adapterId,
    bot: {
      getUpdates(request: TelegramGetUpdatesRequest = {}) {
        return sharedUpdates.filter(
          (update) => update.update_id >= (request.offset ?? 0),
        );
      },
      sendMessage() {
        return { message_id: 1 };
      },
    },
    offsetStore: new MemoryTelegramUpdateOffsetStore(),
    terminalStore: new MemoryTelegramUpdateTerminalStore(),
    bindings: () => [input.localBinding],
    ingest(message) {
      bodies.push(message.body);
      return { status: "routed" };
    },
    ttlMs: 60_000,
    pollTimeoutSeconds: 0,
    participationMode: "mention_or_reply",
    botUserId: input.botUserId,
    botUsername: input.botUsername,
  });
  await connector.pollOnce();
  const diagnostics = connector.diagnostics();
  return {
    bodies,
    ignored: diagnostics.inbound.ignored,
    botMessages: diagnostics.inbound.botMessages,
  };
}

const alphaAddressing = await addressedBodiesFor({
  localBinding: alphaBinding,
  botUserId: "2001",
  botUsername: "InstallAlphaBot",
});
assert.deepEqual(alphaAddressing.bodies, [
  "@InstallAlphaBot please inspect the service",
  "Beta sees the same provider failure.",
]);
assert.equal(alphaAddressing.ignored, 1);
assert.equal(alphaAddressing.botMessages, 1);

const betaAddressing = await addressedBodiesFor({
  localBinding: betaBinding,
  botUserId: "2002",
  botUsername: "InstallBetaBot",
});
assert.deepEqual(betaAddressing.bodies, []);
assert.equal(betaAddressing.ignored, 3);

const transientUpdate: TelegramUpdate = {
  update_id: 20,
  message: {
    message_id: 120,
    date: 1_786_000_020,
    chat: { id: chatId, type: "supergroup" },
    from: { id: 1001, first_name: "Patch" },
    text: "transient ingress failure",
  },
};
const transientOffset = new MemoryTelegramUpdateOffsetStore();
const transientTerminal = new MemoryTelegramUpdateTerminalStore();
const quarantined: TelegramQuarantinedUpdate[] = [];
let transientAttempts = 0;
const transientConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates(request: TelegramGetUpdatesRequest = {}) {
      return transientUpdate.update_id >= (request.offset ?? 0)
        ? [transientUpdate]
        : [];
    },
    sendMessage() {
      return { message_id: 1 };
    },
  },
  offsetStore: transientOffset,
  terminalStore: transientTerminal,
  bindings: () => [{ ...alphaBinding, externalThreadId: undefined }],
  ingest() {
    transientAttempts += 1;
    throw new Error("temporary Rust ingress outage");
  },
  onQuarantine(update) {
    quarantined.push(update);
  },
  maxInboundAttempts: 3,
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
});

await transientConnector.pollOnce();
assert.equal(await transientOffset.read(), undefined);
await transientConnector.pollOnce();
assert.equal(await transientOffset.read(), undefined);
await transientConnector.pollOnce();
assert.equal(await transientOffset.read(), 21);
assert.equal(transientAttempts, 3);
assert.equal(quarantined.length, 1);
assert.equal(quarantined[0]?.attempts, 3);
assert.equal(transientTerminal.records[0]?.disposition, "quarantined");
assert.equal(transientConnector.diagnostics().inbound.quarantined, 1);

const nonExecutable: TelegramNonExecutableUpdate[] = [];
const editedOffset = new MemoryTelegramUpdateOffsetStore();
const editedTerminal = new MemoryTelegramUpdateTerminalStore();
const editedConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates(request: TelegramGetUpdatesRequest = {}) {
      const updates: TelegramUpdate[] = [
        {
          update_id: 30,
          edited_message: edited!.provenance.sourceShape
            ? {
                ...humanMentionAlpha.message!,
                edit_date: 1_786_000_030,
              }
            : undefined,
        },
        { update_id: 31 },
      ];
      return updates.filter(
        (update) => update.update_id >= (request.offset ?? 0),
      );
    },
    sendMessage() {
      return { message_id: 1 };
    },
  },
  offsetStore: editedOffset,
  terminalStore: editedTerminal,
  bindings: () => [alphaBinding],
  ingest() {
    throw new Error("edited/unsupported updates must not execute");
  },
  onNonExecutableUpdate(update) {
    nonExecutable.push(update);
  },
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
});
await editedConnector.pollOnce();
assert.equal(await editedOffset.read(), 32);
assert.deepEqual(
  nonExecutable.map((update) => update.reason),
  ["edited_message", "unsupported_update"],
);
assert.equal(editedConnector.diagnostics().inbound.edited, 1);
assert.equal(editedConnector.diagnostics().inbound.unsupported, 1);
assert.deepEqual(
  editedTerminal.records.map((record) => record.disposition),
  ["non_executable", "non_executable"],
);

const sent: TelegramSendMessageRequest[] = [];
const waits: number[] = [];
let sendAttempts = 0;
const deliveryConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates() {
      return [];
    },
    sendMessage(request) {
      sendAttempts += 1;
      if (sendAttempts === 1) {
        throw new TelegramBotApiError({
          method: "sendMessage",
          status: 429,
          errorCode: 429,
          description: "Too Many Requests",
          retryAfterSeconds: 0,
        });
      }
      sent.push(request);
      return { message_id: 500 + sent.length };
    },
  },
  offsetStore: new MemoryTelegramUpdateOffsetStore(),
  terminalStore: new MemoryTelegramUpdateTerminalStore(),
  bindings: () => [alphaBinding],
  ingest() {
    return { status: "routed" };
  },
  ttlMs: 60_000,
  maxMessageChars: 4,
  maxOutboundAttempts: 3,
  wait(delayMs) {
    waits.push(delayMs);
    return Promise.resolve();
  },
});

const outbound: NormalizedChannelOutboundMessage = {
  kind: "channel_outbound_message.v1",
  adapterId,
  bindingId: alphaBinding.bindingId,
  runtime: {
    agentId: alphaBinding.agentId,
    sessionId: alphaBinding.sessionId,
    profileId: alphaBinding.profileId,
  },
  providerRefs: {
    provider: "telegram",
    externalChannelId: chatId,
    externalThreadId: "42",
  },
  body: "abcdefghij",
  replyToExternalMessageId: "101",
  correlationId: "interaction-alpha-1",
  idempotencyKey: "outbound-alpha-1",
  visibility: "conversation",
  deliveryPolicy: "must_ack",
};
const receipt = await deliveryConnector.sendOutbound(outbound);
assert.deepEqual(receipt, {
  idempotencyKey: "outbound-alpha-1",
  chunkCount: 3,
  attempts: 4,
  externalMessageIds: ["501", "502", "503"],
});
assert.deepEqual(
  sent.map((request) => request.text),
  ["abcd", "efgh", "ij"],
);
assert.deepEqual(sent[0]?.reply_parameters, { message_id: 101 });
assert.equal(sent[1]?.reply_parameters, undefined);
assert.deepEqual(waits, [0]);
assert.equal(deliveryConnector.diagnostics().outbound.retried, 1);
assert.equal(deliveryConnector.diagnostics().outbound.chunksSent, 3);

const terminalBodies: NormalizedChannelInboundMessage[] = [];
let terminalStatus = "telegram_bot_loop_depth_exceeded";
const terminalConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates() {
      return [botReply];
    },
    sendMessage() {
      return { message_id: 1 };
    },
  },
  offsetStore: new MemoryTelegramUpdateOffsetStore(),
  terminalStore: new MemoryTelegramUpdateTerminalStore(),
  bindings: () => [alphaBinding],
  ingest(message) {
    terminalBodies.push(message);
    return { status: terminalStatus };
  },
  ttlMs: 60_000,
  participationMode: "mention_or_reply",
  botUserId: "2001",
  botUsername: "InstallAlphaBot",
});
await terminalConnector.pollOnce();
assert.equal(terminalBodies.length, 1);
assert.equal(terminalConnector.diagnostics().inbound.loopTerminated, 1);

terminalStatus = "telegram_bot_pair_rate_limited";
const rateConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates() {
      return [{ ...botReply, update_id: 40 }];
    },
    sendMessage() {
      return { message_id: 1 };
    },
  },
  offsetStore: new MemoryTelegramUpdateOffsetStore(),
  terminalStore: new MemoryTelegramUpdateTerminalStore(),
  bindings: () => [alphaBinding],
  ingest() {
    return { status: terminalStatus };
  },
  ttlMs: 60_000,
  participationMode: "mention_or_reply",
  botUserId: "2001",
});
await rateConnector.pollOnce();
assert.equal(rateConnector.diagnostics().inbound.rateLimited, 1);

console.log(
  JSON.stringify(
    {
      alphaRouted: alphaAddressing.bodies.length,
      betaRouted: betaAddressing.bodies.length,
      botIdentity: normalizedBot.author.username,
      quarantined: quarantined.length,
      nonExecutable: nonExecutable.length,
      chunks: receipt.chunkCount,
      deliveryAttempts: receipt.attempts,
      loopTerminated: terminalConnector.diagnostics().inbound.loopTerminated,
      rateLimited: rateConnector.diagnostics().inbound.rateLimited,
    },
    null,
    2,
  ),
);
