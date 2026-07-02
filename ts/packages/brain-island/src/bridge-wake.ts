import type {
  AdapterId,
  AgentMessage,
  BodyState,
  BrainWakeRequest,
  CoreEvent,
  ExternalEventPayload,
  ProjectId,
  RunId,
  RuntimeBufferHandle,
  RuntimeBufferView,
  TaskId,
  Unit,
} from "@rusty-crew/contracts";
import type {
  BrainImplementation,
  BrainRoleAssembly,
  BrainWakeResult,
} from "./index.js";

export interface BridgeBufferClient {
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
}

export async function wakeBrainFromBridgeRequest(
  buffers: BridgeBufferClient,
  brain: BrainImplementation,
  request: BrainWakeRequest,
): Promise<BrainWakeResult> {
  const handles = [
    request.bodyState,
    request.systemPrompt,
    request.roleAssembly,
  ];
  let wakeFailed = false;

  try {
    const [bodyStateView, systemPromptView, roleAssemblyView] =
      await Promise.all([
        buffers.getBuffer(request.bodyState),
        buffers.getBuffer(request.systemPrompt),
        buffers.getBuffer(request.roleAssembly),
      ]);

    return await brain.wake({
      wakeId: request.wakeId,
      sessionId: request.sessionId,
      state: parseBodyStateBuffer(bodyStateView),
      systemPrompt: decodeBuffer(systemPromptView),
      roleAssembly: parseJsonBuffer<BrainRoleAssembly>(roleAssemblyView),
      providerState: request.providerState,
      providerStateAbsence: request.providerStateAbsence,
    });
  } catch (error) {
    wakeFailed = true;
    throw error;
  } finally {
    const releases = await Promise.allSettled(
      handles.map((handle) => buffers.releaseBuffer(handle)),
    );
    const failedRelease = releases.find(
      (result): result is PromiseRejectedResult => result.status === "rejected",
    );

    if (!wakeFailed && failedRelease) {
      throw failedRelease.reason;
    }
  }
}

function parseJsonBuffer<T>(view: RuntimeBufferView): T {
  return JSON.parse(decodeBuffer(view)) as T;
}

function parseBodyStateBuffer(view: RuntimeBufferView): BodyState {
  return toBodyState(JSON.parse(decodeBuffer(view)) as unknown);
}

function decodeBuffer(view: RuntimeBufferView): string {
  return new TextDecoder().decode(view.bytes);
}

function toBodyState(value: unknown): BodyState {
  const raw = value as Partial<RustBodyStateJson> & Partial<BodyState>;
  if (raw.session && "agentId" in raw.session) {
    return value as BodyState;
  }

  const state = value as RustBodyStateJson;
  return {
    session: {
      handle: state.session.handle as BodyState["session"]["handle"],
      sessionId: state.session.session_id,
      agentId: state.session.agent_id,
      profileId: state.session.profile_id,
      kind: state.session.kind,
      delegation: toDelegationLineage(state.session.delegation),
      resourceLimits: {
        workdir: state.session.resource_limits?.workdir,
        maxDurationMs: state.session.resource_limits?.max_duration_ms,
        maxDelegationDepth: state.session.resource_limits?.max_delegation_depth,
      },
      toolProfile: {
        tools: (state.session.tool_profile?.tools ?? []).map(toToolDescriptor),
      },
      historyWindow: state.session.history_window
        ? {
            maxMessages: state.session.history_window.max_messages,
          }
        : undefined,
      status: state.session.status,
      brainTurnCount: state.session.brain_turn_count,
      createdAt: state.session.created_at,
      lastActiveAt: state.session.last_active_at,
    },
    pendingMessages: state.pending_messages.map(toAgentMessage),
    recentEvents: state.recent_events.map(toCoreEvent),
    childCompletions: state.child_completions.map(toDelegatedCompletion),
    fanOutGroups: state.fan_out_groups.map(toDelegatedFanOutGroup),
    deltaPolicy: {
      mode: state.delta_policy.mode,
      queueOwner: state.delta_policy.queue_owner,
      queuedMessageTtlMs: state.delta_policy.queued_message_ttl_ms,
      maxQueuedMessages: state.delta_policy.max_queued_messages,
    },
  };
}

function toDelegatedCompletion(
  completion: RustDelegatedCompletionJson,
): BodyState["childCompletions"][number] {
  return {
    runId: completion.run_id as RunId,
    childSessionId: completion.child_session_id,
    requestedTaskId: completion.requested_task_id as TaskId | undefined,
    sourceWakeId: completion.source_wake_id,
    sourceActionIndex: completion.source_action_index,
    correlationId: completion.correlation_id,
    parentConsumption: completion.parent_consumption,
    packet: {
      sessionId: completion.packet.session_id,
      status: completion.packet.status,
      summary: completion.packet.summary,
    },
  };
}

function toDelegatedFanOutGroup(
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

function toAgentMessage(message: RustAgentMessageJson): AgentMessage {
  return {
    from: message.from,
    to: message.to,
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

function toCoreEvent(event: RustCoreEventJson): CoreEvent {
  switch (event.type) {
    case "session_created":
      return { type: event.type, state: toBodyStateSession(event.state) };
    case "session_archived":
      return { type: event.type, sessionId: event.session_id };
    case "agent_message_routed":
      return { type: event.type, message: toAgentMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: {
          parentSessionId: event.lifecycle.parent_session_id,
          delegatedSessionId: event.lifecycle.delegated_session_id,
          runId: event.lifecycle.run_id,
          phase: event.lifecycle.phase,
          detail: event.lifecycle.detail,
        },
      };
    case "external_event_injected":
      return {
        type: event.type,
        event: {
          adapterId: event.event.adapter_id,
          source: event.event.source,
          payload: event.event.payload,
        },
      };
    case "den_data_updated":
      return {
        type: event.type,
        update: {
          projectId: event.update.project_id,
          entityKind: event.update.entity_kind,
          entityId: event.update.entity_id,
          revision: event.update.revision,
        },
      };
    case "brain_wake_requested":
      return { type: event.type, sessionId: event.session_id };
    case "brain_event_observed":
      return {
        type: event.type,
        sessionId: event.session_id,
        wakeId: event.wake_id,
        event: toBrainEvent(event.event),
      };
    case "brain_actions_accepted":
      return {
        type: event.type,
        sessionId: event.session_id,
        count: event.count,
      };
    case "completion_packet_delivered":
      return {
        type: event.type,
        packet: {
          sessionId: event.packet.session_id,
          status: event.packet.status,
          summary: event.packet.summary,
        },
      };
  }
}

function toBodyStateSession(
  session: RustSessionStateJson,
): BodyState["session"] {
  return {
    handle: session.handle as BodyState["session"]["handle"],
    sessionId: session.session_id,
    agentId: session.agent_id,
    profileId: session.profile_id,
    kind: session.kind,
    delegation: toDelegationLineage(session.delegation),
    resourceLimits: {
      workdir: session.resource_limits?.workdir,
      maxDurationMs: session.resource_limits?.max_duration_ms,
      maxDelegationDepth: session.resource_limits?.max_delegation_depth,
    },
    toolProfile: {
      tools: (session.tool_profile?.tools ?? []).map(toToolDescriptor),
    },
    status: session.status,
    brainTurnCount: session.brain_turn_count,
    createdAt: session.created_at,
    lastActiveAt: session.last_active_at,
  };
}

function toToolDescriptor(
  tool: RustToolDescriptorJson,
): BodyState["session"]["toolProfile"]["tools"][number] {
  return {
    name: tool.name,
    description: tool.description,
    inputSchema: tool.input_schema,
  };
}

function toBrainEvent(
  event: RustBrainEventJson,
): Extract<CoreEvent, { type: "brain_event_observed" }>["event"] {
  switch (event.type) {
    case "started":
    case "finished":
      return { type: event.type };
    case "text_delta":
      return { type: event.type, text: event.text };
    case "reasoning_delta":
      return {
        type: event.type,
        text: event.text,
        format: event.format,
      };
    case "tool_call_started":
      return {
        type: event.type,
        toolName: event.tool_name,
        metadata: event.metadata,
      };
    case "tool_call_finished":
      return {
        type: event.type,
        toolName: event.tool_name,
        isError: event.is_error,
        metadata: event.metadata,
      };
    case "provider_status":
      return {
        type: event.type,
        level: event.level,
        message: event.message,
        metadataJson: event.metadata_json,
      };
  }
}

function toDelegationLineage(
  lineage: RustDelegationLineageJson | undefined,
): BodyState["session"]["delegation"] {
  return lineage
    ? {
        parentSessionId: lineage.parent_session_id,
        parentAgentId: lineage.parent_agent_id,
        sourceWakeId: lineage.source_wake_id,
        sourceActionIndex: lineage.source_action_index,
        requestedTaskId: lineage.requested_task_id,
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
  delta_policy: {
    mode: "frozen_snapshot_next_wake";
    queue_owner: "body";
    queued_message_ttl_ms: number;
    max_queued_messages: number;
  };
}

interface RustSessionStateJson {
  handle: number;
  session_id: BodyState["session"]["sessionId"];
  agent_id: BodyState["session"]["agentId"];
  profile_id: BodyState["session"]["profileId"];
  kind: BodyState["session"]["kind"];
  delegation?: RustDelegationLineageJson;
  resource_limits?: {
    workdir?: string;
    max_duration_ms?: number;
    max_delegation_depth?: number;
  };
  tool_profile?: {
    tools: RustToolDescriptorJson[];
  };
  history_window?: {
    max_messages?: number;
  };
  status: BodyState["session"]["status"];
  brain_turn_count: number;
  created_at: string;
  last_active_at: string;
}

interface RustDelegationLineageJson {
  parent_session_id: BodyState["session"]["sessionId"];
  parent_agent_id: BodyState["session"]["agentId"];
  source_wake_id: string;
  source_action_index: number;
  requested_task_id?: TaskId;
  correlation_id: string;
}

interface RustDelegatedCompletionJson {
  run_id: string;
  child_session_id: BodyState["session"]["sessionId"];
  requested_task_id?: TaskId;
  source_wake_id: string;
  source_action_index: number;
  correlation_id?: string;
  parent_consumption: "await_completion" | "observe_only";
  packet: {
    session_id: BodyState["session"]["sessionId"];
    status: BodyState["childCompletions"][number]["packet"]["status"];
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
  from: AgentMessage["from"];
  to: AgentMessage["to"];
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

type RustCoreEventJson =
  | { type: "session_created"; state: RustSessionStateJson }
  | { type: "session_archived"; session_id: BodyState["session"]["sessionId"] }
  | { type: "agent_message_routed"; message: RustAgentMessageJson }
  | {
      type: "delegation_lifecycle_observed";
      lifecycle: {
        parent_session_id: BodyState["session"]["sessionId"];
        delegated_session_id: BodyState["session"]["sessionId"];
        run_id?: BodyState["childCompletions"][number]["runId"];
        phase: Extract<
          CoreEvent,
          { type: "delegation_lifecycle_observed" }
        >["lifecycle"]["phase"];
        detail?: string;
      };
    }
  | {
      type: "external_event_injected";
      event: {
        adapter_id: AdapterId;
        source: string;
        payload: ExternalEventPayload;
      };
    }
  | {
      type: "den_data_updated";
      update: {
        project_id: ProjectId;
        entity_kind: string;
        entity_id: string;
        revision?: string;
      };
    }
  | {
      type: "brain_wake_requested";
      session_id: BodyState["session"]["sessionId"];
    }
  | {
      type: "brain_event_observed";
      session_id: BodyState["session"]["sessionId"];
      wake_id?: string;
      event: RustBrainEventJson;
    }
  | {
      type: "brain_actions_accepted";
      session_id: BodyState["session"]["sessionId"];
      count: number;
    }
  | {
      type: "completion_packet_delivered";
      packet: {
        session_id: BodyState["session"]["sessionId"];
        status: Extract<
          CoreEvent,
          { type: "completion_packet_delivered" }
        >["packet"]["status"];
        summary: string;
      };
    };

interface RustToolDescriptorJson {
  name: string;
  description: string;
  input_schema?: BodyState["session"]["toolProfile"]["tools"][number]["inputSchema"];
}

type ToolEventMetadata = Extract<
  Extract<CoreEvent, { type: "brain_event_observed" }>["event"],
  { type: "tool_call_started" }
>["metadata"];

type RustBrainEventJson =
  | { type: "started" }
  | { type: "finished" }
  | { type: "text_delta"; text: string }
  | { type: "reasoning_delta"; text: string; format?: string }
  | {
      type: "tool_call_started";
      tool_name: string;
      metadata?: ToolEventMetadata;
    }
  | {
      type: "tool_call_finished";
      tool_name: string;
      is_error: boolean;
      metadata?: ToolEventMetadata;
    }
  | {
      type: "provider_status";
      level: Extract<
        Extract<CoreEvent, { type: "brain_event_observed" }>["event"],
        { type: "provider_status" }
      >["level"];
      message: string;
      metadata_json?: string;
    };
