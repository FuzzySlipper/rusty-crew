import {
  type AgentActivityObservationProducer,
  type AgentActivityPublishResult,
  type AgentActivityResultRef,
  type AgentActivityWorkRef,
  type AgentObservationIdentity,
  adapterActivity,
  workActivity,
} from "./agent-activity-observation.js";

export type BackgroundGovernanceLoopKind =
  | "scheduler"
  | "curator"
  | "background_review"
  | "cleanup"
  | "adapter_check";

export type BackgroundGovernancePhase =
  | "started"
  | "completed"
  | "failed"
  | "degraded"
  | "recovered";

export interface BackgroundGovernanceObservationInput {
  producer?: AgentActivityObservationProducer;
  identity: AgentObservationIdentity;
  loopKind: BackgroundGovernanceLoopKind;
  phase: BackgroundGovernancePhase;
  summary: string;
  workRef?: AgentActivityWorkRef;
  resultRef?: AgentActivityResultRef;
  reasonCode?: string;
  adapter?: string;
}

export async function publishBackgroundGovernanceObservation(
  input: BackgroundGovernanceObservationInput,
): Promise<AgentActivityPublishResult | undefined> {
  if (!input.producer) return undefined;
  if (input.loopKind === "adapter_check") {
    return input.producer.publish(
      adapterActivity({
        eventType:
          input.phase === "recovered"
            ? "adapter_recovered"
            : "adapter_degraded",
        identity: input.identity,
        adapter: input.adapter ?? "background",
        surface: "background",
        summary: input.summary,
        reasonCode: input.reasonCode,
      }),
    );
  }
  return input.producer.publish(
    workActivity({
      eventType: workEventType(input.phase),
      identity: input.identity,
      summary: input.summary,
      workRef: {
        ...input.workRef,
        run_id: input.workRef?.run_id ?? `${input.loopKind}:background`,
      },
      resultRef: input.resultRef,
      reasonCode: input.reasonCode ?? input.loopKind,
      visibility: "agent",
    }),
  );
}

function workEventType(
  phase: BackgroundGovernancePhase,
): "work_started" | "work_completed" | "work_failed" {
  switch (phase) {
    case "started":
      return "work_started";
    case "completed":
      return "work_completed";
    case "failed":
    case "degraded":
    case "recovered":
      return "work_failed";
  }
}
