import type { Static, TSchema } from "typebox";
import type {
  BrainAction,
  AgentMessage as RustyAgentMessage,
  BrainEvent,
  BrainEventEnvelope,
  SessionId,
  ToolDescriptor,
  ToolProfile,
} from "@rusty-crew/contracts";
import type { BrainToolExecutionMode } from "../../src/brain-tool.js";
import type {
  BrainActionPlanner,
  BrainHostExecutor,
  BrainRoleAssembly,
  BrainWakeInput,
  BrainWakeResult,
} from "../../src/index.js";
import {
  type BrainActionCollector,
  resolveToolSession,
  type BrainToolResolver,
} from "../../src/tool-session-selection.js";
import { toPiAgentTools } from "./legacy-pi-tool-adapter-test-harness.js";
import {
  localToolCallMetadata,
  type ToolCallDebugStore,
  withToolCallDebugReference,
} from "../../src/tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "../../src/provider-request-debug-store.js";

// Legacy TS pi-agent test harness only. Production pi-agent module wiring now
// uses the Rust pi-agent brain through the native bridge.
export interface AgentToolResult<TDetails = unknown> {
  content: Array<
    | { type: "text"; text: string }
    | { type: "image"; data: string; mimeType: string }
  >;
  details: TDetails;
  terminate?: boolean;
}

export type AgentToolUpdateCallback<TDetails = unknown> = (
  partial: AgentToolResult<TDetails>,
) => void | Promise<void>;

export interface AgentTool<
  TParameters extends TSchema = TSchema,
  TDetails = unknown,
> {
  name: string;
  description: string;
  label: string;
  parameters: TParameters;
  prepareArguments?: (raw: unknown) => Static<TParameters>;
  execute(
    toolCallId: string,
    params: Static<TParameters>,
    signal?: AbortSignal,
    onUpdate?: AgentToolUpdateCallback<TDetails>,
  ): Promise<AgentToolResult<TDetails>>;
  executionMode?: BrainToolExecutionMode;
}

export interface AgentOptions {
  initialState?: {
    systemPrompt?: string;
    messages?: AgentMessage[];
    tools?: AgentTool[];
  };
  sessionId: SessionId;
  steeringMode?: string;
  followUpMode?: string;
}

export interface AgentMessage {
  role: "user" | "assistant" | "system" | string;
  content: Array<
    | { type: "text"; text: string }
    | { type: "thinking"; thinking: string }
    | { type: "image"; data: string; mimeType: string }
  >;
  api?: string;
  provider?: string;
  model?: string;
  usage?: unknown;
  stopReason?: string;
  errorMessage?: string;
  timestamp?: number;
}

export type AgentEvent =
  | { type: "agent_start" }
  | {
      type: "message_update";
      assistantMessageEvent:
        | { type: "text_delta"; delta?: string }
        | { type: "thinking_delta"; delta?: string }
        | { type: string; delta?: string };
    }
  | { type: "message_end"; message: AgentMessage }
  | {
      type: "tool_execution_start";
      toolCallId: string;
      toolName: string;
      args?: unknown;
      partialResult?: unknown;
    }
  | {
      type: "tool_execution_update";
      toolCallId: string;
      toolName: string;
      args?: unknown;
      partialResult?: unknown;
    }
  | {
      type: "tool_execution_end";
      toolCallId: string;
      toolName: string;
      isError: boolean;
      result?: unknown;
    }
  | { type: "agent_end"; messages: AgentMessage[] };

export interface Agent {
  prompt(input: AgentMessage | AgentMessage[] | string): Promise<void>;
  subscribe(
    listener: (event: AgentEvent, signal: AbortSignal) => void,
  ): () => void;
  waitForIdle(): Promise<void>;
  clearAllQueues?(): void;
}

type PiAgent = Agent;
type PiAgentEvent = AgentEvent;
type PiAgentMessage = AgentMessage;
type PiAgentOptions = AgentOptions;
type PiAgentTool = AgentTool;

export type PiAgentLike = Pick<
  PiAgent,
  "prompt" | "subscribe" | "waitForIdle"
> &
  Partial<Pick<PiAgent, "clearAllQueues">>;

export type PiAgentFactory = (options: PiAgentOptions) => PiAgentLike;

export interface PiAgentBrainOptions {
  createAgent: PiAgentFactory;
  planActions?: BrainActionPlanner;
  resolveTools?: BrainToolResolver;
  toolProfile?: ToolProfile;
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  toolCallDebugStore?: ToolCallDebugStore;
  providerRequestDebugStore?: ProviderRequestDebugStore;
}

export function createPiAgentBrain(
  options: PiAgentBrainOptions,
): BrainHostExecutor {
  return {
    async wake(input: BrainWakeInput): Promise<BrainWakeResult> {
      const events: BrainWakeResult["events"] = [];
      const actions = createBrainActionCollector();
      let submitQueue: Promise<void> = Promise.resolve();
      let submitError: unknown;
      const submitLiveEvent = (event: BrainEventEnvelope): void => {
        if (!options.submitEvent) {
          return;
        }
        submitQueue = submitQueue
          .then(() => options.submitEvent?.(event))
          .then(
            () => undefined,
            (error: unknown) => {
              submitError ??= error;
            },
          );
      };
      const agentOptions = buildAgentOptions(input, options, actions);
      const modelConfig = optionalSessionModelConfig(input.state.session);
      const providerDebug = options.providerRequestDebugStore?.record({
        sessionId: input.sessionId,
        wakeId: input.wakeId,
        brainModule: "pi-agent",
        model: modelConfig?.modelName,
        protocol: modelConfig?.api,
        providerKind: modelConfig?.provider,
        request: {
          boundary: "pi_agent_options",
          sessionId: input.sessionId,
          wakeId: input.wakeId,
          initialState: agentOptions.initialState,
          steeringMode: agentOptions.steeringMode,
          followUpMode: agentOptions.followUpMode,
        },
      });
      if (providerDebug) {
        const event = providerRequestDebugEvent(input, providerDebug);
        events.push(event);
        submitLiveEvent(event);
      }
      const agent = options.createAgent(agentOptions);
      let sawTextDelta = false;
      const unsubscribe = agent.subscribe((event) => {
        const mappedEvents = mapPiAgentEvent(event, {
          sawTextDelta,
          wake: input,
          toolCallDebugStore: options.toolCallDebugStore,
        });
        for (const mapped of mappedEvents) {
          if (mapped.type === "text_delta") {
            sawTextDelta = true;
          }
          const eventEnvelope = envelope(input, mapped);
          events.push(eventEnvelope);
          submitLiveEvent(eventEnvelope);
        }
      });

      try {
        await agent.prompt(
          toPiMessages(input.roleAssembly, input.state.pendingMessages),
        );
        await agent.waitForIdle();
      } finally {
        agent.clearAllQueues?.();
        unsubscribe();
      }
      await submitQueue;
      if (submitError !== undefined) {
        throw submitError;
      }

      const plannedActions = options.planActions
        ? await options.planActions({
            wake: input,
            events,
            toolActions: actions.actions,
          })
        : [];
      return {
        events: options.submitEvent ? [] : events,
        actions: [...actions.actions, ...plannedActions],
      };
    },
  };
}

function optionalSessionModelConfig(session: unknown):
  | {
      modelName?: string;
      api?: string;
      provider?: string;
    }
  | undefined {
  if (!session || typeof session !== "object" || !("modelConfig" in session)) {
    return undefined;
  }
  const modelConfig = (session as { modelConfig?: unknown }).modelConfig;
  if (!modelConfig || typeof modelConfig !== "object") return undefined;
  const record = modelConfig as Record<string, unknown>;
  return {
    modelName:
      typeof record.modelName === "string" ? record.modelName : undefined,
    api: typeof record.api === "string" ? record.api : undefined,
    provider: typeof record.provider === "string" ? record.provider : undefined,
  };
}

function providerRequestDebugEvent(
  input: BrainWakeInput,
  debug: {
    debug_detail_id: string;
    request_sha256: string;
    request_json_chars: number;
    expires_at: string;
  },
): BrainEventEnvelope {
  return envelope(input, {
    type: "provider_status",
    level: "info",
    message: "Provider request debug snapshot captured.",
    metadataJson: JSON.stringify({
      provider_request_debug_detail_id: debug.debug_detail_id,
      provider_request_debug_url: `/v1/chat/sessions/${encodeURIComponent(
        String(input.sessionId),
      )}/provider-requests/${encodeURIComponent(debug.debug_detail_id)}`,
      request_sha256: debug.request_sha256,
      request_json_chars: debug.request_json_chars,
      expires_at: debug.expires_at,
    }),
  });
}

function envelope(
  input: BrainWakeInput,
  event: BrainEvent,
): BrainEventEnvelope {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    event,
  };
}

function buildAgentOptions(
  input: BrainWakeInput,
  options: PiAgentBrainOptions,
  actions: BrainActionCollector,
): PiAgentOptions {
  return {
    initialState: {
      systemPrompt: [input.systemPrompt, input.roleAssembly.instructions]
        .filter(Boolean)
        .join("\n\n"),
      messages: toPiMessages(input.roleAssembly, []),
      tools: resolveAllowedTools(
        input,
        options.resolveTools,
        options.toolProfile,
        actions,
        options.toolCallDebugStore,
      ),
    },
    sessionId: input.sessionId,
    steeringMode: "one-at-a-time",
    followUpMode: "one-at-a-time",
  };
}

function resolveAllowedTools(
  input: BrainWakeInput,
  resolveTools: BrainToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
  actions: BrainActionCollector,
  toolCallDebugStore: ToolCallDebugStore | undefined,
): PiAgentTool[] {
  return toPiAgentTools(
    resolveToolSession({ wake: input, resolveTools, toolProfile, actions })
      .tools,
    { wake: input, toolCallDebugStore },
  );
}

function toPiMessages(
  roleAssembly: BrainRoleAssembly,
  pendingMessages: RustyAgentMessage[],
): PiAgentMessage[] {
  const initial = roleAssembly.initialMessages ?? [];
  return [...initial, ...pendingMessages].map((message) => ({
    role: "user",
    content: [{ type: "text", text: message.body }],
    timestamp: Date.now(),
  }));
}

function mapPiAgentEvent(
  event: PiAgentEvent,
  state: {
    sawTextDelta: boolean;
    wake: BrainWakeInput;
    toolCallDebugStore?: ToolCallDebugStore;
  },
): BrainEvent[] {
  switch (event.type) {
    case "agent_start":
      return [{ type: "started" }];
    case "message_update": {
      if (event.assistantMessageEvent.type === "text_delta") {
        return event.assistantMessageEvent.delta
          ? splitLiteralThinkBlocks(event.assistantMessageEvent.delta)
          : [];
      }
      if (event.assistantMessageEvent.type === "thinking_delta") {
        return event.assistantMessageEvent.delta
          ? [
              {
                type: "reasoning_delta",
                text: event.assistantMessageEvent.delta,
                format: "pi-thinking",
              },
            ]
          : [];
      }
      return [];
    }
    case "message_end": {
      if (state.sawTextDelta) {
        return [];
      }
      const events: BrainEvent[] = [];
      const thinking = assistantMessageThinking(event.message);
      if (thinking) {
        events.push({
          type: "reasoning_delta",
          text: thinking,
          format: "pi-thinking",
        });
      }
      const text = assistantMessageText(event.message);
      if (text) events.push(...splitLiteralThinkBlocks(text));
      if (events.length > 0) return events;
      const errorText = assistantMessageErrorText(event.message);
      return errorText ? [{ type: "text_delta", text: errorText }] : [];
    }
    case "tool_execution_start": {
      const debugRecord = state.toolCallDebugStore?.start({
        sessionId: state.wake.sessionId,
        wakeId: state.wake.wakeId,
        toolCallId: event.toolCallId,
        toolName: event.toolName,
        arguments: event.args,
        sourceMetadata: localToolCallMetadata(event.toolName),
      });
      return [
        {
          type: "tool_call_started",
          toolName: event.toolName,
          metadata: withToolCallDebugReference(
            localToolCallMetadata(event.toolName),
            debugRecord?.debug_detail_id,
          ),
        },
      ];
    }
    case "tool_execution_update":
      return [];
    case "tool_execution_end":
      const debugRecord = state.toolCallDebugStore?.referenceForToolCall({
        sessionId: state.wake.sessionId,
        wakeId: state.wake.wakeId,
        toolCallId: event.toolCallId,
      });
      return [
        {
          type: "tool_call_finished",
          toolName: event.toolName,
          isError: event.isError,
          metadata: withToolCallDebugReference(
            localToolCallMetadata(event.toolName),
            debugRecord?.debug_detail_id,
          ),
        },
      ];
    case "agent_end":
      return [{ type: "finished" }];
    default:
      return [];
  }
}

function splitLiteralThinkBlocks(text: string): BrainEvent[] {
  const events: BrainEvent[] = [];
  let cursor = 0;
  const openTag = "<think>";
  const closeTag = "</think>";

  while (cursor < text.length) {
    const open = text.indexOf(openTag, cursor);
    if (open === -1) {
      pushTextDelta(events, text.slice(cursor));
      break;
    }
    pushTextDelta(events, text.slice(cursor, open));
    const contentStart = open + openTag.length;
    const close = text.indexOf(closeTag, contentStart);
    if (close === -1) {
      pushReasoningDelta(events, text.slice(contentStart));
      break;
    }
    pushReasoningDelta(events, text.slice(contentStart, close));
    cursor = close + closeTag.length;
  }

  return events;
}

function pushTextDelta(events: BrainEvent[], text: string): void {
  if (text.length > 0) {
    events.push({ type: "text_delta", text });
  }
}

function pushReasoningDelta(events: BrainEvent[], text: string): void {
  if (text.length > 0) {
    events.push({
      type: "reasoning_delta",
      text,
      format: "literal-think-tag",
    });
  }
}

function assistantMessageText(message: PiAgentMessage): string | undefined {
  if (message.role !== "assistant") return undefined;
  const content = Array.isArray(message.content) ? message.content : [];
  const text = content
    .flatMap((item) =>
      item.type === "text" && typeof item.text === "string" ? [item.text] : [],
    )
    .join("");
  return text.trim() ? text : undefined;
}

function assistantMessageThinking(message: PiAgentMessage): string | undefined {
  if (message.role !== "assistant") return undefined;
  const content = Array.isArray(message.content) ? message.content : [];
  const text = content
    .flatMap((item) =>
      item.type === "thinking" && typeof item.thinking === "string"
        ? [item.thinking]
        : [],
    )
    .join("");
  return text.trim() ? text : undefined;
}

function assistantMessageErrorText(
  message: PiAgentMessage,
): string | undefined {
  if (message.role !== "assistant") return undefined;
  const maybeError = message as {
    stopReason?: string;
    errorMessage?: string;
  };
  if (maybeError.stopReason !== "error" || !maybeError.errorMessage?.trim()) {
    return undefined;
  }
  return `LLM error: ${maybeError.errorMessage.trim()}`;
}

function createBrainActionCollector(): BrainActionCollector {
  const actions: BrainAction[] = [];
  return {
    add(action) {
      actions.push(action);
    },
    addMany(nextActions) {
      actions.push(...nextActions);
    },
    get actions() {
      return actions;
    },
  };
}
