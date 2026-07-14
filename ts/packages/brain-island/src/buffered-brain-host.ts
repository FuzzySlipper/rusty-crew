import type {
  BrainAction,
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
} from "./tool-execution-host.js";

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
  transportMetrics?:
    | OpenAiResponsesTransportMetrics
    | ChatCompletionsTransportMetrics;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
  brainEventCounts: Record<string, number>;
  brainStreamItemCounts: Record<string, number>;
}

export async function runBufferedBrainHost(options: {
  bridge: NativeBridgeModule;
  run: BufferedBrainProviderRun;
  moduleLabel: string;
  wake: BrainWakeInput;
  wakeOptions?: BrainWakeOptions;
  toolResolver?: BrainToolResolver;
  toolProfile: ToolProfile;
  toolCallDebugStore?: ToolCallDebugStore;
  events?: BrainEventEnvelope[];
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  planActions?: BrainActionPlanner;
}): Promise<BufferedBrainHostRunResult> {
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
  const toolDebugReferences = createBrainHostToolDebugReferences();
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
        await projectStreamItem(
          options.bridge,
          debugItem,
          streamActions,
          options.moduleLabel,
          options.submitEvent,
        );
      }
      for (const request of preparedToolRequests) {
        const output = await executePreparedBrainHostToolRequest(
          options.wake,
          request,
          options.toolCallDebugStore,
        );
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
        throw new Error(
          `${options.moduleLabel} buffered wake ${started.wakeId} failed: ${drained.error}`,
        );
      }
      if (drained.terminal) {
        const plannedActions = options.planActions
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
          transportMetrics: drained.transportMetrics,
          credentialSecretUpdate: drained.credentialSecretUpdate,
          brainEventCounts,
          brainStreamItemCounts,
        };
      }
      await delay(25);
    }
  } finally {
    options.wakeOptions?.signal?.removeEventListener("abort", abortListener);
  }
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
): Promise<void> {
  switch (item.type) {
    case "event":
      await submitEvent(item.event);
      return;
    case "actions":
      actions.push(...item.batch.actions);
      return;
    case "wake_failed":
      throw new Error(
        `${moduleLabel} wake ${item.failure.wakeId} failed: ${item.failure.message}`,
      );
  }
}

function incrementCount(counts: Record<string, number>, key: string): void {
  counts[key] = (counts[key] ?? 0) + 1;
}

async function delay(delayMs: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, delayMs));
}
