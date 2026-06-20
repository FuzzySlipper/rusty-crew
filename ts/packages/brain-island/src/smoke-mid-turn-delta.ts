import assert from "node:assert/strict";
import type {
  AgentId,
  AgentMessage as RustyAgentMessage,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentTool as PiAgentTool,
} from "@earendil-works/pi-agent-core";
import {
  BodyControlledDeltaQueue,
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
} from "./index.js";

class FakeAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;
  private readonly promptedMessages: PiAgentMessage[] = [];
  clearAllQueuesCalls = 0;

  constructor(private readonly onPrompt: () => Promise<void>) {}

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    const messages = Array.isArray(input)
      ? input
      : typeof input === "string"
        ? [
            {
              role: "user",
              content: [{ type: "text", text: input }],
              timestamp: Date.now(),
            } as PiAgentMessage,
          ]
        : [input];
    this.promptedMessages.push(...messages);
    await this.emit({ type: "agent_start" } as PiAgentEvent);
    await this.emit({
      type: "message_update",
      message: {
        role: "assistant",
        content: [{ type: "text", text: "working" }],
        timestamp: Date.now(),
      },
    } as PiAgentEvent);
    await this.onPrompt();
    await this.emit({ type: "agent_end", messages: [] } as PiAgentEvent);
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {
    this.clearAllQueuesCalls += 1;
  }

  promptedText(): string {
    return this.promptedMessages
      .flatMap((message) => {
        const maybeMessage = message as { content?: unknown };
        return Array.isArray(maybeMessage.content) ? maybeMessage.content : [];
      })
      .flatMap((content: unknown) =>
        isTextContent(content) ? [content.text] : [],
      )
      .join("\n");
  }

  private async emit(event: PiAgentEvent): Promise<void> {
    this.listener?.(event, new AbortController().signal);
  }
}

function isTextContent(
  content: unknown,
): content is { type: "text"; text: string } {
  return (
    typeof content === "object" &&
    content !== null &&
    "type" in content &&
    content.type === "text" &&
    "text" in content &&
    typeof content.text === "string"
  );
}

const sessionId = "mid-turn-session" as SessionId;
const agentId = "mid-turn-agent" as AgentId;
const queuedMessage: RustyAgentMessage = {
  from: "planner" as AgentId,
  to: agentId,
  body: "arrived while active",
};
const queue = new BodyControlledDeltaQueue({
  ...defaultBodyDeltaPolicy,
  queuedMessageTtlMs: 25,
  maxQueuedMessages: 2,
});
const fakeAgent = new FakeAgent(async () => {
  queue.enqueue({
    sessionId,
    activeWakeId: "wake-1",
    message: queuedMessage,
    nowMs: 1_000,
  });
});

const brain = createPiAgentBrain({
  createAgent: () => fakeAgent,
});

await brain.wake({
  wakeId: "wake-1",
  sessionId,
  systemPrompt: "system",
  roleAssembly: { instructions: "test mid-turn policy" },
  state: {
    session: {
      handle: 1 as SessionHandle,
      sessionId,
      agentId,
      profileId: "mid-turn-profile" as ProfileId,
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
        to: agentId,
        body: "frozen snapshot message",
      },
    ],
    recentEvents: [],
    childCompletions: [],
    fanOutGroups: [],
    deltaPolicy: {
      ...defaultBodyDeltaPolicy,
      queuedMessageTtlMs: 25,
      maxQueuedMessages: 2,
    },
  },
});

assert.equal(fakeAgent.clearAllQueuesCalls, 1);
assert.equal(fakeAgent.promptedText(), "frozen snapshot message");
assert.equal(queue.size(), 1);

let capturedToolNames: string[] = [];
const toolFilteredBrain = createPiAgentBrain({
  createAgent: (options) => {
    capturedToolNames = (options.initialState?.tools ?? []).map(
      (tool) => tool.name,
    );
    return new FakeAgent(async () => {});
  },
  resolveTools: () => [fakeTool("read_file"), fakeTool("dangerous_shell")],
});

await toolFilteredBrain.wake({
  wakeId: "wake-tools",
  sessionId,
  systemPrompt: "system",
  roleAssembly: { instructions: "test tool filtering" },
  state: {
    session: {
      handle: 2 as SessionHandle,
      sessionId,
      agentId,
      profileId: "tool-filter-profile" as ProfileId,
      kind: "delegated",
      resourceLimits: {},
      toolProfile: {
        tools: [
          {
            name: "read_file",
            description: "Read files visible to this delegated profile",
          },
        ],
      },
      status: "idle",
      brainTurnCount: 0,
      createdAt: "2026-06-19T00:00:00Z",
      lastActiveAt: "2026-06-19T00:00:00Z",
    },
    pendingMessages: [],
    recentEvents: [],
    childCompletions: [],
    fanOutGroups: [],
    deltaPolicy: defaultBodyDeltaPolicy,
  },
});

assert.deepEqual(capturedToolNames, ["read_file"]);

const freshDrain = queue.drainForNextWake(sessionId, 1_010);
assert.deepEqual(
  freshDrain.messages.map((message) => message.body),
  ["arrived while active"],
);
assert.equal(freshDrain.droppedExpired, 0);

queue.enqueue({
  sessionId,
  activeWakeId: "wake-2",
  message: queuedMessage,
  nowMs: 2_000,
});
const expiredDrain = queue.drainForNextWake(sessionId, 2_026);
assert.deepEqual(expiredDrain.messages, []);
assert.equal(expiredDrain.droppedExpired, 1);

console.log(
  JSON.stringify(
    {
      currentWakePrompt: fakeAgent.promptedText(),
      freshNextWakeMessages: freshDrain.messages.length,
      expiredDropped: expiredDrain.droppedExpired,
      clearAllQueuesCalls: fakeAgent.clearAllQueuesCalls,
      filteredTools: capturedToolNames,
    },
    null,
    2,
  ),
);

function fakeTool(name: string): PiAgentTool {
  return {
    name,
    description: `${name} description`,
    label: name,
    parameters: {} as PiAgentTool["parameters"],
    execute: async () => ({
      content: [{ type: "text", text: `${name} result` }],
      details: {},
    }),
  };
}
