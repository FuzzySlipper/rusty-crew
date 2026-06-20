import {
  adapterActivity,
  sessionActivity,
  toolActivity,
  workActivity,
  type AgentActivityObservationProducer,
  type AgentActivityPublishResult,
  type AgentActivityResultRef,
  type AgentActivityWorkRef,
  type AgentObservationIdentity,
} from "./agent-activity-observation.js";

export type RuntimeActivityResult =
  | AgentActivityPublishResult
  | {
      status: "suppressed";
      reasonCode: "low_signal_tool_call";
      message: string;
    };

export interface RuntimeActivityObserverOptions {
  producer: AgentActivityObservationProducer;
  identity: AgentObservationIdentity;
  runtimeInstanceId?: string;
}

export interface RuntimeSessionActivityInput {
  eventType:
    | "agent_session_started"
    | "agent_session_resumed"
    | "agent_session_idle"
    | "agent_session_blocked"
    | "agent_session_failed"
    | "agent_session_stopped";
  summary: string;
  reasonCode?: string;
}

export interface RuntimeWorkActivityInput {
  eventType:
    | "work_started"
    | "work_checkpoint"
    | "work_waiting"
    | "work_completed"
    | "work_failed";
  summary: string;
  workRef: AgentActivityWorkRef;
  resultRef?: AgentActivityResultRef;
  reasonCode?: string;
}

export interface RuntimeToolActivityInput {
  eventType: "tool_call_started" | "tool_call_completed" | "tool_call_failed";
  toolName: string;
  summary: string;
  longRunningOrRisky?: boolean;
  resultRef?: AgentActivityResultRef;
  reasonCode?: string;
}

export interface RuntimeAdapterActivityInput {
  eventType:
    | "adapter_connected"
    | "adapter_disconnected"
    | "adapter_degraded"
    | "adapter_recovered";
  adapter: string;
  surface: string;
  summary: string;
  reasonCode?: string;
}

export class RuntimeActivityObserver {
  readonly #producer: AgentActivityObservationProducer;
  readonly #identity: AgentObservationIdentity;
  readonly #runtimeInstanceId: string | undefined;

  constructor(options: RuntimeActivityObserverOptions) {
    this.#producer = options.producer;
    this.#identity = options.identity;
    this.#runtimeInstanceId = options.runtimeInstanceId;
  }

  session(input: RuntimeSessionActivityInput): Promise<RuntimeActivityResult> {
    return this.#producer.publish({
      ...sessionActivity({
        eventType: input.eventType,
        identity: this.#identity,
        summary: input.summary,
        reasonCode: input.reasonCode,
      }),
      runtimeInstanceId: this.#runtimeInstanceId,
    });
  }

  work(input: RuntimeWorkActivityInput): Promise<RuntimeActivityResult> {
    return this.#producer.publish({
      ...workActivity({
        eventType: input.eventType,
        identity: this.#identity,
        summary: input.summary,
        workRef: input.workRef,
        resultRef: input.resultRef,
        reasonCode: input.reasonCode,
      }),
      runtimeInstanceId: this.#runtimeInstanceId,
    });
  }

  tool(input: RuntimeToolActivityInput): Promise<RuntimeActivityResult> {
    if (!input.longRunningOrRisky && input.eventType !== "tool_call_failed") {
      return Promise.resolve({
        status: "suppressed",
        reasonCode: "low_signal_tool_call",
        message: "tool activity is not long-running, risky, or failed",
      });
    }
    return this.#producer.publish({
      ...toolActivity({
        eventType: input.eventType,
        identity: this.#identity,
        toolName: input.toolName,
        summary: input.summary,
        resultRef: input.resultRef,
        reasonCode: input.reasonCode,
      }),
      runtimeInstanceId: this.#runtimeInstanceId,
    });
  }

  adapter(input: RuntimeAdapterActivityInput): Promise<RuntimeActivityResult> {
    return this.#producer.publish({
      ...adapterActivity({
        eventType: input.eventType,
        identity: this.#identity,
        adapter: input.adapter,
        surface: input.surface,
        summary: input.summary,
        reasonCode: input.reasonCode,
      }),
      runtimeInstanceId: this.#runtimeInstanceId,
    });
  }
}

export function createRuntimeActivityObserver(
  options: RuntimeActivityObserverOptions,
): RuntimeActivityObserver {
  return new RuntimeActivityObserver(options);
}
