import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AdapterId,
  AgentId,
  AgentMessage,
  BodyState,
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  CompletionPacket,
  CoreEvent,
  ProfileId,
  RunId,
  SessionHandle,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";
import {
  createDenAdapter,
  createMemoryDenProjectionSink,
} from "@rusty-crew/adapter-den";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createDenRouterPiAgentFactory,
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
} from "./index.js";

const decoder = new TextDecoder();
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-delegated-slice-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-19T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const createAgent = await createDenRouterPiAgentFactory({
    modelId: process.env.RUSTY_CREW_DEN_ROUTER_MODEL,
    maxTokens: Number.parseInt(
      process.env.RUSTY_CREW_DEN_ROUTER_MAX_TOKENS ?? "64",
      10,
    ),
  });
  const plannerSessionId = "planner-session" as SessionId;
  const plannerAgentId = "planner" as AgentId;
  const plannerWakeId = "planner-wake-1";
  const workerWakeId = "worker-wake-1";
  const delegatedSessionId =
    `${plannerSessionId}:delegated:${plannerWakeId}:0` as SessionId;
  const delegatedProfileId = "coder-profile" as ProfileId;

  await native.createSession({
    sessionId: plannerSessionId,
    agentId: plannerAgentId,
    profileId: "planner-profile",
    kind: "full",
  });
  await native.routeAgentMessage(
    "operator",
    plannerAgentId,
    "Delegate one tiny worker slice and ask the worker to report completion.",
  );

  const plannerBrain = createPiAgentBrain({
    createAgent,
    planActions: (): BrainAction[] => [
      {
        type: "request_delegation",
        profileId: delegatedProfileId,
        taskId: "2772" as TaskId,
        prompt:
          "You are the delegated worker. Complete the tiny local slice and report success.",
      },
    ],
  });
  const plannerBody = await projectBodyState(plannerSessionId);
  const plannerResult = await plannerBrain.wake({
    wakeId: plannerWakeId,
    sessionId: plannerSessionId,
    systemPrompt:
      "You are the planner in a local Rusty Crew smoke. Reply briefly before delegating.",
    roleAssembly: {
      instructions:
        "A deterministic action planner will convert this wake into a request_delegation action.",
    },
    state: plannerBody,
  });

  await submitEvents(plannerResult.events);
  const plannerReceipt = await native.submitBrainActionsJson(
    plannerWakeId,
    plannerSessionId,
    plannerResult.actions,
  );
  assert.equal(plannerReceipt.acceptedActions, 1);

  const delegatedBody = await projectBodyState(delegatedSessionId);
  assert.equal(delegatedBody.session.kind, "delegated");
  assert.equal(delegatedBody.session.profileId, delegatedProfileId);
  assert.equal(delegatedBody.pendingMessages.length, 1);
  assert.match(delegatedBody.pendingMessages[0]!.body, /delegated worker/i);
  assert(
    delegatedBody.recentEvents.some(
      (event) =>
        event.type === "brain_wake_requested" &&
        event.sessionId === delegatedSessionId,
    ),
  );

  const workerBrain = createPiAgentBrain({
    createAgent,
    planActions: ({ wake }): BrainAction[] => [
      {
        type: "deliver_completion",
        packet: {
          sessionId: wake.sessionId,
          status: "completed",
          summary: "delegated worker completed the local smoke slice",
        } satisfies CompletionPacket,
      },
    ],
  });
  const workerResult = await workerBrain.wake({
    wakeId: workerWakeId,
    sessionId: delegatedSessionId,
    systemPrompt:
      "You are the delegated worker in a local Rusty Crew smoke. Reply briefly and finish.",
    roleAssembly: {
      instructions:
        "A deterministic action planner will convert this wake into a completion packet.",
    },
    state: delegatedBody,
  });

  await submitEvents(workerResult.events);
  const workerReceipt = await native.submitBrainActionsJson(
    workerWakeId,
    delegatedSessionId,
    workerResult.actions,
  );
  assert.equal(workerReceipt.acceptedActions, 1);

  const completedBody = await projectBodyState(delegatedSessionId);
  const completionEvent = completedBody.recentEvents.find(
    (
      event,
    ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
      event.type === "completion_packet_delivered" &&
      event.packet.sessionId === delegatedSessionId,
  );
  assert(
    completionEvent,
    "completion packet should be routed back through Rust",
  );

  const projectionSink = createMemoryDenProjectionSink();
  const denAdapter = createDenAdapter({
    adapterId: "den" as AdapterId,
    ingress: {
      injectDenDataUpdate: native.injectDenDataUpdate,
      injectExternalEvent: native.injectExternalEvent,
    },
    projectionSink,
  });
  const projection = await denAdapter.projectEvent(completionEvent);
  assert.equal(projection.accepted, true);
  assert.equal(projectionSink.projections.length, 1);

  const counts = {
    sessions: await native.countRows("sessions"),
    workerRuns: await native.countRows("worker_runs"),
    completionPackets: await native.countRows("completion_packets"),
  };
  assert.equal(counts.sessions, 2);
  assert.equal(counts.workerRuns, 1);
  assert.equal(counts.completionPackets, 1);

  console.log(
    JSON.stringify(
      {
        plannerEvents: plannerResult.events.map((event) => event.event.type),
        plannerActions: plannerResult.actions.map((action) => action.type),
        delegatedSessionId,
        workerEvents: workerResult.events.map((event) => event.event.type),
        workerActions: workerResult.actions.map((action) => action.type),
        completionSummary: completionEvent.packet.summary,
        denProjections: projectionSink.projections.map((item) => item.summary),
        counts,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

async function projectBodyState(sessionId: SessionId): Promise<BodyState> {
  const raw = JSON.parse(
    decoder.decode(await native.projectBodyStateJson(sessionId)),
  ) as RustBodyStateJson;
  return {
    session: {
      handle: raw.session.handle as SessionHandle,
      sessionId: raw.session.session_id as SessionId,
      agentId: raw.session.agent_id as AgentId,
      profileId: raw.session.profile_id as ProfileId,
      kind: raw.session.kind,
      delegation: toTsDelegationLineage(raw.session.delegation),
      resourceLimits: {
        workdir: raw.session.resource_limits?.workdir,
        maxDurationMs: raw.session.resource_limits?.max_duration_ms,
        maxDelegationDepth: raw.session.resource_limits?.max_delegation_depth,
      },
      toolProfile: {
        tools: raw.session.tool_profile?.tools ?? [],
      },
      status: raw.session.status,
      brainTurnCount: raw.session.brain_turn_count,
      createdAt: raw.session.created_at,
      lastActiveAt: raw.session.last_active_at,
    },
    pendingMessages: raw.pending_messages.map(toTsMessage),
    recentEvents: raw.recent_events.map(toTsEvent),
    childCompletions: raw.child_completions.map(toTsDelegatedCompletion),
    fanOutGroups: raw.fan_out_groups.map(toTsDelegatedFanOutGroup),
    deltaPolicy: raw.delta_policy
      ? {
          mode: raw.delta_policy.mode,
          queueOwner: raw.delta_policy.queue_owner,
          queuedMessageTtlMs: raw.delta_policy.queued_message_ttl_ms,
          maxQueuedMessages: raw.delta_policy.max_queued_messages,
        }
      : defaultBodyDeltaPolicy,
  };
}

function toTsDelegatedCompletion(
  completion: RustDelegatedCompletionJson,
): BodyState["childCompletions"][number] {
  return {
    runId: completion.run_id as RunId,
    childSessionId: completion.child_session_id as SessionId,
    requestedTaskId: completion.requested_task_id as TaskId | undefined,
    sourceWakeId: completion.source_wake_id,
    sourceActionIndex: completion.source_action_index,
    correlationId: completion.correlation_id,
    parentConsumption: completion.parent_consumption,
    packet: {
      sessionId: completion.packet.session_id as SessionId,
      status: completion.packet.status,
      summary: completion.packet.summary,
    },
  };
}

function toTsDelegatedFanOutGroup(
  group: RustDelegatedFanOutGroupJson,
): BodyState["fanOutGroups"][number] {
  return {
    groupId: group.group_id,
    total: group.total,
    pending: group.pending,
    completed: group.completed,
    failed: group.failed,
    blocked: group.blocked,
    exhausted: group.exhausted,
    cancelled: group.cancelled,
    expired: group.expired,
    maxConcurrency: group.max_concurrency,
    failurePolicy: group.failure_policy,
    status: group.status,
  };
}

async function submitEvents(events: BrainEventEnvelope[]): Promise<void> {
  for (const event of events) {
    await native.submitBrainEvent(event);
  }
}

function toTsMessage(message: RustAgentMessageJson): AgentMessage {
  return {
    from: message.from as AgentId,
    to: message.to as AgentId,
    body: message.body,
    correlationId: message.correlation_id,
  };
}

function toTsEvent(event: RustCoreEventJson): CoreEvent {
  switch (event.type) {
    case "session_created":
      return {
        type: event.type,
        state: {
          handle: event.state.handle as SessionHandle,
          sessionId: event.state.session_id as SessionId,
          agentId: event.state.agent_id as AgentId,
          profileId: event.state.profile_id as ProfileId,
          kind: event.state.kind,
          delegation: toTsDelegationLineage(event.state.delegation),
          resourceLimits: {
            workdir: event.state.resource_limits?.workdir,
            maxDurationMs: event.state.resource_limits?.max_duration_ms,
            maxDelegationDepth:
              event.state.resource_limits?.max_delegation_depth,
          },
          toolProfile: { tools: event.state.tool_profile?.tools ?? [] },
          status: event.state.status,
          brainTurnCount: event.state.brain_turn_count,
          createdAt: event.state.created_at,
          lastActiveAt: event.state.last_active_at,
        },
      };
    case "agent_message_routed":
      return { type: event.type, message: toTsMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: {
          parentSessionId: event.lifecycle.parent_session_id as SessionId,
          delegatedSessionId: event.lifecycle.delegated_session_id as SessionId,
          runId: event.lifecycle.run_id as
            | BodyState["childCompletions"][number]["runId"]
            | undefined,
          phase: event.lifecycle.phase,
          detail: event.lifecycle.detail,
        },
      };
    case "brain_wake_requested":
      return { type: event.type, sessionId: event.session_id as SessionId };
    case "brain_event_observed":
      return {
        type: event.type,
        sessionId: event.session_id as SessionId,
        event: toTsBrainEvent(event.event),
      };
    case "brain_actions_accepted":
      return {
        type: event.type,
        sessionId: event.session_id as SessionId,
        count: event.count,
      };
    case "completion_packet_delivered":
      return {
        type: event.type,
        packet: {
          sessionId: event.packet.session_id as SessionId,
          status: event.packet.status,
          summary: event.packet.summary,
        },
      };
  }
}

function toTsBrainEvent(event: RustBrainEventJson): BrainEvent {
  switch (event.type) {
    case "started":
      return { type: event.type };
    case "text_delta":
      return { type: event.type, text: event.text };
    case "tool_call_started":
      return { type: event.type, toolName: event.tool_name };
    case "tool_call_finished":
      return {
        type: event.type,
        toolName: event.tool_name,
        isError: event.is_error,
      };
    case "finished":
      return { type: event.type };
  }
}

function toTsDelegationLineage(
  lineage: RustDelegationLineageJson | undefined,
): BodyState["session"]["delegation"] {
  return lineage
    ? {
        parentSessionId: lineage.parent_session_id as SessionId,
        parentAgentId: lineage.parent_agent_id as AgentId,
        sourceWakeId: lineage.source_wake_id,
        sourceActionIndex: lineage.source_action_index,
        requestedTaskId: lineage.requested_task_id as TaskId | undefined,
        correlationId: lineage.correlation_id,
      }
    : undefined;
}

interface RustBodyStateJson {
  session: RustSessionStateJson;
  pending_messages: RustAgentMessageJson[];
  recent_events: RustCoreEventJson[];
  child_completions: RustDelegatedCompletionJson[];
  fan_out_groups: RustDelegatedFanOutGroupJson[];
  delta_policy?: {
    mode: "frozen_snapshot_next_wake";
    queue_owner: "body";
    queued_message_ttl_ms: number;
    max_queued_messages: number;
  };
}

interface RustSessionStateJson {
  handle: number;
  session_id: string;
  agent_id: string;
  profile_id: string;
  kind: "full" | "worker" | "delegated";
  delegation?: RustDelegationLineageJson;
  resource_limits?: {
    workdir?: string;
    max_duration_ms?: number;
    max_delegation_depth?: number;
  };
  tool_profile?: { tools: [] };
  status: "active" | "idle" | "archived";
  brain_turn_count: number;
  created_at: string;
  last_active_at: string;
}

interface RustDelegationLineageJson {
  parent_session_id: string;
  parent_agent_id: string;
  source_wake_id: string;
  source_action_index: number;
  requested_task_id?: string;
  correlation_id: string;
}

interface RustDelegatedCompletionJson {
  run_id: string;
  child_session_id: string;
  requested_task_id?: string;
  source_wake_id: string;
  source_action_index: number;
  correlation_id?: string;
  parent_consumption: "await_completion" | "observe_only";
  packet: {
    session_id: string;
    status: "completed" | "failed" | "blocked" | "exhausted";
    summary: string;
  };
}

interface RustDelegatedFanOutGroupJson {
  group_id: string;
  total: number;
  pending: number;
  completed: number;
  failed: number;
  blocked: number;
  exhausted: number;
  cancelled: number;
  expired: number;
  max_concurrency?: number;
  failure_policy: BodyState["fanOutGroups"][number]["failurePolicy"];
  status: BodyState["fanOutGroups"][number]["status"];
}

interface RustAgentMessageJson {
  from: string;
  to: string;
  body: string;
  correlation_id?: string;
}

type RustCoreEventJson =
  | { type: "session_created"; state: RustSessionStateJson }
  | { type: "agent_message_routed"; message: RustAgentMessageJson }
  | {
      type: "delegation_lifecycle_observed";
      lifecycle: {
        parent_session_id: string;
        delegated_session_id: string;
        run_id?: string;
        phase: Extract<
          CoreEvent,
          { type: "delegation_lifecycle_observed" }
        >["lifecycle"]["phase"];
        detail?: string;
      };
    }
  | { type: "brain_wake_requested"; session_id: string }
  | {
      type: "brain_event_observed";
      session_id: string;
      event: RustBrainEventJson;
    }
  | { type: "brain_actions_accepted"; session_id: string; count: number }
  | {
      type: "completion_packet_delivered";
      packet: {
        session_id: string;
        status: "completed" | "failed" | "blocked" | "exhausted";
        summary: string;
      };
    };

type RustBrainEventJson =
  | { type: "started" }
  | { type: "text_delta"; text: string }
  | { type: "tool_call_started"; tool_name: string }
  | { type: "tool_call_finished"; tool_name: string; is_error: boolean }
  | { type: "finished" };
