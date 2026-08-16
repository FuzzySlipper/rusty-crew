import type {
  CrewAgentSessionCreationRecord,
  CrewAgentSessionCreationRequest,
  ProfileId,
  SessionId,
  SessionState,
  SessionWorkspaceUpdateRecord,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeRuntimeConfigPlan,
} from "@rusty-crew/native-bridge";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import type { RuntimeConfigFileForMutation } from "./service-profile-admin-mutations.js";
import { profileMcpBindingsFromRegistryRecord } from "./service-profile-runtime-mutations.js";
import { materializedMcpBindingId } from "./mcp-binding-identity.js";
import type {
  RustyCrewRuntimeConfig,
  RustyCrewRuntimeConfigApplyResult,
} from "./service-runtime-config.js";

const CREATION_REASON_CODES = [
  "crew_agent_session_creation_idempotency_key_required",
  "crew_agent_session_creation_requested_at_required",
  "crew_agent_session_creation_profile_not_found",
  "crew_agent_session_creation_profile_agent_missing",
  "crew_agent_session_creation_idempotency_conflict",
  "crew_agent_session_creation_session_config_missing",
  "crew_agent_session_creation_profile_inactive",
  "crew_agent_session_creation_profile_revision_conflict",
  "crew_agent_session_creation_profile_session_kind_invalid",
  "crew_agent_session_creation_active_session_conflict",
] as const;

export class CrewSessionLifecycleError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
    readonly retryable = false,
    readonly partialOutcome?: CrewSessionLifecyclePartialOutcome,
  ) {
    super(message);
    this.name = "CrewSessionLifecycleError";
  }
}

export interface CrewSessionLifecyclePartialOutcome {
  kind: "workspace_reconciled_forward" | "workspace_reconciliation_failed";
  sessionId: SessionId;
  requestedCwd: string;
  canonicalCwd?: string;
  canonicalRevision?: number;
  authoredCwd?: string;
  primaryError: string;
  compensationError?: string;
  reconciliationError?: string;
}

export interface CrewSessionLifecycleContext {
  bridge: Pick<
    NativeBridgeModule,
    | "archiveSession"
    | "createCrewAgentSession"
    | "getProfileRegistryRecord"
    | "updateSessionWorkspace"
    | "updateProfileRegistryRecord"
  >;
  runtimeConfig: RustyCrewRuntimeConfig;
  serviceConfigFile: string;
  inFlightWakes: ReadonlySet<SessionId>;
  now(): string;
  readRuntimeConfigFile(): Promise<RuntimeConfigFileForMutation>;
  validateRuntimeConfigFile(value: unknown): Promise<{
    ok: boolean;
    diagnostics: Array<{ severity: string; message: string }>;
    runtimePlan?: NativeRuntimeConfigPlan;
  }>;
  writeRuntimeConfigFile(value: unknown): Promise<void>;
  applyRuntimeConfigFromDisk(options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<RustyCrewRuntimeConfigApplyResult>;
  refreshMcpToolsForSession(input: {
    session: SessionState;
    bindingIds: readonly string[];
  }): Promise<void>;
  sessionById(sessionId: string): Promise<SessionState>;
  appendChatEvent?(
    sessionId: SessionId,
    event: Pick<ChatEvent, "kind" | "payload">,
  ): Promise<ChatEvent>;
}

export interface ArchiveCrewSessionResult {
  session: Awaited<ReturnType<NativeBridgeModule["archiveSession"]>>;
  commandEventCursor?: string;
  runtime: {
    sessionEntriesRemoved: number;
    channelBindingsDetached: number;
    mcpBindingsDetached: number;
    scheduledJobsRemoved: number;
  };
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

export interface SwitchCrewSessionWorkspaceResult {
  update: SessionWorkspaceUpdateRecord;
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

export async function archiveCrewSession(
  context: CrewSessionLifecycleContext,
  input: {
    sessionId: SessionId;
    commandName?: string;
    requestId?: string;
    actorId?: string;
  },
): Promise<ArchiveCrewSessionResult> {
  const session = await context.sessionById(input.sessionId);
  if (session.kind !== "full" && session.kind !== "delegated") {
    throw new CrewSessionLifecycleError(
      "crew_session_archive_kind_invalid",
      "Only full or delegated Crew sessions can be archived through this lifecycle.",
    );
  }
  if (session.status === "archived") {
    throw new CrewSessionLifecycleError(
      "crew_session_already_archived",
      `Session ${session.sessionId} is already archived.`,
    );
  }
  if (context.inFlightWakes.has(session.sessionId)) {
    throw new CrewSessionLifecycleError(
      "crew_session_archive_in_flight",
      `Session ${session.sessionId} has an active turn; cancel or finish it before archiving.`,
      true,
    );
  }

  const runtimeConfigFile = await context.readRuntimeConfigFile();
  const previousRuntimeConfig = structuredClone(runtimeConfigFile.value);
  const preArchivePlan = await assertValidRuntimeConfig(
    context,
    runtimeConfigFile.value,
  );
  const channelBindingIds = bindingIdsTargetingSession(
    preArchivePlan.runtimeConfig.channelBindings,
    session.sessionId,
  );
  const mcpBindingIds = bindingIdsTargetingSession(
    preArchivePlan.runtimeConfig.mcpBindings,
    session.sessionId,
  );
  const sessionEntriesRemoved = removeEntriesBySessionId(
    runtimeConfigFile.array("sessions"),
    session.sessionId,
    "sessionId",
    "session_id",
  );
  const channelBindingsDetached = removeEntriesByIds(
    runtimeConfigFile.array("channelBindings"),
    channelBindingIds,
    "bindingId",
    "binding_id",
  );
  const mcpBindingsDetached = removeEntriesByIds(
    runtimeConfigFile.array("mcpBindings"),
    mcpBindingIds,
    "bindingId",
    "binding_id",
  );
  const scheduledJobsRemoved = removeEntriesBySessionId(
    runtimeConfigFile.array("scheduledJobs"),
    session.sessionId,
    "targetSessionId",
    "target_session_id",
  );
  await assertValidRuntimeConfig(context, runtimeConfigFile.value);
  await context.writeRuntimeConfigFile(runtimeConfigFile.value);

  let commandEventCursor: string | undefined;
  let archived: Awaited<ReturnType<NativeBridgeModule["archiveSession"]>>;
  try {
    if (input.commandName === "archive" && context.appendChatEvent) {
      const event = await context.appendChatEvent(session.sessionId, {
        kind: "command_completed",
        payload: {
          status: "completed",
          command_name: "archive",
          summary: `Archived session ${session.sessionId}.`,
          request_id: input.requestId,
          actor_id: input.actorId,
          session_id: session.sessionId,
        },
      });
      commandEventCursor = event.event_id;
    }
    archived = await context.bridge.archiveSession(session.sessionId);
  } catch (error) {
    await context
      .writeRuntimeConfigFile(previousRuntimeConfig)
      .catch(() => undefined);
    await context
      .applyRuntimeConfigFromDisk({
        createMissingSessions: false,
        eventType: "crew_session_archive_rolled_back",
        summaryPrefix: `Crew session ${session.sessionId} archive rolled back`,
      })
      .catch(() => undefined);
    throw new CrewSessionLifecycleError(
      "crew_session_archive_commit_failed",
      error instanceof Error ? error.message : String(error),
      true,
    );
  }

  await markProfileSessionRefArchived(context, session);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "crew_session_archived",
    summaryPrefix: `Crew session ${session.sessionId} archived`,
  });
  return {
    session: archived,
    ...(commandEventCursor === undefined ? {} : { commandEventCursor }),
    runtime: {
      sessionEntriesRemoved,
      channelBindingsDetached,
      mcpBindingsDetached,
      scheduledJobsRemoved,
    },
    applyResult,
  };
}

export async function createFreshCrewSession(
  context: CrewSessionLifecycleContext,
  request: CrewAgentSessionCreationRequest,
): Promise<{
  creation: CrewAgentSessionCreationRecord;
  applyResult: RustyCrewRuntimeConfigApplyResult;
}> {
  let creation: CrewAgentSessionCreationRecord;
  try {
    creation = await context.bridge.createCrewAgentSession(request);
  } catch (error) {
    throw creationError(error);
  }

  const runtimeConfigFile = await context.readRuntimeConfigFile();
  const previousRuntimeConfig = structuredClone(runtimeConfigFile.value);
  const sessions = runtimeConfigFile.array("sessions");
  if (
    !sessions.some(
      (entry) => entrySessionId(entry) === creation.session.sessionId,
    )
  ) {
    sessions.push(runtimeSessionEntry(creation.session));
  }
  try {
    const profileMcpBindingIds = await restoreProfileMcpBindingsForSession(
      context,
      runtimeConfigFile.array("mcpBindings"),
      creation.session,
    );
    await assertValidRuntimeConfig(context, runtimeConfigFile.value);
    await context.writeRuntimeConfigFile(runtimeConfigFile.value);
    const applyResult = await context.applyRuntimeConfigFromDisk({
      createMissingSessions: false,
      eventType: "crew_session_created",
      summaryPrefix: `Crew session ${creation.session.sessionId} created`,
    });
    if (profileMcpBindingIds.length > 0) {
      await context.refreshMcpToolsForSession({
        session: creation.session,
        bindingIds: profileMcpBindingIds,
      });
    }
    return { creation, applyResult };
  } catch (error) {
    await context
      .writeRuntimeConfigFile(previousRuntimeConfig)
      .catch(() => undefined);
    if (creation.outcome !== "replayed") {
      await context.bridge
        .archiveSession(creation.session.sessionId)
        .catch(() => undefined);
      await markProfileSessionRefArchived(context, creation.session).catch(
        () => undefined,
      );
    }
    await context
      .applyRuntimeConfigFromDisk({
        createMissingSessions: false,
        eventType: "crew_session_creation_rolled_back",
        summaryPrefix: `Crew session ${creation.session.sessionId} creation rolled back`,
      })
      .catch(() => undefined);
    throw new CrewSessionLifecycleError(
      "crew_agent_session_creation_runtime_apply_failed",
      error instanceof Error ? error.message : String(error),
      true,
    );
  }
}

export async function switchCrewSessionWorkspace(
  context: CrewSessionLifecycleContext,
  input: {
    sessionId: SessionId;
    cwd: string;
    expectedRevision: number;
  },
): Promise<SwitchCrewSessionWorkspaceResult> {
  const runtimeConfigFile = await context.readRuntimeConfigFile();
  const previousRuntimeConfig = structuredClone(runtimeConfigFile.value);
  const runtimeSession = runtimeConfigFile
    .array("sessions")
    .find((entry) => entrySessionId(entry) === input.sessionId);
  if (!isRecord(runtimeSession)) {
    throw new CrewSessionLifecycleError(
      "session_workspace_config_missing",
      `Session ${input.sessionId} has no authored runtime configuration.`,
    );
  }

  runtimeSession.workspaceCwd = input.cwd;
  delete runtimeSession.workspace_cwd;
  await assertValidRuntimeConfig(context, runtimeConfigFile.value);

  let update: SessionWorkspaceUpdateRecord | undefined;
  try {
    update = await context.bridge.updateSessionWorkspace({
      sessionId: input.sessionId,
      cwd: input.cwd,
      expectedRevision: input.expectedRevision,
      requestedAt: context.now(),
    });
    await context.writeRuntimeConfigFile(runtimeConfigFile.value);
    const applyResult = await context.applyRuntimeConfigFromDisk({
      createMissingSessions: false,
      eventType: "crew_session_workspace_changed",
      summaryPrefix: `Crew session ${input.sessionId} workspace changed`,
    });
    return { update, applyResult };
  } catch (error) {
    if (update === undefined || update.current.cwd === update.previous.cwd) {
      throw error;
    }

    const primaryError = errorMessage(error);
    let compensationError: string | undefined;
    try {
      await context.bridge.updateSessionWorkspace({
        sessionId: input.sessionId,
        cwd: update.previous.cwd,
        expectedRevision: update.current.revision,
        requestedAt: context.now(),
      });
    } catch (compensationFailure) {
      compensationError = errorMessage(compensationFailure);
    }

    let canonicalSession: SessionState;
    try {
      canonicalSession = await context.sessionById(input.sessionId);
      if (canonicalSession.workspace == null) {
        throw new Error("canonical session has no workspace state");
      }
    } catch (reconciliationFailure) {
      throw workspaceReconciliationFailure({
        input,
        primaryError,
        ...(compensationError === undefined ? {} : { compensationError }),
        reconciliationError: errorMessage(reconciliationFailure),
      });
    }

    const canonicalWorkspace = canonicalSession.workspace;
    const reconciledRuntimeConfig = structuredClone(previousRuntimeConfig);
    const reconciledSession = runtimeConfigSessionById(
      reconciledRuntimeConfig,
      input.sessionId,
    );
    if (reconciledSession === undefined) {
      throw workspaceReconciliationFailure({
        input,
        primaryError,
        ...(compensationError === undefined ? {} : { compensationError }),
        canonicalCwd: canonicalWorkspace.cwd,
        canonicalRevision: canonicalWorkspace.revision,
        reconciliationError:
          "authored runtime session disappeared during reconciliation",
      });
    }
    reconciledSession.workspaceCwd = canonicalWorkspace.cwd;
    delete reconciledSession.workspace_cwd;

    try {
      await assertValidRuntimeConfig(context, reconciledRuntimeConfig);
      await context.writeRuntimeConfigFile(reconciledRuntimeConfig);
    } catch (reconciliationFailure) {
      throw workspaceReconciliationFailure({
        input,
        primaryError,
        ...(compensationError === undefined ? {} : { compensationError }),
        canonicalCwd: canonicalWorkspace.cwd,
        canonicalRevision: canonicalWorkspace.revision,
        reconciliationError: errorMessage(reconciliationFailure),
      });
    }

    let settlementApplyError: string | undefined;
    try {
      await context.applyRuntimeConfigFromDisk({
        createMissingSessions: false,
        eventType:
          compensationError === undefined
            ? "crew_session_workspace_change_rolled_back"
            : "crew_session_workspace_change_reconciled_forward",
        summaryPrefix:
          compensationError === undefined
            ? `Crew session ${input.sessionId} workspace change rolled back`
            : `Crew session ${input.sessionId} workspace change reconciled to canonical state`,
      });
    } catch (applyFailure) {
      settlementApplyError = errorMessage(applyFailure);
    }

    if (settlementApplyError !== undefined) {
      throw workspaceReconciliationFailure({
        input,
        primaryError,
        ...(compensationError === undefined ? {} : { compensationError }),
        canonicalCwd: canonicalWorkspace.cwd,
        canonicalRevision: canonicalWorkspace.revision,
        authoredCwd: canonicalWorkspace.cwd,
        reconciliationError: settlementApplyError,
      });
    }

    if (compensationError !== undefined) {
      throw new CrewSessionLifecycleError(
        "session_workspace_change_reconciled_forward",
        `Workspace switch failed to apply and rollback was rejected; canonical and authored state were reconciled at ${canonicalWorkspace.cwd}.`,
        false,
        {
          kind: "workspace_reconciled_forward",
          sessionId: input.sessionId,
          requestedCwd: input.cwd,
          canonicalCwd: canonicalWorkspace.cwd,
          canonicalRevision: canonicalWorkspace.revision,
          authoredCwd: canonicalWorkspace.cwd,
          primaryError,
          compensationError,
        },
      );
    }
    throw error;
  }
}

function workspaceReconciliationFailure(input: {
  input: { sessionId: SessionId; cwd: string };
  primaryError: string;
  compensationError?: string;
  canonicalCwd?: string;
  canonicalRevision?: number;
  authoredCwd?: string;
  reconciliationError: string;
}): CrewSessionLifecycleError {
  return new CrewSessionLifecycleError(
    "session_workspace_change_reconciliation_failed",
    `Workspace switch failed and its authority reconciliation did not complete: ${input.reconciliationError}`,
    true,
    {
      kind: "workspace_reconciliation_failed",
      sessionId: input.input.sessionId,
      requestedCwd: input.input.cwd,
      ...(input.canonicalCwd === undefined
        ? {}
        : { canonicalCwd: input.canonicalCwd }),
      ...(input.canonicalRevision === undefined
        ? {}
        : { canonicalRevision: input.canonicalRevision }),
      ...(input.authoredCwd === undefined
        ? {}
        : { authoredCwd: input.authoredCwd }),
      primaryError: input.primaryError,
      ...(input.compensationError === undefined
        ? {}
        : { compensationError: input.compensationError }),
      reconciliationError: input.reconciliationError,
    },
  );
}

function runtimeConfigSessionById(
  value: unknown,
  sessionId: SessionId,
): Record<string, unknown> | undefined {
  if (!isRecord(value) || !Array.isArray(value.sessions)) return undefined;
  return value.sessions.find(
    (entry): entry is Record<string, unknown> =>
      isRecord(entry) && entrySessionId(entry) === sessionId,
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function restoreProfileMcpBindingsForSession(
  context: CrewSessionLifecycleContext,
  runtimeBindings: unknown[],
  session: SessionState,
): Promise<string[]> {
  const profile = await context.bridge.getProfileRegistryRecord(
    session.profileId,
  );
  if (profile === undefined) return [];
  const configured = profileMcpBindingsFromRegistryRecord(profile);
  const materializedBindingIds: string[] = [];
  const existingTargets = new Map(
    runtimeBindings.flatMap((entry) => {
      if (!isRecord(entry)) return [];
      const bindingId = entry.bindingId ?? entry.binding_id;
      const sessionId = entry.sessionId ?? entry.session_id;
      return typeof bindingId === "string"
        ? [
            [
              bindingId,
              typeof sessionId === "string" ? sessionId : undefined,
            ] as const,
          ]
        : [];
    }),
  );
  configured.forEach((binding, index) => {
    const bindingId = materializedMcpBindingId(
      binding.bindingId ?? `${session.agentId}-mcp-${index + 1}`,
      String(session.sessionId),
    );
    materializedBindingIds.push(bindingId);
    if (existingTargets.get(bindingId) === session.sessionId) return;
    runtimeBindings.push({
      bindingId,
      adapterId: binding.adapterId ?? "mcp-ts-main",
      agentId: session.agentId,
      sessionId: session.sessionId,
      profileId: session.profileId,
      serverNames: binding.serverNames ?? [binding.serverId],
      endpointRef: `config://mcp/${binding.serverId}`,
      transport: binding.transport ?? "streamable_http",
      toolProfileKey: binding.toolProfileKey ?? session.profileId,
      status: "active",
      diagnostics: {
        desiredProfileBindingId:
          binding.bindingId ?? `${session.agentId}-mcp-${index + 1}`,
        reconciliationSource: "profile_registry",
      },
    });
    existingTargets.set(bindingId, session.sessionId);
  });
  return materializedBindingIds;
}

function runtimeSessionEntry(session: SessionState): Record<string, unknown> {
  return compact({
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    workspaceCwd: session.workspace?.cwd,
    resourceLimits: compact({
      maxDurationMs: session.resourceLimits.maxDurationMs,
      maxDelegationDepth: session.resourceLimits.maxDelegationDepth,
    }),
    maxHistoryMessages: session.historyWindow?.maxMessages,
  });
}

async function markProfileSessionRefArchived(
  context: CrewSessionLifecycleContext,
  session: SessionState,
): Promise<void> {
  const profile = await context.bridge.getProfileRegistryRecord(
    session.profileId,
  );
  if (profile === undefined) return;
  let changed = false;
  const derivedRuntimeRefs = profile.derivedRuntimeRefs.map((reference) => {
    if (
      reference.refKind !== "session" ||
      reference.refId !== session.sessionId
    ) {
      return reference;
    }
    changed = true;
    return { ...reference, status: "archived", updatedAt: context.now() };
  });
  if (!changed) return;
  await context.bridge.updateProfileRegistryRecord({
    write: {
      profileId: profile.profileId,
      lifecycleStatus: profile.lifecycleStatus,
      displayName: profile.displayName,
      summary: profile.summary,
      defaultSessionKind: profile.defaultSessionKind,
      agentId: profile.agentId,
      ownerId: profile.ownerId,
      promptSoulMarkdown: profile.promptSoulMarkdown,
      promptMemoryMarkdown: profile.promptMemoryMarkdown,
      activeRuntimeSettingsJson: profile.activeRuntimeSettingsJson ?? {},
      sourceAssetRefs: profile.sourceAssetRefs,
      derivedRuntimeRefs,
      importExport: profile.importExport,
      now: context.now(),
    },
    expectedRevision: profile.revision,
  });
}

async function assertValidRuntimeConfig(
  context: CrewSessionLifecycleContext,
  value: unknown,
): Promise<NativeRuntimeConfigPlan> {
  const validation = await context.validateRuntimeConfigFile(value);
  if (validation.ok && validation.runtimePlan !== undefined) {
    return validation.runtimePlan;
  }
  const message = validation.diagnostics.find(
    (diagnostic) => diagnostic.severity === "error",
  )?.message;
  throw new CrewSessionLifecycleError(
    "crew_session_runtime_config_invalid",
    message ??
      "Crew session runtime config mutation did not produce a canonical Rust plan.",
  );
}

function bindingIdsTargetingSession(
  bindings: ReadonlyArray<{ bindingId: string; sessionId?: string }>,
  sessionId: string,
): Set<string> {
  return new Set(
    bindings
      .filter((binding) => binding.sessionId === sessionId)
      .map((binding) => binding.bindingId),
  );
}

function removeEntriesByIds(
  entries: unknown[],
  ids: ReadonlySet<string>,
  camelKey: string,
  snakeKey: string,
): number {
  let removed = 0;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!isRecord(entry)) continue;
    const id = entry[camelKey] ?? entry[snakeKey];
    if (typeof id !== "string" || !ids.has(id)) continue;
    entries.splice(index, 1);
    removed += 1;
  }
  return removed;
}

function removeEntriesBySessionId(
  entries: unknown[],
  sessionId: string,
  camelKey: string,
  snakeKey: string,
): number {
  let removed = 0;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    if (entrySessionRef(entries[index], camelKey, snakeKey) !== sessionId)
      continue;
    entries.splice(index, 1);
    removed += 1;
  }
  return removed;
}

function entrySessionId(entry: unknown): string | undefined {
  return entrySessionRef(entry, "sessionId", "session_id");
}

function entrySessionRef(
  entry: unknown,
  camelKey: string,
  snakeKey: string,
): string | undefined {
  if (!isRecord(entry)) return undefined;
  const value = entry[camelKey] ?? entry[snakeKey];
  return typeof value === "string" ? value : undefined;
}

function creationError(error: unknown): CrewSessionLifecycleError {
  const message = error instanceof Error ? error.message : String(error);
  const reasonCode = CREATION_REASON_CODES.find((candidate) =>
    message.includes(candidate),
  );
  return new CrewSessionLifecycleError(
    reasonCode ?? "crew_agent_session_creation_internal_error",
    message,
    reasonCode === undefined,
  );
}

function compact(input: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(input).filter(([, value]) => value !== undefined),
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
