import assert from "node:assert/strict";
import type {
  AgentOptions as ChatCompletionsOptions,
  AgentEvent as ChatCompletionsEvent,
} from "./support/chat-completions-test-harness.js";
import type { BodyState, SessionId } from "@rusty-crew/contracts";
import { Type } from "typebox";
import type { BrainTool } from "../src/brain-tool.js";
import { handleRustyViewChatRequest } from "../src/rusty-view-chat-api.js";
import { MemoryToolCallDebugStore } from "../src/tool-call-debug-store.js";
import { createChatCompletionsBrain } from "./support/chat-completions-test-harness.js";

const toolCallDebugStore = new MemoryToolCallDebugStore({
  maxJsonChars: 256,
  maxPartialUpdates: 2,
  retentionMs: 60_000,
});

const parameters = Type.Object({
  apiKey: Type.String(),
  query: Type.String(),
});

const debugTool: BrainTool<typeof parameters, { count: number }> = {
  name: "debug_echo",
  label: "Debug Echo",
  description: "Echoes safe text for debug inspection smoke.",
  parameters,
  execute: async (_callId, params, _signal, onUpdate) => {
    onUpdate?.({
      content: [{ type: "text", text: "partial" }],
      details: { count: 1 },
    });
    return {
      content: [{ type: "text", text: `echo:${params.query}` }],
      details: { count: 2 },
    };
  },
};

let eventSink:
  | ((event: ChatCompletionsEvent, signal: AbortSignal) => Promise<void> | void)
  | undefined;
const emit = (event: ChatCompletionsEvent): void => {
  void eventSink?.(event, new AbortController().signal);
};

const brain = createChatCompletionsBrain({
  toolCallDebugStore,
  createAgent: (options: ChatCompletionsOptions) => ({
    subscribe: (sink) => {
      eventSink = sink;
      return () => {
        eventSink = undefined;
      };
    },
    prompt: async () => {
      const tool = options.initialState?.tools?.[0];
      assert.ok(tool, "debug tool should be available");
      emit({ type: "agent_start" });
      emit({
        type: "tool_execution_start",
        toolCallId: "call-debug-1",
        toolName: tool.name,
        args: { apiKey: "super-secret", query: "inspect me" },
      });
      const result = await tool.execute(
        "call-debug-1",
        { apiKey: "super-secret", query: "inspect me" },
        undefined,
        (partialResult) => {
          emit({
            type: "tool_execution_update",
            toolCallId: "call-debug-1",
            toolName: tool.name,
            args: { apiKey: "super-secret", query: "inspect me" },
            partialResult,
          });
        },
      );
      emit({
        type: "tool_execution_end",
        toolCallId: "call-debug-1",
        toolName: tool.name,
        result,
        isError: false,
      });
      emit({ type: "agent_end", messages: [] });
    },
    waitForIdle: async () => undefined,
    clearAllQueues: () => undefined,
  }),
  resolveTools: () => [debugTool],
});

const result = await brain.wake({
  wakeId: "wake-debug-1",
  sessionId: "session-debug" as SessionId,
  systemPrompt: "debug smoke",
  roleAssembly: {},
  state: {
    session: {
      sessionId: "session-debug",
      agentId: "agent-debug",
      profileId: "profile-debug",
      kind: "full",
      status: "idle",
      brainTurnCount: 0,
      createdAt: "2026-07-03T00:00:00.000Z",
      lastActiveAt: "2026-07-03T00:00:00.000Z",
      resourceLimits: {},
      toolProfile: {
        tools: [
          { name: "debug_echo", description: "Echoes debug smoke input." },
        ],
      },
    },
    pendingMessages: [],
    recentEvents: [],
    activeAssignments: [],
  } as unknown as BodyState,
});

const started = result.events.find(
  (event) => event.event.type === "tool_call_started",
);
assert.equal(started?.event.type, "tool_call_started");
assert.equal(
  started.event.metadata?.debugDetailId?.startsWith("tooldbg_"),
  true,
);

const finished = result.events.find(
  (event) => event.event.type === "tool_call_finished",
);
assert.equal(finished?.event.type, "tool_call_finished");
assert.equal(
  finished.event.metadata?.debugDetailId,
  started.event.metadata?.debugDetailId,
);

const detail = toolCallDebugStore.get({
  sessionId: "session-debug",
  debugDetailId: started.event.metadata?.debugDetailId ?? "",
});
assert.ok(detail);
assert.equal(detail.tool_name, "debug_echo");
assert.equal(
  (detail.arguments.value as { apiKey?: string }).apiKey,
  "[redacted]",
);
assert.equal(detail.partial_updates.length, 1);
assert.equal(detail.status, "completed");

const apiResult = await handleRustyViewChatRequest(
  {
    method: "GET",
    url: `/v1/chat/sessions/session-debug/tool-calls/${detail.debug_detail_id}`,
    requestId: "debug-route",
  },
  {
    listSessions: async () => [
      {
        sessionId: "session-debug",
        agentId: "agent-debug",
        profileId: "profile-debug",
        kind: "full",
        status: "idle",
        brainTurnCount: 1,
        createdAt: "2026-07-03T00:00:00.000Z",
        lastActiveAt: "2026-07-03T00:00:00.000Z",
        resourceLimits: {},
        toolProfile: { tools: [] },
      } as never,
    ],
    projectBodyStateJson: async () => new Uint8Array(),
    getToolCallDebugDetail: async (input) =>
      toolCallDebugStore.get({
        sessionId: input.session.sessionId,
        debugDetailId: input.debugDetailId,
      }) as never,
  },
);
assert.equal(apiResult.status, 200);
assert.equal(apiResult.body.ok, true);
assert.equal(
  (apiResult.body.data as { debug_detail_id: string }).debug_detail_id,
  detail.debug_detail_id,
);

const missing = await handleRustyViewChatRequest(
  {
    method: "GET",
    url: "/v1/chat/sessions/session-debug/tool-calls/tooldbg_missing",
    requestId: "debug-route-missing",
  },
  {
    listSessions: async () => [
      {
        sessionId: "session-debug",
        agentId: "agent-debug",
        profileId: "profile-debug",
        kind: "full",
        status: "idle",
        brainTurnCount: 1,
        createdAt: "2026-07-03T00:00:00.000Z",
        lastActiveAt: "2026-07-03T00:00:00.000Z",
        resourceLimits: {},
        toolProfile: { tools: [] },
      } as never,
    ],
    projectBodyStateJson: async () => new Uint8Array(),
    getToolCallDebugDetail: async () => undefined,
  },
);
assert.equal(missing.status, 404);
assert.equal(
  (missing.body as { error: { reason_code: string } }).error.reason_code,
  "tool_call_debug_detail_not_found",
);

console.log("tool call debug inspection smoke passed");
