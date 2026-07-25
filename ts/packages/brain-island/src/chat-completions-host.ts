import type {
  BrainAction,
  BrainEventEnvelope,
  BrainWakeProviderStateOutput,
  ToolProfile,
} from "@rusty-crew/contracts";
import type {
  ChatCompletionsBrainRunInput,
  ChatCompletionsTransportMetrics,
} from "@rusty-crew/native-bridge";
import type {
  BrainActionPlanner,
  BrainHostExecutor,
  BrainWakeInput,
  BrainWakeOptions,
  BrainWakeResult,
} from "./index.js";
import {
  createRoleplayNarratorBrain,
  type RoleplayNarratorPhaseBrainOptions,
} from "./narrator-brain.js";
import { createRoleplayNarratorFsmBridge } from "./roleplay-narrator-fsm.js";
import type { BrainToolResolver } from "./tool-session-selection.js";
import type { BrainHostContext } from "./brain-host-context.js";
import { brainWakeTimeoutMs } from "./brain-host-timeout.js";
import { providerRequestDebugEvent } from "./provider-debug-projection.js";
import { providerRequestTimeoutMs } from "./provider-request-timeout.js";
import { runBufferedBrainHost } from "./buffered-brain-host.js";
import { chatCompletionsMaxToolRounds } from "./chat-completions-continuation-policy.js";

export function createChatCompletionsBrainHost(
  context: BrainHostContext,
  client: RustChatCompletionsClientConfig = rustChatCompletionsClientConfig(
    context,
  ),
): BrainHostExecutor {
  if (
    context.profile.profile.brain?.strategy === "roleplay_narrator" ||
    context.profile.profile.roleplayNarrator !== undefined
  ) {
    if (!context.bridge) {
      throw new Error("roleplay narrator Rust FSM bridge is required");
    }
    return createRoleplayNarratorBrain({
      narratorFsm: createRoleplayNarratorFsmBridge(context.bridge),
      createPhaseBrain: (phase: RoleplayNarratorPhaseBrainOptions) =>
        createRustChatCompletionsBrainHostExecutor(context, {
          moduleLabel: "chat-completions",
          client,
          toolResolver: phase.resolveTools,
          toolProfile: phase.toolProfile,
          submitEvent: phase.submitEvent,
          liveEvents: phase.submitEvent !== undefined,
          planActions: phase.planActions,
        }),
      planActions: context.planActions,
      resolveTools: context.toolResolver,
      toolProfile: context.profile.toolSelection.toolProfile,
      toolCallDebugStore: context.toolCallDebugStore,
      providerRequestDebugStore: context.providerRequestDebugStore,
      maxReviewCycles:
        context.profile.profile.roleplayNarrator?.review.maxReviewCycles,
      reviewEnabled: context.profile.profile.roleplayNarrator?.review.enabled,
      narratorConfig: context.profile.profile.roleplayNarrator,
      submitEvent: async (event) => {
        await context.bridge?.submitBrainEvent(event);
      },
      persistDiagnostic: async (input) => {
        if (!context.bridge) return;
        const current = (await context.bridge.getRoleplaySessionMetadata(
          input.sessionId,
        )) as
          | {
              profileId: string;
              revision: number;
              [key: string]: unknown;
            }
          | undefined;
        if (!current) return;
        if (current.profileId !== input.profileId) {
          throw new Error(
            `roleplay narrator session ${input.sessionId} belongs to profile ${current.profileId}, not ${input.profileId}`,
          );
        }
        const now = new Date().toISOString();
        await context.bridge.putRoleplaySessionMetadata({
          record: {
            ...current,
            narratorDiagnostic: {
              wakeId: input.wakeId,
              sceneBrief: input.sceneBrief,
              relevantLoreRecordIds: input.relevantLoreRecordIds,
              updatedAt: now,
            },
            updatedAt: now,
          },
          expected_revision: current.revision,
        });
      },
    });
  }
  return createRustChatCompletionsBrainHostExecutor(context, {
    moduleLabel: "chat-completions",
    client,
    planActions: context.planActions,
  });
}

export type RustChatCompletionsClientConfig = NonNullable<
  ChatCompletionsBrainRunInput["client"]
>;

function rustChatCompletionsClientConfig(
  context: BrainHostContext,
): RustChatCompletionsClientConfig {
  const baseUrl = context.profile.profile.modelConfig.baseUrl;
  if (!baseUrl) {
    throw new Error(
      "rust-chat-completions live client requires modelConfig.baseUrl",
    );
  }
  const keyEnv = context.profile.profile.modelConfig.apiKeyEnv;
  const apiKey = keyEnv ? process.env[keyEnv] : undefined;
  return {
    mode: "live",
    baseUrl,
    ...(apiKey ? { apiKey } : {}),
  };
}

interface RustChatCompletionsBrainImplementationOptions {
  moduleLabel: string;
  client: RustChatCompletionsClientConfig;
  toolResolver?: BrainToolResolver;
  toolProfile?: ToolProfile;
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  liveEvents?: boolean;
  planActions?: BrainActionPlanner;
}

function createRustChatCompletionsBrainHostExecutor(
  context: BrainHostContext,
  implementation: RustChatCompletionsBrainImplementationOptions,
): BrainHostExecutor {
  const client = implementation.client;
  return {
    async wake(wake, options): Promise<BrainWakeResult> {
      const bridge = context.bridge;
      if (bridge === undefined) {
        throw new Error(
          `${implementation.moduleLabel} brain requires native bridge`,
        );
      }
      const requestTimeoutMs = providerRequestTimeoutMs("chat-completions");
      const submitEvent =
        implementation.liveEvents === false
          ? undefined
          : (implementation.submitEvent ??
            (async (event: BrainEventEnvelope) => {
              await bridge.submitBrainEvent(event);
            }));
      const input: ChatCompletionsBrainRunInput = {
        wakeId: wake.wakeId,
        sessionId: wake.sessionId,
        messages: rustChatCompletionsMessages(wake),
        providerState: wake.providerState,
        config: {
          model: context.profile.profile.modelConfig.modelName,
          ...(requestTimeoutMs === undefined
            ? {}
            : { providerRequestTimeoutMs: requestTimeoutMs }),
          wakeTimeoutMs: brainWakeTimeoutMs(context, wake),
          temperatureMilli:
            context.profile.profile.modelConfig.temperatureMilli,
          reasoningEffort:
            wake.state.session.inferenceOverrides?.reasoningEffort ??
            context.profile.profile.modelConfig.reasoningEffort,
          wireDialect:
            context.profile.profile.modelConfig.chatCompletionsDialect ??
            "standard",
          thinkingMode:
            context.profile.profile.modelConfig.thinkingMode ??
            "provider_default",
          reasoningHistory:
            context.profile.profile.modelConfig.reasoningHistory ??
            "provider_default",
          reasoningBudgetTokens:
            context.profile.profile.modelConfig.reasoningBudgetTokens,
          providerStateStrategyId:
            context.profile.profile.brain?.strategy ??
            (context.profile.profile.roleplayNarrator
              ? "roleplay_narrator"
              : "default"),
          maxOutputTokens:
            context.profile.profile.modelConfig.maxOutputTokens ??
            context.maxTokens,
          maxToolRounds: chatCompletionsMaxToolRounds(),
        },
        client,
      };
      const providerDebug = context.providerRequestDebugStore?.record({
        sessionId: wake.sessionId,
        wakeId: wake.wakeId,
        brainModule: implementation.moduleLabel,
        providerAlias: context.profile.profile.providerAlias,
        model: input.config.model,
        protocol: "chat_completions",
        providerKind: context.profile.profile.modelConfig.provider,
        request: {
          boundary: "ts_to_native_rust_chat_completions",
          wakeId: input.wakeId,
          sessionId: input.sessionId,
          messages: input.messages,
          config: input.config,
          client: input.client,
        },
      });
      const events: BrainEventEnvelope[] = [];
      if (providerDebug) {
        const event = providerRequestDebugEvent(wake, providerDebug);
        if (submitEvent) {
          await submitEvent(event);
        } else {
          events.push(event);
        }
      }
      const result = await runRustChatCompletionsBrainWithIncrementalDrain(
        context,
        wake,
        input,
        options,
        {
          moduleLabel: implementation.moduleLabel,
          events,
          submitEvent,
          toolResolver: implementation.toolResolver,
          toolProfile: implementation.toolProfile,
          planActions: implementation.planActions,
        },
      );
      const exactDebug = recordChatCompletionsProviderRequestSamples(
        context,
        wake,
        input.config.model,
        result.transportMetrics?.providerRequestDebugSamples,
      );
      if (exactDebug && submitEvent) {
        await submitEvent(providerRequestDebugEvent(wake, exactDebug));
      } else if (exactDebug) {
        result.events.push(providerRequestDebugEvent(wake, exactDebug));
      }
      return withChatCompletionsProviderStateScope(result, context);
    },
  };
}

function withChatCompletionsProviderStateScope<
  T extends { providerState?: BrainWakeProviderStateOutput },
>(result: T, context: BrainHostContext): T {
  if (
    result.providerState?.type !== "replace" ||
    context.providerStateScope === undefined
  ) {
    return result;
  }
  return {
    ...result,
    providerState: {
      type: "replace",
      state: {
        ...result.providerState.state,
        profileFingerprint:
          result.providerState.state.profileFingerprint ===
          "profile-fingerprint"
            ? context.providerStateScope.profileFingerprint
            : result.providerState.state.profileFingerprint,
        providerFingerprint:
          result.providerState.state.providerFingerprint ===
          "provider-fingerprint"
            ? context.providerStateScope.providerFingerprint
            : result.providerState.state.providerFingerprint,
      },
    },
  };
}

function recordChatCompletionsProviderRequestSamples(
  context: BrainHostContext,
  wake: BrainWakeInput,
  model: string,
  samples: unknown[] | undefined,
):
  | {
      debug_detail_id: string;
      request_sha256: string;
      request_json_chars: number;
      expires_at: string;
    }
  | undefined {
  if (!samples || samples.length === 0) return undefined;
  return context.providerRequestDebugStore?.record({
    sessionId: wake.sessionId,
    wakeId: wake.wakeId,
    brainModule: "chat-completions",
    providerAlias: context.profile.profile.providerAlias,
    model,
    protocol: "chat_completions",
    providerKind: context.profile.profile.modelConfig.provider,
    request: {
      boundary: "rust_chat_completions_request",
      requestCount: samples.length,
      requests: samples,
    },
  });
}

function rustChatCompletionsMessages(
  wake: BrainWakeInput,
): ChatCompletionsBrainRunInput["messages"] {
  const system = [wake.systemPrompt, wake.roleAssembly.instructions]
    .filter(Boolean)
    .join("\n\n");
  const initialMessages =
    wake.providerState === undefined
      ? (wake.roleAssembly.initialMessages ?? [])
      : [];
  return [
    ...(system ? [{ role: "system" as const, content: system }] : []),
    ...initialMessages.map((message) => ({
      role: "user" as const,
      content: message.body,
    })),
    ...wake.state.pendingMessages.map((message) => ({
      role: "user" as const,
      content: message.body,
    })),
  ];
}

async function runRustChatCompletionsBrainWithIncrementalDrain(
  context: BrainHostContext,
  wake: BrainWakeInput,
  input: ChatCompletionsBrainRunInput,
  options?: BrainWakeOptions,
  runOptions: {
    moduleLabel: string;
    events: BrainEventEnvelope[];
    submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
    toolResolver?: BrainToolResolver;
    toolProfile?: ToolProfile;
    planActions?: BrainActionPlanner;
  } = {
    moduleLabel: "chat-completions",
    events: [],
  },
): Promise<{
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  transportMetrics?: ChatCompletionsTransportMetrics;
  brainEventCounts?: Record<string, number>;
  brainStreamItemCounts?: Record<string, number>;
  streamRetentionMetrics?: import("@rusty-crew/native-bridge").NativeBufferedBrainStreamRetentionMetrics;
}> {
  const bridge = context.bridge;
  if (bridge === undefined) {
    throw new Error("chat-completions host requires native bridge");
  }
  const result = await runBufferedBrainHost({
    bridge,
    run: { moduleId: "chat-completions", providerInput: input },
    moduleLabel: runOptions.moduleLabel,
    wake,
    wakeOptions: options,
    toolResolver: runOptions.toolResolver ?? context.toolResolver,
    prepareToolResolution: context.prepareToolResolution,
    toolProfile:
      runOptions.toolProfile ?? context.profile.toolSelection.toolProfile,
    toolCallDebugStore: context.toolCallDebugStore,
    toolMediaSink: context.toolMediaSink,
    events: runOptions.events,
    submitEvent: runOptions.submitEvent,
    planActions: runOptions.planActions,
  });
  return {
    ...result,
    transportMetrics: result.transportMetrics as
      | ChatCompletionsTransportMetrics
      | undefined,
  };
}
