use rusty_crew_core_bridge_api::SessionId;
use rusty_crew_core_protocol::{AttachmentId, DataBankScopeId, MessageSlotId, MessageVariantId};
use serde::Deserialize;

#[napi_derive::napi(object)]
pub struct JsEngineConfig {
    pub engine_data_dir: String,
    pub fixed_clock: Option<String>,
    pub default_turn_budget: u32,
    pub default_idle_timeout_ms: u32,
    pub storage_backend: Option<String>,
    pub postgres_database_url: Option<String>,
    pub postgres_schema: Option<String>,
    pub postgres_max_connections: Option<u32>,
    pub postgres_statement_timeout_ms: Option<u32>,
    pub backing_filesystem_path: Option<String>,
    pub filesystem_warning_free_percent: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsToolProfile {
    pub tools: Vec<JsToolDescriptor>,
}

#[napi_derive::napi(object)]
pub struct JsResourceLimits {
    pub max_duration_ms: Option<u32>,
    pub max_delegation_depth: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsSessionWorkspace {
    pub cwd: String,
    pub revision: f64,
    pub updated_at: String,
}

#[napi_derive::napi(object)]
pub struct JsBrainModelConfig {
    pub model_config_id: Option<String>,
    pub model_config_revision: Option<f64>,
    pub endpoint_id: Option<String>,
    pub endpoint_revision: Option<f64>,
    pub credential_id: Option<String>,
    pub credential_revision: Option<f64>,
    pub credential_kind: Option<String>,
    pub protocol: Option<String>,
    pub dialect: Option<String>,
    pub auth_scheme: Option<String>,
    pub prompt_cache_transport: Option<String>,
    pub provider: String,
    pub model_name: String,
    pub temperature_milli: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsBrainProviderStateStrategyMetadata {
    pub mode: String,
}

#[napi_derive::napi(object)]
pub struct JsBrainStrategyMetadata {
    pub module_id: String,
    pub strategy_id: String,
    pub provider_state: JsBrainProviderStateStrategyMetadata,
}

#[napi_derive::napi(object)]
pub struct JsBrainProviderStateScope {
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub compatibility: Option<JsProviderStateCompatibilityFacts>,
}

#[napi_derive::napi(object)]
pub struct JsProviderStateCompatibilityFacts {
    pub version: String,
    pub profile_identity: String,
    pub display_metadata: String,
    pub prompt: String,
    pub skills: String,
    pub tool_catalog: String,
    pub provider_endpoint: String,
    pub model: String,
    pub protocol: String,
    pub dialect: String,
    pub reasoning_semantics: String,
    pub brain_module: String,
    pub brain_strategy: String,
    pub provider_state_schema: String,
}

#[napi_derive::napi(object)]
pub struct JsProviderStateDiagnostic {
    pub record_id: f64,
    pub session_id: String,
    pub module_id: String,
    pub strategy_id: String,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub compatibility_snapshot_json: Option<String>,
    pub compatibility_plan_json: Option<String>,
    pub status: String,
    pub payload_version: Option<String>,
    pub payload_bytes: Option<f64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub expires_at: Option<String>,
    pub last_wake_id: Option<String>,
    pub invalidated_at: Option<String>,
    pub invalidation_reason: Option<String>,
    pub is_current: bool,
}

#[napi_derive::napi(object)]
pub struct JsBrainImplementationRegistration {
    pub implementation_id: String,
    pub profile_id: String,
    pub tool_profile: JsToolProfile,
    pub model_config: JsBrainModelConfig,
    pub strategy: Option<JsBrainStrategyMetadata>,
    pub provider_state_scope: Option<JsBrainProviderStateScope>,
}

#[napi_derive::napi(object)]
pub struct JsEventSubscription {
    pub event_kinds: Vec<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub adapter_id: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsPlatformAdapterRegistration {
    pub adapter_id: String,
    pub kind: String,
    pub display_name: String,
}

#[napi_derive::napi(object)]
pub struct JsEventReceipt {
    pub accepted: bool,
    pub sequence: f64,
}

#[napi_derive::napi(object)]
pub struct JsShutdownSummary {
    pub archived_sessions: u32,
    pub dropped_subscriptions: u32,
}

#[napi_derive::napi(object)]
pub struct JsSessionConfig {
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub workspace: Option<JsSessionWorkspace>,
    pub resource_limits: Option<JsResourceLimits>,
    pub tool_profile: Option<JsToolProfile>,
    pub history_window: Option<JsSessionHistoryWindow>,
}

#[napi_derive::napi(object)]
pub struct JsSessionState {
    pub handle: f64,
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub status: String,
    pub workspace: Option<JsSessionWorkspace>,
    pub resource_limits: JsResourceLimits,
    pub tool_profile: JsToolProfile,
    pub history_window: Option<JsSessionHistoryWindow>,
    pub reasoning_effort: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsSessionHistoryWindow {
    pub max_messages: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryCaps {
    pub max_records_per_profile: Option<u32>,
    pub max_key_bytes: Option<u32>,
    pub max_content_bytes: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryRecord {
    pub profile_id: String,
    pub target_type: String,
    pub target_id: String,
    pub key: String,
    pub content: String,
    pub metadata_json: String,
    pub revision: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[napi_derive::napi(object)]
pub struct JsQueuedMessageRecord {
    pub message_id: String,
    pub owner_session_id: Option<String>,
    pub owner_agent_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub correlation_id: Option<String>,
    pub enqueued_at: String,
    pub expires_at: String,
    pub ttl_ms: u32,
    pub delivery_attempts: u32,
    pub state: String,
    pub terminal_at: Option<String>,
    pub state_reason: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryQuery {
    pub profile_id: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsSimpleKvQuery {
    pub scope_type: String,
    pub scope_id: String,
    pub key_prefix: Option<String>,
    pub include_expired: Option<bool>,
    pub expired_only: Option<bool>,
    pub now: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsSimpleKvRecord {
    pub scope_type: String,
    pub scope_id: String,
    pub key: String,
    pub value_json: String,
    pub revision: f64,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsSimpleKvWrite {
    pub scope_type: String,
    pub scope_id: String,
    pub key: String,
    pub value_json: String,
    pub now: String,
    pub expires_at: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsSimpleKvDelete {
    pub scope_type: String,
    pub scope_id: String,
    pub key: String,
    pub expected_revision: f64,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryWrite {
    pub profile_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub key: String,
    pub content: String,
    pub metadata_json: Option<String>,
    pub caps: Option<JsProfileMemoryCaps>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryReplace {
    pub write: JsProfileMemoryWrite,
    pub expected_revision: f64,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryDelete {
    pub profile_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub key: String,
    pub expected_revision: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeSearchQuery {
    pub query: String,
    pub row_type: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub instance_id: Option<String>,
    pub task_id: Option<String>,
    pub event_kind: Option<String>,
    pub recorded_after: Option<String>,
    pub recorded_before: Option<String>,
    pub limit: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeSearchResult {
    pub row_type: String,
    pub row_key: String,
    pub sequence: Option<f64>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub instance_id: Option<String>,
    pub task_id: Option<String>,
    pub event_kind: Option<String>,
    pub recorded_at: String,
    pub title: String,
    pub body: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeCounterQuery {
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
    pub counter_name: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeCounterRecord {
    pub scope_type: String,
    pub scope_id: String,
    pub counter_name: String,
    pub value: f64,
    pub updated_at: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeCounterSummary {
    pub scope_type: String,
    pub scope_id: String,
    pub brain_turns: f64,
    pub wakes: f64,
    pub tool_calls: f64,
    pub tool_errors: f64,
    pub delegations_created: f64,
    pub delegations_completed: f64,
    pub delegations_failed: f64,
    pub delegations_timed_out: f64,
    pub delegations_cancelled: f64,
    pub messages: f64,
    pub completions: f64,
    pub queue_expirations: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeDatabaseSize {
    pub database_bytes: f64,
    pub page_count: f64,
    pub page_size_bytes: f64,
    pub freelist_pages: f64,
    pub freelist_bytes: f64,
    pub wal_bytes: f64,
}

#[napi_derive::napi(object)]
pub struct JsSchemaMigrationRecord {
    pub version: f64,
    pub description: String,
    pub applied_at: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeStorageCapability {
    pub name: String,
    pub supported: bool,
    pub detail: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeRepositoryBackendRequirement {
    pub capability: String,
    pub required: bool,
    pub detail: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeRepositoryGroupDiagnostic {
    pub group_id: String,
    pub label: String,
    pub correctness_sensitive: bool,
    pub backend_requirements: Vec<JsRuntimeRepositoryBackendRequirement>,
    pub notes: Vec<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleCapabilityStatus {
    pub capability: String,
    pub required: bool,
    pub supported: bool,
    pub backend_variant: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleLogicalStoreDiagnostic {
    pub store_name: String,
    pub description: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModulePhysicalTableDiagnostic {
    pub table_name: String,
    pub logical_store: String,
    pub physical_table: String,
    pub declaration: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModulePhysicalIndexDiagnostic {
    pub table_name: String,
    pub purpose: String,
    pub physical_index: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleRetentionDiagnostic {
    pub store_name: String,
    pub policy: String,
    pub detail: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleNamedDiagnostic {
    pub name: String,
    pub description: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleQueryCatalogDiagnostic {
    pub query_id: String,
    pub store_name: String,
    pub description: String,
    pub parameter_schema_id: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleTransferHookDiagnostic {
    pub hook_name: String,
    pub format_version: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeInstalledModuleSchemaDiagnostic {
    pub module_id: String,
    pub installed_version: f64,
    pub descriptor_fingerprint: String,
    pub installed_at: String,
    pub updated_at: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleSchemaDiagnostic {
    pub module_id: String,
    pub owner_crate: String,
    pub owner_module: String,
    pub descriptor_version: f64,
    pub installed_version: Option<f64>,
    pub migration_status: String,
    pub descriptor_fingerprint: String,
    pub installed_descriptor_fingerprint: Option<String>,
    pub installed_at: Option<String>,
    pub updated_at: Option<String>,
    pub capability_status: Vec<JsRuntimeModuleCapabilityStatus>,
    pub logical_stores: Vec<JsRuntimeModuleLogicalStoreDiagnostic>,
    pub physical_tables: Vec<JsRuntimeModulePhysicalTableDiagnostic>,
    pub physical_indexes: Vec<JsRuntimeModulePhysicalIndexDiagnostic>,
    pub retention: Vec<JsRuntimeModuleRetentionDiagnostic>,
    pub repository_contracts: Vec<JsRuntimeModuleNamedDiagnostic>,
    pub query_catalog_entries: Vec<JsRuntimeModuleQueryCatalogDiagnostic>,
    pub export_hooks: Vec<JsRuntimeModuleTransferHookDiagnostic>,
    pub import_hooks: Vec<JsRuntimeModuleTransferHookDiagnostic>,
    pub migration_notes: Vec<String>,
    pub degraded_reasons: Vec<String>,
    pub blocked_reasons: Vec<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeModuleSchemaRegistryDiagnostics {
    pub source: String,
    pub backend_capabilities: Vec<String>,
    pub modules: Vec<JsRuntimeModuleSchemaDiagnostic>,
    pub orphan_installed_modules: Vec<JsRuntimeInstalledModuleSchemaDiagnostic>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeStorageTableCount {
    pub table: String,
    pub rows: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeQueryPlanCheck {
    pub name: String,
    pub uses_index: bool,
    pub detail: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeStoragePressureSignal {
    pub name: String,
    pub active: bool,
    pub severity: String,
    pub observed_value: f64,
    pub threshold_value: Option<f64>,
    pub detail: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeStorageConnectionHealth {
    pub backend: String,
    pub status: String,
    pub max_connections: f64,
    pub active_connections: f64,
    pub idle_connections: f64,
    pub total_opened: f64,
    pub checkout_count: f64,
    pub checkout_reuse_count: f64,
    pub reconnect_attempts: f64,
    pub reconnect_successes: f64,
    pub closed_connections_discarded: f64,
    pub last_error: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeStorageDiagnostics {
    pub backend: String,
    pub backend_label: String,
    pub schema_version: f64,
    pub supported_schema_version: f64,
    pub migrations: Vec<JsSchemaMigrationRecord>,
    pub size: JsRuntimeDatabaseSize,
    pub table_counts: Vec<JsRuntimeStorageTableCount>,
    pub capabilities: Vec<JsRuntimeStorageCapability>,
    pub repository_groups: Vec<JsRuntimeRepositoryGroupDiagnostic>,
    pub connection_health: JsRuntimeStorageConnectionHealth,
    pub module_registry: JsRuntimeModuleSchemaRegistryDiagnostics,
    pub index_checks: Vec<JsRuntimeQueryPlanCheck>,
    pub search_healthy: bool,
    pub pressure_signals: Vec<JsRuntimeStoragePressureSignal>,
    pub pressure: bool,
    pub external_runtime_events: JsRuntimeExternalEventStorageDiagnostics,
    pub filesystem_headroom: JsRuntimeFilesystemHeadroom,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeExternalEventStorageDiagnostics {
    pub event_rows: f64,
    pub estimated_event_bytes: f64,
    pub checkpoint_rows: f64,
    pub oldest_sequence: Option<f64>,
    pub oldest_created_at: Option<String>,
    pub newest_sequence: Option<f64>,
    pub newest_created_at: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeFilesystemHeadroom {
    pub available: bool,
    pub source: String,
    pub path: Option<String>,
    pub total_bytes: Option<f64>,
    pub free_bytes: Option<f64>,
    pub free_percent: Option<u32>,
    pub warning_free_percent: Option<u32>,
    pub warning_active: bool,
    pub detail: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeMaintenancePolicy {
    pub expire_queued_messages_at: Option<String>,
    pub purge_terminal_queued_messages_before: Option<String>,
    pub expire_provider_wire_states_at: Option<String>,
    pub compact_session_memory_at: Option<String>,
    pub session_memory_max_active_records_per_scope: Option<u32>,
    pub session_memory_archive_batch_size: Option<u32>,
    pub compact_terminal_external_runtime_events_before: Option<String>,
    pub external_runtime_event_retention_at: Option<String>,
    pub external_runtime_event_terminal_turn_batch_size: Option<u32>,
    pub run_wal_checkpoint: Option<bool>,
    pub run_optimize: Option<bool>,
}

#[napi_derive::napi(object)]
pub struct JsSessionMemoryCompactionReport {
    pub enabled: bool,
    pub scopes_inspected: f64,
    pub retention_pressure_scopes: f64,
    pub scopes_compacted: f64,
    pub session_summaries_created: f64,
    pub branch_summaries_created: f64,
    pub records_archived: f64,
    pub records_superseded: f64,
    pub skipped_scopes: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeMaintenanceReport {
    pub size_before: JsRuntimeDatabaseSize,
    pub size_after: JsRuntimeDatabaseSize,
    pub expired_queue_messages: f64,
    pub purged_terminal_queue_messages: f64,
    pub expired_provider_wire_states: f64,
    pub session_memory_compaction: JsSessionMemoryCompactionReport,
    pub external_runtime_event_retention: JsExternalRuntimeEventRetentionReport,
    pub wal_checkpoint_ran: bool,
    pub optimize_ran: bool,
}

#[napi_derive::napi(object)]
pub struct JsExternalRuntimeEventRetentionReport {
    pub enabled: bool,
    pub cutoff: Option<String>,
    pub terminal_turn_batch_size: Option<u32>,
    pub terminal_turns_inspected: f64,
    pub terminal_turns_compacted: f64,
    pub checkpoints_created: f64,
    pub events_deleted: f64,
    pub estimated_reclaimed_bytes: f64,
    pub oldest_retained_sequence: Option<f64>,
    pub oldest_retained_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireDeleteMessageVariantRequest {
    pub(crate) slot_id: MessageSlotId,
    pub(crate) variant_id: MessageVariantId,
    pub(crate) updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireReorderMessageVariantsRequest {
    pub(crate) slot_id: MessageSlotId,
    pub(crate) ordered_variant_ids: Vec<MessageVariantId>,
    pub(crate) updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireGetConversationBranchStateRequest {
    pub(crate) session_id: SessionId,
    pub(crate) default_updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireRemoveAttachmentRequest {
    pub(crate) attachment_id: AttachmentId,
    pub(crate) updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireRemoveDataBankScopeRequest {
    pub(crate) scope_id: DataBankScopeId,
    pub(crate) updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireRemoveLoreEntryFromLayerRequest {
    pub(crate) layer_id: String,
    pub(crate) record_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireSetLoreEntryConstantRequest {
    pub(crate) layer_id: String,
    pub(crate) record_id: String,
    pub(crate) is_constant: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireToggleChatLayerRequest {
    pub(crate) chat_id: String,
    pub(crate) layer_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireReorderChatLayersRequest {
    pub(crate) chat_id: String,
    pub(crate) layer_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireProfileRegistryQuery {
    pub(crate) lifecycle_status: Option<String>,
    pub(crate) limit: Option<u32>,
    pub(crate) offset: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsActionBatchReceipt {
    pub wake_id: String,
    pub accepted_actions: u32,
    pub rejected_actions_json: String,
}

#[napi_derive::napi(object)]
pub struct JsBufferedBrainWakeRequest {
    pub body_state: u32,
    pub system_prompt: u32,
    pub role_assembly: u32,
    pub continuation_state_json: Option<String>,
    pub provider_state_json: Option<String>,
    pub provider_state_absence: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeBufferView {
    pub handle: u32,
    pub media_type: String,
    pub byte_len: f64,
    pub bytes: napi::bindgen_prelude::Buffer,
}
