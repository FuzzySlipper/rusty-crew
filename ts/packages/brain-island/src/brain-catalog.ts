import type {
  BrainStrategyMetadata,
  ProviderStateMode,
} from "@rusty-crew/contracts";
import type {
  NativeBrainHostCapability,
  NativeBrainSelectionPlan,
  NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import type { ProfileConfig } from "./profile-loading.js";

export type BrainModuleId = string;

export interface BrainModuleSelection {
  moduleId: BrainModuleId;
  strategy: string;
}

export type BrainModuleProviderStateRebuildAction =
  | "reconstruct"
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

export interface BrainModuleStrategyDiagnosticsMetadata {
  selectedStrategyId: string;
  effectiveStrategyId: string;
  replayFallbackUsed: boolean;
  fallbackReason?: PreviousResponseChainFallbackReason;
  fallbackReasonCatalog?: readonly PreviousResponseChainFallbackReason[];
}

export interface BrainModuleStrategyMetadata {
  strategyId: string;
  providerState: {
    mode: ProviderStateMode;
    rebuild: BrainModuleProviderStateRebuildPolicy;
  };
  fingerprints?: {
    profileOptions?: unknown;
    providerOptions?: unknown;
  };
  diagnostics: BrainModuleStrategyDiagnosticsMetadata;
}

export interface ResolvedBrainCatalogSelection {
  catalogRevision: number;
  selection: BrainModuleSelection;
  strategy: BrainStrategyMetadata;
  moduleStrategy: BrainModuleStrategyMetadata;
  requiredHostCapabilities: readonly NativeBrainHostCapability[];
}

export interface BrainHostCapabilityRegistration {
  registrationId: "rusty-crew-ts-host";
  capabilities: readonly NativeBrainHostCapability[];
}

export const BRAIN_HOST_CAPABILITY_REGISTRATION: BrainHostCapabilityRegistration =
  {
    registrationId: "rusty-crew-ts-host",
    capabilities: ["execute_tool", "project_debug_reference", "project_event"],
  };

export async function resolveBrainCatalogSelection(
  bridge: Pick<NativeBridgeModule, "planBrainSelection">,
  profile: Pick<ProfileConfig, "brain" | "modelConfig" | "roleplayNarrator">,
  hostRegistration: BrainHostCapabilityRegistration = BRAIN_HOST_CAPABILITY_REGISTRATION,
): Promise<ResolvedBrainCatalogSelection> {
  const plan = await bridge.planBrainSelection({
    ...(profile.brain?.module === undefined
      ? {}
      : { configuredModuleId: profile.brain.module }),
    ...(profile.brain?.strategy === undefined
      ? {}
      : { configuredStrategyId: profile.brain.strategy }),
    providerProtocol: providerProtocol(profile.modelConfig.api),
    providerKind: profile.modelConfig.provider,
    roleplayNarratorEnabled:
      profile.brain?.strategy === "roleplay_narrator" ||
      profile.roleplayNarrator !== undefined,
  });
  const missingCapabilities = plan.required_host_capabilities.filter(
    (capability) => !hostRegistration.capabilities.includes(capability),
  );
  if (missingCapabilities.length > 0) {
    throw new Error(
      `brain ${plan.module_id} requires unregistered host capabilities: ${missingCapabilities.join(", ")}`,
    );
  }
  return selectionFromNativePlan(plan);
}

export function selectionFromNativePlan(
  plan: NativeBrainSelectionPlan,
): ResolvedBrainCatalogSelection {
  const rebuild = {
    action: plan.provider_state_policy.rebuild.action,
    reason: plan.provider_state_policy.rebuild.reason,
    ...(plan.provider_state_policy.rebuild.migration_id === undefined
      ? {}
      : { migrationId: plan.provider_state_policy.rebuild.migration_id }),
  };
  const diagnostics: BrainModuleStrategyDiagnosticsMetadata = {
    selectedStrategyId: plan.strategy_diagnostics.selected_strategy_id,
    effectiveStrategyId: plan.strategy_diagnostics.effective_strategy_id,
    replayFallbackUsed: plan.strategy_diagnostics.replay_fallback_used,
    ...(plan.strategy_diagnostics.fallback_reason === undefined
      ? {}
      : {
          fallbackReason: plan.strategy_diagnostics
            .fallback_reason as PreviousResponseChainFallbackReason,
        }),
    ...(plan.strategy_diagnostics.fallback_reason_catalog === undefined
      ? {}
      : {
          fallbackReasonCatalog: plan.strategy_diagnostics
            .fallback_reason_catalog as PreviousResponseChainFallbackReason[],
        }),
  };
  const moduleStrategy: BrainModuleStrategyMetadata = {
    strategyId: plan.selected_strategy_id,
    providerState: {
      mode: plan.provider_state_policy.mode,
      rebuild,
    },
    ...(plan.profile_fingerprint_options === undefined &&
    plan.provider_fingerprint_options === undefined
      ? {}
      : {
          fingerprints: {
            ...(plan.profile_fingerprint_options === undefined
              ? {}
              : { profileOptions: plan.profile_fingerprint_options }),
            ...(plan.provider_fingerprint_options === undefined
              ? {}
              : { providerOptions: plan.provider_fingerprint_options }),
          },
        }),
    diagnostics,
  };
  return {
    catalogRevision: plan.catalog_revision,
    selection: {
      moduleId: plan.module_id,
      strategy: plan.selected_strategy_id,
    },
    strategy: {
      moduleId: plan.module_id,
      strategyId: plan.selected_strategy_id,
      providerState: moduleStrategy.providerState,
    },
    moduleStrategy,
    requiredHostCapabilities: plan.required_host_capabilities,
  };
}

function providerProtocol(
  api: string | undefined,
): "chat_completions" | "responses" {
  return api === "responses" || api === "openai-responses"
    ? "responses"
    : "chat_completions";
}
