import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  handleRustyViewChatRequest,
  type ChatSendMessageInput,
} from "../src/rusty-view-chat-api.js";

const activeSession = session("chat-authority-session", "idle");
const archivedSession = session("chat-authority-archived", "archived");
const submitted: ChatSendMessageInput[] = [];

const apiSource = readFileSync(
  new URL("../src/rusty-view-chat-api.ts", import.meta.url),
  "utf8",
);
for (const forbidden of [
  "pendingMessagesForSession",
  "messageSlotEvents",
  "chatEventStats",
  "projectBodyStateJson",
  "listChatEvents?",
  "chatReadModelPage?",
]) {
  assert.equal(
    apiSource.includes(forbidden),
    false,
    `chat HTTP envelope must not reconstruct Rust read policy via ${forbidden}`,
  );
}
const operationSource = readFileSync(
  new URL("../src/service-rusty-view-chat-operations.ts", import.meta.url),
  "utf8",
);
for (const forbidden of [
  "bridge.queryMessageSlots(",
  "bridge.queryMessageVariants(",
  "bridge.queryConversationBranches(",
  "bridge.queryConversationSnapshots(",
  "bridge.queryAttachments(",
  "bridge.queryDataBankScopes(",
]) {
  assert.equal(
    operationSource.includes(forbidden),
    false,
    `chat operation boundary must not restore non-exact read ${forbidden}`,
  );
}
for (const required of [
  "queryChatSessionSummaries",
  "readChatSession",
  "queryMessageSlotsPage",
  "queryMessageVariantsPage",
  "readConversationTree",
  "searchChatTranscript",
  "queryAttachmentsPage",
  "queryDataBankScopesPage",
]) {
  assert.ok(
    operationSource.includes(`bridge.${required}`),
    `chat operation boundary must delegate through bridge.${required}`,
  );
}

const accepted = await handleRustyViewChatRequest(
  {
    method: "POST",
    url: "/v1/chat/sessions/chat-authority-session/messages",
    body: {
      actor: { id: "operator", kind: "human", display_name: "Operator" },
      body: "  route delegates to the chat wake port  ",
      client_message_id: "client-msg-1",
      reason: "authority-boundary-smoke",
    },
    requestId: "request-send-1",
  },
  {
    listSessions: async () => [activeSession, archivedSession],
    sendMessage: async (input) => {
      submitted.push(input);
      return {
        status: "accepted",
        message_id: "message-from-port",
        slot_id: "slot-from-port",
        primary_variant_id: "variant-from-port",
        wake_id: "wake-from-port",
        correlation_id: "correlation-from-port",
        latest_cursor: "chat-authority-session:7",
        summary: "accepted by chat wake port",
      };
    },
  },
);

assert.equal(accepted.status, 202);
assert.equal(accepted.body.ok, true);
assert.equal(submitted.length, 1);
assert.equal(submitted[0]?.session.sessionId, activeSession.sessionId);
assert.equal(submitted[0]?.actor.id, "operator");
assert.equal(submitted[0]?.actor.kind, "human");
assert.equal(submitted[0]?.actor.display_name, "Operator");
assert.equal(submitted[0]?.body, "route delegates to the chat wake port");
assert.equal(submitted[0]?.clientMessageId, "client-msg-1");
assert.equal(submitted[0]?.idempotencyKey, "client-msg-1");
assert.equal(submitted[0]?.reason, "authority-boundary-smoke");
assert.equal(submitted[0]?.requestId, "request-send-1");
assert.equal(
  (accepted.body.data as { wake_id?: string }).wake_id,
  "wake-from-port",
);

const headerIdempotency = await handleRustyViewChatRequest(
  {
    method: "POST",
    url: "/v1/chat/sessions/chat-authority-session/messages",
    headers: { "Idempotency-Key": "header-key-1" },
    body: {
      actor: { id: "operator", kind: "human" },
      body: "header key wins",
      client_message_id: "client-msg-2",
    },
    requestId: "request-send-2",
  },
  {
    listSessions: async () => [activeSession],
    sendMessage: async (input) => {
      submitted.push(input);
      return {
        status: "accepted",
        message_id: "message-from-port-2",
        latest_cursor: "chat-authority-session:8",
      };
    },
  },
);

assert.equal(headerIdempotency.status, 202);
assert.equal(submitted.length, 2);
assert.equal(submitted[1]?.idempotencyKey, "header-key-1");

let archivedPortCalled = false;
const archived = await handleRustyViewChatRequest(
  {
    method: "POST",
    url: "/v1/chat/sessions/chat-authority-archived/messages",
    body: {
      actor: { id: "operator", kind: "human" },
      body: "should not reach authority port",
    },
    requestId: "request-archived",
  },
  {
    listSessions: async () => [archivedSession],
    sendMessage: async () => {
      archivedPortCalled = true;
      throw new Error("archived session should not submit chat wake");
    },
  },
);

assert.equal(archived.status, 412);
assert.equal(archivedPortCalled, false);
assert.equal(archived.body.ok, false);
if (!archived.body.ok) {
  assert.equal(archived.body.error.reason_code, "chat_session_archived");
}

console.log("rusty view chat authority boundary smoke passed");

function session(sessionId: string, status: "idle" | "archived"): SessionState {
  return {
    handle: sessionId.length as SessionHandle,
    sessionId: sessionId as SessionId,
    agentId: `${sessionId}-agent` as AgentId,
    profileId: `${sessionId}-profile` as ProfileId,
    kind: "full",
    status,
    brainTurnCount: 0,
    createdAt: "2026-07-06T00:00:00.000Z",
    lastActiveAt: "2026-07-06T00:00:00.000Z",
    resourceLimits: {},
    toolProfile: { tools: [] },
  };
}
