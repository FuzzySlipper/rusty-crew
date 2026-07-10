import {
  type AgentActivityObservationProducer,
  type AgentActivityPublishResult,
  type AgentObservationIdentity,
  workActivity,
} from "./agent-activity-observation.js";

export interface CuratorActivityReceipt {
  sequence: number;
  receiptId: string;
  correlationId?: string;
  profileId?: string;
  sessionId?: string;
  candidateId?: string;
  mutationId?: string;
  activityKind: string;
  outcome: string;
  reasonCode?: string;
  summary: string;
  occurredAt: string;
}

export async function publishCuratorActivityObservation(input: {
  producer: AgentActivityObservationProducer;
  receipt: CuratorActivityReceipt;
  identity?: AgentObservationIdentity;
}): Promise<AgentActivityPublishResult> {
  const identity = input.identity ?? {
    profile: input.receipt.profileId ?? "service",
    instance_id: `curator:${input.receipt.profileId ?? "service"}`,
    ...(input.receipt.sessionId
      ? { session_key: input.receipt.sessionId }
      : {}),
  };
  return input.producer.publish(
    workActivity({
      eventType: curatorObservationEventType(input.receipt),
      identity,
      summary: input.receipt.summary,
      workRef: {
        ...(input.receipt.correlationId
          ? { run_id: input.receipt.correlationId }
          : {}),
        ...(input.receipt.sessionId
          ? { session_id: input.receipt.sessionId }
          : {}),
      },
      resultRef: {
        artifact_path: `curator://receipt/${input.receipt.receiptId}`,
      },
      ...(input.receipt.reasonCode
        ? { reasonCode: input.receipt.reasonCode }
        : {}),
      visibility: "agent",
    }),
  );
}

function curatorObservationEventType(
  receipt: CuratorActivityReceipt,
):
  | "work_started"
  | "work_checkpoint"
  | "work_waiting"
  | "work_completed"
  | "work_failed" {
  if (receipt.outcome === "failed" || receipt.activityKind.endsWith("failed")) {
    return "work_failed";
  }
  switch (receipt.activityKind) {
    case "candidate_discovered":
      return "work_started";
    case "candidate_denied":
      return "work_failed";
    case "candidate_staled":
    case "candidate_archived":
      return "work_waiting";
    case "mutation_applied":
    case "rollback_completed":
      return "work_completed";
    default:
      return "work_checkpoint";
  }
}
