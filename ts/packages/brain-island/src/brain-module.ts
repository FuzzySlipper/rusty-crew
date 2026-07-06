import type {
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  CompletionPacket,
  BrainProviderStateScope,
  BrainWakeProviderStateOutput,
  BrainWakeStreamItem,
  BrainStrategyMetadata,
  ProviderStateMode,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeModelProviderRecord,
  OpenAiResponsesCredentialSecretUpdate,
  OpenAiResponsesBrainRunInput,
  OpenAiResponsesToolRequest,
  OpenAiResponsesTransportMetrics,
} from "@rusty-crew/native-bridge";
import { createDenRouterPiAgentFactory } from "./den-router-agent.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import { createRoleplayNarratorBrain } from "./narrator-brain.js";
import { createPiAgentBrain, type PiAgentFactory } from "./pi-agent-brain.js";
import type { RustyCrewServiceConfig } from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type {
  BrainActionPlanner,
  BrainImplementation,
  BrainWakeInput,
} from "./index.js";
import {
  localToolCallMetadata,
  withToolCallDebugReference,
  type ToolCallDebugStore,
} from "./tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import {
  resolveToolSession,
  type BrainActionCollector,
  type BrainToolResolver,
} from "./tool-session-selection.js";

export type BrainModuleId = "pi-agent-core" | "local" | (string & {});

export interface BrainModuleSelection {
  moduleId: BrainModuleId;
  strategy?: string;
}

export interface BrainModuleStrategyProviderStateMetadata {
  mode: ProviderStateMode;
  rebuild: BrainModuleProviderStateRebuildPolicy;
}

export type BrainModuleProviderStateRebuildAction =
  | "discard"
  | "migrate"
  | "unsupported";

export interface BrainModuleProviderStateRebuildPolicy {
  action: BrainModuleProviderStateRebuildAction;
  reason: string;
  migrationId?: string;
}

export type PreviousResponseChainFallbackReason =
  | "no_predecessor_state"
  | "request_fingerprint_mismatch"
  | "profile_fingerprint_mismatch"
  | "provider_fingerprint_mismatch"
  | "predecessor_rejected_by_provider"
  | "provider_state_expired"
  | "provider_state_load_failed"
  | "input_not_append_only"
  | "normal_invalidation";

export interface BrainModuleStrategyFingerprintMetadata {
  profileOptions?: unknown;
  providerOptions?: unknown;
}

export interface BrainModuleStrategyDiagnosticsMetadata {
  selectedStrategyId: string;
  effectiveStrategyId: string;
  replayFallbackUsed: boolean;
  fallbackReason?: PreviousResponseChainFallbackReason;
  fallbackReasonCatalog?: readonly PreviousResponseChainFallbackReason[];
}

export interface BrainModuleStrategyMetadata {
  strategyId: string;
  providerState: BrainModuleStrategyProviderStateMetadata;
  fingerprints?: BrainModuleStrategyFingerprintMetadata;
  diagnostics?: BrainModuleStrategyDiagnosticsMetadata;
}

export type BrainModuleToolAdapterStatus =
  | "neutral_tools_adapted_to_pi"
  | "native_neutral_tools"
  | "tools_not_used"
  | "unknown";

export interface BrainModuleDiagnosticsMetadata {
  toolAdapterStatus: BrainModuleToolAdapterStatus;
}

export interface BrainModuleConfigSelection {
  module?: BrainModuleId;
  strategy?: string;
}

export interface BrainModuleContext {
  profile: LoadedProfileContext;
  serviceConfig?: RustyCrewServiceConfig;
  runtimeConfig?: RustyCrewRuntimeConfig;
  bridge?: NativeBridgeModule;
  providerStateScope?: BrainProviderStateScope;
  toolResolver?: BrainToolResolver;
  planActions?: BrainActionPlanner;
  maxTokens?: number;
  toolCallDebugStore?: ToolCallDebugStore;
  providerRequestDebugStore?: ProviderRequestDebugStore;
  createDenRouterAgentFactory?: (
    options: Parameters<typeof createDenRouterPiAgentFactory>[0],
  ) => Promise<PiAgentFactory>;
}

export interface BrainModule {
  readonly moduleId: BrainModuleId;
  readonly displayName: string;
  readonly defaultStrategyId: string;
  readonly strategies: readonly BrainModuleStrategyMetadata[];
  readonly diagnostics: BrainModuleDiagnosticsMetadata;
  createBrain(context: BrainModuleContext): Promise<BrainImplementation>;
}

export interface BrainModuleRegistry {
  get(moduleId: BrainModuleId): BrainModule | undefined;
  require(moduleId: BrainModuleId): BrainModule;
  list(): readonly BrainModule[];
}

export function createBrainModuleRegistry(
  modules: readonly BrainModule[] = defaultBrainModules(),
): BrainModuleRegistry {
  const byId = new Map(modules.map((module) => [module.moduleId, module]));
  return {
    get(moduleId) {
      return byId.get(moduleId);
    },
    require(moduleId) {
      const module = byId.get(moduleId);
      if (!module) {
        throw new Error(`unknown brain module ${moduleId}`);
      }
      return module;
    },
    list() {
      return [...byId.values()].sort((left, right) =>
        left.moduleId.localeCompare(right.moduleId),
      );
    },
  };
}

export function defaultBrainModules(): BrainModule[] {
  return [localBrainModule, openAiResponsesBrainModule, piAgentCoreBrainModule];
}

export function resolveBrainModuleSelection(
  input: Pick<LoadedProfileContext["profile"], "brain" | "modelConfig">,
): BrainModuleSelection {
  const configured = input.brain;
  if (configured?.module !== undefined) {
    return {
      moduleId: configured.module,
      ...(configured.strategy === undefined
        ? {}
        : { strategy: configured.strategy }),
    };
  }
  return {
    moduleId:
      input.modelConfig.provider === "local" ? "local" : "pi-agent-core",
  };
}

export function brainModuleSelectionFromRuntimeConfig(
  input?: BrainModuleConfigSelection,
): BrainModuleSelection | undefined {
  if (!input?.module) return undefined;
  return {
    moduleId: input.module,
    ...(input.strategy === undefined ? {} : { strategy: input.strategy }),
  };
}

export function resolveBrainStrategyMetadata(
  module: BrainModule,
  selection: BrainModuleSelection,
): BrainStrategyMetadata {
  const strategy = resolveBrainModuleStrategy(module, selection);
  return brainStrategyMetadataForModuleStrategy(module, strategy);
}

export function resolveBrainModuleStrategy(
  module: BrainModule,
  selection: BrainModuleSelection,
): BrainModuleStrategyMetadata {
  const strategyId = selection.strategy ?? module.defaultStrategyId;
  const strategy = module.strategies.find(
    (candidate) => candidate.strategyId === strategyId,
  );
  if (!strategy) {
    throw new Error(
      `unknown strategy ${strategyId} for brain module ${module.moduleId}`,
    );
  }
  return strategy;
}

export function brainStrategyMetadataForModuleStrategy(
  module: BrainModule,
  strategy: BrainModuleStrategyMetadata,
): BrainStrategyMetadata {
  return {
    moduleId: module.moduleId,
    strategyId: strategy.strategyId,
    providerState: strategy.providerState,
  };
}

export function providerStateRebuildPolicyForModuleStrategy(
  strategy: BrainModuleStrategyMetadata,
): BrainModuleProviderStateRebuildPolicy {
  return strategy.providerState.rebuild;
}

export const piAgentCoreBrainModule: BrainModule = {
  moduleId: "pi-agent-core",
  displayName: "pi-agent-core",
  defaultStrategyId: "default",
  strategies: [
    {
      strategyId: "default",
      providerState: {
        mode: "unused",
        rebuild: {
          action: "discard",
          reason: "pi-agent-core does not use persisted provider wire state",
        },
      },
    },
    {
      strategyId: "roleplay_narrator",
      providerState: {
        mode: "unused",
        rebuild: {
          action: "discard",
          reason:
            "roleplay narrator uses pi-agent-core turns without persisted provider wire state",
        },
      },
    },
  ],
  diagnostics: {
    toolAdapterStatus: "neutral_tools_adapted_to_pi",
  },
  async createBrain(context) {
    const profile = context.profile.profile;
    const createAgent = await (
      context.createDenRouterAgentFactory ?? createDenRouterPiAgentFactory
    )({
      modelId: profile.modelConfig.modelName,
      maxTokens: context.maxTokens,
      baseUrl: profile.modelConfig.baseUrl,
      api: profile.modelConfig.api,
      apiKeyEnv: profile.modelConfig.apiKeyEnv,
      temperature:
        profile.modelConfig.temperatureMilli === undefined
          ? undefined
          : profile.modelConfig.temperatureMilli / 1_000,
    });
    if (context.profile.profile.brain?.strategy === "roleplay_narrator") {
      return createRoleplayNarratorBrain({
        createAgent,
        planActions: context.planActions,
        resolveTools: context.toolResolver,
        toolProfile: context.profile.toolSelection.toolProfile,
        toolCallDebugStore: context.toolCallDebugStore,
        providerRequestDebugStore: context.providerRequestDebugStore,
        maxReviewCycles:
          context.profile.profile.roleplayNarrator?.review.maxReviewCycles,
        reviewEnabled: context.profile.profile.roleplayNarrator?.review.enabled,
        narratorConfig: context.profile.profile.roleplayNarrator,
        submitEvent: context.bridge
          ? async (event) => {
              await context.bridge?.submitBrainEvent(event);
            }
          : undefined,
      });
    }
    return createPiAgentBrain({
      createAgent,
      planActions: context.planActions,
      resolveTools: context.toolResolver,
      toolProfile: context.profile.toolSelection.toolProfile,
      toolCallDebugStore: context.toolCallDebugStore,
      providerRequestDebugStore: context.providerRequestDebugStore,
      submitEvent: context.bridge
        ? async (event) => {
            await context.bridge?.submitBrainEvent(event);
          }
        : undefined,
    });
  },
};

export const localBrainModule: BrainModule = {
  moduleId: "local",
  displayName: "Local deterministic",
  defaultStrategyId: "default",
  strategies: [
    {
      strategyId: "default",
      providerState: {
        mode: "unused",
        rebuild: {
          action: "discard",
          reason: "local deterministic brain does not use provider wire state",
        },
      },
    },
  ],
  diagnostics: {
    toolAdapterStatus: "tools_not_used",
  },
  async createBrain() {
    return {
      async wake(wake): Promise<{
        events: BrainEventEnvelope[];
        actions: BrainAction[];
      }> {
        return {
          events: [
            {
              wakeId: wake.wakeId,
              sessionId: wake.sessionId,
              event: { type: "started" },
            },
            {
              wakeId: wake.wakeId,
              sessionId: wake.sessionId,
              event: { type: "finished" },
            },
          ],
          actions: [
            {
              type: "deliver_completion",
              packet: {
                sessionId: wake.sessionId as SessionId,
                status: "completed",
                summary: "local service brain wake completed",
              } satisfies CompletionPacket,
            },
          ],
        };
      },
    };
  },
};

export type OpenAiResponsesClientMode = "fake" | "live";

export function openAiResponsesClientMode(
  env: Partial<
    Pick<NodeJS.ProcessEnv, "RUSTY_CREW_OPENAI_RESPONSES_LIVE">
  > = process.env,
): OpenAiResponsesClientMode {
  return env.RUSTY_CREW_OPENAI_RESPONSES_LIVE === "1" ? "live" : "fake";
}

export function openAiResponsesStreamIdleTimeoutMs(
  env: Partial<
    Pick<
      NodeJS.ProcessEnv,
      | "RUSTY_CREW_OPENAI_RESPONSES_LIVE"
      | "RUSTY_CREW_OPENAI_RESPONSES_STREAM_IDLE_TIMEOUT_MS"
    >
  > = process.env,
): number {
  const configured = Number.parseInt(
    env.RUSTY_CREW_OPENAI_RESPONSES_STREAM_IDLE_TIMEOUT_MS ?? "",
    10,
  );
  if (Number.isFinite(configured) && configured > 0) {
    return configured;
  }
  return openAiResponsesClientMode(env) === "live" ? 300_000 : 30_000;
}

type OpenAiResponsesClientConfig = NonNullable<
  OpenAiResponsesBrainRunInput["client"]
>;

async function openAiResponsesClientConfig(
  context: BrainModuleContext,
): Promise<OpenAiResponsesClientConfig> {
  if (openAiResponsesClientMode() !== "live") {
    return { mode: "fake" };
  }
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
    ...(apiKey ? { apiKey } : {}),
  };
}

async function persistOpenAiResponsesCredentialSecretUpdate(
  context: BrainModuleContext,
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
    clearSecret: false,
    metadataJson: provider.metadataJson,
  };
}

function withOpenAiResponsesProviderStateScope<
  T extends { providerState?: BrainWakeProviderStateOutput },
>(result: T, context: BrainModuleContext): T {
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
  context: BrainModuleContext,
  wake: BrainWakeInput,
  input: Parameters<NativeBridgeModule["runOpenAiResponsesBrain"]>[0],
): Promise<{
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  transportMetrics?: OpenAiResponsesTransportMetrics;
  brainEventCounts?: Record<string, number>;
  brainStreamItemCounts?: Record<string, number>;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
}> {
  const bridge = context.bridge;
  if (bridge === undefined) {
    throw new Error(
      "OpenAI Responses incremental drain requires native bridge",
    );
  }
  const selectionActions = createResponsesBrainActionCollector();
  const toolSelection = resolveToolSession({
    wake,
    resolveTools: context.toolResolver,
    toolProfile: context.profile.toolSelection.toolProfile,
    actions: selectionActions,
  });
  const toolsByName = new Map(
    toolSelection.tools.map((tool) => [tool.name, tool]),
  );
  const started = await bridge.startOpenAiResponsesBrain({
    ...input,
    tools: toolSelection.tools.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.parameters,
    })),
  });
  const streamActions: BrainAction[] = [];
  const brainEventCounts: Record<string, number> = {};
  const brainStreamItemCounts: Record<string, number> = {};
  const toolDebugReferences = createOpenAiResponsesToolDebugReferences();
  const toolFailurePolicy = createOpenAiResponsesToolFailurePolicyState();

  for (;;) {
    const drained = await bridge.drainOpenAiResponsesBrainStream({
      wakeId: started.wakeId,
      maxItems: 32,
    });
    const preparedToolRequests = drained.toolRequests.map((request) =>
      prepareOpenAiResponsesToolRequest(
        wake,
        request,
        toolsByName,
        context.toolCallDebugStore,
      ),
    );
    addPreparedOpenAiResponsesToolDebugReferences(
      toolDebugReferences,
      preparedToolRequests,
    );
    for (const item of drained.items) {
      incrementCount(brainStreamItemCounts, item.type);
      if (item.type === "event") {
        incrementCount(brainEventCounts, item.event.event.type);
      }
      await handleDrainedOpenAiResponsesStreamItem(
        bridge,
        withOpenAiResponsesToolDebugReference(item, toolDebugReferences),
        streamActions,
      );
    }
    for (const request of preparedToolRequests) {
      const output = await executePreparedOpenAiResponsesToolRequest(
        wake,
        request,
        context.toolCallDebugStore,
      );
      await bridge.submitOpenAiResponsesToolOutput({
        wakeId: started.wakeId,
        callId: request.request.callId,
        output: output.output,
        isError: output.isError,
      });
      const stopReport = recordOpenAiResponsesToolFailure(
        toolFailurePolicy,
        output.failure,
      );
      if (stopReport !== undefined) {
        await submitOpenAiResponsesBrainEvent(bridge, wake, {
          type: "provider_status",
          level: "error",
          message: stopReport,
        });
        throw new Error(stopReport);
      }
    }
    if (drained.error !== undefined) {
      throw new Error(
        `OpenAI Responses buffered wake ${started.wakeId} failed: ${drained.error}`,
      );
    }
    if (drained.terminal) {
      return {
        events: [],
        actions: [...selectionActions.actions, ...streamActions],
        providerState: drained.providerState,
        transportMetrics: drained.transportMetrics,
        brainEventCounts,
        brainStreamItemCounts,
        credentialSecretUpdate: drained.credentialSecretUpdate,
      };
    }
    await delay(25);
  }
}

function createResponsesBrainActionCollector(): BrainActionCollector {
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

interface PreparedOpenAiResponsesToolRequest {
  request: OpenAiResponsesToolRequest;
  tool?: BrainTool;
  params?: unknown;
  debugDetailId?: string;
  preparationError?: string;
}

interface OpenAiResponsesToolFailure {
  toolName: string;
  reasonCode: string;
  retryable: boolean;
  detail: string;
}

interface OpenAiResponsesToolExecutionResult {
  output: string;
  isError: boolean;
  failure?: OpenAiResponsesToolFailure;
}

interface OpenAiResponsesToolFailurePolicyState {
  totalFailures: number;
  consecutiveFailures: number;
  failuresByKey: Map<string, number>;
  recentFailures: OpenAiResponsesToolFailure[];
}

function prepareOpenAiResponsesToolRequest(
  wake: BrainWakeInput,
  request: OpenAiResponsesToolRequest,
  toolsByName: ReadonlyMap<string, BrainTool>,
  toolCallDebugStore: ToolCallDebugStore | undefined,
): PreparedOpenAiResponsesToolRequest {
  const tool = toolsByName.get(request.name);
  if (tool === undefined) {
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: { argumentsJson: request.argumentsJson },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      debugDetailId: debugRecord?.debug_detail_id,
      preparationError: `Tool ${request.name} is not available in this brain session.`,
    };
  }
  let rawArguments: unknown;
  try {
    rawArguments =
      request.argumentsJson.trim().length === 0
        ? {}
        : JSON.parse(request.argumentsJson);
  } catch (error) {
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: { argumentsJson: request.argumentsJson },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      tool,
      debugDetailId: debugRecord?.debug_detail_id,
      preparationError: `Tool ${request.name} arguments were not valid JSON: ${
        error instanceof Error ? error.message : String(error)
      }`,
    };
  }
  try {
    const params = tool.prepareArguments
      ? tool.prepareArguments(rawArguments)
      : (rawArguments as never);
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: {
        rawArguments,
        preparedArguments: params,
      },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      tool,
      params,
      debugDetailId: debugRecord?.debug_detail_id,
    };
  } catch (error) {
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: { rawArguments },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      tool,
      debugDetailId: debugRecord?.debug_detail_id,
      preparationError: `Tool ${request.name} argument preparation failed: ${
        error instanceof Error ? error.message : String(error)
      }`,
    };
  }
}

async function executePreparedOpenAiResponsesToolRequest(
  wake: BrainWakeInput,
  prepared: PreparedOpenAiResponsesToolRequest,
  toolCallDebugStore: ToolCallDebugStore | undefined,
): Promise<OpenAiResponsesToolExecutionResult> {
  const failDebugRecord = (error: unknown) => {
    if (prepared.debugDetailId) {
      toolCallDebugStore?.fail({
        debugDetailId: prepared.debugDetailId,
        error,
      });
    }
  };
  if (prepared.preparationError) {
    failDebugRecord(prepared.preparationError);
    return {
      output: prepared.preparationError,
      isError: true,
      failure: {
        toolName: prepared.request.name,
        reasonCode: "tool_preparation_failed",
        retryable: false,
        detail: prepared.preparationError,
      },
    };
  }
  if (!prepared.tool) {
    const output = `Tool ${prepared.request.name} is not available in this brain session.`;
    failDebugRecord(output);
    return {
      output,
      isError: true,
      failure: {
        toolName: prepared.request.name,
        reasonCode: "tool_unavailable",
        retryable: false,
        detail: output,
      },
    };
  }
  try {
    const controller = new AbortController();
    const result = prepared.tool.executeWithContext
      ? await prepared.tool.executeWithContext(prepared.params as never, {
          wake,
          wakeId: wake.wakeId,
          sessionId: wake.sessionId,
          callId: prepared.request.callId,
          signal: controller.signal,
        })
      : await prepared.tool.execute(
          prepared.request.callId,
          prepared.params as never,
          controller.signal,
        );
    if (prepared.debugDetailId) {
      toolCallDebugStore?.finish({
        debugDetailId: prepared.debugDetailId,
        finalResult: result,
      });
    }
    const failure = openAiResponsesToolFailureFromResult(
      prepared.request.name,
      result,
    );
    return {
      output: brainToolResultToOpenAiResponsesOutput(result),
      isError: failure !== undefined,
      ...(failure === undefined ? {} : { failure }),
    };
  } catch (error) {
    if (prepared.debugDetailId) {
      toolCallDebugStore?.fail({
        debugDetailId: prepared.debugDetailId,
        error,
      });
    }
    const detail = `Tool ${prepared.request.name} failed: ${
      error instanceof Error ? error.message : String(error)
    }`;
    return {
      output: detail,
      isError: true,
      failure: {
        toolName: prepared.request.name,
        reasonCode: "tool_exception",
        retryable: true,
        detail,
      },
    };
  }
}

function createOpenAiResponsesToolFailurePolicyState(): OpenAiResponsesToolFailurePolicyState {
  return {
    totalFailures: 0,
    consecutiveFailures: 0,
    failuresByKey: new Map(),
    recentFailures: [],
  };
}

function openAiResponsesToolFailureFromResult(
  toolName: string,
  result: BrainToolResult,
): OpenAiResponsesToolFailure | undefined {
  const details = result.details;
  if (!isRecord(details)) return undefined;
  if (details.ok !== false && details.action !== "failed") return undefined;
  const reasonCode =
    stringField(details, "reasonCode") ??
    stringField(details, "reason_code") ??
    stringField(details, "code") ??
    stringField(details, "action") ??
    "tool_reported_unsuccessful_result";
  const operation = stringField(details, "operation");
  const retryable =
    typeof details.retryable === "boolean" ? details.retryable : true;
  return {
    toolName,
    reasonCode,
    retryable,
    detail: [
      `${toolName} returned ok=false`,
      operation ? `operation=${operation}` : undefined,
      `reason=${reasonCode}`,
      `retryable=${retryable}`,
    ]
      .filter(Boolean)
      .join(" "),
  };
}

function recordOpenAiResponsesToolFailure(
  state: OpenAiResponsesToolFailurePolicyState,
  failure: OpenAiResponsesToolFailure | undefined,
): string | undefined {
  if (failure === undefined) {
    state.consecutiveFailures = 0;
    return undefined;
  }
  state.totalFailures += 1;
  state.consecutiveFailures += 1;
  state.recentFailures.push(failure);
  if (state.recentFailures.length > 5) state.recentFailures.shift();
  const key = `${failure.toolName}:${failure.reasonCode}`;
  const keyCount = (state.failuresByKey.get(key) ?? 0) + 1;
  state.failuresByKey.set(key, keyCount);

  if (keyCount >= 2) {
    return openAiResponsesToolFailureStopReport(
      state,
      `repeated ${failure.toolName} failure (${failure.reasonCode})`,
    );
  }
  if (state.consecutiveFailures >= 3) {
    return openAiResponsesToolFailureStopReport(
      state,
      "three consecutive tool failures",
    );
  }
  return undefined;
}

function openAiResponsesToolFailureStopReport(
  state: OpenAiResponsesToolFailurePolicyState,
  reason: string,
): string {
  const recent = state.recentFailures
    .slice(-5)
    .map(
      (failure) =>
        `${failure.toolName}: ${failure.reasonCode} (retryable=${failure.retryable})`,
    )
    .join("; ");
  return [
    `Stopping assistant turn after ${reason}.`,
    `Tool failure count this turn: ${state.totalFailures}.`,
    recent ? `Recent tool failures: ${recent}.` : undefined,
    "The assistant should report the unavailable tool/dependency instead of continuing unrelated tool attempts.",
  ]
    .filter(Boolean)
    .join("\n");
}

interface OpenAiResponsesToolDebugReferences {
  startByToolName: Map<string, string[]>;
  finishByToolName: Map<string, string[]>;
}

function createOpenAiResponsesToolDebugReferences(): OpenAiResponsesToolDebugReferences {
  return {
    startByToolName: new Map(),
    finishByToolName: new Map(),
  };
}

function addPreparedOpenAiResponsesToolDebugReferences(
  references: OpenAiResponsesToolDebugReferences,
  preparedRequests: readonly PreparedOpenAiResponsesToolRequest[],
): void {
  for (const prepared of preparedRequests) {
    if (!prepared.debugDetailId) continue;
    pushDebugReference(
      references.startByToolName,
      prepared.request.name,
      prepared.debugDetailId,
    );
    pushDebugReference(
      references.finishByToolName,
      prepared.request.name,
      prepared.debugDetailId,
    );
  }
}

function pushDebugReference(
  references: Map<string, string[]>,
  toolName: string,
  debugDetailId: string,
): void {
  const refs = references.get(toolName) ?? [];
  refs.push(debugDetailId);
  references.set(toolName, refs);
}

function withOpenAiResponsesToolDebugReference(
  item: BrainWakeStreamItem,
  debugReferences: OpenAiResponsesToolDebugReferences,
): BrainWakeStreamItem {
  if (item.type !== "event") return item;
  const event = item.event.event;
  if (
    event.type !== "tool_call_started" &&
    event.type !== "tool_call_finished"
  ) {
    return item;
  }
  const referencesByToolName =
    event.type === "tool_call_started"
      ? debugReferences.startByToolName
      : debugReferences.finishByToolName;
  const refs = referencesByToolName.get(event.toolName);
  const debugDetailId = refs?.shift();
  if (!debugDetailId) return item;
  if (refs && refs.length === 0) {
    referencesByToolName.delete(event.toolName);
  }
  return {
    ...item,
    event: {
      ...item.event,
      event: {
        ...event,
        metadata: withToolCallDebugReference(event.metadata, debugDetailId),
      },
    },
  };
}

function brainToolResultToOpenAiResponsesOutput(
  result: BrainToolResult,
): string {
  const content = result.content
    .map((item) => {
      if (item.type === "text") return item.text;
      return `[image:${item.mimeType};${item.data.length} bytes]`;
    })
    .filter((text) => text.length > 0)
    .join("\n");
  const details =
    result.details === undefined
      ? undefined
      : safeJsonStringify(result.details);
  const output =
    details === undefined || details === "{}"
      ? content
      : [content, `Details:\n${details}`].filter(Boolean).join("\n\n");
  return limitOpenAiResponsesToolOutput(output || "(tool returned no content)");
}

function safeJsonStringify(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function limitOpenAiResponsesToolOutput(output: string): string {
  const maxChars = 20_000;
  if (output.length <= maxChars) return output;
  return `${output.slice(0, maxChars)}\n[truncated ${output.length - maxChars} chars]`;
}

async function handleDrainedOpenAiResponsesStreamItem(
  bridge: NativeBridgeModule,
  item: BrainWakeStreamItem,
  actions: BrainAction[],
): Promise<void> {
  switch (item.type) {
    case "event":
      await bridge.submitBrainEvent(item.event);
      return;
    case "actions":
      actions.push(...item.batch.actions);
      return;
    case "wake_failed":
      throw new Error(
        `OpenAI Responses wake ${item.failure.wakeId} failed: ${item.failure.message}`,
      );
  }
}

async function submitOpenAiResponsesBrainEvent(
  bridge: NativeBridgeModule,
  wake: BrainWakeInput,
  event: BrainEvent,
): Promise<void> {
  await bridge.submitBrainEvent({
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    event,
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(
  record: Record<string, unknown>,
  field: string,
): string | undefined {
  const value = record[field];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function incrementCount(counts: Record<string, number>, key: string): void {
  counts[key] = (counts[key] ?? 0) + 1;
}

async function delay(delayMs: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, delayMs));
}

export const openAiResponsesBrainModule: BrainModule = {
  moduleId: "openai-responses",
  displayName: "OpenAI Responses",
  defaultStrategyId: "replay",
  strategies: [
    {
      strategyId: "replay",
      providerState: {
        mode: "optional",
        rebuild: {
          action: "discard",
          reason:
            "OpenAI Responses wire state is response-chain scoped and is discarded on runtime brain rebuild unless a safe migration is explicitly implemented",
        },
      },
      fingerprints: {
        providerOptions: {
          strategy: "replay",
        },
      },
      diagnostics: {
        selectedStrategyId: "replay",
        effectiveStrategyId: "replay",
        replayFallbackUsed: false,
      },
    },
    {
      strategyId: "previous-response-chain",
      providerState: {
        mode: "optional",
        rebuild: {
          action: "discard",
          reason:
            "OpenAI Responses previous_response_id state is provider-chain scoped and is discarded on runtime brain rebuild unless a safe migration is explicitly implemented",
        },
      },
      fingerprints: {
        providerOptions: {
          strategy: "previous-response-chain",
        },
      },
      diagnostics: {
        selectedStrategyId: "previous-response-chain",
        effectiveStrategyId: "replay",
        replayFallbackUsed: true,
        fallbackReason: "normal_invalidation",
        fallbackReasonCatalog: [
          "no_predecessor_state",
          "request_fingerprint_mismatch",
          "profile_fingerprint_mismatch",
          "provider_fingerprint_mismatch",
          "predecessor_rejected_by_provider",
          "provider_state_expired",
          "provider_state_load_failed",
          "input_not_append_only",
          "normal_invalidation",
        ],
      },
    },
  ],
  diagnostics: {
    toolAdapterStatus: "native_neutral_tools",
  },
  async createBrain(context) {
    let responsesClientConfig = await openAiResponsesClientConfig(context);
    return {
      async wake(wake): Promise<{
        events: BrainEventEnvelope[];
        actions: BrainAction[];
        providerState?: BrainWakeProviderStateOutput;
        transportMetrics?: OpenAiResponsesTransportMetrics;
        stream?: import("@rusty-crew/contracts").BrainWakeStreamItem[];
        credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
      }> {
        if (context.bridge === undefined) {
          throw new Error("openai-responses brain requires native bridge");
        }
        const input = {
          wakeId: wake.wakeId,
          sessionId: wake.sessionId,
          bodyState: wake.state,
          providerState: wake.providerState,
          providerStateAbsence: wake.providerStateAbsence,
          config: {
            model: context.profile.profile.modelConfig.modelName,
            instructions: responsesInstructions(wake),
            streamIdleTimeoutMs: openAiResponsesStreamIdleTimeoutMs(),
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
          providerKind:
            "authKind" in responsesClientConfig &&
            responsesClientConfig.authKind === "openai_oauth"
              ? "openai_oauth"
              : undefined,
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
  },
};

function responsesInstructions(wake: BrainWakeInput): string {
  return [wake.systemPrompt, wake.roleAssembly.instructions]
    .filter((part): part is string => Boolean(part))
    .join("\n\n");
}

function providerRequestDebugEvent(
  wake: BrainWakeInput,
  debug: {
    debug_detail_id: string;
    request_sha256: string;
    request_json_chars: number;
    expires_at: string;
  },
): BrainEventEnvelope {
  return {
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    event: {
      type: "provider_status",
      level: "info",
      message: "Provider request debug snapshot captured.",
      metadataJson: JSON.stringify({
        provider_request_debug_detail_id: debug.debug_detail_id,
        provider_request_debug_url: `/v1/chat/sessions/${encodeURIComponent(
          String(wake.sessionId),
        )}/provider-requests/${encodeURIComponent(debug.debug_detail_id)}`,
        request_sha256: debug.request_sha256,
        request_json_chars: debug.request_json_chars,
        expires_at: debug.expires_at,
      }),
    },
  };
}

function recordOpenAiResponsesProviderRequestSamples(
  context: BrainModuleContext,
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
    providerKind:
      responsesClientConfig &&
      "authKind" in responsesClientConfig &&
      responsesClientConfig.authKind === "openai_oauth"
        ? "openai_oauth"
        : undefined,
    request: {
      boundary: "rust_openai_responses_request",
      requestCount: samples.length,
      requests: samples,
    },
  });
}
