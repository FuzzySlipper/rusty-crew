import type {
  NativeBridgeModule,
  NativeCreateProfilePlan,
  NativeCreateProfileRequest,
  NativeRuntimeConfigPlan,
  NativeRuntimeConfigValidationInput,
  NativeRuntimeConfigValidationResult,
  NativeProfileRuntimeMetadata,
} from "@rusty-crew/native-bridge";
import type { ProfileConfig } from "./profile-loading.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

export function runtimeConfigValidationInput(
  runtimeConfig: RustyCrewRuntimeConfig,
  profiles: readonly ProfileConfig[],
): NativeRuntimeConfigValidationInput {
  return {
    runtimeConfig: {
      profilesDir: runtimeConfig.profilesDir,
      skillsDir: runtimeConfig.skillsDir,
      brains: runtimeConfig.brains.map((brain) => ({
        implementationId: brain.implementationId,
        profileId: brain.profileId,
      })),
      sessions: runtimeConfig.sessions.map((session) => ({
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: session.kind,
        resourceLimits: session.resourceLimits,
        ownerId: session.ownerId,
        maxHistoryMessages: session.maxHistoryMessages,
        turnTimeoutMs: session.turnTimeoutMs,
      })),
      scheduledJobs: runtimeConfig.scheduledJobs.map((job) => ({
        id: job.id,
        schedule: job.schedule,
        shape: job.shape,
        jobKind: job.jobKind,
        targetSessionId: job.targetSessionId,
        script: job.script,
        deliveryChannelId: job.deliveryChannelId,
      })),
      channelBindings: runtimeConfig.channelBindings.map((binding) => ({
        bindingId: binding.bindingId,
        adapterId: binding.adapterId,
        provider: binding.provider,
        agentId: binding.agentId,
        instanceId: binding.instanceId,
        sessionId: binding.sessionId,
        profileId: binding.profileId,
        externalChannelId: binding.externalChannelId,
        externalThreadId: binding.externalThreadId,
        externalUserId: binding.externalUserId,
        conversationProjectId: binding.conversationProjectId,
        conversationChannelId: binding.conversationChannelId,
        providerSubscriptionId: binding.providerSubscriptionId,
        status: binding.status,
      })),
      mcpBindings: runtimeConfig.mcpBindings.map((binding) => ({
        bindingId: binding.bindingId,
        adapterId: binding.adapterId,
        agentId: binding.agentId,
        instanceId: binding.instanceId,
        sessionId: binding.sessionId,
        profileId: binding.profileId,
        serverNames: binding.serverNames,
        endpointRef: binding.endpointRef,
        transport: binding.transport,
        toolProfileKey: binding.toolProfileKey,
        status: binding.status,
      })),
    },
    profiles: profiles.map(profileRuntimeMetadata),
  };
}

export async function validateRuntimeConfigWithRust(input: {
  bridge: Pick<NativeBridgeModule, "validateRuntimeConfigDraft">;
  runtimeConfig: RustyCrewRuntimeConfig;
  profiles: readonly ProfileConfig[];
}): Promise<NativeRuntimeConfigValidationResult> {
  return input.bridge.validateRuntimeConfigDraft(
    runtimeConfigValidationInput(input.runtimeConfig, input.profiles),
  );
}

export async function planRuntimeConfigWithRust(input: {
  bridge: Pick<NativeBridgeModule, "planRuntimeConfig">;
  runtimeConfig: RustyCrewRuntimeConfig;
  profiles: readonly ProfileConfig[];
}): Promise<NativeRuntimeConfigPlan> {
  return input.bridge.planRuntimeConfig(
    runtimeConfigValidationInput(input.runtimeConfig, input.profiles),
  );
}

export async function planCreateProfileWithRust(input: {
  bridge: Pick<NativeBridgeModule, "planCreateProfile">;
  runtimeConfig: RustyCrewRuntimeConfig;
  profiles: readonly ProfileConfig[];
  request: NativeCreateProfileRequest;
}): Promise<NativeCreateProfilePlan> {
  const validationInput = runtimeConfigValidationInput(
    input.runtimeConfig,
    input.profiles,
  );
  return input.bridge.planCreateProfile({
    ...validationInput,
    request: input.request,
  });
}

function profileRuntimeMetadata(
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
  };
}
