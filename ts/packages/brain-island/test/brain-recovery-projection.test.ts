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
import { wakeBrainFromBridgeRequest } from "../src/bridge-wake.js";

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
