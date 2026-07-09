import type {
  BrainImplementationHandle,
  BrainImplementationId,
  ProfileId,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeProfileRegistryRecord,
  NativeProfileRegistryWrite,
} from "@rusty-crew/native-bridge";
import type { AdminControlCommand } from "./admin-control-api.js";
import type {
  RuntimeConfigDraftPlan,
  RuntimeConfigFileForMutation,
} from "./service-profile-admin-mutations.js";
import type {
  RustyCrewBrainRuntimeRebuildResult,
  RustyCrewRuntimeConfig,
  RustyCrewRuntimeConfigApplyResult,
} from "./service-runtime-config.js";

export interface ServiceRuntimeRebuildEvent {
  source: string;
  eventType: string;
  summary: string;
  severity?: "info" | "warning" | "error";
}

export interface ServiceRuntimeRebuildMcpRefreshResult {
  action: "refresh_after_rebuild";
  bindingIds: string[];
  refreshedBindingIds: string[];
  degradedBindingIds: string[];
  missingBindingIds: string[];
  results: Array<{
    bindingId: string;
    sessionId?: string;
    status: "refreshed" | "degraded" | "missing";
    reasonCode?: string;
    summary: string;
  }>;
}

export interface ServiceRuntimeRebuildPlan {
  scope: "session" | "profile";
  profileId: string;
  sessionIds: string[];
  applySupported: true;
  requiredAction: "brain_hot_swap_required";
  preservesSessionId: boolean;
  preservesHistory: boolean;
  replacementSession?: {
    mode: "derive_from_prior_session";
    explicitApplyRequired: true;
    oldSessionId: string;
    requestedNewSessionId?: string;
  };
  configReload: {
    implicit: false;
    requiredBeforeApply: boolean;
  };
  providerState: {
    action: "discard" | "migrate" | "unsupported";
    reason: string;
    migrationId?: string;
    clearedSessions?: number;
  };
  queuedMessages: {
    action:
      | "preserve_existing_queue_without_redelivery"
      | "start_replacement_session_with_empty_queue";
    ttlPolicy: "unchanged";
  };
  channelBindings: {
    action: "unchanged" | "move_to_replacement_session";
    bindingIds: string[];
  };
  mcp: {
    action: "refresh_after_rebuild";
    bindingIds: string[];
    refreshedBindingIds?: string[];
    degradedBindingIds?: string[];
    missingBindingIds?: string[];
    results?: ServiceRuntimeRebuildMcpRefreshResult["results"];
  };
  diagnostics: {
    brainModule?: string;
    profileConfigured: boolean;
    sessionsConfigured: number;
    sessionsActive: number;
  };
}

export interface ServiceRuntimeRebuildApplyResult extends ServiceRuntimeRebuildPlan {
  profileRegistry?: ServiceRuntimeReplacementSessionResult["profileRegistry"];
  apply:
    | {
        status: "completed";
        handle: BrainImplementationHandle;
        implementationId: BrainImplementationId;
        audited: true;
        replacementSession?: ServiceRuntimeReplacementSessionResult;
      }
    | {
        status: "blocked";
        reasonCode:
          | "runtime_rebuild_in_flight"
          | "provider_state_rebuild_unsupported"
          | "provider_state_migration_not_implemented";
        blockedSessionIds: string[];
      };
}

export interface ServiceRuntimeReplacementSessionResult {
  oldSessionId: string;
  newSessionId: string;
  profileRegistry: {
    action: "update_session_refs" | "record_missing" | "unchanged";
    updatedProfileId?: string;
    updatedRefIds: string[];
  };
  channelBindings: {
    action: "unchanged" | "move_to_replacement_session";
    bindingIds: string[];
  };
  mcpBindings: {
    action: "move_to_replacement_session";
    bindingIds: string[];
  };
  scheduledJobs: {
    action: "move_to_replacement_session";
    jobIds: string[];
  };
  queuedMessages: {
    action: "start_replacement_session_with_empty_queue";
    oldSessionQueuePreserved: true;
    expiredQueuedMessagesCopied: false;
  };
}

export interface ServiceRuntimeReplacementConfigPlan {
  oldSessionId: string;
  newSessionId: string;
  runtimeConfigFile: RuntimeConfigFileForMutation;
  validation: RuntimeConfigDraftPlan;
  channelBindings: ServiceRuntimeReplacementSessionResult["channelBindings"];
  mcpBindings: ServiceRuntimeReplacementSessionResult["mcpBindings"];
  scheduledJobs: ServiceRuntimeReplacementSessionResult["scheduledJobs"];
}

export interface ServiceRuntimeRebuildContext {
  bridge: Pick<
    NativeBridgeModule,
    | "listSessions"
    | "clearBrainProviderState"
    | "getProfileRegistryRecord"
    | "updateProfileRegistryRecord"
  >;
  get runtimeConfig(): RustyCrewRuntimeConfig;
  get runtimeConfigApplyResult(): RustyCrewRuntimeConfigApplyResult;
  inFlightWakes: ReadonlySet<SessionId>;
  now(): string;
  nextReplacementSessionId(
    session: Pick<SessionState, "agentId" | "sessionId">,
  ): string;
  readRuntimeConfigFile(): Promise<RuntimeConfigFileForMutation>;
  validateRuntimeConfigFile(value: unknown): Promise<RuntimeConfigDraftPlan>;
  writeRuntimeConfigFile(value: unknown): Promise<void>;
  serviceSessionById(sessionId: string): Promise<SessionState>;
  archiveSession(sessionId: SessionId): Promise<void>;
  applyRuntimeConfigFromDisk(options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<RustyCrewRuntimeConfigApplyResult>;
  rebuildBrainRuntime(
    profileId: ProfileId,
  ): Promise<RustyCrewBrainRuntimeRebuildResult>;
  refreshMcpBindingsAfterRuntimeRebuild(
    bindingIds: readonly string[],
    command: AdminControlCommand,
  ): Promise<ServiceRuntimeRebuildMcpRefreshResult>;
  recordEvent(event: ServiceRuntimeRebuildEvent): void;
}

export async function planServiceRuntimeRebuild(
  context: ServiceRuntimeRebuildContext,
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildPlan> {
  const scope = command.target.scope;
  if (scope !== "session" && scope !== "profile") {
    throw new Error("runtime rebuild target scope must be session or profile");
  }

  const activeSessions = await context.bridge.listSessions();
  const configuredSessions = context.runtimeConfig.sessions;
  const replaceSessionIdentity = runtimeRebuildReplacesSessionIdentity(command);
  const configuredProfileIds = new Set(
    context.runtimeConfig.brains.map((brain) => String(brain.profileId)),
  );

  let profileId: string;
  let sessionIds: string[];
  if (scope === "session") {
    const sessionId = command.target.sessionId;
    if (!sessionId) throw new Error("runtime rebuild session id is required");
    const activeSession = activeSessions.find(
      (session) => session.sessionId === sessionId,
    );
    const configuredSession = configuredSessions.find(
      (session) => session.sessionId === sessionId,
    );
    profileId = activeSession?.profileId ?? configuredSession?.profileId ?? "";
    if (!profileId) throw new Error(`session ${sessionId} was not found`);
    sessionIds = [sessionId];
  } else {
    if (replaceSessionIdentity) {
      throw new Error(
        "replacement session rebuild is only supported for a single session target",
      );
    }
    profileId = command.target.profileId ?? "";
    if (!profileId) throw new Error("runtime rebuild profile id is required");
    if (!configuredProfileIds.has(profileId)) {
      throw new Error(`profile ${profileId} is not configured for a brain`);
    }
    sessionIds = [
      ...new Set(
        [
          ...activeSessions
            .filter((session) => session.profileId === profileId)
            .map((session) => session.sessionId),
          ...configuredSessions
            .filter((session) => session.profileId === profileId)
            .map((session) => session.sessionId),
        ].filter(Boolean),
      ),
    ];
  }

  const channelBindingIds = context.runtimeConfig.channelBindings
    .filter(
      (binding) =>
        binding.sessionId !== undefined &&
        sessionIds.includes(binding.sessionId),
    )
    .map((binding) => binding.bindingId);
  const mcpBindingIds = context.runtimeConfig.mcpBindings
    .filter(
      (binding) =>
        binding.sessionId !== undefined &&
        sessionIds.includes(binding.sessionId),
    )
    .map((binding) => binding.bindingId);
  const brainModule =
    context.runtimeConfigApplyResult.brainModulesByProfileId[profileId]
      ?.moduleId;
  const brainDiagnostics =
    context.runtimeConfigApplyResult.brainDiagnosticsByProfileId[profileId];
  const providerStateRebuild = brainDiagnostics?.providerStateRebuild ?? {
    action: "unsupported" as const,
    reason:
      "brain module did not declare provider-state rebuild handling; fail closed",
  };

  return {
    scope,
    profileId,
    sessionIds,
    applySupported: true,
    requiredAction: "brain_hot_swap_required",
    preservesSessionId: !replaceSessionIdentity,
    preservesHistory: !replaceSessionIdentity,
    ...(replaceSessionIdentity
      ? {
          replacementSession: {
            mode: "derive_from_prior_session" as const,
            explicitApplyRequired: true as const,
            oldSessionId: sessionIds[0] ?? "",
            requestedNewSessionId: optionalBodyString(command, "newSessionId"),
          },
        }
      : {}),
    configReload: {
      implicit: false,
      requiredBeforeApply: false,
    },
    providerState: {
      action: providerStateRebuild.action,
      reason: providerStateRebuild.reason,
      ...(providerStateRebuild.migrationId === undefined
        ? {}
        : { migrationId: providerStateRebuild.migrationId }),
    },
    queuedMessages: {
      action: replaceSessionIdentity
        ? "start_replacement_session_with_empty_queue"
        : "preserve_existing_queue_without_redelivery",
      ttlPolicy: "unchanged",
    },
    channelBindings: {
      action:
        replaceSessionIdentity &&
        replacementChannelBindingAction(command) === "move"
          ? "move_to_replacement_session"
          : "unchanged",
      bindingIds: channelBindingIds,
    },
    mcp: {
      action: "refresh_after_rebuild",
      bindingIds: mcpBindingIds,
    },
    diagnostics: {
      brainModule,
      profileConfigured: configuredProfileIds.has(profileId),
      sessionsConfigured: configuredSessions.filter(
        (session) => session.profileId === profileId,
      ).length,
      sessionsActive: activeSessions.filter(
        (session) => session.profileId === profileId,
      ).length,
    },
  };
}

export async function applyServiceRuntimeRebuild(
  context: ServiceRuntimeRebuildContext,
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildApplyResult> {
  const plan = await planServiceRuntimeRebuild(context, command);
  const activeProfileSessionIds = (await context.bridge.listSessions())
    .filter((session) => session.profileId === plan.profileId)
    .map((session) => session.sessionId);
  const blockedSessionIds = activeProfileSessionIds.filter((sessionId) =>
    context.inFlightWakes.has(sessionId as SessionId),
  );
  if (plan.providerState.action === "unsupported") {
    context.recordEvent({
      source: "service-host",
      eventType: "runtime_rebuild_blocked",
      severity: "warning",
      summary: `Runtime rebuild for profile ${plan.profileId} blocked because provider-state handling is unsupported: ${plan.providerState.reason}.`,
    });
    return {
      ...plan,
      apply: {
        status: "blocked",
        reasonCode: "provider_state_rebuild_unsupported",
        blockedSessionIds: [],
      },
    };
  }
  if (plan.providerState.action === "migrate") {
    context.recordEvent({
      source: "service-host",
      eventType: "runtime_rebuild_blocked",
      severity: "warning",
      summary: `Runtime rebuild for profile ${plan.profileId} blocked because provider-state migration is not implemented: ${plan.providerState.reason}.`,
    });
    return {
      ...plan,
      apply: {
        status: "blocked",
        reasonCode: "provider_state_migration_not_implemented",
        blockedSessionIds: [],
      },
    };
  }
  if (blockedSessionIds.length > 0) {
    context.recordEvent({
      source: "service-host",
      eventType: "runtime_rebuild_blocked",
      severity: "warning",
      summary: `Runtime rebuild for profile ${plan.profileId} blocked by in-flight wake(s): ${blockedSessionIds.join(", ")}.`,
    });
    return {
      ...plan,
      apply: {
        status: "blocked",
        reasonCode: "runtime_rebuild_in_flight",
        blockedSessionIds,
      },
    };
  }

  if (runtimeRebuildReplacesSessionIdentity(command)) {
    return applyServiceRuntimeRebuildWithReplacementSession(
      context,
      command,
      plan,
    );
  }

  const previousBrain =
    context.runtimeConfigApplyResult.brainHandlesByProfileId[plan.profileId];
  let clearedSessions = 0;
  const providerStateMode =
    context.runtimeConfigApplyResult.brainDiagnosticsByProfileId[plan.profileId]
      ?.providerStateMode;
  if (
    previousBrain !== undefined &&
    plan.providerState.action === "discard" &&
    providerStateMode !== undefined &&
    providerStateMode !== "unused"
  ) {
    for (const sessionId of plan.sessionIds) {
      await context.bridge.clearBrainProviderState({
        brain: previousBrain,
        sessionId: sessionId as SessionId,
        wakeId: `runtime-rebuild-${Date.now()}-${sessionId}`,
      });
      clearedSessions += 1;
    }
  }

  const rebuild = await context.rebuildBrainRuntime(
    plan.profileId as ProfileId,
  );
  applyBrainRuntimeRebuild(context, plan.profileId, rebuild);
  context.recordEvent({
    source: "service-host",
    eventType: "runtime_rebuild_applied",
    summary: `Runtime rebuild applied for profile ${plan.profileId} with brain handle ${rebuild.handle}.`,
  });
  const mcpRefresh = await context.refreshMcpBindingsAfterRuntimeRebuild(
    plan.mcp.bindingIds,
    command,
  );

  return {
    ...plan,
    providerState: {
      ...plan.providerState,
      clearedSessions,
    },
    mcp: mcpRefresh,
    apply: {
      status: "completed",
      handle: rebuild.handle,
      implementationId: rebuild.implementationId,
      audited: true,
    },
  };
}

export async function replaceRuntimeSessionInConfig(
  context: ServiceRuntimeRebuildContext,
  oldSession: SessionState,
  newSessionId: string,
  channelBindingAction: "move" | "unchanged",
): Promise<ServiceRuntimeReplacementSessionResult> {
  const plan = await planRuntimeSessionReplacementInConfig(
    context,
    oldSession,
    newSessionId,
    channelBindingAction,
  );
  return commitRuntimeSessionReplacementInConfig(context, oldSession, plan);
}

export async function planRuntimeSessionReplacementInConfig(
  context: ServiceRuntimeRebuildContext,
  oldSession: SessionState,
  newSessionId: string,
  channelBindingAction: "move" | "unchanged",
): Promise<ServiceRuntimeReplacementConfigPlan> {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(newSessionId)) {
    throw new Error("replacement session id contains unsupported characters");
  }
  const runtimeConfigFile = await context.readRuntimeConfigFile();
  const sessions = runtimeConfigFile.array("sessions");
  const sessionEntry = sessions.find(
    (entry): entry is Record<string, unknown> =>
      isRecord(entry) &&
      runtimeEntryString(entry, "sessionId", "session_id") ===
        oldSession.sessionId,
  );
  if (sessionEntry === undefined) {
    sessions.push(runtimeConfigSessionEntryFromState(oldSession, newSessionId));
  } else {
    sessionEntry.sessionId = newSessionId;
    delete sessionEntry.session_id;
  }

  const channelBindingIds =
    channelBindingAction === "move"
      ? replaceRuntimeConfigSessionRefs(
          runtimeConfigFile.array("channelBindings"),
          oldSession.sessionId,
          newSessionId,
          "sessionId",
          "session_id",
          "bindingId",
          "binding_id",
        )
      : context.runtimeConfig.channelBindings
          .filter((binding) => binding.sessionId === oldSession.sessionId)
          .map((binding) => binding.bindingId);
  const mcpBindingIds = replaceRuntimeConfigSessionRefs(
    runtimeConfigFile.array("mcpBindings"),
    oldSession.sessionId,
    newSessionId,
    "sessionId",
    "session_id",
    "bindingId",
    "binding_id",
  );
  const scheduledJobIds = replaceRuntimeConfigSessionRefs(
    runtimeConfigFile.array("scheduledJobs"),
    oldSession.sessionId,
    newSessionId,
    "targetSessionId",
    "target_session_id",
    "id",
    "id",
  );

  const validation = await context.validateRuntimeConfigFile(
    runtimeConfigFile.value,
  );
  if (!validation.ok) {
    const errors = validation.diagnostics.filter(
      (diagnostic) => diagnostic.severity === "error",
    );
    const message =
      errors[0]?.message ??
      "runtime config session replacement validation failed";
    throw new Error(message);
  }

  return {
    oldSessionId: oldSession.sessionId,
    newSessionId,
    runtimeConfigFile,
    validation,
    channelBindings: {
      action:
        channelBindingAction === "move"
          ? "move_to_replacement_session"
          : "unchanged",
      bindingIds: channelBindingIds,
    },
    mcpBindings: {
      action: "move_to_replacement_session",
      bindingIds: mcpBindingIds,
    },
    scheduledJobs: {
      action: "move_to_replacement_session",
      jobIds: scheduledJobIds,
    },
  };
}

export async function commitRuntimeSessionReplacementInConfig(
  context: ServiceRuntimeRebuildContext,
  oldSession: SessionState,
  plan: ServiceRuntimeReplacementConfigPlan,
): Promise<ServiceRuntimeReplacementSessionResult> {
  await context.writeRuntimeConfigFile(plan.runtimeConfigFile.value);
  const profileRegistry = await replaceProfileRegistrySessionRefs(
    context,
    oldSession,
    plan.newSessionId,
  );
  return {
    oldSessionId: oldSession.sessionId,
    newSessionId: plan.newSessionId,
    profileRegistry,
    channelBindings: plan.channelBindings,
    mcpBindings: plan.mcpBindings,
    scheduledJobs: plan.scheduledJobs,
    queuedMessages: {
      action: "start_replacement_session_with_empty_queue",
      oldSessionQueuePreserved: true,
      expiredQueuedMessagesCopied: false,
    },
  };
}

export function runtimeRebuildAffectedIds(
  plan: ServiceRuntimeRebuildPlan,
): Record<string, string | number> {
  const affected: Record<string, string | number> = {
    profileId: plan.profileId,
    sessionCount: plan.sessionIds.length,
  };
  if (plan.sessionIds.length === 1) {
    affected.sessionId = plan.sessionIds[0] ?? "";
  }
  return affected;
}

async function applyServiceRuntimeRebuildWithReplacementSession(
  context: ServiceRuntimeRebuildContext,
  command: AdminControlCommand,
  plan: ServiceRuntimeRebuildPlan,
): Promise<ServiceRuntimeRebuildApplyResult> {
  if (plan.scope !== "session") {
    throw new Error(
      "replacement session rebuild requires a session-scoped target",
    );
  }
  const oldSessionId = plan.sessionIds[0];
  if (!oldSessionId)
    throw new Error("replacement session rebuild requires a session id");
  const oldSession = await context.serviceSessionById(oldSessionId);
  if (oldSession.status === "archived") {
    throw new Error(`session ${oldSessionId} is already archived`);
  }
  const newSessionId =
    optionalBodyString(command, "newSessionId") ??
    context.nextReplacementSessionId(oldSession);
  if (newSessionId === oldSessionId) {
    throw new Error(
      "replacement session id must differ from the old session id",
    );
  }
  const existingSession = (await context.bridge.listSessions()).find(
    (session) => session.sessionId === newSessionId,
  );
  if (existingSession !== undefined) {
    throw new Error(`replacement session ${newSessionId} already exists`);
  }

  const previousBrain =
    context.runtimeConfigApplyResult.brainHandlesByProfileId[plan.profileId];
  const providerStateMode =
    context.runtimeConfigApplyResult.brainDiagnosticsByProfileId[plan.profileId]
      ?.providerStateMode;
  let clearedSessions = 0;
  if (
    previousBrain !== undefined &&
    plan.providerState.action === "discard" &&
    providerStateMode !== undefined &&
    providerStateMode !== "unused"
  ) {
    await context.bridge.clearBrainProviderState({
      brain: previousBrain,
      sessionId: oldSessionId as SessionId,
      wakeId: `runtime-rebuild-replace-${Date.now()}-${oldSessionId}`,
    });
    clearedSessions = 1;
  }

  const replacement = await replaceRuntimeSessionInConfig(
    context,
    oldSession,
    newSessionId,
    replacementChannelBindingAction(command),
  );
  await context.archiveSession(oldSessionId as SessionId);
  await context.applyRuntimeConfigFromDisk({
    createMissingSessions: true,
    eventType: "runtime_rebuild_replacement_session_created",
    summaryPrefix: `Runtime rebuild replaced session ${oldSessionId}`,
  });
  const rebuild = await context.rebuildBrainRuntime(
    plan.profileId as ProfileId,
  );
  applyBrainRuntimeRebuild(context, plan.profileId, rebuild);
  context.recordEvent({
    source: "service-host",
    eventType: "runtime_rebuild_replacement_session_applied",
    summary: `Runtime rebuild archived ${oldSessionId} and created replacement session ${newSessionId}.`,
  });
  const mcpRefresh = await context.refreshMcpBindingsAfterRuntimeRebuild(
    replacement.mcpBindings.bindingIds,
    command,
  );

  return {
    ...plan,
    sessionIds: [newSessionId],
    providerState: {
      ...plan.providerState,
      clearedSessions,
    },
    queuedMessages: {
      action: "start_replacement_session_with_empty_queue",
      ttlPolicy: "unchanged",
    },
    channelBindings: replacement.channelBindings,
    profileRegistry: replacement.profileRegistry,
    mcp: mcpRefresh,
    apply: {
      status: "completed",
      handle: rebuild.handle,
      implementationId: rebuild.implementationId,
      audited: true,
      replacementSession: {
        ...replacement,
        queuedMessages: {
          action: "start_replacement_session_with_empty_queue",
          oldSessionQueuePreserved: true,
          expiredQueuedMessagesCopied: false,
        },
      },
    },
    diagnostics: plan.diagnostics,
  };
}

function applyBrainRuntimeRebuild(
  context: ServiceRuntimeRebuildContext,
  profileId: string,
  rebuild: RustyCrewBrainRuntimeRebuildResult,
): void {
  context.runtimeConfigApplyResult.brainHandlesByProfileId[profileId] =
    rebuild.handle;
  context.runtimeConfigApplyResult.brainModulesByProfileId[profileId] =
    rebuild.module;
  context.runtimeConfigApplyResult.brainDiagnosticsByProfileId[profileId] =
    rebuild.diagnostics;
}

function runtimeRebuildReplacesSessionIdentity(
  command: AdminControlCommand,
): boolean {
  const mode =
    optionalBodyString(command, "sessionIdentity") ??
    optionalBodyString(command, "sessionIdentityMode");
  if (mode === undefined || mode === "preserve") return false;
  if (mode === "replace") return true;
  throw new Error("sessionIdentity must be preserve or replace");
}

function replacementChannelBindingAction(
  command: AdminControlCommand,
): "move" | "unchanged" {
  const action =
    optionalBodyString(command, "channelBindingAction") ?? "unchanged";
  if (action === "move" || action === "unchanged") return action;
  throw new Error("channelBindingAction must be move or unchanged");
}

async function replaceProfileRegistrySessionRefs(
  context: ServiceRuntimeRebuildContext,
  oldSession: SessionState,
  newSessionId: string,
): Promise<ServiceRuntimeReplacementSessionResult["profileRegistry"]> {
  const record = await context.bridge.getProfileRegistryRecord(
    oldSession.profileId,
  );
  if (record === undefined) {
    return { action: "record_missing", updatedRefIds: [] };
  }

  const now = context.now();
  const updatedRefIds: string[] = [];
  const derivedRuntimeRefs = record.derivedRuntimeRefs.map((ref) => {
    if (ref.refKind !== "session" || ref.refId !== oldSession.sessionId) {
      return ref;
    }
    updatedRefIds.push(ref.refId);
    return {
      ...ref,
      refId: newSessionId,
      updatedAt: now,
      metadataJson: replaceRuntimeRefSessionMetadata(
        ref.metadataJson,
        newSessionId,
      ),
    };
  });

  if (updatedRefIds.length === 0) {
    return {
      action: "unchanged",
      updatedProfileId: record.profileId,
      updatedRefIds: [],
    };
  }

  await context.bridge.updateProfileRegistryRecord({
    write: profileRegistryRecordToWrite(
      {
        ...record,
        derivedRuntimeRefs,
        updatedAt: now,
      },
      now,
    ),
    expectedRevision: record.revision,
  });

  return {
    action: "update_session_refs",
    updatedProfileId: record.profileId,
    updatedRefIds,
  };
}

function replaceRuntimeRefSessionMetadata(
  metadata: unknown,
  newSessionId: string,
): unknown {
  if (!isRecord(metadata)) return metadata;
  const next = { ...metadata };
  if (next.session_id !== undefined) next.session_id = newSessionId;
  if (next.sessionId !== undefined) next.sessionId = newSessionId;
  return next;
}

function runtimeConfigSessionEntryFromState(
  session: SessionState,
  newSessionId: string,
): Record<string, unknown> {
  return compactRecord({
    sessionId: newSessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    resourceLimits: compactRecord({
      workdir: session.resourceLimits.workdir,
      maxDurationMs: session.resourceLimits.maxDurationMs,
      maxDelegationDepth: session.resourceLimits.maxDelegationDepth,
    }),
    maxHistoryMessages: session.historyWindow?.maxMessages,
  });
}

function replaceRuntimeConfigSessionRefs(
  entries: unknown[],
  oldSessionId: string,
  newSessionId: string,
  sessionCamelKey: string,
  sessionSnakeKey: string,
  idCamelKey: string,
  idSnakeKey: string,
): string[] {
  const changedIds: string[] = [];
  for (const entry of entries) {
    if (!isRecord(entry)) continue;
    if (
      runtimeEntryString(entry, sessionCamelKey, sessionSnakeKey) !==
      oldSessionId
    ) {
      continue;
    }
    entry[sessionCamelKey] = newSessionId;
    if (sessionSnakeKey !== sessionCamelKey) delete entry[sessionSnakeKey];
    const id = runtimeEntryString(entry, idCamelKey, idSnakeKey);
    if (id !== undefined) changedIds.push(id);
  }
  return changedIds;
}

function profileRegistryRecordToWrite(
  record: NativeProfileRegistryRecord,
  now: string,
): NativeProfileRegistryWrite {
  return {
    profileId: record.profileId,
    lifecycleStatus: record.lifecycleStatus,
    displayName: record.displayName,
    summary: record.summary,
    defaultSessionKind: record.defaultSessionKind,
    agentId: record.agentId,
    ownerId: record.ownerId,
    promptSoulMarkdown: record.promptSoulMarkdown,
    promptMemoryMarkdown: record.promptMemoryMarkdown,
    activeRuntimeSettingsJson: record.activeRuntimeSettingsJson ?? {},
    sourceAssetRefs: record.sourceAssetRefs,
    derivedRuntimeRefs: record.derivedRuntimeRefs,
    importExport: record.importExport,
    now,
  };
}

function optionalBodyString(
  command: AdminControlCommand,
  key: string,
): string | undefined {
  const value = command.body[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function runtimeEntryString(
  entry: Record<string, unknown>,
  camelKey: string,
  snakeKey: string,
): string | undefined {
  const value = entry[camelKey] ?? entry[snakeKey];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function compactRecord<T extends Record<string, unknown>>(record: T): T {
  for (const key of Object.keys(record)) {
    if (record[key] === undefined) delete record[key];
  }
  return record;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
