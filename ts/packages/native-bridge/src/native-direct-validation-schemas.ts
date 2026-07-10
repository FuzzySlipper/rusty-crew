import { Type } from "typebox";

const nullableNumber = Type.Union([Type.Number(), Type.Null()]);
const nullableString = Type.Union([Type.String(), Type.Null()]);
const optionalString = Type.Optional(Type.String());

export const nativeHandleSchema = Type.Number({ minimum: 0 });

export const nativeShutdownSummarySchema = Type.Object(
  {
    archivedSessions: Type.Number({ minimum: 0 }),
    droppedSubscriptions: Type.Number({ minimum: 0 }),
  },
  { additionalProperties: false },
);

export const nativeEventReceiptSchema = Type.Object(
  {
    accepted: Type.Boolean(),
    sequence: Type.Number({ minimum: 0 }),
  },
  { additionalProperties: false },
);

const nativeSessionHistoryWindowSchema = Type.Object(
  { maxMessages: Type.Optional(Type.Number({ minimum: 0 })) },
  { additionalProperties: false },
);

export const nativeSessionStateSummarySchema = Type.Object(
  {
    handle: nativeHandleSchema,
    sessionId: Type.String(),
    agentId: Type.String(),
    profileId: Type.String(),
    kind: Type.Union([
      Type.Literal("full"),
      Type.Literal("worker"),
      Type.Literal("delegated"),
    ]),
    status: Type.Union([
      Type.Literal("active"),
      Type.Literal("idle"),
      Type.Literal("archived"),
    ]),
    historyWindow: Type.Optional(nativeSessionHistoryWindowSchema),
  },
  { additionalProperties: false },
);

export const nativeSessionIdArraySchema = Type.Array(Type.String());

export const nativeQueuedMessageRecordSchema = Type.Object(
  {
    messageId: Type.String(),
    ownerSessionId: optionalString,
    ownerAgentId: Type.String(),
    fromAgent: Type.String(),
    toAgent: Type.String(),
    body: Type.String(),
    correlationId: optionalString,
    enqueuedAt: Type.String(),
    expiresAt: Type.String(),
    ttlMs: Type.Number({ minimum: 0 }),
    deliveryAttempts: Type.Number({ minimum: 0 }),
    state: Type.Union([
      Type.Literal("pending"),
      Type.Literal("delivered"),
      Type.Literal("expired"),
      Type.Literal("discarded"),
      Type.Literal("cancelled"),
    ]),
    terminalAt: optionalString,
    stateReason: optionalString,
  },
  { additionalProperties: false },
);

const rawToolDescriptorSchema = Type.Object(
  {
    name: Type.String(),
    description: Type.String(),
    input_schema: nullableNumber,
  },
  { additionalProperties: false },
);

const rawDelegationLineageSchema = Type.Object(
  {
    parent_session_id: Type.String(),
    parent_agent_id: Type.String(),
    source_wake_id: Type.String(),
    source_action_index: Type.Number({ minimum: 0 }),
    requested_task_id: nullableString,
    correlation_id: Type.String(),
  },
  { additionalProperties: false },
);

const rawSessionStateSchema = Type.Object(
  {
    handle: Type.Number({ minimum: 0 }),
    session_id: Type.String(),
    agent_id: Type.String(),
    profile_id: Type.String(),
    kind: Type.Union([
      Type.Literal("full"),
      Type.Literal("worker"),
      Type.Literal("delegated"),
    ]),
    delegation: Type.Union([rawDelegationLineageSchema, Type.Null()]),
    resource_limits: Type.Object(
      {
        workdir: nullableString,
        max_duration_ms: nullableNumber,
        max_delegation_depth: nullableNumber,
      },
      { additionalProperties: false },
    ),
    tool_profile: Type.Object(
      { tools: Type.Array(rawToolDescriptorSchema) },
      { additionalProperties: false },
    ),
    history_window: Type.Union([
      Type.Object(
        { max_messages: nullableNumber },
        { additionalProperties: false },
      ),
      Type.Null(),
    ]),
    status: Type.Union([
      Type.Literal("active"),
      Type.Literal("idle"),
      Type.Literal("archived"),
    ]),
    brain_turn_count: Type.Number({ minimum: 0 }),
    created_at: Type.String(),
    last_active_at: Type.String(),
  },
  { additionalProperties: false },
);

export const rawDelegatedResourceCleanupReportSchema = Type.Object(
  {
    cleaned_at: Type.String(),
    terminal_archived: Type.Array(Type.String()),
    orphaned_archived: Type.Array(Type.String()),
    expired_archived: Type.Array(Type.String()),
    resources_released: Type.Number({ minimum: 0 }),
  },
  { additionalProperties: false },
);

export const rawDelegatedSessionRuntimeStatusSchema = Type.Object(
  {
    session: rawSessionStateSchema,
    parent_session_id: nullableString,
    run_id: nullableString,
    run_status: Type.Union([
      Type.Literal("requested"),
      Type.Literal("session_created"),
      Type.Literal("wake_requested"),
      Type.Literal("running"),
      Type.Literal("checkpoint_waiting"),
      Type.Literal("completed"),
      Type.Literal("failed"),
      Type.Literal("blocked"),
      Type.Literal("exhausted"),
      Type.Literal("cancelled"),
      Type.Literal("expired"),
      Type.Null(),
    ]),
    terminal: Type.Boolean(),
  },
  { additionalProperties: false },
);
