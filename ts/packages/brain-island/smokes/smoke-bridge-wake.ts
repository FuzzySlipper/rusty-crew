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
import { createLocalBrain } from "./support/local-brain-test-support.js";
import {
  type BridgeBufferClient,
  wakeBrainFromBridgeRequest,
} from "../src/bridge-wake.js";

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
const localBrain = createLocalBrain();
const request: BrainWakeRequest = {
  brain: 1 as BrainImplementationHandle,
  sessionId,
  bodyState: insertJson({
    session: {
      handle: 1 as SessionHandle,
      session_id: sessionId,
      agent_id: "bridge-agent" as AgentId,
      profile_id: "bridge-profile" as ProfileId,
      kind: "worker",
      resource_limits: {},
      tool_profile: { tools: [] },
      inference_overrides: { reasoning_effort: "high" },
      status: "idle",
      brain_turn_count: 0,
      created_at: "2026-06-19T00:00:00Z",
      last_active_at: "2026-06-19T00:00:00Z",
    },
    pending_messages: [
      {
        from: "planner" as AgentId,
        to: "bridge-agent" as AgentId,
        body: "large body state ".repeat(4096),
      },
    ],
    recent_events: [
      null,
      {
        type: "agent_message_delivery_observed",
        receipt: { marker: "delivery-preserved" },
      },
      {
        type: "agent_round_observed",
        round: { marker: "round-preserved" },
      },
    ],
    child_completions: [],
    fan_out_groups: [],
    delta_policy: {
      mode: "frozen_snapshot_next_wake",
      queue_owner: "body",
      queued_message_ttl_ms: 5_000,
      max_queued_messages: 32,
    },
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
  {
    async wake(wake, options) {
      assert.equal(
        wake.state.session.inferenceOverrides?.reasoningEffort,
        "high",
      );
      assert.deepEqual(
        wake.state.recentEvents.map((event) => event.type),
        ["agent_message_delivery_observed", "agent_round_observed"],
      );
      return localBrain.wake(wake, options);
    },
  },
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
