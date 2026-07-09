import type {
  BrainEvent,
  BrainImplementationHandle,
  CompletionPacket,
  CoreEvent,
  ProfileId,
  SessionId,
  SessionState,
  SubscriptionHandle,
  ToolCallMetadata,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { buildChatWakeFailureSummaryFromEvents } from "./chat-wake-failure-summary.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import {
  effectiveTurnTimeoutMs,
  WakeDispatchTimeoutError,
  withWakeTimeout,
} from "./wake-timeout.js";
import {
  buildProfileRoleAssembly,
  type BuildProfileRoleAssemblyOptions,
} from "./profile-role-assembly.js";
import type { loadProfileContext } from "./profile-loading.js";
import {
  effectiveWakeTimeoutMs,
  type RustyCrewConfiguredSession,
  type RustyCrewRuntimeConfig,
  type ServiceBrainWakeResultObservation,
} from "./service-runtime-config.js";

export interface ServiceWakeDispatchReport {
  sessionId: SessionId;
  wakeId?: string;
  status: "completed" | "skipped" | "failed";
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
    | "subscribeEvents"
    | "unsubscribeEvents"
    | "wakeBrain"
  >;
  inFlightWakes: Set<SessionId>;
  toolCallDebugStore: ToolCallDebugStore;
  wakeTimeout: RustyCrewRuntimeConfig["wakeTimeout"];
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
  recordEvent(event: {
    source: string;
    eventType: string;
    summary: string;
    severity?: string;
    workRef?: Record<string, unknown>;
    resultRef?: Record<string, unknown>;
  }): void;
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
  if (context.inFlightWakes.has(sessionId)) {
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
    const profileContext = await context.loadProfileContext(session.profileId);
    const configured = context.configuredSessionForRuntimeSession(session);
    const contextStrategy = await context.prepareContextStrategy({
      session,
      configuredSession: configured,
      profileContext,
    });
    const roleplayContext =
      await context.roleplayPromptContextForSession(session);
    const roleInput: BuildProfileRoleAssemblyOptions = {
      sessionMemoryContext: contextStrategy.sessionMemoryContext,
      additionalInstructions: [
        ...contextStrategy.additionalInstructions,
        ...(roleplayContext === undefined ? [] : [roleplayContext]),
      ],
    };
    const role = buildProfileRoleAssembly(profileContext, roleInput);
    const turnTimeoutMs = effectiveTurnTimeoutMs(
      effectiveWakeTimeoutMs({
        session: configured,
        profile: profileContext.profile,
        service: context.wakeTimeout,
      }),
    );
    const wakeTimeoutController = new AbortController();
    const observed = await withWakeTimeout(
      observeWakeEvents(
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
          return context.bridge.wakeBrain(request, {
            signal: wakeTimeoutController.signal,
          });
        },
        appendChatEvents
          ? (events) =>
              appendCoreEventsToChatLog(context, session, wakeId, events)
          : undefined,
        wakeTimeoutController.signal,
      ),
      {
        wakeId,
        sessionId,
        timeoutMs: turnTimeoutMs,
        onTimeout: () => wakeTimeoutController.abort(),
      },
    );
    await context.publishWakeToolActivity({
      session,
      wakeId,
      events: observed.events,
      observationContext,
    });
    const accepted = observed.accepted;
    const completionPacket = wakeCompletionPacket(observed.events);
    const completionSummary = wakeCompletionSummary(observed.events);
    const report: ServiceWakeDispatchReport = {
      sessionId,
      wakeId,
      status: accepted.accepted ? "completed" : "failed",
      summary:
        completionSummary ??
        (accepted.accepted
          ? `wake ${wakeId} completed for ${session.agentId}`
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
    return report;
  } catch (error) {
    if (error instanceof WakeDispatchTimeoutError) {
      const report: ServiceWakeDispatchReport = {
        sessionId,
        wakeId: error.wakeId,
        status: "failed",
        summary: await buildChatWakeFailureSummary(
          context,
          activeWake?.session,
          error.wakeId,
          `wake ${error.wakeId} timed out after ${error.timeoutMs}ms`,
        ),
        reasonCode: "wake_timeout",
      };
      if (appendChatEvents && activeWake !== undefined) {
        await ensureChatWakeTerminalEventsFromChatLog(
          context,
          activeWake.session,
          error.wakeId,
          {
            status: "failed",
            summary: report.summary,
            reasonCode: report.reasonCode,
            source: "wake_timeout",
            allowWithoutAssistantTurn: true,
          },
        );
      }
      context.recordEvent({
        source: "service-host",
        eventType: "brain_wake_timeout",
        severity: "error",
        summary: `${report.summary} (${source}).`,
      });
      return report;
    }
    const report: ServiceWakeDispatchReport = {
      sessionId,
      wakeId: activeWake?.wakeId,
      status: "failed",
      summary: await buildChatWakeFailureSummary(
        context,
        activeWake?.session,
        activeWake?.wakeId,
        errorMessage(error, `wake for ${sessionId} failed`),
      ),
      reasonCode: "wake_dispatch_failed",
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
          source: "wake_dispatch_failed",
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
    return report;
  } finally {
    context.inFlightWakes.delete(sessionId);
  }
}

async function appendCoreEventsToChatLog(
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
        event.wakeId,
        event.event,
      );
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
      await context.appendChatEvent(session.sessionId, {
        kind: "provider_status",
        payload: {
          wake_id: wakeId,
          level: event.level,
          message: event.message,
          ...(event.metadataJson === undefined
            ? {}
            : { metadata_json: event.metadataJson }),
        },
      });
      return;
    case "tool_call_started":
      await context.appendChatEvent(session.sessionId, {
        kind: "tool_call_started",
        payload: {
          wake_id: wakeId,
          tool_call_id: chatToolCallId(wakeId, event.toolName, event.metadata),
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
          tool_call_id: chatToolCallId(wakeId, event.toolName, event.metadata),
          tool_name: event.toolName,
          is_error: event.isError,
          debug_detail_id: event.metadata?.debugDetailId,
          metadata: event.metadata,
        },
      });
      return;
    case "finished":
      await context.appendChatEvent(session.sessionId, {
        kind: "assistant_turn_finished",
        payload: { wake_id: wakeId },
      });
      return;
  }
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

  const hasCompletion = wakeEvents.some(
    (event) => event.type === "completion_packet_delivered",
  );
  const hasFinished = wakeEvents.some(
    (event) =>
      event.type === "brain_event_observed" && event.event.type === "finished",
  );

  await ensureChatWakeTerminalEventsFromChatLog(context, session, wakeId, {
    status: "completed",
    summary: fallback.summary,
    source: "terminal_fallback",
    requireCompletion: !hasCompletion,
    requireFinished: !hasFinished,
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

async function observeWakeEvents<T>(
  context: ServiceWakeDispatchContext,
  sessionId: SessionId,
  callback: () => Promise<T>,
  onEvents?: (events: readonly CoreEvent[]) => void | Promise<void>,
  signal?: AbortSignal,
): Promise<{ accepted: T; events: CoreEvent[] }> {
  const subscription = await context.bridge.subscribeEvents({
    eventKinds: [
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
      if (signal?.aborted) {
        throw new Error(`wake event observation aborted for ${sessionId}`);
      }
      await delay(25);
      if (signal?.aborted) {
        throw new Error(`wake event observation aborted for ${sessionId}`);
      }
      const chunk = await drainSubscriptionEventsUntilIdle(
        context.bridge,
        subscription,
      );
      if (signal?.aborted) {
        throw new Error(`wake event observation aborted for ${sessionId}`);
      }
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
