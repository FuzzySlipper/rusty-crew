import { createHash } from "node:crypto";
import type {
  BrainEvent,
  BrainImplementationHandle,
  CompletionPacket,
  ContextCompactionArtifact,
  CoreEvent,
  ProfileId,
  RuntimeActivityFinish,
  RuntimeActivityWakeSettlement,
  SessionExecutionState,
  SessionId,
  SessionState,
  SubscriptionHandle,
  ToolCallMetadata,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { buildChatWakeFailureSummaryFromEvents } from "./chat-wake-failure-summary.js";
import { BufferedBrainWakeError } from "./buffered-brain-host.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import {
  buildProfileRoleAssembly,
  type BuildProfileRoleAssemblyOptions,
} from "./profile-role-assembly.js";
import type { loadProfileContext } from "./profile-loading.js";
import { effectiveToolSelectionForResourceLimits } from "./tool-profile-selection.js";
import type {
  RustyCrewConfiguredSession,
  RustyCrewRuntimeConfig,
  ServiceBrainWakeResultObservation,
} from "./service-runtime-config.js";

export interface ServiceWakeDispatchReport {
  sessionId: SessionId;
  wakeId?: string;
  status: "completed" | "continuing" | "skipped" | "failed";
  summary: string;
  reasonCode?: string;
  completionPacket?: CompletionPacket;
  observedEvents?: readonly CoreEvent[];
}

export interface ServiceWakeObservationContext {
  deliveryIntentId?: number;
  channelId?: number;
  channelMessageId?: number;
}

export type ServiceWakeSource =
  | "background"
  | "direct_debug"
  | "delivery"
  | "external_runtime"
  | "chat";

export type WakeProfileContext = Awaited<ReturnType<typeof loadProfileContext>>;

export interface WakeContextStrategyPreparation {
  additionalInstructions: string[];
  sessionMemoryContext?: string;
}

export interface WakeRuntimePauseRecord {
  pauseId: string;
  scope: string;
  targetId: string;
  reason?: string;
}

export interface ServiceWakeDispatchContext {
  bridge: Pick<
    NativeBridgeModule,
    | "buildBrainWakeRequestForSession"
    | "drainSubscriptionEvents"
    | "listSessions"
    | "planRoleplayMechanicProfile"
    | "beginRuntimeActivity"
    | "finishRuntimeActivity"
    | "settleRuntimeActivityWake"
    | "readChatSession"
    | "saveContextCompactionArtifact"
    | "subscribeEvents"
    | "unsubscribeEvents"
    | "wakeBrain"
  >;
  inFlightWakes: Set<SessionId>;
  deferredWakeSessions: Set<SessionId>;
  toolCallDebugStore: ToolCallDebugStore;
  brainForProfile(profileId: ProfileId): BrainImplementationHandle | undefined;
  configuredSessionForRuntimeSession(
    session: Pick<SessionState, "sessionId" | "profileId">,
  ): RustyCrewRuntimeConfig["sessions"][number] | undefined;
  loadProfileContext(profileId: ProfileId): Promise<WakeProfileContext>;
  nextWakeId(session: SessionState): string;
  prepareContextStrategy(input: {
    session: SessionState;
    configuredSession?: Pick<
      RustyCrewConfiguredSession,
      "sessionMemoryPrompt" | "contextPolicy"
    >;
    profileContext: WakeProfileContext;
  }): Promise<WakeContextStrategyPreparation>;
  roleplayPromptContextForSession(
    session: SessionState,
  ): Promise<string | undefined>;
  appendChatEvent(
    sessionId: SessionId,
    event: Pick<ChatEvent, "kind" | "payload">,
  ): Promise<ChatEvent>;
  listChatEventsAfterCursor(
    session: SessionState,
    cursor: string | undefined,
    limit: number,
  ): Promise<readonly ChatEvent[]>;
  publishWakeToolActivity(input: {
    session: SessionState;
    wakeId: string;
    events: readonly CoreEvent[];
    observationContext?: ServiceWakeObservationContext;
  }): Promise<void>;
  runPostTurnMaintenance(input: {
    session: SessionState;
    profileContext: WakeProfileContext;
    wakeId: string;
    source: ServiceWakeSource;
    observedEvents: readonly CoreEvent[];
    completionSummary?: string;
  }): Promise<void>;
  persistSessionActivityDigest(input: {
    session: SessionState;
    wakeId: string;
    source: ServiceWakeSource;
    observedEvents: readonly CoreEvent[];
    completionSummary?: string;
  }): Promise<void>;
  runtimePauseForSession(
    session: Pick<SessionState, "sessionId" | "agentId" | "profileId">,
  ): WakeRuntimePauseRecord | undefined;
  deferRuntimeActivitySettlement(input: {
    wake: RuntimeActivityWakeSettlement;
    dispatch: RuntimeActivityFinish;
  }): void;
  recordEvent(event: {
    source: string;
    eventType: string;
    summary: string;
    severity?: string;
    workRef?: Record<string, unknown>;
    resultRef?: Record<string, unknown>;
  }): void;
  now: () => string;
}

export async function dispatchWake(
  context: ServiceWakeDispatchContext,
  event: Extract<CoreEvent, { type: "brain_wake_requested" }>,
  source: ServiceWakeSource,
  observationContext?: ServiceWakeObservationContext,
  options: { appendChatEvents?: boolean } = {},
): Promise<ServiceWakeDispatchReport> {
  const sessionId = event.sessionId;
  const appendChatEvents = options.appendChatEvents !== false;
  let activeWake:
    | {
        session: SessionState;
        wakeId: string;
      }
    | undefined;
  let dispatchActivityStarted = false;
  let dispatchFinish: RuntimeActivityFinish | undefined;
  let lastObservedExecution: SessionExecutionState | undefined;
  if (context.inFlightWakes.has(sessionId)) {
    context.deferredWakeSessions.add(sessionId);
    return {
      sessionId,
      status: "skipped",
      summary: `wake for ${sessionId} skipped because one is already in flight`,
      reasonCode: "wake_already_in_flight",
    };
  }

  context.inFlightWakes.add(sessionId);
  try {
    const session = (await context.bridge.listSessions()).find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return wakeDispatchSkipped(
        context,
        sessionId,
        "wake_session_missing",
        `wake for ${sessionId} skipped because the session is missing`,
      );
    }
    if (session.status === "archived") {
      return wakeDispatchSkipped(
        context,
        sessionId,
        "wake_session_archived",
        `wake for ${sessionId} skipped because the session is archived`,
      );
    }
    const pause = context.runtimePauseForSession(session);
    if (pause !== undefined) {
      return runtimePauseWakeReport(context, sessionId, pause);
    }

    const brain = context.brainForProfile(session.profileId);
    if (brain === undefined) {
      return wakeDispatchSkipped(
        context,
        sessionId,
        "wake_brain_missing",
        `wake for ${sessionId} skipped because profile ${session.profileId} has no registered brain`,
      );
    }

    const wakeId = context.nextWakeId(session);
    activeWake = { session, wakeId };
    await context.bridge
      .beginRuntimeActivity({
        activityId: `dispatch:${wakeId}`,
        kind: "dispatch",
        owner: "type_script_host",
        agentId: session.agentId,
        profileId: session.profileId,
        sessionId: session.sessionId,
        wakeId,
        phase: "preparing",
        summary: `${source} wake dispatch`,
      })
      .then(() => {
        dispatchActivityStarted = true;
      })
      .catch((error: unknown) => {
        context.recordEvent({
          source: "service-host",
          eventType: "runtime_activity_record_failed",
          severity: "warning",
          summary: errorMessage(error, "wake dispatch activity begin failed"),
        });
      });
    const profileContext = await context.loadProfileContext(session.profileId);
    const configured = context.configuredSessionForRuntimeSession(session);
    const contextStrategy = await context.prepareContextStrategy({
      session,
      configuredSession: configured,
      profileContext,
    });
    const roleplayContext =
      await context.roleplayPromptContextForSession(session);
    const mechanicPlan =
      profileContext.profile.roleplayMechanic === undefined
        ? undefined
        : ((await context.bridge.planRoleplayMechanicProfile({
            name:
              profileContext.profile.displayName ??
              profileContext.profile.profileId,
            providerAlias: profileContext.profile.providerAlias,
            autoMonitor: profileContext.profile.roleplayMechanic.autoMonitor,
          })) as {
            systemPrompt: string;
            localToolProfileId: string;
          });
    if (
      mechanicPlan !== undefined &&
      profileContext.profile.localToolProfileId !==
        mechanicPlan.localToolProfileId
    ) {
      throw new Error(
        `roleplay mechanic profile ${profileContext.profile.profileId} must select local tool profile ${mechanicPlan.localToolProfileId}`,
      );
    }
    const roleInput: BuildProfileRoleAssemblyOptions = {
      ...(mechanicPlan === undefined
        ? {}
        : { systemPromptOverride: mechanicPlan.systemPrompt }),
      sessionMemoryContext: contextStrategy.sessionMemoryContext,
      additionalInstructions: [
        ...contextStrategy.additionalInstructions,
        ...(roleplayContext === undefined ? [] : [roleplayContext]),
      ],
    };
    const effectiveProfileContext = {
      ...profileContext,
      toolSelection: effectiveToolSelectionForResourceLimits(
        profileContext.toolSelection,
        session.resourceLimits,
      ),
    };
    const role = buildProfileRoleAssembly(effectiveProfileContext, roleInput);
    const observed = await observeWakeEvents(
      context,
      sessionId,
      async () => {
        const request = await context.bridge.buildBrainWakeRequestForSession({
          brain,
          sessionId,
          systemPrompt: role.systemPrompt,
          roleAssemblyJson: new TextEncoder().encode(
            JSON.stringify(role.roleAssembly),
          ),
          wakeId,
        });
        return context.bridge.wakeBrain(request);
      },
      appendChatEvents
        ? (events) =>
            appendCoreEventsToChatLog(context, session, wakeId, events)
        : undefined,
    );
    await context.publishWakeToolActivity({
      session,
      wakeId,
      events: observed.events,
      observationContext,
    });
    for (let index = observed.events.length - 1; index >= 0; index -= 1) {
      const observedEvent = observed.events[index];
      if (observedEvent?.type === "session_execution_observed") {
        lastObservedExecution = observedEvent.execution;
        break;
      }
    }
    const accepted = observed.accepted;
    const completionPacket = wakeCompletionPacket(observed.events);
    const completionSummary = wakeCompletionSummary(observed.events);
    const continuing = accepted.outcome === "continuing";
    const report: ServiceWakeDispatchReport = {
      sessionId,
      wakeId,
      status: accepted.accepted
        ? continuing
          ? "continuing"
          : "completed"
        : "failed",
      summary:
        completionSummary ??
        (accepted.accepted
          ? continuing
            ? `wake ${wakeId} yielded and will continue for ${session.agentId}`
            : `wake ${wakeId} completed for ${session.agentId}`
          : `wake ${wakeId} was rejected for ${session.agentId}`),
      reasonCode: accepted.accepted ? undefined : "wake_rejected",
      completionPacket,
      observedEvents: observed.events,
    };
    if (appendChatEvents && report.status === "completed") {
      await ensureChatWakeTerminalEvents(
        context,
        session,
        wakeId,
        observed.events,
        {
          summary: completionSummary ?? report.summary,
        },
      );
    }
    context.recordEvent({
      source: "service-host",
      eventType: "brain_wake_dispatched",
      severity: accepted.accepted ? undefined : "error",
      summary: `${report.summary} (${source}).`,
    });
    if (report.status === "completed") {
      await context.runPostTurnMaintenance({
        session,
        profileContext,
        wakeId,
        source,
        observedEvents: observed.events,
        completionSummary: report.summary,
      });
      await context.persistSessionActivityDigest({
        session,
        wakeId,
        source,
        observedEvents: observed.events,
        completionSummary: report.summary,
      });
    }
    dispatchFinish = runtimeDispatchFinish(report);
    return report;
  } catch (error) {
    if (error instanceof BufferedBrainWakeError) {
      const report: ServiceWakeDispatchReport = {
        sessionId,
        wakeId: activeWake?.wakeId,
        status: "failed",
        summary: await buildChatWakeFailureSummary(
          context,
          activeWake?.session,
          activeWake?.wakeId,
          error.message,
        ),
        reasonCode: error.reasonCode,
      };
      if (appendChatEvents && activeWake !== undefined) {
        await ensureChatWakeTerminalEventsFromChatLog(
          context,
          activeWake.session,
          activeWake.wakeId,
          {
            status: "failed",
            summary: report.summary,
            reasonCode: error.reasonCode,
            source: error.source,
            allowWithoutAssistantTurn: true,
          },
        );
      }
      context.recordEvent({
        source: "service-host",
        eventType: "brain_wake_failed",
        severity: "error",
        summary: report.summary,
      });
      dispatchFinish = runtimeDispatchFinish(report);
      return report;
    }
    const failure = classifyWakeDispatchFailure(error, sessionId);
    const report: ServiceWakeDispatchReport = {
      sessionId,
      wakeId: activeWake?.wakeId,
      status: "failed",
      summary: await buildChatWakeFailureSummary(
        context,
        activeWake?.session,
        activeWake?.wakeId,
        failure.message,
      ),
      reasonCode: failure.reasonCode,
    };
    if (appendChatEvents && activeWake !== undefined) {
      await ensureChatWakeTerminalEventsFromChatLog(
        context,
        activeWake.session,
        activeWake.wakeId,
        {
          status: "failed",
          summary: report.summary,
          reasonCode: report.reasonCode,
          source: failure.reasonCode,
          allowWithoutAssistantTurn: true,
        },
      );
    }
    context.recordEvent({
      source: "service-host",
      eventType: "brain_wake_failed",
      severity: "error",
      summary: report.summary,
    });
    dispatchFinish = runtimeDispatchFinish(report);
    return report;
  } finally {
    if (dispatchActivityStarted && dispatchFinish !== undefined) {
      const wakeSettlement = runtimeWakeSettlement(dispatchFinish);
      let settlementDeferred = false;
      try {
        await context.bridge.settleRuntimeActivityWake(wakeSettlement);
      } catch (error: unknown) {
        settlementDeferred = true;
        context.recordEvent({
          source: "service-host",
          eventType: "runtime_activity_wake_settlement_deferred",
          severity: "warning",
          summary: errorMessage(
            error,
            "wake runtime activity settlement failed",
          ),
        });
      }
      try {
        await context.bridge.finishRuntimeActivity(dispatchFinish);
      } catch (error: unknown) {
        settlementDeferred = true;
        context.recordEvent({
          source: "service-host",
          eventType: "runtime_activity_record_failed",
          severity: "warning",
          summary: errorMessage(error, "wake dispatch activity finish failed"),
        });
      }
      if (settlementDeferred) {
        context.deferRuntimeActivitySettlement({
          wake: wakeSettlement,
          dispatch: dispatchFinish,
        });
      }
      if (appendChatEvents && activeWake !== undefined) {
        try {
          const readback = await context.bridge.readChatSession({
            session_id: activeWake.session.sessionId,
            cursor: null,
            limit: 1,
            include_alternates: false,
          });
          if (
            lastObservedExecution === undefined ||
            executionTransitionKey(lastObservedExecution) !==
              executionTransitionKey(readback.execution)
          ) {
            await context.appendChatEvent(activeWake.session.sessionId, {
              kind: "session_execution_changed",
              payload: { execution: readback.execution },
            });
          }
        } catch (error: unknown) {
          context.recordEvent({
            source: "service-host",
            eventType: "session_execution_projection_failed",
            severity: "warning",
            summary: errorMessage(
              error,
              "wake terminal execution projection failed",
            ),
          });
        }
      }
    }
    context.inFlightWakes.delete(sessionId);
    if (context.deferredWakeSessions.delete(sessionId)) {
      queueMicrotask(() => {
        void dispatchWake(
          context,
          { type: "brain_wake_requested", sessionId },
          source,
          observationContext,
          options,
        ).catch((error: unknown) => {
          context.recordEvent({
            source: "service-host",
            eventType: "deferred_wake_dispatch_failed",
            severity: "error",
            summary: errorMessage(
              error,
              `deferred wake for ${sessionId} failed`,
            ),
          });
        });
      });
    }
  }
}

function executionTransitionKey(execution: SessionExecutionState): string {
  return JSON.stringify([
    execution.sessionId,
    execution.lifecycleStatus,
    execution.phase,
    execution.source,
    execution.wakeId ?? null,
    execution.logicalTurnId ?? null,
    execution.lastOutcome ?? null,
    execution.reasonCode ?? null,
  ]);
}

function runtimeDispatchFinish(
  report: ServiceWakeDispatchReport,
): RuntimeActivityFinish | undefined {
  if (report.wakeId === undefined) {
    return undefined;
  }
  return {
    activityId: `dispatch:${report.wakeId}`,
    status:
      report.status === "completed" || report.status === "continuing"
        ? "completed"
        : "failed",
    phase: report.status,
    reasonCode: report.reasonCode,
    summary: `wake dispatch ${report.status}`,
  };
}

function runtimeWakeSettlement(
  dispatch: RuntimeActivityFinish,
): RuntimeActivityWakeSettlement {
  return {
    wakeId: dispatch.activityId.replace(/^dispatch:/, ""),
    status: dispatch.status,
    reasonCode: dispatch.reasonCode,
    summary: dispatch.summary ?? `wake activity ${dispatch.status}`,
  };
}

export function classifyWakeDispatchFailure(
  error: unknown,
  sessionId: SessionId,
): { message: string; reasonCode: string } {
  const message = errorMessage(error, `wake for ${sessionId} failed`);
  const explicitReasonCode =
    typeof error === "object" &&
    error !== null &&
    typeof (error as { reasonCode?: unknown }).reasonCode === "string" &&
    (error as { reasonCode: string }).reasonCode.length > 0
      ? (error as { reasonCode: string }).reasonCode
      : undefined;
  const classified = message.match(/\[(postgres_[a-z_]+)\]/)?.[1];
  return {
    message,
    reasonCode: explicitReasonCode ?? classified ?? "wake_dispatch_failed",
  };
}

export async function appendCoreEventsToChatLog(
  context: ServiceWakeDispatchContext,
  session: SessionState,
  wakeId: string,
  events: readonly CoreEvent[],
): Promise<void> {
  for (const event of events) {
    if (
      event.type === "brain_event_observed" &&
      event.sessionId === session.sessionId
    ) {
      await appendBrainEventToChatLog(
        context,
        session,
        event.wakeId ?? undefined,
        event.event,
      );
    } else if (
      event.type === "session_execution_observed" &&
      event.execution.sessionId === session.sessionId
    ) {
      await context.appendChatEvent(session.sessionId, {
        kind: "session_execution_changed",
        payload: { execution: event.execution },
      });
    } else if (
      event.type === "logical_turn_lifecycle_observed" &&
      event.lifecycle.sessionId === session.sessionId
    ) {
      await context.appendChatEvent(session.sessionId, {
        kind: logicalTurnChatEventKind(event.lifecycle.kind),
        payload: {
          logical_turn_id: event.lifecycle.logicalTurnId,
          projection_id: event.lifecycle.projectionId,
          continuation_id: event.lifecycle.continuationId,
          continuation_count: event.lifecycle.continuationCount,
          execution_epoch_id: event.lifecycle.executionEpochId,
          wake_id: event.lifecycle.wakeId,
          phase: event.lifecycle.phase,
          operator_state: event.lifecycle.operatorState,
          progress_classification: event.lifecycle.progressClassification,
          reason_code: event.lifecycle.reasonCode,
          summary: event.lifecycle.summary,
          progress: event.lifecycle.progress,
          logical_turn_revision: event.lifecycle.logicalTurnRevision,
        },
      });
      if (event.lifecycle.kind === "continuation_yielded") {
        await context.appendChatEvent(session.sessionId, {
          kind: "logical_turn_queued_to_continue",
          payload: {
            logical_turn_id: event.lifecycle.logicalTurnId,
            projection_id: `${event.lifecycle.projectionId}:queued`,
            continuation_id: event.lifecycle.continuationId,
            continuation_count: event.lifecycle.continuationCount,
            execution_epoch_id: event.lifecycle.executionEpochId,
            wake_id: event.lifecycle.wakeId,
            phase: event.lifecycle.phase,
            operator_state: event.lifecycle.operatorState,
            progress_classification: event.lifecycle.progressClassification,
            reason_code: "continuation_queued",
            summary: "logical turn is queued to continue",
            progress: event.lifecycle.progress,
            logical_turn_revision: event.lifecycle.logicalTurnRevision,
          },
        });
      }
    } else if (
      event.type === "completion_packet_delivered" &&
      event.packet.sessionId === session.sessionId
    ) {
      await context.appendChatEvent(session.sessionId, {
        kind: "assistant_message_completed",
        payload: {
          status: event.packet.status,
          summary: event.packet.summary,
          wake_id: wakeId,
        },
      });
      // A `finished` brain event is observed before the completion packet in
      // the normal wake lifecycle. Keep `assistant_turn_finished` terminal in
      // the chat log: live transcript consumers can stop reading when it
      // arrives, so appending it first strands the message in `streaming`
      // until refresh even though the later completion event is persisted.
      await context.appendChatEvent(session.sessionId, {
        kind: "assistant_turn_finished",
        payload: { wake_id: wakeId },
      });
    } else if (
      event.type === "brain_actions_accepted" &&
      event.sessionId === session.sessionId
    ) {
      await context.appendChatEvent(session.sessionId, {
        kind: "unknown",
        payload: {
          source_event_type: event.type,
          accepted_action_count: event.count,
        },
      });
    }
  }
}

async function appendBrainEventToChatLog(
  context: ServiceWakeDispatchContext,
  session: SessionState,
  wakeId: string | undefined,
  event: BrainEvent,
): Promise<void> {
  switch (event.type) {
    case "started":
      await context.appendChatEvent(session.sessionId, {
        kind: "assistant_turn_started",
        payload: { wake_id: wakeId },
      });
      return;
    case "text_delta":
      await context.appendChatEvent(session.sessionId, {
        kind: "assistant_text_delta",
        payload: { wake_id: wakeId, text: event.text },
      });
      return;
    case "reasoning_delta":
      await context.appendChatEvent(session.sessionId, {
        kind: "assistant_reasoning_delta",
        payload: {
          wake_id: wakeId,
          text: event.text,
          visibility: "reasoning",
          ...(event.format === undefined ? {} : { format: event.format }),
        },
      });
      return;
    case "phase_change":
      await context.appendChatEvent(session.sessionId, {
        kind: "phase_change",
        payload: {
          wake_id: wakeId,
          phase: event.phase,
          ...(event.message === undefined ? {} : { message: event.message }),
        },
      });
      return;
    case "provider_status":
      const compactionKind = contextCompactionEventKind(event.metadataJson);
      await context.appendChatEvent(session.sessionId, {
        kind: compactionKind ?? "provider_status",
        payload: {
          wake_id: wakeId,
          level: event.level,
          message: event.message,
          ...(event.metadataJson === undefined
            ? {}
            : { metadata_json: event.metadataJson }),
        },
      });
      if (
        compactionKind === "context_compaction_completed" ||
        compactionKind === "context_compaction_failed"
      ) {
        const artifact = contextCompactionArtifactFromMetadata(
          session.sessionId,
          wakeId,
          event.metadataJson,
          context.now(),
        );
        if (artifact !== undefined) {
          await persistContextCompactionArtifact(
            context,
            session,
            wakeId,
            artifact,
          );
        }
      }
      return;
    case "tool_call_started":
      await context.appendChatEvent(session.sessionId, {
        kind: "tool_call_started",
        payload: {
          wake_id: wakeId,
          tool_call_id: chatToolCallId(
            wakeId,
            event.toolName,
            event.metadata ?? undefined,
          ),
          tool_name: event.toolName,
          debug_detail_id: event.metadata?.debugDetailId,
          metadata: event.metadata,
        },
      });
      return;
    case "tool_call_finished":
      await context.appendChatEvent(session.sessionId, {
        kind: event.isError ? "tool_call_failed" : "tool_call_completed",
        payload: {
          wake_id: wakeId,
          tool_call_id: chatToolCallId(
            wakeId,
            event.toolName,
            event.metadata ?? undefined,
          ),
          tool_name: event.toolName,
          is_error: event.isError,
          debug_detail_id: event.metadata?.debugDetailId,
          metadata: event.metadata,
        },
      });
      return;
    case "finished":
      // Deferred until `completion_packet_delivered`; see
      // appendCoreEventsToChatLog. The post-observation terminal fallback
      // supplies both events in the same order when a completion packet is
      // absent (timeout/provider failure).
      return;
  }
}

function contextCompactionEventKind(
  metadataJson: string | null | undefined,
):
  | "context_compaction_started"
  | "context_compaction_completed"
  | "context_compaction_failed"
  | undefined {
  if (!metadataJson) return undefined;
  try {
    const parsed = JSON.parse(metadataJson) as { kind?: unknown };
    switch (parsed.kind) {
      case "context_compaction_started":
      case "context_compaction_completed":
      case "context_compaction_failed":
        return parsed.kind;
      default:
        return undefined;
    }
  } catch {
    return undefined;
  }
}

type CompactionMetadata = {
  kind?: unknown;
  artifact?: unknown;
};

type CompactionRuntimeArtifact = {
  artifactId?: unknown;
  sequence?: unknown;
  strategyId?: unknown;
  strategyRevision?: unknown;
  logicalTurnId?: unknown;
  executionEpochId?: unknown;
  sourceProjectionFingerprint?: unknown;
  trigger?: unknown;
  beforeTokens?: unknown;
  afterTokens?: unknown;
  preservedItemCount?: unknown;
  excisedItemCount?: unknown;
  intentKey?: unknown;
  terminalStatus?: unknown;
  reasonCode?: unknown;
  usageBefore?: unknown;
  estimatedTokensAfter?: unknown;
  compactedItemCount?: unknown;
  retainedItemCount?: unknown;
  summaryText?: unknown;
  providerChainAction?: unknown;
};

function contextCompactionArtifactFromMetadata(
  sessionId: SessionId,
  wakeId: string | undefined,
  metadataJson: string | null | undefined,
  now: string,
): ContextCompactionArtifact | undefined {
  if (!metadataJson) return undefined;
  let metadata: CompactionMetadata;
  try {
    const parsed: unknown = JSON.parse(metadataJson);
    if (!isRecord(parsed)) return undefined;
    metadata = parsed;
  } catch {
    return undefined;
  }
  const artifactIsRecord = isRecord(metadata.artifact);
  const kindRaw = isRecord(metadata)
    ? (metadata as { kind?: unknown }).kind
    : undefined;
  const kindStr = typeof kindRaw === "string" ? kindRaw : undefined;
  // Failed brain events currently emit artifact:null (see chat-completions manual_intent_failed and
  // openai-responses compaction paths). For durability we must still persist a failed terminal
  // artifact that preserves the prior valid projection, even when the runtime did not supply a full artifact.
  if (!artifactIsRecord) {
    if (kindStr !== "context_compaction_failed") return undefined;
    // Synthesize a minimal failed artifact so restart/readback can see a durable failed terminal
    // record and keep the prior completed projection as latest_valid.
    const usage = (metadata as { usage?: unknown }).usage;
    const usageRecord = isRecord(usage)
      ? (usage as Record<string, unknown>)
      : {};
    const promptTokens =
      typeof usageRecord.prompt_tokens === "number"
        ? usageRecord.prompt_tokens
        : undefined;
    const completionTokens =
      typeof usageRecord.completion_tokens === "number"
        ? usageRecord.completion_tokens
        : undefined;
    const totalTokens =
      typeof usageRecord.total_tokens === "number"
        ? usageRecord.total_tokens
        : undefined;
    const beforeTokens = promptTokens ?? totalTokens ?? 0;
    const intentKeyRaw =
      (metadata as { intentKey?: unknown; intent_key?: unknown }).intentKey ??
      (metadata as { intentKey?: unknown; intent_key?: unknown }).intent_key;
    const intentKey =
      typeof intentKeyRaw === "string" && intentKeyRaw.trim().length > 0
        ? intentKeyRaw.trim()
        : (wakeId ?? "manual");
    const sourceFingerprintRaw =
      (
        metadata as {
          sourceProjectionFingerprint?: unknown;
          source_projection_fingerprint?: unknown;
        }
      ).sourceProjectionFingerprint ??
      (
        metadata as {
          sourceProjectionFingerprint?: unknown;
          source_projection_fingerprint?: unknown;
        }
      ).source_projection_fingerprint;
    const sourceFingerprint =
      typeof sourceFingerprintRaw === "string" &&
      sourceFingerprintRaw.trim().length > 0
        ? sourceFingerprintRaw.trim()
        : `manual-${intentKey}`;
    const strategyIdRaw =
      (metadata as { strategyId?: unknown; strategy_id?: unknown })
        .strategyId ??
      (metadata as { strategyId?: unknown; strategy_id?: unknown }).strategy_id;
    const strategyId =
      typeof strategyIdRaw === "string" && strategyIdRaw.trim().length > 0
        ? strategyIdRaw.trim()
        : "rolling_summary_compaction";
    const strategyRevisionRaw =
      (metadata as { strategyRevision?: unknown; strategy_revision?: unknown })
        .strategyRevision ??
      (metadata as { strategyRevision?: unknown; strategy_revision?: unknown })
        .strategy_revision;
    const strategyRevision =
      typeof strategyRevisionRaw === "string" &&
      strategyRevisionRaw.trim().length > 0
        ? strategyRevisionRaw.trim()
        : "1";
    const reasonCodeRaw =
      (metadata as { reasonCode?: unknown; reason_code?: unknown })
        .reasonCode ??
      (metadata as { reasonCode?: unknown; reason_code?: unknown }).reason_code;
    const reasonCode =
      typeof reasonCodeRaw === "string" && reasonCodeRaw.trim().length > 0
        ? reasonCodeRaw.trim()
        : "manual_intent_failed";
    const digest = createHash("sha256")
      .update([sessionId, wakeId ?? "unknown_wake", intentKey, now].join(":"))
      .digest("hex")
      .slice(0, 32);
    return {
      artifact_id: `context_compaction_${digest}`,
      session_id: sessionId,
      branch_id: undefined,
      strategy_id: strategyId,
      strategy_revision: strategyRevision,
      logical_turn_id: undefined,
      execution_epoch_id: undefined,
      source_projection_fingerprint: sourceFingerprint,
      trigger: "manual_intent",
      before_tokens: beforeTokens,
      after_tokens: beforeTokens,
      preserved_item_count: 0,
      excised_item_count: 0,
      intent_key: intentKey,
      terminal_status: "failed",
      provider_chain_action: "preserve_prior_valid_projection",
      source_refs_json: {
        source: "native_brain_stream_failed",
        wake_id: wakeId,
        reason_code: reasonCode,
        synthetic_failed_artifact: true,
      },
      provider_metadata_json: {
        provider_chain_action: "preserve_prior_valid_projection",
        source_event_kind: "context_compaction_failed",
        reason_code: reasonCode,
      },
      estimate_before_json: isRecord(usage)
        ? usage
        : {
            prompt_tokens: promptTokens,
            completion_tokens: completionTokens,
            total_tokens: totalTokens,
          },
      estimate_after_json: isRecord(usage)
        ? usage
        : {
            prompt_tokens: promptTokens,
            completion_tokens: completionTokens,
            total_tokens: totalTokens,
          },
      summary_text: `manual compaction ${intentKey} failed (${reasonCode}) – prior projection preserved`,
      enters_future_context: false,
      context_policy: strategyId,
      metadata_json: {
        schema_version: 1,
        wake_id: wakeId,
        reason_code: reasonCode,
        synthetic_failed_artifact: true,
      },
      created_at: now,
      updated_at: now,
    };
  }
  const runtimeArtifact = metadata.artifact as CompactionRuntimeArtifact;
  const sequence = positiveInteger(runtimeArtifact.sequence);
  const strategyId = nonEmptyString(runtimeArtifact.strategyId);
  const reasonCode = nonEmptyString(runtimeArtifact.reasonCode);
  const usageBefore = runtimeArtifact.usageBefore;
  const estimatedTokensAfter = nonNegativeInteger(
    runtimeArtifact.estimatedTokensAfter,
  );
  const compactedItemCount = positiveInteger(
    runtimeArtifact.compactedItemCount,
  );
  const retainedItemCount = positiveInteger(runtimeArtifact.retainedItemCount);
  const summaryText = nonEmptyString(runtimeArtifact.summaryText);
  if (
    sequence === undefined ||
    strategyId === undefined ||
    reasonCode === undefined ||
    !isRecord(usageBefore) ||
    estimatedTokensAfter === undefined ||
    compactedItemCount === undefined ||
    retainedItemCount === undefined ||
    summaryText === undefined
  ) {
    return undefined;
  }

  const providerChainAction =
    runtimeArtifact.providerChainAction === null
      ? null
      : nonEmptyString(runtimeArtifact.providerChainAction);
  const artifactId = nonEmptyString(runtimeArtifact.artifactId);
  const strategyRevision = nonEmptyString(runtimeArtifact.strategyRevision);
  const logicalTurnId = nonEmptyString(runtimeArtifact.logicalTurnId);
  const executionEpochId = nonEmptyString(runtimeArtifact.executionEpochId);
  const sourceProjectionFingerprint = nonEmptyString(
    runtimeArtifact.sourceProjectionFingerprint,
  );
  const trigger = nonEmptyString(runtimeArtifact.trigger);
  const beforeTokens = nonNegativeInteger(runtimeArtifact.beforeTokens);
  const afterTokens = nonNegativeInteger(runtimeArtifact.afterTokens);
  const preservedItemCount = nonNegativeInteger(
    runtimeArtifact.preservedItemCount,
  );
  const excisedItemCount = nonNegativeInteger(runtimeArtifact.excisedItemCount);
  // R6613-8: BrainContextCompactionArtifact has no intent_key field; Rust now emits authoritative
  // intentKey at top-level metadata (see context_compaction_status/responses_context_compaction_event).
  // Prefer top-level, fall back to artifact field for backward compat.
  const topIntentRaw =
    (metadata as Record<string, unknown>).intentKey ??
    (metadata as Record<string, unknown>).intent_key;
  const topIntent = nonEmptyString(topIntentRaw);
  const artifactIntent = nonEmptyString(
    (runtimeArtifact as unknown as Record<string, unknown>)
      .intentKey as unknown,
  );
  const intentKey = topIntent ?? artifactIntent;
  const terminalStatus = nonEmptyString(runtimeArtifact.terminalStatus);
  const identity = [sessionId, wakeId ?? "unknown_wake", sequence].join(":");
  const digest = createHash("sha256")
    .update(identity)
    .digest("hex")
    .slice(0, 32);
  return {
    artifact_id: artifactId ?? `context_compaction_${digest}`,
    session_id: sessionId,
    branch_id: undefined,
    strategy_id: strategyId,
    strategy_revision: strategyRevision,
    logical_turn_id: logicalTurnId,
    execution_epoch_id: executionEpochId,
    source_projection_fingerprint: sourceProjectionFingerprint,
    trigger: trigger,
    before_tokens: beforeTokens,
    after_tokens: afterTokens,
    preserved_item_count: preservedItemCount,
    excised_item_count: excisedItemCount,
    intent_key: intentKey,
    terminal_status: terminalStatus,
    provider_chain_action: providerChainAction,
    source_refs_json: {
      source: "native_brain_stream",
      wake_id: wakeId,
      sequence,
      reason_code: reasonCode,
      compacted_item_count: compactedItemCount,
      retained_item_count: retainedItemCount,
    },
    provider_metadata_json: {
      provider_chain_action: providerChainAction,
      source_event_kind:
        terminalStatus === "failed"
          ? "context_compaction_failed"
          : "context_compaction_completed",
    },
    estimate_before_json: usageBefore,
    estimate_after_json: {
      tokens: estimatedTokensAfter,
      source: "serialized_compaction_projection",
    },
    summary_text: summaryText,
    enters_future_context: true,
    context_policy: strategyId,
    metadata_json: {
      schema_version: 1,
      runtime_artifact_sequence: sequence,
      wake_id: wakeId,
      reason_code: reasonCode,
    },
    created_at: now,
    updated_at: now,
  };
}

async function persistContextCompactionArtifact(
  context: ServiceWakeDispatchContext,
  session: SessionState,
  wakeId: string | undefined,
  artifact: ContextCompactionArtifact,
): Promise<void> {
  try {
    await context.bridge.saveContextCompactionArtifact(artifact);
  } catch (error) {
    context.recordEvent({
      source: "service-host",
      eventType: "context_compaction_artifact_persist_failed",
      severity: "warning",
      summary: errorMessage(
        error,
        `context compaction artifact persistence failed for ${session.sessionId}`,
      ),
      workRef: {
        session_id: session.sessionId,
        wake_id: wakeId,
        artifact_id: artifact.artifact_id,
      },
    });
  }
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value
    : undefined;
}

function nonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function positiveInteger(value: unknown): number | undefined {
  const integer = nonNegativeInteger(value);
  return integer === undefined || integer === 0 ? undefined : integer;
}

function chatToolCallId(
  wakeId: string | undefined,
  toolName: string,
  metadata: ToolCallMetadata | undefined,
): string {
  if (metadata?.debugDetailId) return metadata.debugDetailId;
  return [
    wakeId ?? "wake",
    metadata?.source ?? "tool",
    metadata?.bindingId ?? "local",
    metadata?.sourceToolName ?? toolName,
  ]
    .map((part) => part.replace(/[^A-Za-z0-9_.:-]+/g, "_"))
    .join(":");
}

async function ensureChatWakeTerminalEvents(
  context: ServiceWakeDispatchContext,
  session: SessionState,
  wakeId: string,
  events: readonly CoreEvent[],
  fallback: { summary?: string },
): Promise<void> {
  const wakeEvents = events.filter(
    (event) =>
      (event.type === "brain_event_observed" &&
        event.sessionId === session.sessionId &&
        (event.wakeId === undefined || event.wakeId === wakeId)) ||
      (event.type === "completion_packet_delivered" &&
        event.packet.sessionId === session.sessionId),
  );
  const hasAssistantTurn = wakeEvents.some(
    (event) =>
      event.type === "brain_event_observed" &&
      (event.event.type === "started" ||
        event.event.type === "text_delta" ||
        event.event.type === "reasoning_delta" ||
        event.event.type === "tool_call_started" ||
        event.event.type === "tool_call_finished"),
  );
  if (!hasAssistantTurn) return;

  await ensureChatWakeTerminalEventsFromChatLog(context, session, wakeId, {
    status: "completed",
    summary: fallback.summary,
    source: "terminal_fallback",
  });
}

async function ensureChatWakeTerminalEventsFromChatLog(
  context: ServiceWakeDispatchContext,
  session: SessionState,
  wakeId: string,
  input: {
    status: "completed" | "failed";
    summary?: string;
    reasonCode?: string;
    source: string;
    requireCompletion?: boolean;
    requireFinished?: boolean;
    allowWithoutAssistantTurn?: boolean;
  },
): Promise<void> {
  const events = await context.listChatEventsAfterCursor(
    session,
    undefined,
    1_000,
  );
  const wakeEvents = events.filter((event) => {
    const payload = event.payload;
    return isRecord(payload) && payload.wake_id === wakeId;
  });
  const hasAssistantTurn = wakeEvents.some((event) =>
    [
      "assistant_turn_started",
      "assistant_text_delta",
      "assistant_reasoning_delta",
      "tool_call_started",
      "tool_call_completed",
      "tool_call_failed",
    ].includes(event.kind),
  );
  if (!hasAssistantTurn && input.allowWithoutAssistantTurn !== true) return;

  const needsCompletion =
    input.requireCompletion !== false &&
    !wakeEvents.some((event) => event.kind === "assistant_message_completed");
  const needsFinished =
    input.requireFinished !== false &&
    !wakeEvents.some((event) => event.kind === "assistant_turn_finished");
  const summary = input.summary?.trim();
  if (needsCompletion && summary) {
    await context.appendChatEvent(session.sessionId, {
      kind: "assistant_message_completed",
      payload: {
        status: input.status,
        summary,
        wake_id: wakeId,
        source: input.source,
        ...(input.reasonCode === undefined
          ? {}
          : { reason_code: input.reasonCode }),
      },
    });
  }
  if (needsFinished) {
    await context.appendChatEvent(session.sessionId, {
      kind: "assistant_turn_finished",
      payload: {
        wake_id: wakeId,
        source: input.source,
        status: input.status,
        ...(input.reasonCode === undefined
          ? {}
          : { reason_code: input.reasonCode }),
      },
    });
  }
}

async function buildChatWakeFailureSummary(
  context: ServiceWakeDispatchContext,
  session: SessionState | undefined,
  wakeId: string | undefined,
  failureSummary: string,
): Promise<string> {
  const base = failureSummary.trim() || "assistant turn failed";
  if (!session || !wakeId) return base;

  const events = (
    await context.listChatEventsAfterCursor(session, undefined, 1_000)
  ).filter((event) => {
    const payload = event.payload;
    return isRecord(payload) && payload.wake_id === wakeId;
  });
  if (events.length === 0) return base;

  return buildChatWakeFailureSummaryFromEvents({
    failureSummary: base,
    events,
    sessionId: session.sessionId,
    toolDebugLookup: context.toolCallDebugStore,
  });
}

export async function observeWakeEvents<T>(
  context: ServiceWakeDispatchContext,
  sessionId: SessionId,
  callback: () => Promise<T>,
  onEvents?: (events: readonly CoreEvent[]) => void | Promise<void>,
): Promise<{ accepted: T; events: CoreEvent[] }> {
  const subscription = await context.bridge.subscribeEvents({
    eventKinds: [
      "session_execution_observed",
      "logical_turn_lifecycle_observed",
      "brain_event_observed",
      "brain_actions_accepted",
      "completion_packet_delivered",
    ],
    sessionId,
  });
  try {
    const events: CoreEvent[] = [];
    let callbackSettled = false;
    const callbackResult = callback()
      .then((value) => ({ ok: true as const, value }))
      .catch((error: unknown) => ({ ok: false as const, error }))
      .finally(() => {
        callbackSettled = true;
      });

    while (!callbackSettled) {
      await delay(25);
      const chunk = await drainSubscriptionEventsUntilIdle(
        context.bridge,
        subscription,
      );
      if (chunk.length > 0) {
        events.push(...chunk);
        await onEvents?.(chunk);
      }
    }

    const result = await callbackResult;
    if (!result.ok) throw result.error;

    const finalEvents = await drainSubscriptionEventsUntilIdle(
      context.bridge,
      subscription,
    );
    if (finalEvents.length > 0) {
      events.push(...finalEvents);
      await onEvents?.(finalEvents);
    }
    return { accepted: result.value, events };
  } finally {
    await context.bridge.unsubscribeEvents(subscription).catch(() => undefined);
  }
}

function logicalTurnChatEventKind(
  kind: Extract<
    CoreEvent,
    { type: "logical_turn_lifecycle_observed" }
  >["lifecycle"]["kind"],
):
  | "logical_turn_admitted"
  | "logical_turn_continuing"
  | "logical_turn_yielding"
  | "logical_turn_attention_required"
  | "logical_turn_cancelling"
  | "logical_turn_completed"
  | "logical_turn_cancelled"
  | "logical_turn_failed"
  | "unknown" {
  switch (kind) {
    case "continuation_yielded":
      return "logical_turn_yielding";
    case "continuation_resumed":
    case "continuation_claimed":
    case "continuation_checkpointed":
    case "continuation_progress":
      return "logical_turn_continuing";
    case "attention_required":
      return "logical_turn_attention_required";
    case "completed":
      return "logical_turn_completed";
    case "failed":
      return "logical_turn_failed";
    case "cancelled":
      return "logical_turn_cancelled";
    case "cancel_requested":
      return "logical_turn_cancelling";
    case "admitted":
      return "logical_turn_admitted";
    default:
      return "unknown";
  }
}

async function drainSubscriptionEventsUntilIdle(
  bridge: Pick<NativeBridgeModule, "drainSubscriptionEvents">,
  subscription: SubscriptionHandle,
): Promise<CoreEvent[]> {
  const chunkSize = 128;
  const maxEvents = 65_536;
  const events: CoreEvent[] = [];
  while (events.length < maxEvents) {
    const chunk = await bridge.drainSubscriptionEvents(subscription, chunkSize);
    events.push(...chunk);
    if (chunk.length < chunkSize) break;
  }
  return events;
}

async function delay(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function wakeCompletionSummary(
  events: readonly CoreEvent[],
): string | undefined {
  const packet = wakeCompletionPacket(events);
  if (packet?.summary.trim()) {
    return packet.summary.trim();
  }

  const text = mergeTextParts(
    events.flatMap((event) =>
      event.type === "brain_event_observed" && event.event.type === "text_delta"
        ? [event.event.text]
        : [],
    ),
  ).trim();
  return text ? truncate(text, 480) : undefined;
}

function wakeCompletionPacket(
  events: readonly CoreEvent[],
): CompletionPacket | undefined {
  return events
    .filter(
      (
        event,
      ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
        event.type === "completion_packet_delivered",
    )
    .at(-1)?.packet;
}

export function completionPacketProjectionMetadata(
  packet: CompletionPacket | undefined,
): Record<string, unknown> | undefined {
  if (packet === undefined) return undefined;
  return {
    kind: "completion_packet.v1",
    session_id: packet.sessionId,
    status: packet.status,
    summary: packet.summary,
  };
}

function mergeTextParts(parts: readonly string[]): string {
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

function wakeDispatchSkipped(
  context: ServiceWakeDispatchContext,
  sessionId: SessionId,
  reasonCode: string,
  summary: string,
): ServiceWakeDispatchReport {
  context.recordEvent({
    source: "service-host",
    eventType: "brain_wake_skipped",
    severity: "warning",
    summary,
  });
  return { sessionId, status: "skipped", summary, reasonCode };
}

export function runtimePauseWakeReport(
  context: ServiceWakeDispatchContext,
  sessionId: SessionId,
  pause: WakeRuntimePauseRecord,
): ServiceWakeDispatchReport {
  return wakeDispatchSkipped(
    context,
    sessionId,
    "runtime_paused",
    runtimePauseSummary(pause, sessionId),
  );
}

export function runtimePauseSummary(
  pause: WakeRuntimePauseRecord,
  sessionId: string,
): string {
  const reason = pause.reason ? `: ${pause.reason}` : "";
  return `runtime wake for ${sessionId} is paused by ${pause.scope} ${pause.targetId}${reason}`;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
