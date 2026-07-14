import assert from "node:assert/strict";
import type {
  AgentEvent as ChatCompletionsEvent,
  AgentMessage as ChatCompletionsMessage,
  AgentOptions as ChatCompletionsOptions,
} from "./support/chat-completions-test-harness.js";
import type {
  AgentId,
  BrainEventEnvelope,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { createChatCompletionsBrain } from "./support/chat-completions-test-harness.js";

const sessionId = "chat-completions-brain-events-session" as SessionId;

function assistantMessage(content: unknown): ChatCompletionsMessage {
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
  } as ChatCompletionsMessage;
}

class FinalMessageOnlyAgent {
  private listener?: (event: ChatCompletionsEvent, signal: AbortSignal) => void;

  constructor(
    private readonly finalMessage:
      | { kind: "text"; text: string }
      | { kind: "error"; errorMessage: string },
  ) {}

  subscribe(
    listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as ChatCompletionsEvent, signal);
    this.listener?.(
      {
        type: "message_end",
        message: this.assistantMessage(),
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as ChatCompletionsEvent,
      signal,
    );
  }

  private assistantMessage(): ChatCompletionsMessage {
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
    } as ChatCompletionsMessage;
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

class StreamingThenFinalMessageAgent {
  private listener?: (event: ChatCompletionsEvent, signal: AbortSignal) => void;

  subscribe(
    listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as ChatCompletionsEvent, signal);
    this.listener?.(
      {
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: "streamed " },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: "answer" },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_end",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "streamed answer" }],
        },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as ChatCompletionsEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

class ControlledLiveSubmitAgent {
  private listener?: (event: ChatCompletionsEvent, signal: AbortSignal) => void;
  private readonly idle = deferred<void>();

  subscribe(
    listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as ChatCompletionsEvent, signal);
    this.listener?.(
      {
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: "early" },
      } as ChatCompletionsEvent,
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
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as ChatCompletionsEvent,
      signal,
    );
  }

  releaseIdle(): void {
    this.idle.resolve();
  }

  clearAllQueues(): void {}
}

const textBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new FinalMessageOnlyAgent({
      kind: "text",
      text: "final message text without streaming deltas",
    }),
});

const textResult = await wake(textBrain, "chat-completions-brain-events-wake");

assert.deepEqual(
  textResult.events.map((event) => event.event.type),
  ["started", "text_delta", "finished"],
);
const textDelta = textDeltaText(textResult);
assert.equal(textDelta, "final message text without streaming deltas");

const streamedBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new StreamingThenFinalMessageAgent(),
});

const streamedResult = await wake(
  streamedBrain,
  "chat-completions-brain-streamed-events-wake",
);

assert.deepEqual(
  streamedResult.events.map((event) => event.event.type),
  ["started", "text_delta", "text_delta", "finished"],
);
assert.deepEqual(textDeltaTexts(streamedResult), ["streamed ", "answer"]);

const liveSubmitAgent = new ControlledLiveSubmitAgent();
const liveSubmittedEvents: BrainEventEnvelope[] = [];
const liveSubmitBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) => liveSubmitAgent,
  submitEvent: async (event) => {
    liveSubmittedEvents.push(event);
  },
});
let liveSubmitWakeSettled = false;
const liveSubmitWake = wake(
  liveSubmitBrain,
  "chat-completions-brain-live-submit-events-wake",
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

const reasoningFinalBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new FinalMessageOnlyAgent({
      kind: "text",
      text: "<think>private chain</think>visible answer",
    }),
});

const reasoningFinalResult = await wake(
  reasoningFinalBrain,
  "chat-completions-brain-reasoning-final-events-wake",
);

assert.deepEqual(
  reasoningFinalResult.events.map((event) => event.event.type),
  ["started", "reasoning_delta", "text_delta", "finished"],
);
assert.deepEqual(reasoningDeltaTexts(reasoningFinalResult), ["private chain"]);
assert.deepEqual(textDeltaTexts(reasoningFinalResult), ["visible answer"]);

class StreamingThinkMessageAgent {
  private listener?: (event: ChatCompletionsEvent, signal: AbortSignal) => void;

  subscribe(
    listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as ChatCompletionsEvent, signal);
    this.listener?.(
      {
        type: "message_update",
        assistantMessageEvent: {
          type: "text_delta",
          delta: "before <think>stream thought</think> after",
        },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_end",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "before  after" }],
        },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as ChatCompletionsEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const reasoningStreamedBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new StreamingThinkMessageAgent(),
});

const reasoningStreamedResult = await wake(
  reasoningStreamedBrain,
  "chat-completions-brain-reasoning-streamed-events-wake",
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
  private listener?: (event: ChatCompletionsEvent, signal: AbortSignal) => void;

  subscribe(
    listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as ChatCompletionsEvent, signal);
    this.listener?.(
      {
        type: "message_update",
        message: assistantMessage([
          { type: "thinking", thinking: "native chat completions thinking " },
        ]),
        assistantMessageEvent: {
          type: "thinking_delta",
          contentIndex: 0,
          delta: "native chat completions thinking ",
          partial: assistantMessage([
            { type: "thinking", thinking: "native chat completions thinking " },
          ]),
        },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_update",
        message: assistantMessage([
          {
            type: "thinking",
            thinking: "native chat completions thinking continued",
          },
        ]),
        assistantMessageEvent: {
          type: "thinking_delta",
          contentIndex: 0,
          delta: "continued",
          partial: assistantMessage([
            {
              type: "thinking",
              thinking: "native chat completions thinking continued",
            },
          ]),
        },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_update",
        message: assistantMessage([
          {
            type: "thinking",
            thinking: "native chat completions thinking continued",
          },
          { type: "text", text: "visible native answer" },
        ]),
        assistantMessageEvent: {
          type: "text_delta",
          contentIndex: 1,
          delta: "visible native answer",
          partial: assistantMessage([
            {
              type: "thinking",
              thinking: "native chat completions thinking continued",
            },
            { type: "text", text: "visible native answer" },
          ]),
        },
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      {
        type: "message_end",
        message: assistantMessage([
          {
            type: "thinking",
            thinking: "native chat completions thinking continued",
          },
          { type: "text", text: "visible native answer" },
        ]),
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as ChatCompletionsEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const nativeThinkingStreamedBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new StreamingPiThinkingMessageAgent(),
});

const nativeThinkingStreamedResult = await wake(
  nativeThinkingStreamedBrain,
  "chat-completions-brain-native-thinking-streamed-events-wake",
);

assert.deepEqual(
  nativeThinkingStreamedResult.events.map((event) => event.event.type),
  ["started", "reasoning_delta", "reasoning_delta", "text_delta", "finished"],
);
assert.deepEqual(reasoningDeltaTexts(nativeThinkingStreamedResult), [
  "native chat completions thinking ",
  "continued",
]);
assert.deepEqual(textDeltaTexts(nativeThinkingStreamedResult), [
  "visible native answer",
]);

const nativeThinkingFinalBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new FinalNativeThinkingMessageAgent(),
});

class FinalNativeThinkingMessageAgent {
  private listener?: (event: ChatCompletionsEvent, signal: AbortSignal) => void;

  subscribe(
    listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as ChatCompletionsEvent, signal);
    this.listener?.(
      {
        type: "message_end",
        message: assistantMessage([
          { type: "thinking", thinking: "final native thinking" },
          { type: "text", text: "final visible answer" },
        ]),
      } as ChatCompletionsEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as ChatCompletionsEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const nativeThinkingFinalResult = await wake(
  nativeThinkingFinalBrain,
  "chat-completions-brain-native-thinking-final-events-wake",
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

const errorBrain = createChatCompletionsBrain({
  createAgent: (_options: ChatCompletionsOptions) =>
    new FinalMessageOnlyAgent({
      kind: "error",
      errorMessage: "OpenAI API error (404): 404 status code (no body)",
    }),
});

const errorResult = await wake(
  errorBrain,
  "chat-completions-brain-error-events-wake",
);
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
  brain: ReturnType<typeof createChatCompletionsBrain>,
  wakeId: string,
) {
  return brain.wake({
    wakeId,
    sessionId,
    systemPrompt: "Map chat-completions events.",
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
  result: Awaited<
    ReturnType<ReturnType<typeof createChatCompletionsBrain>["wake"]>
  >,
): string | undefined {
  return textDeltaTexts(result)[0];
}

function textDeltaTexts(
  result: Awaited<
    ReturnType<ReturnType<typeof createChatCompletionsBrain>["wake"]>
  >,
): string[] {
  return result.events
    .map((event) => event.event)
    .flatMap((event) => (event.type === "text_delta" ? [event.text] : []));
}

function reasoningDeltaTexts(
  result: Awaited<
    ReturnType<ReturnType<typeof createChatCompletionsBrain>["wake"]>
  >,
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
