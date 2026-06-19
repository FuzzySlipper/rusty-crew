import type {
  Agent as PiAgent,
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  AgentMessage as RustyAgentMessage,
  BrainEvent,
  BrainEventEnvelope,
} from "@rusty-crew/contracts";
import type {
  BrainActionPlanner,
  BrainImplementation,
  BrainRoleAssembly,
  BrainWakeInput,
  BrainWakeResult,
} from "./index.js";

export type PiAgentLike = Pick<PiAgent, "prompt" | "subscribe" | "waitForIdle">;

export type PiAgentFactory = (options: PiAgentOptions) => PiAgentLike;

export interface PiAgentBrainOptions {
  createAgent: PiAgentFactory;
  planActions?: BrainActionPlanner;
}

export function createPiAgentBrain(
  options: PiAgentBrainOptions,
): BrainImplementation {
  return {
    async wake(input: BrainWakeInput): Promise<BrainWakeResult> {
      const events: BrainWakeResult["events"] = [];
      const agent = options.createAgent(buildAgentOptions(input));
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

function buildAgentOptions(input: BrainWakeInput): PiAgentOptions {
  return {
    initialState: {
      systemPrompt: [input.systemPrompt, input.roleAssembly.instructions]
        .filter(Boolean)
        .join("\n\n"),
      messages: toPiMessages(input.roleAssembly, []),
      tools: [],
    },
    sessionId: input.sessionId,
  };
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
