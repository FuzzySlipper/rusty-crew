import type {
  Agent as PiAgent,
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool as PiAgentTool,
} from "@earendil-works/pi-agent-core";
import type {
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
  resolveToolSession,
  type PiAgentToolResolver,
} from "./tool-session-selection.js";

export type PiAgentLike = Pick<
  PiAgent,
  "prompt" | "subscribe" | "waitForIdle"
> &
  Partial<Pick<PiAgent, "clearAllQueues">>;

export type PiAgentFactory = (options: PiAgentOptions) => PiAgentLike;

export interface PiAgentBrainOptions {
  createAgent: PiAgentFactory;
  planActions?: BrainActionPlanner;
  resolveTools?: PiAgentToolResolver;
  toolProfile?: ToolProfile;
}

export function createPiAgentBrain(
  options: PiAgentBrainOptions,
): BrainImplementation {
  return {
    async wake(input: BrainWakeInput): Promise<BrainWakeResult> {
      const events: BrainWakeResult["events"] = [];
      const agent = options.createAgent(buildAgentOptions(input, options));
      const unsubscribe = agent.subscribe((event) => {
        const mapped = mapPiAgentEvent(event);
        if (mapped) {
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

      return {
        events,
        actions: options.planActions
          ? await options.planActions({ wake: input, events })
          : [],
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
      ),
    },
    sessionId: input.sessionId,
    steeringMode: "one-at-a-time",
    followUpMode: "one-at-a-time",
  };
}

function resolveAllowedTools(
  input: BrainWakeInput,
  resolveTools: PiAgentToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
): PiAgentTool[] {
  return resolveToolSession({ wake: input, resolveTools, toolProfile }).tools;
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

function mapPiAgentEvent(event: PiAgentEvent): BrainEvent | undefined {
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
