import type {
  CrewAgentSessionCreationRecord,
  CrewAgentSessionCreationRequest,
  ProfileId,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import type { RuntimeConfigFileForMutation } from "./service-profile-admin-mutations.js";
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
  ) {
    super(message);
    this.name = "CrewSessionLifecycleError";
  }
}

export interface CrewSessionLifecycleContext {
  bridge: Pick<
    NativeBridgeModule,
    | "archiveSession"
    | "createCrewAgentSession"
    | "getProfileRegistryRecord"
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
  }>;
  writeRuntimeConfigFile(value: unknown): Promise<void>;
  applyRuntimeConfigFromDisk(options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<RustyCrewRuntimeConfigApplyResult>;
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
  if (session.kind !== "full") {
    throw new CrewSessionLifecycleError(
      "crew_session_archive_kind_invalid",
      "Only full Crew brain sessions can be archived through this lifecycle.",
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
  const sessionEntriesRemoved = removeEntriesBySessionId(
    runtimeConfigFile.array("sessions"),
    session.sessionId,
    "sessionId",
    "session_id",
  );
  const channelBindingsDetached = detachSessionReferences(
    runtimeConfigFile.array("channelBindings"),
    session.sessionId,
    "sessionId",
    "session_id",
  );
  const mcpBindingsDetached = detachSessionReferences(
    runtimeConfigFile.array("mcpBindings"),
    session.sessionId,
    "sessionId",
    "session_id",
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
  if (creation.templateSessionId != null) {
    replaceSessionReferences(
      runtimeConfigFile.array("channelBindings"),
      creation.templateSessionId,
      creation.session.sessionId,
      "sessionId",
      "session_id",
    );
    replaceSessionReferences(
      runtimeConfigFile.array("mcpBindings"),
      creation.templateSessionId,
      creation.session.sessionId,
      "sessionId",
      "session_id",
    );
    replaceSessionReferences(
      runtimeConfigFile.array("scheduledJobs"),
      creation.templateSessionId,
      creation.session.sessionId,
      "targetSessionId",
      "target_session_id",
    );
  }
  try {
    await assertValidRuntimeConfig(context, runtimeConfigFile.value);
    await context.writeRuntimeConfigFile(runtimeConfigFile.value);
    const applyResult = await context.applyRuntimeConfigFromDisk({
      createMissingSessions: false,
      eventType: "crew_session_created",
      summaryPrefix: `Crew session ${creation.session.sessionId} created`,
    });
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

function runtimeSessionEntry(session: SessionState): Record<string, unknown> {
  return compact({
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    resourceLimits: compact({
      workdir: session.resourceLimits.workdir,
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
): Promise<void> {
  const validation = await context.validateRuntimeConfigFile(value);
  if (validation.ok) return;
  const message = validation.diagnostics.find(
    (diagnostic) => diagnostic.severity === "error",
  )?.message;
  throw new CrewSessionLifecycleError(
    "crew_session_runtime_config_invalid",
    message ?? "Crew session runtime config mutation was invalid.",
  );
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

function detachSessionReferences(
  entries: unknown[],
  sessionId: string,
  camelKey: string,
  snakeKey: string,
): number {
  let changed = 0;
  for (const entry of entries) {
    if (
      !isRecord(entry) ||
      entrySessionRef(entry, camelKey, snakeKey) !== sessionId
    )
      continue;
    delete entry[camelKey];
    delete entry[snakeKey];
    changed += 1;
  }
  return changed;
}

function replaceSessionReferences(
  entries: unknown[],
  oldSessionId: string,
  newSessionId: string,
  camelKey: string,
  snakeKey: string,
): void {
  for (const entry of entries) {
    if (
      !isRecord(entry) ||
      entrySessionRef(entry, camelKey, snakeKey) !== oldSessionId
    )
      continue;
    entry[camelKey] = newSessionId;
    delete entry[snakeKey];
  }
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
