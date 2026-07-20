import type {
  NativeProfileRegistryLifecycleStatus,
  NativeProfileRegistrySourceAssetRef,
  NativeProfileRegistryDerivedRuntimeRef,
  NativeProfileRegistryImportExportMetadata,
  NativeProfileRegistryRecord,
  NativeProfileRegistryQuery,
  NativeProfilePurgeReport,
  NativeModelProviderStatus,
  NativeModelProviderProtocol,
  NativeModelProviderCredentialKind,
  NativeModelProviderRecord,
  NativeModelProviderWrite,
  NativeModelProviderQuery,
  NativeModelProviderRefreshImpact,
  NativeModelProviderRefreshMode,
  NativeModelProviderRefreshPlan,
  NativeRuntimeConfigDiagnostic,
  NativeProfileRegistryWrite,
  NativeProfileRegistryUpdate,
  NativeProfileRegistryMutationRequest,
  NativeProfileRegistryMutationPlan,
} from "./public-api.js";

export function toNativeProfileRegistryWrite(
  write: RawProfileRegistryWrite,
): NativeProfileRegistryWrite {
  return {
    profileId: write.profile_id,
    lifecycleStatus: write.lifecycle_status,
    displayName: write.display_name ?? undefined,
    summary: write.summary ?? undefined,
    defaultSessionKind: write.default_session_kind ?? undefined,
    agentId: write.agent_id ?? undefined,
    ownerId: write.owner_id ?? undefined,
    promptSoulMarkdown: write.prompt_soul_markdown ?? undefined,
    promptMemoryMarkdown: write.prompt_memory_markdown ?? undefined,
    activeRuntimeSettingsJson: write.active_runtime_settings_json,
    sourceAssetRefs: write.source_asset_refs.map(
      toNativeProfileRegistryAssetRef,
    ),
    derivedRuntimeRefs: write.derived_runtime_refs.map(
      toNativeProfileRegistryRuntimeRef,
    ),
    importExport: toNativeProfileRegistryImportExport(write.import_export),
    now: write.now,
  };
}

export function toRawProfileRegistryQuery(
  query: NativeProfileRegistryQuery,
): RawProfileRegistryQuery {
  return {
    lifecycle_status: query.lifecycleStatus,
    limit: query.limit,
    offset: query.offset,
  };
}

export function toRawProfileRegistryWrite(
  write: NativeProfileRegistryWrite,
): RawProfileRegistryWrite {
  return {
    profile_id: write.profileId,
    lifecycle_status: write.lifecycleStatus,
    display_name: write.displayName,
    summary: write.summary,
    default_session_kind: write.defaultSessionKind,
    agent_id: write.agentId,
    owner_id: write.ownerId,
    prompt_soul_markdown: write.promptSoulMarkdown,
    prompt_memory_markdown: write.promptMemoryMarkdown,
    active_runtime_settings_json: write.activeRuntimeSettingsJson,
    source_asset_refs: write.sourceAssetRefs.map(toRawProfileRegistryAssetRef),
    derived_runtime_refs: write.derivedRuntimeRefs.map(
      toRawProfileRegistryRuntimeRef,
    ),
    import_export: toRawProfileRegistryImportExport(write.importExport),
    now: write.now,
  };
}

export function toRawProfileRegistryUpdate(
  update: NativeProfileRegistryUpdate,
): RawProfileRegistryUpdate {
  return {
    write: toRawProfileRegistryWrite(update.write),
    expected_revision: update.expectedRevision,
  };
}

export function toRawProfileRegistryMutationRequest(
  request: NativeProfileRegistryMutationRequest,
): RawProfileRegistryMutationRequest {
  return {
    profile_id: request.profileId,
    kind: request.kind,
    mode: request.mode,
    current: toRawProfileRegistryRecord(request.current),
    body_json: request.bodyJson,
    now: request.now,
  };
}

export function toNativeProfileRegistryMutationPlan(
  plan: RawProfileRegistryMutationPlan,
): NativeProfileRegistryMutationPlan {
  return {
    ok: plan.ok,
    profileId: plan.profile_id,
    kind: plan.kind,
    mode: plan.mode,
    expectedRevision: plan.expected_revision,
    current: toNativeProfileRegistryRecord(plan.current),
    next: toNativeProfileRegistryRecord(plan.next),
    nextWrite: toNativeProfileRegistryWrite(plan.next_write),
    diagnostics: plan.diagnostics.map((diagnostic) => ({
      ...diagnostic,
      path: diagnostic.path ?? "",
    })),
    implications: {
      registryRevisionWillIncrement:
        plan.implications.registry_revision_will_increment,
      profileFilesUnchanged: plan.implications.profile_files_unchanged,
      serviceConfigUnchanged: plan.implications.service_config_unchanged,
      runtimeRebuildRecommended: plan.implications.runtime_rebuild_recommended,
      lifecycleEffects: plan.implications.lifecycle_effects,
    },
  };
}

export function toNativeProfileRegistryRecord(
  record: RawProfileRegistryRecord,
): NativeProfileRegistryRecord {
  return {
    profileId: record.profile_id,
    lifecycleStatus: record.lifecycle_status,
    displayName: record.display_name ?? undefined,
    summary: record.summary ?? undefined,
    defaultSessionKind: record.default_session_kind ?? undefined,
    agentId: record.agent_id ?? undefined,
    ownerId: record.owner_id ?? undefined,
    promptSoulMarkdown: record.prompt_soul_markdown ?? undefined,
    promptMemoryMarkdown: record.prompt_memory_markdown ?? undefined,
    activeRuntimeSettingsJson: record.active_runtime_settings_json,
    sourceAssetRefs: record.source_asset_refs.map(
      toNativeProfileRegistryAssetRef,
    ),
    derivedRuntimeRefs: record.derived_runtime_refs.map(
      toNativeProfileRegistryRuntimeRef,
    ),
    importExport: toNativeProfileRegistryImportExport(record.import_export),
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
  };
}

export function toRawProfileRegistryRecord(
  record: NativeProfileRegistryRecord,
): RawProfileRegistryRecord {
  return {
    profile_id: record.profileId,
    lifecycle_status: record.lifecycleStatus,
    display_name: record.displayName,
    summary: record.summary,
    default_session_kind: record.defaultSessionKind,
    agent_id: record.agentId,
    owner_id: record.ownerId,
    prompt_soul_markdown: record.promptSoulMarkdown,
    prompt_memory_markdown: record.promptMemoryMarkdown,
    active_runtime_settings_json: record.activeRuntimeSettingsJson,
    source_asset_refs: record.sourceAssetRefs.map(toRawProfileRegistryAssetRef),
    derived_runtime_refs: record.derivedRuntimeRefs.map(
      toRawProfileRegistryRuntimeRef,
    ),
    import_export: toRawProfileRegistryImportExport(record.importExport),
    revision: record.revision,
    created_at: record.createdAt,
    updated_at: record.updatedAt,
  };
}

export function toNativeProfilePurgeReport(
  report: RawProfilePurgeReport,
): NativeProfilePurgeReport {
  return {
    profileId: report.profile_id,
    profileRegistryDeleted: report.profile_registry_deleted,
    sessionIds: report.session_ids,
    agentIds: report.agent_ids,
    tableCounts: report.table_counts.map((count) => ({
      table: count.table,
      rowsDeleted: count.rows_deleted,
    })),
    rowsDeleted: report.rows_deleted,
  };
}

export function toRawModelProviderQuery(
  query: NativeModelProviderQuery,
): RawModelProviderQuery {
  return {
    status: query.status,
    alias_prefix: query.aliasPrefix,
    limit: query.limit,
    offset: query.offset,
  };
}

export function toRawModelProviderWrite(
  write: NativeModelProviderWrite,
): RawModelProviderWrite {
  return {
    alias: write.alias,
    status: write.status,
    protocol: write.protocol,
    provider_kind: write.providerKind,
    display_name: write.displayName,
    description: write.description,
    base_url: write.baseUrl,
    model_id: write.modelId,
    context_window_tokens: write.contextWindowTokens,
    max_output_tokens: write.maxOutputTokens,
    temperature_milli: write.temperatureMilli,
    reasoning_effort: write.reasoningEffort,
    reasoning_format: write.reasoningFormat,
    chat_completions_dialect: write.chatCompletionsDialect ?? "standard",
    thinking_mode: write.thinkingMode ?? "provider_default",
    reasoning_history: write.reasoningHistory ?? "provider_default",
    reasoning_budget_tokens: write.reasoningBudgetTokens,
    secret: write.secret,
    clear_secret: write.clearSecret ?? false,
    expected_credential_revision: write.expectedCredentialRevision,
    metadata_json: write.metadataJson ?? {},
    expected_revision: write.expectedRevision,
    now: write.now,
  };
}

export function toNativeModelProviderRefreshImpact(
  impact: RawModelProviderRefreshImpact,
): NativeModelProviderRefreshImpact {
  return {
    providerAlias: impact.provider_alias,
    affectedProfiles: impact.affected_profiles.map((profile) => ({
      profileId: profile.profile_id,
      sessionIds: profile.session_ids,
      configuredSessionIds: profile.configured_session_ids,
      activeSessionIds: profile.active_session_ids,
    })),
  };
}

export function toRawModelProviderRefreshImpact(
  impact: NativeModelProviderRefreshImpact,
): RawModelProviderRefreshImpact {
  return {
    provider_alias: impact.providerAlias,
    affected_profiles: impact.affectedProfiles.map((profile) => ({
      profile_id: profile.profileId,
      session_ids: profile.sessionIds,
      configured_session_ids: profile.configuredSessionIds,
      active_session_ids: profile.activeSessionIds,
    })),
  };
}

export function toNativeModelProviderRefreshPlan(
  plan: RawModelProviderRefreshPlan,
): NativeModelProviderRefreshPlan {
  return {
    providerAlias: plan.provider_alias,
    mode: plan.mode,
    affectedProfiles: plan.affected_profiles.map((profile) => ({
      profileId: profile.profile_id,
      sessionIds: profile.session_ids,
      configuredSessionIds: profile.configured_session_ids,
      activeSessionIds: profile.active_session_ids,
    })),
    actions: plan.actions.map((action) => ({
      profileId: action.profile_id,
      commandName: action.command_name,
      reason: action.reason,
      plannedSummary: action.planned_summary,
      appliedSummary: action.applied_summary,
      blockedSummary: action.blocked_summary,
      failureReasonCode: action.failure_reason_code,
    })),
  };
}

export function toNativeModelProviderRecord(
  record: RawModelProviderRecord,
): NativeModelProviderRecord {
  return {
    alias: record.alias,
    status: record.status,
    protocol: record.protocol,
    providerKind: record.provider_kind,
    displayName: record.display_name ?? undefined,
    description: record.description ?? undefined,
    baseUrl: record.base_url ?? undefined,
    modelId: record.model_id,
    contextWindowTokens: record.context_window_tokens ?? undefined,
    maxOutputTokens: record.max_output_tokens ?? undefined,
    temperatureMilli: record.temperature_milli ?? undefined,
    reasoningEffort: record.reasoning_effort ?? undefined,
    reasoningFormat: record.reasoning_format ?? undefined,
    chatCompletionsDialect: record.chat_completions_dialect,
    thinkingMode: record.thinking_mode,
    reasoningHistory: record.reasoning_history,
    reasoningBudgetTokens: record.reasoning_budget_tokens ?? undefined,
    credentialId: record.credential_id ?? undefined,
    credential: {
      hasSecret: record.credential.has_secret,
      secretRef: record.credential.secret_ref ?? undefined,
      updatedAt: record.credential.updated_at ?? undefined,
      kind: record.credential.kind ?? undefined,
      revision: record.credential.revision ?? undefined,
    },
    metadataJson: record.metadata_json,
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
  };
}

export function toRawModelProviderRecord(
  record: NativeModelProviderRecord,
): RawModelProviderRecord {
  return {
    alias: record.alias,
    status: record.status,
    protocol: record.protocol,
    provider_kind: record.providerKind,
    display_name: record.displayName,
    description: record.description,
    base_url: record.baseUrl,
    model_id: record.modelId,
    context_window_tokens: record.contextWindowTokens,
    max_output_tokens: record.maxOutputTokens,
    temperature_milli: record.temperatureMilli,
    reasoning_effort: record.reasoningEffort,
    reasoning_format: record.reasoningFormat,
    chat_completions_dialect: record.chatCompletionsDialect,
    thinking_mode: record.thinkingMode,
    reasoning_history: record.reasoningHistory,
    reasoning_budget_tokens: record.reasoningBudgetTokens,
    credential_id: record.credentialId,
    credential: {
      has_secret: record.credential.hasSecret,
      secret_ref: record.credential.secretRef,
      updated_at: record.credential.updatedAt,
      kind: record.credential.kind,
      revision: record.credential.revision,
    },
    metadata_json: record.metadataJson,
    revision: record.revision,
    created_at: record.createdAt,
    updated_at: record.updatedAt,
  };
}

export function toNativeProfileRegistryAssetRef(
  ref: RawProfileRegistrySourceAssetRef,
): NativeProfileRegistrySourceAssetRef {
  return {
    assetKind: ref.asset_kind,
    path: ref.path,
    contentHash: ref.content_hash ?? undefined,
    lastSeenAt: ref.last_seen_at ?? undefined,
    metadataJson: ref.metadata_json,
  };
}

export function toRawProfileRegistryAssetRef(
  ref: NativeProfileRegistrySourceAssetRef,
): RawProfileRegistrySourceAssetRef {
  return {
    asset_kind: ref.assetKind,
    path: ref.path,
    content_hash: ref.contentHash,
    last_seen_at: ref.lastSeenAt,
    metadata_json: ref.metadataJson,
  };
}

export function toNativeProfileRegistryRuntimeRef(
  ref: RawProfileRegistryDerivedRuntimeRef,
): NativeProfileRegistryDerivedRuntimeRef {
  return {
    refKind: ref.ref_kind,
    refId: ref.ref_id,
    status: ref.status,
    updatedAt: ref.updated_at ?? undefined,
    metadataJson: ref.metadata_json,
  };
}

export function toRawProfileRegistryRuntimeRef(
  ref: NativeProfileRegistryDerivedRuntimeRef,
): RawProfileRegistryDerivedRuntimeRef {
  return {
    ref_kind: ref.refKind,
    ref_id: ref.refId,
    status: ref.status,
    updated_at: ref.updatedAt,
    metadata_json: ref.metadataJson,
  };
}

export function toNativeProfileRegistryImportExport(
  metadata: RawProfileRegistryImportExportMetadata,
): NativeProfileRegistryImportExportMetadata {
  return {
    importedFrom: metadata.imported_from ?? undefined,
    importedAt: metadata.imported_at ?? undefined,
    exportedTo: metadata.exported_to ?? undefined,
    exportedAt: metadata.exported_at ?? undefined,
    metadataJson: metadata.metadata_json,
  };
}

export function toRawProfileRegistryImportExport(
  metadata: NativeProfileRegistryImportExportMetadata,
): RawProfileRegistryImportExportMetadata {
  return {
    imported_from: metadata.importedFrom,
    imported_at: metadata.importedAt,
    exported_to: metadata.exportedTo,
    exported_at: metadata.exportedAt,
    metadata_json: metadata.metadataJson,
  };
}

export interface RawProfileRegistryWrite {
  profile_id: string;
  lifecycle_status: NativeProfileRegistryLifecycleStatus;
  display_name?: string;
  summary?: string;
  default_session_kind?: "full" | "worker" | "delegated";
  agent_id?: string;
  owner_id?: string;
  prompt_soul_markdown?: string;
  prompt_memory_markdown?: string;
  active_runtime_settings_json: unknown;
  source_asset_refs: RawProfileRegistrySourceAssetRef[];
  derived_runtime_refs: RawProfileRegistryDerivedRuntimeRef[];
  import_export: RawProfileRegistryImportExportMetadata;
  now: string;
}

export interface RawProfileRegistryUpdate {
  write: RawProfileRegistryWrite;
  expected_revision: number;
}

export interface RawProfileRegistryMutationRequest {
  profile_id: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  current: RawProfileRegistryRecord;
  body_json: unknown;
  now: string;
}

export interface RawProfileRegistryMutationPlan {
  ok: boolean;
  profile_id: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  expected_revision: number;
  current: RawProfileRegistryRecord;
  next: RawProfileRegistryRecord;
  next_write: RawProfileRegistryWrite;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  implications: RawProfileRegistryMutationImplications;
}

export interface RawProfileRegistryMutationImplications {
  registry_revision_will_increment: boolean;
  profile_files_unchanged: boolean;
  service_config_unchanged: boolean;
  runtime_rebuild_recommended: boolean;
  lifecycle_effects: "none" | "archive_active_sessions_and_unregister_brain";
}

export interface RawProfileRegistryQuery {
  lifecycle_status?: NativeProfileRegistryLifecycleStatus;
  limit?: number;
  offset?: number;
}

export interface RawProfileRegistrySourceAssetRef {
  asset_kind: string;
  path: string;
  content_hash?: string | null;
  last_seen_at?: string | null;
  metadata_json: unknown;
}

export interface RawProfileRegistryDerivedRuntimeRef {
  ref_kind: string;
  ref_id: string;
  status: string;
  updated_at?: string | null;
  metadata_json: unknown;
}

export interface RawProfileRegistryImportExportMetadata {
  imported_from?: string | null;
  imported_at?: string | null;
  exported_to?: string | null;
  exported_at?: string | null;
  metadata_json: unknown;
}

export interface RawProfileRegistryRecord {
  profile_id: string;
  lifecycle_status: NativeProfileRegistryLifecycleStatus;
  display_name?: string | null;
  summary?: string | null;
  default_session_kind?: "full" | "worker" | "delegated" | null;
  agent_id?: string | null;
  owner_id?: string | null;
  prompt_soul_markdown?: string | null;
  prompt_memory_markdown?: string | null;
  active_runtime_settings_json: unknown;
  source_asset_refs: RawProfileRegistrySourceAssetRef[];
  derived_runtime_refs: RawProfileRegistryDerivedRuntimeRef[];
  import_export: RawProfileRegistryImportExportMetadata;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface RawProfilePurgeReport {
  profile_id: string;
  profile_registry_deleted: boolean;
  session_ids: string[];
  agent_ids: string[];
  table_counts: RawProfilePurgeTableCount[];
  rows_deleted: number;
}

export interface RawProfilePurgeTableCount {
  table: string;
  rows_deleted: number;
}

export interface RawModelProviderCredential {
  has_secret: boolean;
  secret_ref?: string | null;
  updated_at?: string | null;
  kind?: NativeModelProviderCredentialKind | null;
  revision?: number | null;
}

export interface RawModelProviderRecord {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  provider_kind: string;
  display_name?: string | null;
  description?: string | null;
  base_url?: string | null;
  model_id: string;
  context_window_tokens?: number | null;
  max_output_tokens?: number | null;
  temperature_milli?: number | null;
  reasoning_effort?: string | null;
  reasoning_format?: string | null;
  chat_completions_dialect: NativeModelProviderRecord["chatCompletionsDialect"];
  thinking_mode: NativeModelProviderRecord["thinkingMode"];
  reasoning_history: NativeModelProviderRecord["reasoningHistory"];
  reasoning_budget_tokens?: number | null;
  credential_id?: string | null;
  credential: RawModelProviderCredential;
  metadata_json: unknown;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface RawModelProviderWrite {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  provider_kind: string;
  display_name?: string;
  description?: string;
  base_url?: string;
  model_id: string;
  context_window_tokens?: number;
  max_output_tokens?: number;
  temperature_milli?: number;
  reasoning_effort?: string;
  reasoning_format?: string;
  chat_completions_dialect: NonNullable<
    NativeModelProviderWrite["chatCompletionsDialect"]
  >;
  thinking_mode: NonNullable<NativeModelProviderWrite["thinkingMode"]>;
  reasoning_history: NonNullable<NativeModelProviderWrite["reasoningHistory"]>;
  reasoning_budget_tokens?: number;
  secret?: string;
  clear_secret: boolean;
  expected_credential_revision?: number;
  metadata_json: unknown;
  expected_revision?: number;
  now: string;
}

export interface RawModelProviderQuery {
  status?: NativeModelProviderStatus;
  alias_prefix?: string;
  limit?: number;
  offset?: number;
}

export interface RawModelProviderAffectedProfile {
  profile_id: string;
  session_ids: string[];
  configured_session_ids: string[];
  active_session_ids: string[];
}

export interface RawModelProviderRefreshImpact {
  provider_alias: string;
  affected_profiles: RawModelProviderAffectedProfile[];
}

export interface RawModelProviderRefreshProfileAction {
  profile_id: string;
  command_name: string;
  reason: string;
  planned_summary: string;
  applied_summary: string;
  blocked_summary: string;
  failure_reason_code: string;
}

export interface RawModelProviderRefreshPlan {
  provider_alias: string;
  mode: NativeModelProviderRefreshMode;
  affected_profiles: RawModelProviderAffectedProfile[];
  actions: RawModelProviderRefreshProfileAction[];
}
