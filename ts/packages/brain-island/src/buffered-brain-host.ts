import type {
  BrainAction,
  BrainContinuationPayload,
  BrainEventEnvelope,
  BrainWakeProviderStateOutput,
  BrainWakeStreamItem,
  ToolProfile,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  OpenAiResponsesBrainRunInput,
  OpenAiResponsesCredentialSecretUpdate,
  OpenAiResponsesTransportMetrics,
  ChatCompletionsBrainRunInput,
  ChatCompletionsTransportMetrics,
  NativeBufferedBrainStreamRetentionMetrics,
} from "@rusty-crew/native-bridge";
import type {
  BrainActionPlanner,
  BrainWakeInput,
  BrainWakeOptions,
} from "./index.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import {
  resolveToolSession,
  type BrainActionCollector,
  type BrainToolResolver,
} from "./tool-session-selection.js";
import {
  addPreparedBrainHostToolDebugReferences,
  createBrainHostToolDebugReferences,
  executePreparedBrainHostToolRequest,
  prepareBrainHostToolRequest,
  withBrainHostToolDebugReference,
  type PreparedBrainHostToolRequest,
} from "./tool-execution-host.js";
import type { BrainToolMediaSink } from "./brain-tool-media.js";
import type { BrainToolResult } from "./brain-tool.js";

export type BufferedBrainProviderRun =
  | {
      moduleId: "chat-completions";
      providerInput: ChatCompletionsBrainRunInput;
    }
  | {
      moduleId: "openai-responses";
      providerInput: OpenAiResponsesBrainRunInput;
    };

export interface BufferedBrainHostRunResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  outcome: "completed" | "yielded";
  continuationState?: BrainContinuationPayload;
  transportMetrics?:
    | OpenAiResponsesTransportMetrics
    | ChatCompletionsTransportMetrics;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
  brainEventCounts: Record<string, number>;
  brainStreamItemCounts: Record<string, number>;
  streamRetentionMetrics: NativeBufferedBrainStreamRetentionMetrics;
}

export class BufferedBrainWakeError extends Error {
  constructor(
    readonly reasonCode: string,
    readonly source: string,
    message: string,
    readonly transportMetrics?:
      | OpenAiResponsesTransportMetrics
      | ChatCompletionsTransportMetrics,
    readonly brainEventCounts?: Record<string, number>,
    readonly brainStreamItemCounts?: Record<string, number>,
    readonly streamRetentionMetrics?: NativeBufferedBrainStreamRetentionMetrics,
  ) {
    super(message);
    this.name = "BufferedBrainWakeError";
  }
}

export async function runBufferedBrainHost(options: {
  bridge: NativeBridgeModule;
  run: BufferedBrainProviderRun;
  moduleLabel: string;
  wake: BrainWakeInput;
  wakeOptions?: BrainWakeOptions;
  toolResolver?: BrainToolResolver;
  prepareToolResolution?: () => Promise<void>;
  toolProfile: ToolProfile;
  toolCallDebugStore?: ToolCallDebugStore;
  toolMediaSink?: BrainToolMediaSink;
  events?: BrainEventEnvelope[];
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  planActions?: BrainActionPlanner;
}): Promise<BufferedBrainHostRunResult> {
  await options.prepareToolResolution?.();
  const selectionActions = createBrainActionCollector();
  const toolSelection = resolveToolSession({
    wake: options.wake,
    resolveTools: options.toolResolver,
    toolProfile: options.toolProfile,
    actions: selectionActions,
  });
  const toolsByName = new Map(
    toolSelection.tools.map((tool) => [tool.name, tool]),
  );
  const tools = toolSelection.tools.map((tool) => ({
    name: tool.name,
    description: tool.description,
    inputSchema: tool.parameters,
  }));
  const started = await options.bridge.startBrainRun(
    options.run.moduleId === "chat-completions"
      ? {
          moduleId: "chat-completions",
          providerInput: { ...options.run.providerInput, tools },
        }
      : {
          moduleId: "openai-responses",
          providerInput: { ...options.run.providerInput, tools },
        },
  );
  let cancelRequested = false;
  const cancelBufferedWake = async (
    reasonCode = "wake_timeout",
    summary = `wake ${started.wakeId} was cancelled by service wake timeout policy`,
  ) => {
    if (cancelRequested) return;
    cancelRequested = true;
    await options.bridge.cancelBrainRun({
      moduleId: options.run.moduleId,
      wakeId: started.wakeId,
      reasonCode,
      summary,
    });
  };
  const abortListener = () => {
    void cancelBufferedWake().catch(() => {
      // A terminal drain may already have removed the buffered run.
    });
  };
  options.wakeOptions?.signal?.addEventListener("abort", abortListener, {
    once: true,
  });

  const events = options.events ?? [];
  const streamActions: BrainAction[] = [];
  const brainEventCounts: Record<string, number> = {};
  const brainStreamItemCounts: Record<string, number> = {};
  let streamRetentionMetrics:
    | NativeBufferedBrainStreamRetentionMetrics
    | undefined;
  let streamFailure:
    | { reasonCode: string; source: string; message: string }
    | undefined;
  const toolDebugReferences = createBrainHostToolDebugReferences();
  let toolUpdateProjection = Promise.resolve();
  try {
    for (;;) {
      if (options.wakeOptions?.signal?.aborted) {
        await cancelBufferedWake();
      }
      const drained = await options.bridge.drainBrainRun({
        moduleId: options.run.moduleId,
        wakeId: started.wakeId,
        maxItems: 32,
      });
      streamRetentionMetrics = drained.streamRetentionMetrics;
      const preparedToolRequests = drained.toolRequests.map((request) =>
        prepareBrainHostToolRequest(
          options.wake,
          request,
          toolsByName,
          options.toolCallDebugStore,
        ),
      );
      addPreparedBrainHostToolDebugReferences(
        toolDebugReferences,
        preparedToolRequests,
      );
      for (const item of drained.items) {
        incrementCount(brainStreamItemCounts, item.type);
        if (item.type === "event") {
          incrementCount(brainEventCounts, item.event.event.type);
        }
        const debugItem = withBrainHostToolDebugReference(
          item,
          toolDebugReferences,
        );
        if (options.submitEvent === undefined && debugItem.type === "event") {
          events.push(debugItem.event);
          continue;
        }
        const failure = await projectStreamItem(
          options.bridge,
          debugItem,
          streamActions,
          options.moduleLabel,
          options.submitEvent,
        );
        streamFailure ??= failure;
      }
      for (const request of preparedToolRequests) {
        const output = await executePreparedBrainHostToolRequest(
          options.wake,
          request,
          options.toolCallDebugStore,
          options.toolMediaSink,
          (partialResult) => {
            toolUpdateProjection = toolUpdateProjection.then(() =>
              projectToolUpdate(options, request, partialResult, events),
            );
          },
          options.wakeOptions?.signal,
        );
        await toolUpdateProjection;
        await options.bridge.submitBrainHostResult({
          moduleId: options.run.moduleId,
          wakeId: started.wakeId,
          callId: request.request.callId,
          output: output.output,
          status:
            output.failure?.action === "denied"
              ? "denied"
              : output.failure
                ? "failed"
                : "succeeded",
          retryable: output.failure?.retryable ?? false,
          ...(output.failure === undefined
            ? {}
            : { reasonCode: output.failure.reasonCode }),
          ...(output.failure?.action === undefined
            ? {}
            : { action: output.failure.action }),
          ...(output.failure === undefined
            ? {}
            : { summary: output.failure.detail }),
          ...(request.debugDetailId === undefined
            ? {}
            : { debugDetailId: request.debugDetailId }),
        });
        if (output.suspend === true) {
          await cancelBufferedWake(
            "external_gate_wait",
            `wake ${started.wakeId} suspended until its external GitHub gate is terminal`,
          );
          break;
        }
      }
      if (drained.error !== undefined) {
        throw new BufferedBrainWakeError(
          streamFailure?.reasonCode ??
            drained.terminalReasonCode ??
            "provider_error",
          streamFailure?.source ?? "native_brain_terminal",
          streamFailure?.message ??
            `${options.moduleLabel} buffered wake ${started.wakeId} failed: ${drained.error}`,
          drained.transportMetrics,
          brainEventCounts,
          brainStreamItemCounts,
          streamRetentionMetrics,
        );
      }
      if (drained.terminal) {
        if (streamFailure !== undefined) {
          throw new BufferedBrainWakeError(
            streamFailure.reasonCode,
            streamFailure.source,
            streamFailure.message,
            drained.transportMetrics,
            brainEventCounts,
            brainStreamItemCounts,
            streamRetentionMetrics,
          );
        }
        const plannedActions =
          !drained.yielded && options.planActions
            ? await options.planActions({
                wake: options.wake,
                events,
                toolActions: [...selectionActions.actions, ...streamActions],
              })
            : [];
        return {
          events: options.submitEvent ? [] : events,
          actions: [
            ...selectionActions.actions,
            ...streamActions,
            ...plannedActions,
          ],
          providerState: drained.providerState,
          outcome: drained.yielded ? "yielded" : "completed",
          continuationState: drained.continuationState,
          transportMetrics: drained.transportMetrics,
          credentialSecretUpdate: drained.credentialSecretUpdate,
          brainEventCounts,
          brainStreamItemCounts,
          streamRetentionMetrics,
        };
      }
      await delay(25);
    }
  } finally {
    options.wakeOptions?.signal?.removeEventListener("abort", abortListener);
  }
}

async function projectToolUpdate(
  options: Parameters<typeof runBufferedBrainHost>[0],
  request: PreparedBrainHostToolRequest,
  partialResult: BrainToolResult,
  events: BrainEventEnvelope[],
): Promise<void> {
  const details = recordValue(partialResult.details);
  const status = stringValue(details.status) ?? "update";
  const message =
    partialResult.content
      .filter(
        (
          item,
        ): item is Extract<
          (typeof partialResult.content)[number],
          { type: "text" }
        > => item.type === "text",
      )
      .map((item) => item.text)
      .join("\n")
      .trim()
      .slice(0, 2_000) || `Tool ${request.request.name} reported ${status}.`;
  const event: BrainEventEnvelope = {
    wakeId: options.wake.wakeId,
    sessionId: options.wake.sessionId,
    event: {
      type: "provider_status",
      level: status === "failed" ? "error" : "info",
      message,
      metadataJson: JSON.stringify({
        source: "tool_status",
        tool_name: request.request.name,
        tool_call_id: request.request.callId,
        status,
        provider_id: stringValue(details.providerId),
        provider_job_id: stringValue(details.jobId),
      }),
    },
  };
  if (options.submitEvent) {
    await options.submitEvent(event);
  } else {
    events.push(event);
  }
}

function recordValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
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

async function projectStreamItem(
  bridge: NativeBridgeModule,
  item: BrainWakeStreamItem,
  actions: BrainAction[],
  moduleLabel: string,
  submitEvent: (event: BrainEventEnvelope) => Promise<void> = async (event) => {
    await bridge.submitBrainEvent(event);
  },
): Promise<
  { reasonCode: string; source: string; message: string } | undefined
> {
  switch (item.type) {
    case "event":
      await submitEvent(item.event);
      return undefined;
    case "actions":
      actions.push(...item.batch.actions);
      return undefined;
    case "wake_failed":
      return {
        reasonCode: item.failure.reasonCode ?? "brain_unavailable",
        source: "native_brain_stream",
        message: `${moduleLabel} wake ${item.failure.wakeId} failed: ${item.failure.message}`,
      };
  }
}

function incrementCount(counts: Record<string, number>, key: string): void {
  counts[key] = (counts[key] ?? 0) + 1;
}

async function delay(delayMs: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, delayMs));
}
