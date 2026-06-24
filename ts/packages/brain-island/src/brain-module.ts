import type {
  BrainAction,
  BrainEventEnvelope,
  CompletionPacket,
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
  return [localBrainModule, piAgentCoreBrainModule];
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

export const piAgentCoreBrainModule: BrainModule = {
  moduleId: "pi-agent-core",
  displayName: "pi-agent-core",
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
