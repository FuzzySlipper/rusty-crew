import type { AgentId, AgentMessage } from "@rusty-crew/contracts";

export function toNativeAgentMessage(message: AgentMessage): RawAgentMessage {
  return {
    from: message.from,
    to: message.to,
    from_session_id: message.fromSessionId ?? undefined,
    to_session_id: message.toSessionId ?? undefined,
    body: message.body,
    correlation_id: message.correlationId ?? undefined,
    projection: message.projection
      ? {
          visibility: message.projection.visibility,
          target_ref: message.projection.targetRef ?? undefined,
          work_ref: message.projection.workRef ?? undefined,
          reason: message.projection.reason ?? undefined,
        }
      : undefined,
  };
}

export function toAgentMessage(message: RawAgentMessage): AgentMessage {
  return {
    from: message.from,
    to: message.to,
    fromSessionId: message.from_session_id,
    toSessionId: message.to_session_id,
    body: message.body,
    correlationId: message.correlation_id,
    projection: message.projection
      ? {
          visibility: message.projection.visibility,
          targetRef: message.projection.target_ref,
          workRef: message.projection.work_ref,
          reason: message.projection.reason,
        }
      : undefined,
  };
}

export interface RawAgentMessage {
  from: AgentId;
  to: AgentId;
  from_session_id?: string;
  to_session_id?: string;
  body: string;
  correlation_id?: string;
  projection?: {
    visibility: "observation" | "user_visible";
    target_ref?: {
      system: string;
      kind: string;
      id: string;
    };
    work_ref?: {
      system: string;
      kind: string;
      id: string;
    };
    reason?: string;
  };
}
