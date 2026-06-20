import type {
  AgentId,
  AgentInstanceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";

export type ObservationSourceDomain =
  | "observation"
  | "delivery"
  | "runtime"
  | "conversation"
  | "legacy";

export type AgentActivityEventType =
  | "agent_session_started"
  | "agent_session_resumed"
  | "agent_session_idle"
  | "agent_session_blocked"
  | "agent_session_failed"
  | "agent_session_stopped"
  | "work_started"
  | "work_checkpoint"
  | "work_waiting"
  | "work_completed"
  | "work_failed"
  | "model_turn_started"
  | "model_turn_completed"
  | "tool_call_started"
  | "tool_call_completed"
  | "tool_call_failed"
  | "adapter_connected"
  | "adapter_disconnected"
  | "adapter_degraded"
  | "adapter_recovered"
  | "admin_command_started"
  | "admin_command_completed"
  | "admin_command_failed";

export type AgentActivitySeverity = "info" | "success" | "warning" | "error";
export type AgentActivityVisibility = "channel" | "task" | "agent" | "debug";

export interface AgentObservationIdentity {
  profile: ProfileId | string;
  instance_id: AgentInstanceId | AgentId | string;
  session_key?: SessionId | string;
}

export interface AgentActivityWorkRef {
  project_id?: string;
  task_id?: string | number;
  assignment_id?: string | number;
  run_id?: string;
  review_round_id?: string | number;
  channel_id?: string | number;
  channel_message_id?: string | number;
  session_id?: SessionId | string;
}

export interface AgentActivityResultRef {
  document_slug?: string;
  message_id?: string | number;
  commit?: string;
  artifact_path?: string;
}

export interface AgentActivityPayload {
  kind: "agent_activity.v1";
  schema_version: 1;
  summary: string;
  severity: AgentActivitySeverity;
  visibility: AgentActivityVisibility;
  adapter: string;
  surface: string;
  work_ref?: AgentActivityWorkRef;
  session_key?: SessionId | string;
  tool_name?: string;
  model?: string;
  reason_code?: string;
  result_ref?: AgentActivityResultRef;
}

export interface AgentActivityObservationEvent {
  source_domain: ObservationSourceDomain;
  event_type: AgentActivityEventType;
  agent_identity: AgentObservationIdentity;
  runtime_instance_id?: string;
  payload: AgentActivityPayload;
}

export interface AgentActivityEventInput {
  sourceDomain?: ObservationSourceDomain;
  eventType: AgentActivityEventType;
  identity: AgentObservationIdentity;
  runtimeInstanceId?: string;
  summary: string;
  severity?: AgentActivitySeverity;
  visibility?: AgentActivityVisibility;
  adapter?: string;
  surface?: string;
  workRef?: AgentActivityWorkRef;
  sessionKey?: SessionId | string;
  toolName?: string;
  model?: string;
  reasonCode?: string;
  resultRef?: AgentActivityResultRef;
}

export interface AgentActivityObservationSink {
  writeAgentActivity(
    event: AgentActivityObservationEvent,
  ): Promise<unknown> | unknown;
}

export type AgentActivityPublishResult =
  | {
      status: "published";
      event: AgentActivityObservationEvent;
    }
  | {
      status: "skipped" | "degraded";
      reasonCode: "observation_unavailable";
      message: string;
      event: AgentActivityObservationEvent;
    };

export interface AgentActivityObservationProducerOptions {
  sink?: AgentActivityObservationSink;
  required?: boolean;
}

export class AgentActivityObservationProducer {
  readonly #sink: AgentActivityObservationSink | undefined;
  readonly #required: boolean;

  constructor(options: AgentActivityObservationProducerOptions = {}) {
    this.#sink = options.sink;
    this.#required = options.required ?? false;
  }

  async publish(
    input: AgentActivityEventInput,
  ): Promise<AgentActivityPublishResult> {
    const event = createAgentActivityObservationEvent(input);
    if (this.#sink === undefined) {
      return {
        status: this.#required ? "degraded" : "skipped",
        reasonCode: "observation_unavailable",
        message: "observation sink is not configured",
        event,
      };
    }

    try {
      await this.#sink.writeAgentActivity(event);
      return { status: "published", event };
    } catch (error) {
      return {
        status: "degraded",
        reasonCode: "observation_unavailable",
        message: error instanceof Error ? error.message : String(error),
        event,
      };
    }
  }
}

export interface MemoryAgentActivityObservationSink extends AgentActivityObservationSink {
  readonly events: AgentActivityObservationEvent[];
  failNext(error?: Error): void;
}

export function createMemoryAgentActivityObservationSink(): MemoryAgentActivityObservationSink {
  const events: AgentActivityObservationEvent[] = [];
  let nextError: Error | undefined;
  return {
    events,
    failNext(error = new Error("observation sink unavailable")) {
      nextError = error;
    },
    writeAgentActivity(event) {
      if (nextError) {
        const error = nextError;
        nextError = undefined;
        throw error;
      }
      events.push(event);
    },
  };
}

export function createAgentActivityObservationEvent(
  input: AgentActivityEventInput,
): AgentActivityObservationEvent {
  const sessionKey = input.sessionKey ?? input.identity.session_key;
  return {
    source_domain: input.sourceDomain ?? "runtime",
    event_type: input.eventType,
    agent_identity: input.identity,
    runtime_instance_id: input.runtimeInstanceId,
    payload: {
      kind: "agent_activity.v1",
      schema_version: 1,
      summary: boundedSummary(input.summary),
      severity: input.severity ?? severityForEvent(input.eventType),
      visibility: input.visibility ?? "agent",
      adapter: input.adapter ?? "rusty-crew",
      surface: input.surface ?? surfaceForEvent(input.eventType),
      work_ref: input.workRef,
      session_key: sessionKey,
      tool_name: input.toolName,
      model: input.model,
      reason_code: input.reasonCode,
      result_ref: input.resultRef,
    },
  };
}

export function sessionActivity(input: {
  eventType: Extract<
    AgentActivityEventType,
    | "agent_session_started"
    | "agent_session_resumed"
    | "agent_session_idle"
    | "agent_session_blocked"
    | "agent_session_failed"
    | "agent_session_stopped"
  >;
  identity: AgentObservationIdentity;
  summary: string;
  reasonCode?: string;
  surface?: string;
}): AgentActivityEventInput {
  return {
    eventType: input.eventType,
    identity: input.identity,
    summary: input.summary,
    reasonCode: input.reasonCode,
    sessionKey: input.identity.session_key,
    surface: input.surface ?? "runtime",
  };
}

export function workActivity(input: {
  eventType: Extract<
    AgentActivityEventType,
    | "work_started"
    | "work_checkpoint"
    | "work_waiting"
    | "work_completed"
    | "work_failed"
  >;
  identity: AgentObservationIdentity;
  summary: string;
  workRef: AgentActivityWorkRef;
  resultRef?: AgentActivityResultRef;
  reasonCode?: string;
  visibility?: AgentActivityVisibility;
}): AgentActivityEventInput {
  return {
    eventType: input.eventType,
    identity: input.identity,
    summary: input.summary,
    workRef: input.workRef,
    resultRef: input.resultRef,
    reasonCode: input.reasonCode,
    visibility: input.visibility ?? "task",
    surface: "task",
  };
}

export function toolActivity(input: {
  eventType: Extract<
    AgentActivityEventType,
    "tool_call_started" | "tool_call_completed" | "tool_call_failed"
  >;
  identity: AgentObservationIdentity;
  toolName: string;
  summary: string;
  resultRef?: AgentActivityResultRef;
  reasonCode?: string;
  visibility?: AgentActivityVisibility;
}): AgentActivityEventInput {
  return {
    eventType: input.eventType,
    identity: input.identity,
    toolName: input.toolName,
    summary: input.summary,
    resultRef: input.resultRef,
    reasonCode: input.reasonCode,
    visibility: input.visibility ?? "debug",
    surface: "runtime",
  };
}

export function adapterActivity(input: {
  eventType: Extract<
    AgentActivityEventType,
    | "adapter_connected"
    | "adapter_disconnected"
    | "adapter_degraded"
    | "adapter_recovered"
  >;
  identity: AgentObservationIdentity;
  adapter: string;
  surface: string;
  summary: string;
  reasonCode?: string;
}): AgentActivityEventInput {
  return {
    eventType: input.eventType,
    identity: input.identity,
    adapter: input.adapter,
    surface: input.surface,
    summary: input.summary,
    reasonCode: input.reasonCode,
    visibility: "debug",
  };
}

export function adminCommandActivity(input: {
  eventType: Extract<
    AgentActivityEventType,
    "admin_command_started" | "admin_command_completed" | "admin_command_failed"
  >;
  identity: AgentObservationIdentity;
  commandName: string;
  summary: string;
  reasonCode?: string;
  resultRef?: AgentActivityResultRef;
}): AgentActivityEventInput {
  return {
    eventType: input.eventType,
    identity: input.identity,
    summary: input.summary,
    reasonCode: input.reasonCode,
    resultRef: input.resultRef,
    workRef: { run_id: `command:${input.commandName}` },
    visibility: "debug",
    surface: "runtime",
  };
}

function boundedSummary(summary: string): string {
  const trimmed = summary.trim();
  if (trimmed.length <= 240) return trimmed;
  return `${trimmed.slice(0, 237)}...`;
}

function severityForEvent(
  eventType: AgentActivityEventType,
): AgentActivitySeverity {
  if (
    eventType.endsWith("_failed") ||
    eventType === "agent_session_failed" ||
    eventType === "tool_call_failed"
  ) {
    return "error";
  }
  if (
    eventType.endsWith("_completed") ||
    eventType === "work_completed" ||
    eventType === "adapter_recovered"
  ) {
    return "success";
  }
  if (
    eventType.endsWith("_blocked") ||
    eventType === "work_waiting" ||
    eventType === "adapter_degraded" ||
    eventType === "adapter_disconnected"
  ) {
    return "warning";
  }
  return "info";
}

function surfaceForEvent(eventType: AgentActivityEventType): string {
  if (eventType.startsWith("work_")) return "task";
  if (eventType.startsWith("adapter_")) return "runtime";
  if (eventType.startsWith("tool_") || eventType.startsWith("model_")) {
    return "runtime";
  }
  return "runtime";
}
