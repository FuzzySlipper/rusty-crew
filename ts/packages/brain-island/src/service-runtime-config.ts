import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type {
  AgentId,
  BrainModelConfig,
  BrainAction,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationId,
  BrainProviderStateScope,
  BrainStrategyMetadata,
  CompletionStatus,
  CompletionPacket,
  ChannelBindingRecord,
  CoreEventKind,
  McpBindingRecord,
  McpSurfaceDiagnostics,
  ProfileId,
  ResourceLimits,
  ScheduledJobSummary,
  SessionId,
  SessionKind,
  ToolProfile,
} from "@rusty-crew/contracts";
import type { ServiceAdapterFactories } from "./service-adapter-ports.js";
import type {
  NativeBridgeModule,
  NativeBrainConfigDraft,
  NativeRuntimeConfigDiagnostic,
  NativeRuntimeGraphPlan,
  NativeModelProviderRecord,
  NativeScheduledJobConfigDraft,
  NativeSessionConfigDraft,
  NativeSessionStateSummary,
} from "@rusty-crew/native-bridge";
import {
  loadNativeBridge,
  type NativeLocalCodeResourcePolicyPlan,
} from "@rusty-crew/native-bridge";
import { createBrowserToolResolver } from "./browser-tools.js";
import {
  createCoordinationToolResolver,
  type CoordinationToolRuntime,
} from "./coordination-tools.js";
import { resolveCompletionTools } from "./completion-tools.js";
import { createBuiltInBrainHost } from "./built-in-brain-host.js";
import { providerRequestTimeoutDiagnostics } from "./provider-request-timeout.js";
import { responsesContinuationDiagnostics } from "./responses-continuation-policy.js";
import { chatCompletionsContinuationDiagnostics } from "./chat-completions-continuation-policy.js";
import {
  resolveBrainCatalogSelection,
  type BrainModuleSelection,
  type BrainModuleStrategyMetadata,
} from "./brain-catalog.js";
import { nextCronDueAt } from "./cron-expression.js";
import {
  denseProfileMemoryTool,
  type DenseProfileMemoryMode,
} from "./dense-profile-memory-tool.js";
import { resolveDenMemoryTools } from "./den-memory-tools.js";
import { resolveDelegationTools } from "./delegation-tools.js";
import { resolveLoreMemoryTools } from "./lore-memory-tool.js";
import { resolveSceneStateTools } from "./scene-state-tool.js";
import { createRoleplayMechanicToolResolver } from "./roleplay-mechanic-tools.js";
import type { BrainHostExecutor } from "./index.js";
import { createLocalCodeToolResolver } from "./local-code-tools.js";
import { createMemorySpaceToolResolver } from "./memory-space-api.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type { BrainToolMediaSink } from "./brain-tool-media.js";
import type { NarratorImageContextResolver } from "./narrator-image-context.js";
import { narratorImageInputCapability } from "./narrator-image-context.js";
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
  contextStrategyPolicyFromUnknown,
  type ContextStrategyPolicy,
} from "./context-strategy.js";
import {
  buildServiceMcpToolCatalog,
  buildServiceMcpEndpointConfig,
  createServiceMcpToolResolver,
  type ServiceMcpToolCatalog,
  type ServiceMcpToolDiscoveryClientFactory,
  type ServiceMcpToolExecutorFactory,
} from "./service-mcp-tools.js";
import {
  createServiceBrainWakeExecutor,
  type ServiceBrainWakeResultObservation,
} from "./service-brain-wake-executor.js";
export type { ServiceBrainWakeResultObservation } from "./service-brain-wake-executor.js";
import {
  createServiceBrowserResources,
  type ServiceBrowserResources,
} from "./service-browser-resources.js";
import type {
  RustyCrewMcpServerConfig,
  RustyCrewServiceConfig,
  RustyCrewStorageConfig,
} from "./service-config.js";
import { planRuntimeGraphWithRust } from "./runtime-config-validation.js";
import {
  createSkillsToolResolver,
  type SkillManageMode,
} from "./skills-tools.js";
import {
  combineResolvers,
  type BrainToolResolver,
} from "./tool-session-selection.js";
import { createWebToolResolver } from "./web-tools.js";
import {
  createImageGenerationRuntime,
  createImageGenerationToolResolver,
  imageGenerationConfigFromUnknown,
  type ImageGenerationConfig,
} from "./image-generation.js";
import type { RuntimeBrainModuleDiagnostics } from "./runtime-diagnostics.js";
import { type ExternalMemoryReadiness } from "./external-memory-readiness.js";
import {
  createServiceDenMemoryClient,
  createServiceExternalMemoryReadiness,
} from "./service-external-memory-readiness.js";
import {
  DEFAULT_DEN_OBSERVATION_EVENT_FILTERS,
  type DenObservationEventFilter,
} from "./runtime-core-event-observation.js";

export interface RustyCrewConfiguredBrain extends Omit<
  NativeBrainConfigDraft,
  "implementationId" | "profileId"
> {
  implementationId: BrainImplementationId;
  profileId: ProfileId;
}

export interface RustyCrewConfiguredSession extends Omit<
  NativeSessionConfigDraft,
  "agentId" | "profileId" | "sessionId"
> {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  toolProfile?: ToolProfile;
  sessionMemoryPrompt?: SessionMemoryPromptConfig;
  contextPolicy?: ContextStrategyPolicy;
}

export interface EffectiveSessionDefaults {
  ownerId?: string;
  maxHistoryMessages?: number;
}

export type RustyCrewScheduledJobShape = NativeScheduledJobConfigDraft["shape"];

export interface RustyCrewScheduledJob extends Omit<
  NativeScheduledJobConfigDraft,
  "targetSessionId"
> {
  targetSessionId?: SessionId;
  payload?: unknown;
}

export interface ServiceRuntimeEnvelope {
  // TS/service-host owned loader fields. These configure process storage,
  // adapters, and service observation, not the Rust-owned runtime graph draft.
  storage?: RustyCrewStorageConfig;
  denObservation?: RustyCrewDenObservationConfig;
  mcpServers?: RustyCrewMcpServerConfig[];
  imageGeneration?: ImageGenerationConfig;
}

export interface RustyCrewRuntimeGraphDraft {
  profilesDir: string;
  skillsDir?: string;
  brains: RustyCrewConfiguredBrain[];
  sessions: RustyCrewConfiguredSession[];
  scheduledJobs: RustyCrewScheduledJob[];
  channelBindings: ChannelBindingRecord[];
  mcpBindings: McpBindingRecord[];
}

export interface RustyCrewRuntimeConfig
  extends ServiceRuntimeEnvelope, RustyCrewRuntimeGraphDraft {}

export interface RustyCrewDenObservationConfig {
  eventFilters: DenObservationEventFilter[];
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
    }>;
  };
}

export async function loadRustyCrewRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): Promise<RustyCrewRuntimeConfig> {
  const parsed = JSON.parse(
    await readFile(serviceConfig.paths.serviceConfigFile, "utf8"),
  ) as unknown;
  return planEffectiveRuntimeConfig(parsed, serviceConfig);
}

export async function preflightRustyCrewRuntimeConfig(input: {
  serviceConfig: RustyCrewServiceConfig;
  bridge?: Pick<NativeBridgeModule, "planRuntimeGraph">;
}): Promise<RuntimeConfigValidationPreflightReport> {
  const configPath = input.serviceConfig.paths.serviceConfigFile;
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(configPath, "utf8")) as unknown;
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return preflightFailure(configPath, "runtime_config_missing", error);
    }
    return preflightFailure(configPath, "invalid_runtime_config_json", error);
  }

  let source: RuntimeGraphAuthoredSource;
  try {
    source = runtimeGraphAuthoredSource(parsed, input.serviceConfig);
  } catch (error) {
    return preflightFailure(configPath, "invalid_runtime_config_shape", error);
  }
  const loadedProfiles = await loadRuntimeProfilesForValidation(source);
  if (loadedProfiles.diagnostics.length > 0) {
    const emptyPlan: NativeRuntimeGraphPlan = {
      accepted: false,
      sourceRevision: "unplanned",
      runtimeConfig: emptyNativeRuntimeGraph(source, input.serviceConfig),
      diagnostics: loadedProfiles.diagnostics,
      derived: [],
      defaultsApplied: [],
    };
    return preflightReport(configPath, source, emptyPlan);
  }

  const bridge = input.bridge ?? (await loadNativeBridge());
  const plan = await planRuntimeGraphWithRust({
    bridge,
    ...runtimeGraphPlanningFacts(source, input.serviceConfig),
    runtimeConfig: source.runtimeConfig,
    profiles: loadedProfiles.profiles,
  });
  return preflightReport(configPath, source, plan);
}

async function planEffectiveRuntimeConfig(
  parsed: unknown,
  serviceConfig: RustyCrewServiceConfig,
): Promise<RustyCrewRuntimeConfig> {
  const source = runtimeGraphAuthoredSource(parsed, serviceConfig);
  const profiles = await loadRuntimeProfiles(source);
  const bridge = await loadNativeBridge();
  const plan = await planRuntimeGraphWithRust({
    bridge,
    ...runtimeGraphPlanningFacts(source, serviceConfig),
    runtimeConfig: source.runtimeConfig,
    profiles,
  });
  assertRuntimeConfigPlan(plan.diagnostics);
  return runtimeConfigFromGraphPlan(plan, source, profiles);
}

async function loadRuntimeProfiles(
  source: RuntimeGraphAuthoredSource,
): Promise<ProfileConfig[]> {
  const profileIds = runtimeGraphProfileIds(source.runtimeConfig);
  const profiles: ProfileConfig[] = [];
  for (const profileId of profileIds) {
    profiles.push(await loadProfileConfig(source.profilesDir, profileId));
  }
  return profiles;
}

async function loadRuntimeProfilesForValidation(
  source: RuntimeGraphAuthoredSource,
): Promise<{
  profiles: ProfileConfig[];
  diagnostics: NativeRuntimeConfigDiagnostic[];
}> {
  const profileIds = runtimeGraphProfileIds(source.runtimeConfig);
  const profiles: ProfileConfig[] = [];
  const diagnostics: NativeRuntimeConfigDiagnostic[] = [];
  for (const profileId of profileIds) {
    try {
      profiles.push(await loadProfileConfig(source.profilesDir, profileId));
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

interface RuntimeGraphAuthoredSource {
  profilesDir: string;
  runtimeConfig: Record<string, unknown>;
  denObservation: RustyCrewDenObservationConfig;
  mcpServers: RustyCrewMcpServerConfig[];
  imageGeneration: ImageGenerationConfig;
}

function runtimeGraphAuthoredSource(
  parsed: unknown,
  serviceConfig: RustyCrewServiceConfig,
): RuntimeGraphAuthoredSource {
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  rejectRetiredTurnLifetimeFields(parsed);
  const profilesDir = pathValue(
    parsed.profilesDir,
    join(serviceConfig.paths.configDir, "profiles"),
  );
  const skillsDir =
    parsed.skillsDir == null ? undefined : pathValue(parsed.skillsDir);
  const runtimeConfig: Record<string, unknown> = {
    profilesDir,
    ...(skillsDir === undefined ? {} : { skillsDir }),
    ...(parsed.storage === undefined ? {} : { storage: parsed.storage }),
    brains: arrayValue(parsed.brains),
    sessions: arrayValue(parsed.sessions),
    scheduledJobs: arrayValue(parsed.scheduledJobs),
    channelBindings: arrayValue(parsed.channelBindings),
    mcpBindings: arrayValue(parsed.mcpBindings),
  };
  return {
    profilesDir,
    runtimeConfig,
    denObservation: runtimeDenObservationConfig(parsed.denObservation),
    mcpServers: optionalArrayValue(
      parsed.mcpServers,
      serviceConfig.mcp.servers,
    ).map((item, index) => configuredMcpServer(item, index)),
    imageGeneration: imageGenerationConfigFromUnknown(parsed.imageGeneration),
  };
}

function rejectRetiredTurnLifetimeFields(
  parsed: Record<string, unknown>,
): void {
  if (Object.hasOwn(parsed, "wakeTimeout")) {
    throw new Error(
      "wakeTimeout is retired; logical turns continue until completion, operator attention, or explicit cancellation",
    );
  }
  for (const [index, session] of arrayValue(parsed.sessions).entries()) {
    if (isRecord(session) && Object.hasOwn(session, "turnTimeoutMs")) {
      throw new Error(
        `sessions[${index}].turnTimeoutMs is retired; session turns have no finite lifetime`,
      );
    }
  }
}

function runtimeGraphPlanningFacts(
  source: RuntimeGraphAuthoredSource,
  serviceConfig: RustyCrewServiceConfig,
) {
  const storage = serviceConfig.storage;
  const authoredStorage = isRecord(source.runtimeConfig.storage)
    ? source.runtimeConfig.storage
    : undefined;
  const authoredPostgres = isRecord(authoredStorage?.postgres)
    ? authoredStorage.postgres
    : undefined;
  const databaseUrlEnv =
    optionalString(authoredPostgres?.databaseUrlEnv) ??
    storage.postgres.databaseUrlEnv;
  return {
    hostFacts: {
      configDir: serviceConfig.paths.configDir,
      engineDataDir: serviceConfig.paths.engineDataDir,
      defaultWorkdir: serviceConfig.paths.defaultWorkdir,
      postgresDatabaseUrlEnvPresent:
        serviceConfig.environmentVariablePresent(databaseUrlEnv),
    },
    serviceDefaults: {
      storage: {
        backend: storage.backend,
        sqlite: {
          path: storage.sqlite.path,
          wal: storage.sqlite.wal,
          busyTimeoutMs: storage.sqlite.busyTimeoutMs,
        },
        postgres: {
          databaseUrlEnv: storage.postgres.databaseUrlEnv,
          schema: storage.postgres.schema,
          bootMode: storage.postgres.bootMode,
          maxConnections: storage.postgres.maxConnections,
          statementTimeoutMs: storage.postgres.statementTimeoutMs,
        },
      },
    },
  };
}

function runtimeGraphProfileIds(
  runtimeConfig: Record<string, unknown>,
): Set<ProfileId> {
  const ids = new Set<ProfileId>();
  for (const collection of [runtimeConfig.brains, runtimeConfig.sessions]) {
    for (const item of arrayValue(collection)) {
      if (!isRecord(item)) continue;
      const profileId = optionalString(item.profileId);
      if (profileId !== undefined) ids.add(profileId as ProfileId);
    }
  }
  return ids;
}

function emptyNativeRuntimeGraph(
  source: RuntimeGraphAuthoredSource,
  serviceConfig: RustyCrewServiceConfig,
): NativeRuntimeGraphPlan["runtimeConfig"] {
  const storage = serviceConfig.storage;
  return {
    profilesDir: source.profilesDir,
    storage,
    brains: [],
    sessions: [],
    scheduledJobs: [],
    channelBindings: [],
    mcpBindings: [],
  };
}

function preflightReport(
  configPath: string,
  source: RuntimeGraphAuthoredSource,
  plan: NativeRuntimeGraphPlan,
): RuntimeConfigValidationPreflightReport {
  const diagnostics = plan.diagnostics;
  const errors = diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  ).length;
  const warnings = diagnostics.filter(
    (diagnostic) => diagnostic.severity === "warning",
  ).length;
  const sessionDefaults = plan.defaultsApplied.filter((item) =>
    item.path.startsWith("sessions["),
  );
  return {
    ok: errors === 0,
    configPath,
    profilesDir: source.profilesDir,
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
      derivedScheduledJobs: plan.derived.filter(
        (item) => item.kind === "scheduled_job",
      ).length,
      derivedMcpBindings: plan.derived.filter(
        (item) => item.kind === "mcp_binding",
      ).length,
      sessionDefaultsApplied: sessionDefaults.length,
    },
    derived: {
      scheduledJobs: plan.runtimeConfig.scheduledJobs
        .filter((job) =>
          plan.derived.some(
            (item) => item.kind === "scheduled_job" && item.id === job.id,
          ),
        )
        .map((job) => ({
          id: job.id,
          shape: job.shape,
          jobKind: job.jobKind,
          targetSessionId: job.targetSessionId,
        })),
      mcpBindings: plan.runtimeConfig.mcpBindings
        .filter((binding) =>
          plan.derived.some(
            (item) =>
              item.kind === "mcp_binding" && item.id === binding.bindingId,
          ),
        )
        .map((binding) => ({
          bindingId: binding.bindingId,
          agentId: binding.agentId,
          sessionId: binding.sessionId,
          profileId: binding.profileId,
          transport: binding.transport,
          toolProfileKey: binding.toolProfileKey,
          serverNames: binding.serverNames,
        })),
      sessionDefaultsApplied: sessionDefaultSummaries(plan),
    },
  };
}

function sessionDefaultSummaries(
  plan: NativeRuntimeGraphPlan,
): RuntimeConfigValidationPreflightReport["derived"]["sessionDefaultsApplied"] {
  const summaries = new Map<
    string,
    RuntimeConfigValidationPreflightReport["derived"]["sessionDefaultsApplied"][number]
  >();
  for (const item of plan.defaultsApplied) {
    const match = /^sessions\[([^\]]+)\]\.(.+)$/.exec(item.path);
    if (!match) continue;
    const sessionId = match[1]!;
    const summary = summaries.get(sessionId) ?? {
      sessionId,
      ownerId: false,
      resourceLimits: false,
      maxHistoryMessages: false,
    };
    const field = match[2]!;
    summary.ownerId ||= field === "ownerId";
    summary.resourceLimits ||= field.startsWith("resourceLimits");
    summary.maxHistoryMessages ||= field === "maxHistoryMessages";
    summaries.set(sessionId, summary);
  }
  return [...summaries.values()];
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

function runtimeConfigFromGraphPlan(
  plan: NativeRuntimeGraphPlan,
  source: RuntimeGraphAuthoredSource,
  profiles: readonly ProfileConfig[],
): RustyCrewRuntimeConfig {
  const effective = plan.runtimeConfig;
  const profilesById = new Map(
    profiles.map((profile) => [profile.profileId, profile]),
  );
  const sessionsById = rawRecordsById(
    source.runtimeConfig.sessions,
    "sessionId",
  );
  const channelBindingsById = rawRecordsById(
    source.runtimeConfig.channelBindings,
    "bindingId",
  );
  const mcpBindingsById = rawRecordsById(
    source.runtimeConfig.mcpBindings,
    "bindingId",
  );
  return {
    profilesDir: effective.profilesDir,
    ...(effective.skillsDir == null ? {} : { skillsDir: effective.skillsDir }),
    storage: effective.storage,
    denObservation: source.denObservation,
    mcpServers: source.mcpServers,
    imageGeneration: source.imageGeneration,
    brains: effective.brains.map((brain) => ({
      implementationId: brain.implementationId as BrainImplementationId,
      profileId: brain.profileId as ProfileId,
    })),
    sessions: effective.sessions.map((session) => {
      const profile = profilesById.get(session.profileId as ProfileId);
      const authored = sessionsById.get(session.sessionId) ?? {};
      return {
        ...authored,
        sessionId: session.sessionId as SessionId,
        agentId: session.agentId as AgentId,
        profileId: session.profileId as ProfileId,
        kind: session.kind,
        resourceLimits: session.resourceLimits,
        ownerId: session.ownerId,
        historyWindow: session.historyWindow,
        maxHistoryMessages:
          session.maxHistoryMessages ?? session.historyWindow?.maxMessages,
        sessionMemoryPrompt:
          profile?.memoryConfig?.sessionMemoryPrompt ??
          (isRecord(authored.sessionMemoryPrompt)
            ? sessionMemoryPromptConfig(authored.sessionMemoryPrompt)
            : undefined),
        contextPolicy:
          profile?.contextPolicy ??
          (isRecord(authored.contextPolicy)
            ? contextStrategyPolicyFromUnknown(authored.contextPolicy)
            : undefined),
      } as RustyCrewConfiguredSession;
    }),
    scheduledJobs: effective.scheduledJobs.map((job) => ({
      id: job.id,
      schedule: job.schedule,
      shape: job.shape,
      jobKind: job.jobKind,
      targetSessionId: job.targetSessionId as SessionId | undefined,
      payload: job.payload,
      script: job.script,
      deliveryChannelId: job.deliveryChannelId,
    })),
    channelBindings: effective.channelBindings.map((binding) => ({
      ...(channelBindingsById.get(binding.bindingId) ?? {}),
      ...binding,
      adapterId: binding.adapterId as never,
      agentId: binding.agentId as AgentId,
      instanceId: binding.instanceId as never,
      sessionId: binding.sessionId as SessionId | undefined,
      profileId: binding.profileId as ProfileId,
    })) as ChannelBindingRecord[],
    mcpBindings: effective.mcpBindings.map((binding) => ({
      ...(mcpBindingsById.get(binding.bindingId) ?? {}),
      ...binding,
      adapterId: binding.adapterId as never,
      agentId: binding.agentId as AgentId,
      instanceId: binding.instanceId as never,
      sessionId: binding.sessionId as SessionId | undefined,
      profileId: binding.profileId as ProfileId,
      diagnostics: isRecord(mcpBindingsById.get(binding.bindingId)?.diagnostics)
        ? mcpBindingsById.get(binding.bindingId)!.diagnostics
        : {},
    })) as McpBindingRecord[],
  };
}

function rawRecordsById(
  input: unknown,
  idField: string,
): Map<string, Record<string, unknown>> {
  const records = new Map<string, Record<string, unknown>>();
  for (const item of arrayValue(input)) {
    if (!isRecord(item)) continue;
    const id = optionalString(item[idField]);
    if (id !== undefined) records.set(id, item);
  }
  return records;
}

function runtimeGraphSourceFromEffective(
  runtimeConfig: RustyCrewRuntimeConfig,
): Record<string, unknown> {
  return {
    profilesDir: runtimeConfig.profilesDir,
    skillsDir: runtimeConfig.skillsDir,
    storage: runtimeConfig.storage,
    denObservation: runtimeConfig.denObservation,
    mcpServers: runtimeConfig.mcpServers,
    imageGeneration: runtimeConfig.imageGeneration,
    brains: runtimeConfig.brains,
    sessions: runtimeConfig.sessions,
    scheduledJobs: runtimeConfig.scheduledJobs,
    channelBindings: runtimeConfig.channelBindings,
    mcpBindings: runtimeConfig.mcpBindings,
  };
}

export async function applyRustyCrewRuntimeConfig(input: {
  serviceConfig: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  bridge: NativeBridgeModule;
  existingBrainHandlesByProfileId?: Record<string, BrainImplementationHandle>;
  existingBrainModulesByProfileId?: Record<string, BrainModuleSelection>;
  existingBrainDiagnosticsByProfileId?: Record<
    string,
    RuntimeBrainModuleDiagnostics
  >;
  createMissingSessions?: boolean;
  curatorExecutor?: CuratorExecuteContext["executor"];
  mcpSurfaceDiagnostics?: readonly McpSurfaceDiagnostics[];
  mcpToolDiscoveryClientFactory?: ServiceMcpToolDiscoveryClientFactory;
  mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
  adapterFactories?: Pick<ServiceAdapterFactories, "createDenMemoryClient">;
  externalMemoryReadiness?: ExternalMemoryReadiness;
  coordinationRuntime?: CoordinationToolRuntime;
  toolCallDebugStore?: ToolCallDebugStore;
  providerRequestDebugStore?: ProviderRequestDebugStore;
  browserResources?: ServiceBrowserResources;
  toolMediaSink?: BrainToolMediaSink;
  narratorImageContextResolver?: NarratorImageContextResolver;
  onBrainWakeResult: (observation: ServiceBrainWakeResultObservation) => void;
}): Promise<RustyCrewRuntimeConfigApplyResult> {
  const runtimeConfig = await planEffectiveRuntimeConfig(
    runtimeGraphSourceFromEffective(input.runtimeConfig),
    input.serviceConfig,
  );
  const browserResources =
    input.browserResources ??
    createServiceBrowserResources({
      resourcePolicy: await input.bridge.planWebBrowserResourcePolicy({}),
      bridge: input.bridge,
    });
  const localCodeResourcePolicy =
    await input.bridge.planLocalCodeResourcePolicy({});
  const externalMemoryReadiness =
    input.externalMemoryReadiness ??
    createServiceExternalMemoryReadiness(
      input.serviceConfig,
      input.adapterFactories,
    );
  await externalMemoryReadiness.refresh();
  const createMissingSessions = input.createMissingSessions ?? true;
  const mcpToolCatalog = await buildServiceMcpToolCatalog({
    bridge: input.bridge,
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
      toolAvailabilityPlanner: (request) =>
        input.bridge.planToolAvailability(request),
      externalMemoryAvailability: externalMemoryReadiness.current(),
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

  for (const brain of runtimeConfig.brains) {
    const profile = await loadProfile(brain.profileId);
    const resolvedBrain = await resolveBrainCatalogSelection(
      input.bridge,
      profile.profile,
    );
    const { selection, moduleStrategy, strategy } = resolvedBrain;
    const providerStateScope = providerStateScopeForProfile({
      profile,
      strategy,
      moduleStrategy,
    });
    const nextDiagnostics = brainModuleDiagnostics({
      profile,
      implementationId: brain.implementationId,
      selection,
      strategy,
      moduleStrategy,
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
        createServiceBrainWakeExecutor(
          await createConfiguredBrain(selection, profile, {
            bridge: input.bridge,
            providerStateScope,
            runtimeConfig,
            serviceConfig: input.serviceConfig,
            curatorExecutor: input.curatorExecutor,
            mcpToolCatalog,
            mcpToolExecutorFactory: input.mcpToolExecutorFactory,
            adapterFactories: input.adapterFactories,
            externalMemoryReadiness,
            coordinationRuntime: input.coordinationRuntime,
            toolCallDebugStore: input.toolCallDebugStore,
            providerRequestDebugStore: input.providerRequestDebugStore,
            browserResources,
            toolMediaSink: input.toolMediaSink,
            narratorImageContextResolver: input.narratorImageContextResolver,
            localCodeResourcePolicy,
          }),
          {
            profileId: brain.profileId,
            onBrainWakeResult: input.onBrainWakeResult,
          },
        ),
      );
      result.brainHandlesByProfileId[brain.profileId] = handle;
      result.brainModulesByProfileId[brain.profileId] = selection;
      result.brainDiagnosticsByProfileId[brain.profileId] = nextDiagnostics;
      result.brainsRegistered += 1;
    } catch (error) {
      if (!isAlreadyPresentError(error)) throw error;
      const existingHandle =
        input.existingBrainHandlesByProfileId?.[brain.profileId];
      if (existingHandle !== undefined) {
        result.brainHandlesByProfileId[brain.profileId] = existingHandle;
      }
      result.brainModulesByProfileId[brain.profileId] =
        input.existingBrainModulesByProfileId?.[brain.profileId] ?? selection;
      result.brainDiagnosticsByProfileId[brain.profileId] =
        input.existingBrainDiagnosticsByProfileId?.[brain.profileId] ??
        nextDiagnostics;
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
    const configuredSession = sessionWithProfileDefaults(
      session,
      profile,
      input.serviceConfig.paths.defaultWorkdir,
    );
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
  const apiKey = modelProviderApiKeySecret(provider, secret);
  const credentialKind =
    provider.credential.kind ??
    (apiKey === undefined ? undefined : "legacy_raw_api_key");
  const apiKeyEnv =
    apiKey === undefined
      ? undefined
      : modelProviderSecretEnvName(provider.alias);
  if (apiKeyEnv !== undefined) {
    process.env[apiKeyEnv] = apiKey;
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
    credentialKind,
    contextWindowTokens: provider.contextWindowTokens,
    temperatureMilli: provider.temperatureMilli,
    maxOutputTokens: provider.maxOutputTokens,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    narratorImageInput: narratorImageInputCapability(provider.metadataJson),
  };
}

function modelProviderApiKeySecret(
  provider: NativeModelProviderRecord,
  secret: string | undefined,
): string | undefined {
  if (secret === undefined) {
    return undefined;
  }
  const trimmed = secret.trim();
  if (!trimmed.startsWith("{")) {
    return secret;
  }
  const envelope = JSON.parse(trimmed) as unknown;
  if (!isRuntimeRecord(envelope)) {
    throw new Error(
      `model provider ${provider.alias} secret envelope is invalid`,
    );
  }
  if (envelope.kind === "api_key" && typeof envelope.value === "string") {
    return envelope.value;
  }
  if (envelope.kind === "openai_oauth") {
    return undefined;
  }
  throw new Error(
    `model provider ${provider.alias} secret envelope kind is unsupported`,
  );
}

function modelProviderSecretEnvName(alias: string): string {
  return `RUSTY_CREW_MODEL_PROVIDER_SECRET_${alias
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")}`;
}

function isRuntimeRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export async function rebuildConfiguredBrainRuntime(input: {
  serviceConfig: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  profileId: ProfileId;
  bridge: NativeBridgeModule;
  curatorExecutor?: CuratorExecuteContext["executor"];
  mcpSurfaceDiagnostics?: readonly McpSurfaceDiagnostics[];
  mcpToolDiscoveryClientFactory?: ServiceMcpToolDiscoveryClientFactory;
  mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
  adapterFactories?: Pick<ServiceAdapterFactories, "createDenMemoryClient">;
  externalMemoryReadiness?: ExternalMemoryReadiness;
  coordinationRuntime?: CoordinationToolRuntime;
  toolCallDebugStore?: ToolCallDebugStore;
  providerRequestDebugStore?: ProviderRequestDebugStore;
  browserResources?: ServiceBrowserResources;
  toolMediaSink?: BrainToolMediaSink;
  narratorImageContextResolver?: NarratorImageContextResolver;
  onBrainWakeResult: (observation: ServiceBrainWakeResultObservation) => void;
}): Promise<RustyCrewBrainRuntimeRebuildResult> {
  const runtimeConfig = await planEffectiveRuntimeConfig(
    runtimeGraphSourceFromEffective(input.runtimeConfig),
    input.serviceConfig,
  );
  const browserResources =
    input.browserResources ??
    createServiceBrowserResources({
      resourcePolicy: await input.bridge.planWebBrowserResourcePolicy({}),
      bridge: input.bridge,
    });
  const localCodeResourcePolicy =
    await input.bridge.planLocalCodeResourcePolicy({});
  const externalMemoryReadiness =
    input.externalMemoryReadiness ??
    createServiceExternalMemoryReadiness(
      input.serviceConfig,
      input.adapterFactories,
    );
  await externalMemoryReadiness.refresh();
  const brain = runtimeConfig.brains.find(
    (candidate) => candidate.profileId === input.profileId,
  );
  if (brain === undefined) {
    throw new Error(`profile ${input.profileId} is not configured for a brain`);
  }

  const mcpToolCatalog = await buildServiceMcpToolCatalog({
    bridge: input.bridge,
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
    toolAvailabilityPlanner: (request) =>
      input.bridge.planToolAvailability(request),
    externalMemoryAvailability: externalMemoryReadiness.current(),
  });
  const resolvedBrain = await resolveBrainCatalogSelection(
    input.bridge,
    profile.profile,
  );
  const { selection, moduleStrategy, strategy } = resolvedBrain;
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
    createServiceBrainWakeExecutor(
      await createConfiguredBrain(selection, profile, {
        bridge: input.bridge,
        providerStateScope,
        runtimeConfig,
        serviceConfig: input.serviceConfig,
        curatorExecutor: input.curatorExecutor,
        mcpToolCatalog,
        mcpToolExecutorFactory: input.mcpToolExecutorFactory,
        adapterFactories: input.adapterFactories,
        externalMemoryReadiness,
        coordinationRuntime: input.coordinationRuntime,
        toolCallDebugStore: input.toolCallDebugStore,
        providerRequestDebugStore: input.providerRequestDebugStore,
        browserResources,
        toolMediaSink: input.toolMediaSink,
        narratorImageContextResolver: input.narratorImageContextResolver,
        localCodeResourcePolicy,
      }),
      {
        profileId: brain.profileId,
        onBrainWakeResult: input.onBrainWakeResult,
      },
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
  defaultWorkdir?: string,
): RustyCrewConfiguredSession {
  const defaults = effectiveSessionDefaults(session, profile.profile);
  return {
    ...session,
    resourceLimits: resourceLimitsWithDefaultWorkdir(
      session.resourceLimits ?? profile.profile.runtime?.defaultResourceLimits,
      defaultWorkdir,
    ),
    toolProfile: session.toolProfile ?? profile.toolSelection.toolProfile,
    ...defaults,
  };
}

function resourceLimitsWithDefaultWorkdir(
  limits: ResourceLimits | undefined,
  defaultWorkdir: string | undefined,
): ResourceLimits | undefined {
  if (defaultWorkdir === undefined) {
    return limits;
  }
  return {
    ...limits,
    workdir: limits?.workdir ?? defaultWorkdir,
  };
}

export function effectiveSessionDefaults(
  session: Pick<RustyCrewConfiguredSession, "ownerId" | "maxHistoryMessages">,
  profile: Pick<ProfileConfig, "sessionDefaults">,
): EffectiveSessionDefaults {
  return definedDefaults({
    ownerId: session.ownerId ?? profile.sessionDefaults?.ownerId,
    maxHistoryMessages:
      session.maxHistoryMessages ?? profile.sessionDefaults?.maxHistoryMessages,
  });
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
  strategy: BrainStrategyMetadata;
  moduleStrategy: BrainModuleStrategyMetadata;
}): RuntimeBrainModuleDiagnostics {
  return {
    profileId: input.profile.profile.profileId,
    implementationId: input.implementationId,
    moduleId: input.selection.moduleId,
    ...(input.selection.strategy === undefined
      ? {}
      : { strategy: input.selection.strategy }),
    effectiveStrategy: input.moduleStrategy.diagnostics.effectiveStrategyId,
    ...(input.profile.profile.providerAlias === undefined
      ? {}
      : { providerAlias: input.profile.profile.providerAlias }),
    modelProvider: {
      providerKind: input.profile.profile.modelConfig.provider,
      protocol:
        input.profile.profile.modelConfig.api === "openai-responses"
          ? "responses"
          : "chat_completions",
      ...(input.selection.moduleId === "openai-responses"
        ? { clientMode: "live" }
        : {}),
      ...providerRequestTimeoutDiagnostics(input.selection.moduleId),
      ...chatCompletionsContinuationDiagnostics(input.selection.moduleId),
      ...responsesContinuationDiagnostics(input.selection.moduleId),
      modelId: input.profile.profile.modelConfig.modelName,
      ...(input.profile.profile.modelConfig.baseUrl === undefined
        ? {}
        : { baseUrl: input.profile.profile.modelConfig.baseUrl }),
      ...(input.profile.profile.modelConfig.contextWindowTokens === undefined
        ? {}
        : {
            contextWindowTokens:
              input.profile.profile.modelConfig.contextWindowTokens,
          }),
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
        ? {
            credential: {
              hasSecret:
                input.profile.profile.modelConfig.credentialKind !== undefined,
              kind: input.profile.profile.modelConfig.credentialKind,
            },
          }
        : {
            credential: {
              hasSecret: true,
              secretRef: input.profile.profile.modelConfig.apiKeyEnv,
              kind: input.profile.profile.modelConfig.credentialKind,
            },
          }),
    },
    ...(input.profile.profile.contextPolicy === undefined
      ? {}
      : {
          contextCompaction: {
            enabled: input.profile.profile.contextPolicy.enabled,
            autoCompactionEnabled:
              input.profile.profile.contextPolicy.autoCompactionEnabled,
            strategyId: input.profile.profile.contextPolicy.strategyId,
            compactAtPercent:
              input.profile.profile.contextPolicy.compactAtPercent,
            targetPercentAfterCompaction:
              input.profile.profile.contextPolicy.targetPercentAfterCompaction,
            contextWindowTokens:
              input.profile.profile.modelConfig.contextWindowTokens,
          },
        }),
    providerStateMode: input.strategy.providerState.mode,
    providerStateRebuild: input.moduleStrategy.providerState.rebuild,
    strategyDiagnostics: input.moduleStrategy.diagnostics,
    selectedToolCount: input.profile.toolSelection.toolProfile.tools.length,
    selectedToolSource: input.profile.toolSelection.catalogId,
    toolAdapterStatus: "native_neutral_tools",
  };
}

async function createConfiguredBrain(
  selection: BrainModuleSelection,
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
  options: {
    bridge?: NativeBridgeModule;
    runtimeConfig?: RustyCrewRuntimeConfig;
    serviceConfig?: RustyCrewServiceConfig;
    providerStateScope?: BrainProviderStateScope;
    curatorExecutor?: CuratorExecuteContext["executor"];
    mcpToolCatalog?: ServiceMcpToolCatalog;
    mcpToolExecutorFactory?: ServiceMcpToolExecutorFactory;
    adapterFactories?: Pick<ServiceAdapterFactories, "createDenMemoryClient">;
    externalMemoryReadiness: ExternalMemoryReadiness;
    coordinationRuntime?: CoordinationToolRuntime;
    toolCallDebugStore?: ToolCallDebugStore;
    providerRequestDebugStore?: ProviderRequestDebugStore;
    browserResources: ServiceBrowserResources;
    toolMediaSink?: BrainToolMediaSink;
    narratorImageContextResolver?: NarratorImageContextResolver;
    localCodeResourcePolicy: NativeLocalCodeResourcePolicyPlan;
  },
): Promise<BrainHostExecutor> {
  const usesExternalMemory = profile.toolSelection.toolProfile.tools.some(
    (tool) => EXTERNAL_MEMORY_TOOL_NAMES.has(tool.name),
  );
  return createBuiltInBrainHost(selection, {
    profile,
    bridge: options.bridge,
    providerStateScope: options.providerStateScope,
    runtimeConfig: options.runtimeConfig,
    serviceConfig: options.serviceConfig,
    toolResolver: createServiceToolResolver(profile, options),
    ...(usesExternalMemory
      ? {
          prepareToolResolution: async () => {
            await options.externalMemoryReadiness.refresh();
          },
        }
      : {}),
    planActions: completionActionFromEvents,
    maxTokens: effectiveModelMaxTokens(profile),
    toolCallDebugStore: options.toolCallDebugStore,
    providerRequestDebugStore: options.providerRequestDebugStore,
    toolMediaSink: options.toolMediaSink,
    narratorImageContextResolver: options.narratorImageContextResolver,
  });
}

const EXTERNAL_MEMORY_TOOL_NAMES = new Set([
  "memory_recall",
  "memory_read",
  "memory_search",
  "memory_store",
  "memory_propose",
]);

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
    adapterFactories?: Pick<ServiceAdapterFactories, "createDenMemoryClient">;
    externalMemoryReadiness: ExternalMemoryReadiness;
    coordinationRuntime?: CoordinationToolRuntime;
    browserResources: ServiceBrowserResources;
    localCodeResourcePolicy: NativeLocalCodeResourcePolicyPlan;
  },
): BrainToolResolver {
  const todoStore = createServiceTodoStore(options.serviceConfig);
  return combineResolvers(
    createLocalCodeToolResolver({
      resourcePolicy: options.localCodeResourcePolicy,
      bridge: options.bridge,
    }),
    createWebToolResolver({
      searchDefaultLimit:
        options.browserResources.resourcePolicy.web.searchDefaultLimit,
      searchMaxResults:
        options.browserResources.resourcePolicy.web.searchMaxResults,
      maxExtractUrls:
        options.browserResources.resourcePolicy.web.maxExtractUrls,
      maxExtractChars:
        options.browserResources.resourcePolicy.web.maxExtractChars,
      maxExtractBytes:
        options.browserResources.resourcePolicy.web.maxExtractBytes,
      maxRedirects: options.browserResources.resourcePolicy.web.maxRedirects,
      allowPrivateNet:
        options.browserResources.resourcePolicy.web.allowPrivateNet,
      allowedNonstandardPorts:
        options.browserResources.resourcePolicy.web.allowedNonstandardPorts,
    }),
    createBrowserToolResolver({
      manager: options.browserResources.manager,
      pageLoadTimeoutMs:
        options.browserResources.resourcePolicy.browser.pageLoadTimeoutMs,
      allowPrivateNet:
        options.browserResources.resourcePolicy.browser.allowPrivateNet,
      screenshotStore: options.browserResources.screenshotStore,
      maxScreenshotBytes:
        options.browserResources.resourcePolicy.browser.maxScreenshotBytes,
    }),
    createImageGenerationToolResolver(
      createImageGenerationRuntime(
        options.runtimeConfig?.imageGeneration ?? {
          providers: [],
          presets: [],
        },
      ),
    ),
    createMemoryToolResolver(profile, options),
    createRoleplayMechanicToolResolver({
      bridge: options.bridge,
      profile: profile.profile,
    }),
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
    resolveCompletionTools,
    createCoordinationToolResolver(options.coordinationRuntime),
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
    adapterFactories?: Pick<ServiceAdapterFactories, "createDenMemoryClient">;
    externalMemoryReadiness: ExternalMemoryReadiness;
  },
): BrainToolResolver {
  const denMemoryClient = createServiceDenMemoryClient(
    options.serviceConfig,
    options.adapterFactories,
  );
  const memorySpaceResolver = options.bridge
    ? createMemorySpaceToolResolver({ bridge: options.bridge })
    : undefined;
  return (input) => [
    ...(memorySpaceResolver?.(input) ?? []),
    ...(options.externalMemoryReadiness.current().clientAvailable
      ? resolveDenMemoryTools({
          client: denMemoryClient,
          policy: {
            mode: "metadata",
            defaultAudience: [profile.profile.profileId],
          },
          runtimeContext: {
            projectId: options.serviceConfig?.denConversationProjectId,
          },
          session: input.wake.state.session,
        })
      : []),
    denseProfileMemoryTool({
      client: options.bridge,
      mode: denseProfileMemoryMode(profile),
      session: input.wake.state.session,
    }),
    ...resolveLoreMemoryTools({
      client: options.bridge,
      session: input.wake.state.session,
    }),
    ...resolveSceneStateTools({
      client: options.bridge,
      session: input.wake.state.session,
    }),
  ];
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
        summary: text ? truncate(text, 8_192) : "LLM wake completed.",
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

const CORE_EVENT_KINDS = new Set<CoreEventKind>([
  "session_created",
  "session_archived",
  "agent_message_routed",
  "delegation_lifecycle_observed",
  "external_event_injected",
  "den_data_updated",
  "brain_wake_requested",
  "brain_event_observed",
  "brain_actions_accepted",
  "completion_packet_delivered",
]);

const OBSERVATION_VISIBILITIES = new Set<
  NonNullable<DenObservationEventFilter["visibility"]>
>(["channel", "task", "agent", "debug"]);

const SESSION_KINDS = new Set<SessionKind>(["full", "worker", "delegated"]);

const COMPLETION_STATUSES = new Set<CompletionStatus>([
  "completed",
  "failed",
  "blocked",
  "exhausted",
]);

function runtimeDenObservationConfig(
  input: unknown,
): RustyCrewDenObservationConfig {
  if (input === undefined) {
    return {
      eventFilters: [...DEFAULT_DEN_OBSERVATION_EVENT_FILTERS],
    };
  }
  if (!isRecord(input)) {
    throw new Error("denObservation config must be an object");
  }
  return {
    eventFilters:
      input.eventFilters === undefined
        ? [...DEFAULT_DEN_OBSERVATION_EVENT_FILTERS]
        : arrayValue(input.eventFilters).map((item, index) =>
            denObservationEventFilter(item, index),
          ),
  };
}

function denObservationEventFilter(
  input: unknown,
  index: number,
): DenObservationEventFilter {
  const path = `denObservation.eventFilters[${index}]`;
  if (!isRecord(input)) {
    throw new Error(`${path} must be an object`);
  }
  const eventKind = enumString(
    input.eventKind,
    `${path}.eventKind`,
    CORE_EVENT_KINDS,
  );
  return {
    eventKind,
    visibility:
      input.visibility === undefined
        ? undefined
        : enumString(
            input.visibility,
            `${path}.visibility`,
            OBSERVATION_VISIBILITIES,
          ),
    sessionKind:
      input.sessionKind === undefined
        ? undefined
        : enumString(input.sessionKind, `${path}.sessionKind`, SESSION_KINDS),
    completionStatus:
      input.completionStatus === undefined
        ? undefined
        : enumString(
            input.completionStatus,
            `${path}.completionStatus`,
            COMPLETION_STATUSES,
          ),
    profileId: optionalString(input.profileId),
    agentId: optionalString(input.agentId),
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

function enumString<T extends string>(
  input: unknown,
  name: string,
  allowed: ReadonlySet<T>,
): T {
  if (typeof input !== "string" || !allowed.has(input as T)) {
    throw new Error(
      `${name} must be one of ${[...allowed]
        .map((item) => JSON.stringify(item))
        .join(", ")}`,
    );
  }
  return input as T;
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
