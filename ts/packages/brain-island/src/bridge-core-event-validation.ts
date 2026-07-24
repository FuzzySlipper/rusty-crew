type JsonRecord = Record<string, unknown>;

const sessionKinds = ["full", "worker", "delegated"] as const;
const sessionStatuses = ["active", "idle", "archived"] as const;
const deliveryStatuses = [
  "pending",
  "accepted",
  "rejected",
  "expired",
] as const;
const roundStatuses = [
  "pending",
  "replied",
  "expired",
  "cancelled",
  "failed",
] as const;
const completionStatuses = [
  "completed",
  "failed",
  "blocked",
  "exhausted",
] as const;
const delegationPhases = [
  "created",
  "wake_requested",
  "checkpoint_requested",
  "completed",
  "failed",
  "blocked",
  "exhausted",
  "timed_out",
  "cancelled",
] as const;

export function rustCoreEventValidationError(
  value: unknown,
): string | undefined {
  if (!isRecord(value) || !hasString(value, "type")) {
    return "missing event object or type";
  }
  switch (value.type) {
    case "session_created":
      return isSessionState(value.state)
        ? undefined
        : "invalid session_created.state";
    case "session_archived":
      return hasString(value, "session_id")
        ? undefined
        : "invalid session_archived.session_id";
    case "agent_message_routed":
      return isAgentMessage(value.message)
        ? undefined
        : "invalid agent_message_routed.message";
    case "agent_message_delivery_observed":
      return isDeliveryReceipt(value.receipt)
        ? undefined
        : "invalid agent_message_delivery_observed.receipt";
    case "agent_round_observed":
      return isCorrelatedRound(value.round)
        ? undefined
        : "invalid agent_round_observed.round";
    case "delegation_lifecycle_observed":
      return isDelegationLifecycle(value.lifecycle)
        ? undefined
        : "invalid delegation_lifecycle_observed.lifecycle";
    case "external_event_injected":
      return isExternalEvent(value.event)
        ? undefined
        : "invalid external_event_injected.event";
    case "den_data_updated":
      return isDenDataUpdate(value.update)
        ? undefined
        : "invalid den_data_updated.update";
    case "brain_wake_requested":
      return hasString(value, "session_id")
        ? undefined
        : "invalid brain_wake_requested.session_id";
    case "brain_event_observed":
      return hasString(value, "session_id") &&
        nullableString(value.wake_id) &&
        isBrainEvent(value.event)
        ? undefined
        : "invalid brain_event_observed payload";
    case "brain_actions_accepted":
      return hasString(value, "session_id") && nonNegativeInteger(value.count)
        ? undefined
        : "invalid brain_actions_accepted payload";
    case "completion_packet_delivered":
      return isCompletionPacket(value.packet)
        ? undefined
        : "invalid completion_packet_delivered.packet";
    default:
      return `unsupported type ${value.type}`;
  }
}

function isSessionState(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return (
    nonNegativeInteger(value.handle) &&
    strings(value, [
      "session_id",
      "agent_id",
      "profile_id",
      "created_at",
      "last_active_at",
    ]) &&
    member(value.kind, sessionKinds) &&
    member(value.status, sessionStatuses) &&
    nonNegativeInteger(value.brain_turn_count) &&
    isDelegationLineage(value.delegation) &&
    isResourceLimits(value.resource_limits) &&
    isToolProfile(value.tool_profile) &&
    isInferenceOverrides(value.inference_overrides)
  );
}

function isDelegationLineage(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  return (
    isRecord(value) &&
    strings(value, [
      "parent_session_id",
      "parent_agent_id",
      "source_wake_id",
      "correlation_id",
    ]) &&
    nonNegativeInteger(value.source_action_index) &&
    nullableString(value.requested_task_id)
  );
}

function isResourceLimits(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  return (
    isRecord(value) &&
    nullableString(value.workdir) &&
    nullableNonNegativeNumber(value.max_duration_ms) &&
    nullableNonNegativeInteger(value.max_delegation_depth)
  );
}

function isToolProfile(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  return (
    isRecord(value) &&
    Array.isArray(value.tools) &&
    value.tools.every(
      (tool) =>
        isRecord(tool) &&
        strings(tool, ["name", "description"]) &&
        nullableNonNegativeNumber(tool.input_schema),
    )
  );
}

function isInferenceOverrides(value: unknown): boolean {
  return (
    value === undefined ||
    value === null ||
    (isRecord(value) && nullableString(value.reasoning_effort))
  );
}

function isAgentMessage(value: unknown): boolean {
  if (!isRecord(value) || !strings(value, ["from", "to", "body"])) return false;
  if (!nullableString(value.correlation_id)) return false;
  if (value.projection === undefined || value.projection === null) return true;
  if (!isRecord(value.projection)) return false;
  return (
    member(value.projection.visibility, [
      "observation",
      "user_visible",
    ] as const) &&
    isProjectionRef(value.projection.target_ref) &&
    isProjectionRef(value.projection.work_ref) &&
    nullableString(value.projection.reason)
  );
}

function isProjectionRef(value: unknown): boolean {
  return (
    value === undefined ||
    value === null ||
    (isRecord(value) && strings(value, ["system", "kind", "id"]))
  );
}

function isDeliveryReceipt(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return (
    isDeliveryRequest(value.request) &&
    member(value.status, deliveryStatuses) &&
    nonNegativeInteger(value.revision) &&
    nullableNonNegativeInteger(value.sequence) &&
    nullableString(value.reasonCode) &&
    nullableString(value.resolvedRoundId) &&
    nullableString(value.terminalAt) &&
    isActivation(value.activation)
  );
}

function isDeliveryRequest(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return (
    strings(value, [
      "deliveryId",
      "idempotencyKey",
      "messageId",
      "fromAgentId",
      "requestedAddress",
      "toAgentId",
      "body",
      "createdAt",
      "expiresAt",
    ]) &&
    member(value.inputKind, ["operator", "routed_agent_message"] as const) &&
    typeof value.requireWake === "boolean" &&
    nullableString(value.fromSessionId) &&
    nullableString(value.toSessionId) &&
    nullableString(value.replyToMessageId) &&
    nullableString(value.correlationId) &&
    (value.collaborationMode === undefined ||
      value.collaborationMode === null ||
      value.collaborationMode === "plan") &&
    isRouteProvenance(value.routing)
  );
}

function isRouteProvenance(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  if (!isRecord(value) || !strings(value, ["address", "routeKey"]))
    return false;
  if (
    !nonNegativeInteger(value.routeRevision) ||
    !isRecord(value.resolvedTarget)
  )
    return false;
  return (
    strings(value.resolvedTarget, [
      "agentId",
      "sessionId",
      "profileId",
      "displayLabel",
    ]) &&
    member(value.resolvedTarget.runtimeKind, [
      "direct_brain",
      "codex_app_server",
    ] as const)
  );
}

function isActivation(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  if (!isRecord(value) || !hasString(value, "type")) return false;
  switch (value.type) {
    case "direct_brain_wake_requested":
      return strings(value, ["sessionId", "wakeId"]);
    case "external_turn_requested":
      return strings(value, ["sessionId", "requestId", "bindingId"]);
    case "external_turn_steer_requested":
      return strings(value, [
        "sessionId",
        "requestId",
        "bindingId",
        "nativeThreadId",
        "nativeTurnId",
        "messageText",
      ]);
    case "queued_for_next_turn":
      return strings(value, ["sessionId", "queueId"]);
    case "rejected":
      return hasString(value, "reasonCode");
    default:
      return false;
  }
}

function isCorrelatedRound(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return (
    strings(value, [
      "roundId",
      "idempotencyKey",
      "senderAgentId",
      "recipientAgentId",
      "recipientSessionId",
      "messageId",
      "correlationId",
      "createdAt",
      "expiresAt",
    ]) &&
    member(value.status, roundStatuses) &&
    nonNegativeInteger(value.revision) &&
    nullableString(value.senderSessionId) &&
    nullableString(value.senderRequestId) &&
    nullableString(value.replyMessageId) &&
    nullableString(value.terminalAt) &&
    nullableString(value.terminalReasonCode)
  );
}

function isDelegationLifecycle(value: unknown): boolean {
  return (
    isRecord(value) &&
    strings(value, ["parent_session_id", "delegated_session_id"]) &&
    member(value.phase, delegationPhases) &&
    nullableString(value.run_id) &&
    nullableString(value.detail)
  );
}

function isExternalEvent(value: unknown): boolean {
  return (
    isRecord(value) &&
    strings(value, ["adapter_id", "source"]) &&
    isExternalPayload(value.payload)
  );
}

function isExternalPayload(value: unknown): boolean {
  if (!isRecord(value) || !hasString(value, "type")) return false;
  switch (value.type) {
    case "human_message":
      return strings(value, ["from", "text"]);
    case "channel_message":
      return (
        strings(value, [
          "binding_id",
          "correlation_id",
          "idempotency_key",
          "provider",
          "external_channel_id",
          "from",
          "text",
          "received_at",
          "expires_at",
        ]) &&
        nullableString(value.external_thread_id) &&
        nullableString(value.external_message_id)
      );
    case "adapter_status":
      return hasString(value, "status") && nullableString(value.detail);
    case "tool_catalog_changed":
      return hasString(value, "catalog_id");
    case "raw_json":
      return hasString(value, "json");
    default:
      return false;
  }
}

function isDenDataUpdate(value: unknown): boolean {
  return (
    isRecord(value) &&
    strings(value, ["project_id", "entity_kind", "entity_id"]) &&
    nullableString(value.revision)
  );
}

function isBrainEvent(value: unknown): boolean {
  if (!isRecord(value) || !hasString(value, "type")) return false;
  switch (value.type) {
    case "started":
    case "finished":
      return true;
    case "text_delta":
      return hasString(value, "text");
    case "reasoning_delta":
      return hasString(value, "text") && nullableString(value.format);
    case "phase_change":
      return (
        member(value.phase, [
          "idle",
          "exploring",
          "composing",
          "reviewing",
        ] as const) && nullableString(value.message)
      );
    case "tool_call_started":
      return hasString(value, "tool_name") && isToolMetadata(value.metadata);
    case "tool_call_finished":
      return (
        hasString(value, "tool_name") &&
        typeof value.is_error === "boolean" &&
        isToolMetadata(value.metadata)
      );
    case "provider_status":
      return (
        member(value.level, ["info", "degraded", "error"] as const) &&
        hasString(value, "message") &&
        nullableString(value.metadata_json)
      );
    default:
      return false;
  }
}

function isToolMetadata(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  if (!isRecord(value)) return false;
  return (
    member(value.source, ["local", "mcp", "web", "browser"] as const) &&
    Array.isArray(value.server_names) &&
    value.server_names.every((name) => typeof name === "string") &&
    [
      "adapter_id",
      "binding_id",
      "profile_id",
      "tool_profile_key",
      "source_tool_name",
      "catalog_revision",
      "debug_detail_id",
    ].every((key) => nullableString(value[key])) &&
    isToolPolicy(value.policy)
  );
}

function isToolPolicy(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  return (
    isRecord(value) &&
    nullableBoolean(value.allowed) &&
    nullableString(value.denial_reason) &&
    nullableNonNegativeNumber(value.timeout_ms) &&
    nullableBoolean(value.cancelled) &&
    nullableBoolean(value.archive_cleanup)
  );
}

function isCompletionPacket(value: unknown): boolean {
  return (
    isRecord(value) &&
    strings(value, ["session_id", "summary"]) &&
    member(value.status, completionStatuses)
  );
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasString(value: JsonRecord, key: string): boolean {
  return typeof value[key] === "string";
}

function strings(value: JsonRecord, keys: readonly string[]): boolean {
  return keys.every((key) => hasString(value, key));
}

function nullableString(value: unknown): boolean {
  return value === undefined || value === null || typeof value === "string";
}

function nullableBoolean(value: unknown): boolean {
  return value === undefined || value === null || typeof value === "boolean";
}

function nonNegativeInteger(value: unknown): boolean {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function nullableNonNegativeInteger(value: unknown): boolean {
  return value === undefined || value === null || nonNegativeInteger(value);
}

function nullableNonNegativeNumber(value: unknown): boolean {
  return (
    value === undefined ||
    value === null ||
    (typeof value === "number" && Number.isFinite(value) && value >= 0)
  );
}

function member<const T extends readonly string[]>(
  value: unknown,
  values: T,
): value is T[number] {
  return typeof value === "string" && values.includes(value);
}
