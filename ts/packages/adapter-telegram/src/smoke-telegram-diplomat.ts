import assert from "node:assert/strict";
import { createHash } from "node:crypto";
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

const topicHumanBinding: ChannelBindingRecord = {
  ...betaBinding,
  externalThreadId: "43",
};
const mixedParticipationUpdates: TelegramUpdate[] = [
  {
    update_id: 10,
    message: {
      message_id: 110,
      message_thread_id: 42,
      date: 1_786_000_010,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      text: "unaddressed mention-only topic message",
    },
  },
  {
    update_id: 11,
    message: {
      message_id: 111,
      message_thread_id: 43,
      date: 1_786_000_011,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      text: "unaddressed topic-human message",
    },
  },
];

async function mixedParticipationBodies(
  bindings: readonly ChannelBindingRecord[],
): Promise<string[]> {
  const bodies: string[] = [];
  const connector = new TelegramChannelConnector({
    adapterId,
    bot: {
      getUpdates(request: TelegramGetUpdatesRequest = {}) {
        return mixedParticipationUpdates.filter(
          (update) => update.update_id >= (request.offset ?? 0),
        );
      },
      sendMessage() {
        return { message_id: 1 };
      },
    },
    offsetStore: new MemoryTelegramUpdateOffsetStore(),
    terminalStore: new MemoryTelegramUpdateTerminalStore(),
    bindings: () => bindings,
    participationForBinding(bindingId) {
      if (bindingId === alphaBinding.bindingId) {
        return {
          participationMode: "mention_or_reply",
          botUserId: "2001",
          botUsername: "InstallAlphaBot",
        };
      }
      if (bindingId === topicHumanBinding.bindingId) {
        return {
          participationMode: "topic_human_messages",
          botUserId: "2002",
          botUsername: "InstallBetaBot",
        };
      }
      return undefined;
    },
    ingest(message) {
      bodies.push(`${message.bindingId}:${message.body}`);
      return { status: "routed" };
    },
    ttlMs: 60_000,
    pollTimeoutSeconds: 0,
  });
  await connector.pollOnce();
  assert.equal(connector.diagnostics().inbound.ignored, 1);
  return bodies;
}

const mixedExpected = ["telegram-beta:unaddressed topic-human message"];
assert.deepEqual(
  await mixedParticipationBodies([alphaBinding, topicHumanBinding]),
  mixedExpected,
);
assert.deepEqual(
  await mixedParticipationBodies([topicHumanBinding, alphaBinding]),
  mixedExpected,
);

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

const mediaUpdates: TelegramUpdate[] = [
  {
    update_id: 50,
    message: {
      message_id: 150,
      message_thread_id: 42,
      date: 1_786_000_050,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      caption: "screenshot proof",
      photo: [
        {
          file_id: "photo-small",
          file_unique_id: "photo-small-unique",
          file_size: 2,
          width: 10,
          height: 10,
        },
        {
          file_id: "photo-large",
          file_unique_id: "photo-proof-unique",
          file_size: 4,
          width: 100,
          height: 100,
        },
      ],
    },
  },
  {
    update_id: 51,
    message: {
      message_id: 151,
      message_thread_id: 42,
      date: 1_786_000_051,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      caption: "diagnostic report",
      document: {
        file_id: "report-file",
        file_unique_id: "report-unique",
        file_name: "report.txt",
        mime_type: "text/plain",
        file_size: 6,
      },
    },
  },
  {
    update_id: 52,
    message: {
      message_id: 152,
      message_thread_id: 42,
      date: 1_786_000_052,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      caption: "too large",
      document: {
        file_id: "oversized-file",
        file_unique_id: "oversized-unique",
        file_name: "large.log",
        mime_type: "text/plain",
        file_size: 99,
      },
    },
  },
  {
    update_id: 53,
    message: {
      message_id: 153,
      message_thread_id: 42,
      date: 1_786_000_053,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      caption: "mismatched type",
      document: {
        file_id: "mismatch-file",
        file_unique_id: "mismatch-unique",
        file_name: "claim.txt",
        mime_type: "text/plain",
        file_size: 4,
      },
    },
  },
  {
    update_id: 54,
    message: {
      message_id: 154,
      message_thread_id: 42,
      date: 1_786_000_054,
      chat: { id: chatId, type: "supergroup" },
      from: { id: 1001, first_name: "Patch" },
      caption: "same screenshot forwarded again",
      photo: [
        {
          file_id: "photo-large",
          file_unique_id: "photo-proof-unique",
          file_size: 4,
          width: 100,
          height: 100,
        },
      ],
    },
  },
];
const mediaBodies: NormalizedChannelInboundMessage[] = [];
const persistedMedia = new Map<string, string>();
const mediaConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates(request: TelegramGetUpdatesRequest = {}) {
      return mediaUpdates.filter(
        (update) => update.update_id >= (request.offset ?? 0),
      );
    },
    sendMessage() {
      return { message_id: 1 };
    },
    getFile(fileId) {
      const unique =
        fileId === "photo-large"
          ? "photo-proof-unique"
          : fileId === "report-file"
            ? "report-unique"
            : fileId === "mismatch-file"
              ? "mismatch-unique"
              : `${fileId}-unique`;
      return {
        file_id: fileId,
        file_unique_id: unique,
        file_size: fileId === "report-file" ? 6 : 4,
        file_path: `${fileId}.bin`,
      };
    },
    downloadFile(filePath) {
      if (filePath.startsWith("report-file")) {
        return {
          bytes: new TextEncoder().encode("report"),
          contentType: "text/plain",
        };
      }
      return {
        bytes: new Uint8Array([1, 2, 3, 4]),
        contentType: filePath.startsWith("mismatch-file")
          ? "application/pdf"
          : "image/jpeg",
      };
    },
  },
  offsetStore: new MemoryTelegramUpdateOffsetStore(),
  terminalStore: new MemoryTelegramUpdateTerminalStore(),
  bindings: () => [alphaBinding],
  ingest(message) {
    mediaBodies.push(message);
    return { status: "routed" };
  },
  persistMedia(input) {
    const attachmentId =
      persistedMedia.get(input.fileUniqueId) ??
      `attachment:${input.fileUniqueId}`;
    persistedMedia.set(input.fileUniqueId, attachmentId);
    return {
      attachmentId,
      filename: input.filename,
      mediaType: input.mediaType,
      byteSize: input.bytes.byteLength,
      sha256: createHash("sha256").update(input.bytes).digest("hex"),
      contentUrl: `http://crew.local/${attachmentId}`,
    };
  },
  maxDocumentBytes: 10,
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
});
await mediaConnector.pollOnce();
assert.equal(mediaBodies[0]?.attachments.length, 1);
assert.equal(mediaBodies[0]?.attachments[0]?.state, "available");
assert.equal(mediaBodies[0]?.attachments[0]?.mediaType, "image/jpeg");
assert.equal(mediaBodies[1]?.attachments[0]?.state, "available");
assert.equal(mediaBodies[1]?.attachments[0]?.mediaType, "text/plain");
assert.equal(mediaBodies[2]?.attachments[0]?.state, "oversized");
assert.equal(
  mediaBodies[2]?.attachments[0]?.reasonCode,
  "telegram_media_oversized",
);
assert.equal(mediaBodies[3]?.attachments[0]?.state, "unsupported");
assert.equal(
  mediaBodies[3]?.attachments[0]?.reasonCode,
  "telegram_media_mime_mismatch",
);
assert.equal(
  mediaBodies[4]?.attachments[0]?.attachmentId,
  mediaBodies[0]?.attachments[0]?.attachmentId,
);
assert.equal(persistedMedia.size, 2);
assert.equal(mediaConnector.diagnostics().media.available, 3);
assert.equal(mediaConnector.diagnostics().media.oversized, 1);
assert.equal(mediaConnector.diagnostics().media.unsupported, 1);

let interruptedDownloadAttempts = 0;
const interruptedOffset = new MemoryTelegramUpdateOffsetStore();
const interruptedConnector = new TelegramChannelConnector({
  adapterId,
  bot: {
    getUpdates(request: TelegramGetUpdatesRequest = {}) {
      return mediaUpdates[0]!.update_id >= (request.offset ?? 0)
        ? [mediaUpdates[0]!]
        : [];
    },
    sendMessage() {
      return { message_id: 1 };
    },
    getFile() {
      return {
        file_id: "photo-large",
        file_unique_id: "photo-proof-unique",
        file_size: 4,
        file_path: "photo-large.jpg",
      };
    },
    downloadFile() {
      interruptedDownloadAttempts += 1;
      if (interruptedDownloadAttempts < 3) {
        throw new TypeError("interrupted Telegram download");
      }
      return {
        bytes: new Uint8Array([1, 2, 3, 4]),
        contentType: "image/jpeg",
      };
    },
  },
  offsetStore: interruptedOffset,
  terminalStore: new MemoryTelegramUpdateTerminalStore(),
  bindings: () => [alphaBinding],
  ingest() {
    return { status: "routed" };
  },
  persistMedia(input) {
    return {
      attachmentId: `attachment:${input.fileUniqueId}`,
      filename: input.filename,
      mediaType: input.mediaType,
      byteSize: input.bytes.byteLength,
      sha256: createHash("sha256").update(input.bytes).digest("hex"),
      contentUrl: "http://crew.local/photo",
    };
  },
  ttlMs: 60_000,
  pollTimeoutSeconds: 0,
  maxInboundAttempts: 3,
});
await interruptedConnector.pollOnce();
assert.equal(await interruptedOffset.read(), undefined);
await interruptedConnector.pollOnce();
assert.equal(await interruptedOffset.read(), undefined);
await interruptedConnector.pollOnce();
assert.equal(await interruptedOffset.read(), 51);
assert.equal(interruptedConnector.diagnostics().media.retried, 2);

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
      mediaAvailable: mediaConnector.diagnostics().media.available,
      mediaRetries: interruptedConnector.diagnostics().media.retried,
    },
    null,
    2,
  ),
);
