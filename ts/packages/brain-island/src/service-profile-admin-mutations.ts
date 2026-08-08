import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import type {
  BrainImplementationHandle,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeCreateProfilePlan,
  NativeModelProviderRecord,
  NativeProfilePurgeReport,
  NativeRuntimeConfigPlan,
} from "@rusty-crew/native-bridge";
import type { AdminControlCommand } from "./admin-control-api.js";
import { createLocalToolProfileStore } from "./local-tool-profiles.js";
import {
  loadProfileConfig,
  parseExternalMessageDeliveryPolicy,
  parseProfileConfigDraft,
  type ProfileConfig,
} from "./profile-loading.js";
import {
  planCreateProfileWithRust,
  planRuntimeConfigWithRust,
} from "./runtime-config-validation.js";
import type { RustyCrewMcpServerConfig } from "./service-config.js";
import { mcpServerWriteFromBody } from "./service-mcp-server-registry-routes.js";
import {
  type RustyCrewRuntimeConfig,
  type RustyCrewRuntimeConfigApplyResult,
} from "./service-runtime-config.js";

export interface ServiceProfileAdminMutationContext {
  bridge: NativeBridgeModule;
  runtimeConfig: RustyCrewRuntimeConfig;
  serviceConfigFile: string;
  now(): string;
  inFlightWakes: ReadonlySet<SessionId>;
  applyRuntimeConfigFromDisk(options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<RustyCrewRuntimeConfigApplyResult>;
  archiveSession(sessionId: SessionId): Promise<void>;
  forgetPurgedSessions(sessionIds: Iterable<string>): void;
}

export interface CreatedServiceProfile {
  profileId: string;
  displayName?: string;
  agentId: string;
  sessionId: string;
  implementationId: string;
  profilePath: string;
  runtimeConfigPath: string;
  registryWrite?: NativeCreateProfilePlan["registryWrite"];
  registryRecord?: Awaited<
    ReturnType<NativeBridgeModule["createProfileRegistryRecord"]>
  >;
  localToolProfileId?: string;
  fileAssetActions: NativeCreateProfilePlan["fileAssetActions"];
  derivedRuntimeActions: NativeCreateProfilePlan["derivedRuntimeActions"];
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

export interface DecommissionedServiceProfile {
  profileId: string;
  runtimeConfigPath: string;
  profilePath?: string;
  profileDirectoryPreserved: true;
  sessionsArchived: string[];
  removed: {
    brains: number;
    sessions: number;
    channelBindings: number;
    mcpBindings: number;
    scheduledJobs: number;
  };
  brainHandle: {
    action: "removed" | "already_absent";
    handle?: BrainImplementationHandle;
  };
  skipped: {
    profileDirectory: "preserved";
  };
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

export interface DeletedServiceProfile {
  profileId: string;
  runtimeConfigPath: string;
  profilePath?: string;
  profileDirectoryDeleted: boolean;
  sessionsDeleted: string[];
  removed: DecommissionedServiceProfile["removed"];
  brainHandle: DecommissionedServiceProfile["brainHandle"];
  storagePurge: NativeProfilePurgeReport;
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

export interface ProfileUpdatePlan {
  profileId: string;
  ok: boolean;
  profilePath: string;
  diagnostics: Array<{
    severity: "error" | "warning" | "info";
    code: string;
    path: string;
    message: string;
  }>;
  implications: {
    configReloadRequired: true;
    mcpRefreshRecommended: boolean;
    runtimeRebuildRecommended: boolean;
    profileDirectoryFiles: "json_profile_only";
  };
  runtimePlan?: unknown;
}

export interface RuntimeConfigDraftPlan {
  ok: boolean;
  configPath: string;
  diagnostics: Array<{
    severity: "error" | "warning" | "info";
    code: string;
    path: string;
    message: string;
  }>;
  implications: {
    configReloadRequired: true;
    createMissingSessions: false;
    explicitChannelLifecycle: true;
    explicitSessionLifecycle: true;
  };
  runtimePlan?: NativeRuntimeConfigPlan;
}

export async function readServiceProfileConfig(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<Record<string, unknown>> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  const profilePath = safeProfileConfigPath(
    context.runtimeConfig.profilesDir,
    profileId,
  );
  if (profilePath === undefined) {
    throw new Error(`profile id ${profileId} is not a valid file profile id`);
  }
  const raw = JSON.parse(await readFile(profilePath, "utf8")) as unknown;
  if (!isRecord(raw)) {
    throw new Error(`profile ${profileId} config root must be an object`);
  }
  const loaded = await loadProfileConfig(
    context.runtimeConfig.profilesDir,
    profileId as ProfileId,
  );
  return {
    profileId,
    profilePath,
    profileConfig: raw,
    loaded,
    editable: {
      format: "json_profile",
      supportsSoulMarkdown: true,
      supportsMemoryMarkdown: true,
    },
  };
}

export async function planServiceProfileUpdate(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<ProfileUpdatePlan> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  const profilePath = safeProfileConfigPath(
    context.runtimeConfig.profilesDir,
    profileId,
  );
  if (profilePath === undefined) {
    throw new Error(`profile id ${profileId} is not a valid file profile id`);
  }
  const draft = profileConfigDraftFromCommand(command, profileId);
  const diagnostics: ProfileUpdatePlan["diagnostics"] = [];
  let parsedDraft: ProfileConfig | undefined;
  try {
    parsedDraft = parseProfileConfigDraft({
      profilesDir: context.runtimeConfig.profilesDir,
      profileId: profileId as ProfileId,
      profileConfig: draft,
      soulMarkdown: optionalBodyString(command, "soulMarkdown"),
      memoryMarkdown: optionalBodyString(command, "memoryMarkdown"),
    });
  } catch (error) {
    diagnostics.push({
      severity: "error",
      code: "invalid_profile_config",
      path: `profiles.${profileId}`,
      message: errorMessage(error, "profile draft is invalid"),
    });
  }

  const currentProfile = await loadProfileConfig(
    context.runtimeConfig.profilesDir,
    profileId as ProfileId,
  ).catch(() => undefined);
  let runtimePlan: NativeRuntimeConfigPlan | undefined;
  if (parsedDraft !== undefined) {
    const profiles = await loadRuntimeConfigProfilesReplacing(
      context,
      profileId,
      parsedDraft,
    );
    const plan = await planRuntimeConfigWithRust({
      bridge: context.bridge,
      runtimeConfig: context.runtimeConfig,
      profiles,
    });
    runtimePlan = plan;
    for (const diagnostic of plan.diagnostics) {
      diagnostics.push({
        severity: diagnostic.severity,
        code: diagnostic.code,
        path: diagnostic.path ?? "runtimeConfig",
        message: diagnostic.message,
      });
    }
  }

  return {
    profileId,
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    profilePath,
    diagnostics,
    implications: {
      configReloadRequired: true,
      mcpRefreshRecommended: profileMcpChanged(currentProfile, parsedDraft),
      runtimeRebuildRecommended: profileRuntimeBrainChanged(
        currentProfile,
        parsedDraft,
      ),
      profileDirectoryFiles: "json_profile_only",
    },
    runtimePlan,
  };
}

export async function applyServiceProfileUpdate(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<
  ProfileUpdatePlan & { applyResult?: RustyCrewRuntimeConfigApplyResult }
> {
  const plan = await planServiceProfileUpdate(context, command);
  if (!plan.ok) return plan;
  const draft = profileConfigDraftFromCommand(command, plan.profileId);
  await writeJsonFileAtomic(plan.profilePath, draft);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "profile_config_updated",
    summaryPrefix: `Profile ${plan.profileId} updated`,
  });
  return { ...plan, applyResult };
}

export async function planServiceRuntimeConfigDraft(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<RuntimeConfigDraftPlan> {
  const runtimeConfig = runtimeConfigDraftFromCommand(context, command);
  return planRuntimeConfigValue(context, runtimeConfig);
}

export async function planRuntimeConfigFileValue(
  context: ServiceProfileAdminMutationContext,
  value: Record<string, unknown>,
): Promise<RuntimeConfigDraftPlan> {
  return planRuntimeConfigValue(
    context,
    runtimeConfigDraftFromFileValue(context, value),
  );
}

export function assertRuntimeConfigDraftPlanOk(
  plan: RuntimeConfigDraftPlan,
): void {
  const errors = plan.diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (errors.length === 0) return;
  const first = errors[0]!;
  const suffix =
    errors.length === 1
      ? ""
      : ` (${errors.length - 1} additional diagnostic${errors.length === 2 ? "" : "s"})`;
  throw new Error(
    `${first.path ? `${first.path}: ` : ""}${first.message}${suffix}`,
  );
}

export async function applyServiceRuntimeConfigDraft(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<
  RuntimeConfigDraftPlan & { applyResult?: RustyCrewRuntimeConfigApplyResult }
> {
  const plan = await planServiceRuntimeConfigDraft(context, command);
  if (!plan.ok) return plan;
  const runtimeConfig = runtimeConfigDraftFromCommand(context, command);
  await writeJsonFileAtomic(context.serviceConfigFile, runtimeConfig);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "runtime_config_draft_applied",
    summaryPrefix: "Runtime config draft applied",
  });
  return { ...plan, applyResult };
}

export async function decommissionServiceProfile(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<DecommissionedServiceProfile> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  if (optionalBodyBoolean(command, "deleteProfileDirectory") === true) {
    throw new Error(
      "deleteProfileDirectory is not supported by profile decommission; profile files are preserved",
    );
  }

  const configuredSessionIds = context.runtimeConfig.sessions
    .filter((session) => String(session.profileId) === profileId)
    .map((session) => String(session.sessionId));
  const activeSessions = await context.bridge.listSessions();
  const activeSessionIds = activeSessions
    .filter((session) => String(session.profileId) === profileId)
    .map((session) => String(session.sessionId));
  const sessionIds = [
    ...new Set([...configuredSessionIds, ...activeSessionIds]),
  ];
  const inFlightSessionIds = sessionIds.filter((sessionId) =>
    context.inFlightWakes.has(sessionId as SessionId),
  );
  if (inFlightSessionIds.length > 0) {
    throw new Error(
      `profile ${profileId} decommission blocked by in-flight wake(s): ${inFlightSessionIds.join(", ")}`,
    );
  }

  const sessionsArchived: string[] = [];
  for (const session of activeSessions) {
    if (
      String(session.profileId) !== profileId ||
      session.status === "archived"
    ) {
      continue;
    }
    await context.archiveSession(session.sessionId);
    sessionsArchived.push(String(session.sessionId));
  }

  const runtimeConfigFile = await readRuntimeConfigFileForMutation(context);
  const removed = removeProfileRuntimeConfigEntries(
    runtimeConfigFile,
    profileId,
    sessionIds,
  );

  const profilePath = safeProfileConfigPath(
    context.runtimeConfig.profilesDir,
    profileId,
  );
  const matchedRuntimeConfig =
    removed.brains +
      removed.sessions +
      removed.channelBindings +
      removed.mcpBindings +
      removed.scheduledJobs >
    0;
  if (
    !matchedRuntimeConfig &&
    sessionsArchived.length === 0 &&
    (profilePath === undefined || !existsSync(profilePath))
  ) {
    throw new Error(`profile ${profileId} was not found`);
  }

  await writeJsonFileAtomic(context.serviceConfigFile, runtimeConfigFile.value);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "profile_decommissioned",
    summaryPrefix: `Profile ${profileId} decommissioned`,
  });
  const brainHandle = await unregisterServiceProfileBrain(context, profileId);
  return {
    profileId,
    runtimeConfigPath: context.serviceConfigFile,
    ...(profilePath === undefined ? {} : { profilePath }),
    profileDirectoryPreserved: true,
    sessionsArchived,
    removed,
    brainHandle,
    skipped: {
      profileDirectory: "preserved",
    },
    applyResult,
  };
}

export async function deleteServiceProfile(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<DeletedServiceProfile> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  const confirmProfileId = requiredBodyString(command, "confirmProfileId");
  if (confirmProfileId !== profileId) {
    throw new Error(
      `profile delete confirmation mismatch: expected ${profileId}`,
    );
  }

  const configuredSessionIds = context.runtimeConfig.sessions
    .filter((session) => String(session.profileId) === profileId)
    .map((session) => String(session.sessionId));
  const activeSessions = await context.bridge.listSessions();
  const activeSessionIds = activeSessions
    .filter((session) => String(session.profileId) === profileId)
    .map((session) => String(session.sessionId));
  const sessionIds = [
    ...new Set([...configuredSessionIds, ...activeSessionIds]),
  ];
  const inFlightSessionIds = sessionIds.filter((sessionId) =>
    context.inFlightWakes.has(sessionId as SessionId),
  );
  if (inFlightSessionIds.length > 0) {
    throw new Error(
      `profile ${profileId} delete blocked by in-flight wake(s): ${inFlightSessionIds.join(", ")}`,
    );
  }

  const runtimeConfigFile = await readRuntimeConfigFileForMutation(context);
  const removed = removeProfileRuntimeConfigEntries(
    runtimeConfigFile,
    profileId,
    sessionIds,
  );

  const profilePath = safeProfileConfigPath(
    context.runtimeConfig.profilesDir,
    profileId,
  );
  const registryRecord =
    await context.bridge.getProfileRegistryRecord(profileId);
  const matchedRuntimeConfig =
    removed.brains +
      removed.sessions +
      removed.channelBindings +
      removed.mcpBindings +
      removed.scheduledJobs >
    0;
  if (
    !matchedRuntimeConfig &&
    sessionIds.length === 0 &&
    registryRecord === undefined &&
    (profilePath === undefined || !existsSync(profilePath))
  ) {
    throw new Error(`profile ${profileId} was not found`);
  }

  await writeJsonFileAtomic(context.serviceConfigFile, runtimeConfigFile.value);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "profile_deleted",
    summaryPrefix: `Profile ${profileId} deleted`,
  });
  const brainHandle = await unregisterServiceProfileBrain(context, profileId);

  let profileDirectoryDeleted = false;
  if (profilePath !== undefined && existsSync(profilePath)) {
    await rm(profilePath, { recursive: true, force: true });
    profileDirectoryDeleted = true;
  }

  const storagePurge = await context.bridge.purgeProfile(profileId);
  const purgedSessionIds = new Set([
    ...sessionIds,
    ...storagePurge.sessionIds.map(String),
  ]);
  context.forgetPurgedSessions(purgedSessionIds);

  return {
    profileId,
    runtimeConfigPath: context.serviceConfigFile,
    ...(profilePath === undefined ? {} : { profilePath }),
    profileDirectoryDeleted,
    sessionsDeleted: [...purgedSessionIds].sort(),
    removed,
    brainHandle,
    storagePurge,
    applyResult,
  };
}

export async function createServiceProfile(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): Promise<CreatedServiceProfile> {
  const profileId = requiredBodyString(command, "profileId");
  const displayName = optionalBodyString(command, "displayName");
  const soulMarkdown = optionalBodyMarkdown(command, "soulMarkdown");
  const memoryMarkdown = optionalBodyMarkdown(command, "memoryMarkdown");
  const workspaceCwd = optionalBodyString(command, "workspaceCwd");
  const providerAlias =
    optionalBodyString(command, "providerAlias") ?? "default";
  const externalMessageDeliveryPolicy = parseExternalMessageDeliveryPolicy(
    command.body.externalMessageDeliveryPolicy,
  );
  const modelProvider = await context.bridge.getModelProvider(providerAlias);
  if (modelProvider === undefined) {
    throw new Error(`model provider alias ${providerAlias} was not found`);
  }
  if (modelProvider.status !== "active") {
    throw new Error(
      `model provider alias ${providerAlias} is ${modelProvider.status}; active provider required`,
    );
  }
  const profilePath = safeProfileConfigPath(
    context.runtimeConfig.profilesDir,
    profileId,
  );
  const runtimeConfigFile = await readRuntimeConfigFileForMutation(context);
  const profiles = await loadRuntimeConfigProfiles(context);
  const requestedBrain =
    profileBrainFromBody(command.body.brain ?? command.body.brainSelection) ??
    defaultProfileBrainForModelProvider(modelProvider);
  const brainSelection = await context.bridge.planBrainSelection({
    ...(requestedBrain.module === undefined
      ? {}
      : { configuredModuleId: requestedBrain.module }),
    ...(requestedBrain.strategy === undefined
      ? {}
      : { configuredStrategyId: requestedBrain.strategy }),
    providerProtocol: modelProvider.protocol,
    providerKind: modelProvider.providerKind,
  });
  const plan = await planCreateProfileWithRust({
    bridge: context.bridge,
    runtimeConfig: context.runtimeConfig,
    profiles,
    request: {
      profileId,
      ...(displayName === undefined ? {} : { displayName }),
      ...(soulMarkdown === undefined ? {} : { soulMarkdown }),
      ...(memoryMarkdown === undefined ? {} : { memoryMarkdown }),
      agentId: optionalBodyString(command, "agentId"),
      sessionId: optionalBodyString(command, "sessionId"),
      implementationId: optionalBodyString(command, "implementationId"),
      kind: createProfileKind(command),
      ...(workspaceCwd === undefined ? {} : { workspaceCwd }),
      providerAlias,
      externalMessageDeliveryPolicy,
      brain: {
        module: brainSelection.module_id,
        strategy: brainSelection.selected_strategy_id,
      },
      mcpBindings: createProfileMcpBindingsFromBody(command.body.mcpBindings),
      mcpToolProfile: optionalBodyString(command, "mcpToolProfile"),
      source: profileCreateSourceFromBody(command.body.source),
      now: context.now(),
      profileFileExists:
        profilePath === undefined ? false : existsSync(profilePath),
    },
  });
  assertCreateProfilePlan(plan);

  const profileSeed = plan.profileSeed;
  const runtimeBrain = plan.runtimeBrain;
  const runtimeSession = plan.runtimeSession;
  const profileMcpConfig = plan.profileMcpConfig;
  if (!profileSeed || !runtimeBrain || !runtimeSession) {
    throw new Error(
      "create-profile plan did not include required profile/runtime entries",
    );
  }
  const profileFileAction = plan.fileAssetActions.find(
    (action) => action.kind === "write_profile_json",
  );
  const plannedProfilePath = join(
    context.runtimeConfig.profilesDir,
    profileFileAction?.relativePath ?? `${profileSeed.profileId}.json`,
  );
  const localToolProfileId = optionalBodyString(command, "localToolProfileId");
  const localToolProfile =
    localToolProfileId === undefined
      ? undefined
      : await createLocalToolProfileStore({
          bridge: context.bridge,
          now: context.now,
        }).resolve(localToolProfileId);
  const registryRuntimeSettings =
    plan.registryWrite === undefined
      ? {}
      : (optionalRecord(plan.registryWrite.activeRuntimeSettingsJson) ?? {});
  const registryWrite =
    plan.registryWrite === undefined
      ? undefined
      : {
          ...plan.registryWrite,
          activeRuntimeSettingsJson: {
            ...registryRuntimeSettings,
            ...(localToolProfile === undefined
              ? {}
              : {
                  localToolProfileId: localToolProfile.id,
                  toolPolicy: localToolProfile.toolPolicy,
                  profile: {
                    ...(optionalRecord(registryRuntimeSettings.profile) ?? {}),
                    localToolProfileId: localToolProfile.id,
                    toolPolicy: localToolProfile.toolPolicy,
                  },
                }),
          },
        };
  const [runtimeConfigSnapshot, profileFileSnapshot] = await Promise.all([
    captureFileSnapshot(context.serviceConfigFile),
    captureFileSnapshot(plannedProfilePath),
  ]);
  let registryRecord: CreatedServiceProfile["registryRecord"];
  let mutationStarted = false;
  let runtimeApplyAttempted = false;
  try {
    if (registryWrite !== undefined) {
      registryRecord =
        await context.bridge.createProfileRegistryRecord(registryWrite);
      mutationStarted = true;
    }

    await mkdir(context.runtimeConfig.profilesDir, { recursive: true });
    await writeJsonFileAtomic(plannedProfilePath, {
      profileId: profileSeed.profileId,
      ...(profileSeed.displayName === undefined
        ? {}
        : { displayName: profileSeed.displayName }),
      providerAlias: profileSeed.providerAlias,
      externalMessageDeliveryPolicy: profileSeed.externalMessageDeliveryPolicy,
      brain: profileSeed.brain,
      ...(profileMcpConfig === undefined
        ? {}
        : { mcpConfig: profileMcpConfig }),
      ...(localToolProfile === undefined
        ? {}
        : {
            localToolProfileId: localToolProfile.id,
            toolPolicy: localToolProfile.toolPolicy,
          }),
      skills: profileSeed.skillsMode,
    });
    mutationStarted = true;

    runtimeConfigFile.array("brains").push(runtimeBrain);
    runtimeConfigFile.array("sessions").push(runtimeSession);
    runtimeConfigFile.array("mcpBindings").push(...plan.runtimeMcpBindings);
    await writeJsonFileAtomic(
      context.serviceConfigFile,
      runtimeConfigFile.value,
    );

    runtimeApplyAttempted = true;
    const applyResult = await context.applyRuntimeConfigFromDisk({
      createMissingSessions: true,
      eventType: "profile_created",
      summaryPrefix: `Profile ${profileId} created`,
    });
    return {
      profileId: profileSeed.profileId,
      ...(profileSeed.displayName === undefined
        ? {}
        : { displayName: profileSeed.displayName }),
      agentId: runtimeSession.agentId,
      sessionId: runtimeSession.sessionId,
      implementationId: runtimeBrain.implementationId,
      profilePath: plannedProfilePath,
      runtimeConfigPath: context.serviceConfigFile,
      registryWrite,
      registryRecord,
      localToolProfileId: localToolProfile?.id,
      fileAssetActions: plan.fileAssetActions,
      derivedRuntimeActions: plan.derivedRuntimeActions,
      applyResult,
    };
  } catch (error) {
    if (!mutationStarted) throw error;
    const rollbackErrors = await rollbackFailedProfileCreate({
      context,
      profileId: profileSeed.profileId,
      runtimeConfigSnapshot,
      profileFileSnapshot,
      runtimeApplyAttempted,
    });
    if (rollbackErrors.length > 0) {
      throw new AggregateError(
        [error, ...rollbackErrors],
        `profile ${profileId} creation failed and rollback was incomplete`,
      );
    }
    throw error;
  }
}

async function rollbackFailedProfileCreate(input: {
  context: ServiceProfileAdminMutationContext;
  profileId: string;
  runtimeConfigSnapshot: FileSnapshot;
  profileFileSnapshot: FileSnapshot;
  runtimeApplyAttempted: boolean;
}): Promise<Error[]> {
  const errors: Error[] = [];
  const attempt = async (
    label: string,
    operation: () => Promise<void>,
  ): Promise<boolean> => {
    try {
      await operation();
      return true;
    } catch (error) {
      errors.push(
        new Error(
          `${label}: ${errorMessage(error, "profile create rollback failed")}`,
          { cause: error },
        ),
      );
      return false;
    }
  };

  const runtimeConfigRestored = await attempt(
    "restore service runtime config",
    () => restoreFileSnapshot(input.runtimeConfigSnapshot),
  );
  await attempt("restore profile config", () =>
    restoreFileSnapshot(input.profileFileSnapshot),
  );
  await attempt("unregister profile brain", async () => {
    await unregisterServiceProfileBrain(input.context, input.profileId);
  });
  await attempt("purge profile runtime state", async () => {
    const report = await input.context.bridge.purgeProfile(input.profileId);
    input.context.forgetPurgedSessions(report.sessionIds.map(String));
  });
  if (input.runtimeApplyAttempted && runtimeConfigRestored) {
    await attempt("restore active runtime config", async () => {
      await input.context.applyRuntimeConfigFromDisk({
        createMissingSessions: false,
        eventType: "profile_create_rolled_back",
        summaryPrefix: `Profile ${input.profileId} create rolled back`,
      });
    });
  }
  return errors;
}

async function loadRuntimeConfigProfiles(
  context: ServiceProfileAdminMutationContext,
): Promise<ProfileConfig[]> {
  const profileIds = new Set<ProfileId>();
  for (const session of context.runtimeConfig.sessions) {
    profileIds.add(session.profileId);
  }
  const profiles: ProfileConfig[] = [];
  for (const profileId of profileIds) {
    profiles.push(
      await loadProfileConfigWithRegistryPrompt(context, profileId),
    );
  }
  return profiles;
}

async function loadProfileConfigWithRegistryPrompt(
  context: ServiceProfileAdminMutationContext,
  profileId: ProfileId,
): Promise<ProfileConfig> {
  const profile = await loadProfileConfig(
    context.runtimeConfig.profilesDir,
    profileId,
  );
  const record = await context.bridge
    .getProfileRegistryRecord(String(profileId))
    .catch(() => undefined);
  if (record === undefined) return profile;
  return {
    ...profile,
    prompt: {
      ...(profile.prompt ?? {}),
      soulMarkdown: record.promptSoulMarkdown,
      memoryMarkdown: record.promptMemoryMarkdown,
    },
  };
}

function createProfileKind(
  command: AdminControlCommand,
): "full" | "worker" | "delegated" | undefined {
  const kind = optionalBodyString(command, "kind");
  if (kind === undefined) {
    return undefined;
  }
  if (kind === "full" || kind === "worker" || kind === "delegated") {
    return kind;
  }
  throw new Error("profile session kind must be full, worker, or delegated");
}

function createProfileMcpBindingsFromBody(input: unknown):
  | Array<{
      serverId: string;
      bindingId?: string;
      adapterId?: string;
      serverNames?: string[];
      transport?: string;
      toolProfileKey?: string;
    }>
  | undefined {
  if (input === undefined || input === null) return undefined;
  if (!Array.isArray(input)) {
    throw new Error("mcpBindings must be an array when provided");
  }
  return input.map((item, index) => {
    if (!isRecord(item)) {
      throw new Error(`mcpBindings[${index}] must be an object`);
    }
    const serverId = optionalString(item.serverId);
    if (serverId === undefined) {
      throw new Error(`mcpBindings[${index}].serverId is required`);
    }
    return compactRecord({
      serverId,
      bindingId: optionalString(item.bindingId),
      adapterId: optionalString(item.adapterId),
      serverNames:
        item.serverNames === undefined
          ? undefined
          : stringArray(item.serverNames, `mcpBindings[${index}].serverNames`),
      transport: optionalString(item.transport),
      toolProfileKey:
        optionalString(item.toolProfileKey) ?? optionalString(item.toolProfile),
    }) as {
      serverId: string;
      bindingId?: string;
      adapterId?: string;
      serverNames?: string[];
      transport?: string;
      toolProfileKey?: string;
    };
  });
}

function profileCreateSourceFromBody(input: unknown):
  | {
      templateId?: string;
      sourceProfileId?: string;
      sourceBundlePath?: string;
    }
  | undefined {
  const source = optionalRecord(input);
  if (!source) {
    return undefined;
  }
  const result = compactRecord({
    templateId: optionalString(source.templateId),
    sourceProfileId: optionalString(source.sourceProfileId),
    sourceBundlePath: optionalString(source.sourceBundlePath),
  }) as {
    templateId?: string;
    sourceProfileId?: string;
    sourceBundlePath?: string;
  };
  return Object.keys(result).length > 0 ? result : undefined;
}

function assertCreateProfilePlan(plan: NativeCreateProfilePlan): void {
  const errors = plan.diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (errors.length > 0) {
    const first = errors[0]!;
    const suffix =
      errors.length === 1
        ? ""
        : ` (${errors.length - 1} additional diagnostic${errors.length === 2 ? "" : "s"})`;
    throw new Error(
      `${first.path ? `${first.path}: ` : ""}${first.message}${suffix}`,
    );
  }
}

async function planRuntimeConfigValue(
  context: ServiceProfileAdminMutationContext,
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<RuntimeConfigDraftPlan> {
  const loaded = await loadRuntimeConfigProfilesForDraft(runtimeConfig);
  const diagnostics: RuntimeConfigDraftPlan["diagnostics"] =
    loaded.diagnostics.map((diagnostic) => ({
      severity: diagnostic.severity,
      code: diagnostic.code,
      path: diagnostic.path,
      message: diagnostic.message,
    }));
  let runtimePlan: NativeRuntimeConfigPlan | undefined;
  if (!diagnostics.some((diagnostic) => diagnostic.severity === "error")) {
    const plan = await planRuntimeConfigWithRust({
      bridge: context.bridge,
      runtimeConfig,
      profiles: loaded.profiles,
    });
    runtimePlan = plan;
    for (const diagnostic of plan.diagnostics) {
      diagnostics.push({
        severity: diagnostic.severity,
        code: diagnostic.code,
        path: diagnostic.path ?? "runtimeConfig",
        message: diagnostic.message,
      });
    }
  }
  return {
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    configPath: context.serviceConfigFile,
    diagnostics,
    implications: {
      configReloadRequired: true,
      createMissingSessions: false,
      explicitChannelLifecycle: true,
      explicitSessionLifecycle: true,
    },
    runtimePlan,
  };
}

function runtimeConfigSectionCounts(
  value: Record<string, unknown>,
): Record<string, number | undefined> {
  return {
    brains: sectionCount(value.brains),
    sessions: sectionCount(value.sessions),
    scheduledJobs: sectionCount(value.scheduledJobs),
    channelBindings: sectionCount(value.channelBindings),
    mcpServers: sectionCount(value.mcpServers),
    mcpBindings: sectionCount(value.mcpBindings),
  };
}

function sectionCount(value: unknown): number | undefined {
  return Array.isArray(value) ? value.length : undefined;
}

export async function unregisterServiceProfileBrain(
  context: ServiceProfileAdminMutationContext,
  profileId: string,
): Promise<DecommissionedServiceProfile["brainHandle"]> {
  try {
    const handle = await context.bridge.unregisterBrainImplementationForProfile(
      profileId as ProfileId,
    );
    return { action: "removed", handle };
  } catch (error) {
    if (isNativeNotFoundError(error)) {
      return { action: "already_absent" };
    }
    throw error;
  }
}

function isNativeNotFoundError(error: unknown): boolean {
  const message = error instanceof Error ? error.message.toLowerCase() : "";
  return message.includes("notfound") || message.includes("not found");
}

function removeProfileRuntimeConfigEntries(
  runtimeConfigFile: RuntimeConfigFileForMutation,
  profileId: string,
  sessionIds: string[],
): DecommissionedServiceProfile["removed"] {
  return {
    brains: removeRuntimeConfigEntries(
      runtimeConfigFile.array("brains"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId,
    ),
    sessions: removeRuntimeConfigEntries(
      runtimeConfigFile.array("sessions"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId,
    ),
    channelBindings: removeRuntimeConfigEntries(
      runtimeConfigFile.array("channelBindings"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId ||
        sessionIds.includes(
          runtimeEntryString(entry, "sessionId", "session_id") ?? "",
        ),
    ),
    mcpBindings: removeRuntimeConfigEntries(
      runtimeConfigFile.array("mcpBindings"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId ||
        sessionIds.includes(
          runtimeEntryString(entry, "sessionId", "session_id") ?? "",
        ),
    ),
    scheduledJobs: removeRuntimeConfigEntries(
      runtimeConfigFile.array("scheduledJobs"),
      (entry) =>
        sessionIds.includes(
          runtimeEntryString(entry, "targetSessionId", "target_session_id") ??
            "",
        ),
    ),
  };
}

function profileBrainFromBody(
  input: unknown,
): { module?: string; strategy?: string } | undefined {
  const brain = optionalRecord(input);
  if (!brain) {
    return undefined;
  }
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as { module?: string; strategy?: string };
}

export function defaultProfileBrainForModelProvider(
  provider: NativeModelProviderRecord,
): { module?: string; strategy?: string } {
  if (provider.protocol === "responses") {
    return { module: "openai-responses" };
  }
  return { module: "chat-completions" };
}

export interface RuntimeConfigFileForMutation {
  value: Record<string, unknown>;
  array(key: string): unknown[];
}

interface FileSnapshot {
  path: string;
  contents?: string;
}

export async function readRuntimeConfigFileForMutation(
  context: ServiceProfileAdminMutationContext,
): Promise<RuntimeConfigFileForMutation> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(context.serviceConfigFile, "utf8"));
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      parsed = {};
    } else {
      throw error;
    }
  }
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  if (parsed.profilesDir === undefined) {
    parsed.profilesDir = context.runtimeConfig.profilesDir;
  }
  if (
    context.runtimeConfig.skillsDir !== undefined &&
    parsed.skillsDir === undefined
  ) {
    parsed.skillsDir = context.runtimeConfig.skillsDir;
  }
  return {
    value: parsed,
    array(key) {
      const existing = parsed[key];
      if (existing === undefined) {
        const created: unknown[] = [];
        parsed[key] = created;
        return created;
      }
      if (!Array.isArray(existing)) {
        throw new Error(`runtime config ${key} must be an array`);
      }
      return existing;
    },
  };
}

export async function writeJsonFileAtomic(
  path: string,
  value: unknown,
): Promise<void> {
  await writeFileAtomic(path, `${JSON.stringify(value, null, 2)}\n`);
}

async function captureFileSnapshot(path: string): Promise<FileSnapshot> {
  try {
    return { path, contents: await readFile(path, "utf8") };
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return { path };
    throw error;
  }
}

async function restoreFileSnapshot(snapshot: FileSnapshot): Promise<void> {
  if (snapshot.contents === undefined) {
    await rm(snapshot.path, { force: true });
    return;
  }
  await writeFileAtomic(snapshot.path, snapshot.contents);
}

async function writeFileAtomic(path: string, contents: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const tmpPath = `${path}.${process.pid}.${Date.now()}.${randomBytes(8).toString("hex")}.tmp`;
  await writeFile(tmpPath, contents);
  await rename(tmpPath, path);
}

export function removeRuntimeConfigEntries(
  entries: unknown[],
  shouldRemove: (entry: Record<string, unknown>) => boolean,
): number {
  let removed = 0;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!isRecord(entry) || !shouldRemove(entry)) continue;
    entries.splice(index, 1);
    removed += 1;
  }
  return removed;
}

export function runtimeEntryString(
  entry: Record<string, unknown>,
  camelKey: string,
  snakeKey: string,
): string | undefined {
  const value = entry[camelKey] ?? entry[snakeKey];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function profileConfigDraftFromCommand(
  command: AdminControlCommand,
  profileId: string,
): Record<string, unknown> {
  const draft = optionalRecord(command.body.profileConfig);
  if (draft === undefined) {
    throw new Error("profileConfig object is required");
  }
  const next = structuredCloneRecord(draft);
  next.profileId = profileId;
  const soulMarkdown = optionalBodyString(command, "soulMarkdown");
  const memoryMarkdown = optionalBodyString(command, "memoryMarkdown");
  if (soulMarkdown !== undefined || memoryMarkdown !== undefined) {
    const prompt = optionalRecord(next.prompt);
    next.prompt = {
      ...(prompt ?? {}),
      ...(soulMarkdown === undefined ? {} : { soulMarkdown }),
      ...(memoryMarkdown === undefined ? {} : { memoryMarkdown }),
    };
  }
  return next;
}

function runtimeConfigDraftFromCommand(
  context: ServiceProfileAdminMutationContext,
  command: AdminControlCommand,
): RustyCrewRuntimeConfig {
  const draft = optionalRecord(command.body.runtimeConfig);
  if (draft === undefined) {
    throw new Error("runtimeConfig object is required");
  }
  rejectRetiredTurnLifetimeDraft(draft);
  return {
    profilesDir:
      optionalString(draft.profilesDir) ?? context.runtimeConfig.profilesDir,
    ...(optionalString(draft.skillsDir) === undefined
      ? {}
      : { skillsDir: optionalString(draft.skillsDir) }),
    brains: arrayValue(draft.brains).map((brain, index) =>
      runtimeConfigBrainDraft(brain, index),
    ),
    sessions: arrayValue(draft.sessions) as RustyCrewRuntimeConfig["sessions"],
    scheduledJobs: arrayValue(
      draft.scheduledJobs,
    ) as RustyCrewRuntimeConfig["scheduledJobs"],
    channelBindings: arrayValue(
      draft.channelBindings,
    ) as RustyCrewRuntimeConfig["channelBindings"],
    mcpServers: Object.hasOwn(draft, "mcpServers")
      ? arrayValue(draft.mcpServers).map((server) =>
          runtimeConfigMcpServerDraft(server),
        )
      : context.runtimeConfig.mcpServers,
    imageGeneration: Object.hasOwn(draft, "imageGeneration")
      ? (draft.imageGeneration as RustyCrewRuntimeConfig["imageGeneration"])
      : context.runtimeConfig.imageGeneration,
    mcpBindings: arrayValue(
      draft.mcpBindings,
    ) as RustyCrewRuntimeConfig["mcpBindings"],
  };
}

function runtimeConfigDraftFromFileValue(
  context: ServiceProfileAdminMutationContext,
  draft: Record<string, unknown>,
): RustyCrewRuntimeConfig {
  rejectRetiredTurnLifetimeDraft(draft);
  return {
    profilesDir:
      optionalString(draft.profilesDir) ?? context.runtimeConfig.profilesDir,
    ...(optionalString(draft.skillsDir) === undefined
      ? {}
      : { skillsDir: optionalString(draft.skillsDir) }),
    storage: context.runtimeConfig.storage,
    denObservation: context.runtimeConfig.denObservation,
    imageGeneration: Object.hasOwn(draft, "imageGeneration")
      ? (draft.imageGeneration as RustyCrewRuntimeConfig["imageGeneration"])
      : context.runtimeConfig.imageGeneration,
    brains: arrayValue(draft.brains).map((brain, index) =>
      runtimeConfigBrainDraft(brain, index),
    ),
    sessions: arrayValue(draft.sessions) as RustyCrewRuntimeConfig["sessions"],
    scheduledJobs: arrayValue(
      draft.scheduledJobs,
    ) as RustyCrewRuntimeConfig["scheduledJobs"],
    channelBindings: arrayValue(
      draft.channelBindings,
    ) as RustyCrewRuntimeConfig["channelBindings"],
    mcpServers: Object.hasOwn(draft, "mcpServers")
      ? arrayValue(draft.mcpServers).map((server) =>
          runtimeConfigMcpServerDraft(server),
        )
      : context.runtimeConfig.mcpServers,
    mcpBindings: arrayValue(
      draft.mcpBindings,
    ) as RustyCrewRuntimeConfig["mcpBindings"],
  };
}

function rejectRetiredTurnLifetimeDraft(draft: Record<string, unknown>): void {
  if (Object.hasOwn(draft, "wakeTimeout")) {
    throw new Error(
      "runtimeConfig.wakeTimeout is retired; logical turns have no finite lifetime",
    );
  }
  for (const [index, session] of arrayValue(draft.sessions).entries()) {
    if (isRecord(session) && Object.hasOwn(session, "turnTimeoutMs")) {
      throw new Error(
        `runtimeConfig.sessions[${index}].turnTimeoutMs is retired`,
      );
    }
  }
}

function runtimeConfigMcpServerDraft(value: unknown): RustyCrewMcpServerConfig {
  if (!isRecord(value)) {
    throw new Error("runtimeConfig.mcpServers entries must be objects");
  }
  return mcpServerWriteFromBody(value, undefined);
}

function runtimeConfigBrainDraft(
  value: unknown,
  index: number,
): RustyCrewRuntimeConfig["brains"][number] {
  if (!isRecord(value)) {
    throw new Error(`runtimeConfig.brains[${index}] must be an object`);
  }
  const profileId = optionalString(value.profileId);
  if (profileId === undefined) {
    throw new Error(`runtimeConfig.brains[${index}].profileId is required`);
  }
  return {
    profileId: profileId as ProfileId,
    implementationId: (optionalString(value.implementationId) ??
      `${profileId}-brain`) as never,
  };
}

async function loadRuntimeConfigProfilesReplacing(
  context: ServiceProfileAdminMutationContext,
  profileId: string,
  replacement: ProfileConfig,
): Promise<ProfileConfig[]> {
  const profileIds = new Set<ProfileId>();
  for (const brain of context.runtimeConfig.brains) {
    profileIds.add(brain.profileId);
  }
  for (const session of context.runtimeConfig.sessions) {
    profileIds.add(session.profileId);
  }
  profileIds.add(profileId as ProfileId);
  const profiles: ProfileConfig[] = [];
  for (const candidateId of profileIds) {
    if (String(candidateId) === profileId) {
      profiles.push(replacement);
      continue;
    }
    profiles.push(
      await loadProfileConfigWithRegistryPrompt(context, candidateId),
    );
  }
  return profiles;
}

async function loadRuntimeConfigProfilesForDraft(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<{
  profiles: ProfileConfig[];
  diagnostics: Array<{
    severity: "error";
    code: string;
    path: string;
    message: string;
  }>;
}> {
  const profileIds = new Set<ProfileId>();
  for (const brain of runtimeConfig.brains) profileIds.add(brain.profileId);
  for (const session of runtimeConfig.sessions)
    profileIds.add(session.profileId);
  const profiles: ProfileConfig[] = [];
  const diagnostics: Array<{
    severity: "error";
    code: string;
    path: string;
    message: string;
  }> = [];
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

function profileRuntimeBrainChanged(
  before: ProfileConfig | undefined,
  after: ProfileConfig | undefined,
): boolean {
  if (before === undefined || after === undefined) return false;
  return (
    before.providerAlias !== after.providerAlias ||
    JSON.stringify(before.modelConfig) !== JSON.stringify(after.modelConfig) ||
    JSON.stringify(before.brain ?? {}) !== JSON.stringify(after.brain ?? {})
  );
}

function profileMcpChanged(
  before: ProfileConfig | undefined,
  after: ProfileConfig | undefined,
): boolean {
  if (before === undefined || after === undefined) return false;
  return (
    JSON.stringify(before.mcpConfig ?? {}) !==
    JSON.stringify(after.mcpConfig ?? {})
  );
}

function safeProfileConfigPath(
  profilesDir: string,
  profileId: string,
): string | undefined {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(profileId)) {
    return undefined;
  }
  return join(profilesDir, `${profileId}.json`);
}

function requiredBodyString(command: AdminControlCommand, key: string): string {
  const value = optionalBodyString(command, key);
  if (!value) throw new Error(`control body field ${key} is required`);
  return value;
}

function optionalBodyString(
  command: AdminControlCommand,
  key: string,
): string | undefined {
  const value = command.body[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function optionalBodyMarkdown(
  command: AdminControlCommand,
  key: string,
): string | undefined {
  const value = command.body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") {
    throw new Error(`control body field ${key} must be a string or null`);
  }
  return value;
}

function optionalBodyBoolean(
  command: AdminControlCommand,
  key: string,
): boolean | undefined {
  const value = command.body[key];
  return typeof value === "boolean" ? value : undefined;
}

function structuredCloneRecord(
  record: Record<string, unknown>,
): Record<string, unknown> {
  return JSON.parse(JSON.stringify(record)) as Record<string, unknown>;
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return isRecord(value) ? value : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`${fieldName} must be an array`);
  }
  return value.map((item, index) => {
    const parsed = optionalString(item);
    if (parsed === undefined) {
      throw new Error(`${fieldName}[${index}] must be a non-empty string`);
    }
    return parsed;
  });
}

function compactRecord(
  value: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(
      ([, entry]) => entry !== null && entry !== undefined,
    ),
  );
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
