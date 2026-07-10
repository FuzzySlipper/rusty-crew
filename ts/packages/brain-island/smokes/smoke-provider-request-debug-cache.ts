import assert from "node:assert/strict";
import type {
  AgentEvent as PiAgentEvent,
  AgentOptions as PiAgentOptions,
} from "./support/legacy-pi-agent-test-harness.js";
import type { AgentId, BodyState, SessionId } from "@rusty-crew/contracts";
import { Type } from "typebox";
import type { BrainTool } from "../src/brain-tool.js";
import { handleRustyViewChatRequest } from "../src/rusty-view-chat-api.js";
import { MemoryProviderRequestDebugStore } from "../src/provider-request-debug-store.js";
import { createPiAgentBrain } from "./support/legacy-pi-agent-test-harness.js";

const providerRequestDebugStore = new MemoryProviderRequestDebugStore({
  maxJsonChars: 20_000,
  retentionMs: 60_000,
  maxRecords: 10,
});

let capturedOptions: PiAgentOptions | undefined;
let eventSink:
  | ((event: PiAgentEvent, signal: AbortSignal) => Promise<void> | void)
  | undefined;
const emit = (event: PiAgentEvent): void => {
  void eventSink?.(event, new AbortController().signal);
};

const debugTool: BrainTool<
  ReturnType<typeof Type.Object>,
  Record<string, never>
> = {
  name: "debug_tool",
  label: "Debug Tool",
  description: "Tool inventory marker for provider debug smoke.",
  parameters: Type.Object({
    query: Type.String(),
  }),
  execute: async () => ({
    content: [{ type: "text", text: "unused" }],
    details: {},
  }),
};

const brain = createPiAgentBrain({
  providerRequestDebugStore,
  createAgent: (options: PiAgentOptions) => {
    capturedOptions = options;
    return {
      subscribe: (sink) => {
        eventSink = sink;
        return () => {
          eventSink = undefined;
        };
      },
      prompt: async () => {
        emit({ type: "agent_start" });
        emit({
          type: "message_update",
          assistantMessageEvent: {
            type: "text_delta",
            delta: "provider debug response",
          },
        } as PiAgentEvent);
        emit({ type: "agent_end", messages: [] });
      },
      waitForIdle: async () => undefined,
      clearAllQueues: () => undefined,
    };
  },
  resolveTools: () => [debugTool],
});

const result = await brain.wake({
  wakeId: "wake-provider-debug-1",
  sessionId: "session-provider-debug" as SessionId,
  systemPrompt: "System prompt marker with apiKey as ordinary text.",
  roleAssembly: {
    instructions: "Role inventory marker with debug_tool.",
    initialMessages: [
      {
        from: "human" as AgentId,
        to: "agent-debug" as AgentId,
        body: "Initial role message marker.",
      },
    ],
  },
  state: {
    session: {
      sessionId: "session-provider-debug",
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
          {
            name: "debug_tool",
            description: "Tool inventory marker for provider debug smoke.",
          },
        ],
      },
      modelConfig: {
        provider: "den-router",
        api: "openai-completions",
        modelName: "debug-model",
        apiKeyEnv: "DEBUG_API_KEY",
      },
    },
    pendingMessages: [
      {
        from: "human" as AgentId,
        to: "agent-debug" as AgentId,
        body: "Pending message marker.",
      },
    ],
    recentEvents: [],
    activeAssignments: [],
  } as unknown as BodyState,
});

assert.ok(capturedOptions, "Pi agent options should be built");
const providerStatus = result.events.find(
  (event) => event.event.type === "provider_status",
);
assert.equal(providerStatus?.event.type, "provider_status");
const metadata = JSON.parse(providerStatus.event.metadataJson ?? "{}") as {
  provider_request_debug_detail_id?: string;
  provider_request_debug_url?: string;
};
assert.equal(
  metadata.provider_request_debug_detail_id?.startsWith("providerdbg_"),
  true,
);
assert.match(metadata.provider_request_debug_url ?? "", /provider-requests/);

const detail = providerRequestDebugStore.get({
  sessionId: "session-provider-debug",
  debugDetailId: metadata.provider_request_debug_detail_id ?? "",
});
assert.ok(detail);
assert.equal(detail.provider.brain_module, "pi-agent");
assert.equal(detail.provider.model, "debug-model");
assert.equal(detail.request.redacted, false);
const request = detail.request.value as {
  initialState?: {
    systemPrompt?: string;
    messages?: unknown[];
    tools?: Array<{ name?: string; description?: string }>;
  };
};
assert.match(request.initialState?.systemPrompt ?? "", /System prompt marker/);
assert.match(request.initialState?.systemPrompt ?? "", /Role inventory marker/);
assert.match(JSON.stringify(request.initialState?.tools ?? []), /debug_tool/);

const redacted = providerRequestDebugStore.record({
  sessionId: "session-provider-debug",
  wakeId: "wake-provider-debug-redaction",
  brainModule: "manual",
  request: {
    headers: { authorization: "Bearer should-not-show" },
    body: { accessToken: "secret-token", prompt: "visible prompt" },
  },
});
const redactedRequest = redacted.request.value as {
  headers?: { authorization?: { redacted?: boolean } };
  body?: { accessToken?: { redacted?: boolean }; prompt?: string };
};
assert.equal(redactedRequest.headers?.authorization?.redacted, true);
assert.equal(redactedRequest.body?.accessToken?.redacted, true);
assert.equal(redactedRequest.body?.prompt, "visible prompt");

const apiResult = await handleRustyViewChatRequest(
  {
    method: "GET",
    url: `/v1/chat/sessions/session-provider-debug/provider-requests/${detail.debug_detail_id}`,
    requestId: "provider-debug-route",
  },
  {
    listSessions: async () => [
      {
        sessionId: "session-provider-debug",
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
    getProviderRequestDebugDetail: async (input) =>
      providerRequestDebugStore.get({
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

console.log("provider request debug cache smoke passed");
