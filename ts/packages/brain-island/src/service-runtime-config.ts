import { readFile } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";
import type {
  AgentId,
  BrainModelConfig,
  BrainAction,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationId,
  BrainProviderStateScope,
  CompletionPacket,
  ChannelBindingRecord,
  McpBindingRecord,
  McpSurfaceDiagnostics,
  ProfileId,
  ResourceLimits,
  ScheduledJobSummary,
  SessionId,
  SessionKind,
  ToolProfile,
} from "@rusty-crew/contracts";
import {
  createDenMemoryClient,
  type DenMemoryClient,
} from "@rusty-crew/adapter-den";
import type {
  BrainWakeExecutor,
  NativeBridgeModule,
  NativeRuntimeConfigDiagnostic,
  NativeRuntimeConfigDraft,
  NativeRuntimeConfigPlan,
  NativeModelProviderRecord,
  NativeSessionStateSummary,
} from "@rusty-crew/native-bridge";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createBrowserToolResolver,
  MemoryBrowserScreenshotStore,
} from "./browser-tools.js";
import { BrowserSessionManager } from "./browser-session-manager.js";
import {
  createBrainModuleRegistry,
  brainStrategyMetadataForModuleStrategy,
  providerStateRebuildPolicyForModuleStrategy,
  resolveBrainModuleStrategy,
  resolveBrainStrategyMetadata,
  resolveBrainModuleSelection,
  type BrainModule,
  type BrainModuleSelection,
} from "./brain-module.js";
import { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
import { nextCronDueAt } from "./cron-expression.js";
import { createDenRouterPiAgentFactory } from "./den-router-agent.js";
import {
  denseProfileMemoryTool,
  type DenseProfileMemoryMode,
} from "./dense-profile-memory-tool.js";
import { resolveDenMemoryTools } from "./den-memory-tools.js";
import { resolveDelegationTools } from "./delegation-tools.js";
import { resolveLoreMemoryTools } from "./lore-memory-tool.js";
import type { BrainImplementation } from "./index.js";
import { resolveLocalCodeTools } from "./local-code-tools.js";
import { createMemorySpaceToolResolver } from "./memory-space-api.js";
import type { PiAgentFactory } from "./pi-agent-brain.js";
import { providerStateScopeForProfile } from "./provider-state-fingerprints.js";
import {
  channelReadbackTool,
  counterResetTool,
  curatorExecuteTool,
  type CuratorExecuteContext,
  FileSessionTodoStore,
  MemorySessionTodoStore,
  sessionSearchTool,
  type SessionTodoStore,
  todoTool,
} from "./planning-tools.js";
import {
  loadProfileConfig,
  loadProfileContext,
  sessionMemoryPromptConfig,
  type ProfileConfig,
  type SessionMemoryPromptConfig,
} from "./profile-loading.js";
import {
  buildServiceMcpToolCatalog,
  buildServiceMcpEndpointConfig,
  createServiceMcpToolResolver,
  type ServiceMcpToolCatalog,
  type ServiceMcpToolDiscoveryClientFactory,
  type ServiceMcpToolExecutorFactory,
} from "./service-mcp-tools.js";
import type {
  RustyCrewMcpServerConfig,
  RustyCrewServiceConfig,
  RustyCrewStorageBackend,
  RustyCrewStorageConfig,
} from "./service-config.js";
import { RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND } from "./scheduled-host-executors.js";
import { planRuntimeConfigWithRust } from "./runtime-config-validation.js";
import {
  createSkillsToolResolver,
  type SkillManageMode,
} from "./skills-tools.js";
import {
  combineResolvers,
  type BrainToolResolver,
} from "./tool-session-selection.js";
import { createWebToolResolver } from "./web-tools.js";
import type { RuntimeBrainModuleDiagnostics } from "./runtime-diagnostics.js";

export interface RustyCrewConfiguredBrain {
  implementationId: BrainImplementationId;
  profileId: ProfileId;
}

export interface RustyCrewConfiguredSession {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  kind: SessionKind;
  resourceLimits?: ResourceLimits;
  toolProfile?: ToolProfile;
  ownerId?: string;
  maxHistoryMessages?: number;
  turnTimeoutMs?: number;
  sessionMemoryPrompt?: SessionMemoryPromptConfig;
}

export interface EffectiveSessionDefaults {
  ownerId?: string;
  maxHistoryMessages?: number;
  turnTimeoutMs?: number;
}

export type RustyCrewScheduledJobShape =
  | "host_job"
  | "session_wake"
  | "script_only"
  | "data_collection";

export interface RustyCrewScheduledJob {
  id: string;
  schedule: string;
  shape: RustyCrewScheduledJobShape;
  jobKind?: string;
  targetSessionId?: SessionId;
  payload?: unknown;
  script?: string;
  deliveryChannelId?: string;
}

export interface RustyCrewRuntimeConfig {
  profilesDir: string;
  skillsDir?: string;
  storage?: RustyCrewStorageConfig;
  brains: RustyCrewConfiguredBrain[];
  sessions: RustyCrewConfiguredSession[];
  scheduledJobs: RustyCrewScheduledJob[];
  channelBindings: ChannelBindingRecord[];
  mcpServers?: RustyCrewMcpServerConfig[];
  mcpBindings: McpBindingRecord[];
}

export interface RustyCrewRuntimeConfigApplyResult {
  brainsRegistered: number;
  brainsAlreadyPresent: number;
  sessionsCreated: number;
  sessionsAlreadyPresent: number;
  sessionsReactivated: number;
  sessionsMissing: number;
  scheduledJobsRegistered: number;
  brainHandlesByProfileId: Record<string, BrainImplementationHandle>;
  brainModulesByProfileId: Record<string, BrainModuleSelection>;
  brainDiagnosticsByProfileId: Record<string, RuntimeBrainModuleDiagnostics>;
}

export interface RustyCrewBrainRuntimeRebuildResult {
  profileId: ProfileId;
  implementationId: BrainImplementationId;
  handle: BrainImplementationHandle;
  module: BrainModuleSelection;
  diagnostics: RuntimeBrainModuleDiagnostics;
}

export interface ScheduledJobRegistrationResult {
  registered: number;
  jobs: ScheduledJobSummary[];
}

export interface RuntimeConfigValidationPreflightReport {
  ok: boolean;
  configPath: string;
  profilesDir?: string;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  summary: {
    diagnostics: number;
    errors: number;
    warnings: number;
    brains: number;
    sessions: number;
    scheduledJobs: number;
    channelBindings: number;
    mcpBindings: number;
    derivedScheduledJobs: number;
    derivedMcpBindings: number;
    sessionDefaultsApplied: number;
  };
  derived: {
    scheduledJobs: Array<{
      id: string;
      shape: RustyCrewScheduledJobShape;
      jobKind?: string;
      targetSessionId?: string;
    }>;
    mcpBindings: Array<{
      bindingId: string;
      agentId: string;
      sessionId?: string;
      profileId: string;
      transport: string;
      toolProfileKey: string;
      serverNames: string[];
    }>;
    sessionDefaultsApplied: Array<{
      sessionId: string;
      ownerId: boolean;
      resourceLimits: boolean;
      maxHistoryMessages: boolean;
      turnTimeoutMs: boolean;
    }>;
  };
}

export async function loadRustyCrewRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): Promise<RustyCrewRuntimeConfig> {
  let raw: string;
  try {
    raw = await readFile(serviceConfig.paths.serviceConfigFile, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return emptyRuntimeConfig(serviceConfig);
    }
    throw error;
  }

  const parsed = JSON.parse(raw) as unknown;
  return expandRuntimeConfigFromProfiles(
    validateRuntimeConfig(parsed, serviceConfig),
  );
}

export async function preflightRustyCrewRuntimeConfig(input: {
  serviceConfig: RustyCrewServiceConfig;
  bridge?: Pick<NativeBridgeModule, "planRuntimeConfig">;
}): Promise<RuntimeConfigValidationPreflightReport> {
  const configPath = input.serviceConfig.paths.serviceConfigFile;
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(configPath, "utf8")) as unknown;
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      parsed = {};
    } else {
      return preflightFailure(configPath, "invalid_runtime_config_json", error);
    }
  }

  let runtimeConfig: RustyCrewRuntimeConfig;
  try {
    runtimeConfig = validateRuntimeConfig(parsed, input.serviceConfig);
  } catch (error) {
    return preflightFailure(configPath, "invalid_runtime_config_shape", error);
  }

  const loadedProfiles = await loadRuntimeProfilesForValidation(runtimeConfig);
  if (loadedProfiles.diagnostics.length > 0) {
    const emptyPlan: NativeRuntimeConfigPlan = {
      runtimeConfig: runtimeConfigValidationInputShape(runtimeConfig),
      diagnostics: loadedProfiles.diagnostics,
      derivedScheduledJobs: [],
      derivedMcpBindings: [],
    };
    return preflightReport(configPath, runtimeConfig, emptyPlan);
  }

  const bridge = input.bridge ?? (await loadNativeBridge());
  const plan = await planRuntimeConfigWithRust({
    bridge,
    runtimeConfig,
    profiles: loadedProfiles.profiles,
  });
  return preflightReport(configPath, runtimeConfig, plan);
}

async function expandRuntimeConfigFromProfiles(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<RustyCrewRuntimeConfig> {
  const profiles = await loadRuntimeProfiles(runtimeConfig);
  const bridge = await loadNativeBridge();
  const plan = await planRuntimeConfigWithRust({
    bridge,
    runtimeConfig,
    profiles,
  });
  assertRuntimeConfigPlan(plan.diagnostics);
  return runtimeConfigFromNativeDraft(
    plan.runtimeConfig,
    runtimeConfig,
    profiles,
  );
}

async function loadRuntimeProfiles(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<ProfileConfig[]> {
  const profileIds = new Set<ProfileId>();
  for (const session of runtimeConfig.sessions) {
    profileIds.add(session.profileId);
  }
  const profiles: ProfileConfig[] = [];
  for (const profileId of profileIds) {
    profiles.push(
      await loadProfileConfig(runtimeConfig.profilesDir, profileId),
    );
  }
  return profiles;
}

async function loadRuntimeProfilesForValidation(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<{
  profiles: ProfileConfig[];
  diagnostics: NativeRuntimeConfigDiagnostic[];
}> {
  const profileIds = new Set<ProfileId>();
  for (const brain of runtimeConfig.brains) {
    profileIds.add(brain.profileId);
  }
  for (const session of runtimeConfig.sessions) {
    profileIds.add(session.profileId);
  }
  const profiles: ProfileConfig[] = [];
  const diagnostics: NativeRuntimeConfigDiagnostic[] = [];
  for (const profileId of profileIds) {
    try {
      profiles.push(
        await loadProfileConfig(runtimeConfig.profilesDir, profileId),
      );
    } catch (error) {
      diagnostics.push({
        severity: "error",
        code: "profile_metadata_load_failed",
        path: `profiles.${profileId}`,
        message: errorMessage(
          error,
          `profile ${profileId} could not be loaded`,
        ),
      });
    }
  }
  return { profiles, diagnostics };
}

function preflightFailure(
  configPath: string,
  code: string,
  error: unknown,
): RuntimeConfigValidationPreflightReport {
  const diagnostic = {
    severity: "error",
    code,
    path: "serviceConfig",
    message: errorMessage(error, "runtime config preflight failed"),
  } satisfies NativeRuntimeConfigDiagnostic;
  return {
    ok: false,
    configPath,
    diagnostics: [diagnostic],
    summary: {
      diagnostics: 1,
      errors: 1,
      warnings: 0,
      brains: 0,
      sessions: 0,
      scheduledJobs: 0,
      channelBindings: 0,
      mcpBindings: 0,
      derivedScheduledJobs: 0,
      derivedMcpBindings: 0,
      sessionDefaultsApplied: 0,
    },
    derived: {
      scheduledJobs: [],
      mcpBindings: [],
      sessionDefaultsApplied: [],
    },
  };
}

function preflightReport(
  configPath: string,
  original: RustyCrewRuntimeConfig,
  plan: NativeRuntimeConfigPlan,
): RuntimeConfigValidationPreflightReport {
  const diagnostics = plan.diagnostics;
  const errors = diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  ).length;
  const warnings = diagnostics.filter(
    (diagnostic) => diagnostic.severity === "warning",
  ).length;
  const defaultsApplied = sessionDefaultsApplied(original, plan.runtimeConfig);
  return {
    ok: errors === 0,
    configPath,
    profilesDir: original.profilesDir,
    diagnostics,
    summary: {
      diagnostics: diagnostics.length,
      errors,
      warnings,
      brains: plan.runtimeConfig.brains.length,
      sessions: plan.runtimeConfig.sessions.length,
      scheduledJobs: plan.runtimeConfig.scheduledJobs.length,
      channelBindings: plan.runtimeConfig.channelBindings.length,
      mcpBindings: plan.runtimeConfig.mcpBindings.length,
      derivedScheduledJobs: plan.derivedScheduledJobs.length,
      derivedMcpBindings: plan.derivedMcpBindings.length,
      sessionDefaultsApplied: defaultsApplied.length,
    },
    derived: {
      scheduledJobs: plan.derivedScheduledJobs.map((job) => ({
        id: job.id,
        shape: job.shape,
        jobKind: job.jobKind,
        targetSessionId: job.targetSessionId,
      })),
      mcpBindings: plan.derivedMcpBindings.map((binding) => ({
        bindingId: binding.bindingId,
        agentId: binding.agentId,
        sessionId: binding.sessionId,
        profileId: binding.profileId,
        transport: binding.transport,
        toolProfileKey: binding.toolProfileKey,
        serverNames: binding.serverNames,
      })),
      sessionDefaultsApplied: defaultsApplied,
    },
  };
}

function sessionDefaultsApplied(
  original: RustyCrewRuntimeConfig,
  planned: NativeRuntimeConfigDraft,
): RuntimeConfigValidationPreflightReport["derived"]["sessionDefaultsApplied"] {
  const originalSessions = new Map(
    original.sessions.map((session) => [session.sessionId, session]),
  );
  return planned.sessions
    .map((plannedSession) => {
      const originalSession = originalSessions.get(
        plannedSession.sessionId as SessionId,
      );
      if (!originalSession) return undefined;
      const applied = {
        sessionId: plannedSession.sessionId,
        ownerId:
          originalSession.ownerId === undefined &&
          plannedSession.ownerId !== undefined,
        resourceLimits:
          originalSession.resourceLimits === undefined &&
          plannedSession.resourceLimits !== undefined,
        maxHistoryMessages:
          originalSession.maxHistoryMessages === undefined &&
          plannedSession.maxHistoryMessages !== undefined,
        turnTimeoutMs:
          originalSession.turnTimeoutMs === undefined &&
          plannedSession.turnTimeoutMs !== undefined,
      };
      return applied.ownerId ||
        applied.resourceLimits ||
        applied.maxHistoryMessages ||
        applied.turnTimeoutMs
        ? applied
        : undefined;
    })
    .filter(
      (
        value,
      ): value is RuntimeConfigValidationPreflightReport["derived"]["sessionDefaultsApplied"][number] =>
        value !== undefined,
    );
}

function runtimeConfigValidationInputShape(
  runtimeConfig: RustyCrewRuntimeConfig,
): NativeRuntimeConfigDraft {
  return {
    profilesDir: runtimeConfig.profilesDir,
    skillsDir: runtimeConfig.skillsDir,
    brains: runtimeConfig.brains,
    sessions: runtimeConfig.sessions,
    scheduledJobs: runtimeConfig.scheduledJobs,
    channelBindings: runtimeConfig.channelBindings,
    mcpBindings: runtimeConfig.mcpBindings,
  };
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

function assertRuntimeConfigPlan(
  diagnostics: readonly { severity: string; path?: string; message: string }[],
): void {
  const errors = diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (errors.length === 0) {
    return;
  }
  const first = errors[0]!;
  const suffix =
    errors.length === 1
      ? ""
      : ` (${errors.length - 1} additional diagnostic${errors.length === 2 ? "" : "s"})`;
  throw new Error(
    `${first.path ? `${first.path}: ` : ""}${first.message}${suffix}`,
  );
}

function backgroundReviewScheduledJob(
  profile: Awaited<ReturnType<typeof loadProfileConfig>>,
): RustyCrewScheduledJob {
  const review = profile.backgroundReview;
  const profileId = profile.profileId;
  return {
    id: `background-review-${profileId}`,
    schedule: review?.schedule ?? "0 3 * * *",
    shape: "host_job",
    jobKind: RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND,
    payload: {
      schemaVersion: 1,
      reviewType: review?.reviewType ?? "combined",
      profileId,
      triggerSource: "profile_background_review",
      includeDenseProfileMemory: true,
      includeDenMemoryDiagnostics: true,
      memoryNudgeInterval: review?.memoryNudgeInterval,
      skillNudgeInterval: review?.skillNudgeInterval,
      maxTokens: review?.maxTokens,
      maxFindings: review?.maxFindings,
      maxCandidates: review?.maxCandidates,
      llmReviewEnabled: review?.llmReviewEnabled ?? false,
      captureProviderAlias: review?.captureProviderAlias,
      captureMaxProposals: review?.captureMaxProposals,
      dryRun: review?.dryRun ?? true,
      reason: `profile ${profileId} backgroundReview`,
    },
  };
}

function runtimeConfigFromNativeDraft(
  draft: NativeRuntimeConfigDraft,
  original: RustyCrewRuntimeConfig,
  profiles: readonly ProfileConfig[],
): RustyCrewRuntimeConfig {
  const originalSessions = new Map(
    original.sessions.map((session) => [session.sessionId, session]),
  );
  const originalScheduledJobs = new Map(
    original.scheduledJobs.map((job) => [job.id, job]),
  );
  const originalChannelBindings = new Map(
    original.channelBindings.map((binding) => [binding.bindingId, binding]),
  );
  const originalMcpBindings = new Map(
    original.mcpBindings.map((binding) => [binding.bindingId, binding]),
  );
  const profilesById = new Map(
    profiles.map((profile) => [profile.profileId, profile]),
  );
  return {
    profilesDir: draft.profilesDir,
    skillsDir: draft.skillsDir,
    storage: original.storage,
    brains: draft.brains.map((brain) => ({
      implementationId: brain.implementationId as BrainImplementationId,
      profileId: brain.profileId as ProfileId,
    })),
    sessions: draft.sessions.map((session) => ({
      ...originalSessions.get(session.sessionId as SessionId),
      sessionId: session.sessionId as SessionId,
      agentId: session.agentId as AgentId,
      profileId: session.profileId as ProfileId,
      kind: session.kind,
      resourceLimits: session.resourceLimits,
      ownerId: session.ownerId,
      maxHistoryMessages:
        session.maxHistoryMessages ?? session.historyWindow?.maxMessages,
      turnTimeoutMs: session.turnTimeoutMs,
    })),
    scheduledJobs: draft.scheduledJobs.map((job) => {
      const originalJob = originalScheduledJobs.get(job.id);
      return {
        ...originalJob,
        id: job.id,
        schedule: job.schedule,
        shape: job.shape,
        jobKind: job.jobKind,
        targetSessionId: job.targetSessionId as SessionId | undefined,
        script: job.script,
        deliveryChannelId: job.deliveryChannelId,
        payload:
          originalJob?.payload ??
          backgroundReviewPayloadForJob(job.id, profilesById),
      };
    }),
    channelBindings: draft.channelBindings.map((binding) => ({
      ...originalChannelBindings.get(binding.bindingId),
      bindingId: binding.bindingId,
      adapterId: binding.adapterId as never,
      provider: binding.provider,
      agentId: binding.agentId as AgentId,
      instanceId: binding.instanceId as never,
      sessionId: binding.sessionId as SessionId | undefined,
      profileId: binding.profileId as ProfileId,
      externalChannelId: binding.externalChannelId,
      externalThreadId: binding.externalThreadId,
      externalUserId: binding.externalUserId,
      conversationProjectId: binding.conversationProjectId,
      conversationChannelId: binding.conversationChannelId,
      providerSubscriptionId: binding.providerSubscriptionId,
      status: binding.status,
    })),
    mcpServers: original.mcpServers ?? [],
    mcpBindings: draft.mcpBindings.map((binding) => ({
      ...originalMcpBindings.get(binding.bindingId),
      bindingId: binding.bindingId,
      adapterId: binding.adapterId as never,
      agentId: binding.agentId as AgentId,
      instanceId: binding.instanceId as never,
      sessionId: binding.sessionId as SessionId | undefined,
      profileId: binding.profileId as ProfileId,
      serverNames: binding.serverNames,
      endpointRef: binding.endpointRef,
      transport: binding.transport,
      toolProfileKey: binding.toolProfileKey,
      status: binding.status,
      diagnostics:
        originalMcpBindings.get(binding.bindingId)?.diagnostics ?? {},
    })),
  };
}

function backgroundReviewPayloadForJob(
  jobId: string,
  profilesById: ReadonlyMap<ProfileId, ProfileConfig>,
): unknown {
  const prefix = "background-review-";
  if (!jobId.startsWith(prefix)) {
    return undefined;
  }
  const profile = profilesById.get(jobId.slice(prefix.length) as ProfileId);
  return profile ? backgroundReviewScheduledJob(profile).payload : undefined;
}

export async function applyRustyCrewRuntimeConfig(input: {
  serviceConfig: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  bridge: NativeBridgeModule;
  existingBrainHandlesByProfileId?: Record<string, BrainImplementationHandle>;
  createMissingSessions?: boolean;
  createDenRouterAgentFactory?: (
    options: Parameters<typeof createDenRouterPiAgentFactory>[0],
  ) => Promise<PiAgentFactory>;
  curatorExecutor?: CuratorExecuteContext["executor"];
  mcpSurfaceDiagnostics?: readonly McpSurfaceDiagnostics[];
  mcpToolDiscoveryClientFactory?: ServiceMcpToolDiscoveryClientFactory;
  mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
}): Promise<RustyCrewRuntimeConfigApplyResult> {
  const runtimeConfig = await expandRuntimeConfigFromProfiles(
    input.runtimeConfig,
  );
  const createMissingSessions = input.createMissingSessions ?? true;
  const mcpToolCatalog = await buildServiceMcpToolCatalog({
    runtimeConfig,
    mcpConfig: input.serviceConfig.mcp,
    discoveryClientFactory: input.mcpToolDiscoveryClientFactory,
    surfaceDiagnostics: input.mcpSurfaceDiagnostics,
  });
  const profileContexts = new Map<
    ProfileId,
    Awaited<ReturnType<typeof loadProfileContext>>
  >();
  const loadProfile = async (profileId: ProfileId) => {
    const existing = profileContexts.get(profileId);
    if (existing !== undefined) return existing;
    const profile = await loadProfileContext({
      profilesDir: runtimeConfig.profilesDir,
      skillsDir: runtimeConfig.skillsDir,
      profileId,
      modelProviderResolver: (alias) =>
        resolveModelProviderForBrain(input.bridge, alias),
      registry: mcpToolCatalog.registryForProfile(profileId),
      extraRequestedToolsets: mcpToolCatalog.toolsetsForProfile(profileId),
      catalogId:
        mcpToolCatalog.toolsetsForProfile(profileId).length > 0
          ? `service:mcp:${profileId}`
          : undefined,
    });
    profileContexts.set(profileId, profile);
    return profile;
  };
  const result: RustyCrewRuntimeConfigApplyResult = {
    brainsRegistered: 0,
    brainsAlreadyPresent: 0,
    sessionsCreated: 0,
    sessionsAlreadyPresent: 0,
    sessionsReactivated: 0,
    sessionsMissing: 0,
    scheduledJobsRegistered: 0,
    brainHandlesByProfileId: {},
    brainModulesByProfileId: {},
    brainDiagnosticsByProfileId: {},
  };

  const brainModuleRegistry = createBrainModuleRegistry();
  for (const brain of runtimeConfig.brains) {
    const profile = await loadProfile(brain.profileId);
    const selection = resolveBrainModuleSelection(profile.profile);
    const module = brainModuleRegistry.require(selection.moduleId);
    const moduleStrategy = resolveBrainModuleStrategy(module, selection);
    const strategy = brainStrategyMetadataForModuleStrategy(
      module,
      moduleStrategy,
    );
    const providerStateScope = providerStateScopeForProfile({
      profile,
      strategy,
      moduleStrategy,
    });
    result.brainModulesByProfileId[brain.profileId] = selection;
    result.brainDiagnosticsByProfileId[brain.profileId] =
      brainModuleDiagnostics({
        profile,
        implementationId: brain.implementationId,
        selection,
        strategy,
        moduleStrategy,
        module,
      });
    try {
      const handle = await input.bridge.registerBrainRuntime(
        {
          implementationId: brain.implementationId,
          profileId: brain.profileId,
          toolProfile: profile.toolSelection.toolProfile,
          modelConfig: profile.profile.modelConfig,
          strategy,
          providerStateScope,
        },
        toBridgeWakeExecutor(
          await createConfiguredBrain(module, profile, {
            createDenRouterAgentFactory: input.createDenRouterAgentFactory,
            bridge: input.bridge,
            providerStateScope,
            runtimeConfig,
            serviceConfig: input.serviceConfig,
            curatorExecutor: input.curatorExecutor,
            mcpToolCatalog,
            mcpToolExecutorFactory: input.mcpToolExecutorFactory,
          }),
        ),
      );
      result.brainHandlesByProfileId[brain.profileId] = handle;
      result.brainsRegistered += 1;
    } catch (error) {
      if (!isAlreadyPresentError(error)) throw error;
      const existingHandle =
        input.existingBrainHandlesByProfileId?.[brain.profileId];
      if (existingHandle !== undefined) {
        result.brainHandlesByProfileId[brain.profileId] = existingHandle;
      }
      result.brainsAlreadyPresent += 1;
    }
  }

  const existingSessionsById = new Map(
    (await input.bridge.listSessions()).map((session) => [
      session.sessionId,
      session,
    ]),
  );
  for (const session of runtimeConfig.sessions) {
    const profile = await loadProfile(session.profileId);
    const configuredSession = sessionWithProfileDefaults(session, profile);
    const existing = existingSessionsById.get(session.sessionId);
    if (!existing && !createMissingSessions) {
      result.sessionsMissing += 1;
      continue;
    }
    const ensured = await input.bridge.ensureConfiguredSession(
      nativeSessionConfig(configuredSession),
    );
    if (!existing) {
      result.sessionsCreated += 1;
    } else if (
      existing.status === "archived" &&
      ensured.status !== "archived"
    ) {
      result.sessionsReactivated += 1;
    } else {
      result.sessionsAlreadyPresent += 1;
    }
  }

  const scheduledJobs = await registerConfiguredScheduledJobs({
    bridge: input.bridge,
    runtimeConfig,
  });
  result.scheduledJobsRegistered = scheduledJobs.registered;

  return result;
}

async function resolveModelProviderForBrain(
  bridge: NativeBridgeModule,
  alias: string,
): Promise<BrainModelConfig> {
  const provider = await bridge.getModelProvider(alias);
  if (provider === undefined) {
    throw new Error(`model provider alias ${alias} was not found`);
  }
  if (provider.status !== "active") {
    throw new Error(
      `model provider alias ${alias} is ${provider.status}; active provider required`,
    );
  }
  const secret = provider.credential.hasSecret
    ? await bridge.getModelProviderSecret(alias)
    : undefined;
  return modelProviderToBrainModelConfig(provider, secret);
}

function modelProviderToBrainModelConfig(
  provider: NativeModelProviderRecord,
  secret: string | undefined,
): BrainModelConfig {
  const apiKeyEnv =
    secret === undefined
      ? undefined
      : modelProviderSecretEnvName(provider.alias);
  if (apiKeyEnv !== undefined) {
    process.env[apiKeyEnv] = secret;
  }
  return {
    provider: provider.providerKind,
    modelName: provider.modelId,
    baseUrl: provider.baseUrl,
    api:
      provider.protocol === "responses"
        ? "openai-responses"
        : "openai-completions",
    apiKeyEnv,
    temperatureMilli: provider.temperatureMilli,
    maxOutputTokens: provider.maxOutputTokens,
  };
}

function modelProviderSecretEnvName(alias: string): string {
  return `RUSTY_CREW_MODEL_PROVIDER_SECRET_${alias
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")}`;
}

export async function rebuildConfiguredBrainRuntime(input: {
  serviceConfig: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  profileId: ProfileId;
  bridge: NativeBridgeModule;
  createDenRouterAgentFactory?: (
    options: Parameters<typeof createDenRouterPiAgentFactory>[0],
  ) => Promise<PiAgentFactory>;
  curatorExecutor?: CuratorExecuteContext["executor"];
  mcpSurfaceDiagnostics?: readonly McpSurfaceDiagnostics[];
  mcpToolDiscoveryClientFactory?: ServiceMcpToolDiscoveryClientFactory;
  mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
}): Promise<RustyCrewBrainRuntimeRebuildResult> {
  const runtimeConfig = await expandRuntimeConfigFromProfiles(
    input.runtimeConfig,
  );
  const brain = runtimeConfig.brains.find(
    (candidate) => candidate.profileId === input.profileId,
  );
  if (brain === undefined) {
    throw new Error(`profile ${input.profileId} is not configured for a brain`);
  }

  const mcpToolCatalog = await buildServiceMcpToolCatalog({
    runtimeConfig,
    mcpConfig: input.serviceConfig.mcp,
    discoveryClientFactory: input.mcpToolDiscoveryClientFactory,
    surfaceDiagnostics: input.mcpSurfaceDiagnostics,
  });
  const profile = await loadProfileContext({
    profilesDir: runtimeConfig.profilesDir,
    skillsDir: runtimeConfig.skillsDir,
    profileId: input.profileId,
    modelProviderResolver: (alias) =>
      resolveModelProviderForBrain(input.bridge, alias),
    registry: mcpToolCatalog.registryForProfile(input.profileId),
    extraRequestedToolsets: mcpToolCatalog.toolsetsForProfile(input.profileId),
    catalogId:
      mcpToolCatalog.toolsetsForProfile(input.profileId).length > 0
        ? `service:mcp:${input.profileId}`
        : undefined,
  });
  const brainModuleRegistry = createBrainModuleRegistry();
  const selection = resolveBrainModuleSelection(profile.profile);
  const module = brainModuleRegistry.require(selection.moduleId);
  const moduleStrategy = resolveBrainModuleStrategy(module, selection);
  const strategy = brainStrategyMetadataForModuleStrategy(
    module,
    moduleStrategy,
  );
  const providerStateScope = providerStateScopeForProfile({
    profile,
    strategy,
    moduleStrategy,
  });
  const handle = await input.bridge.replaceBrainRuntime(
    {
      implementationId: brain.implementationId,
      profileId: brain.profileId,
      toolProfile: profile.toolSelection.toolProfile,
      modelConfig: profile.profile.modelConfig,
      strategy,
      providerStateScope,
    },
    toBridgeWakeExecutor(
      await createConfiguredBrain(module, profile, {
        createDenRouterAgentFactory: input.createDenRouterAgentFactory,
        bridge: input.bridge,
        providerStateScope,
        runtimeConfig,
        serviceConfig: input.serviceConfig,
        curatorExecutor: input.curatorExecutor,
        mcpToolCatalog,
        mcpToolExecutorFactory: input.mcpToolExecutorFactory,
      }),
    ),
  );

  return {
    profileId: brain.profileId,
    implementationId: brain.implementationId,
    handle,
    module: selection,
    diagnostics: brainModuleDiagnostics({
      profile,
      implementationId: brain.implementationId,
      selection,
      strategy,
      moduleStrategy,
      module,
    }),
  };
}

export async function registerConfiguredScheduledJobs(input: {
  bridge: Pick<
    NativeBridgeModule,
    "registerScheduledWakeJob" | "registerScheduledHostJob"
  >;
  runtimeConfig: RustyCrewRuntimeConfig;
  now?: () => string;
}): Promise<ScheduledJobRegistrationResult> {
  const now = input.now ?? (() => new Date().toISOString());
  const jobs: ScheduledJobSummary[] = [];
  for (const job of input.runtimeConfig.scheduledJobs) {
    if (job.shape === "session_wake") {
      if (!job.targetSessionId) {
        throw new Error(
          `scheduled job ${job.id} requires targetSessionId for session_wake`,
        );
      }
      jobs.push(
        await input.bridge.registerScheduledWakeJob({
          jobId: job.id,
          targetSessionId: job.targetSessionId,
          firstDueAt: nextCronDueAt(job.schedule, now()),
        }),
      );
      continue;
    }
    if (job.shape === "host_job") {
      if (!job.jobKind) {
        throw new Error(
          `scheduled job ${job.id} requires jobKind for host_job`,
        );
      }
      jobs.push(
        await input.bridge.registerScheduledHostJob({
          jobId: job.id,
          jobKind: job.jobKind,
          firstDueAt: nextCronDueAt(job.schedule, now()),
          payload: job.payload ?? {},
        }),
      );
      continue;
    }
    throw new Error(
      `scheduled job ${job.id} shape ${job.shape} is not executable in Rusty Crew v1`,
    );
  }
  return { registered: jobs.length, jobs };
}

export function sessionWithProfileDefaults(
  session: RustyCrewConfiguredSession,
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
): RustyCrewConfiguredSession {
  const defaults = effectiveSessionDefaults(session, profile.profile);
  return {
    ...session,
    resourceLimits:
      session.resourceLimits ??
      profile.profile.runtime?.defaultResourceLimits ??
      undefined,
    toolProfile: session.toolProfile ?? profile.toolSelection.toolProfile,
    ...defaults,
  };
}

export function effectiveSessionDefaults(
  session: Pick<
    RustyCrewConfiguredSession,
    "ownerId" | "maxHistoryMessages" | "turnTimeoutMs"
  >,
  profile: Pick<ProfileConfig, "sessionDefaults">,
): EffectiveSessionDefaults {
  return definedDefaults({
    ownerId: session.ownerId ?? profile.sessionDefaults?.ownerId,
    maxHistoryMessages:
      session.maxHistoryMessages ?? profile.sessionDefaults?.maxHistoryMessages,
    turnTimeoutMs:
      session.turnTimeoutMs ?? profile.sessionDefaults?.turnTimeoutMs,
  });
}

export function effectiveWakeTimeoutMs(input: {
  session?: Pick<RustyCrewConfiguredSession, "turnTimeoutMs">;
  profile: Pick<ProfileConfig, "runtime" | "sessionDefaults">;
}): number | undefined {
  return (
    input.session?.turnTimeoutMs ??
    input.profile.runtime?.maxTurnDurationMs ??
    input.profile.sessionDefaults?.turnTimeoutMs
  );
}

function definedDefaults(
  defaults: EffectiveSessionDefaults,
): EffectiveSessionDefaults {
  return Object.fromEntries(
    Object.entries(defaults).filter(([, value]) => value !== undefined),
  ) as EffectiveSessionDefaults;
}

function nativeSessionConfig(session: RustyCrewConfiguredSession): {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  kind: SessionKind;
  resourceLimits?: ResourceLimits;
  toolProfile?: ToolProfile;
  historyWindow?: { maxMessages?: number };
} {
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    resourceLimits: session.resourceLimits,
    toolProfile: session.toolProfile,
    historyWindow:
      session.maxHistoryMessages === undefined
        ? undefined
        : { maxMessages: session.maxHistoryMessages },
  };
}

export function configuredSessionForChannelBinding(
  runtimeConfig: RustyCrewRuntimeConfig,
  binding: ChannelBindingRecord,
): RustyCrewConfiguredSession | undefined {
  const matchingSessions =
    binding.sessionId === undefined
      ? runtimeConfig.sessions.filter(
          (session) => session.agentId === binding.agentId,
        )
      : runtimeConfig.sessions.filter(
          (session) => session.sessionId === binding.sessionId,
        );

  if (matchingSessions.length === 0) return undefined;
  if (matchingSessions.length > 1) {
    throw new Error(
      `channel binding ${binding.bindingId} matches multiple configured sessions for agent ${binding.agentId}`,
    );
  }

  const session = matchingSessions[0]!;
  if (session.agentId !== binding.agentId) {
    throw new Error(
      `channel binding ${binding.bindingId} targets agent ${binding.agentId} but configured session ${session.sessionId} belongs to ${session.agentId}`,
    );
  }
  if (session.profileId !== binding.profileId) {
    throw new Error(
      `channel binding ${binding.bindingId} targets profile ${binding.profileId} but configured session ${session.sessionId} uses ${session.profileId}`,
    );
  }
  return session;
}

export async function ensureConfiguredSessionForChannelBinding(input: {
  bridge: Pick<NativeBridgeModule, "ensureConfiguredSession">;
  runtimeConfig: RustyCrewRuntimeConfig;
  binding: ChannelBindingRecord;
}): Promise<NativeSessionStateSummary> {
  const session = configuredSessionForChannelBinding(
    input.runtimeConfig,
    input.binding,
  );
  if (session === undefined) {
    throw new Error(
      `channel binding ${input.binding.bindingId} has no matching configured session`,
    );
  }
  return input.bridge.ensureConfiguredSession(nativeSessionConfig(session));
}

function brainModuleDiagnostics(input: {
  profile: Awaited<ReturnType<typeof loadProfileContext>>;
  implementationId: BrainImplementationId;
  selection: BrainModuleSelection;
  strategy: ReturnType<typeof resolveBrainStrategyMetadata>;
  moduleStrategy: ReturnType<typeof resolveBrainModuleStrategy>;
  module: BrainModule;
}): RuntimeBrainModuleDiagnostics {
  return {
    profileId: input.profile.profile.profileId,
    implementationId: input.implementationId,
    moduleId: input.selection.moduleId,
    ...(input.selection.strategy === undefined
      ? {}
      : { strategy: input.selection.strategy }),
    effectiveStrategy: input.strategy.strategyId,
    ...(input.profile.profile.providerAlias === undefined
      ? {}
      : { providerAlias: input.profile.profile.providerAlias }),
    modelProvider: {
      providerKind: input.profile.profile.modelConfig.provider,
      protocol:
        input.profile.profile.modelConfig.api === "openai-responses"
          ? "responses"
          : "chat_completions",
      modelId: input.profile.profile.modelConfig.modelName,
      ...(input.profile.profile.modelConfig.baseUrl === undefined
        ? {}
        : { baseUrl: input.profile.profile.modelConfig.baseUrl }),
      ...(input.profile.profile.modelConfig.maxOutputTokens === undefined
        ? {}
        : {
            maxOutputTokens: input.profile.profile.modelConfig.maxOutputTokens,
          }),
      ...(input.profile.profile.modelConfig.temperatureMilli === undefined
        ? {}
        : {
            temperatureMilli:
              input.profile.profile.modelConfig.temperatureMilli,
          }),
      ...(input.profile.profile.modelConfig.apiKeyEnv === undefined
        ? { credential: { hasSecret: false } }
        : {
            credential: {
              hasSecret: true,
              secretRef: input.profile.profile.modelConfig.apiKeyEnv,
            },
          }),
    },
    providerStateMode: input.strategy.providerState.mode,
    providerStateRebuild: providerStateRebuildPolicyForModuleStrategy(
      input.moduleStrategy,
    ),
    ...(input.moduleStrategy.diagnostics === undefined
      ? {}
      : { strategyDiagnostics: input.moduleStrategy.diagnostics }),
    selectedToolCount: input.profile.toolSelection.toolProfile.tools.length,
    selectedToolSource: input.profile.toolSelection.catalogId,
    toolAdapterStatus: input.module.diagnostics.toolAdapterStatus,
  };
}

async function createConfiguredBrain(
  module: BrainModule,
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
  options: {
    createDenRouterAgentFactory?: (
      options: Parameters<typeof createDenRouterPiAgentFactory>[0],
    ) => Promise<PiAgentFactory>;
    bridge?: NativeBridgeModule;
    runtimeConfig?: RustyCrewRuntimeConfig;
    serviceConfig?: RustyCrewServiceConfig;
    providerStateScope?: BrainProviderStateScope;
    curatorExecutor?: CuratorExecuteContext["executor"];
    mcpToolCatalog?: ServiceMcpToolCatalog;
    mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
  } = {},
): Promise<BrainImplementation> {
  return module.createBrain({
    profile,
    bridge: options.bridge,
    providerStateScope: options.providerStateScope,
    runtimeConfig: options.runtimeConfig,
    serviceConfig: options.serviceConfig,
    toolResolver: createServiceToolResolver(profile, options),
    planActions: completionActionFromEvents,
    maxTokens: effectiveModelMaxTokens(profile),
    createDenRouterAgentFactory: options.createDenRouterAgentFactory,
  });
}

function effectiveModelMaxTokens(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
): number {
  const modelMaxTokens = profile.profile.modelConfig.maxOutputTokens ?? 128;
  const turnMaxTokens = profile.profile.runtime?.maxTokensPerTurn;
  if (turnMaxTokens === undefined) return modelMaxTokens;
  return Math.min(modelMaxTokens, turnMaxTokens);
}

function createServiceToolResolver(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
  options: {
    bridge?: NativeBridgeModule;
    runtimeConfig?: RustyCrewRuntimeConfig;
    serviceConfig?: RustyCrewServiceConfig;
    curatorExecutor?: CuratorExecuteContext["executor"];
    mcpToolCatalog?: ServiceMcpToolCatalog;
    mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
  },
): BrainToolResolver {
  const todoStore = createServiceTodoStore(options.serviceConfig);
  const browserManager = new BrowserSessionManager();
  const browserScreenshotStore = new MemoryBrowserScreenshotStore();
  return combineResolvers(
    resolveLocalCodeTools,
    createWebToolResolver({}),
    createBrowserToolResolver({
      manager: browserManager,
      screenshotStore: browserScreenshotStore,
    }),
    createMemoryToolResolver(profile, options),
    options.mcpToolCatalog
      ? createServiceMcpToolResolver({
          catalog: options.mcpToolCatalog,
          bridge: options.bridge,
          mcpConfig: buildServiceMcpEndpointConfig({
            mcpConfig: options.serviceConfig?.mcp,
            mcpServers: options.runtimeConfig?.mcpServers,
          }),
          executorFactory: options.mcpToolExecutorFactory,
        })
      : () => [],
    createSkillsToolResolver({
      skillsDir: serviceSkillsDir(profile, options.runtimeConfig),
      allowedSkills:
        profile.profile.skillsMode === "all"
          ? undefined
          : profile.profile.skills,
      manageMode: serviceSkillManageMode(profile),
    }),
    resolveDelegationTools,
    createPlanningToolResolver({
      bridge: options.bridge,
      runtimeConfig: options.runtimeConfig,
      curatorExecutor: options.curatorExecutor,
      todoStore,
    }),
  );
}

function createServiceTodoStore(
  serviceConfig: RustyCrewServiceConfig | undefined,
): SessionTodoStore {
  if (!serviceConfig) return new MemorySessionTodoStore();
  return new FileSessionTodoStore({
    rootDir: join(serviceConfig.paths.dataDir, "data", "session-todos"),
  });
}

function createMemoryToolResolver(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
  options: {
    bridge?: NativeBridgeModule;
    serviceConfig?: RustyCrewServiceConfig;
  },
): BrainToolResolver {
  const denMemoryClient = createServiceDenMemoryClient(options.serviceConfig);
  const memorySpaceResolver = options.bridge
    ? createMemorySpaceToolResolver({ bridge: options.bridge })
    : undefined;
  return (input) => [
    ...(memorySpaceResolver?.(input) ?? []),
    ...resolveDenMemoryTools({
      client: denMemoryClient,
      policy: {
        mode: "metadata",
        defaultAudience: [profile.profile.profileId],
      },
      runtimeContext: {
        projectId: options.serviceConfig?.denConversationProjectId,
      },
      session: input.wake.state.session,
    }),
    denseProfileMemoryTool({
      client: options.bridge,
      mode: denseProfileMemoryMode(profile),
      session: input.wake.state.session,
    }),
    ...resolveLoreMemoryTools({
      client: options.bridge,
      session: input.wake.state.session,
    }),
  ];
}

function createServiceDenMemoryClient(
  serviceConfig: RustyCrewServiceConfig | undefined,
): DenMemoryClient | undefined {
  const config = serviceConfig?.denMemory;
  if (!config?.baseUrl) return undefined;
  return createDenMemoryClient({
    baseUrl: config.baseUrl,
    bearerToken: config.bearerToken,
    apiMode: config.apiMode,
    timeoutMs: config.timeoutMs,
    paths: config.paths,
  });
}

function denseProfileMemoryMode(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
): DenseProfileMemoryMode {
  return profile.toolSelection.toolProfile.tools.some(
    (tool) => tool.name === "dense_profile_memory",
  )
    ? "read_write"
    : "read_only";
}

function createPlanningToolResolver(input: {
  bridge?: NativeBridgeModule;
  runtimeConfig?: RustyCrewRuntimeConfig;
  curatorExecutor?: CuratorExecuteContext["executor"];
  todoStore: SessionTodoStore;
}): BrainToolResolver {
  return ({ wake }) => {
    const session = wake.state.session;
    const allowedBindingIds = channelBindingIdsForSession(
      input.runtimeConfig,
      session.sessionId,
      session.agentId,
      session.profileId,
    );
    return [
      todoTool({ store: input.todoStore, sessionId: session.sessionId }),
      sessionSearchTool({ client: input.bridge }),
      channelReadbackTool({
        requester: {
          agentId: session.agentId,
          sessionId: session.sessionId,
          profileId: session.profileId,
        },
        allowedBindingIds,
      }),
      counterResetTool({ client: input.bridge }),
      curatorExecuteTool({
        executor: input.curatorExecutor,
        actorId: session.agentId,
        sessionId: session.sessionId,
        profileId: session.profileId,
      }),
    ];
  };
}

function channelBindingIdsForSession(
  runtimeConfig: RustyCrewRuntimeConfig | undefined,
  sessionId: SessionId,
  agentId: AgentId,
  profileId: ProfileId,
): string[] {
  return (runtimeConfig?.channelBindings ?? [])
    .filter(
      (binding) =>
        binding.status === "active" &&
        binding.agentId === agentId &&
        binding.profileId === profileId &&
        (binding.sessionId === undefined || binding.sessionId === sessionId),
    )
    .map((binding) => binding.bindingId);
}

function serviceSkillsDir(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
  runtimeConfig: RustyCrewRuntimeConfig | undefined,
): string | undefined {
  return (
    profile.profile.profileSkillsDir ??
    runtimeConfig?.skillsDir ??
    (runtimeConfig ? join(runtimeConfig.profilesDir, "skills") : undefined)
  );
}

function serviceSkillManageMode(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
): SkillManageMode {
  return profile.profile.toolPolicy?.requestedToolsets?.includes(
    "skills_manage",
  )
    ? "profile"
    : "off";
}

function toBridgeWakeExecutor(brain: BrainImplementation): BrainWakeExecutor {
  return {
    wake(request, buffers) {
      return wakeBrainFromBridgeRequest(buffers, brain, request);
    },
  };
}

function completionActionFromEvents(input: {
  wake: { sessionId: SessionId };
  events: BrainEventEnvelope[];
  toolActions?: readonly BrainAction[];
}): BrainAction[] {
  if (
    input.toolActions?.some((action) => action.type === "request_delegation")
  ) {
    return [];
  }
  const text = mergeTextDeltas(
    input.events.flatMap((event) =>
      event.event.type === "text_delta" ? [event.event.text] : [],
    ),
  ).trim();
  return [
    {
      type: "deliver_completion",
      packet: {
        sessionId: input.wake.sessionId,
        status: "completed",
        summary: text ? truncate(text, 480) : "LLM wake completed.",
      } satisfies CompletionPacket,
    },
  ];
}

function mergeTextDeltas(parts: readonly string[]): string {
  return parts
    .filter((part) => part.length > 0)
    .reduce((merged, part) => {
      if (!merged) return part;
      if (part.startsWith(merged)) return part;
      if (merged.endsWith(part)) return merged;
      return `${merged}${part}`;
    }, "");
}

function truncate(value: string, maxChars: number): string {
  return value.length <= maxChars ? value : `${value.slice(0, maxChars)}...`;
}

function emptyRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewRuntimeConfig {
  return {
    profilesDir: join(serviceConfig.paths.configDir, "profiles"),
    storage: serviceConfig.storage,
    brains: [],
    sessions: [],
    scheduledJobs: [],
    channelBindings: [],
    mcpServers: serviceConfig.mcp.servers,
    mcpBindings: [],
  };
}

function validateRuntimeConfig(
  parsed: unknown,
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewRuntimeConfig {
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  const profilesDir = pathValue(
    parsed.profilesDir,
    join(serviceConfig.paths.configDir, "profiles"),
  );
  const skillsDir =
    parsed.skillsDir === undefined ? undefined : pathValue(parsed.skillsDir);
  return {
    profilesDir,
    skillsDir,
    storage: runtimeStorageConfig(parsed.storage, serviceConfig),
    brains: arrayValue(parsed.brains).map((item, index) =>
      configuredBrain(item, index),
    ),
    sessions: arrayValue(parsed.sessions).map((item, index) =>
      configuredSession(item, index),
    ),
    scheduledJobs: arrayValue(parsed.scheduledJobs).map((item, index) =>
      configuredScheduledJob(item, index),
    ),
    channelBindings: arrayValue(parsed.channelBindings).map((item, index) =>
      configuredChannelBinding(item, index),
    ),
    mcpServers: optionalArrayValue(
      parsed.mcpServers,
      serviceConfig.mcp.servers,
    ).map((item, index) => configuredMcpServer(item, index)),
    mcpBindings: arrayValue(parsed.mcpBindings).map((item, index) =>
      configuredMcpBinding(item, index),
    ),
  };
}

function runtimeStorageConfig(
  input: unknown,
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewStorageConfig {
  if (input === undefined) return serviceConfig.storage;
  if (!isRecord(input)) {
    throw new Error("storage config must be an object");
  }
  const backend = runtimeStorageBackend(input.backend);
  const sqlite = isRecord(input.sqlite) ? input.sqlite : {};
  const postgres = isRecord(input.postgres) ? input.postgres : {};
  const sqlitePath =
    optionalString(sqlite.path) ?? serviceConfig.storage.sqlite.path;
  const postgresDatabaseUrlEnv =
    optionalString(postgres.databaseUrlEnv) ??
    serviceConfig.storage.postgres.databaseUrlEnv;
  const postgresSchema =
    optionalString(postgres.schema) ?? serviceConfig.storage.postgres.schema;
  const postgresBootMode =
    runtimePostgresBootMode(postgres.bootMode) ??
    serviceConfig.storage.postgres.bootMode;
  const config: RustyCrewStorageConfig = {
    backend,
    sqlite: {
      path: sqlitePath,
      wal:
        optionalBoolean(sqlite.wal, "storage.sqlite.wal") ??
        serviceConfig.storage.sqlite.wal,
      busyTimeoutMs:
        optionalPositiveInteger(
          sqlite.busyTimeoutMs,
          "storage.sqlite.busyTimeoutMs",
        ) ?? serviceConfig.storage.sqlite.busyTimeoutMs,
      effectivePath: isAbsolute(sqlitePath)
        ? sqlitePath
        : join(serviceConfig.paths.engineDataDir, sqlitePath),
    },
    postgres: {
      databaseUrlEnv: postgresDatabaseUrlEnv,
      schema: postgresSchema,
      bootMode: postgresBootMode,
      maxConnections:
        optionalPositiveInteger(
          postgres.maxConnections,
          "storage.postgres.maxConnections",
        ) ?? serviceConfig.storage.postgres.maxConnections,
      statementTimeoutMs:
        optionalPositiveInteger(
          postgres.statementTimeoutMs,
          "storage.postgres.statementTimeoutMs",
        ) ?? serviceConfig.storage.postgres.statementTimeoutMs,
    },
    implementationStatus:
      backend === "sqlite"
        ? "active"
        : postgresBootMode === "active"
          ? "active"
          : postgresBootMode === "proof_admin"
            ? "proof_admin_only"
            : "blocked_unimplemented",
  };
  validateRuntimeStorageConfig(config);
  return config;
}

function runtimeStorageBackend(input: unknown): RustyCrewStorageBackend {
  const value = optionalString(input);
  if (value === undefined || value === "sqlite") return "sqlite";
  if (value === "postgres" || value === "postgresql") return "postgres";
  throw new Error("storage.backend must be sqlite or postgres");
}

function runtimePostgresBootMode(
  input: unknown,
): "blocked" | "proof_admin" | "active" | undefined {
  const value = optionalString(input);
  if (value === undefined) return undefined;
  if (value === "blocked") return "blocked";
  if (value === "proof_admin" || value === "proof-admin") return "proof_admin";
  if (value === "active") return "active";
  throw new Error(
    "storage.postgres.bootMode must be blocked, proof_admin, or active",
  );
}

function validateRuntimeStorageConfig(config: RustyCrewStorageConfig): void {
  if (!config.sqlite.path.trim()) {
    throw new Error("storage.sqlite.path must not be empty");
  }
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(config.postgres.databaseUrlEnv)) {
    throw new Error(
      "storage.postgres.databaseUrlEnv must be an environment variable name",
    );
  }
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(config.postgres.schema)) {
    throw new Error("storage.postgres.schema must be a PostgreSQL identifier");
  }
}

function configuredScheduledJob(
  parsed: unknown,
  index: number,
): RustyCrewScheduledJob {
  if (!isRecord(parsed)) {
    throw new Error(`configured scheduled job ${index} must be an object`);
  }
  const shape = optionalString(parsed.shape) ?? "session_wake";
  if (
    shape !== "host_job" &&
    shape !== "session_wake" &&
    shape !== "script_only" &&
    shape !== "data_collection"
  ) {
    throw new Error(
      `scheduledJobs[${index}].shape must be host_job, session_wake, script_only, or data_collection`,
    );
  }

  const job = {
    id: requiredString(parsed.id, `scheduledJobs[${index}].id`),
    schedule: requiredString(
      parsed.schedule,
      `scheduledJobs[${index}].schedule`,
    ),
    shape,
    jobKind: optionalString(parsed.jobKind),
    targetSessionId: optionalString(parsed.targetSessionId) as
      | SessionId
      | undefined,
    payload: parsed.payload,
    script: optionalString(parsed.script),
    deliveryChannelId: optionalString(parsed.deliveryChannelId),
  } satisfies RustyCrewScheduledJob;

  nextCronDueAt(job.schedule, new Date("2026-06-21T00:00:00Z"));
  if (job.shape === "session_wake" && !job.targetSessionId) {
    throw new Error(
      `scheduledJobs[${index}].targetSessionId is required for session_wake`,
    );
  }
  if (job.shape === "host_job" && !job.jobKind) {
    throw new Error(`scheduledJobs[${index}].jobKind is required for host_job`);
  }
  return job;
}

function configuredBrain(
  parsed: unknown,
  index: number,
): RustyCrewConfiguredBrain {
  if (!isRecord(parsed)) {
    throw new Error(`configured brain ${index} must be an object`);
  }
  const profileId = requiredString(
    parsed.profileId,
    `brains[${index}].profileId`,
  );
  return {
    profileId: profileId as ProfileId,
    implementationId: (optionalString(parsed.implementationId) ??
      `${profileId}-brain`) as BrainImplementationId,
  };
}

function configuredSession(
  parsed: unknown,
  index: number,
): RustyCrewConfiguredSession {
  if (!isRecord(parsed)) {
    throw new Error(`configured session ${index} must be an object`);
  }
  const kind = optionalString(parsed.kind) ?? "full";
  if (kind !== "full" && kind !== "worker" && kind !== "delegated") {
    throw new Error(
      `sessions[${index}].kind must be full, worker, or delegated`,
    );
  }
  return {
    sessionId: requiredString(
      parsed.sessionId,
      `sessions[${index}].sessionId`,
    ) as SessionId,
    agentId: requiredString(
      parsed.agentId,
      `sessions[${index}].agentId`,
    ) as AgentId,
    profileId: requiredString(
      parsed.profileId,
      `sessions[${index}].profileId`,
    ) as ProfileId,
    kind,
    resourceLimits: isRecord(parsed.resourceLimits)
      ? resourceLimits(parsed.resourceLimits)
      : undefined,
    ownerId: optionalString(parsed.ownerId),
    maxHistoryMessages: optionalNumber(parsed.maxHistoryMessages),
    turnTimeoutMs: optionalNumber(parsed.turnTimeoutMs),
    sessionMemoryPrompt: isRecord(parsed.sessionMemoryPrompt)
      ? sessionMemoryPromptConfig(parsed.sessionMemoryPrompt)
      : undefined,
  };
}

function resourceLimits(parsed: Record<string, unknown>): ResourceLimits {
  return {
    workdir: optionalString(parsed.workdir),
    maxDurationMs: optionalNumber(parsed.maxDurationMs),
    maxDelegationDepth: optionalNumber(parsed.maxDelegationDepth),
  };
}

function configuredChannelBinding(
  parsed: unknown,
  index: number,
): ChannelBindingRecord {
  if (!isRecord(parsed)) {
    throw new Error(`configured channel binding ${index} must be an object`);
  }
  return {
    bindingId: requiredString(
      parsed.bindingId,
      `channelBindings[${index}].bindingId`,
    ),
    adapterId: requiredString(
      parsed.adapterId,
      `channelBindings[${index}].adapterId`,
    ) as never,
    provider: optionalString(parsed.provider) ?? "den_channels",
    agentId: requiredString(
      parsed.agentId,
      `channelBindings[${index}].agentId`,
    ) as AgentId,
    sessionId: optionalString(parsed.sessionId) as SessionId | undefined,
    profileId: requiredString(
      parsed.profileId,
      `channelBindings[${index}].profileId`,
    ) as ProfileId,
    externalChannelId: requiredString(
      parsed.externalChannelId,
      `channelBindings[${index}].externalChannelId`,
    ),
    externalThreadId: optionalString(parsed.externalThreadId),
    externalUserId: optionalString(parsed.externalUserId),
    conversationProjectId: optionalString(parsed.conversationProjectId),
    conversationChannelId: optionalPositiveInteger(
      parsed.conversationChannelId,
      `channelBindings[${index}].conversationChannelId`,
    ),
    providerSubscriptionId: optionalString(parsed.providerSubscriptionId),
    cursor: optionalString(parsed.cursor),
    membershipState: optionalString(parsed.membershipState),
    presenceState: optionalString(parsed.presenceState),
    status: externalBindingStatus(parsed.status),
    degradedReason: optionalString(parsed.degradedReason),
  };
}

function configuredMcpBinding(
  parsed: unknown,
  index: number,
): McpBindingRecord {
  if (!isRecord(parsed)) {
    throw new Error(`configured MCP binding ${index} must be an object`);
  }
  const profileId = requiredString(
    parsed.profileId,
    `mcpBindings[${index}].profileId`,
  );
  return {
    bindingId: requiredString(
      parsed.bindingId,
      `mcpBindings[${index}].bindingId`,
    ),
    adapterId: requiredString(
      parsed.adapterId,
      `mcpBindings[${index}].adapterId`,
    ) as never,
    agentId: requiredString(
      parsed.agentId,
      `mcpBindings[${index}].agentId`,
    ) as AgentId,
    sessionId: optionalString(parsed.sessionId) as SessionId | undefined,
    profileId: profileId as ProfileId,
    serverNames: stringList(
      parsed.serverNames,
      `mcpBindings[${index}].serverNames`,
    ),
    endpointRef: requiredString(
      parsed.endpointRef,
      `mcpBindings[${index}].endpointRef`,
    ),
    transport: optionalString(parsed.transport) ?? "stdio",
    toolProfileKey: optionalString(parsed.toolProfileKey) ?? `${profileId}-mcp`,
    discoveredToolRevision: optionalString(parsed.discoveredToolRevision),
    status: externalBindingStatus(parsed.status),
    degradedReason: optionalString(parsed.degradedReason),
    diagnostics: isRecord(parsed.diagnostics)
      ? {
          lastError: optionalString(parsed.diagnostics.lastError),
          lastCheckedAt: optionalString(parsed.diagnostics.lastCheckedAt),
          notes: optionalString(parsed.diagnostics.notes),
        }
      : {},
  };
}

function configuredMcpServer(
  parsed: unknown,
  index: number,
): RustyCrewMcpServerConfig {
  if (!isRecord(parsed)) {
    throw new Error(`configured MCP server ${index} must be an object`);
  }
  const id = requiredString(parsed.id, `mcpServers[${index}].id`);
  if (!/^[A-Za-z0-9_.:-]+$/.test(id)) {
    throw new Error(
      `mcpServers[${index}].id may only contain letters, numbers, dot, underscore, colon, or dash`,
    );
  }
  const baseUrl = requiredString(
    parsed.baseUrl,
    `mcpServers[${index}].baseUrl`,
  );
  try {
    const url = new URL(baseUrl);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw new Error("protocol must be http or https");
    }
  } catch (error) {
    throw new Error(
      `mcpServers[${index}].baseUrl must be a valid HTTP(S) URL`,
      { cause: error },
    );
  }
  const requestTimeoutMs =
    parsed.requestTimeoutMs === undefined
      ? undefined
      : optionalPositiveInteger(
          parsed.requestTimeoutMs,
          `mcpServers[${index}].requestTimeoutMs`,
        );
  return {
    id,
    label: optionalString(parsed.label),
    baseUrl,
    transport: optionalString(parsed.transport) ?? "streamable_http",
    requestTimeoutMs,
    source: optionalString(parsed.source) === "env" ? "env" : "runtime",
  };
}

function arrayValue(input: unknown): unknown[] {
  if (input === undefined) return [];
  if (!Array.isArray(input))
    throw new Error("runtime config arrays must be arrays");
  return input;
}

function optionalArrayValue(
  input: unknown,
  fallback: readonly unknown[],
): unknown[] {
  if (input === undefined) return [...fallback];
  return arrayValue(input);
}

function stringList(input: unknown, name: string): string[] {
  if (!Array.isArray(input) || input.length === 0) {
    throw new Error(`${name} must be a non-empty string array`);
  }
  return input.map((item, index) => requiredString(item, `${name}[${index}]`));
}

function externalBindingStatus(
  input: unknown,
): "active" | "degraded" | "disconnected" | "archived" {
  const status = optionalString(input) ?? "active";
  if (
    status !== "active" &&
    status !== "degraded" &&
    status !== "disconnected" &&
    status !== "archived"
  ) {
    throw new Error(
      "external binding status must be active, degraded, disconnected, or archived",
    );
  }
  return status;
}

function pathValue(input: unknown, fallback?: string): string {
  const raw = input === undefined ? fallback : requiredString(input, "path");
  if (!raw) throw new Error("path must not be empty");
  return resolve(raw);
}

function requiredString(input: unknown, name: string): string {
  if (typeof input !== "string" || !input.trim()) {
    throw new Error(`${name} must be a non-empty string`);
  }
  return input.trim();
}

function optionalString(input: unknown): string | undefined {
  return typeof input === "string" && input.trim() ? input.trim() : undefined;
}

function optionalNumber(input: unknown): number | undefined {
  return typeof input === "number" && Number.isFinite(input)
    ? input
    : undefined;
}

function optionalBoolean(input: unknown, name: string): boolean | undefined {
  if (input === undefined || input === null) return undefined;
  if (typeof input !== "boolean") {
    throw new Error(`${name} must be a boolean`);
  }
  return input;
}

function optionalPositiveInteger(
  input: unknown,
  name: string,
): number | undefined {
  if (input === undefined || input === null) return undefined;
  if (typeof input !== "number" || !Number.isSafeInteger(input) || input <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return input;
}

function isAlreadyPresentError(error: unknown): boolean {
  const message = error instanceof Error ? error.message.toLowerCase() : "";
  return (
    message.includes("already exists") ||
    message.includes("already registered") ||
    message.includes("duplicate")
  );
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
