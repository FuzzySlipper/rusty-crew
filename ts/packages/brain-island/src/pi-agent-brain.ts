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
} from "@rusty-crew/contracts";
import type {
  BrainActionPlanner,
  BrainImplementation,
  BrainRoleAssembly,
  BrainWakeInput,
  BrainWakeResult,
} from "./index.js";

export type PiAgentLike = Pick<
  PiAgent,
  "prompt" | "subscribe" | "waitForIdle"
> &
  Partial<Pick<PiAgent, "clearAllQueues">>;

export type PiAgentFactory = (options: PiAgentOptions) => PiAgentLike;

export type PiAgentToolResolver = (input: {
  wake: BrainWakeInput;
  tools: ToolDescriptor[];
}) => PiAgentTool[];

export interface PiAgentBrainOptions {
  createAgent: PiAgentFactory;
  planActions?: BrainActionPlanner;
  resolveTools?: PiAgentToolResolver;
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
      tools: resolveAllowedTools(input, options.resolveTools),
    },
    sessionId: input.sessionId,
    steeringMode: "one-at-a-time",
    followUpMode: "one-at-a-time",
  };
}

function resolveAllowedTools(
  input: BrainWakeInput,
  resolveTools: PiAgentToolResolver | undefined,
): PiAgentTool[] {
  const allowedDescriptors = input.state.session.toolProfile.tools;
  if (!resolveTools || allowedDescriptors.length === 0) {
    return [];
  }

  const allowedNames = new Set(
    allowedDescriptors.map((descriptor) => descriptor.name),
  );
  return resolveTools({
    wake: input,
    tools: allowedDescriptors,
  }).filter((tool) => allowedNames.has(tool.name));
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
      const text = extractText(event.message);
      return text ? { type: "text_delta", text } : undefined;
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

function extractText(message: PiAgentMessage): string | undefined {
  if (!("content" in message) || !Array.isArray(message.content)) {
    return undefined;
  }

  const parts = message.content.flatMap((part: unknown) => {
    if (
      typeof part === "object" &&
      part !== null &&
      "type" in part &&
      part.type === "text" &&
      "text" in part &&
      typeof part.text === "string"
    ) {
      return [part.text];
    }
    return [];
  });
  return parts.join("");
}
