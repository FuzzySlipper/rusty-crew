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
    resource_limits: Type.Optional(rawResourceLimitsSchema),
    tool_profile: Type.Optional(rawToolProfileSchema),
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

export const rawBodyStateSchema = Type.Object(
  {
    session: rawSessionStateSchema,
    pending_messages: Type.Array(rawAgentMessageSchema),
    recent_events: Type.Array(
      Type.Object({ type: Type.String() }, { additionalProperties: true }),
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
  Type.Object({ type: Type.Literal("started") }, { additionalProperties: true }),
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
    default_session_kind: Type.Optional(Type.Union([sessionKindSchema, Type.Null()])),
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

export const openAiResponsesBrainRunInputSchema = Type.Object(
  {
    wakeId: Type.String(),
    sessionId: Type.String(),
    bodyState: bodyStateSchema,
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
        streamIdleTimeoutMs: Type.Optional(Type.Number()),
      },
      { additionalProperties: true },
    ),
    client: Type.Optional(openAiResponsesClientSchema),
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
  Type.Object({ type: Type.Literal("started") }, { additionalProperties: true }),
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
    provider_state: Type.Optional(rawProviderStateOutputSchema),
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
