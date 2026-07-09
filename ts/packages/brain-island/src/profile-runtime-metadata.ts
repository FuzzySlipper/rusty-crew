import type { NativeProfileRuntimeMetadata } from "@rusty-crew/native-bridge";
import type { ProfileConfig } from "./profile-loading.js";

export function profileRuntimeMetadata(
  profile: ProfileConfig,
): NativeProfileRuntimeMetadata {
  return {
    profileId: profile.profileId,
    brain: profile.brain
      ? {
          module: profile.brain.module,
          strategy: profile.brain.strategy,
        }
      : undefined,
    runtime: profile.runtime
      ? {
          defaultResourceLimits: profile.runtime.defaultResourceLimits,
          maxTurnDurationMs: profile.runtime.maxTurnDurationMs,
          maxTokensPerTurn: profile.runtime.maxTokensPerTurn,
        }
      : undefined,
    sessionDefaults: profile.sessionDefaults,
    mcpConfig: profile.mcpConfig
      ? {
          bindingId: profile.mcpConfig.bindingId,
          endpointRef: profile.mcpConfig.endpointRef,
          serverNames: profile.mcpConfig.serverNames ?? [],
          transport: profile.mcpConfig.transport,
          toolProfile: profile.mcpConfig.toolProfile,
        }
      : undefined,
    backgroundReview: profile.backgroundReview
      ? {
          enabled: profile.backgroundReview.enabled,
          reviewType: profile.backgroundReview.reviewType,
          schedule: profile.backgroundReview.schedule,
        }
      : undefined,
    channelDefaults: profile.channelDefaults,
    contextPolicy: profile.contextPolicy
      ? {
          enabled: profile.contextPolicy.enabled,
          strategyId: profile.contextPolicy.strategyId,
          autoCompactionEnabled: profile.contextPolicy.autoCompactionEnabled,
          compactAtPercent: profile.contextPolicy.compactAtPercent,
          targetPercentAfterCompaction:
            profile.contextPolicy.targetPercentAfterCompaction,
          maxContextPercentForWake:
            profile.contextPolicy.maxContextPercentForWake,
          debugVisibility: profile.contextPolicy.debugVisibility,
          includeDebugEventsInModelContext:
            profile.contextPolicy.includeDebugEventsInModelContext,
          strategyConfig: profile.contextPolicy.strategyConfig,
        }
      : undefined,
  };
}

export function profileRuntimeMetadataList(
  profiles: readonly ProfileConfig[],
): NativeProfileRuntimeMetadata[] {
  return profiles.map(profileRuntimeMetadata);
}
