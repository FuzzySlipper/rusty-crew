import type {
  AgentMessage,
  CompletionStatus,
  CoreEvent,
  CoreEventKind,
  ProjectionRef,
  SessionId,
  SessionKind,
  SessionState,
} from "@rusty-crew/contracts";
import type {
  AgentActivityEventInput,
  AgentActivityVisibility,
  AgentActivityWorkRef,
  AgentObservationIdentity,
} from "./agent-activity-observation.js";

export interface RuntimeObservationSessionIdentity {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind?: SessionKind;
}

export interface DenObservationEventFilter {
  eventKind: CoreEventKind;
  visibility?: AgentActivityVisibility;
  sessionKind?: SessionKind;
  completionStatus?: CompletionStatus;
  profileId?: string;
  agentId?: string;
}

export interface RuntimeCoreEventObservationOptions {
  lookupSession?: (
    sessionId: SessionId | string,
  ) => RuntimeObservationSessionIdentity | undefined;
  filters?: readonly DenObservationEventFilter[];
}

export const DEFAULT_DEN_OBSERVATION_EVENT_FILTERS: readonly DenObservationEventFilter[] =
  [
    { eventKind: "session_created" },
    { eventKind: "session_archived" },
    { eventKind: "agent_message_routed", visibility: "channel" },
    { eventKind: "delegation_lifecycle_observed" },
    { eventKind: "brain_wake_requested" },
    { eventKind: "brain_actions_accepted" },
    { eventKind: "completion_packet_delivered" },
  ];

export function runtimeCoreEventObservationInput(
  event: CoreEvent,
  options:
    | RuntimeCoreEventObservationOptions
    | ((
        sessionId: SessionId | string,
      ) => RuntimeObservationSessionIdentity | undefined) = {},
): AgentActivityEventInput | undefined {
  const normalized =
    typeof options === "function" ? { lookupSession: options } : options;
  const lookupSession = normalized.lookupSession;
  const filters = normalized.filters ?? DEFAULT_DEN_OBSERVATION_EVENT_FILTERS;
  const input = runtimeCoreEventObservationInputUnchecked(event, lookupSession);
  if (input === undefined) return undefined;
  if (!passesObservationFilters(event, input, lookupSession, filters)) {
    return undefined;
  }
  return input;
}

function runtimeCoreEventObservationInputUnchecked(
  event: CoreEvent,
  lookupSession?: (
    sessionId: SessionId | string,
  ) => RuntimeObservationSessionIdentity | undefined,
): AgentActivityEventInput | undefined {
  switch (event.type) {
    case "session_created":
      return {
        eventType: "agent_session_started",
        identity: identityForSession(event.state),
        summary: `Session ${event.state.sessionId} started for ${event.state.agentId}.`,
        sessionKey: event.state.sessionId,
        visibility: "agent",
        surface: "runtime",
      };
    case "session_archived": {
      const session = lookupSession?.(event.sessionId);
      return {
        eventType: "agent_session_stopped",
        identity: identityForKnownOrSessionId(session, event.sessionId),
        summary: `Session ${event.sessionId} archived.`,
        sessionKey: event.sessionId,
        visibility: "agent",
        surface: "runtime",
      };
    }
    case "brain_wake_requested": {
      const session = lookupSession?.(event.sessionId);
      return {
        eventType: "model_turn_started",
        identity: identityForKnownOrSessionId(session, event.sessionId),
        summary: `Brain wake requested for ${event.sessionId}.`,
        sessionKey: event.sessionId,
        visibility: "debug",
        surface: "runtime",
        workRef: { session_id: event.sessionId },
      };
    }
    case "brain_actions_accepted": {
      const session = lookupSession?.(event.sessionId);
      return {
        eventType: "model_turn_completed",
        identity: identityForKnownOrSessionId(session, event.sessionId),
        summary: `Brain actions accepted for ${event.sessionId}: ${event.count}.`,
        sessionKey: event.sessionId,
        visibility: "debug",
        surface: "runtime",
        workRef: { session_id: event.sessionId },
      };
    }
    case "completion_packet_delivered": {
      const session = lookupSession?.(event.packet.sessionId);
      return {
        eventType: completionEventType(event.packet.status),
        identity: identityForKnownOrSessionId(session, event.packet.sessionId),
        summary: completionSummary(event.packet.status, event.packet.summary),
        sessionKey: event.packet.sessionId,
        visibility: "task",
        surface: "task",
        reasonCode:
          event.packet.status === "completed"
            ? undefined
            : `completion_${event.packet.status}`,
        workRef: { session_id: event.packet.sessionId },
      };
    }
    case "agent_message_routed":
      return projectedAgentMessageObservation(event.message);
    case "delegation_lifecycle_observed": {
      const session = lookupSession?.(event.lifecycle.delegatedSessionId);
      return {
        eventType:
          event.lifecycle.phase === "completed"
            ? "work_completed"
            : event.lifecycle.phase === "failed"
              ? "work_failed"
              : event.lifecycle.phase === "blocked" ||
                  event.lifecycle.phase === "timed_out" ||
                  event.lifecycle.phase === "exhausted"
                ? "work_waiting"
                : event.lifecycle.phase === "created" ||
                    event.lifecycle.phase === "wake_requested"
                  ? "work_started"
                  : "work_checkpoint",
        identity: identityForKnownOrSessionId(
          session,
          event.lifecycle.delegatedSessionId,
        ),
        summary: `Delegation ${event.lifecycle.phase} for ${event.lifecycle.delegatedSessionId}.`,
        sessionKey: event.lifecycle.delegatedSessionId,
        visibility: "task",
        surface: "task",
        reasonCode:
          event.lifecycle.phase === "completed"
            ? undefined
            : `delegation_${event.lifecycle.phase}`,
        workRef: {
          session_id: event.lifecycle.delegatedSessionId,
          run_id: event.lifecycle.runId,
        },
      };
    }
    case "brain_event_observed":
    case "external_event_injected":
    case "den_data_updated":
      return undefined;
  }
}

function passesObservationFilters(
  event: CoreEvent,
  input: AgentActivityEventInput,
  lookupSession:
    | ((
        sessionId: SessionId | string,
      ) => RuntimeObservationSessionIdentity | undefined)
    | undefined,
  filters: readonly DenObservationEventFilter[],
): boolean {
  return filters.some((filter) => {
    if (filter.eventKind !== event.type) return false;
    if (
      filter.visibility !== undefined &&
      filter.visibility !== input.visibility
    )
      return false;
    if (
      filter.completionStatus !== undefined &&
      (event.type !== "completion_packet_delivered" ||
        event.packet.status !== filter.completionStatus)
    ) {
      return false;
    }
    const session = sessionForFilter(event, lookupSession);
    if (
      filter.sessionKind !== undefined &&
      session?.kind !== filter.sessionKind
    ) {
      return false;
    }
    if (
      filter.profileId !== undefined &&
      session?.profileId !== filter.profileId
    )
      return false;
    if (filter.agentId !== undefined && session?.agentId !== filter.agentId)
      return false;
    return true;
  });
}

function sessionForFilter(
  event: CoreEvent,
  lookupSession:
    | ((
        sessionId: SessionId | string,
      ) => RuntimeObservationSessionIdentity | undefined)
    | undefined,
): RuntimeObservationSessionIdentity | undefined {
  switch (event.type) {
    case "session_created":
      return {
        sessionId: event.state.sessionId,
        agentId: event.state.agentId,
        profileId: event.state.profileId,
        kind: event.state.kind,
      };
    case "session_archived":
      return lookupSession?.(event.sessionId);
    case "brain_wake_requested":
    case "brain_actions_accepted":
      return lookupSession?.(event.sessionId);
    case "completion_packet_delivered":
      return lookupSession?.(event.packet.sessionId);
    case "delegation_lifecycle_observed":
      return lookupSession?.(event.lifecycle.delegatedSessionId);
    case "agent_message_routed":
    case "brain_event_observed":
    case "external_event_injected":
    case "den_data_updated":
      return undefined;
  }
}

function projectedAgentMessageObservation(
  message: AgentMessage,
): AgentActivityEventInput | undefined {
  const projection = message.projection;
  if (projection === undefined) return undefined;
  return {
    eventType: "work_checkpoint",
    identity: {
      profile: message.from,
      instance_id: `${message.from}@rusty-crew`,
    },
    summary:
      projection.reason?.trim() ||
      `Projected message routed ${message.from} -> ${message.to}.`,
    visibility: projectionVisibility(projection.visibility),
    surface: "conversation",
    reasonCode: "projected_agent_message",
    workRef: workRefFromProjectionRefs(
      projection.targetRef,
      projection.workRef,
    ),
    resultRef: {
      message_id: message.correlationId,
    },
  };
}

function identityForSession(session: SessionState): AgentObservationIdentity {
  return {
    profile: session.profileId,
    instance_id: `${session.agentId}@rusty-crew`,
    session_key: session.sessionId,
  };
}

function identityForKnownOrSessionId(
  session: RuntimeObservationSessionIdentity | undefined,
  sessionId: string,
): AgentObservationIdentity {
  if (session !== undefined) {
    return {
      profile: session.profileId,
      instance_id: `${session.agentId}@rusty-crew`,
      session_key: session.sessionId,
    };
  }
  return {
    profile: "unknown",
    instance_id: "rusty-crew@runtime",
    session_key: sessionId,
  };
}

function completionEventType(
  status: CompletionStatus,
): AgentActivityEventInput["eventType"] {
  switch (status) {
    case "completed":
      return "work_completed";
    case "failed":
      return "work_failed";
    case "blocked":
    case "exhausted":
      return "work_waiting";
  }
}

function completionSummary(status: CompletionStatus, summary: string): string {
  const trimmed = summary.trim();
  if (trimmed) return trimmed;
  return `Completion packet delivered with status ${status}.`;
}

function projectionVisibility(
  visibility: NonNullable<AgentMessage["projection"]>["visibility"],
): AgentActivityVisibility {
  return visibility === "user_visible" ? "channel" : "agent";
}

function workRefFromProjectionRefs(
  targetRef: ProjectionRef | undefined,
  workRef: ProjectionRef | undefined,
): AgentActivityWorkRef | undefined {
  const ref: AgentActivityWorkRef = {};
  applyProjectionRef(ref, targetRef);
  applyProjectionRef(ref, workRef);
  return Object.keys(ref).length > 0 ? ref : undefined;
}

function applyProjectionRef(
  output: AgentActivityWorkRef,
  ref: ProjectionRef | undefined,
): void {
  if (ref === undefined) return;
  if (ref.system === "den" && ref.kind === "project") {
    output.project_id = ref.id;
    return;
  }
  if (ref.system === "den" && ref.kind === "task") {
    output.task_id = ref.id;
    return;
  }
  if (ref.kind === "session") {
    output.session_id = ref.id;
    return;
  }
  if (output.run_id === undefined) {
    output.run_id = `${ref.system}:${ref.kind}:${ref.id}`;
  }
}
