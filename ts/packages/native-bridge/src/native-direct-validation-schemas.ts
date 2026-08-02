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

const nativeSessionResourceLimitsSchema = Type.Object(
  {
    workdir: Type.Optional(Type.String()),
    maxDurationMs: Type.Optional(Type.Number({ minimum: 0 })),
    maxDelegationDepth: Type.Optional(Type.Number({ minimum: 0 })),
  },
  { additionalProperties: false },
);

const nativeSessionToolProfileSchema = Type.Object(
  {
    tools: Type.Array(
      Type.Object(
        {
          name: Type.String(),
          description: Type.String(),
          inputSchema: Type.Optional(Type.Number({ minimum: 0 })),
        },
        { additionalProperties: false },
      ),
    ),
  },
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
    resourceLimits: nativeSessionResourceLimitsSchema,
    toolProfile: nativeSessionToolProfileSchema,
    historyWindow: Type.Optional(nativeSessionHistoryWindowSchema),
    reasoningEffort: Type.Optional(Type.String()),
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
    inference_overrides: Type.Optional(
      Type.Object(
        { reasoning_effort: optionalString },
        { additionalProperties: false },
      ),
    ),
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

const rawBufferedBrainRunModuleDiagnosticsSchema = Type.Object(
  {
    module_label: Type.String(),
    active_run_count: Type.Number({ minimum: 0 }),
  },
  { additionalProperties: false },
);

const rawBufferedBrainRunDiagnosticSchema = Type.Object(
  {
    module_label: Type.String(),
    wake_id: Type.String(),
    session_id: Type.String(),
    agent_id: nullableString,
    profile_id: nullableString,
    phase: Type.String(),
    queued_stream_item_count: Type.Number({ minimum: 0 }),
    stream_retention_metrics: Type.Object(
      {
        raw_stream_item_count: Type.Number({ minimum: 0 }),
        raw_delta_item_count: Type.Number({ minimum: 0 }),
        retained_stream_item_count: Type.Number({ minimum: 0 }),
        coalesced_delta_item_count: Type.Number({ minimum: 0 }),
        dropped_stream_item_count: Type.Number({ minimum: 0 }),
        retained_delta_bytes: Type.Number({ minimum: 0 }),
        queued_delta_bytes: Type.Number({ minimum: 0 }),
        max_stream_items: Type.Number({ minimum: 1 }),
        max_stream_delta_bytes: Type.Number({ minimum: 1 }),
      },
      { additionalProperties: false },
    ),
    pending_tool_request_count: Type.Number({ minimum: 0 }),
    submitted_tool_output_count: Type.Number({ minimum: 0 }),
    age_ms: Type.Number({ minimum: 0 }),
    terminal: Type.Boolean(),
    cancelled: Type.Boolean(),
    has_error: Type.Boolean(),
    started_at: Type.String(),
    last_transition_at: Type.String(),
  },
  { additionalProperties: false },
);

export const rawBufferedBrainRunDiagnosticsSchema = Type.Object(
  {
    active_run_count: Type.Number({ minimum: 0 }),
    modules: Type.Array(rawBufferedBrainRunModuleDiagnosticsSchema),
    runs: Type.Array(rawBufferedBrainRunDiagnosticSchema),
  },
  { additionalProperties: false },
);

const rawBufferedBrainRunCleanupModuleSchema = Type.Object(
  {
    module_label: Type.String(),
    active_runs: Type.Number({ minimum: 0 }),
    terminal_runs: Type.Number({ minimum: 0 }),
    cancelled_nonterminal_runs: Type.Number({ minimum: 0 }),
    removed_runs: Type.Number({ minimum: 0 }),
  },
  { additionalProperties: false },
);

export const rawBufferedBrainRunCleanupSummarySchema = Type.Object(
  {
    active_runs: Type.Number({ minimum: 0 }),
    terminal_runs: Type.Number({ minimum: 0 }),
    cancelled_nonterminal_runs: Type.Number({ minimum: 0 }),
    removed_runs: Type.Number({ minimum: 0 }),
    modules: Type.Array(rawBufferedBrainRunCleanupModuleSchema),
  },
  { additionalProperties: false },
);

const rawGitHubGateWaitRecordSchema = Type.Object(
  {
    session_id: Type.String(),
    run_id: nullableString,
    provider_thread_id: nullableString,
    project_id: Type.String(),
    task_id: Type.String(),
    gate_id: Type.Number({ minimum: 0 }),
    commit_sha: Type.String(),
    phase: Type.Union([
      Type.Literal("waiting"),
      Type.Literal("wake_scheduled"),
      Type.Literal("consumed"),
      Type.Literal("cancelled"),
    ]),
    terminal_event_id: nullableNumber,
    created_at: Type.String(),
    updated_at: Type.String(),
  },
  { additionalProperties: false },
);

export const rawNullableGitHubGateWaitRecordSchema = Type.Union([
  rawGitHubGateWaitRecordSchema,
  Type.Null(),
]);

export const rawGitHubGateTerminalReceiptSchema = Type.Object(
  {
    event_id: Type.Number({ minimum: 0 }),
    cursor: Type.Number({ minimum: 0 }),
    duplicate: Type.Boolean(),
    wake_scheduled: Type.Boolean(),
    ignored_reason: nullableString,
    wait: Type.Union([rawGitHubGateWaitRecordSchema, Type.Null()]),
  },
  { additionalProperties: false },
);

const rawOpenAiOauthSummarySchema = Type.Object(
  {
    kind: Type.Literal("openai_oauth"),
    version: Type.Number({ minimum: 0 }),
    has_secret: Type.Boolean(),
    account_id: nullableString,
    email: nullableString,
    plan_type: nullableString,
    is_fedramp_account: Type.Boolean(),
    access_token_expires_at: nullableString,
  },
  { additionalProperties: false },
);

export const rawOpenAiOauthCodeExchangeResultSchema = Type.Union([
  Type.Object(
    {
      ok: Type.Literal(true),
      secret: Type.String(),
      summary: rawOpenAiOauthSummarySchema,
    },
    { additionalProperties: false },
  ),
  Type.Object(
    {
      ok: Type.Literal(false),
      error: Type.Object(
        {
          code: Type.String(),
          reasonCode: Type.String(),
          status: Type.Optional(Type.Number()),
          message: Type.String(),
          retryable: Type.Boolean(),
        },
        { additionalProperties: false },
      ),
    },
    { additionalProperties: false },
  ),
]);

export const rawModelProviderSecretSchema = Type.Union([
  Type.String(),
  Type.Null(),
]);

export const nativeRuntimeBufferViewSchema = Type.Object(
  {
    handle: nativeHandleSchema,
    mediaType: Type.String(),
    byteLen: Type.Number({ minimum: 0 }),
    bytes: Type.Unknown(),
  },
  { additionalProperties: false },
);
