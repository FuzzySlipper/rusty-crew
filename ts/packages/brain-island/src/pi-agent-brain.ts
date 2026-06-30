import type {
  Agent as PiAgent,
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool as PiAgentTool,
} from "@earendil-works/pi-agent-core";
import type {
  BrainAction,
  AgentMessage as RustyAgentMessage,
  BrainEvent,
  BrainEventEnvelope,
  ToolDescriptor,
  ToolProfile,
} from "@rusty-crew/contracts";
import type {
  BrainActionPlanner,
  BrainImplementation,
  BrainRoleAssembly,
  BrainWakeInput,
  BrainWakeResult,
} from "./index.js";
import {
  type BrainActionCollector,
  resolveToolSession,
  type BrainToolResolver,
} from "./tool-session-selection.js";
import { toPiAgentTools } from "./pi-tool-adapter.js";

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
}

export function createPiAgentBrain(
  options: PiAgentBrainOptions,
): BrainImplementation {
  return {
    async wake(input: BrainWakeInput): Promise<BrainWakeResult> {
      const events: BrainWakeResult["events"] = [];
      const actions = createBrainActionCollector();
      const agent = options.createAgent(
        buildAgentOptions(input, options, actions),
      );
      let sawTextDelta = false;
      const unsubscribe = agent.subscribe((event) => {
        const mapped = mapPiAgentEvent(event, { sawTextDelta });
        if (mapped) {
          if (mapped.type === "text_delta") {
            sawTextDelta = true;
          }
          events.push(envelope(input, mapped));
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

      const plannedActions = options.planActions
        ? await options.planActions({
            wake: input,
            events,
            toolActions: actions.actions,
          })
        : [];
      return {
        events,
        actions: [...actions.actions, ...plannedActions],
      };
    },
  };
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
): PiAgentTool[] {
  return toPiAgentTools(
    resolveToolSession({ wake: input, resolveTools, toolProfile, actions })
      .tools,
    { wake: input },
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
  state: { sawTextDelta: boolean },
): BrainEvent | undefined {
  switch (event.type) {
    case "agent_start":
      return { type: "started" };
    case "message_update": {
      if (event.assistantMessageEvent.type !== "text_delta") {
        return undefined;
      }
      return event.assistantMessageEvent.delta
        ? { type: "text_delta", text: event.assistantMessageEvent.delta }
        : undefined;
    }
    case "message_end": {
      if (state.sawTextDelta) {
        return undefined;
      }
      const text = assistantMessageText(event.message);
      if (text) return { type: "text_delta", text };
      const errorText = assistantMessageErrorText(event.message);
      return errorText ? { type: "text_delta", text: errorText } : undefined;
    }
    case "tool_execution_start":
      return { type: "tool_call_started", toolName: event.toolName };
    case "tool_execution_end":
      return {
        type: "tool_call_finished",
        toolName: event.toolName,
        isError: event.isError,
      };
    case "agent_end":
      return { type: "finished" };
    default:
      return undefined;
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
