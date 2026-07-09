import type {
  BrainEvent,
  CoreEvent,
  SessionState,
} from "@rusty-crew/contracts";
import type { SessionActivityDigest } from "@rusty-crew/contracts";
import type {
  DenSuccessorAgentIdentity,
  DenSuccessorGatewayClient,
} from "./service-adapter-ports.js";
import {
  AgentActivityObservationProducer,
  type AgentActivityObservationEvent,
  type AgentActivityObservationSink,
  type AgentActivityWorkRef,
} from "./agent-activity-observation.js";
import { createRuntimeActivityObserver } from "./runtime-activity-observer.js";
import { buildSessionActivityDigest } from "./session-activity-digest.js";
import { postTurnMaintenanceDecision } from "./post-turn-maintenance.js";
import {
  discoverCuratorCandidates,
  type CuratorCandidateBatch,
} from "./curator-candidates.js";
import type { CuratorMutationCandidate } from "./curator-mutations.js";
import type {
  ServiceWakeObservationContext,
  ServiceWakeSource,
  WakeProfileContext,
} from "./service-wake-dispatch.js";

export interface ServiceWakeMaintenanceEvent {
  source: string;
  eventType: string;
  summary: string;
  severity?: string;
  workRef?: Record<string, unknown>;
  resultRef?: Record<string, unknown>;
}

export interface ServiceWakeMaintenanceContext {
  denGatewayClient?: DenSuccessorGatewayClient;
  now(): string;
  saveSessionActivityDigest(digest: SessionActivityDigest): Promise<void>;
  upsertCuratorBatch(
    batch: CuratorCandidateBatch,
    mutations: readonly CuratorMutationCandidate[],
  ): void | Promise<void>;
  setCuratorLastRunAt(value: string): void;
  mutationForCuratorCandidate(
    candidate: CuratorCandidateBatch["candidates"][number],
  ): readonly CuratorMutationCandidate[];
  recordEvent(event: ServiceWakeMaintenanceEvent): void;
}

export async function publishWakeToolActivity(input: {
  context: ServiceWakeMaintenanceContext;
  session: SessionState;
  wakeId: string;
  events: readonly CoreEvent[];
  observationContext?: ServiceWakeObservationContext;
}): Promise<void> {
  if (input.context.denGatewayClient === undefined) return;
  const toolEvents = input.events.filter((event): event is ObservedToolEvent =>
    isObservedToolEvent(event, input.wakeId),
  );
  if (toolEvents.length === 0) return;

  const observer = createRuntimeActivityObserver({
    producer: new AgentActivityObservationProducer({
      sink: createDenGatewayObservationSink(input.context.denGatewayClient),
      required: true,
    }),
    identity: observationIdentityForSession(input.session),
    runtimeInstanceId: runtimeInstanceIdForSession(input.session),
  });
  const workRef = toolActivityWorkRef({
    sessionId: input.session.sessionId,
    wakeId: input.wakeId,
    observationContext: input.observationContext,
  });
  let degraded = 0;
  for (const event of toolEvents) {
    const toolEvent = event.event;
    const result = await observer.tool({
      eventType:
        toolEvent.type === "tool_call_started"
          ? "tool_call_started"
          : toolEvent.isError
            ? "tool_call_failed"
            : "tool_call_completed",
      toolName: toolEvent.toolName,
      adapter: "rusty-crew",
      visibility:
        input.observationContext?.channelId === undefined
          ? undefined
          : "channel",
      summary:
        toolEvent.type === "tool_call_started"
          ? `Tool ${toolEvent.toolName} started.`
          : toolEvent.isError
            ? `Tool ${toolEvent.toolName} failed.`
            : `Tool ${toolEvent.toolName} completed.`,
      longRunningOrRisky: true,
      workRef,
      resultRef:
        toolEvent.type === "tool_call_finished"
          ? {
              artifact_path: `runtime://tool/${toolEvent.toolName}/${input.wakeId}`,
            }
          : undefined,
      reasonCode:
        toolEvent.type === "tool_call_finished" && toolEvent.isError
          ? "tool_call_failed"
          : undefined,
    });
    if (result.status === "degraded") degraded += 1;
  }
  if (degraded > 0) {
    input.context.recordEvent({
      source: "den-successor-gateway",
      eventType: "den_observation_tool_activity_degraded",
      severity: "warning",
      summary: `Publishing ${degraded} tool Observation event(s) degraded for wake ${input.wakeId}.`,
    });
  }
}

export async function persistSessionActivityDigest(input: {
  context: ServiceWakeMaintenanceContext;
  session: SessionState;
  wakeId: string;
  source: ServiceWakeSource;
  observedEvents: readonly CoreEvent[];
  completionSummary?: string;
}): Promise<void> {
  try {
    const digest = buildSessionActivityDigest({
      profileId: input.session.profileId,
      sessionId: input.session.sessionId,
      wakeId: input.wakeId,
      source: input.source,
      events: input.observedEvents,
      completionSummary: input.completionSummary,
      now: input.context.now(),
    });
    await input.context.saveSessionActivityDigest(digest);
    input.context.recordEvent({
      source: "session-activity-digest",
      eventType: "session_activity_digest_saved",
      summary: `Saved activity digest ${digest.digest_id} for wake ${input.wakeId}.`,
    });
  } catch (error) {
    input.context.recordEvent({
      source: "session-activity-digest",
      eventType: "session_activity_digest_save_failed",
      severity: "warning",
      summary: errorMessage(error, "session activity digest save failed"),
    });
  }
}

export async function runPostTurnMaintenance(input: {
  context: ServiceWakeMaintenanceContext;
  session: SessionState;
  profileContext: WakeProfileContext;
  wakeId: string;
  source: ServiceWakeSource;
  observedEvents: readonly CoreEvent[];
  completionSummary?: string;
}): Promise<void> {
  const decision = postTurnMaintenanceDecision({
    profileId: input.session.profileId,
    wakeId: input.wakeId,
    source: input.source,
    backgroundReviewEnabled:
      input.profileContext.profile.backgroundReview?.enabled ?? false,
    events: input.observedEvents,
    completionSummary: input.completionSummary,
  });
  if (decision.action === "noop") {
    input.context.recordEvent({
      source: "post-turn-maintenance",
      eventType: "post_turn_auto_maintenance_noop",
      summary: `${decision.summary} for wake ${input.wakeId}.`,
    });
    return;
  }

  const batch = discoverCuratorCandidates({
    batchId: [
      "post-turn",
      input.session.profileId,
      input.wakeId.replace(/[^0-9A-Za-z_-]/g, ""),
    ].join(":"),
    now: input.context.now(),
    scopeType: "profile",
    scopeId: input.session.profileId,
    profileId: input.session.profileId,
    skills: input.profileContext.skills,
    expectedSkillSlugs:
      input.profileContext.profile.skillsMode === "all"
        ? []
        : input.profileContext.profile.skills,
    observedBehavior: [decision.evidence],
    maxCandidates: 1,
    dryRun: true,
  });
  await input.context.upsertCuratorBatch(
    batch,
    batch.candidates.flatMap((candidate) =>
      input.context.mutationForCuratorCandidate(candidate),
    ),
  );
  input.context.setCuratorLastRunAt(input.context.now());
  input.context.recordEvent({
    source: "post-turn-maintenance",
    eventType:
      batch.candidateCount > 0
        ? "post_turn_curator_candidate_created"
        : "post_turn_auto_maintenance_noop",
    summary:
      batch.candidateCount > 0
        ? `Post-turn maintenance proposed ${batch.candidateCount} curator candidate(s) for wake ${input.wakeId}.`
        : `Post-turn maintenance observed reusable behavior for wake ${input.wakeId}, but no new candidate was needed.`,
  });
}

export function createDenGatewayObservationSink(
  client: DenSuccessorGatewayClient,
): AgentActivityObservationSink {
  return {
    writeAgentActivity(event: AgentActivityObservationEvent): Promise<unknown> {
      return client.createObservationActivityEvent({
        source_domain: event.source_domain,
        event_type: event.event_type,
        agent_identity: event.agent_identity,
        runtime_instance_id: event.runtime_instance_id,
        payload: event.payload as unknown as Record<string, unknown>,
      });
    },
  };
}

type ObservedToolEvent = Extract<
  CoreEvent,
  { type: "brain_event_observed" }
> & {
  event: Extract<
    BrainEvent,
    { type: "tool_call_started" | "tool_call_finished" }
  >;
};

function isObservedToolEvent(
  event: CoreEvent,
  wakeId: string,
): event is ObservedToolEvent {
  return (
    event.type === "brain_event_observed" &&
    (event.wakeId === undefined || event.wakeId === wakeId) &&
    (event.event.type === "tool_call_started" ||
      event.event.type === "tool_call_finished")
  );
}

function observationIdentityForSession(
  session: SessionState,
): DenSuccessorAgentIdentity {
  return {
    profile: session.profileId,
    instance_id: runtimeInstanceIdForSession(session),
    session_key: session.sessionId,
  };
}

function runtimeInstanceIdForSession(
  session: Pick<SessionState, "agentId">,
): string {
  return `${session.agentId}@rusty-crew`;
}

function toolActivityWorkRef(input: {
  sessionId: string;
  wakeId: string;
  observationContext?: ServiceWakeObservationContext;
}): AgentActivityWorkRef {
  const deliveryIntentId = input.observationContext?.deliveryIntentId;
  return {
    session_id: input.sessionId,
    run_id:
      deliveryIntentId === undefined
        ? `wake:${input.wakeId}`
        : `delivery_intent:${deliveryIntentId};wake:${input.wakeId}`,
    channel_id: input.observationContext?.channelId,
    channel_message_id: input.observationContext?.channelMessageId,
  };
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
