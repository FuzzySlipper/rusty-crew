import assert from "node:assert/strict";
import type {
  AgentId,
  BrainImplementationHandle,
  BrainWakeRequest,
  ProfileId,
  RuntimeBufferHandle,
  RuntimeBufferView,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { createLocalBrain } from "./index.js";
import {
  type BridgeBufferClient,
  wakeBrainFromBridgeRequest,
} from "./bridge-wake.js";

const encoder = new TextEncoder();
let nextHandle = 1;
const activeBuffers = new Map<RuntimeBufferHandle, RuntimeBufferView>();
const releasedHandles: RuntimeBufferHandle[] = [];

const buffers: BridgeBufferClient = {
  async getBuffer(handle) {
    const view = activeBuffers.get(handle);
    if (!view) {
      throw new Error(`buffer ${handle} is not active`);
    }
    return view;
  },
  async releaseBuffer(handle) {
    if (!activeBuffers.delete(handle)) {
      throw new Error(`buffer ${handle} released twice`);
    }
    releasedHandles.push(handle);
    return {};
  },
};

const sessionId = "bridge-wake-session" as SessionId;
const request: BrainWakeRequest = {
  brain: 1 as BrainImplementationHandle,
  sessionId,
  bodyState: insertJson({
    session: {
      handle: 1 as SessionHandle,
      sessionId,
      agentId: "bridge-agent" as AgentId,
      profileId: "bridge-profile" as ProfileId,
      kind: "worker",
      resourceLimits: {},
      toolProfile: { tools: [] },
      status: "idle",
      brainTurnCount: 0,
      createdAt: "2026-06-19T00:00:00Z",
      lastActiveAt: "2026-06-19T00:00:00Z",
    },
    pendingMessages: [
      {
        from: "planner" as AgentId,
        to: "bridge-agent" as AgentId,
        body: "large body state ".repeat(4096),
      },
    ],
    recentEvents: [],
  }),
  systemPrompt: insertText("system prompt ".repeat(4096)),
  roleAssembly: insertJson({
    instructions: "hydrate through RuntimeBufferHandle",
    initialMessages: [],
  }),
  wakeId: "bridge-wake-1",
};

const result = await wakeBrainFromBridgeRequest(
  buffers,
  createLocalBrain(),
  request,
);

assert.deepEqual(
  result.events.map((event) => event.event.type),
  ["started", "text_delta", "finished"],
);
assert.equal(result.actions[0]?.type, "deliver_completion");
assert.equal(releasedHandles.length, 3);
assert.equal(activeBuffers.size, 0);

console.log(
  JSON.stringify(
    {
      eventTypes: result.events.map((event) => event.event.type),
      actionTypes: result.actions.map((action) => action.type),
      releasedHandles: releasedHandles.length,
      activeBuffers: activeBuffers.size,
    },
    null,
    2,
  ),
);

function insertJson(value: unknown): RuntimeBufferHandle {
  return insert("application/json", JSON.stringify(value));
}

function insertText(value: string): RuntimeBufferHandle {
  return insert("text/plain; charset=utf-8", value);
}

function insert(mediaType: string, value: string): RuntimeBufferHandle {
  const handle = nextHandle++ as RuntimeBufferHandle;
  const bytes = encoder.encode(value);
  activeBuffers.set(handle, {
    handle,
    mediaType,
    byteLen: bytes.byteLength,
    bytes,
  });
  return handle;
}
