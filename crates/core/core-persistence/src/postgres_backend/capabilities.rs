use crate::{repositories, RuntimeRepositoryGroupDiagnostic, RuntimeStorageCapability};

pub(super) fn postgres_backend_capabilities() -> Vec<RuntimeStorageCapability> {
    [
        (
            "transactions",
            true,
            "PostgreSQL transactions are available for durable service repositories",
        ),
        (
            "json_metadata",
            true,
            "PostgreSQL stores structured metadata in JSON/JSONB columns behind typed repository APIs",
        ),
        (
            "concurrent_writers",
            true,
            "PostgreSQL supports concurrent writers for durable service repositories",
        ),
        (
            "estimated_table_size",
            true,
            "the backend exposes row counts for Crew-owned tables",
        ),
        (
            "row_level_claims",
            true,
            "PostgreSQL scheduler expiry uses FOR UPDATE SKIP LOCKED row-level claim semantics",
        ),
        (
            "runtime_full_text_search",
            true,
            "PostgreSQL runtime search backend uses tsvector behind the typed RuntimeSearchFilter API",
        ),
        (
            "logical_export_import",
            false,
            "logical cross-backend export/import remains future work",
        ),
        (
            "online_migrations",
            false,
            "schema migrations run during service startup/open and fail closed on unsupported versions",
        ),
    ]
    .into_iter()
    .map(|(name, supported, detail)| RuntimeStorageCapability {
        name: name.to_string(),
        supported,
        detail: detail.to_string(),
    })
    .collect()
}

pub(super) fn postgres_backend_repository_groups() -> Vec<RuntimeRepositoryGroupDiagnostic> {
    repositories::core_repository_group_diagnostics()
        .into_iter()
        .map(|mut group| {
            if group.group_id == "storage_admin" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: startup applies versioned migrations and reports storage diagnostics.".to_string(),
                );
            } else if group.group_id == "sessions_identities" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for session/config/identity hydration conformance.".to_string(),
                );
            } else if group.group_id == "events_projections" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for event history and typed event-index query conformance.".to_string(),
                );
            } else if group.group_id == "queues_messages" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for queued-message TTL and no-resurrection conformance.".to_string(),
                );
            } else if group.group_id == "scheduler_jobs" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for scheduled jobs, scheduled run claim/completion, stale-run expiry, and row-level claim conformance.".to_string(),
                );
            } else if group.group_id == "worker_runs_completions" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for worker run lifecycle, delegated completion lookup, completion packet persistence, and terminal-status conformance.".to_string(),
                );
            } else if group.group_id == "module_schema_registry" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: module schema diagnostics are projected from the compiled registry; module simple_kv storage is implemented.".to_string(),
                );
            } else if group.group_id == "runtime_counters" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for runtime counters.".to_string(),
                );
            } else if group.group_id == "runtime_search" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for runtime search entries through the typed search API.".to_string(),
                );
            } else if group.group_id == "provider_state" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for provider wire-state conformance through the typed provider-state API.".to_string(),
                );
            } else if group.group_id == "conversations_attachments" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for conversation transcript, attachment, and data-bank repository surfaces.".to_string(),
                );
            } else if group.group_id == "profile_memory" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for profile_dense descriptor projection, dense profile memory conformance, and roleplay_lore record/layer physical schema.".to_string(),
                );
            } else if group.group_id == "install_diplomat" {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: implemented for revisioned install-diplomat bindings and restart-safe Telegram interaction budgets.".to_string(),
                );
            } else {
                group.notes.insert(
                    0,
                    "PostgreSQL durable backend status: repository group still needs parity review before it can be considered stable under load.".to_string(),
                );
            }
            group
        })
        .collect()
}
