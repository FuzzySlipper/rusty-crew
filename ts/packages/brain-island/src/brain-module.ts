import type {
  BrainAction,
  BrainEventEnvelope,
  CompletionPacket,
  BrainProviderStateScope,
  BrainWakeProviderStateOutput,
  BrainStrategyMetadata,
  ProviderStateMode,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
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
}

export interface BrainModuleStrategyFingerprintMetadata {
  profileOptions?: unknown;
  providerOptions?: unknown;
}

export interface BrainModuleStrategyMetadata {
  strategyId: string;
  providerState: BrainModuleStrategyProviderStateMetadata;
  fingerprints?: BrainModuleStrategyFingerprintMetadata;
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

export const piAgentCoreBrainModule: BrainModule = {
  moduleId: "pi-agent-core",
  displayName: "pi-agent-core",
  defaultStrategyId: "default",
  strategies: [
    {
      strategyId: "default",
      providerState: { mode: "unused" },
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
      providerState: { mode: "unused" },
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

function openAiResponsesClientConfig(
  context: BrainModuleContext,
): { mode: "fake" } | { mode: "live"; baseUrl: string; apiKey: string } {
  if (process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE !== "1") {
    return { mode: "fake" };
  }
  const keyEnv =
    context.profile.profile.modelConfig.apiKeyEnv ?? "OPENAI_API_KEY";
  const apiKey = process.env[keyEnv];
  if (!apiKey) {
    throw new Error(
      `openai-responses live client requested but ${keyEnv} is not set`,
    );
  }
  return {
    mode: "live",
    baseUrl:
      context.profile.profile.modelConfig.baseUrl ??
      "https://api.openai.com/v1",
    apiKey,
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

export const openAiResponsesBrainModule: BrainModule = {
  moduleId: "openai-responses",
  displayName: "OpenAI Responses",
  defaultStrategyId: "replay",
  strategies: [
    {
      strategyId: "replay",
      providerState: { mode: "optional" },
      fingerprints: {
        providerOptions: {
          strategy: "replay",
        },
      },
    },
  ],
  diagnostics: {
    toolAdapterStatus: "native_neutral_tools",
  },
  async createBrain(context) {
    return {
      async wake(wake): Promise<{
        events: BrainEventEnvelope[];
        actions: BrainAction[];
        providerState?: BrainWakeProviderStateOutput;
        stream?: import("@rusty-crew/contracts").BrainWakeStreamItem[];
      }> {
        if (context.bridge?.runOpenAiResponsesBrain !== undefined) {
          try {
            return withOpenAiResponsesProviderStateScope(
              await context.bridge.runOpenAiResponsesBrain({
                wakeId: wake.wakeId,
                sessionId: wake.sessionId,
                bodyState: wake.state,
                providerState: wake.providerState,
                providerStateAbsence: wake.providerStateAbsence,
                config: {
                  model: context.profile.profile.modelConfig.modelName,
                  instructions: wake.systemPrompt,
                },
                client: openAiResponsesClientConfig(context),
              }),
              context,
            );
          } catch (error) {
            if (
              process.env.RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE === "1"
            ) {
              throw error;
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
