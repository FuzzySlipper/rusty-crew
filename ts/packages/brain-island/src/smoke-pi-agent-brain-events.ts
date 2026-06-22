import assert from "node:assert/strict";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { createPiAgentBrain } from "./pi-agent-brain.js";

const sessionId = "pi-agent-brain-events-session" as SessionId;

class FinalMessageOnlyAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(
    private readonly finalMessage:
      | { kind: "text"; text: string }
      | { kind: "error"; errorMessage: string },
  ) {}

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as PiAgentEvent, signal);
    this.listener?.(
      {
        type: "message_end",
        message: this.assistantMessage(),
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  private assistantMessage(): PiAgentMessage {
    return {
      role: "assistant",
      content:
        this.finalMessage.kind === "text"
          ? [{ type: "text", text: this.finalMessage.text }]
          : [],
      api: "openai-completions",
      provider: "den-router",
      model: "fake-model",
      usage: {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: {
          input: 0,
          output: 0,
          cacheRead: 0,
          cacheWrite: 0,
          total: 0,
        },
      },
      stopReason: this.finalMessage.kind === "error" ? "error" : "stop",
      errorMessage:
        this.finalMessage.kind === "error"
          ? this.finalMessage.errorMessage
          : undefined,
      timestamp: Date.now(),
    } as PiAgentMessage;
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const textBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) =>
    new FinalMessageOnlyAgent({
      kind: "text",
      text: "final message text without streaming deltas",
    }),
});

const textResult = await wake(textBrain, "pi-agent-brain-events-wake");

assert.deepEqual(
  textResult.events.map((event) => event.event.type),
  ["started", "text_delta", "finished"],
);
const textDelta = textDeltaText(textResult);
assert.equal(textDelta, "final message text without streaming deltas");

const errorBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) =>
    new FinalMessageOnlyAgent({
      kind: "error",
      errorMessage: "OpenAI API error (404): 404 status code (no body)",
    }),
});

const errorResult = await wake(errorBrain, "pi-agent-brain-error-events-wake");
assert.equal(
  textDeltaText(errorResult),
  "LLM error: OpenAI API error (404): 404 status code (no body)",
);

console.log(
  JSON.stringify(
    {
      eventTypes: textResult.events.map((event) => event.event.type),
      text: textDelta,
      errorText: textDeltaText(errorResult),
    },
    null,
    2,
  ),
);

async function wake(
  brain: ReturnType<typeof createPiAgentBrain>,
  wakeId: string,
) {
  return brain.wake({
    wakeId,
    sessionId,
    systemPrompt: "Map pi-agent events.",
    roleAssembly: {
      instructions: "Return final text.",
      initialMessages: [],
    },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "event-agent" as AgentId,
        profileId: "event-profile" as ProfileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: { tools: [] },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-22T00:00:00Z",
        lastActiveAt: "2026-06-22T00:00:00Z",
      },
      pendingMessages: [
        {
          from: "operator" as AgentId,
          to: "event-agent" as AgentId,
          body: "please reply",
        },
      ],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 5_000,
        maxQueuedMessages: 32,
      },
    },
  });
}

function textDeltaText(
  result: Awaited<ReturnType<ReturnType<typeof createPiAgentBrain>["wake"]>>,
): string | undefined {
  return result.events
    .map((event) => event.event)
    .find((event) => event.type === "text_delta")?.text;
}
