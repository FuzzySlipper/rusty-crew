import type {
  AdapterId,
  AgentId,
  AgentMessage,
  BrainEvent,
  BodyState,
  CompletionPacket,
  CoreEvent,
  DelegatedResourceCleanupReport,
  DelegatedSessionRuntimeStatus,
  DenDataUpdate,
  ExternalEvent,
  FanOutFailurePolicy,
  ParentConsumptionPolicy,
  ProjectId,
  RunId,
  SessionId,
  SessionState,
  TaskId,
  ToolCallMetadata,
} from "@rusty-crew/contracts";

import { toSessionState, type RawSessionState } from "./session-wire.js";

export function toNativeBodyState(state: BodyState): unknown {
  return {
    session: toNativeSessionState(state.session),
    pending_messages: state.pendingMessages.map(toNativeAgentMessage),
    recent_events: state.recentEvents.map(toNativeCoreEvent),
    child_completions: state.childCompletions.map(toNativeDelegatedCompletion),
    fan_out_groups: state.fanOutGroups.map(toNativeDelegatedFanOutGroup),
    delta_policy: {
      mode: state.deltaPolicy.mode,
      queue_owner: state.deltaPolicy.queueOwner,
      queued_message_ttl_ms: state.deltaPolicy.queuedMessageTtlMs,
      max_queued_messages: state.deltaPolicy.maxQueuedMessages,
    },
  };
}

export function toBodyState(state: RawBodyState): BodyState {
  return {
    session: toSessionState(state.session),
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

export function toNativeSessionState(session: SessionState): unknown {
  return {
    handle: session.handle,
    session_id: session.sessionId,
    agent_id: session.agentId,
    profile_id: session.profileId,
    kind: session.kind,
    delegation: session.delegation
      ? {
          parent_session_id: session.delegation.parentSessionId,
          parent_agent_id: session.delegation.parentAgentId,
          source_wake_id: session.delegation.sourceWakeId,
          source_action_index: session.delegation.sourceActionIndex,
          requested_task_id: session.delegation.requestedTaskId,
          correlation_id: session.delegation.correlationId,
        }
      : undefined,
    resource_limits: {
      workdir: session.resourceLimits.workdir,
      max_duration_ms: session.resourceLimits.maxDurationMs,
      max_delegation_depth: session.resourceLimits.maxDelegationDepth,
    },
    tool_profile: {
      tools: session.toolProfile.tools.map((tool) => ({
        name: tool.name,
        description: tool.description,
        input_schema: tool.inputSchema,
      })),
    },
    history_window: session.historyWindow
      ? { max_messages: session.historyWindow.maxMessages }
      : undefined,
    inference_overrides: {
      reasoning_effort: session.inferenceOverrides?.reasoningEffort,
    },
    status: session.status,
    brain_turn_count: session.brainTurnCount,
    created_at: session.createdAt,
    last_active_at: session.lastActiveAt,
  };
}

export function toNativeAgentMessage(message: AgentMessage): RawAgentMessage {
  return {
    from: message.from,
    to: message.to,
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

export function toNativeCoreEvent(event: CoreEvent): unknown {
  switch (event.type) {
    case "session_created":
      return { type: event.type, state: toNativeSessionState(event.state) };
    case "session_archived":
      return { type: event.type, session_id: event.sessionId };
    case "agent_message_routed":
      return { type: event.type, message: toNativeAgentMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: {
          parent_session_id: event.lifecycle.parentSessionId,
          delegated_session_id: event.lifecycle.delegatedSessionId,
          run_id: event.lifecycle.runId,
          phase: event.lifecycle.phase,
          detail: event.lifecycle.detail,
        },
      };
    case "external_event_injected":
      return {
        type: event.type,
        event: toNativeExternalEvent(event.event),
      };
    case "den_data_updated":
      return { type: event.type, update: toNativeDenDataUpdate(event.update) };
    case "brain_wake_requested":
      return { type: event.type, session_id: event.sessionId };
    case "brain_event_observed":
      return {
        type: event.type,
        session_id: event.sessionId,
        wake_id: event.wakeId,
        event: toNativeBrainEventForJson(event.event),
      };
    case "brain_actions_accepted":
      return {
        type: event.type,
        session_id: event.sessionId,
        count: event.count,
      };
    case "completion_packet_delivered":
      return {
        type: event.type,
        packet: {
          session_id: event.packet.sessionId,
          status: event.packet.status,
          summary: event.packet.summary,
        },
      };
  }
}

export function toNativeBrainEventForJson(event: BrainEvent): unknown {
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
    case "phase_change":
      return {
        type: event.type,
        phase: event.phase,
        message: event.message,
      };
    case "tool_call_started":
      return {
        type: event.type,
        tool_name: event.toolName,
        metadata: event.metadata
          ? toRawToolCallMetadata(event.metadata)
          : undefined,
      };
    case "tool_call_finished":
      return {
        type: event.type,
        tool_name: event.toolName,
        is_error: event.isError,
        metadata: event.metadata
          ? toRawToolCallMetadata(event.metadata)
          : undefined,
      };
    case "provider_status":
      return {
        type: event.type,
        level: event.level,
        message: event.message,
        metadata_json: event.metadataJson,
      };
  }
}

export function toNativeDelegatedCompletion(
  completion: BodyState["childCompletions"][number],
): unknown {
  return {
    run_id: completion.runId,
    child_session_id: completion.childSessionId,
    requested_task_id: completion.requestedTaskId,
    source_wake_id: completion.sourceWakeId,
    source_action_index: completion.sourceActionIndex,
    correlation_id: completion.correlationId,
    parent_consumption: completion.parentConsumption,
    packet: {
      session_id: completion.packet.sessionId,
      status: completion.packet.status,
      summary: completion.packet.summary,
    },
  };
}

export function toDelegatedCompletion(
  completion: RawDelegatedCompletion,
): BodyState["childCompletions"][number] {
  return {
    runId: completion.run_id,
    childSessionId: completion.child_session_id,
    requestedTaskId: completion.requested_task_id,
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

export function toNativeDelegatedFanOutGroup(
  group: BodyState["fanOutGroups"][number],
): unknown {
  return {
    group_id: group.groupId,
    total: group.total,
    pending: group.pending,
    completed: group.completed,
    failed: group.failed,
    blocked: group.blocked,
    exhausted: group.exhausted,
    cancelled: group.cancelled,
    expired: group.expired,
    max_concurrency: group.maxConcurrency,
    failure_policy: group.failurePolicy,
    status: group.status,
  };
}

export function toDelegatedFanOutGroup(
  group: RawDelegatedFanOutGroup,
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

export function toNativeDenDataUpdate(update: DenDataUpdate): unknown {
  return {
    project_id: update.projectId,
    entity_kind: update.entityKind,
    entity_id: update.entityId,
    revision: update.revision,
  };
}

export function toNativeExternalEvent(event: ExternalEvent): unknown {
  return {
    adapter_id: event.adapterId,
    source: event.source,
    payload: toNativeExternalEventPayload(event.payload),
  };
}

export function toNativeExternalEventPayload(
  payload: ExternalEvent["payload"],
): unknown {
  switch (payload.type) {
    case "human_message":
      return payload;
    case "channel_message":
      return {
        type: payload.type,
        binding_id: payload.bindingId,
        correlation_id: payload.correlationId,
        idempotency_key: payload.idempotencyKey,
        provider: payload.provider,
        external_channel_id: payload.externalChannelId,
        external_thread_id: payload.externalThreadId,
        external_message_id: payload.externalMessageId,
        from: payload.from,
        text: payload.text,
        received_at: payload.receivedAt,
        expires_at: payload.expiresAt,
      };
    case "adapter_status":
      return payload;
    case "tool_catalog_changed":
      return {
        type: payload.type,
        catalog_id: payload.catalogId,
      };
    case "raw_json":
      return payload;
  }
}

export function toExternalEventPayload(
  payload: unknown,
): ExternalEvent["payload"] {
  const raw = payload as Record<string, unknown>;
  switch (raw["type"]) {
    case "channel_message":
      return {
        type: "channel_message",
        bindingId: raw["binding_id"] as string,
        correlationId: raw["correlation_id"] as string,
        idempotencyKey: raw["idempotency_key"] as string,
        provider: raw["provider"] as string,
        externalChannelId: raw["external_channel_id"] as string,
        externalThreadId: raw["external_thread_id"] as string | undefined,
        externalMessageId: raw["external_message_id"] as string | undefined,
        from: raw["from"] as string,
        text: raw["text"] as string,
        receivedAt: raw["received_at"] as string,
        expiresAt: raw["expires_at"] as string,
      };
    case "tool_catalog_changed":
      return {
        type: "tool_catalog_changed",
        catalogId: raw["catalog_id"] as string,
      };
    default:
      return payload as ExternalEvent["payload"];
  }
}

export function encodeJson(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

export function toCoreEvent(event: RawCoreEvent): CoreEvent {
  switch (event.type) {
    case "session_created":
      return { type: event.type, state: toSessionState(event.state) };
    case "session_archived":
      return { type: event.type, sessionId: event.session_id };
    case "agent_message_routed":
      return { type: event.type, message: toAgentMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: toDelegationLifecycleEvent(event.lifecycle),
      };
    case "external_event_injected":
      return {
        type: event.type,
        event: {
          adapterId: event.event.adapter_id,
          source: event.event.source,
          payload: toExternalEventPayload(event.event.payload),
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

export function toDelegationLifecycleEvent(
  lifecycle: RawDelegationLifecycleEvent,
): Extract<CoreEvent, { type: "delegation_lifecycle_observed" }>["lifecycle"] {
  return {
    parentSessionId: lifecycle.parent_session_id,
    delegatedSessionId: lifecycle.delegated_session_id,
    runId: lifecycle.run_id,
    phase: lifecycle.phase,
    detail: lifecycle.detail,
  };
}

export function toDelegatedSessionRuntimeStatus(
  status: RawDelegatedSessionRuntimeStatus,
): DelegatedSessionRuntimeStatus {
  return {
    session: toSessionState(status.session),
    parentSessionId: status.parent_session_id,
    runId: status.run_id,
    runStatus: status.run_status,
    terminal: status.terminal,
  };
}

export function toDelegatedResourceCleanupReport(
  report: RawDelegatedResourceCleanupReport,
): DelegatedResourceCleanupReport {
  return {
    cleanedAt: report.cleaned_at,
    terminalArchived: report.terminal_archived,
    orphanedArchived: report.orphaned_archived,
    expiredArchived: report.expired_archived,
    resourcesReleased: report.resources_released,
  };
}

export function toAgentMessage(message: RawAgentMessage): AgentMessage {
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

export function toBrainEvent(event: RawBrainEvent): BrainEvent {
  switch (event.type) {
    case "started":
    case "finished":
      return event;
    case "text_delta":
      return { type: event.type, text: event.text };
    case "reasoning_delta":
      return {
        type: event.type,
        text: event.text,
        format: event.format,
      };
    case "phase_change":
      return {
        type: event.type,
        phase: event.phase,
        message: event.message,
      };
    case "tool_call_started":
      return {
        type: event.type,
        toolName: event.tool_name,
        metadata: event.metadata
          ? toToolCallMetadata(event.metadata)
          : undefined,
      };
    case "tool_call_finished":
      return {
        type: event.type,
        toolName: event.tool_name,
        isError: event.is_error,
        metadata: event.metadata
          ? toToolCallMetadata(event.metadata)
          : undefined,
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

export function toNativeBrainEvent(event: BrainEvent): {
  eventType: string;
  text?: string;
  toolName?: string;
  isError?: boolean;
  metadataJson?: string;
} {
  switch (event.type) {
    case "started":
      return { eventType: event.type };
    case "text_delta":
      return { eventType: event.type, text: event.text };
    case "reasoning_delta":
      return {
        eventType: event.type,
        text: event.text,
        toolName: event.format ?? undefined,
      };
    case "phase_change":
      return {
        eventType: event.type,
        text: event.message ?? undefined,
        toolName: event.phase,
      };
    case "tool_call_started":
      return {
        eventType: event.type,
        toolName: event.toolName,
        metadataJson: event.metadata
          ? JSON.stringify(toRawToolCallMetadata(event.metadata))
          : undefined,
      };
    case "tool_call_finished":
      return {
        eventType: event.type,
        toolName: event.toolName,
        isError: event.isError,
        metadataJson: event.metadata
          ? JSON.stringify(toRawToolCallMetadata(event.metadata))
          : undefined,
      };
    case "provider_status":
      return {
        eventType: event.type,
        text: event.message,
        toolName: event.level,
        metadataJson: event.metadataJson ?? undefined,
      };
    case "finished":
      return { eventType: event.type };
  }
}

export function toToolCallMetadata(
  metadata: RawToolCallMetadata,
): ToolCallMetadata {
  return {
    source: metadata.source,
    adapterId: metadata.adapter_id as ToolCallMetadata["adapterId"],
    bindingId: metadata.binding_id,
    serverNames: metadata.server_names,
    profileId: metadata.profile_id as ToolCallMetadata["profileId"],
    toolProfileKey: metadata.tool_profile_key,
    sourceToolName: metadata.source_tool_name,
    catalogRevision: metadata.catalog_revision,
    debugDetailId: metadata.debug_detail_id,
    policy: metadata.policy
      ? {
          allowed: metadata.policy.allowed,
          denialReason: metadata.policy.denial_reason,
          timeoutMs: metadata.policy.timeout_ms,
          cancelled: metadata.policy.cancelled,
          archiveCleanup: metadata.policy.archive_cleanup,
        }
      : undefined,
  };
}

export function toRawToolCallMetadata(
  metadata: ToolCallMetadata,
): RawToolCallMetadata {
  return {
    source: metadata.source,
    adapter_id: metadata.adapterId ?? undefined,
    binding_id: metadata.bindingId ?? undefined,
    server_names: metadata.serverNames ?? [],
    profile_id: metadata.profileId ?? undefined,
    tool_profile_key: metadata.toolProfileKey ?? undefined,
    source_tool_name: metadata.sourceToolName ?? undefined,
    catalog_revision: metadata.catalogRevision ?? undefined,
    debug_detail_id: metadata.debugDetailId ?? undefined,
    policy: metadata.policy
      ? {
          allowed: metadata.policy.allowed ?? undefined,
          denial_reason: metadata.policy.denialReason ?? undefined,
          timeout_ms: metadata.policy.timeoutMs ?? undefined,
          cancelled: metadata.policy.cancelled ?? undefined,
          archive_cleanup: metadata.policy.archiveCleanup ?? undefined,
        }
      : undefined,
  };
}

export type RawCoreEvent =
  | { type: "session_created"; state: RawSessionState }
  | { type: "session_archived"; session_id: SessionId }
  | { type: "agent_message_routed"; message: RawAgentMessage }
  | {
      type: "delegation_lifecycle_observed";
      lifecycle: RawDelegationLifecycleEvent;
    }
  | {
      type: "external_event_injected";
      event: {
        adapter_id: AdapterId;
        source: string;
        payload: unknown;
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
  | { type: "brain_wake_requested"; session_id: SessionId }
  | {
      type: "brain_event_observed";
      session_id: SessionId;
      wake_id?: string;
      event: RawBrainEvent;
    }
  | {
      type: "brain_actions_accepted";
      session_id: SessionId;
      count: number;
    }
  | {
      type: "completion_packet_delivered";
      packet: {
        session_id: SessionId;
        status: Extract<
          CoreEvent,
          { type: "completion_packet_delivered" }
        >["packet"]["status"];
        summary: string;
      };
    };

export interface RawDelegationLifecycleEvent {
  parent_session_id: SessionId;
  delegated_session_id: SessionId;
  run_id?: RunId;
  phase: Extract<
    CoreEvent,
    { type: "delegation_lifecycle_observed" }
  >["lifecycle"]["phase"];
  detail?: string;
}

export interface RawDelegatedSessionRuntimeStatus {
  session: RawSessionState;
  parent_session_id?: SessionId;
  run_id?: RunId;
  run_status?: DelegatedSessionRuntimeStatus["runStatus"];
  terminal: boolean;
}

export interface RawDelegatedResourceCleanupReport {
  cleaned_at: string;
  terminal_archived: SessionId[];
  orphaned_archived: SessionId[];
  expired_archived: SessionId[];
  resources_released: number;
}

export interface RawBodyState {
  session: RawSessionState;
  pending_messages: RawAgentMessage[];
  recent_events: RawCoreEvent[];
  child_completions: RawDelegatedCompletion[];
  fan_out_groups: RawDelegatedFanOutGroup[];
  delta_policy: {
    mode: BodyState["deltaPolicy"]["mode"];
    queue_owner: BodyState["deltaPolicy"]["queueOwner"];
    queued_message_ttl_ms: number;
    max_queued_messages: number;
  };
}

export interface RawAgentMessage {
  from: AgentId;
  to: AgentId;
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

export interface RawToolProfile {
  tools: Array<{
    name: string;
    description: string;
    input_schema?: number;
  }>;
}

export interface RawDelegatedCompletion {
  run_id: RunId;
  child_session_id: SessionId;
  requested_task_id?: TaskId;
  source_wake_id: string;
  source_action_index: number;
  correlation_id?: string;
  parent_consumption: ParentConsumptionPolicy;
  packet: {
    session_id: SessionId;
    status: CompletionPacket["status"];
    summary: string;
  };
}

export interface RawDelegatedFanOutGroup {
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
  failure_policy: FanOutFailurePolicy;
  status: BodyState["fanOutGroups"][number]["status"];
}

export type RawBrainEvent =
  | { type: "started" }
  | { type: "text_delta"; text: string }
  | { type: "reasoning_delta"; text: string; format?: string }
  | {
      type: "phase_change";
      phase: "idle" | "exploring" | "composing" | "reviewing";
      message?: string;
    }
  | {
      type: "tool_call_started";
      tool_name: string;
      metadata?: RawToolCallMetadata;
    }
  | {
      type: "tool_call_finished";
      tool_name: string;
      is_error: boolean;
      metadata?: RawToolCallMetadata;
    }
  | {
      type: "provider_status";
      level: "info" | "degraded" | "error";
      message: string;
      metadata_json?: string;
    }
  | { type: "finished" };

export interface RawToolCallPolicyMetadata {
  allowed?: boolean;
  denial_reason?: string;
  timeout_ms?: number;
  cancelled?: boolean;
  archive_cleanup?: boolean;
}

export interface RawToolCallMetadata {
  source: ToolCallMetadata["source"];
  adapter_id?: string;
  binding_id?: string;
  server_names: string[];
  profile_id?: string;
  tool_profile_key?: string;
  source_tool_name?: string;
  catalog_revision?: string;
  debug_detail_id?: string;
  policy?: RawToolCallPolicyMetadata;
}
