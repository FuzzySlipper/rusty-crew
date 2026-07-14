import { Type } from "typebox";

const unknownRecord = Type.Record(Type.String(), Type.Unknown());
const nullableNumber = Type.Union([Type.Number(), Type.Null()]);
const nullableString = Type.Union([Type.String(), Type.Null()]);

const providerStateInputSchema = Type.Object(
  {
    moduleId: Type.String(),
    strategyId: Type.String(),
    profileFingerprint: Type.String(),
    providerFingerprint: Type.String(),
    payloadVersion: Type.String(),
    payload: Type.Unknown(),
    expiresAt: Type.Optional(Type.String()),
  },
  { additionalProperties: true },
);

const toolProfileSchema = Type.Object(
  {
    tools: Type.Array(
      Type.Object(
        {
          name: Type.String(),
          description: Type.String(),
          inputSchema: Type.Optional(Type.Number()),
        },
        { additionalProperties: true },
      ),
    ),
  },
  { additionalProperties: true },
);

export const sessionStateSchema = Type.Object(
  {
    handle: Type.Number(),
    sessionId: Type.String(),
    agentId: Type.String(),
    profileId: Type.String(),
    kind: Type.Union([
      Type.Literal("full"),
      Type.Literal("worker"),
      Type.Literal("delegated"),
    ]),
    resourceLimits: unknownRecord,
    toolProfile: toolProfileSchema,
    status: Type.Union([
      Type.Literal("active"),
      Type.Literal("idle"),
      Type.Literal("archived"),
    ]),
    brainTurnCount: Type.Number(),
    createdAt: Type.String(),
    lastActiveAt: Type.String(),
  },
  { additionalProperties: true },
);

export const bodyStateSchema = Type.Object(
  {
    session: sessionStateSchema,
    pendingMessages: Type.Array(Type.Unknown()),
    recentEvents: Type.Array(Type.Unknown()),
    childCompletions: Type.Array(Type.Unknown()),
    fanOutGroups: Type.Array(Type.Unknown()),
    deltaPolicy: Type.Object(
      {
        mode: Type.Literal("frozen_snapshot_next_wake"),
        queueOwner: Type.Literal("body"),
        queuedMessageTtlMs: Type.Number(),
        maxQueuedMessages: Type.Number(),
      },
      { additionalProperties: true },
    ),
  },
  { additionalProperties: true },
);

const rawResourceLimitsSchema = Type.Object(
  {
    workdir: Type.Optional(nullableString),
    max_duration_ms: Type.Optional(nullableNumber),
    max_delegation_depth: Type.Optional(nullableNumber),
  },
  { additionalProperties: true },
);

const rawToolProfileSchema = Type.Object(
  {
    tools: Type.Array(
      Type.Object(
        {
          name: Type.String(),
          description: Type.String(),
          inputSchema: Type.Optional(nullableNumber),
          input_schema: Type.Optional(nullableNumber),
        },
        { additionalProperties: true },
      ),
    ),
  },
  { additionalProperties: true },
);

const rawAgentMessageSchema = Type.Object(
  {
    from: Type.String(),
    to: Type.String(),
    body: Type.String(),
    correlation_id: Type.Optional(Type.String()),
    projection: Type.Optional(Type.Unknown()),
  },
  { additionalProperties: true },
);

const rawExternalBindingStatusSchema = Type.Union([
  Type.Literal("active"),
  Type.Literal("degraded"),
  Type.Literal("disconnected"),
  Type.Literal("archived"),
]);

const rawChannelBindingConfigDraftSchema = Type.Object(
  {
    binding_id: Type.String(),
    adapter_id: Type.String(),
    provider: Type.String(),
    agent_id: Type.String(),
    instance_id: Type.Optional(nullableString),
    session_id: Type.Optional(nullableString),
    profile_id: Type.String(),
    external_channel_id: Type.String(),
    external_thread_id: Type.Optional(nullableString),
    external_user_id: Type.Optional(nullableString),
    conversation_project_id: Type.Optional(nullableString),
    conversation_channel_id: Type.Optional(nullableNumber),
    provider_subscription_id: Type.Optional(nullableString),
    status: rawExternalBindingStatusSchema,
  },
  { additionalProperties: true },
);

export const rawChannelIngressRoutePlanInputSchema = Type.Object(
  {
    message: Type.Object(
      {
        adapter_id: Type.String(),
        binding_id: Type.String(),
        provider: Type.String(),
        external_channel_id: Type.String(),
        external_thread_id: Type.Optional(Type.String()),
        external_user_id: Type.String(),
        body: Type.String(),
        mentions: Type.Array(Type.String()),
        expires_at: Type.String(),
        idempotency_key: Type.String(),
        runtime_agent_id: Type.Optional(Type.String()),
      },
      { additionalProperties: true },
    ),
    bindings: Type.Array(rawChannelBindingConfigDraftSchema),
    mention_aliases: Type.Record(Type.String(), Type.String()),
    system_agent_id: Type.Optional(Type.String()),
    now: Type.Optional(Type.String()),
    seen_idempotency_keys: Type.Array(Type.String()),
  },
  { additionalProperties: true },
);

export const rawChannelIngressRoutePlanSchema = Type.Object(
  {
    status: Type.Union([
      Type.Literal("routed"),
      Type.Literal("no_binding"),
      Type.Literal("inactive_binding"),
      Type.Literal("ambiguous"),
      Type.Literal("expired"),
      Type.Literal("duplicate"),
      Type.Literal("denied"),
    ]),
    reason_code: Type.String(),
    reason: Type.String(),
    correlation_id: Type.Optional(nullableString),
    binding: Type.Optional(
      Type.Union([rawChannelBindingConfigDraftSchema, Type.Null()]),
    ),
    candidates: Type.Array(rawChannelBindingConfigDraftSchema),
    route: Type.Optional(
      Type.Union([
        Type.Object(
          {
            from: Type.String(),
            to: Type.String(),
            body: Type.String(),
            correlation_id: Type.String(),
            binding_id: Type.String(),
            session_id: Type.Optional(nullableString),
          },
          { additionalProperties: true },
        ),
        Type.Null(),
      ]),
    ),
  },
  { additionalProperties: true },
);

export const rawDenProductIngressPolicyInputSchema = Type.Object(
  {
    operation: Type.String(),
    entity_kind: Type.String(),
    entity_id: Type.String(),
    project_id: Type.Optional(nullableString),
  },
  { additionalProperties: true },
);

export const rawDenProductIngressPolicyPlanSchema = Type.Object(
  {
    status: Type.Union([Type.Literal("allowed"), Type.Literal("denied")]),
    operation: Type.String(),
    reason_code: Type.String(),
    reason: Type.String(),
    lifecycle_operation: Type.Boolean(),
  },
  { additionalProperties: true },
);

export const rawSessionStateSchema = Type.Object(
  {
    handle: Type.Number(),
    session_id: Type.String(),
    agent_id: Type.String(),
    profile_id: Type.String(),
    kind: Type.Union([
      Type.Literal("full"),
      Type.Literal("worker"),
      Type.Literal("delegated"),
    ]),
    delegation: Type.Optional(Type.Unknown()),
    resource_limits: Type.Optional(rawResourceLimitsSchema),
    tool_profile: Type.Optional(rawToolProfileSchema),
    history_window: Type.Optional(
      Type.Union([
        Type.Object(
          {
            max_messages: Type.Optional(nullableNumber),
          },
          { additionalProperties: true },
        ),
        Type.Null(),
      ]),
    ),
    status: Type.Union([
      Type.Literal("active"),
      Type.Literal("idle"),
      Type.Literal("archived"),
    ]),
    brain_turn_count: Type.Number(),
    created_at: Type.String(),
    last_active_at: Type.String(),
  },
  { additionalProperties: true },
);

export const rawSessionStateArraySchema = Type.Array(rawSessionStateSchema);

export const chatReadModelPageSchema = Type.Object(
  {
    items: Type.Array(
      Type.Object(
        {
          event_id: Type.String(),
          session_id: Type.String(),
          sequence_id: Type.Number(),
          created_at: Type.String(),
          kind: Type.Literal("message_created"),
          payload: Type.Object(
            {
              message_id: Type.String(),
              role: Type.Union([
                Type.Literal("assistant"),
                Type.Literal("user"),
              ]),
              body: Type.String(),
              correlation_id: Type.Optional(nullableString),
              source: Type.Union([
                Type.Literal("durable_message_slot"),
                Type.Literal("pending_body_state"),
              ]),
              slot_status: Type.Optional(Type.String()),
            },
            { additionalProperties: true },
          ),
        },
        { additionalProperties: true },
      ),
    ),
    latest_cursor: Type.String(),
    has_more: Type.Boolean(),
    total: Type.Number(),
    source: Type.Union([
      Type.Literal("event_log"),
      Type.Literal("message_slots"),
      Type.Literal("pending_messages"),
      Type.Literal("empty"),
    ]),
  },
  { additionalProperties: true },
);

export const chatEventLogEventSchema = Type.Object(
  {
    event_id: Type.String(),
    session_id: Type.String(),
    sequence_id: Type.Number(),
    created_at: Type.String(),
    kind: Type.String(),
    payload: Type.Record(Type.String(), Type.Unknown()),
  },
  { additionalProperties: false },
);

export const chatEventLogPageSchema = Type.Object(
  {
    items: Type.Array(chatEventLogEventSchema),
    latest_cursor: Type.String(),
    has_more: Type.Boolean(),
    total: Type.Number(),
    message_count: Type.Number(),
    has_more_before: Type.Boolean(),
  },
  { additionalProperties: false },
);

export const rawBodyStateSchema = Type.Object(
  {
    session: rawSessionStateSchema,
    pending_messages: Type.Array(rawAgentMessageSchema),
    recent_events: Type.Array(
      Type.Object(
        { type: Type.String(), session_id: Type.Optional(Type.String()) },
        { additionalProperties: true },
      ),
    ),
    child_completions: Type.Array(Type.Unknown()),
    fan_out_groups: Type.Array(Type.Unknown()),
    delta_policy: Type.Object(
      {
        mode: Type.Literal("frozen_snapshot_next_wake"),
        queue_owner: Type.Literal("body"),
        queued_message_ttl_ms: Type.Number(),
        max_queued_messages: Type.Number(),
      },
      { additionalProperties: true },
    ),
  },
  { additionalProperties: true },
);

export const brainWakeRequestSchema = Type.Object(
  {
    brain: Type.Number(),
    sessionId: Type.String(),
    bodyState: Type.Number(),
    systemPrompt: Type.Number(),
    roleAssembly: Type.Number(),
    wakeId: Type.String(),
    providerState: Type.Optional(providerStateInputSchema),
    providerStateAbsence: Type.Optional(Type.String()),
  },
  { additionalProperties: true },
);

const projectionRefSchema = Type.Object(
  {
    system: Type.String(),
    kind: Type.String(),
    id: Type.String(),
  },
  { additionalProperties: true },
);

const agentMessageSchema = Type.Object(
  {
    from: Type.String(),
    to: Type.String(),
    body: Type.String(),
    correlationId: Type.Optional(Type.String()),
    projection: Type.Optional(
      Type.Object(
        {
          visibility: Type.Union([
            Type.Literal("observation"),
            Type.Literal("user_visible"),
          ]),
          targetRef: Type.Optional(projectionRefSchema),
          workRef: Type.Optional(projectionRefSchema),
          reason: Type.Optional(Type.String()),
        },
        { additionalProperties: true },
      ),
    ),
  },
  { additionalProperties: true },
);

const brainEventSchema = Type.Union([
  Type.Object(
    { type: Type.Literal("started") },
    { additionalProperties: true },
  ),
  Type.Object(
    { type: Type.Literal("text_delta"), text: Type.String() },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("reasoning_delta"),
      text: Type.String(),
      format: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("phase_change"),
      phase: Type.Union([
        Type.Literal("idle"),
        Type.Literal("exploring"),
        Type.Literal("composing"),
        Type.Literal("reviewing"),
      ]),
      message: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    { type: Type.Literal("tool_call_started"), toolName: Type.String() },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("tool_call_finished"),
      toolName: Type.String(),
      isError: Type.Boolean(),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("provider_status"),
      level: Type.Union([
        Type.Literal("info"),
        Type.Literal("degraded"),
        Type.Literal("error"),
      ]),
      message: Type.String(),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    { type: Type.Literal("finished") },
    { additionalProperties: true },
  ),
]);

export const brainEventEnvelopeSchema = Type.Object(
  {
    wakeId: Type.String(),
    sessionId: Type.String(),
    event: brainEventSchema,
  },
  { additionalProperties: true },
);

const brainActionSchema = Type.Union([
  Type.Object(
    {
      type: Type.Literal("send_message"),
      message: agentMessageSchema,
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("request_delegation"),
      profileId: Type.String(),
      prompt: Type.String(),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("deliver_completion"),
      packet: Type.Object(
        {
          sessionId: Type.String(),
          status: Type.String(),
          summary: Type.String(),
        },
        { additionalProperties: true },
      ),
    },
    { additionalProperties: true },
  ),
]);

export const brainActionBatchSchema = Type.Object(
  {
    wakeId: Type.String(),
    sessionId: Type.String(),
    actions: Type.Array(brainActionSchema),
  },
  { additionalProperties: true },
);

export const brainWakeAcceptedSchema = Type.Object(
  {
    wakeId: Type.String(),
    accepted: Type.Boolean(),
  },
  { additionalProperties: true },
);

export const eventReceiptSchema = Type.Object(
  {
    accepted: Type.Boolean(),
    sequence: Type.Number(),
  },
  { additionalProperties: true },
);

const actionRejectionSchema = Type.Object(
  {
    index: Type.Number(),
    kind: Type.String(),
    message: Type.String(),
  },
  { additionalProperties: true },
);

export const actionBatchReceiptSchema = Type.Object(
  {
    wakeId: Type.String(),
    acceptedActions: Type.Number(),
    rejectedActions: Type.Array(actionRejectionSchema),
  },
  { additionalProperties: true },
);

export const providerStateDiagnosticArraySchema = Type.Array(
  Type.Object(
    {
      sessionId: Type.String(),
      moduleId: Type.String(),
      strategyId: Type.String(),
      status: Type.Union([
        Type.Literal("unused"),
        Type.Literal("valid"),
        Type.Literal("missing"),
        Type.Literal("expired"),
        Type.Literal("invalidated"),
        Type.Literal("load_failed"),
        Type.Literal("save_failed"),
      ]),
      payloadVersion: Type.Optional(Type.String()),
      payloadBytes: Type.Optional(Type.Number()),
      lastWakeId: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
);

const profileRegistryLifecycleStatusSchema = Type.Union([
  Type.Literal("active"),
  Type.Literal("paused"),
  Type.Literal("decommissioned"),
  Type.Literal("archived"),
]);

const sessionKindSchema = Type.Union([
  Type.Literal("full"),
  Type.Literal("worker"),
  Type.Literal("delegated"),
]);

const rawProfileRegistrySourceAssetRefSchema = Type.Object(
  {
    asset_kind: Type.String(),
    path: Type.String(),
    content_hash: Type.Optional(nullableString),
    last_seen_at: Type.Optional(nullableString),
    metadata_json: Type.Unknown(),
  },
  { additionalProperties: true },
);

const rawProfileRegistryDerivedRuntimeRefSchema = Type.Object(
  {
    ref_kind: Type.String(),
    ref_id: Type.String(),
    status: Type.String(),
    updated_at: Type.Optional(nullableString),
    metadata_json: Type.Unknown(),
  },
  { additionalProperties: true },
);

const rawProfileRegistryImportExportSchema = Type.Object(
  {
    imported_from: Type.Optional(nullableString),
    imported_at: Type.Optional(nullableString),
    exported_to: Type.Optional(nullableString),
    exported_at: Type.Optional(nullableString),
    metadata_json: Type.Unknown(),
  },
  { additionalProperties: true },
);

export const rawProfileRegistryRecordSchema = Type.Object(
  {
    profile_id: Type.String(),
    lifecycle_status: profileRegistryLifecycleStatusSchema,
    display_name: Type.Optional(nullableString),
    summary: Type.Optional(nullableString),
    default_session_kind: Type.Optional(
      Type.Union([sessionKindSchema, Type.Null()]),
    ),
    agent_id: Type.Optional(nullableString),
    owner_id: Type.Optional(nullableString),
    prompt_soul_markdown: Type.Optional(nullableString),
    prompt_memory_markdown: Type.Optional(nullableString),
    active_runtime_settings_json: Type.Unknown(),
    source_asset_refs: Type.Array(rawProfileRegistrySourceAssetRefSchema),
    derived_runtime_refs: Type.Array(rawProfileRegistryDerivedRuntimeRefSchema),
    import_export: rawProfileRegistryImportExportSchema,
    revision: Type.Number(),
    created_at: Type.String(),
    updated_at: Type.String(),
  },
  { additionalProperties: true },
);

export const rawProfileRegistryRecordArraySchema = Type.Array(
  rawProfileRegistryRecordSchema,
);

export const rawProfilePurgeReportSchema = Type.Object(
  {
    profile_id: Type.String(),
    profile_registry_deleted: Type.Boolean(),
    session_ids: Type.Array(Type.String()),
    agent_ids: Type.Array(Type.String()),
    table_counts: Type.Array(
      Type.Object(
        {
          table: Type.String(),
          rows_deleted: Type.Number(),
        },
        { additionalProperties: true },
      ),
    ),
    rows_deleted: Type.Number(),
  },
  { additionalProperties: true },
);

const modelProviderStatusSchema = Type.Union([
  Type.Literal("active"),
  Type.Literal("disabled"),
  Type.Literal("archived"),
]);

const modelProviderProtocolSchema = Type.Union([
  Type.Literal("responses"),
  Type.Literal("chat_completions"),
]);

const modelProviderCredentialKindSchema = Type.Union([
  Type.Literal("api_key"),
  Type.Literal("openai_oauth"),
  Type.Literal("legacy_raw_api_key"),
]);

const rawModelProviderCredentialSchema = Type.Object(
  {
    has_secret: Type.Boolean(),
    secret_ref: Type.Optional(nullableString),
    updated_at: Type.Optional(nullableString),
    kind: Type.Optional(
      Type.Union([modelProviderCredentialKindSchema, Type.Null()]),
    ),
  },
  { additionalProperties: true },
);

export const rawModelProviderRecordSchema = Type.Object(
  {
    alias: Type.String(),
    status: modelProviderStatusSchema,
    protocol: modelProviderProtocolSchema,
    provider_kind: Type.String(),
    display_name: Type.Optional(nullableString),
    description: Type.Optional(nullableString),
    base_url: Type.Optional(nullableString),
    model_id: Type.String(),
    context_window_tokens: Type.Optional(nullableNumber),
    max_output_tokens: Type.Optional(nullableNumber),
    temperature_milli: Type.Optional(nullableNumber),
    reasoning_effort: Type.Optional(nullableString),
    reasoning_format: Type.Optional(nullableString),
    credential: rawModelProviderCredentialSchema,
    metadata_json: Type.Unknown(),
    revision: Type.Number(),
    created_at: Type.String(),
    updated_at: Type.String(),
  },
  { additionalProperties: true },
);

export const rawModelProviderRecordArraySchema = Type.Array(
  rawModelProviderRecordSchema,
);

const rawModelProviderAffectedProfileSchema = Type.Object(
  {
    profile_id: Type.String(),
    session_ids: Type.Array(Type.String()),
    configured_session_ids: Type.Array(Type.String()),
    active_session_ids: Type.Array(Type.String()),
  },
  { additionalProperties: true },
);

export const rawModelProviderRefreshImpactSchema = Type.Object(
  {
    provider_alias: Type.String(),
    affected_profiles: Type.Array(rawModelProviderAffectedProfileSchema),
  },
  { additionalProperties: true },
);

const rawModelProviderRefreshProfileActionSchema = Type.Object(
  {
    profile_id: Type.String(),
    command_name: Type.String(),
    reason: Type.String(),
    planned_summary: Type.String(),
    applied_summary: Type.String(),
    blocked_summary: Type.String(),
    failure_reason_code: Type.String(),
  },
  { additionalProperties: true },
);

export const rawModelProviderRefreshPlanSchema = Type.Object(
  {
    provider_alias: Type.String(),
    mode: Type.Union([
      Type.Literal("none"),
      Type.Literal("plan"),
      Type.Literal("apply"),
    ]),
    affected_profiles: Type.Array(rawModelProviderAffectedProfileSchema),
    actions: Type.Array(rawModelProviderRefreshProfileActionSchema),
  },
  { additionalProperties: true },
);

const memoryEvidenceRefSchema = Type.Object(
  {
    evidence_type: Type.String(),
    ref_id: Type.String(),
    label: Type.Optional(nullableString),
  },
  { additionalProperties: true },
);

const memoryRecordShapeRefSchema = Type.Object(
  {
    shape_id: Type.String(),
    version: Type.Number(),
  },
  { additionalProperties: true },
);

const memoryScopeSchema = Type.Object(
  {
    scope_type: Type.String(),
    scope_id: Type.String(),
  },
  { additionalProperties: true },
);

export const rawMemorySpaceDescriptorSchema = Type.Object(
  {
    space_id: Type.String(),
    schema_version: Type.Number(),
    module_id: Type.Optional(nullableString),
    description: Type.String(),
    record_shapes: Type.Array(
      Type.Object(
        {
          shape_id: Type.String(),
          version: Type.Number(),
          description: Type.String(),
          fields: Type.Array(
            Type.Object(
              {
                field_name: Type.String(),
                field_type: Type.String(),
                required: Type.Boolean(),
                description: Type.String(),
              },
              { additionalProperties: true },
            ),
          ),
        },
        { additionalProperties: true },
      ),
    ),
    scope_model: Type.Object(
      {
        allowed_scopes: Type.Array(Type.String()),
        primary_scope: Type.String(),
      },
      { additionalProperties: true },
    ),
    visibility_model: Type.String(),
    retrieval_strategies: Type.Array(Type.String()),
    indexing: Type.Object(
      {
        required_capabilities: Type.Array(Type.String()),
        optional_capabilities: Type.Array(Type.String()),
      },
      { additionalProperties: true },
    ),
    prompt_policy: Type.String(),
    write_policy: Type.Object(
      {
        default_mode: Type.String(),
        operation_policies: Type.Array(
          Type.Object(
            {
              operation: Type.String(),
              governance_mode: Type.String(),
              requires_expected_revision: Type.Boolean(),
              min_confidence: Type.Optional(nullableNumber),
            },
            { additionalProperties: true },
          ),
        ),
      },
      { additionalProperties: true },
    ),
    operations: Type.Array(Type.String()),
    provenance_policy: Type.Object(
      {
        required_evidence: Type.Array(Type.String()),
        source_required: Type.Boolean(),
        rationale_required: Type.Boolean(),
      },
      { additionalProperties: true },
    ),
    retention_policy: Type.String(),
    conflict_policy: Type.String(),
    diagnostics: Type.Object(
      {
        expose_catalog: Type.Boolean(),
        expose_record_counts: Type.Boolean(),
        expose_policy_decisions: Type.Boolean(),
      },
      { additionalProperties: true },
    ),
    export_import: Type.Object(
      {
        export_supported: Type.Boolean(),
        import_supported: Type.Boolean(),
        import_governance_mode: Type.String(),
      },
      { additionalProperties: true },
    ),
  },
  { additionalProperties: true },
);

export const rawMemoryProposalEnvelopeSchema = Type.Object(
  {
    proposal_id: Type.String(),
    space_id: Type.String(),
    operation: Type.String(),
    scope: memoryScopeSchema,
    shape: memoryRecordShapeRefSchema,
    content: Type.Unknown(),
    evidence_refs: Type.Array(memoryEvidenceRefSchema),
    confidence: Type.Number(),
    durability_rationale: Type.Optional(nullableString),
    governance_mode: Type.String(),
    source: Type.String(),
    dedupe_key: Type.Optional(nullableString),
    created_at: Type.Optional(nullableString),
  },
  { additionalProperties: true },
);

export const rawMemoryProposalRecordSchema = Type.Object(
  {
    proposal: rawMemoryProposalEnvelopeSchema,
    status: Type.String(),
    selected_governance_mode: Type.String(),
    created_at: Type.String(),
    updated_at: Type.String(),
    decided_at: Type.Optional(nullableString),
    applied_at: Type.Optional(nullableString),
    resulting_revision: Type.Optional(nullableNumber),
    duplicate_of: Type.Optional(nullableString),
  },
  { additionalProperties: true },
);

export const rawMemoryGovernanceDecisionRecordSchema = Type.Object(
  {
    decision_id: Type.String(),
    proposal_id: Type.String(),
    decision: Type.String(),
    actor: Type.String(),
    source: Type.String(),
    evidence_refs: Type.Array(memoryEvidenceRefSchema),
    policy_mode: Type.String(),
    confidence: Type.Optional(nullableNumber),
    message: Type.Optional(nullableString),
    resulting_revision: Type.Optional(nullableNumber),
    decided_at: Type.String(),
  },
  { additionalProperties: true },
);

export const rawSessionActivityDigestSchema = Type.Object(
  {
    digest_id: Type.String(),
    profile_id: Type.String(),
    session_id: Type.String(),
    wake_id: Type.String(),
    source: Type.String(),
    summary_text: Type.String(),
    event_counts_json: Type.Unknown(),
    tool_calls_json: Type.Unknown(),
    signals_json: Type.Unknown(),
    completion_summary: Type.Optional(nullableString),
    allowed_capture_spaces: Type.Array(Type.String()),
    created_at: Type.String(),
    retention_until: Type.Optional(nullableString),
    reviewed_at: Type.Optional(nullableString),
  },
  { additionalProperties: false },
);

export const rawSessionActivityDigestArraySchema = Type.Array(
  rawSessionActivityDigestSchema,
);

export const rawSessionActivityDigestQuerySchema = Type.Object(
  {
    profile_id: Type.Optional(Type.String()),
    session_id: Type.Optional(Type.String()),
    wake_id: Type.Optional(Type.String()),
    include_reviewed: Type.Boolean(),
    limit: Type.Optional(Type.Number()),
    offset: Type.Optional(Type.Number()),
  },
  { additionalProperties: false },
);

export const rawContextCompactionArtifactSchema = Type.Object(
  {
    artifact_id: Type.String(),
    session_id: Type.String(),
    branch_id: Type.Optional(nullableString),
    strategy_id: Type.String(),
    source_refs_json: Type.Unknown(),
    provider_metadata_json: Type.Unknown(),
    estimate_before_json: Type.Unknown(),
    estimate_after_json: Type.Optional(Type.Unknown()),
    summary_text: Type.String(),
    enters_future_context: Type.Boolean(),
    context_policy: Type.String(),
    metadata_json: Type.Unknown(),
    created_at: Type.String(),
    updated_at: Type.String(),
  },
  { additionalProperties: false },
);

export const rawContextCompactionArtifactArraySchema = Type.Array(
  rawContextCompactionArtifactSchema,
);

export const rawContextCompactionArtifactQuerySchema = Type.Object(
  {
    session_id: Type.Optional(Type.String()),
    branch_id: Type.Optional(Type.String()),
    strategy_id: Type.Optional(Type.String()),
    enters_future_context: Type.Optional(Type.Boolean()),
    latest_only: Type.Boolean(),
    limit: Type.Optional(Type.Number()),
    offset: Type.Optional(Type.Number()),
  },
  { additionalProperties: false },
);

const openAiResponsesClientSchema = Type.Union([
  Type.Object(
    {
      mode: Type.Literal("fake"),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      mode: Type.Literal("live"),
      baseUrl: Type.String(),
      apiKey: Type.Optional(Type.String()),
      authKind: Type.Optional(
        Type.Union([Type.Literal("api_key"), Type.Literal("openai_oauth")]),
      ),
      providerAlias: Type.Optional(Type.String()),
      oauthCredentialSecret: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
]);

const openAiResponsesTransportMetricsSchema = Type.Object(
  {
    effectiveTransport: Type.String(),
    selectedStrategyId: Type.String(),
    effectiveStrategyId: Type.String(),
    fallbackReason: Type.Optional(Type.Union([Type.String(), Type.Null()])),
    providerRequestCount: Type.Number(),
    continuationRoundCount: Type.Number(),
    providerRequestPayloadBytes: Type.Number(),
    providerRequestDebugSamples: Type.Optional(Type.Array(Type.Unknown())),
    providerEventCounts: Type.Record(Type.String(), Type.Number()),
    firstTextDeltaLatencyMs: Type.Optional(
      Type.Union([Type.Number(), Type.Null()]),
    ),
    totalTurnDurationMs: Type.Number(),
  },
  { additionalProperties: true },
);

export const openAiResponsesBrainRunInputSchema = Type.Object(
  {
    wakeId: Type.String(),
    sessionId: Type.String(),
    bodyState: bodyStateSchema,
    tools: Type.Optional(
      Type.Array(
        Type.Object(
          {
            name: Type.String(),
            description: Type.String(),
            inputSchema: Type.Unknown(),
          },
          { additionalProperties: true },
        ),
      ),
    ),
    providerState: Type.Optional(providerStateInputSchema),
    providerStateAbsence: Type.Optional(
      Type.Union([
        Type.Literal("not_configured"),
        Type.Literal("missing"),
        Type.Literal("expired"),
        Type.Literal("invalidated"),
        Type.Literal("module_does_not_use_state"),
        Type.Literal("load_failed"),
      ]),
    ),
    config: Type.Object(
      {
        model: Type.String(),
        instructions: Type.Optional(Type.String()),
        providerRequestTimeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
      },
      { additionalProperties: true },
    ),
    client: Type.Optional(openAiResponsesClientSchema),
  },
  { additionalProperties: true },
);

const chatCompletionMessageSchema = Type.Object(
  {
    role: Type.Union([
      Type.Literal("system"),
      Type.Literal("user"),
      Type.Literal("assistant"),
      Type.Literal("tool"),
    ]),
    content: Type.Optional(Type.String()),
    name: Type.Optional(Type.String()),
    toolCallId: Type.Optional(Type.String()),
    toolCalls: Type.Optional(Type.Array(Type.Unknown())),
  },
  { additionalProperties: true },
);

const piAgentClientSchema = Type.Union([
  Type.Object(
    {
      mode: Type.Literal("fake"),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      mode: Type.Literal("live"),
      baseUrl: Type.String(),
      apiKey: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
]);

export const piAgentBrainRunInputSchema = Type.Object(
  {
    wakeId: Type.String(),
    sessionId: Type.String(),
    messages: Type.Array(chatCompletionMessageSchema),
    tools: Type.Optional(
      Type.Array(
        Type.Object(
          {
            name: Type.String(),
            description: Type.String(),
            inputSchema: Type.Unknown(),
          },
          { additionalProperties: true },
        ),
      ),
    ),
    config: Type.Object(
      {
        model: Type.String(),
        providerRequestTimeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
        wakeTimeoutMs: Type.Optional(Type.Number()),
        temperatureMilli: Type.Optional(Type.Number()),
        maxOutputTokens: Type.Optional(Type.Number()),
        maxToolRounds: Type.Optional(Type.Number()),
        repeatedToolCallLimit: Type.Optional(Type.Number()),
        finalMessageFallbackText: Type.Optional(Type.String()),
      },
      { additionalProperties: true },
    ),
    client: Type.Optional(piAgentClientSchema),
  },
  { additionalProperties: true },
);

const nativeProviderStateInputSchema = Type.Object(
  {
    module_id: Type.String(),
    strategy_id: Type.String(),
    profile_fingerprint: Type.String(),
    provider_fingerprint: Type.String(),
    payload_version: Type.String(),
    payload: Type.Unknown(),
    expires_at: Type.Optional(Type.String()),
  },
  { additionalProperties: true },
);

const rawBrainEventSchema = Type.Union([
  Type.Object(
    { type: Type.Literal("started") },
    { additionalProperties: true },
  ),
  Type.Object(
    { type: Type.Literal("text_delta"), text: Type.String() },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("reasoning_delta"),
      text: Type.String(),
      format: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("phase_change"),
      phase: Type.Union([
        Type.Literal("idle"),
        Type.Literal("exploring"),
        Type.Literal("composing"),
        Type.Literal("reviewing"),
      ]),
      message: Type.Optional(Type.String()),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    { type: Type.Literal("tool_call_started"), tool_name: Type.String() },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("tool_call_finished"),
      tool_name: Type.String(),
      is_error: Type.Boolean(),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("provider_status"),
      level: Type.Union([
        Type.Literal("info"),
        Type.Literal("degraded"),
        Type.Literal("error"),
      ]),
      message: Type.String(),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    { type: Type.Literal("finished") },
    { additionalProperties: true },
  ),
]);

const rawBrainActionSchema = Type.Union([
  Type.Object(
    {
      type: Type.Literal("send_message"),
      message: Type.Object(
        {
          from: Type.String(),
          to: Type.String(),
          body: Type.String(),
          correlation_id: Type.Optional(Type.String()),
          projection: Type.Optional(Type.Unknown()),
        },
        { additionalProperties: true },
      ),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("request_delegation"),
      profile_id: Type.String(),
      prompt: Type.String(),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("deliver_completion"),
      packet: Type.Object(
        {
          session_id: Type.String(),
          status: Type.String(),
          summary: Type.String(),
        },
        { additionalProperties: true },
      ),
    },
    { additionalProperties: true },
  ),
]);

const rawBrainWakeStreamItemSchema = Type.Union([
  Type.Object(
    {
      type: Type.Literal("event"),
      event: Type.Object(
        {
          wake_id: Type.String(),
          session_id: Type.String(),
          event: rawBrainEventSchema,
        },
        { additionalProperties: true },
      ),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("actions"),
      batch: Type.Object(
        {
          wake_id: Type.String(),
          session_id: Type.String(),
          actions: Type.Array(rawBrainActionSchema),
        },
        { additionalProperties: true },
      ),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("wake_failed"),
      failure: Type.Object(
        {
          wake_id: Type.String(),
          session_id: Type.String(),
          kind: Type.String(),
          message: Type.String(),
        },
        { additionalProperties: true },
      ),
    },
    { additionalProperties: true },
  ),
]);

const rawProviderStateOutputSchema = Type.Union([
  Type.Object(
    { type: Type.Literal("unchanged") },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("replace"),
      state: Type.Intersect([
        nativeProviderStateInputSchema,
        Type.Object(
          {
            ttl_ms: Type.Optional(Type.Number()),
          },
          { additionalProperties: true },
        ),
      ]),
    },
    { additionalProperties: true },
  ),
  Type.Object(
    {
      type: Type.Literal("clear"),
      reason: Type.Literal("brain_requested_clear"),
    },
    { additionalProperties: true },
  ),
]);

export const rawOpenAiResponsesBrainRunResultSchema = Type.Object(
  {
    stream: Type.Array(rawBrainWakeStreamItemSchema),
    provider_state: Type.Optional(
      Type.Union([rawProviderStateOutputSchema, Type.Null()]),
    ),
    transport_metrics: Type.Optional(openAiResponsesTransportMetricsSchema),
    credential_secret_update: Type.Optional(
      Type.Object(
        {
          provider_alias: Type.String(),
          secret: Type.String(),
        },
        { additionalProperties: true },
      ),
    ),
  },
  { additionalProperties: true },
);

const piAgentTransportMetricsSchema = Type.Object(
  {
    provider_request_count: Type.Number(),
    tool_round_count: Type.Number(),
  },
  { additionalProperties: true },
);

export const rawPiAgentBufferedDrainResultSchema = Type.Object(
  {
    wake_id: Type.String(),
    items: Type.Array(rawBrainWakeStreamItemSchema),
    tool_requests: Type.Optional(
      Type.Array(
        Type.Object(
          {
            call_id: Type.String(),
            provider_item_id: Type.Optional(
              Type.Union([Type.String(), Type.Null()]),
            ),
            name: Type.String(),
            arguments_json: Type.String(),
          },
          { additionalProperties: true },
        ),
      ),
    ),
    terminal: Type.Boolean(),
    transport_metrics: Type.Optional(
      Type.Union([Type.Null(), piAgentTransportMetricsSchema]),
    ),
    error: Type.Optional(Type.Union([Type.String(), Type.Null()])),
    cancellation: Type.Optional(
      Type.Union([
        Type.Null(),
        Type.Object(
          {
            reason_code: Type.String(),
            summary: Type.String(),
            cancelled_at: Type.String(),
          },
          { additionalProperties: true },
        ),
      ]),
    ),
  },
  { additionalProperties: true },
);

export const rawBufferedBrainRunDrainSchema = Type.Object(
  {
    module_id: Type.Union([
      Type.Literal("pi-agent"),
      Type.Literal("openai-responses"),
    ]),
    wake_id: Type.String(),
    items: Type.Array(rawBrainWakeStreamItemSchema),
    tool_requests: Type.Array(
      Type.Object(
        {
          wake_id: Type.Optional(Type.String()),
          call_id: Type.String(),
          provider_item_id: Type.Optional(
            Type.Union([Type.String(), Type.Null()]),
          ),
          name: Type.String(),
          arguments_json: Type.String(),
        },
        { additionalProperties: true },
      ),
    ),
    terminal: Type.Boolean(),
    provider_state: Type.Optional(
      Type.Union([rawProviderStateOutputSchema, Type.Null()]),
    ),
    transport_metrics: Type.Optional(
      Type.Union([
        openAiResponsesTransportMetricsSchema,
        piAgentTransportMetricsSchema,
        Type.Null(),
      ]),
    ),
    credential_secret_update: Type.Optional(
      Type.Union([
        Type.Null(),
        Type.Object(
          {
            provider_alias: Type.String(),
            secret: Type.String(),
          },
          { additionalProperties: true },
        ),
      ]),
    ),
    cancellation: Type.Optional(
      Type.Union([
        Type.Null(),
        Type.Object(
          {
            reason_code: Type.String(),
            summary: Type.String(),
            cancelled_at: Type.String(),
          },
          { additionalProperties: true },
        ),
      ]),
    ),
    error: Type.Optional(Type.Union([Type.String(), Type.Null()])),
  },
  { additionalProperties: true },
);
