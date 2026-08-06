import type {
  BrainAction,
  BrainEventEnvelope,
  BrainWakeProviderStateOutput,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeModelProviderRecord,
  OpenAiResponsesCredentialSecretUpdate,
  OpenAiResponsesBrainRunInput,
  OpenAiResponsesTransportMetrics,
} from "@rusty-crew/native-bridge";
import type {
  BrainHostExecutor,
  BrainWakeInput,
  BrainWakeOptions,
} from "./index.js";
import { runBufferedBrainHost } from "./buffered-brain-host.js";
import type { BrainHostContext } from "./brain-host-context.js";
import { providerRequestDebugEvent } from "./provider-debug-projection.js";
import { providerRequestTimeoutMs } from "./provider-request-timeout.js";
import {
  responsesNoProgressAttentionThreshold,
  responsesWorkQuantumContinuationRounds,
} from "./responses-continuation-policy.js";

export type OpenAiResponsesClientConfig = NonNullable<
  OpenAiResponsesBrainRunInput["client"]
>;

async function openAiResponsesClientConfig(
  context: BrainHostContext,
): Promise<OpenAiResponsesClientConfig> {
  if (context.profile.profile.modelConfig.credentialKind === "openai_oauth") {
    const bridge = context.bridge;
    const providerAlias = context.profile.profile.providerAlias;
    if (bridge === undefined || providerAlias === undefined) {
      throw new Error(
        "openai-responses OAuth live client requires native bridge and providerAlias",
      );
    }
    const oauthCredentialSecret =
      await bridge.getModelProviderSecret(providerAlias);
    if (oauthCredentialSecret === undefined) {
      throw new Error(
        `openai-responses OAuth live client requested but provider ${providerAlias} has no credential secret`,
      );
    }
    return {
      mode: "live",
      baseUrl:
        context.profile.profile.modelConfig.baseUrl ??
        "https://chatgpt.com/backend-api/codex",
      authKind: "openai_oauth",
      providerAlias,
      oauthCredentialSecret,
    };
  }
  const keyEnv =
    context.profile.profile.modelConfig.apiKeyEnv ?? "OPENAI_API_KEY";
  const apiKey = process.env[keyEnv];
  if (!apiKey && process.env.RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY !== "1") {
    throw new Error(
      `openai-responses live client requested but ${keyEnv} is not set`,
    );
  }
  return {
    mode: "live",
    baseUrl:
      context.profile.profile.modelConfig.baseUrl ??
      "https://api.openai.com/v1",
    authKind: "api_key",
    ...(context.profile.profile.providerAlias === undefined
      ? {}
      : { providerAlias: context.profile.profile.providerAlias }),
    ...(apiKey ? { apiKey } : {}),
  };
}

async function persistOpenAiResponsesCredentialSecretUpdate(
  context: BrainHostContext,
  currentConfig: OpenAiResponsesClientConfig,
  update: OpenAiResponsesCredentialSecretUpdate | undefined,
): Promise<OpenAiResponsesClientConfig> {
  if (update === undefined) {
    return currentConfig;
  }
  const bridge = context.bridge;
  if (bridge === undefined) {
    throw new Error("OpenAI Responses credential update requires bridge");
  }
  const provider = await bridge.getModelProvider(update.providerAlias);
  if (provider === undefined) {
    throw new Error(
      `OpenAI Responses credential update provider ${update.providerAlias} was not found`,
    );
  }
  await bridge.upsertModelProvider({
    ...modelProviderWriteFromRecord(provider),
    secret: update.secret,
    expectedRevision: provider.revision,
    expectedCredentialRevision: provider.credential.revision,
    now: new Date().toISOString(),
  });
  if (
    currentConfig.mode === "live" &&
    currentConfig.authKind === "openai_oauth" &&
    currentConfig.providerAlias === update.providerAlias
  ) {
    return {
      ...currentConfig,
      oauthCredentialSecret: update.secret,
    };
  }
  return currentConfig;
}

function modelProviderWriteFromRecord(provider: NativeModelProviderRecord) {
  return {
    alias: provider.alias,
    status: provider.status,
    protocol: provider.protocol,
    providerKind: provider.providerKind,
    displayName: provider.displayName,
    description: provider.description,
    baseUrl: provider.baseUrl,
    modelId: provider.modelId,
    contextWindowTokens: provider.contextWindowTokens,
    maxOutputTokens: provider.maxOutputTokens,
    temperatureMilli: provider.temperatureMilli,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    responsesDialect: provider.responsesDialect,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    promptCaching: provider.promptCaching,
    clearSecret: false,
    expectedCredentialRevision: provider.credential.revision,
    metadataJson: provider.metadataJson,
  };
}

function withOpenAiResponsesProviderStateScope<
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

async function runOpenAiResponsesBrainWithIncrementalDrain(
  context: BrainHostContext,
  wake: BrainWakeInput,
  input: OpenAiResponsesBrainRunInput,
  options?: BrainWakeOptions,
): Promise<{
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  transportMetrics?: OpenAiResponsesTransportMetrics;
  brainEventCounts?: Record<string, number>;
  brainStreamItemCounts?: Record<string, number>;
  streamRetentionMetrics?: import("@rusty-crew/native-bridge").NativeBufferedBrainStreamRetentionMetrics;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
  outcome: "completed" | "yielded" | "attention_required";
  continuationState?: import("@rusty-crew/contracts").BrainContinuationPayload;
  attention?: import("@rusty-crew/contracts").BrainWakeAttention;
}> {
  const bridge = context.bridge;
  if (bridge === undefined) {
    throw new Error("OpenAI Responses host requires native bridge");
  }
  const result = await runBufferedBrainHost({
    bridge,
    run: { moduleId: "openai-responses", providerInput: input },
    moduleLabel: "OpenAI Responses",
    wake,
    wakeOptions: options,
    toolResolver: context.toolResolver,
    prepareToolResolution: context.prepareToolResolution,
    toolProfile: context.profile.toolSelection.toolProfile,
    toolCallDebugStore: context.toolCallDebugStore,
    toolMediaSink: context.toolMediaSink,
    submitEvent: async (event) => {
      await bridge.submitBrainEvent(event);
    },
  });
  return {
    ...result,
    transportMetrics: result.transportMetrics as
      | OpenAiResponsesTransportMetrics
      | undefined,
  };
}

export async function createOpenAiResponsesBrainHost(
  context: BrainHostContext,
  client?: OpenAiResponsesClientConfig,
  strategyId: "replay" | "previous-response-chain" = "replay",
): Promise<BrainHostExecutor> {
  const responsesDialect = context.profile.profile.modelConfig.responsesDialect;
  if (responsesDialect === undefined) {
    throw new Error(
      "Responses brain requires provider modelConfig.responsesDialect",
    );
  }
  if (
    strategyId === "previous-response-chain" &&
    responsesDialect !== "openai_stateful" && responsesDialect !== "meta"
  ) {
    throw new Error(
      `Responses dialect ${responsesDialect} does not support previous-response-chain`,
    );
  }
  let responsesClientConfig =
    client ?? (await openAiResponsesClientConfig(context));
  return {
    async wake(wake, options) {
      if (context.bridge === undefined) {
        throw new Error("openai-responses brain requires native bridge");
      }
      const requestTimeoutMs = providerRequestTimeoutMs("openai-responses");
      const input = {
        wakeId: wake.wakeId,
        sessionId: wake.sessionId,
        bodyState: wake.state,
        ...(wake.providerState === undefined && wake.durableConversation
          ? { durableConversation: wake.durableConversation }
          : {}),
        providerState: wake.providerState,
        providerStateAbsence: wake.providerStateAbsence,
        continuationState: wake.continuationState,
        config: {
          model: context.profile.profile.modelConfig.modelName,
          responsesDialect,
          strategyId,
          instructions: responsesInstructions(wake),
          reasoningEffort:
            wake.state.session.inferenceOverrides?.reasoningEffort ??
            context.profile.profile.modelConfig.reasoningEffort,
          maxOutputTokens: context.profile.profile.modelConfig.maxOutputTokens,
          ...(requestTimeoutMs === undefined
            ? {}
            : { providerRequestTimeoutMs: requestTimeoutMs }),
          workQuantumContinuationRounds:
            responsesWorkQuantumContinuationRounds(),
          noProgressAttentionThreshold: responsesNoProgressAttentionThreshold(),
          ...(context.profile.profile.contextPolicy &&
          context.profile.profile.modelConfig.contextWindowTokens
            ? {
                contextCompaction: {
                  enabled: context.profile.profile.contextPolicy.enabled,
                  autoCompactionEnabled:
                    context.profile.profile.contextPolicy.autoCompactionEnabled,
                  strategyId: context.profile.profile.contextPolicy.strategyId,
                  contextWindowTokens:
                    context.profile.profile.modelConfig.contextWindowTokens,
                  compactAtPercent:
                    context.profile.profile.contextPolicy.compactAtPercent,
                  targetPercentAfterCompaction:
                    context.profile.profile.contextPolicy
                      .targetPercentAfterCompaction,
                },
              }
            : {}),
        },
        client: responsesClientConfig,
      };
      const providerDebug = context.providerRequestDebugStore?.record({
        sessionId: wake.sessionId,
        wakeId: wake.wakeId,
        brainModule: "openai-responses",
        providerAlias:
          "providerAlias" in responsesClientConfig
            ? responsesClientConfig.providerAlias
            : undefined,
        model: input.config.model,
        protocol: "responses",
        providerKind: context.profile.profile.modelConfig.provider,
        request: {
          boundary: "ts_to_native_openai_responses",
          wakeId: input.wakeId,
          sessionId: input.sessionId,
          providerState: input.providerState,
          providerStateAbsence: input.providerStateAbsence,
          config: input.config,
          client: input.client,
          bodyState: input.bodyState,
        },
      });
      if (providerDebug) {
        await context.bridge.submitBrainEvent(
          providerRequestDebugEvent(wake, providerDebug),
        );
      }
      const result = await runOpenAiResponsesBrainWithIncrementalDrain(
        context,
        wake,
        input,
        options,
      );
      const exactDebug = recordOpenAiResponsesProviderRequestSamples(
        context,
        wake,
        input.config.model,
        responsesClientConfig,
        result.transportMetrics?.providerRequestDebugSamples,
      );
      if (exactDebug) {
        await context.bridge.submitBrainEvent(
          providerRequestDebugEvent(wake, exactDebug),
        );
      }
      responsesClientConfig =
        await persistOpenAiResponsesCredentialSecretUpdate(
          context,
          responsesClientConfig,
          result.credentialSecretUpdate,
        );
      return withOpenAiResponsesProviderStateScope(result, context);
    },
  };
}

function responsesInstructions(wake: BrainWakeInput): string {
  return [wake.systemPrompt, wake.roleAssembly.instructions]
    .filter((part): part is string => Boolean(part))
    .join("\n\n");
}

function recordOpenAiResponsesProviderRequestSamples(
  context: BrainHostContext,
  wake: BrainWakeInput,
  model: string,
  responsesClientConfig: OpenAiResponsesBrainRunInput["client"],
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
    brainModule: "openai-responses",
    providerAlias:
      responsesClientConfig && "providerAlias" in responsesClientConfig
        ? responsesClientConfig.providerAlias
        : undefined,
    model,
    protocol: "responses",
    providerKind: context.profile.profile.modelConfig.provider,
    request: {
      boundary: "rust_openai_responses_request",
      requestCount: samples.length,
      requests: samples,
    },
  });
}
