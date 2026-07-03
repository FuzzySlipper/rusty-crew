import assert from "node:assert/strict";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  BrainEventEnvelope,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { createPiAgentBrain } from "./pi-agent-brain.js";

const sessionId = "pi-agent-brain-events-session" as SessionId;

function assistantMessage(content: unknown): PiAgentMessage {
  return {
    role: "assistant",
    content,
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
    stopReason: "stop",
    timestamp: Date.now(),
  } as PiAgentMessage;
}

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

class StreamingThenFinalMessageAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

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
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: "streamed " },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: "answer" },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_end",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "streamed answer" }],
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

class ControlledLiveSubmitAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;
  private readonly idle = deferred<void>();

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
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: "early" },
      } as PiAgentEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {
    await this.idle.promise;
    const signal = new AbortController().signal;
    this.listener?.(
      {
        type: "message_end",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "early" }],
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  releaseIdle(): void {
    this.idle.resolve();
  }

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

const streamedBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) =>
    new StreamingThenFinalMessageAgent(),
});

const streamedResult = await wake(
  streamedBrain,
  "pi-agent-brain-streamed-events-wake",
);

assert.deepEqual(
  streamedResult.events.map((event) => event.event.type),
  ["started", "text_delta", "text_delta", "finished"],
);
assert.deepEqual(textDeltaTexts(streamedResult), ["streamed ", "answer"]);

const liveSubmitAgent = new ControlledLiveSubmitAgent();
const liveSubmittedEvents: BrainEventEnvelope[] = [];
const liveSubmitBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) => liveSubmitAgent,
  submitEvent: async (event) => {
    liveSubmittedEvents.push(event);
  },
});
let liveSubmitWakeSettled = false;
const liveSubmitWake = wake(
  liveSubmitBrain,
  "pi-agent-brain-live-submit-events-wake",
).finally(() => {
  liveSubmitWakeSettled = true;
});

await waitUntil(() =>
  liveSubmittedEvents.some((event) => event.event.type === "text_delta"),
);
assert.equal(
  liveSubmitWakeSettled,
  false,
  "live submit should expose events before waitForIdle resolves",
);
assert.deepEqual(
  liveSubmittedEvents.map((event) => event.event.type),
  ["started", "text_delta"],
);
liveSubmitAgent.releaseIdle();
const liveSubmitResult = await liveSubmitWake;
assert.deepEqual(liveSubmitResult.events, []);
assert.deepEqual(
  liveSubmittedEvents.map((event) => event.event.type),
  ["started", "text_delta", "finished"],
);

const reasoningFinalBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) =>
    new FinalMessageOnlyAgent({
      kind: "text",
      text: "<think>private chain</think>visible answer",
    }),
});

const reasoningFinalResult = await wake(
  reasoningFinalBrain,
  "pi-agent-brain-reasoning-final-events-wake",
);

assert.deepEqual(
  reasoningFinalResult.events.map((event) => event.event.type),
  ["started", "reasoning_delta", "text_delta", "finished"],
);
assert.deepEqual(reasoningDeltaTexts(reasoningFinalResult), ["private chain"]);
assert.deepEqual(textDeltaTexts(reasoningFinalResult), ["visible answer"]);

class StreamingThinkMessageAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

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
        type: "message_update",
        assistantMessageEvent: {
          type: "text_delta",
          delta: "before <think>stream thought</think> after",
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_end",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "before  after" }],
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const reasoningStreamedBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) => new StreamingThinkMessageAgent(),
});

const reasoningStreamedResult = await wake(
  reasoningStreamedBrain,
  "pi-agent-brain-reasoning-streamed-events-wake",
);

assert.deepEqual(
  reasoningStreamedResult.events.map((event) => event.event.type),
  ["started", "text_delta", "reasoning_delta", "text_delta", "finished"],
);
assert.deepEqual(reasoningDeltaTexts(reasoningStreamedResult), [
  "stream thought",
]);
assert.deepEqual(textDeltaTexts(reasoningStreamedResult), [
  "before ",
  " after",
]);

class StreamingPiThinkingMessageAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

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
        type: "message_update",
        message: assistantMessage([
          { type: "thinking", thinking: "native pi thinking " },
        ]),
        assistantMessageEvent: {
          type: "thinking_delta",
          contentIndex: 0,
          delta: "native pi thinking ",
          partial: assistantMessage([
            { type: "thinking", thinking: "native pi thinking " },
          ]),
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_update",
        message: assistantMessage([
          { type: "thinking", thinking: "native pi thinking continued" },
        ]),
        assistantMessageEvent: {
          type: "thinking_delta",
          contentIndex: 0,
          delta: "continued",
          partial: assistantMessage([
            { type: "thinking", thinking: "native pi thinking continued" },
          ]),
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_update",
        message: assistantMessage([
          { type: "thinking", thinking: "native pi thinking continued" },
          { type: "text", text: "visible native answer" },
        ]),
        assistantMessageEvent: {
          type: "text_delta",
          contentIndex: 1,
          delta: "visible native answer",
          partial: assistantMessage([
            { type: "thinking", thinking: "native pi thinking continued" },
            { type: "text", text: "visible native answer" },
          ]),
        },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_end",
        message: assistantMessage([
          { type: "thinking", thinking: "native pi thinking continued" },
          { type: "text", text: "visible native answer" },
        ]),
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const nativeThinkingStreamedBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) =>
    new StreamingPiThinkingMessageAgent(),
});

const nativeThinkingStreamedResult = await wake(
  nativeThinkingStreamedBrain,
  "pi-agent-brain-native-thinking-streamed-events-wake",
);

assert.deepEqual(
  nativeThinkingStreamedResult.events.map((event) => event.event.type),
  ["started", "reasoning_delta", "reasoning_delta", "text_delta", "finished"],
);
assert.deepEqual(reasoningDeltaTexts(nativeThinkingStreamedResult), [
  "native pi thinking ",
  "continued",
]);
assert.deepEqual(textDeltaTexts(nativeThinkingStreamedResult), [
  "visible native answer",
]);

const nativeThinkingFinalBrain = createPiAgentBrain({
  createAgent: (_options: PiAgentOptions) =>
    new FinalNativeThinkingMessageAgent(),
});

class FinalNativeThinkingMessageAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

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
        message: assistantMessage([
          { type: "thinking", thinking: "final native thinking" },
          { type: "text", text: "final visible answer" },
        ]),
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const nativeThinkingFinalResult = await wake(
  nativeThinkingFinalBrain,
  "pi-agent-brain-native-thinking-final-events-wake",
);

assert.deepEqual(
  nativeThinkingFinalResult.events.map((event) => event.event.type),
  ["started", "reasoning_delta", "text_delta", "finished"],
);
assert.deepEqual(reasoningDeltaTexts(nativeThinkingFinalResult), [
  "final native thinking",
]);
assert.deepEqual(textDeltaTexts(nativeThinkingFinalResult), [
  "final visible answer",
]);

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
      streamedTextParts: textDeltaTexts(streamedResult),
      reasoningText: reasoningDeltaTexts(reasoningFinalResult).join(""),
      nativeThinkingText: reasoningDeltaTexts(
        nativeThinkingStreamedResult,
      ).join(""),
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
  return textDeltaTexts(result)[0];
}

function textDeltaTexts(
  result: Awaited<ReturnType<ReturnType<typeof createPiAgentBrain>["wake"]>>,
): string[] {
  return result.events
    .map((event) => event.event)
    .flatMap((event) => (event.type === "text_delta" ? [event.text] : []));
}

function reasoningDeltaTexts(
  result: Awaited<ReturnType<ReturnType<typeof createPiAgentBrain>["wake"]>>,
): string[] {
  return result.events
    .map((event) => event.event)
    .flatMap((event) => (event.type === "reasoning_delta" ? [event.text] : []));
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T | PromiseLike<T>) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

async function waitUntil(
  predicate: () => boolean,
  timeoutMs = 1_000,
): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) {
      throw new Error("timed out waiting for condition");
    }
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}
