import type {
  BrainAction,
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
} from "@rusty-crew/native-bridge";
import { createDenRouterPiAgentFactory } from "./den-router-agent.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import { createPiAgentBrain, type PiAgentFactory } from "./pi-agent-brain.js";
import type { RustyCrewServiceConfig } from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { BrainActionPlanner, BrainImplementation } from "./index.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

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
    return createPiAgentBrain({
      createAgent,
      planActions: context.planActions,
      resolveTools: context.toolResolver,
      toolProfile: context.profile.toolSelection.toolProfile,
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
  return openAiResponsesClientMode(env) === "live" ? 120_000 : 30_000;
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
  input: Parameters<NativeBridgeModule["runOpenAiResponsesBrain"]>[0],
): Promise<{
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
}> {
  const bridge = context.bridge;
  if (bridge === undefined) {
    throw new Error(
      "OpenAI Responses incremental drain requires native bridge",
    );
  }
  const started = await bridge.startOpenAiResponsesBrain(input);
  const actions: BrainAction[] = [];

  for (;;) {
    const drained = await bridge.drainOpenAiResponsesBrainStream({
      wakeId: started.wakeId,
      maxItems: 32,
    });
    for (const item of drained.items) {
      await handleDrainedOpenAiResponsesStreamItem(bridge, item, actions);
    }
    if (drained.error !== undefined) {
      throw new Error(
        `OpenAI Responses buffered wake ${started.wakeId} failed: ${drained.error}`,
      );
    }
    if (drained.terminal) {
      return {
        events: [],
        actions,
        providerState: drained.providerState,
        credentialSecretUpdate: drained.credentialSecretUpdate,
      };
    }
    await delay(25);
  }
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
        stream?: import("@rusty-crew/contracts").BrainWakeStreamItem[];
        credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
      }> {
        if (context.bridge?.runOpenAiResponsesBrain !== undefined) {
          try {
            const input = {
              wakeId: wake.wakeId,
              sessionId: wake.sessionId,
              bodyState: wake.state,
              providerState: wake.providerState,
              providerStateAbsence: wake.providerStateAbsence,
              config: {
                model: context.profile.profile.modelConfig.modelName,
                instructions: wake.systemPrompt,
                streamIdleTimeoutMs: openAiResponsesStreamIdleTimeoutMs(),
              },
              client: responsesClientConfig,
            };
            const result = await runOpenAiResponsesBrainWithIncrementalDrain(
              context,
              input,
            );
            responsesClientConfig =
              await persistOpenAiResponsesCredentialSecretUpdate(
                context,
                responsesClientConfig,
                result.credentialSecretUpdate,
              );
            return withOpenAiResponsesProviderStateScope(result, context);
          } catch (error) {
            if (
              process.env.RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE === "1"
            ) {
              throw error;
            }
            try {
              const result = await context.bridge.runOpenAiResponsesBrain({
                wakeId: wake.wakeId,
                sessionId: wake.sessionId,
                bodyState: wake.state,
                providerState: wake.providerState,
                providerStateAbsence: wake.providerStateAbsence,
                config: {
                  model: context.profile.profile.modelConfig.modelName,
                  instructions: wake.systemPrompt,
                  streamIdleTimeoutMs: openAiResponsesStreamIdleTimeoutMs(),
                },
                client: responsesClientConfig,
              });
              responsesClientConfig =
                await persistOpenAiResponsesCredentialSecretUpdate(
                  context,
                  responsesClientConfig,
                  result.credentialSecretUpdate,
                );
              return withOpenAiResponsesProviderStateScope(result, context);
            } catch {
              // Fall through to the deterministic TS scaffold below. This
              // preserves the existing non-required-native behavior while the
              // buffered drain path is still settling.
            }
          }
        }
        const toolName = wake.state.session.toolProfile.tools[0]?.name;
        const hydrated = wake.providerState !== undefined;
        const outputItems = [
          {
            itemId: `message-${wake.wakeId}`,
            itemType: "message",
            rawJson: {
              type: "message",
              id: `message-${wake.wakeId}`,
              text: "responses replay service wake completed",
            },
          },
          ...(toolName
            ? [
                {
                  itemId: `call-${wake.wakeId}`,
                  itemType: "function_call",
                  callId: `call-${wake.wakeId}`,
                  rawJson: {
                    type: "function_call",
                    id: `call-${wake.wakeId}`,
                    call_id: `call-${wake.wakeId}`,
                    name: toolName,
                    arguments: "{}",
                  },
                },
                {
                  itemType: "function_call_output",
                  callId: `call-${wake.wakeId}`,
                  rawJson: {
                    type: "function_call_output",
                    call_id: `call-${wake.wakeId}`,
                    output: `${toolName} completed in deterministic field scaffold`,
                    is_error: false,
                  },
                },
              ]
            : []),
        ];
        const providerState: BrainWakeProviderStateOutput = {
          type: "replace",
          state: {
            moduleId: "openai-responses",
            strategyId: "replay",
            profileFingerprint:
              wake.providerState?.profileFingerprint ??
              context.providerStateScope?.profileFingerprint ??
              "profile-fingerprint",
            providerFingerprint:
              wake.providerState?.providerFingerprint ??
              context.providerStateScope?.providerFingerprint ??
              "provider-fingerprint",
            payloadVersion: "openai-responses-state-v1",
            payload: {
              kind: "openai-responses",
              strategyId: "replay",
              payloadVersion: "openai-responses-state-v1",
              lastCompletedResponse: {
                responseId: `resp-${wake.wakeId}`,
                outputItems,
                tokenUsage: {
                  inputTokens: 1,
                  cachedInputTokens: hydrated ? 1 : 0,
                  outputTokens: 1,
                  reasoningOutputTokens: 0,
                  totalTokens: 2,
                },
              },
              replayHints: {
                promptCacheKey: `profile:${wake.state.session.profileId}`,
                providerItemWatermark: `message-${wake.wakeId}`,
              },
            },
            ttlMs: 24 * 60 * 60 * 1000,
          },
        };
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
              event: {
                type: "provider_status",
                level: "info",
                message: hydrated
                  ? "openai-responses replay hydrated provider state"
                  : `openai-responses replay starting without provider state: ${
                      wake.providerStateAbsence ?? "missing"
                    }`,
              },
            },
            ...(toolName
              ? [
                  {
                    wakeId: wake.wakeId,
                    sessionId: wake.sessionId,
                    event: {
                      type: "tool_call_started" as const,
                      toolName,
                    },
                  },
                  {
                    wakeId: wake.wakeId,
                    sessionId: wake.sessionId,
                    event: {
                      type: "tool_call_finished" as const,
                      toolName,
                      isError: false,
                    },
                  },
                ]
              : []),
            {
              wakeId: wake.wakeId,
              sessionId: wake.sessionId,
              event: {
                type: "text_delta",
                text: "responses module scaffold wake completed",
              },
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
                summary: "responses replay service wake completed",
              } satisfies CompletionPacket,
            },
          ],
          providerState,
        };
      },
    };
  },
};
