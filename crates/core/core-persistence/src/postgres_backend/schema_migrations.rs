//! PostgreSQL schema migration catalog and application logic.

use super::*;

pub(super) const POSTGRES_SCHEMA_VERSION: i64 = 19;
const POSTGRES_MIN_SUPPORTED_SCHEMA_VERSION: i64 = 1;

#[allow(dead_code)]
pub(super) const POSTGRES_BACKEND_SCHEMA_VERSION: i64 = POSTGRES_SCHEMA_VERSION;

struct PostgresSchemaMigration {
    version: i64,
    description: &'static str,
    apply: Option<fn(&mut Transaction<'_>, &str) -> CoreResult<()>>,
}

const POSTGRES_SCHEMA_MIGRATIONS: &[PostgresSchemaMigration] = &[
    PostgresSchemaMigration {
        version: 1,
        description: "create baseline PostgreSQL durable service schema",
        apply: Some(apply_postgres_baseline_schema),
    },
    PostgresSchemaMigration {
        version: 2,
        description: "baseline includes sessions and immutable session configs",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 3,
        description: "baseline includes profile registry and model providers",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 4,
        description: "baseline includes per-agent channel and MCP bindings",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 5,
        description: "baseline includes durable identities and event projections",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 6,
        description: "baseline includes queued messages and scheduler state",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 7,
        description: "baseline includes worker runs, pools, and completion packets",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 8,
        description: "baseline includes tool telemetry and module simple_kv",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 9,
        description: "baseline includes session memory and activity digests",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 10,
        description: "baseline includes context compaction artifacts",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 11,
        description: "baseline includes memory proposal governance",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 12,
        description: "baseline includes runtime search and provider wire state",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 13,
        description: "baseline includes conversation trees and message variants",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 14,
        description: "baseline includes attachments and data-bank scopes",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 15,
        description: "baseline includes profile memory and roleplay lore records",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 16,
        description: "baseline includes roleplay lore layers and recall traces",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 17,
        description: "add durable chat event replay log",
        apply: Some(apply_postgres_chat_event_log),
    },
    PostgresSchemaMigration {
        version: 18,
        description: "add typed roleplay character persona session and import records",
        apply: Some(apply_postgres_roleplay_records),
    },
    PostgresSchemaMigration {
        version: 19,
        description: "add typed curator governance records and audit receipts",
        apply: Some(apply_postgres_curator_governance),
    },
];

#[cfg(test)]
pub(super) fn postgres_schema_migration_count() -> usize {
    POSTGRES_SCHEMA_MIGRATIONS.len()
}

impl PostgresBackendStore {
    pub(super) fn schema_version(&self) -> CoreResult<i64> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        current_postgres_schema_version(&mut *client, &schema)
    }

    pub(super) fn schema_migrations(&self) -> CoreResult<Vec<SchemaMigrationRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        load_postgres_schema_migration_records(&mut *client, &schema)
    }
}

impl PostgresBackendStore {
    pub(super) fn migrate(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        prepare_postgres_migration_metadata(&mut client, &schema)?;
        apply_postgres_schema_migrations(&mut client, &schema)
    }
}

fn apply_postgres_baseline_schema(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {schema};
                 CREATE TABLE IF NOT EXISTS {schema}.rusty_crew_storage_metadata (
                    metadata_key TEXT PRIMARY KEY,
                    metadata_value TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_counters (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    counter_name TEXT NOT NULL,
                    value BIGINT NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(scope_type, scope_id, counter_name)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_counters_scope_idx
                    ON {schema}.runtime_counters(scope_type, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.sessions (
                    session_id TEXT PRIMARY KEY,
                    handle BIGINT NOT NULL,
                    agent_id TEXT NOT NULL,
                    profile_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    state_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS sessions_agent_profile_idx
                    ON {schema}.sessions(agent_id, profile_id, kind, status, session_id);
                 CREATE TABLE IF NOT EXISTS {schema}.session_configs (
                    session_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.profile_registry (
                    profile_id TEXT PRIMARY KEY,
                    lifecycle_status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS profile_registry_status_idx
                    ON {schema}.profile_registry(lifecycle_status, profile_id);
                 CREATE TABLE IF NOT EXISTS {schema}.model_providers (
                    alias TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    protocol TEXT NOT NULL,
                    provider_json TEXT NOT NULL,
                    secret_ciphertext TEXT,
                    secret_updated_at TEXT,
                    revision BIGINT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS model_providers_status_idx
                    ON {schema}.model_providers(status, updated_at DESC, alias);
                 CREATE INDEX IF NOT EXISTS model_providers_protocol_idx
                    ON {schema}.model_providers(protocol, alias);
                 CREATE TABLE IF NOT EXISTS {schema}.channel_bindings (
                    binding_id TEXT PRIMARY KEY,
                    adapter_id TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    agent_id TEXT NOT NULL,
                    instance_id TEXT,
                    session_id TEXT,
                    profile_id TEXT NOT NULL,
                    external_channel_id TEXT NOT NULL,
                    external_thread_id TEXT,
                    external_user_id TEXT,
                    provider_subscription_id TEXT,
                    cursor TEXT,
                    membership_state TEXT,
                    presence_state TEXT,
                    status TEXT NOT NULL,
                    degraded_reason TEXT,
                    provenance_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS channel_bindings_agent_provider_idx
                    ON {schema}.channel_bindings(agent_id, provider, status);
                 CREATE INDEX IF NOT EXISTS channel_bindings_profile_agent_idx
                    ON {schema}.channel_bindings(profile_id, agent_id, status);
                 CREATE INDEX IF NOT EXISTS channel_bindings_session_idx
                    ON {schema}.channel_bindings(session_id, status);
                 CREATE INDEX IF NOT EXISTS channel_bindings_external_idx
                    ON {schema}.channel_bindings(provider, external_channel_id, external_thread_id);
                 CREATE TABLE IF NOT EXISTS {schema}.mcp_bindings (
                    binding_id TEXT PRIMARY KEY,
                    adapter_id TEXT NOT NULL,
                    agent_id TEXT NOT NULL,
                    instance_id TEXT,
                    session_id TEXT,
                    profile_id TEXT NOT NULL,
                    server_names_json TEXT NOT NULL,
                    endpoint_ref TEXT NOT NULL,
                    transport TEXT NOT NULL,
                    tool_profile_key TEXT NOT NULL,
                    discovered_tool_revision TEXT,
                    status TEXT NOT NULL,
                    degraded_reason TEXT,
                    diagnostics_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS mcp_bindings_agent_profile_idx
                    ON {schema}.mcp_bindings(agent_id, profile_id, status);
                 CREATE INDEX IF NOT EXISTS mcp_bindings_session_idx
                    ON {schema}.mcp_bindings(session_id, status);
                 CREATE INDEX IF NOT EXISTS mcp_bindings_adapter_idx
                    ON {schema}.mcp_bindings(adapter_id, status);
                 CREATE TABLE IF NOT EXISTS {schema}.agent_identities (
                    agent_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    archived_at TEXT
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.agent_instances (
                    instance_id TEXT PRIMARY KEY,
                    agent_id TEXT NOT NULL,
                    profile_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL,
                    archived_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS agent_instances_agent_idx
                    ON {schema}.agent_instances(agent_id, status, last_active_at DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.session_identities (
                    session_id TEXT PRIMARY KEY,
                    instance_id TEXT NOT NULL,
                    agent_id TEXT NOT NULL,
                    profile_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL,
                    archived_at TEXT
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.event_history (
                    sequence BIGINT PRIMARY KEY,
                    event_kind TEXT NOT NULL,
                    recorded_at TEXT NOT NULL DEFAULT to_char(
                        CURRENT_TIMESTAMP AT TIME ZONE 'UTC',
                        'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'
                    ),
                    event_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS event_history_kind_idx
                    ON {schema}.event_history(event_kind, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.event_index (
                    sequence BIGINT NOT NULL REFERENCES {schema}.event_history(sequence) ON DELETE CASCADE,
                    projection TEXT NOT NULL,
                    value TEXT NOT NULL,
                    PRIMARY KEY(sequence, projection, value)
                 );
                 CREATE INDEX IF NOT EXISTS event_index_lookup_idx
                    ON {schema}.event_index(projection, value, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.chat_events (
                    session_id TEXT NOT NULL,
                    sequence_id BIGINT NOT NULL,
                    event_id TEXT NOT NULL UNIQUE,
                    created_at TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY(session_id, sequence_id)
                 );
                 CREATE INDEX IF NOT EXISTS chat_events_session_created_idx
                    ON {schema}.chat_events(session_id, created_at, sequence_id);
                 CREATE INDEX IF NOT EXISTS chat_events_kind_idx
                    ON {schema}.chat_events(kind, created_at, session_id, sequence_id);
                 CREATE TABLE IF NOT EXISTS {schema}.queued_messages (
                    message_id TEXT PRIMARY KEY,
                    owner_session_id TEXT,
                    owner_agent_id TEXT NOT NULL,
                    from_agent TEXT NOT NULL,
                    to_agent TEXT NOT NULL,
                    body TEXT NOT NULL,
                    correlation_id TEXT,
                    source_sequence BIGINT,
                    enqueued_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL,
                    ttl_ms BIGINT NOT NULL,
                    delivery_attempts BIGINT NOT NULL,
                    state TEXT NOT NULL,
                    terminal_at TEXT,
                    state_reason TEXT,
                    message_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS queued_messages_state_expiry_idx
                    ON {schema}.queued_messages(state, expires_at);
                 CREATE INDEX IF NOT EXISTS queued_messages_owner_agent_idx
                    ON {schema}.queued_messages(owner_agent_id, state, expires_at);
                 CREATE INDEX IF NOT EXISTS queued_messages_owner_session_idx
                    ON {schema}.queued_messages(owner_session_id, state, expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.scheduled_jobs (
                    job_id TEXT PRIMARY KEY,
                    job_kind TEXT NOT NULL,
                    target_session_id TEXT,
                    interval_ms BIGINT,
                    next_due_at TEXT,
                    payload_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    paused_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS scheduled_jobs_due_idx
                    ON {schema}.scheduled_jobs(status, next_due_at, job_id);
                 CREATE INDEX IF NOT EXISTS scheduled_jobs_kind_due_idx
                    ON {schema}.scheduled_jobs(job_kind, status, next_due_at, job_id);
                 CREATE TABLE IF NOT EXISTS {schema}.scheduled_job_runs (
                    run_id TEXT PRIMARY KEY,
                    job_id TEXT NOT NULL,
                    job_kind TEXT NOT NULL,
                    target_session_id TEXT,
                    status TEXT NOT NULL,
                    trigger_kind TEXT NOT NULL,
                    scheduled_for TEXT,
                    claimed_at TEXT NOT NULL,
                    claim_deadline_at TEXT NOT NULL,
                    completed_at TEXT,
                    error TEXT,
                    output_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS scheduled_job_runs_job_status_idx
                    ON {schema}.scheduled_job_runs(job_id, status, created_at, run_id);
                 CREATE INDEX IF NOT EXISTS scheduled_job_runs_claim_idx
                    ON {schema}.scheduled_job_runs(status, claim_deadline_at, run_id);
                 CREATE INDEX IF NOT EXISTS scheduled_job_runs_session_idx
                    ON {schema}.scheduled_job_runs(target_session_id, status, created_at, run_id);
                 CREATE TABLE IF NOT EXISTS {schema}.worker_runs (
                    run_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    delegated_session_id TEXT,
                    parent_agent_id TEXT,
                    profile_id TEXT NOT NULL,
                    task_id TEXT,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_updated_at TEXT NOT NULL,
                    source_wake_id TEXT NOT NULL,
                    source_action_index BIGINT NOT NULL,
                    delegation_correlation_id TEXT,
                    parent_consumption TEXT NOT NULL,
                    fan_out_group_id TEXT,
                    fan_out_max_concurrency BIGINT,
                    fan_out_failure_policy TEXT NOT NULL,
                    worker_pool_work_item_id TEXT,
                    worker_pool_lease_id TEXT,
                    worker_pool_member_id TEXT,
                    worker_pool_claim_token TEXT
                 );
                 CREATE INDEX IF NOT EXISTS worker_runs_parent_status_created_idx
                    ON {schema}.worker_runs(session_id, status, created_at, run_id);
                 CREATE INDEX IF NOT EXISTS worker_runs_delegated_session_idx
                    ON {schema}.worker_runs(delegated_session_id);
                 CREATE INDEX IF NOT EXISTS worker_runs_profile_task_created_idx
                    ON {schema}.worker_runs(profile_id, task_id, created_at, run_id);
                 ALTER TABLE {schema}.worker_runs
                    ADD COLUMN IF NOT EXISTS worker_pool_work_item_id TEXT;
                 ALTER TABLE {schema}.worker_runs
                    ADD COLUMN IF NOT EXISTS worker_pool_lease_id TEXT;
                 ALTER TABLE {schema}.worker_runs
                    ADD COLUMN IF NOT EXISTS worker_pool_member_id TEXT;
                 ALTER TABLE {schema}.worker_runs
                    ADD COLUMN IF NOT EXISTS worker_pool_claim_token TEXT;
                 CREATE INDEX IF NOT EXISTS worker_runs_pool_lease_idx
                    ON {schema}.worker_runs(worker_pool_lease_id);
                 CREATE INDEX IF NOT EXISTS worker_runs_pool_member_idx
                    ON {schema}.worker_runs(worker_pool_member_id, status);
                 CREATE TABLE IF NOT EXISTS {schema}.worker_pool_members (
                    member_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    agent_id TEXT,
                    session_id TEXT,
                    status TEXT NOT NULL,
                    concurrency_limit BIGINT NOT NULL CHECK (concurrency_limit >= 0),
                    active_leases BIGINT NOT NULL DEFAULT 0 CHECK (active_leases >= 0),
                    capabilities_json TEXT NOT NULL,
                    registered_at TEXT NOT NULL,
                    last_heartbeat_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS worker_pool_members_status_heartbeat_idx
                    ON {schema}.worker_pool_members(status, last_heartbeat_at, member_id);
                 CREATE INDEX IF NOT EXISTS worker_pool_members_profile_status_idx
                    ON {schema}.worker_pool_members(profile_id, status, member_id);
                 CREATE TABLE IF NOT EXISTS {schema}.worker_pool_work_items (
                    work_item_id TEXT PRIMARY KEY,
                    requested_profile_id TEXT,
                    task_id TEXT,
                    status TEXT NOT NULL,
                    priority BIGINT NOT NULL DEFAULT 100,
                    work_json TEXT NOT NULL,
                    required_capabilities_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    claimed_by_member_id TEXT,
                    lease_id TEXT,
                    claim_token TEXT,
                    claim_deadline_at TEXT,
                    terminal_at TEXT,
                    terminal_summary TEXT
                 );
                 CREATE INDEX IF NOT EXISTS worker_pool_work_items_pending_idx
                    ON {schema}.worker_pool_work_items(status, priority, created_at, work_item_id);
                 CREATE INDEX IF NOT EXISTS worker_pool_work_items_claim_deadline_idx
                    ON {schema}.worker_pool_work_items(status, claim_deadline_at, work_item_id);
                 CREATE INDEX IF NOT EXISTS worker_pool_work_items_member_status_idx
                    ON {schema}.worker_pool_work_items(claimed_by_member_id, status, work_item_id);
                 CREATE TABLE IF NOT EXISTS {schema}.worker_pool_leases (
                    lease_id TEXT PRIMARY KEY,
                    work_item_id TEXT NOT NULL,
                    member_id TEXT NOT NULL,
                    claim_token TEXT NOT NULL,
                    status TEXT NOT NULL,
                    claimed_at TEXT NOT NULL,
                    claim_deadline_at TEXT NOT NULL,
                    terminal_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS worker_pool_leases_member_status_idx
                    ON {schema}.worker_pool_leases(member_id, status, claimed_at, lease_id);
                 CREATE INDEX IF NOT EXISTS worker_pool_leases_work_item_idx
                    ON {schema}.worker_pool_leases(work_item_id, status, lease_id);
                 CREATE TABLE IF NOT EXISTS {schema}.worker_pool_events (
                    sequence BIGSERIAL PRIMARY KEY,
                    work_item_id TEXT NOT NULL,
                    lease_id TEXT,
                    member_id TEXT,
                    event_type TEXT NOT NULL,
                    event_json TEXT NOT NULL,
                    recorded_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS worker_pool_events_work_item_idx
                    ON {schema}.worker_pool_events(work_item_id, sequence);
                 CREATE INDEX IF NOT EXISTS worker_pool_events_member_idx
                    ON {schema}.worker_pool_events(member_id, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.completion_packets (
                    sequence BIGINT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    packet_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS completion_packets_session_sequence_idx
                    ON {schema}.completion_packets(session_id, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.tool_call_history (
                    sequence BIGINT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    wake_id TEXT,
                    tool_name TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    is_error BOOLEAN,
                    metadata_json TEXT
                 );
                 CREATE INDEX IF NOT EXISTS tool_call_history_session_sequence_idx
                    ON {schema}.tool_call_history(session_id, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.module_simple_kv_entries (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    entry_key TEXT NOT NULL,
                    value_json TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT,
                    PRIMARY KEY(scope_type, scope_id, entry_key)
                 );
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_scope_key_idx
                    ON {schema}.module_simple_kv_entries(scope_type, scope_id, entry_key);
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_expires_at_idx
                    ON {schema}.module_simple_kv_entries(expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.session_memory_records (
                    record_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    branch_id TEXT,
                    shape_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS session_memory_session_scope_idx
                    ON {schema}.session_memory_records(session_id, scope_type, scope_id, status, updated_at DESC);
                 CREATE INDEX IF NOT EXISTS session_memory_branch_idx
                    ON {schema}.session_memory_records(branch_id, status, updated_at DESC);
                 CREATE INDEX IF NOT EXISTS session_memory_shape_idx
                    ON {schema}.session_memory_records(shape_id, status, updated_at DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.session_activity_digests (
                    digest_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    wake_id TEXT NOT NULL,
                    reviewed_at TEXT,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    retention_until TEXT
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS session_activity_digests_wake_idx
                    ON {schema}.session_activity_digests(profile_id, session_id, wake_id);
                 CREATE INDEX IF NOT EXISTS session_activity_digests_profile_review_idx
                    ON {schema}.session_activity_digests(profile_id, reviewed_at, created_at DESC, digest_id);
                 CREATE INDEX IF NOT EXISTS session_activity_digests_session_idx
                    ON {schema}.session_activity_digests(session_id, created_at DESC, digest_id);
                 CREATE TABLE IF NOT EXISTS {schema}.context_compaction_artifacts (
                    artifact_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    branch_id TEXT,
                    strategy_id TEXT NOT NULL,
                    enters_future_context BOOLEAN NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS context_compaction_session_latest_idx
                    ON {schema}.context_compaction_artifacts(session_id, created_at DESC, artifact_id);
                 CREATE INDEX IF NOT EXISTS context_compaction_branch_latest_idx
                    ON {schema}.context_compaction_artifacts(session_id, branch_id, created_at DESC, artifact_id);
                 CREATE INDEX IF NOT EXISTS context_compaction_strategy_latest_idx
                    ON {schema}.context_compaction_artifacts(session_id, strategy_id, created_at DESC, artifact_id);
                 CREATE INDEX IF NOT EXISTS context_compaction_future_context_idx
                    ON {schema}.context_compaction_artifacts(session_id, enters_future_context, created_at DESC, artifact_id);
                 CREATE TABLE IF NOT EXISTS {schema}.memory_proposals (
                    proposal_id TEXT PRIMARY KEY,
                    space_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    dedupe_key TEXT,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS memory_proposals_dedupe_idx
                    ON {schema}.memory_proposals(space_id, dedupe_key)
                    WHERE dedupe_key IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS memory_proposals_status_idx
                    ON {schema}.memory_proposals(space_id, status, updated_at DESC, proposal_id);
                 CREATE TABLE IF NOT EXISTS {schema}.memory_governance_decisions (
                    decision_id TEXT PRIMARY KEY,
                    proposal_id TEXT NOT NULL REFERENCES {schema}.memory_proposals(proposal_id),
                    decision TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    decided_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS memory_governance_decisions_proposal_idx
                    ON {schema}.memory_governance_decisions(proposal_id, decided_at, decision_id);
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_search_entries (
                    row_type TEXT NOT NULL,
                    row_key TEXT NOT NULL,
                    sequence BIGINT,
                    session_id TEXT,
                    agent_id TEXT,
                    instance_id TEXT,
                    task_id TEXT,
                    event_kind TEXT,
                    recorded_at TEXT NOT NULL,
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    search_vector TSVECTOR GENERATED ALWAYS AS (
                        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, ''))
                    ) STORED,
                    PRIMARY KEY(row_type, row_key)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_vector_idx
                    ON {schema}.runtime_search_entries USING GIN(search_vector);
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_metadata_idx
                    ON {schema}.runtime_search_entries(
                        row_type,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at
                    );
                 CREATE TABLE IF NOT EXISTS {schema}.provider_wire_states (
                    row_id BIGSERIAL PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    module_id TEXT NOT NULL,
                    strategy_id TEXT NOT NULL,
                    profile_fingerprint TEXT NOT NULL,
                    provider_fingerprint TEXT NOT NULL,
                    payload_version TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    payload_encoding TEXT NOT NULL DEFAULT 'json',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT,
                    last_wake_id TEXT,
                    invalidated_at TEXT,
                    invalidation_reason TEXT
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS provider_wire_states_current_idx
                    ON {schema}.provider_wire_states(session_id, module_id, strategy_id)
                    WHERE invalidated_at IS NULL;
                 CREATE INDEX IF NOT EXISTS provider_wire_states_session_current_idx
                    ON {schema}.provider_wire_states(session_id, invalidated_at);
                 CREATE INDEX IF NOT EXISTS provider_wire_states_expiry_idx
                    ON {schema}.provider_wire_states(invalidated_at, expires_at);
                 CREATE INDEX IF NOT EXISTS provider_wire_states_updated_idx
                    ON {schema}.provider_wire_states(updated_at DESC, row_id DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.message_slots (
                    slot_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    primary_variant_id TEXT NOT NULL,
                    active_variant_id TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS message_slots_session_slot_idx
                    ON {schema}.message_slots(session_id, slot_id);
                 CREATE INDEX IF NOT EXISTS message_slots_active_variant_idx
                    ON {schema}.message_slots(active_variant_id);
                 CREATE TABLE IF NOT EXISTS {schema}.messages (
                    message_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    branch_id TEXT,
                    parent_message_id TEXT,
                    previous_message_id TEXT,
                    author_id TEXT NOT NULL,
                    author_role TEXT NOT NULL,
                    status TEXT NOT NULL,
                    body TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS messages_session_created_idx
                    ON {schema}.messages(session_id, created_at, message_id);
                 CREATE INDEX IF NOT EXISTS messages_session_branch_idx
                    ON {schema}.messages(session_id, branch_id);
                 CREATE INDEX IF NOT EXISTS messages_parent_message_idx
                    ON {schema}.messages(parent_message_id);
                 CREATE TABLE IF NOT EXISTS {schema}.message_blocks (
                    block_id TEXT PRIMARY KEY,
                    message_id TEXT NOT NULL REFERENCES {schema}.messages(message_id),
                    ordinal BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    content_json TEXT NOT NULL,
                    render_policy_json TEXT,
                    metadata_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS message_blocks_message_ordinal_idx
                    ON {schema}.message_blocks(message_id, ordinal);
                 CREATE TABLE IF NOT EXISTS {schema}.message_variants (
                    variant_id TEXT PRIMARY KEY,
                    slot_id TEXT NOT NULL REFERENCES {schema}.message_slots(slot_id),
                    source TEXT NOT NULL,
                    ordinal BIGINT NOT NULL,
                    status TEXT NOT NULL,
                    message_id TEXT NOT NULL REFERENCES {schema}.messages(message_id),
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS message_variants_slot_ordinal_idx
                    ON {schema}.message_variants(slot_id, ordinal);
                 CREATE INDEX IF NOT EXISTS message_variants_slot_status_idx
                    ON {schema}.message_variants(slot_id, status, ordinal);
                 CREATE INDEX IF NOT EXISTS message_variants_message_idx
                    ON {schema}.message_variants(message_id);
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_branches (
                    branch_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    parent_branch_id TEXT,
                    parent_message_id TEXT,
                    origin_message_id TEXT,
                    head_message_id TEXT,
                    label TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS conversation_branches_session_branch_idx
                    ON {schema}.conversation_branches(session_id, branch_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_parent_branch_idx
                    ON {schema}.conversation_branches(parent_branch_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_parent_message_idx
                    ON {schema}.conversation_branches(parent_message_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_session_created_idx
                    ON {schema}.conversation_branches(session_id, created_at, branch_id);
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_branch_state (
                    session_id TEXT PRIMARY KEY,
                    active_branch_id TEXT,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_snapshots (
                    snapshot_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    branch_id TEXT,
                    message_id TEXT,
                    cursor TEXT,
                    label TEXT,
                    summary TEXT,
                    source TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_message_idx
                    ON {schema}.conversation_snapshots(session_id, message_id);
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_branch_idx
                    ON {schema}.conversation_snapshots(session_id, branch_id, created_at);
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_created_idx
                    ON {schema}.conversation_snapshots(session_id, created_at, snapshot_id);
                 CREATE TABLE IF NOT EXISTS {schema}.attachments (
                    attachment_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    filename TEXT NOT NULL,
                    mime_type TEXT NOT NULL,
                    byte_size BIGINT NOT NULL,
                    storage_url TEXT,
                    download_url TEXT,
                    thumbnail_url TEXT,
                    extracted_text TEXT,
                    extracted_text_truncated BOOLEAN NOT NULL DEFAULT FALSE,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS attachments_session_status_idx
                    ON {schema}.attachments(session_id, status, created_at, attachment_id);
                 CREATE INDEX IF NOT EXISTS attachments_expiry_idx
                    ON {schema}.attachments(expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.attachment_links (
                    link_id TEXT PRIMARY KEY,
                    attachment_id TEXT NOT NULL REFERENCES {schema}.attachments(attachment_id),
                    session_id TEXT NOT NULL,
                    message_id TEXT,
                    block_id TEXT,
                    scope_id TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS attachment_links_attachment_idx
                    ON {schema}.attachment_links(attachment_id, created_at, link_id);
                 CREATE INDEX IF NOT EXISTS attachment_links_session_message_idx
                    ON {schema}.attachment_links(session_id, message_id);
                 CREATE INDEX IF NOT EXISTS attachment_links_session_block_idx
                    ON {schema}.attachment_links(session_id, block_id);
                 CREATE INDEX IF NOT EXISTS attachment_links_session_scope_idx
                    ON {schema}.attachment_links(session_id, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.data_bank_scopes (
                    scope_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    label TEXT,
                    description TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS data_bank_scopes_session_status_idx
                    ON {schema}.data_bank_scopes(session_id, status, created_at, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.profile_memories (
                    profile_id TEXT NOT NULL,
                    target_type TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    memory_key TEXT NOT NULL,
                    content TEXT NOT NULL,
                    metadata_json JSONB NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(profile_id, target_type, target_id, memory_key)
                 );
                 CREATE INDEX IF NOT EXISTS profile_memories_profile_updated_idx
                    ON {schema}.profile_memories(profile_id, updated_at DESC);
                 CREATE INDEX IF NOT EXISTS profile_memories_target_idx
                    ON {schema}.profile_memories(profile_id, target_type, target_id, memory_key);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_records (
                    record_id TEXT PRIMARY KEY,
                    world_id TEXT NOT NULL,
                    entity_id TEXT,
                    session_id TEXT,
                    branch_id TEXT,
                    shape_id TEXT NOT NULL,
                    shape_version BIGINT NOT NULL,
                    canon_status TEXT NOT NULL,
                    visibility TEXT NOT NULL,
                    status TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    content_json JSONB NOT NULL,
                    evidence_refs_json JSONB NOT NULL,
                    source TEXT NOT NULL,
                    confidence DOUBLE PRECISION NOT NULL,
                    durability_rationale TEXT NOT NULL,
                    supersedes_record_id TEXT,
                    superseded_by_record_id TEXT,
                    tombstoned_at TEXT,
                    tombstone_reason TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    search_vector TSVECTOR GENERATED ALWAYS AS (
                        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, ''))
                    ) STORED
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_lore_world_status_updated_idx
                    ON {schema}.module_roleplay_lore_records(world_id, status, updated_at DESC, record_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_entity_idx
                    ON {schema}.module_roleplay_lore_records(world_id, entity_id, canon_status, visibility, updated_at DESC, record_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_shape_idx
                    ON {schema}.module_roleplay_lore_records(shape_id, shape_version, updated_at DESC, record_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_supersedes_idx
                    ON {schema}.module_roleplay_lore_records(supersedes_record_id)
                    WHERE supersedes_record_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS roleplay_lore_search_vector_idx
                    ON {schema}.module_roleplay_lore_records USING GIN(search_vector);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_provenance_events (
                    event_id TEXT PRIMARY KEY,
                    record_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_lore_records(record_id),
                    world_id TEXT NOT NULL,
                    evidence_refs_json JSONB NOT NULL,
                    source TEXT NOT NULL,
                    actor TEXT NOT NULL,
                    note TEXT,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_lore_provenance_record_idx
                    ON {schema}.module_roleplay_lore_provenance_events(record_id, created_at, event_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_provenance_world_idx
                    ON {schema}.module_roleplay_lore_provenance_events(world_id, created_at, event_id);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_layers (
                    layer_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT,
                    purpose TEXT NOT NULL DEFAULT 'mixed',
                    write_policy TEXT NOT NULL DEFAULT 'manual',
                    is_archived BOOLEAN NOT NULL DEFAULT FALSE,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_lore_layers_profile_idx
                    ON {schema}.module_roleplay_lore_layers(profile_id, is_archived, name);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_layer_entries (
                    layer_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_lore_layers(layer_id) ON DELETE CASCADE,
                    record_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_lore_records(record_id) ON DELETE CASCADE,
                    is_constant BOOLEAN NOT NULL DEFAULT FALSE,
                    priority BIGINT NOT NULL DEFAULT 0,
                    added_at TEXT NOT NULL,
                    PRIMARY KEY(layer_id, record_id)
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_lore_layer_entries_record_idx
                    ON {schema}.module_roleplay_lore_layer_entries(record_id, layer_id);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_chat_layers (
                    chat_id TEXT NOT NULL,
                    layer_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_lore_layers(layer_id) ON DELETE CASCADE,
                    priority BIGINT NOT NULL DEFAULT 0,
                    enabled BOOLEAN NOT NULL DEFAULT TRUE,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY(chat_id, layer_id)
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_chat_layers_enabled_idx
                    ON {schema}.module_roleplay_chat_layers(chat_id, enabled, priority, layer_id);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_recall_traces (
                    trace_id TEXT PRIMARY KEY,
                    session_id TEXT,
                    layer_ids JSONB NOT NULL,
                    query_text TEXT,
                    active_subjects JSONB,
                    excluded_subjects JSONB,
                    config_snapshot JSONB NOT NULL,
                    entries_considered BIGINT NOT NULL,
                    entries_returned BIGINT NOT NULL,
                    token_budget BIGINT,
                    tokens_consumed BIGINT,
                    created_at TEXT NOT NULL
                 );
                 ALTER TABLE {schema}.module_roleplay_lore_recall_traces
                    ALTER COLUMN session_id DROP NOT NULL;
                 CREATE INDEX IF NOT EXISTS roleplay_lore_recall_traces_session_idx
                    ON {schema}.module_roleplay_lore_recall_traces(session_id, created_at DESC, trace_id);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_layer_config (
                    config_id TEXT PRIMARY KEY,
                    layer_id TEXT NOT NULL UNIQUE REFERENCES {schema}.module_roleplay_lore_layers(layer_id) ON DELETE CASCADE,
                    fts_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
                    subject_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
                    canon_weight DOUBLE PRECISION NOT NULL DEFAULT 0.5,
                    tag_boost_weight DOUBLE PRECISION NOT NULL DEFAULT 0.5,
                    recency_weight DOUBLE PRECISION NOT NULL DEFAULT 0.2,
                    default_token_budget BIGINT NOT NULL DEFAULT 4000,
                    constant_token_reserve BIGINT NOT NULL DEFAULT 500,
                    min_relevance_score DOUBLE PRECISION NOT NULL DEFAULT 0.3,
                    max_constants BIGINT NOT NULL DEFAULT 5,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );"
            ))
            .map_err(|error| postgres_error("migrate PostgreSQL durable backend baseline", error))
}

fn apply_postgres_curator_governance(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.module_curator_candidates (
            candidate_id TEXT PRIMARY KEY, batch_id TEXT NOT NULL, profile_id TEXT NOT NULL,
            session_id TEXT, status TEXT NOT NULL, lifecycle_state TEXT NOT NULL,
            fingerprint TEXT NOT NULL, expires_at TEXT, revision BIGINT NOT NULL,
            record_json JSONB NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS module_curator_candidates_profile_status_idx ON {schema}.module_curator_candidates(profile_id, status, updated_at DESC, candidate_id);
         CREATE INDEX IF NOT EXISTS module_curator_candidates_profile_lifecycle_idx ON {schema}.module_curator_candidates(profile_id, lifecycle_state, updated_at DESC, candidate_id);
         CREATE INDEX IF NOT EXISTS module_curator_candidates_batch_idx ON {schema}.module_curator_candidates(batch_id, candidate_id);
         CREATE INDEX IF NOT EXISTS module_curator_candidates_session_idx ON {schema}.module_curator_candidates(session_id, updated_at DESC, candidate_id);
         CREATE INDEX IF NOT EXISTS module_curator_candidates_expires_idx ON {schema}.module_curator_candidates(expires_at);
         CREATE TABLE IF NOT EXISTS {schema}.module_curator_approvals (
            approval_id TEXT PRIMARY KEY, receipt_id TEXT NOT NULL UNIQUE, candidate_id TEXT NOT NULL,
            actor_id TEXT, approved_at TEXT NOT NULL, record_json JSONB NOT NULL);
         CREATE INDEX IF NOT EXISTS module_curator_approvals_candidate_idx ON {schema}.module_curator_approvals(candidate_id, approved_at DESC, approval_id);
         CREATE INDEX IF NOT EXISTS module_curator_approvals_actor_idx ON {schema}.module_curator_approvals(actor_id, approved_at DESC, approval_id);
         CREATE TABLE IF NOT EXISTS {schema}.module_curator_snapshot_refs (
            snapshot_id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL, status TEXT NOT NULL,
            created_at TEXT NOT NULL, record_json JSONB NOT NULL);
         CREATE INDEX IF NOT EXISTS module_curator_snapshots_candidate_idx ON {schema}.module_curator_snapshot_refs(candidate_id, created_at DESC, snapshot_id);
         CREATE TABLE IF NOT EXISTS {schema}.module_curator_mutations (
            mutation_id TEXT PRIMARY KEY, receipt_id TEXT NOT NULL UNIQUE, candidate_id TEXT NOT NULL,
            snapshot_id TEXT NOT NULL, actor_id TEXT, status TEXT NOT NULL, revision BIGINT NOT NULL,
            record_json JSONB NOT NULL, created_at TEXT NOT NULL, applied_at TEXT, rolled_back_at TEXT);
         CREATE INDEX IF NOT EXISTS module_curator_mutations_candidate_idx ON {schema}.module_curator_mutations(candidate_id, created_at DESC, mutation_id);
         CREATE INDEX IF NOT EXISTS module_curator_mutations_status_idx ON {schema}.module_curator_mutations(status, created_at DESC, mutation_id);
         CREATE INDEX IF NOT EXISTS module_curator_mutations_snapshot_idx ON {schema}.module_curator_mutations(snapshot_id);
         CREATE INDEX IF NOT EXISTS module_curator_mutations_actor_idx ON {schema}.module_curator_mutations(actor_id, created_at DESC, mutation_id);
         CREATE TABLE IF NOT EXISTS {schema}.module_curator_audit_receipts (
            sequence BIGSERIAL PRIMARY KEY, receipt_id TEXT NOT NULL UNIQUE, correlation_id TEXT,
            idempotency_key TEXT, profile_id TEXT, session_id TEXT, candidate_id TEXT, mutation_id TEXT,
            activity_kind TEXT NOT NULL, outcome TEXT NOT NULL, reason_code TEXT, occurred_at TEXT NOT NULL,
            record_json JSONB NOT NULL, UNIQUE(activity_kind, idempotency_key));
         CREATE INDEX IF NOT EXISTS module_curator_audit_candidate_idx ON {schema}.module_curator_audit_receipts(candidate_id, sequence);
         CREATE INDEX IF NOT EXISTS module_curator_audit_mutation_idx ON {schema}.module_curator_audit_receipts(mutation_id, sequence);
         CREATE INDEX IF NOT EXISTS module_curator_audit_profile_idx ON {schema}.module_curator_audit_receipts(profile_id, sequence);
         CREATE INDEX IF NOT EXISTS module_curator_audit_session_idx ON {schema}.module_curator_audit_receipts(session_id, sequence);
         CREATE INDEX IF NOT EXISTS module_curator_audit_kind_idx ON {schema}.module_curator_audit_receipts(activity_kind, sequence);
         CREATE INDEX IF NOT EXISTS module_curator_audit_time_idx ON {schema}.module_curator_audit_receipts(occurred_at, sequence);"
    ))
    .map_err(|error| postgres_error("create typed PostgreSQL curator governance tables", error))
}

fn apply_postgres_chat_event_log(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.chat_events (
            session_id TEXT NOT NULL,
            sequence_id BIGINT NOT NULL,
            event_id TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            PRIMARY KEY(session_id, sequence_id)
         );
         CREATE INDEX IF NOT EXISTS chat_events_session_created_idx
            ON {schema}.chat_events(session_id, created_at, sequence_id);
         CREATE INDEX IF NOT EXISTS chat_events_kind_idx
            ON {schema}.chat_events(kind, created_at, session_id, sequence_id);"
    ))
    .map_err(|error| postgres_error("apply PostgreSQL chat event log migration", error))
}

fn apply_postgres_roleplay_records(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_characters (
            character_id TEXT PRIMARY KEY, profile_id TEXT NOT NULL, status TEXT NOT NULL,
            name TEXT NOT NULL, revision BIGINT NOT NULL, record_json JSONB NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_characters_profile_status_idx
            ON {schema}.module_roleplay_characters(profile_id, status, updated_at DESC, character_id);
         CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_player_personas (
            persona_id TEXT PRIMARY KEY, profile_id TEXT NOT NULL, status TEXT NOT NULL,
            display_name TEXT NOT NULL, revision BIGINT NOT NULL, record_json JSONB NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_personas_profile_status_idx
            ON {schema}.module_roleplay_player_personas(profile_id, status, updated_at DESC, persona_id);
         CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_session_metadata (
            session_id TEXT PRIMARY KEY, profile_id TEXT NOT NULL, archived BOOLEAN NOT NULL,
            character_id TEXT, persona_id TEXT, revision BIGINT NOT NULL,
            record_json JSONB NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_sessions_profile_archived_idx
            ON {schema}.module_roleplay_session_metadata(profile_id, archived, updated_at DESC, session_id);
         CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_imports (
            import_id TEXT PRIMARY KEY, profile_id TEXT NOT NULL, session_id TEXT NOT NULL,
            source_kind TEXT NOT NULL, status TEXT NOT NULL, revision BIGINT NOT NULL,
            record_json JSONB NOT NULL, imported_at TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_imports_profile_status_idx
            ON {schema}.module_roleplay_imports(profile_id, status, imported_at DESC, import_id);
         CREATE INDEX IF NOT EXISTS roleplay_imports_session_idx
            ON {schema}.module_roleplay_imports(session_id, imported_at DESC, import_id);"
    ))
    .map_err(|error| postgres_error("apply PostgreSQL typed roleplay record migration", error))
}

fn prepare_postgres_migration_metadata(client: &mut Client, schema: &str) -> CoreResult<()> {
    client
        .batch_execute(&format!(
            "CREATE SCHEMA IF NOT EXISTS {schema};
             CREATE TABLE IF NOT EXISTS {schema}.rusty_crew_storage_metadata (
                metadata_key TEXT PRIMARY KEY,
                metadata_value TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS {schema}.schema_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL DEFAULT '',
                applied_at TEXT NOT NULL DEFAULT to_char(
                    CURRENT_TIMESTAMP AT TIME ZONE 'UTC',
                    'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'
                )
             );"
        ))
        .map_err(|error| postgres_error("prepare PostgreSQL schema migration metadata", error))
}

fn apply_postgres_schema_migrations(client: &mut Client, schema: &str) -> CoreResult<()> {
    validate_postgres_migration_catalog(POSTGRES_SCHEMA_MIGRATIONS)?;
    let migration_version = current_postgres_schema_version(client, schema)?;
    let legacy_version = legacy_postgres_schema_version(client, schema)?;
    let current_version = migration_version.max(legacy_version.unwrap_or(0));

    if current_version > POSTGRES_SCHEMA_VERSION {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "postgres schema version {current_version} is newer than supported version {POSTGRES_SCHEMA_VERSION}"
            ),
        ));
    }
    if current_version > 0 && current_version < POSTGRES_MIN_SUPPORTED_SCHEMA_VERSION {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "postgres schema version {current_version} is older than supported version {POSTGRES_MIN_SUPPORTED_SCHEMA_VERSION}"
            ),
        ));
    }

    if migration_version == 0 && current_version > 0 {
        backfill_postgres_schema_migrations(client, schema, current_version)?;
    }

    for migration in POSTGRES_SCHEMA_MIGRATIONS {
        let current = current_postgres_schema_version(client, schema)?;
        if migration.version <= current {
            refresh_postgres_schema_migration_description(
                client,
                schema,
                migration.version,
                migration.description,
            )?;
            continue;
        }

        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL schema migration", error))?;
        if let Some(apply) = migration.apply {
            apply(&mut tx, schema)?;
        }
        insert_postgres_schema_migration(&mut tx, schema, migration)?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL schema migration", error))?;
    }

    Ok(())
}

fn validate_postgres_migration_catalog(migrations: &[PostgresSchemaMigration]) -> CoreResult<()> {
    for (index, migration) in migrations.iter().enumerate() {
        let expected = (index as i64) + 1;
        if migration.version != expected {
            return Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "invalid postgres schema migration catalog: expected version {expected}, found {}",
                    migration.version
                ),
            ));
        }
    }
    if migrations.last().map(|migration| migration.version) != Some(POSTGRES_SCHEMA_VERSION) {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "invalid postgres schema migration catalog: supported version {POSTGRES_SCHEMA_VERSION} is not the final migration"
            ),
        ));
    }
    Ok(())
}

fn current_postgres_schema_version<C: GenericClient>(
    client: &mut C,
    schema: &str,
) -> CoreResult<i64> {
    client
        .query_one(
            &format!("SELECT COALESCE(MAX(version), 0) FROM {schema}.schema_migrations"),
            &[],
        )
        .map(|row| row.get::<_, i64>(0))
        .map_err(|error| postgres_error("read PostgreSQL schema version", error))
}

fn legacy_postgres_schema_version<C: GenericClient>(
    client: &mut C,
    schema: &str,
) -> CoreResult<Option<i64>> {
    let row = client
        .query_opt(
            &format!(
                "SELECT metadata_value
                 FROM {schema}.rusty_crew_storage_metadata
                 WHERE metadata_key = 'runtime_counter_proof_schema_version'"
            ),
            &[],
        )
        .map_err(|error| postgres_error("read legacy PostgreSQL schema metadata", error))?;
    row.map(|row| {
        let raw: String = row.get(0);
        raw.parse::<i64>().map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse legacy PostgreSQL schema version: {error}"),
            )
        })
    })
    .transpose()
}

fn backfill_postgres_schema_migrations(
    client: &mut Client,
    schema: &str,
    through_version: i64,
) -> CoreResult<()> {
    let mut tx = client
        .transaction()
        .map_err(|error| postgres_error("start PostgreSQL schema migration backfill", error))?;
    for migration in POSTGRES_SCHEMA_MIGRATIONS {
        if migration.version > through_version {
            break;
        }
        insert_postgres_schema_migration(&mut tx, schema, migration)?;
    }
    tx.commit()
        .map_err(|error| postgres_error("commit PostgreSQL schema migration backfill", error))
}

fn insert_postgres_schema_migration<C: GenericClient>(
    client: &mut C,
    schema: &str,
    migration: &PostgresSchemaMigration,
) -> CoreResult<()> {
    client
        .execute(
            &format!(
                "INSERT INTO {schema}.schema_migrations (version, description)
                 VALUES ($1, $2)
                 ON CONFLICT(version) DO UPDATE SET description = EXCLUDED.description"
            ),
            &[&migration.version, &migration.description],
        )
        .map(|_| ())
        .map_err(|error| postgres_error("record PostgreSQL schema migration", error))
}

fn refresh_postgres_schema_migration_description<C: GenericClient>(
    client: &mut C,
    schema: &str,
    version: i64,
    description: &str,
) -> CoreResult<()> {
    client
        .execute(
            &format!("UPDATE {schema}.schema_migrations SET description = $1 WHERE version = $2"),
            &[&description, &version],
        )
        .map(|_| ())
        .map_err(|error| postgres_error("refresh PostgreSQL schema migration metadata", error))
}

fn load_postgres_schema_migration_records<C: GenericClient>(
    client: &mut C,
    schema: &str,
) -> CoreResult<Vec<SchemaMigrationRecord>> {
    let rows = client
        .query(
            &format!(
                "SELECT version, description, applied_at
                 FROM {schema}.schema_migrations
                 ORDER BY version ASC"
            ),
            &[],
        )
        .map_err(|error| postgres_error("load PostgreSQL schema migration records", error))?;
    Ok(rows
        .into_iter()
        .map(|row| SchemaMigrationRecord {
            version: row.get(0),
            description: row.get(1),
            applied_at: row.get(2),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_migration_catalog_is_ordered_and_current() {
        validate_postgres_migration_catalog(POSTGRES_SCHEMA_MIGRATIONS).unwrap();
        assert_eq!(
            POSTGRES_SCHEMA_MIGRATIONS
                .last()
                .map(|migration| migration.version),
            Some(POSTGRES_SCHEMA_VERSION)
        );
        assert_eq!(POSTGRES_SCHEMA_MIGRATIONS[0].version, 1);
        assert!(POSTGRES_SCHEMA_MIGRATIONS[0].apply.is_some());
        assert!(POSTGRES_SCHEMA_MIGRATIONS
            .iter()
            .any(|migration| { migration.version > 1 && migration.apply.is_some() }));
    }

    #[test]
    fn postgres_migration_catalog_rejects_gaps() {
        let migrations = [
            PostgresSchemaMigration {
                version: 1,
                description: "first",
                apply: None,
            },
            PostgresSchemaMigration {
                version: 3,
                description: "gap",
                apply: None,
            },
        ];
        let error = validate_postgres_migration_catalog(&migrations).unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(error.message.contains("expected version 2"));
    }
}
