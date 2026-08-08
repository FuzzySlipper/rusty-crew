import assert from "node:assert/strict";
import test from "node:test";

import type {
  BrainImplementationHandle,
  BrainWakeRequest,
  RuntimeBufferHandle,
  RuntimeBufferView,
  SessionId,
  Unit,
} from "@rusty-crew/contracts";
import type {
  BrainHostExecutor,
  BrainWakeInput,
} from "../src/brain-host-runtime.js";
import {
  DurableConversationReconstructionError,
  wakeBrainFromBridgeRequest,
} from "../src/bridge-wake.js";

test("provider-state recovery loads the durable conversation projection", async () => {
  const observed: BrainWakeInput[] = [];
  let readCount = 0;
  const bodyState = JSON.stringify({ session: { agentId: "agent-1" } });
  const buffers = recoveryBuffers({
    bodyState,
    readChatSession: async (input) => {
      readCount += 1;
      assert.equal(
        (input as { cursor?: string }).cursor,
        readCount === 1 ? "session-1:0" : "session-1:2",
      );
      if (readCount === 2) {
        return {
          events: [
            {
              kind: "message_created",
              payload: { role: "tool", body: "tool result" },
            },
          ],
          has_more: false,
          latest_cursor: "session-1:3",
        };
      }
      return {
        events: [
          {
            kind: "message_created",
            payload: { role: "user", body: "first prompt" },
          },
          {
            kind: "message_created",
            payload: { role: "assistant", body: "first answer" },
          },
        ],
        has_more: true,
        latest_cursor: "session-1:2",
      };
    },
  });

  await wakeBrainFromBridgeRequest(
    buffers,
    recordingBrain(observed),
    wakeRequest(),
  );

  assert.equal(readCount, 2);
  assert.deepEqual(observed[0]?.durableConversation, [
    { role: "user", content: "first prompt" },
    { role: "assistant", content: "first answer" },
    { role: "tool", content: "tool result" },
  ]);
});

test("provider-state recovery does not read durable history when state is present", async () => {
  const observed: BrainWakeInput[] = [];
  let readCount = 0;
  const buffers = recoveryBuffers({
    bodyState: JSON.stringify({ session: { agentId: "agent-1" } }),
    readChatSession: async () => {
      readCount += 1;
      return { events: [], has_more: false };
    },
  });

  await wakeBrainFromBridgeRequest(
    buffers,
    recordingBrain(observed),
    wakeRequest({
      providerState: {
        moduleId: "chat-completions",
        strategyId: "default",
        profileFingerprint: "profile:v1",
        providerFingerprint: "provider:v1",
        payloadVersion: "chat-completions:v1",
        payload: {},
      },
    }),
  );

  assert.equal(readCount, 0);
  assert.equal(observed[0]?.durableConversation, undefined);
});

test("Rust body-state decoding preserves the canonical session workspace", async () => {
  const observed: BrainWakeInput[] = [];
  const bodyState = JSON.stringify({
    session: {
      handle: 1,
      session_id: "session-1",
      agent_id: "agent-1",
      profile_id: "profile-1",
      kind: "full",
      workspace: {
        cwd: "/home/dev/rusty-crew",
        revision: 3,
        updated_at: "2026-08-08T18:00:00Z",
      },
      resource_limits: {},
      tool_profile: { tools: [] },
      status: "idle",
      brain_turn_count: 0,
      created_at: "2026-08-08T17:00:00Z",
      last_active_at: "2026-08-08T18:00:00Z",
    },
    pending_messages: [],
    recent_events: [],
    child_completions: [],
    fan_out_groups: [],
    delta_policy: {
      mode: "frozen_snapshot_next_wake",
      queue_owner: "body",
      queued_message_ttl_ms: 60_000,
      max_queued_messages: 32,
    },
  });
  const buffers = recoveryBuffers({
    bodyState,
    readChatSession: async () => ({ events: [], has_more: false }),
  });

  await wakeBrainFromBridgeRequest(
    buffers,
    recordingBrain(observed),
    wakeRequest({
      providerState: {
        moduleId: "chat-completions",
        strategyId: "default",
        profileFingerprint: "profile:v1",
        providerFingerprint: "provider:v1",
        payloadVersion: "chat-completions:v1",
        payload: {},
      },
    }),
  );

  assert.deepEqual(observed[0]?.state.session.workspace, {
    cwd: "/home/dev/rusty-crew",
    revision: 3,
    updatedAt: "2026-08-08T18:00:00Z",
  });
});

test("provider-state recovery fails before the brain runs when the first page is unavailable", async () => {
  const observed: BrainWakeInput[] = [];
  const buffers = recoveryBuffers({
    bodyState: JSON.stringify({ session: { agentId: "agent-1" } }),
    readChatSession: async () => {
      throw new Error("projection store unavailable");
    },
  });

  await assert.rejects(
    wakeBrainFromBridgeRequest(
      buffers,
      recordingBrain(observed),
      wakeRequest(),
    ),
    (error: unknown) =>
      error instanceof DurableConversationReconstructionError &&
      error.reasonCode === "durable_conversation_reconstruction_failed" &&
      error.failureKind === "read_failed" &&
      error.loadedMessageCount === 0 &&
      error.cursor === "session-1:0",
  );
  assert.deepEqual(observed, []);
});

test("provider-state recovery fails before the brain runs when a later page is unavailable", async () => {
  const observed: BrainWakeInput[] = [];
  let readCount = 0;
  const buffers = recoveryBuffers({
    bodyState: JSON.stringify({ session: { agentId: "agent-1" } }),
    readChatSession: async () => {
      readCount += 1;
      if (readCount === 1) {
        return {
          events: [
            {
              kind: "message_created",
              payload: { role: "user", body: "durable prompt" },
            },
          ],
          has_more: true,
          latest_cursor: "session-1:1",
        };
      }
      throw new Error("projection store disconnected");
    },
  });

  await assert.rejects(
    wakeBrainFromBridgeRequest(
      buffers,
      recordingBrain(observed),
      wakeRequest(),
    ),
    (error: unknown) =>
      error instanceof DurableConversationReconstructionError &&
      error.failureKind === "read_failed" &&
      error.loadedMessageCount === 1 &&
      error.cursor === "session-1:1",
  );
  assert.deepEqual(observed, []);
});

test("provider-state recovery rejects a page that advertises more data without a cursor", async () => {
  const observed: BrainWakeInput[] = [];
  const buffers = recoveryBuffers({
    bodyState: JSON.stringify({ session: { agentId: "agent-1" } }),
    readChatSession: async () => ({
      events: [
        {
          kind: "message_created",
          payload: { role: "user", body: "partial prompt" },
        },
      ],
      has_more: true,
    }),
  });

  await assert.rejects(
    wakeBrainFromBridgeRequest(
      buffers,
      recordingBrain(observed),
      wakeRequest(),
    ),
    (error: unknown) =>
      error instanceof DurableConversationReconstructionError &&
      error.failureKind === "pagination_cursor_missing" &&
      error.loadedMessageCount === 1 &&
      error.cursor === "session-1:0",
  );
  assert.deepEqual(observed, []);
});

test("provider-state recovery rejects a page with a non-string next cursor", async () => {
  const observed: BrainWakeInput[] = [];
  const buffers = recoveryBuffers({
    bodyState: JSON.stringify({ session: { agentId: "agent-1" } }),
    readChatSession: async () => ({
      events: [],
      has_more: true,
      latest_cursor: 2,
    }),
  });

  await assert.rejects(
    wakeBrainFromBridgeRequest(
      buffers,
      recordingBrain(observed),
      wakeRequest(),
    ),
    (error: unknown) =>
      error instanceof DurableConversationReconstructionError &&
      error.failureKind === "pagination_cursor_missing" &&
      error.loadedMessageCount === 0 &&
      error.cursor === "session-1:0",
  );
  assert.deepEqual(observed, []);
});

function recordingBrain(observed: BrainWakeInput[]): BrainHostExecutor {
  return {
    async wake(input) {
      observed.push(input);
      return { events: [], actions: [] };
    },
  };
}

function recoveryBuffers(options: {
  bodyState: string;
  readChatSession: (input: unknown) => Promise<unknown>;
}): {
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
  readChatSession(input: unknown): Promise<unknown>;
} {
  const bodyHandle = 1 as unknown as RuntimeBufferHandle;
  const systemPromptHandle = 2 as unknown as RuntimeBufferHandle;
  const roleAssemblyHandle = 3 as unknown as RuntimeBufferHandle;
  const values = new Map<RuntimeBufferHandle, string>([
    [bodyHandle, options.bodyState],
    [systemPromptHandle, "system prompt"],
    [roleAssemblyHandle, "{}"],
  ]);
  return {
    async getBuffer(handle) {
      const value = values.get(handle);
      if (value === undefined) throw new Error(`missing buffer ${handle}`);
      const bytes = new TextEncoder().encode(value);
      return {
        handle,
        mediaType: "text/plain",
        byteLen: bytes.byteLength,
        bytes,
      };
    },
    async releaseBuffer() {
      return {};
    },
    readChatSession: options.readChatSession,
  };
}

function wakeRequest(
  overrides: Partial<BrainWakeRequest> = {},
): BrainWakeRequest {
  return {
    bodyState: 1 as unknown as RuntimeBufferHandle,
    brain: 9 as unknown as BrainImplementationHandle,
    roleAssembly: 3 as unknown as RuntimeBufferHandle,
    sessionId: "session-1" as unknown as SessionId,
    systemPrompt: 2 as unknown as RuntimeBufferHandle,
    wakeId: "wake-1",
    ...overrides,
  };
}
