import type {
  CoreEvent,
  SessionId,
  SessionState,
  BrainModelConfig,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeModelProviderRecord,
} from "@rusty-crew/native-bridge";
import { contextStrategyPolicyFromUnknown } from "./context-strategy.js";
import {
  estimateContextUsage,
  estimateTextFragmentsTokens,
  textFragmentsFromPayload,
} from "./context-estimate.js";
import { buildProfileRoleAssembly } from "./profile-role-assembly.js";
import { loadServiceProfileContext } from "./service-profile-context.js";
import { defaultProfileBrainForModelProvider } from "./service-profile-admin-mutations.js";
import { resolveReasoningEffort } from "./reasoning-effort-policy.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { ToolMediaAttachmentStore } from "./tool-media-attachments.js";
import type {
  RoleplayRouteContext,
  RoleplayAssistantAlternativeGenerationInput,
  RoleplayAssistantAlternativeGenerationResult,
} from "./service-roleplay-routes.js";
import {
  roleplayPromptContextForSession,
  roleplaySpeakerIdentitySnapshotForMessage,
} from "./service-roleplay-routes.js";
import type {
  AttachmentMutationResult,
  AttachmentPage,
  AttachmentRecord,
  ChatEvent,
  ChatSessionReadFactsPage,
  ChatSessionReadInput,
  ChatSessionReadProjection,
  ChatSessionSummaryQuery,
  ChatSendMessageInput,
  ConversationBranchMutationResult,
  ConversationBranchRecord,
  ConversationBranchStateInput,
  ConversationBranchStateRecord,
  ConversationJumpResult,
  ConversationSnapshotMutationResult,
  ConversationSnapshotRecord,
  ConversationTreeInput,
  ConversationTreeProjection,
  CreateAttachmentInput,
  CreateConversationBranchInput,
  CreateConversationSnapshotInput,
  CreateDataBankScopeInput,
  CreateMessageSlotInput,
  CreateMessageVariantInput,
  DataBankScopeMutationResult,
  DataBankScopePage,
  DataBankScopeRecord,
  DeleteMessageVariantInput,
  ListAttachmentsInput,
  ListDataBankScopesInput,
  ListMessageSlotsInput,
  ListMessageVariantsInput,
  MessageBlockDraft,
  MessageSlotMutationResult,
  MessageSlotPage,
  MessageSlotRecord,
  MessageVariantMutationResult,
  MessageVariantPage,
  MessageVariantRecord,
  MessageVariantsReorderResult,
  NativeContextAccountingSnapshot,
  ProviderRequestDebugDetail,
  ReorderMessageVariantsInput,
  RemoveAttachmentInput,
  RemoveDataBankScopeInput,
  ResolveConversationJumpInput,
  SearchTranscriptInput,
  SelectActiveConversationBranchInput,
  SelectActiveConversationBranchResult,
  SelectActiveMessageVariantInput,
  SelectActiveMessageVariantResult,
  SendChatMessageResult,
  SessionContextUsageResult,
  ToolCallDebugDetail,
  TranscriptSearchResultPage,
  UpdateConversationBranchHeadInput,
  UpdateConversationBranchHeadResult,
} from "./rusty-view-chat-api.js";

export interface RustyViewChatWakeReport {
  status: "completed" | "continuing" | "rejected" | "skipped" | "failed";
  wakeId?: string;
  summary: string;
  reasonCode?: string;
  observedEvents?: readonly CoreEvent[];
  completionPacket?: { summary?: string };
}

export interface RustyViewChatOperationsContext {
  bridge: NativeBridgeModule;
  get runtimeConfig(): RustyCrewRuntimeConfig;
  toolCallDebugStore: ToolCallDebugStore;
  providerRequestDebugStore: ProviderRequestDebugStore;
  toolMediaAttachments: ToolMediaAttachmentStore;
  now(): string;
  appendChatEvent(
    sessionId: SessionId,
    event: Pick<ChatEvent, "kind" | "payload">,
  ): Promise<ChatEvent>;
  listChatEventsAfterCursor(
    session: SessionState,
    cursor: string | undefined,
    limit: number,
  ): Promise<readonly ChatEvent[]>;
  roleplayRouteContext(): RoleplayRouteContext;
  submitServiceTurn(input: {
    sessionId: SessionId;
    from: string;
    body: string;
    correlationId: string;
    source: "chat";
    appendChatEvents?: boolean;
  }): Promise<RustyViewChatWakeReport>;
  resolveModelProviderForBrain(alias: string): Promise<BrainModelConfig>;
}

export async function submitRustyViewChatMessage(
  context: RustyViewChatOperationsContext,
  input: ChatSendMessageInput,
): Promise<SendChatMessageResult> {
  const messageId = input.clientMessageId ?? `chat:${input.idempotencyKey}`;
  const correlationId = `chat:${input.idempotencyKey}`;
  const slotId = stableChatRecordId(
    "slot",
    `${input.session.sessionId}:${input.idempotencyKey}`,
  );
  const primaryVariantId = stableChatRecordId("variant", slotId);
  const now = context.now();
  const fallbackBranchId = stableChatRecordId(
    "branch",
    `${input.session.sessionId}:default`,
  );
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    context.roleplayRouteContext(),
    input.session,
    input.actor,
    now,
  ).catch(() => undefined);
  const messageMetadata = {
    source: "rusty_view_chat",
    correlation_id: correlationId,
    reason: input.reason,
    ...(speakerIdentity === undefined
      ? {}
      : { speaker_identity: speakerIdentity }),
  };
  const durable = (await context.bridge.createChatMessageSlot({
    slot: {
      slot_id: slotId,
      session_id: input.session.sessionId,
      primary_variant_id: primaryVariantId,
      active_variant_id: null,
      metadata_json: messageMetadata,
      created_at: now,
      updated_at: now,
    },
    primary_variant: messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId,
      variantId: primaryVariantId,
      messageId,
      source: "primary",
      ordinal: 0,
      actor: input.actor,
      body: input.body,
      branchId: fallbackBranchId,
      metadataJson: messageMetadata,
      now,
    }),
    branch_id: fallbackBranchId,
    expected_branch_head: { type: "any" },
    updated_at: now,
    ensure_active_branch: {
      session_id: input.session.sessionId,
      branch_id: fallbackBranchId,
      label: "Default",
      metadata_json: { source: "rusty_view_chat_default" },
      created_at: now,
      updated_at: now,
    },
    inherit_branch_head: true,
    idempotency_key: input.idempotencyKey,
  })) as {
    slot?: MessageSlotRecord | null;
    branch: ConversationBranchRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
    duplicate?: boolean;
  };
  if (durable.conflict || !durable.slot) {
    throw new Error(
      `chat message ingest conflicted for ${input.session.sessionId}`,
    );
  }
  const branch = durable.branch;
  const persistedMessage = durable.slot.primary.message;
  if (durable.duplicate) {
    const latest = await context.listChatEventsAfterCursor(
      input.session,
      undefined,
      1,
    );
    return {
      status: "duplicate",
      message_id: persistedMessage.message_id,
      slot_id: durable.slot.slot_id,
      primary_variant_id: durable.slot.primary_variant_id,
      correlation_id: correlationId,
      latest_cursor: latest.at(-1)?.event_id ?? `${input.session.sessionId}:0`,
      summary: "duplicate chat message ignored",
      reason_code: "duplicate_idempotency_key",
    };
  }
  const inbound = await context.appendChatEvent(input.session.sessionId, {
    kind: "message_created",
    payload: {
      message_id: messageId,
      slot_id: slotId,
      primary_variant_id: primaryVariantId,
      branch_id: persistedMessage.branch_id,
      parent_message_id: persistedMessage.parent_message_id,
      previous_message_id: persistedMessage.previous_message_id,
      role: input.actor.kind === "agent" ? "assistant" : "user",
      actor: input.actor,
      body: input.body,
      ...(speakerIdentity === undefined
        ? {}
        : { speaker_identity: speakerIdentity }),
      correlation_id: correlationId,
      reason: input.reason,
    },
  });
  const wakeReport = await context.submitServiceTurn({
    sessionId: input.session.sessionId,
    from: input.actor.id,
    body: input.body,
    correlationId,
    source: "chat",
  });
  if (wakeReport.status === "completed") {
    await persistCompletedAssistantTurn(context, {
      session: input.session,
      branch,
      userMessageId: messageId,
      correlationId,
      wakeReport,
    });
  }
  const result: SendChatMessageResult = {
    status:
      wakeReport.status === "completed" || wakeReport.status === "continuing"
        ? "accepted"
        : "rejected",
    message_id: messageId,
    slot_id: slotId,
    primary_variant_id: primaryVariantId,
    wake_id: wakeReport.wakeId,
    correlation_id: correlationId,
    latest_cursor: inbound.event_id,
    summary: wakeReport.summary,
    reason_code: wakeReport.reasonCode,
  };
  return result;
}

async function persistCompletedAssistantTurn(
  context: RustyViewChatOperationsContext,
  input: {
    session: ChatSendMessageInput["session"];
    branch: ConversationBranchRecord;
    userMessageId: string;
    correlationId: string;
    wakeReport: RustyViewChatWakeReport;
  },
): Promise<void> {
  const stableWakeId = input.wakeReport.wakeId ?? input.correlationId;
  const attachments = input.wakeReport.wakeId
    ? await context.toolMediaAttachments.attachmentsForWake(
        input.session.sessionId,
        input.wakeReport.wakeId,
      )
    : [];
  const body =
    assistantTextFromCoreEvents(input.wakeReport.observedEvents ?? []) ??
    (attachments.length > 0 ? "" : undefined);
  if (body === undefined) return;
  const now = context.now();
  const messageId = stableChatRecordId("assistant-message", stableWakeId);
  const slotId = stableChatRecordId("slot", messageId);
  const variantId = stableChatRecordId("variant", slotId);
  const actor = { id: input.session.agentId, kind: "agent" as const };
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    context.roleplayRouteContext(),
    input.session,
    actor,
    now,
  ).catch(() => undefined);
  const metadataJson = {
    source: "rusty_view_chat_assistant_wake",
    correlation_id: input.correlationId,
    wake_id: input.wakeReport.wakeId,
    ...(speakerIdentity === undefined
      ? {}
      : { speaker_identity: speakerIdentity }),
  };
  const result = (await context.bridge.createChatMessageSlot({
    slot: {
      slot_id: slotId,
      session_id: input.session.sessionId,
      primary_variant_id: variantId,
      active_variant_id: null,
      metadata_json: metadataJson,
      created_at: now,
      updated_at: now,
    },
    primary_variant: messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId,
      variantId,
      messageId,
      source: "primary",
      ordinal: 0,
      actor,
      body,
      branchId: input.branch.branch_id,
      parentMessageId: input.userMessageId,
      previousMessageId: input.userMessageId,
      metadataJson,
      now,
      blocks: [
        ...(body.length > 0
          ? [{ kind: "text", content_json: { text: body } }]
          : []),
        ...attachments.map((attachment) => ({
          block_id: attachmentBlockId(messageId, attachment.attachment_id),
          kind: "attachment",
          content_json: {
            attachment_id: attachment.attachment_id,
            filename: attachment.filename,
            mime_type: attachment.mime_type,
            byte_size: attachment.byte_size,
            download_url: attachment.download_url,
          },
          render_policy_json: { display: "inline_media" },
          metadata_json: { source: "brain_tool_media" },
        })),
      ],
    }),
    branch_id: input.branch.branch_id,
    expected_branch_head: { type: "any" },
    updated_at: now,
    ensure_active_branch: null,
    inherit_branch_head: false,
    idempotency_key: null,
  })) as {
    slot?: MessageSlotRecord | null;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  if (result.conflict || !result.slot) {
    throw new Error(
      `assistant chat slot persistence conflicted for ${input.session.sessionId}`,
    );
  }
  if (input.wakeReport.wakeId && attachments.length > 0) {
    await context.toolMediaAttachments
      .linkAttachmentsToMessage({
        sessionId: input.session.sessionId,
        wakeId: input.wakeReport.wakeId,
        messageId,
        blockIdsByAttachmentId: new Map(
          attachments.map((attachment) => [
            attachment.attachment_id,
            attachmentBlockId(messageId, attachment.attachment_id),
          ]),
        ),
      })
      .catch(async (error) => {
        await context.appendChatEvent(input.session.sessionId, {
          kind: "provider_status",
          payload: {
            wake_id: input.wakeReport.wakeId,
            level: "error",
            message: errorMessage(
              error,
              "tool media attachment link persistence failed",
            ),
            metadata_json: JSON.stringify({
              reason_code: "tool_media_link_persistence_failed",
              attachment_ids: attachments.map(
                (attachment) => attachment.attachment_id,
              ),
            }),
          },
        });
      });
  }
}

export async function generateRoleplayAssistantAlternativeViaWake(
  context: RustyViewChatOperationsContext,
  input: RoleplayAssistantAlternativeGenerationInput,
): Promise<RoleplayAssistantAlternativeGenerationResult> {
  const beforeCursor = undefined;
  const correlationId = `roleplay-alternative:${input.requestId}`;
  const wakeReport = await context.submitServiceTurn({
    sessionId: input.session.sessionId,
    from: "roleplay-alternative-generator",
    body: input.prompt,
    correlationId,
    source: "chat",
    appendChatEvents: false,
  });
  if (wakeReport.status !== "completed") {
    throw new Error(
      `roleplay assistant alternative generation failed: ${wakeReport.summary}`,
    );
  }
  const generatedBody =
    assistantTextFromCoreEvents(wakeReport.observedEvents ?? []) ??
    wakeReport.completionPacket?.summary ??
    wakeReport.summary;
  return {
    body: generatedBody,
    wakeId: wakeReport.wakeId,
    summary: wakeReport.summary,
    metadataJson: {
      correlation_id: correlationId,
      generator: "service_wake",
      suppressed_chat_events_after_cursor: beforeCursor,
    },
  };
}

function assistantTextFromCoreEvents(
  events: readonly CoreEvent[],
): string | undefined {
  const text = events
    .filter(
      (event): event is Extract<CoreEvent, { type: "brain_event_observed" }> =>
        event.type === "brain_event_observed" &&
        event.event.type === "text_delta",
    )
    .map((event) => (event.event.type === "text_delta" ? event.event.text : ""))
    .join("")
    .trim();
  return text.length > 0 ? text : undefined;
}

export async function rustyViewSessionContextUsage(
  context: RustyViewChatOperationsContext,
  input: { session: SessionState; requestId: string },
): Promise<SessionContextUsageResult> {
  const diagnostics: SessionContextUsageResult["diagnostics"] = [];
  const registryRecord = await context.bridge
    .getProfileRegistryRecord(input.session.profileId)
    .catch((error) => {
      diagnostics.push({
        severity: "warning",
        code: "profile_registry_read_failed",
        message: errorMessage(error, "profile registry read failed"),
      });
      return undefined;
    });
  if (registryRecord === undefined) {
    diagnostics.push({
      severity: "warning",
      code: "profile_registry_record_missing",
      message:
        "profile registry record is missing; model diagnostics may be incomplete until the profile is created through the DB-backed profile API",
    });
  }

  const settings =
    optionalRecord(registryRecord?.activeRuntimeSettingsJson) ?? {};
  const providerAlias =
    optionalString(settings.providerAlias) ??
    optionalString(settings.provider_alias) ??
    "default";
  const provider = await context.bridge
    .getModelProvider(providerAlias)
    .catch((error) => {
      diagnostics.push({
        severity: "warning",
        code: "model_provider_read_failed",
        message: errorMessage(error, "model provider read failed"),
      });
      return undefined;
    });
  if (provider === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_provider_missing",
      message: `model provider alias ${providerAlias} was not found`,
    });
  } else if (provider.status !== "active") {
    diagnostics.push({
      severity: "warning",
      code: "model_provider_not_active",
      message: `model provider alias ${providerAlias} is ${provider.status}`,
    });
  }

  const brain =
    brainMetadataFromUnknown(settings.brain) ??
    (provider === undefined
      ? undefined
      : defaultProfileBrainForModelProvider(provider));
  const toolPolicy = profileToolPolicyFromUnknown(
    settings.toolPolicy ?? settings.tool_policy,
  );
  const contextPolicy = contextStrategyPolicyFromUnknown(
    settings.contextPolicy ?? settings.context_policy,
  );
  const localToolProfileId =
    optionalString(settings.localToolProfileId) ??
    optionalString(settings.local_tool_profile_id);
  const mcpBindings = context.runtimeConfig.mcpBindings.filter(
    (binding) =>
      String(binding.profileId) === input.session.profileId ||
      String(binding.sessionId) === input.session.sessionId,
  );
  const activeMcpBindings = mcpBindings.filter(
    (binding) => binding.status === undefined || binding.status === "active",
  );
  const sampledEvents = await context.listChatEventsAfterCursor(
    input.session,
    undefined,
    1_000,
  );
  const nativeSnapshot = latestNativeContextAccountingSnapshot(sampledEvents);
  if (nativeSnapshot === undefined) {
    diagnostics.push({
      severity: "info",
      code: "native_context_snapshot_not_yet_available",
      message:
        "the Rust-owned context accounting snapshot is emitted after a provider request; legacy fields below are compatibility-only estimates until one is available",
    });
  }
  const sampledMessageCount = sampledEvents.filter(
    (event) =>
      event.kind === "message_created" ||
      event.kind === "assistant_message_completed",
  ).length;
  const historyFragments = sampledEvents.flatMap((event) =>
    textFragmentsFromPayload(event.payload),
  );
  const systemFragments: string[] = [];
  const segmentNotes: NonNullable<
    SessionContextUsageResult["context"]["token_segments"]
  >["notes"] = [];
  const profileContext = await loadServiceProfileContext({
    bridge: context.bridge,
    profilesDir: context.runtimeConfig.profilesDir,
    skillsDir: context.runtimeConfig.skillsDir,
    profileId: input.session.profileId,
    modelProviderResolver: (alias) =>
      context.resolveModelProviderForBrain(alias),
  }).catch((error) => {
    diagnostics.push({
      severity: "warning",
      code: "profile_context_load_failed",
      message: errorMessage(error, "profile context load failed"),
    });
    segmentNotes.push({
      segment: "system",
      status: "unavailable",
      message:
        "profile role assembly could not be loaded, so system/narrator prompt tokens are unavailable",
    });
    return undefined;
  });
  if (profileContext !== undefined) {
    const role = buildProfileRoleAssembly(profileContext, {
      includeSkillBodies: false,
    });
    systemFragments.push(
      ...[role.systemPrompt, role.roleAssembly.instructions].filter(
        (fragment): fragment is string => typeof fragment === "string",
      ),
    );
    segmentNotes.push({
      segment: "system",
      status: "estimated",
      message:
        "system/narrator prompt tokens are approximate fallback estimates from profile role assembly without live provider tokenizer",
    });
  }
  const roleplayContext = await roleplayPromptContextForSession(
    context.roleplayRouteContext(),
    input.session,
  ).catch((error) => {
    diagnostics.push({
      severity: "warning",
      code: "roleplay_context_load_failed",
      message: errorMessage(error, "roleplay context load failed"),
    });
    segmentNotes.push({
      segment: "lore",
      status: "unavailable",
      message:
        "roleplay session lore/setup context could not be loaded, so lore tokens are unavailable",
    });
    return undefined;
  });
  const loreFragments = roleplayContext === undefined ? [] : [roleplayContext];
  segmentNotes.push({
    segment: "lore",
    status: loreFragments.length === 0 ? "unavailable" : "estimated",
    message:
      loreFragments.length === 0
        ? "no roleplay session lore/setup context is active for this session"
        : "lore tokens are approximate fallback estimates from roleplay session setup context; tool-recalled lore is selected during the model turn and is not pre-counted here",
  });
  segmentNotes.push({
    segment: "history",
    status: "estimated",
    message:
      "history tokens are approximate fallback estimates from sampled chat event text",
  });
  const systemTokens =
    systemFragments.length === 0
      ? undefined
      : estimateTextFragmentsTokens(systemFragments);
  const loreTokens =
    loreFragments.length === 0
      ? undefined
      : estimateTextFragmentsTokens(loreFragments);
  const historyTokens = estimateTextFragmentsTokens(historyFragments);
  const contextUsage = estimateContextUsage({
    provider,
    textFragments: [...systemFragments, ...loreFragments, ...historyFragments],
    sampledEventCount: sampledEvents.length,
    sampledMessageCount,
  });
  if (contextUsage.budget.contextWindowTokens === undefined) {
    diagnostics.push({
      severity: "info",
      code: "context_window_unknown",
      message: "model provider does not declare contextWindowTokens",
    });
  }
  if (provider?.reasoningFormat !== undefined) {
    diagnostics.push({
      severity: "warning",
      code: "provider_reasoning_format_not_applied",
      message:
        "reasoningFormat is stored for provider diagnostics but is not mapped by the selected native brain protocol",
    });
  }
  if (
    provider?.protocol === "responses" &&
    provider.temperatureMilli !== undefined
  ) {
    diagnostics.push({
      severity: "warning",
      code: "provider_temperature_not_applied",
      message:
        "temperature is not supported by the native Responses request contract and is omitted",
    });
  }
  const latestCompactionArtifact = await context.bridge
    .listContextCompactionArtifacts({
      session_id: input.session.sessionId,
      latest_only: true,
      terminal_status: "completed",
      limit: 1,
      offset: 0,
    })
    .then((artifacts) => artifacts[0])
    .catch((error) => {
      diagnostics.push({
        severity: "warning",
        code: "context_compaction_artifact_read_failed",
        message: errorMessage(error, "context compaction artifact read failed"),
      });
      return undefined;
    });
  const redactedUrl = redactedProviderUrl(provider?.baseUrl);
  const reasoningEffort = resolveReasoningEffort(
    input.session.inferenceOverrides?.reasoningEffort ?? undefined,
    provider?.reasoningEffort,
  );
  return {
    session_id: input.session.sessionId,
    agent_id: input.session.agentId,
    profile_id: input.session.profileId,
    provider: {
      alias: providerAlias,
      status: provider?.status ?? "missing",
      protocol: provider?.protocol,
      provider_kind: provider?.providerKind,
      display_name: provider?.displayName,
      base_url_host: redactedUrl.host,
      base_url_redacted: redactedUrl.redacted,
      model_id: provider?.modelId,
      context_window_tokens: contextUsage.budget.contextWindowTokens,
      max_output_tokens: contextUsage.budget.maxOutputTokens,
      temperature:
        provider?.temperatureMilli === undefined
          ? undefined
          : provider.temperatureMilli / 1_000,
      reasoning_effort: reasoningEffort.value,
      reasoning_effort_source: reasoningEffort.source,
      provider_reasoning_effort: provider?.reasoningEffort,
      session_reasoning_effort_override:
        input.session.inferenceOverrides?.reasoningEffort ?? undefined,
      reasoning_format: provider?.reasoningFormat,
      responses_dialect: provider?.responsesDialect,
      chat_completions_dialect: provider?.chatCompletionsDialect,
      thinking_mode: provider?.thinkingMode,
      reasoning_history: provider?.reasoningHistory,
      reasoning_budget_tokens: provider?.reasoningBudgetTokens,
      prompt_caching: provider?.promptCaching,
      thinking_settings_applied:
        provider?.protocol === "chat_completions" &&
        provider.chatCompletionsDialect !== "standard" &&
        (provider.thinkingMode !== "provider_default" ||
          provider.reasoningHistory !== "provider_default" ||
          provider.reasoningBudgetTokens !== undefined),
      thinking_mode_applied:
        provider?.protocol === "chat_completions" &&
        provider.chatCompletionsDialect !== "standard" &&
        provider.thinkingMode !== "provider_default",
      reasoning_history_applied:
        provider?.protocol === "chat_completions" &&
        provider.chatCompletionsDialect !== "standard" &&
        provider.reasoningHistory !== "provider_default",
      reasoning_budget_applied:
        provider?.protocol === "chat_completions" &&
        provider.chatCompletionsDialect === "qwen" &&
        provider.reasoningBudgetTokens !== undefined,
      revision: provider?.revision,
    },
    brain: {
      module: brain?.module,
      strategy: brain?.strategy,
      backend: brain?.module ?? providerBrainBackend(provider),
    },
    context_strategy: {
      strategy_id: contextPolicy.strategyId,
      enabled: contextPolicy.enabled,
      auto_compaction_enabled: contextPolicy.autoCompactionEnabled,
      compact_at_percent: contextPolicy.compactAtPercent,
      target_percent_after_compaction:
        contextPolicy.targetPercentAfterCompaction,
      max_context_percent_for_wake: contextPolicy.maxContextPercentForWake,
      debug_visibility: contextPolicy.debugVisibility,
      include_debug_events_in_model_context:
        contextPolicy.includeDebugEventsInModelContext,
    },
    tools: {
      local_tool_profile_id: localToolProfileId,
      tool_count: input.session.toolProfile.tools.length,
      requested_toolsets:
        toolPolicy?.requestedToolsets === undefined
          ? undefined
          : [...toolPolicy.requestedToolsets],
      requested_tools:
        toolPolicy?.requestedTools === undefined
          ? undefined
          : [...toolPolicy.requestedTools],
      mcp_binding_count: mcpBindings.length,
      mcp_active_count: activeMcpBindings.length,
    },
    context: {
      estimate_quality: contextUsage.estimateQuality,
      estimate_method: contextUsage.estimateMethod,
      estimator_id: contextUsage.estimatorId,
      context_window_tokens: contextUsage.budget.contextWindowTokens,
      estimated_prompt_tokens: contextUsage.estimatedPromptTokens,
      estimated_remaining_tokens: contextUsage.estimatedRemainingTokens,
      system_tokens: systemTokens,
      lore_tokens: loreTokens,
      history_tokens: historyTokens,
      max_output_tokens: contextUsage.budget.maxOutputTokens,
      reserved_response_tokens: contextUsage.budget.reservedResponseTokens,
      safety_margin_tokens: contextUsage.budget.safetyMarginTokens,
      usable_input_tokens: contextUsage.budget.usableInputTokens,
      sampled_event_count: contextUsage.sampledEventCount,
      sampled_message_count: contextUsage.sampledMessageCount,
      token_segments: {
        estimate_quality: contextUsage.estimateQuality,
        estimate_method: contextUsage.estimateMethod,
        estimator_id: contextUsage.estimatorId,
        system_tokens: systemTokens,
        lore_tokens: loreTokens,
        history_tokens: historyTokens,
        prompt_tokens: contextUsage.estimatedPromptTokens,
        reserved_response_tokens: contextUsage.budget.reservedResponseTokens,
        safety_margin_tokens: contextUsage.budget.safetyMarginTokens,
        estimated_remaining_tokens: contextUsage.estimatedRemainingTokens,
        notes: segmentNotes,
      },
    },
    native_snapshot: nativeSnapshot,
    latest_compaction_artifact:
      latestCompactionArtifact === undefined
        ? undefined
        : {
            artifact_id: latestCompactionArtifact.artifact_id,
            strategy_id: latestCompactionArtifact.strategy_id,
            branch_id: latestCompactionArtifact.branch_id,
            enters_future_context:
              latestCompactionArtifact.enters_future_context,
            context_policy: latestCompactionArtifact.context_policy,
            created_at: latestCompactionArtifact.created_at,
            updated_at: latestCompactionArtifact.updated_at,
            estimate_before_json: latestCompactionArtifact.estimate_before_json,
            estimate_after_json: latestCompactionArtifact.estimate_after_json,
          },
    degraded: diagnostics.some((diagnostic) => diagnostic.severity !== "info"),
    diagnostics,
  };
}

function latestNativeContextAccountingSnapshot(
  events: readonly ChatEvent[],
): NativeContextAccountingSnapshot | undefined {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index];
    if (event?.kind !== "provider_status") continue;
    const payload = optionalRecord(event.payload);
    if (payload === undefined) continue;
    const metadataJson = optionalString(payload.metadata_json);
    if (metadataJson === undefined) continue;
    let metadata: unknown;
    try {
      metadata = JSON.parse(metadataJson);
    } catch {
      continue;
    }
    const record = optionalRecord(metadata);
    if (record === undefined) continue;
    if (record.kind !== "context_accounting_snapshot") continue;
    const snapshot = optionalRecord(record.snapshot);
    if (snapshot === undefined) continue;
    if (!isNativeContextAccountingSnapshot(snapshot)) continue;
    return snapshot;
  }
  return undefined;
}

function isNativeContextAccountingSnapshot(
  value: Record<string, unknown>,
): value is NativeContextAccountingSnapshot {
  return (
    value.schemaVersion === 1 &&
    optionalRecord(value.provider) !== undefined &&
    optionalRecord(value.promptProjection) !== undefined &&
    optionalRecord(value.reservedOutput) !== undefined &&
    optionalRecord(value.admission) !== undefined &&
    optionalRecord(value.providerUsage) !== undefined &&
    optionalRecord(value.durableTranscript) !== undefined &&
    optionalRecord(value.providerState) !== undefined &&
    optionalRecord(value.compaction) !== undefined &&
    Array.isArray(value.diagnostics)
  );
}

export async function rustyViewToolCallDebugDetail(
  context: RustyViewChatOperationsContext,
  input: { session: SessionState; debugDetailId: string; requestId: string },
): Promise<ToolCallDebugDetail | undefined> {
  const record = context.toolCallDebugStore.get({
    sessionId: input.session.sessionId,
    debugDetailId: input.debugDetailId,
  });
  if (!record) return undefined;
  return {
    debug_detail_id: record.debug_detail_id,
    tool_call_id: record.tool_call_id,
    session_id: record.session_id,
    wake_id: record.wake_id,
    tool_name: record.tool_name,
    status: record.status,
    arguments: record.arguments,
    partial_updates: record.partial_updates,
    final_result: record.final_result,
    error: record.error,
    source_metadata: record.source_metadata,
    started_at: record.started_at,
    updated_at: record.updated_at,
    expires_at: record.expires_at,
    limits: { ...record.limits },
  };
}

export async function rustyViewProviderRequestDebugDetail(
  context: RustyViewChatOperationsContext,
  input: { session: SessionState; debugDetailId: string; requestId: string },
): Promise<ProviderRequestDebugDetail | undefined> {
  const record = context.providerRequestDebugStore.get({
    sessionId: input.session.sessionId,
    debugDetailId: input.debugDetailId,
  });
  if (!record) return undefined;
  return {
    debug_detail_id: record.debug_detail_id,
    session_id: record.session_id,
    wake_id: record.wake_id,
    provider: record.provider,
    request: record.request,
    request_sha256: record.request_sha256,
    request_json_chars: record.request_json_chars,
    recorded_at: record.recorded_at,
    expires_at: record.expires_at,
    limits: { ...record.limits },
  };
}

function providerBrainBackend(
  provider: NativeModelProviderRecord | undefined,
): string {
  if (provider === undefined) return "unknown";
  return provider.protocol === "responses"
    ? "openai-responses"
    : "chat-completions";
}

function redactedProviderUrl(baseUrl: string | undefined): {
  host?: string;
  redacted?: string;
} {
  if (baseUrl === undefined || baseUrl.trim() === "") return {};
  try {
    const parsed = new URL(baseUrl);
    return { host: parsed.host, redacted: parsed.origin };
  } catch {
    return { redacted: "invalid-url" };
  }
}

export async function listRustyViewMessageSlots(
  context: RustyViewChatOperationsContext,
  input: ListMessageSlotsInput,
): Promise<MessageSlotPage> {
  const page = await context.bridge.queryMessageSlotsPage({
    session_id: input.session.sessionId,
    include_alternates: input.includeAlternates,
    page: { limit: input.limit, offset: input.offset },
  });
  return publicExactPage(page as ExactPageWire<MessageSlotRecord>);
}

export async function queryRustyViewChatSessionSummaries(
  context: RustyViewChatOperationsContext,
  input: ChatSessionSummaryQuery,
): Promise<ChatSessionReadFactsPage> {
  const result = await context.bridge.queryChatSessionSummaries({
    profile_id: input.profileId,
    status: input.status,
    page: { limit: input.limit, offset: input.offset },
  });
  const page = publicExactPage(result.page);
  return {
    ...page,
    items: page.items.map((facts) => ({
      ...facts,
    })),
  };
}

export async function readRustyViewChatSession(
  context: RustyViewChatOperationsContext,
  input: ChatSessionReadInput,
): Promise<ChatSessionReadProjection> {
  const result = await context.bridge.readChatSession({
    session_id: input.sessionId,
    cursor: input.cursor ?? undefined,
    limit: input.limit,
    include_alternates: input.includeAlternates,
  });
  return {
    session: result.session,
    execution: result.execution,
    events: result.events as ChatEvent[],
    latest_cursor: result.latest_cursor,
    has_more: result.has_more,
    has_more_before: result.has_more_before,
    total: result.total,
    message_count: result.message_count,
    source: result.source,
    message_slots: publicExactPage(
      result.message_slots as ExactPageWire<MessageSlotRecord>,
    ),
  };
}

export async function searchRustyViewTranscript(
  context: RustyViewChatOperationsContext,
  input: SearchTranscriptInput,
): Promise<TranscriptSearchResultPage> {
  const query = input.query.trim();
  const result = (await context.bridge.searchChatTranscript({
    scope:
      input.scope === "current_session"
        ? "current_session"
        : "all_conversations",
    session_id: input.session?.sessionId ?? input.sessionId,
    profile_id: input.profileId,
    query,
    author_role: input.role,
    created_after: input.createdAfter,
    created_before: input.createdBefore,
    page: { limit: input.limit, offset: input.offset },
  })) as {
    page: ExactPageWire<TranscriptSearchResultPage["items"][number]>;
  };
  const page = publicExactPage(result.page);
  return {
    ...page,
    items: page.items.map((item) => ({ ...item, scope: input.scope })),
    query,
    scope: input.scope,
    source: "rust_coordination",
  };
}

export async function rustyViewConversationTree(
  context: RustyViewChatOperationsContext,
  input: ConversationTreeInput,
): Promise<ConversationTreeProjection> {
  const result = (await context.bridge.readConversationTree({
    session_id: input.session.sessionId,
    include_snapshots: input.includeSnapshots,
    page: { limit: input.limit, offset: input.offset },
    default_updated_at: context.now(),
  })) as {
    branches: ExactPageWire<ConversationBranchRecord>;
    snapshots: ExactPageWire<ConversationSnapshotRecord>;
    branch_state: ConversationBranchStateRecord;
    active_branch_id?: string | null;
  };
  return {
    branches: result.branches.items,
    snapshots: result.snapshots.items,
    branch_state: result.branch_state,
    active_branch_id: result.active_branch_id,
  };
}

export async function getRustyViewConversationBranchState(
  context: RustyViewChatOperationsContext,
  input: ConversationBranchStateInput,
): Promise<ConversationBranchStateRecord> {
  return (await context.bridge.getConversationBranchState({
    session_id: input.session.sessionId,
    default_updated_at: context.now(),
  })) as ConversationBranchStateRecord;
}

export async function createRustyViewConversationBranch(
  context: RustyViewChatOperationsContext,
  input: CreateConversationBranchInput,
): Promise<ConversationBranchMutationResult> {
  const now = context.now();
  const branchId =
    input.request.branch_id ??
    stableChatRecordId(
      "branch",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const branch = (await context.bridge.createChatConversationBranch({
    branch: {
      branch_id: branchId,
      session_id: input.session.sessionId,
      parent_branch_id: input.request.parent_branch_id ?? null,
      parent_message_id: input.request.parent_message_id ?? null,
      origin_message_id: input.request.origin_message_id ?? null,
      head_message_id: input.request.head_message_id ?? null,
      label: input.request.label ?? null,
      metadata_json: input.request.metadata_json ?? {},
      created_at: now,
      updated_at: now,
    },
  })) as ConversationBranchRecord;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "conversation_branch_created",
    payload: { branch },
  });
  return { status: "created", branch, latest_cursor: event.event_id };
}

export async function selectRustyViewActiveConversationBranch(
  context: RustyViewChatOperationsContext,
  input: SelectActiveConversationBranchInput,
): Promise<SelectActiveConversationBranchResult> {
  const result = (await context.bridge.selectActiveConversationBranch({
    session_id: input.session.sessionId,
    active_branch_id: input.request.active_branch_id ?? null,
    expected: input.request.expected,
    updated_at: context.now(),
  })) as {
    state: ConversationBranchStateRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "selected";
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "conversation_active_branch_selected",
    payload: {
      active_branch_id: result.state.active_branch_id,
      conflict: result.conflict,
      state: result.state,
    },
  });
  return {
    status,
    state: result.state,
    ...(result.conflict ? { conflict: result.conflict } : {}),
    latest_cursor: event.event_id,
  };
}

export async function updateRustyViewConversationBranchHead(
  context: RustyViewChatOperationsContext,
  input: UpdateConversationBranchHeadInput,
): Promise<UpdateConversationBranchHeadResult> {
  const result = (await context.bridge.updateConversationBranchHead({
    branch_id: input.branchId,
    head_message_id: input.request.head_message_id ?? null,
    expected: input.request.expected,
    updated_at: context.now(),
  })) as {
    branch: ConversationBranchRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "updated";
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "conversation_branch_head_updated",
    payload: {
      branch_id: input.branchId,
      head_message_id: result.branch.head_message_id,
      conflict: result.conflict,
      branch: result.branch,
    },
  });
  return {
    status,
    branch: result.branch,
    ...(result.conflict ? { conflict: result.conflict } : {}),
    latest_cursor: event.event_id,
  };
}

export async function createRustyViewConversationSnapshot(
  context: RustyViewChatOperationsContext,
  input: CreateConversationSnapshotInput,
): Promise<ConversationSnapshotMutationResult> {
  const now = context.now();
  const snapshotId =
    input.request.snapshot_id ??
    stableChatRecordId(
      "snapshot",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const result = (await context.bridge.createChatConversationSnapshot({
    snapshot: {
      snapshot_id: snapshotId,
      session_id: input.session.sessionId,
      branch_id: input.request.branch_id ?? null,
      message_id: input.request.message_id ?? null,
      cursor: input.request.cursor ?? null,
      label: input.request.label ?? null,
      summary: input.request.summary ?? null,
      source: input.request.source ?? "user",
      metadata_json: input.request.metadata_json ?? {},
      created_at: now,
      updated_at: now,
    },
  })) as { snapshot: ConversationSnapshotRecord };
  const snapshot = result.snapshot;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "conversation_snapshot_created",
    payload: { snapshot },
  });
  return { status: "created", snapshot, latest_cursor: event.event_id };
}

export async function resolveRustyViewConversationJump(
  context: RustyViewChatOperationsContext,
  input: ResolveConversationJumpInput,
): Promise<ConversationJumpResult> {
  return (await context.bridge.resolveConversationJump({
    session_id: input.session.sessionId,
    target: input.target,
  })) as ConversationJumpResult;
}

export async function createRustyViewAttachment(
  context: RustyViewChatOperationsContext,
  input: CreateAttachmentInput,
): Promise<AttachmentMutationResult> {
  const now = context.now();
  const attachmentId =
    input.request.attachment_id ??
    stableChatRecordId(
      "attachment",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const link = attachmentLinkRecord({
    attachmentId,
    sessionId: input.session.sessionId,
    messageId: input.request.message_id ?? null,
    blockId: input.request.block_id ?? null,
    scopeId: input.request.scope_id ?? null,
    metadataJson: input.request.link_metadata_json ?? {},
    createdAt: now,
  });
  const result = (await context.bridge.createChatAttachment({
    attachment: {
      attachment_id: attachmentId,
      session_id: input.session.sessionId,
      status: "active",
      filename: input.request.filename,
      mime_type: input.request.mime_type,
      byte_size: input.request.byte_size,
      storage_url: input.request.storage_url ?? null,
      download_url: input.request.download_url ?? null,
      thumbnail_url: input.request.thumbnail_url ?? null,
      extracted_text: input.request.extracted_text ?? null,
      extracted_text_truncated: input.request.extracted_text_truncated ?? false,
      metadata_json: input.request.metadata_json ?? {},
      created_at: now,
      updated_at: now,
      expires_at: input.request.expires_at ?? null,
      link: link.message_id || link.block_id || link.scope_id ? link : null,
    },
  })) as {
    status: "created" | "updated" | "linked";
    attachment: AttachmentRecord;
  };
  const attachment = result.attachment;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind:
      result.status === "updated"
        ? "attachment_updated"
        : "attachment_uploaded",
    payload: { attachment },
  });
  if (link.message_id || link.block_id || link.scope_id) {
    await context.appendChatEvent(input.session.sessionId, {
      kind: "attachment_linked",
      payload: { attachment_id: attachmentId, link, attachment },
    });
  }
  return {
    status: result.status,
    attachment,
    latest_cursor: event.event_id,
  };
}

export async function listRustyViewAttachments(
  context: RustyViewChatOperationsContext,
  input: ListAttachmentsInput,
): Promise<AttachmentPage> {
  const page = await context.bridge.queryAttachmentsPage({
    session_id: input.session.sessionId,
    message_id: input.messageId,
    scope_id: input.scopeId,
    include_removed: input.includeRemoved,
    include_expired: false,
    expired_only: false,
    page: { limit: input.limit, offset: input.offset },
  });
  return publicExactPage(page as ExactPageWire<AttachmentRecord>);
}

export async function removeRustyViewAttachment(
  context: RustyViewChatOperationsContext,
  input: RemoveAttachmentInput,
): Promise<AttachmentMutationResult> {
  const removed = (await context.bridge.removeChatAttachment({
    session_id: input.session.sessionId,
    attachment_id: input.attachmentId,
    updated_at: context.now(),
  })) as AttachmentRecord;
  let contentRemovalError: string | undefined;
  await context.toolMediaAttachments.removeContent(removed).catch((error) => {
    contentRemovalError = errorMessage(
      error,
      "attachment content removal failed",
    );
  });
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "attachment_removed",
    payload: {
      attachment_id: input.attachmentId,
      attachment: removed,
      ...(contentRemovalError === undefined
        ? {}
        : { content_removal_error: contentRemovalError }),
    },
  });
  return {
    status: "removed",
    attachment: removed,
    latest_cursor: event.event_id,
  };
}

export async function createRustyViewDataBankScope(
  context: RustyViewChatOperationsContext,
  input: CreateDataBankScopeInput,
): Promise<DataBankScopeMutationResult> {
  const now = context.now();
  const scopeId =
    input.request.scope_id ??
    stableChatRecordId(
      "scope",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const result = (await context.bridge.createChatDataBankScope({
    scope: {
      scope_id: scopeId,
      session_id: input.session.sessionId,
      status: "active",
      label: input.request.label ?? null,
      description: input.request.description ?? null,
      metadata_json: input.request.metadata_json ?? {},
      created_at: now,
      updated_at: now,
    },
  })) as {
    status: "created" | "updated";
    scope: DataBankScopeRecord;
  };
  const scope = result.scope;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "data_bank_scope_created",
    payload: { scope },
  });
  return {
    status: result.status,
    scope,
    latest_cursor: event.event_id,
  };
}

export async function listRustyViewDataBankScopes(
  context: RustyViewChatOperationsContext,
  input: ListDataBankScopesInput,
): Promise<DataBankScopePage> {
  const page = await context.bridge.queryDataBankScopesPage({
    session_id: input.session.sessionId,
    include_removed: input.includeRemoved,
    page: { limit: input.limit, offset: input.offset },
  });
  return publicExactPage(page as ExactPageWire<DataBankScopeRecord>);
}

export async function removeRustyViewDataBankScope(
  context: RustyViewChatOperationsContext,
  input: RemoveDataBankScopeInput,
): Promise<DataBankScopeMutationResult> {
  const removed = (await context.bridge.removeChatDataBankScope({
    session_id: input.session.sessionId,
    scope_id: input.scopeId,
    updated_at: context.now(),
  })) as DataBankScopeRecord;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "data_bank_scope_removed",
    payload: { scope_id: input.scopeId, scope: removed },
  });
  return { status: "removed", scope: removed, latest_cursor: event.event_id };
}

export async function listRustyViewMessageVariants(
  context: RustyViewChatOperationsContext,
  input: ListMessageVariantsInput,
): Promise<MessageVariantPage> {
  const page = await context.bridge.queryMessageVariantsPage({
    session_id: input.session.sessionId,
    slot_id: input.slotId,
    include_deleted: false,
    page: { limit: input.limit, offset: input.offset },
  });
  const exact = publicExactPage(page as ExactPageWire<MessageVariantRecord>);
  return {
    items: exact.items,
    total: exact.total,
    limit: exact.limit,
    offset: exact.offset,
  };
}

export async function createRustyViewMessageSlot(
  context: RustyViewChatOperationsContext,
  input: CreateMessageSlotInput,
): Promise<MessageSlotMutationResult> {
  const now = context.now();
  const slotId =
    input.request.slot_id ??
    stableChatRecordId("slot", `${input.session.sessionId}:${input.requestId}`);
  const variantId =
    input.request.primary_variant_id ?? stableChatRecordId("variant", slotId);
  const fallbackBranchId = stableChatRecordId(
    "branch",
    `${input.session.sessionId}:default`,
  );
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    context.roleplayRouteContext(),
    input.session,
    input.request.actor,
    now,
  ).catch(() => undefined);
  const slotMetadata = {
    ...(optionalRecord(input.request.metadata_json) ?? {}),
    ...(speakerIdentity === undefined
      ? {}
      : { speaker_identity: speakerIdentity }),
  };
  const variantMetadata = {
    ...(optionalRecord(input.request.variant_metadata_json) ?? {}),
    ...(speakerIdentity === undefined
      ? {}
      : { speaker_identity: speakerIdentity }),
  };
  const messageId =
    input.request.message_id ?? stableChatRecordId("message", variantId);
  const result = (await context.bridge.createChatMessageSlot({
    slot: {
      slot_id: slotId,
      session_id: input.session.sessionId,
      primary_variant_id: variantId,
      active_variant_id: null,
      metadata_json: slotMetadata,
      created_at: now,
      updated_at: now,
    },
    primary_variant: messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId,
      variantId,
      messageId,
      source: "primary",
      ordinal: 0,
      actor: input.request.actor,
      body: input.request.body,
      branchId: fallbackBranchId,
      metadataJson: variantMetadata,
      blocks: input.request.blocks,
      now,
    }),
    branch_id: fallbackBranchId,
    expected_branch_head: { type: "any" },
    updated_at: context.now(),
    ensure_active_branch: {
      session_id: input.session.sessionId,
      branch_id: fallbackBranchId,
      label: "Default",
      metadata_json: { source: "rusty_view_chat_default" },
      created_at: now,
      updated_at: now,
    },
    inherit_branch_head: true,
    idempotency_key: null,
  })) as {
    slot?: MessageSlotRecord | null;
    branch: ConversationBranchRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  if (result.conflict || !result.slot) {
    return {
      status: "conflict",
      branch: result.branch,
      conflict: result.conflict ?? { expected: null, actual: null },
    };
  }
  const slot = result.slot;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "message_slot_created",
    payload: { slot },
  });
  return { status: "created", slot, latest_cursor: event.event_id };
}

export async function createRustyViewMessageVariant(
  context: RustyViewChatOperationsContext,
  input: CreateMessageVariantInput,
): Promise<MessageVariantMutationResult> {
  const now = context.now();
  const variantId =
    input.request.variant_id ??
    stableChatRecordId("variant", `${input.slotId}:${input.requestId}`);
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    context.roleplayRouteContext(),
    input.session,
    input.request.actor,
    now,
  ).catch(() => undefined);
  const result = (await context.bridge.createChatMessageVariant({
    session_id: input.session.sessionId,
    slot_id: input.slotId,
    variant: messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId: input.slotId,
      variantId,
      messageId:
        input.request.message_id ?? stableChatRecordId("message", variantId),
      source: "alternate",
      ordinal: 0,
      actor: input.request.actor,
      body: input.request.body,
      metadataJson: {
        ...(optionalRecord(input.request.metadata_json) ?? {}),
        ...(speakerIdentity === undefined
          ? {}
          : { speaker_identity: speakerIdentity }),
      },
      blocks: input.request.blocks,
      now,
    }),
  })) as { variant: MessageVariantRecord };
  const variant = result.variant;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "message_variant_created",
    payload: { slot_id: input.slotId, variant },
  });
  return { status: "created", variant, latest_cursor: event.event_id };
}

export async function deleteRustyViewMessageVariant(
  context: RustyViewChatOperationsContext,
  input: DeleteMessageVariantInput,
): Promise<MessageSlotMutationResult> {
  const slot = (await context.bridge.deleteChatMessageVariant({
    session_id: input.session.sessionId,
    slot_id: input.slotId,
    variant_id: input.variantId,
    updated_at: context.now(),
  })) as MessageSlotRecord;
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "message_variant_deleted",
    payload: { slot_id: input.slotId, variant_id: input.variantId, slot },
  });
  return { status: "deleted", slot, latest_cursor: event.event_id };
}

export async function reorderRustyViewMessageVariants(
  context: RustyViewChatOperationsContext,
  input: ReorderMessageVariantsInput,
): Promise<MessageVariantsReorderResult> {
  const variants = (await context.bridge.reorderChatMessageVariants({
    session_id: input.session.sessionId,
    slot_id: input.slotId,
    ordered_variant_ids: input.orderedVariantIds,
    updated_at: context.now(),
  })) as MessageVariantRecord[];
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "message_variants_reordered",
    payload: {
      slot_id: input.slotId,
      ordered_variant_ids: input.orderedVariantIds,
      variants,
    },
  });
  return { status: "reordered", variants, latest_cursor: event.event_id };
}

export async function selectRustyViewActiveMessageVariant(
  context: RustyViewChatOperationsContext,
  input: SelectActiveMessageVariantInput,
): Promise<SelectActiveMessageVariantResult> {
  const result = (await context.bridge.selectActiveChatMessageVariant({
    session_id: input.session.sessionId,
    slot_id: input.slotId,
    active_variant_id: input.request.active_variant_id ?? null,
    expected: input.request.expected,
    updated_at: context.now(),
  })) as {
    slot: MessageSlotRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "selected";
  const event = await context.appendChatEvent(input.session.sessionId, {
    kind: "message_active_variant_selected",
    payload: {
      slot_id: input.slotId,
      active_variant_id: result.slot.active_variant_id,
      conflict: result.conflict,
      slot: result.slot,
    },
  });
  return {
    status,
    slot: result.slot,
    ...(result.conflict ? { conflict: result.conflict } : {}),
    latest_cursor: event.event_id,
  };
}

function messageVariantWrite(input: {
  sessionId: SessionId;
  slotId: string;
  variantId: string;
  messageId: string;
  source: "primary" | "alternate";
  ordinal: number;
  actor: { id: string; kind: "human" | "agent" | "system" };
  body: string;
  branchId?: string | null;
  parentMessageId?: string | null;
  previousMessageId?: string | null;
  metadataJson: unknown;
  blocks?: MessageBlockDraft[];
  now: string;
}): Record<string, unknown> {
  return {
    variant_id: input.variantId,
    slot_id: input.slotId,
    source: input.source,
    ordinal: input.ordinal,
    status: "active",
    message: {
      message_id: input.messageId,
      session_id: input.sessionId,
      branch_id: input.branchId ?? null,
      parent_message_id: input.parentMessageId ?? null,
      previous_message_id: input.previousMessageId ?? null,
      author_id: input.actor.id,
      author_role:
        input.actor.kind === "agent"
          ? "assistant"
          : input.actor.kind === "system"
            ? "system"
            : "user",
      status: "completed",
      body: input.body,
      metadata_json: input.metadataJson ?? {},
      created_at: input.now,
      blocks: messageBlockWrites(input.messageId, input.body, input.blocks),
    },
    metadata_json: input.metadataJson ?? {},
    created_at: input.now,
    updated_at: input.now,
  };
}

function messageBlockWrites(
  messageId: string,
  body: string,
  blocks: MessageBlockDraft[] | undefined,
): Array<Record<string, unknown>> {
  const source =
    blocks && blocks.length > 0
      ? blocks
      : [{ kind: "text", content_json: { text: body }, metadata_json: {} }];
  return source.map((block, index) => ({
    block_id: block.block_id ?? `${messageId}:block:${index + 1}`,
    ordinal: index,
    kind: block.kind,
    content_json: block.content_json,
    render_policy_json: block.render_policy_json,
    metadata_json: block.metadata_json ?? {},
  }));
}

function stableChatRecordId(prefix: string, raw: string): string {
  return `${prefix}:${raw.replace(/[^A-Za-z0-9._:-]+/g, "_").slice(0, 160)}`;
}

function attachmentBlockId(messageId: string, attachmentId: string): string {
  return stableChatRecordId("attachment-block", `${messageId}:${attachmentId}`);
}

interface ExactPageWire<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  next_offset?: number | null;
}

function publicExactPage<T>(page: ExactPageWire<T>): {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number;
} {
  return {
    items: page.items,
    total: page.total,
    limit: page.limit,
    offset: page.offset,
    ...(page.next_offset === undefined || page.next_offset === null
      ? {}
      : { nextOffset: page.next_offset }),
  };
}

function attachmentLinkRecord(input: {
  attachmentId: string;
  sessionId: SessionId;
  messageId?: string | null;
  blockId?: string | null;
  scopeId?: string | null;
  metadataJson: unknown;
  createdAt: string;
}): AttachmentRecord["links"][number] {
  const target = [
    input.messageId ?? "no-message",
    input.blockId ?? "no-block",
    input.scopeId ?? "no-scope",
  ].join(":");
  return {
    link_id: stableChatRecordId(
      "attachment-link",
      `${input.attachmentId}:${target}`,
    ),
    attachment_id: input.attachmentId,
    session_id: input.sessionId,
    message_id: input.messageId ?? null,
    block_id: input.blockId ?? null,
    scope_id: input.scopeId ?? null,
    metadata_json: input.metadataJson,
    created_at: input.createdAt,
  };
}

type ProfileRuntimeToolPolicy = {
  requestedToolsets?: string[];
  requestedTools?: string[];
  deniedTools?: string[];
  includeDeprecated?: boolean;
};

function profileToolPolicyFromUnknown(
  value: unknown,
): ProfileRuntimeToolPolicy | undefined {
  const policy = optionalRecord(value);
  if (policy === undefined) return undefined;
  return {
    requestedToolsets:
      policy.requestedToolsets === undefined
        ? undefined
        : stringArray(policy.requestedToolsets, "toolPolicy.requestedToolsets"),
    requestedTools:
      policy.requestedTools === undefined
        ? undefined
        : stringArray(policy.requestedTools, "toolPolicy.requestedTools"),
    deniedTools:
      policy.deniedTools === undefined
        ? undefined
        : stringArray(policy.deniedTools, "toolPolicy.deniedTools"),
    includeDeprecated:
      typeof policy.includeDeprecated === "boolean"
        ? policy.includeDeprecated
        : undefined,
  };
}

type ProfileRuntimeBrainMetadata = { module?: string; strategy?: string };

function brainMetadataFromUnknown(
  value: unknown,
): ProfileRuntimeBrainMetadata | undefined {
  const brain = optionalRecord(value);
  if (brain === undefined) return undefined;
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as ProfileRuntimeBrainMetadata;
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`${fieldName} must be an array`);
  }
  return value.map((entry, index) => {
    if (typeof entry !== "string" || !entry.trim()) {
      throw new Error(`${fieldName}[${index}] must be a non-empty string`);
    }
    return entry.trim();
  });
}

function compactRecord<T extends Record<string, unknown>>(record: T): T {
  for (const key of Object.keys(record)) {
    if (record[key] === undefined) delete record[key];
  }
  return record;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.trim()
    ? error.message
    : fallback;
}
