//! PostgreSQL schema migration catalog and application logic.

use super::install_diplomat::apply_postgres_install_diplomat_state;
use super::logical_turns::apply_postgres_logical_turns;
use super::review_submissions::{
    allow_external_review_submitters, apply_postgres_review_submissions,
};
use super::runtime_activities::apply_postgres_runtime_activities;
use super::*;

pub(super) const POSTGRES_SCHEMA_VERSION: i64 = 59;
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
    PostgresSchemaMigration {
        version: 20,
        description: "add managed external agent runtime lifecycle records",
        apply: Some(apply_postgres_external_runtime),
    },
    PostgresSchemaMigration {
        version: 21,
        description: "add runtime-neutral agent delivery and correlated round records",
        apply: Some(apply_postgres_agent_coordination),
    },
    PostgresSchemaMigration {
        version: 22,
        description: "add typed bounded chat message ingest receipts",
        apply: Some(apply_postgres_chat_message_ingest_receipts),
    },
    PostgresSchemaMigration {
        version: 23,
        description: "add idempotent external agent session creation records",
        apply: Some(apply_postgres_external_agent_session_creations),
    },
    PostgresSchemaMigration {
        version: 24,
        description: "allow system operator correlated rounds without fake sessions",
        apply: Some(apply_postgres_operator_agent_rounds),
    },
    PostgresSchemaMigration {
        version: 25,
        description: "add roleplay lore recall entry decisions",
        apply: Some(apply_postgres_roleplay_lore_recall_entry_decisions),
    },
    PostgresSchemaMigration {
        version: 26,
        description: "add durable roleplay mechanic proposals",
        apply: Some(apply_postgres_roleplay_mechanic_proposals),
    },
    PostgresSchemaMigration {
        version: 27,
        description: "add roleplay mechanic session associations and diagnostics",
        apply: Some(apply_postgres_roleplay_mechanic_sessions_and_diagnostics),
    },
    PostgresSchemaMigration {
        version: 28,
        description: "replace exact external runtime pins with compatibility state",
        apply: Some(apply_postgres_external_runtime_compatibility_state),
    },
    PostgresSchemaMigration {
        version: 29,
        description: "add external runtime compatibility probe diagnostics",
        apply: Some(apply_postgres_external_runtime_compatibility_probe),
    },
    PostgresSchemaMigration {
        version: 30,
        description: "add typed external runtime compatibility certifications",
        apply: Some(apply_postgres_external_runtime_certifications),
    },
    PostgresSchemaMigration {
        version: 31,
        description: "rename chat completions brain identity",
        apply: Some(apply_postgres_rename_chat_completions_brain),
    },
    PostgresSchemaMigration {
        version: 32,
        description: "add agent message session provenance and reply linkage",
        apply: Some(apply_postgres_agent_message_reply_links),
    },
    PostgresSchemaMigration {
        version: 33,
        description: "add explicit agent message input kind",
        apply: Some(apply_postgres_agent_message_input_kind),
    },
    PostgresSchemaMigration {
        version: 34,
        description: "add agent message input kind to durable event history",
        apply: Some(apply_postgres_agent_message_event_input_kind),
    },
    PostgresSchemaMigration {
        version: 35,
        description: "repair agent message input kind event discriminator",
        apply: Some(apply_postgres_agent_message_event_input_kind),
    },
    PostgresSchemaMigration {
        version: 36,
        description: "add service-scoped provider credentials",
        apply: Some(apply_postgres_service_credentials),
    },
    PostgresSchemaMigration {
        version: 37,
        description: "record typed chat completions dialect policy in provider JSON",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 38,
        description: "add agent routing switchboard",
        apply: Some(apply_postgres_agent_routes),
    },
    PostgresSchemaMigration {
        version: 39,
        description: "add requested address to durable agent delivery history",
        apply: Some(apply_postgres_agent_delivery_requested_address),
    },
    PostgresSchemaMigration {
        version: 40,
        description: "add typed runtime activity accounting",
        apply: Some(apply_postgres_runtime_activities),
    },
    PostgresSchemaMigration {
        version: 41,
        description: "add durable logical brain turn continuation",
        apply: Some(apply_postgres_logical_turns),
    },
    PostgresSchemaMigration {
        version: 42,
        description: "add bounded external runtime event retention checkpoints",
        apply: Some(apply_postgres_external_runtime_event_retention),
    },
    PostgresSchemaMigration {
        version: 43,
        description: "record typed chat completions prompt caching policy in provider JSON",
        apply: None,
    },
    PostgresSchemaMigration {
        version: 44,
        description: "record explicit Responses provider dialect in provider JSON",
        apply: Some(apply_postgres_responses_provider_dialect),
    },
    PostgresSchemaMigration {
        version: 45,
        description: "add durable review submission workflows",
        apply: Some(apply_postgres_review_submissions),
    },
    PostgresSchemaMigration {
        version: 46,
        description: "allow external CLI review submissions without sessions",
        apply: Some(allow_external_review_submitters),
    },
    PostgresSchemaMigration {
        version: 47,
        description: "add atomic idempotency for manual compaction intent",
        apply: Some(apply_postgres_context_compaction_intent_unique),
    },
    PostgresSchemaMigration {
        version: 48,
        description: "make compaction idempotency projection-aware",
        apply: Some(apply_postgres_context_compaction_intent_projection_aware),
    },
    PostgresSchemaMigration {
        version: 49,
        description: "add provider state compatibility lineage",
        apply: Some(apply_postgres_provider_state_compatibility_lineage),
    },
    PostgresSchemaMigration {
        version: 50,
        description: "store first-class session workspace in session JSON",
        apply: Some(apply_postgres_session_workspace),
    },
    PostgresSchemaMigration {
        version: 51,
        description: "migrate legacy delegated workspace constraints",
        apply: Some(apply_postgres_delegated_workspace_constraints),
    },
    PostgresSchemaMigration {
        version: 52,
        description: "migrate legacy session workspace event payloads",
        apply: Some(apply_postgres_session_workspace_events),
    },
    PostgresSchemaMigration {
        version: 53,
        description: "index external runtime events by native thread cursor",
        apply: Some(apply_postgres_external_runtime_thread_cursor),
    },
    PostgresSchemaMigration {
        version: 54,
        description: "add durable Telegram install diplomat coordination state",
        apply: Some(apply_postgres_install_diplomat_state),
    },
    PostgresSchemaMigration {
        version: 55,
        description: "index external turns by immutable creation cursor",
        apply: Some(apply_postgres_external_turn_creation_cursor),
    },
    PostgresSchemaMigration {
        version: 56,
        description: "order external turns by backend-owned creation ordinal",
        apply: Some(apply_postgres_external_turn_creation_ordinal),
    },
    PostgresSchemaMigration {
        version: 57,
        description: "repair missing external turn creation ordinal",
        apply: Some(apply_postgres_external_turn_creation_ordinal),
    },
    PostgresSchemaMigration {
        version: 58,
        description: "repair skipped quoted-schema external runtime migrations",
        apply: Some(apply_postgres_quoted_schema_external_runtime_repairs),
    },
    PostgresSchemaMigration {
        version: 59,
        description: "add normalized model endpoint and configuration registries",
        apply: Some(super::model_registry::apply_postgres_model_registry),
    },
];

fn postgres_catalog_schema_name(schema: &str) -> String {
    schema
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(schema)
        .replace("\"\"", "\"")
}

fn apply_postgres_quoted_schema_external_runtime_repairs(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    apply_postgres_external_runtime_thread_cursor(tx, schema)?;
    apply_postgres_external_turn_creation_cursor(tx, schema)?;
    apply_postgres_external_turn_creation_ordinal(tx, schema)
}

fn apply_postgres_external_turn_creation_ordinal(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    let catalog_schema = postgres_catalog_schema_name(schema);
    let external_turns_exists = tx
        .query_opt(
            "SELECT 1 FROM information_schema.tables
              WHERE table_schema::text = $1 AND table_name = 'external_turns'",
            &[&catalog_schema],
        )
        .map_err(|error| postgres_error("inspect PostgreSQL external turns table", error))?
        .is_some();
    if !external_turns_exists {
        return Ok(());
    }
    tx.batch_execute(&format!(
        "CREATE SEQUENCE IF NOT EXISTS {schema}.external_turn_creation_ordinal_seq;
         ALTER TABLE {schema}.external_turns ADD COLUMN IF NOT EXISTS creation_ordinal BIGINT;
         WITH ranked AS (
            SELECT request_id,
                   ROW_NUMBER() OVER (ORDER BY created_at, request_id) AS ordinal
              FROM {schema}.external_turns
         )
         UPDATE {schema}.external_turns AS turns
            SET creation_ordinal = ranked.ordinal
           FROM ranked
          WHERE turns.request_id = ranked.request_id
            AND turns.creation_ordinal IS NULL;
         SELECT setval(
            '{schema}.external_turn_creation_ordinal_seq',
            GREATEST(COALESCE(MAX(creation_ordinal), 0), 1),
            COALESCE(MAX(creation_ordinal), 0) > 0
         ) FROM {schema}.external_turns;
         ALTER TABLE {schema}.external_turns
            ALTER COLUMN creation_ordinal SET DEFAULT nextval('{schema}.external_turn_creation_ordinal_seq'),
            ALTER COLUMN creation_ordinal SET NOT NULL;
         CREATE UNIQUE INDEX IF NOT EXISTS external_turns_creation_ordinal_idx
            ON {schema}.external_turns(creation_ordinal);
         CREATE INDEX IF NOT EXISTS external_turns_thread_creation_ordinal_idx
            ON {schema}.external_turns(runtime_id, native_thread_id, creation_ordinal);"
    ))
    .map_err(|error| postgres_error("add PostgreSQL external turn creation ordinal", error))
}

fn apply_postgres_external_turn_creation_cursor(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    let catalog_schema = postgres_catalog_schema_name(schema);
    let external_turns_exists = tx
        .query_opt(
            "SELECT 1
               FROM information_schema.tables
              WHERE table_schema::text = $1
                AND table_name = 'external_turns'",
            &[&catalog_schema],
        )
        .map_err(|error| postgres_error("inspect PostgreSQL external turns table", error))?
        .is_some();
    if !external_turns_exists {
        return Ok(());
    }
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.external_turns ADD COLUMN IF NOT EXISTS created_at TEXT;
         UPDATE {schema}.external_turns
            SET created_at = record_json::jsonb #>> '{{request,createdAt}}'
          WHERE created_at IS NULL;
         ALTER TABLE {schema}.external_turns ALTER COLUMN created_at SET NOT NULL;
         CREATE INDEX IF NOT EXISTS external_turns_creation_cursor_idx
            ON {schema}.external_turns(runtime_id, native_thread_id, created_at, request_id);"
    ))
    .map_err(|error| postgres_error("add PostgreSQL external turn creation cursor", error))
}

fn apply_postgres_external_runtime_thread_cursor(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    let catalog_schema = postgres_catalog_schema_name(schema);
    let external_runtime_events_exists = tx
        .query_opt(
            "SELECT 1
               FROM information_schema.tables
              WHERE table_schema::text = $1
                AND table_name = 'external_runtime_events'",
            &[&catalog_schema],
        )
        .map_err(|error| postgres_error("inspect PostgreSQL external runtime events table", error))?
        .is_some();
    if !external_runtime_events_exists {
        return Ok(());
    }
    tx.batch_execute(&format!(
        "CREATE INDEX IF NOT EXISTS external_runtime_events_thread_cursor_idx
            ON {schema}.external_runtime_events(runtime_id, native_thread_id, sequence_id)
            WHERE native_thread_id IS NOT NULL;"
    ))
    .map_err(|error| postgres_error("add PostgreSQL external runtime thread cursor index", error))
}

fn apply_postgres_session_workspace_events(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.event_history
            SET event_json = (
                CASE
                    WHEN event_json::jsonb #>> '{{state,kind}}' = 'full'
                         AND event_json::jsonb #> '{{state,workspace}}' IS NULL
                    THEN jsonb_set(
                        event_json::jsonb,
                        '{{state,workspace}}',
                        jsonb_build_object(
                            'cwd', event_json::jsonb #> '{{state,resource_limits,workdir}}',
                            'revision', 1,
                            'updated_at', COALESCE(
                                event_json::jsonb #> '{{state,last_active_at}}',
                                to_jsonb(recorded_at)
                            )
                        )
                    )
                    WHEN event_json::jsonb #>> '{{state,kind}}' = 'delegated'
                         AND event_json::jsonb #> '{{state,delegation,workspace_constraint}}' IS NULL
                    THEN jsonb_set(
                        event_json::jsonb,
                        '{{state,delegation,workspace_constraint}}',
                        jsonb_build_object(
                            'cwd', event_json::jsonb #> '{{state,resource_limits,workdir}}'
                        )
                    )
                    ELSE event_json::jsonb
                END #- '{{state,resource_limits,workdir}}'
            )::text
          WHERE event_kind = 'SessionCreated'
            AND event_json::jsonb #> '{{state,resource_limits,workdir}}' IS NOT NULL;"
    ))
    .map_err(|error| postgres_error("migrate session workspace event payloads", error))
}

fn apply_postgres_delegated_workspace_constraints(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.sessions
            SET state_json = (
                CASE
                    WHEN state_json::jsonb #> '{{delegation,workspace_constraint}}' IS NULL
                    THEN jsonb_set(
                        state_json::jsonb,
                        '{{delegation,workspace_constraint}}',
                        jsonb_build_object(
                            'cwd', state_json::jsonb #> '{{resource_limits,workdir}}'
                        )
                    )
                    ELSE state_json::jsonb
                END #- '{{resource_limits,workdir}}'
            )::text
          WHERE kind = 'delegated'
            AND state_json::jsonb #> '{{resource_limits,workdir}}' IS NOT NULL;
         UPDATE {schema}.session_configs
            SET record_json = (
                CASE
                    WHEN record_json::jsonb #> '{{delegation,workspace_constraint}}' IS NULL
                    THEN jsonb_set(
                        record_json::jsonb,
                        '{{delegation,workspace_constraint}}',
                        jsonb_build_object(
                            'cwd', record_json::jsonb #> '{{resource_limits,workdir}}'
                        )
                    )
                    ELSE record_json::jsonb
                END #- '{{resource_limits,workdir}}'
            )::text
          WHERE kind = 'delegated'
            AND record_json::jsonb #> '{{resource_limits,workdir}}' IS NOT NULL;"
    ))
    .map_err(|error| postgres_error("migrate delegated workspace constraints", error))
}

fn apply_postgres_session_workspace(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.sessions
            SET state_json = jsonb_set(
                state_json::jsonb #- '{{resource_limits,workdir}}',
                '{{workspace}}',
                jsonb_build_object(
                    'cwd', state_json::jsonb #> '{{resource_limits,workdir}}',
                    'revision', 1,
                    'updated_at', last_active_at
                )
            )::text
          WHERE kind = 'full'
            AND state_json::jsonb -> 'workspace' IS NULL
            AND state_json::jsonb #> '{{resource_limits,workdir}}' IS NOT NULL;
         UPDATE {schema}.session_configs
            SET record_json = jsonb_set(
                record_json::jsonb #- '{{resource_limits,workdir}}',
                '{{workspace}}',
                jsonb_build_object(
                    'cwd', record_json::jsonb #> '{{resource_limits,workdir}}',
                    'revision', 1,
                    'updated_at', created_at
                )
            )::text
          WHERE kind = 'full'
            AND record_json::jsonb -> 'workspace' IS NULL
            AND record_json::jsonb #> '{{resource_limits,workdir}}' IS NOT NULL;"
    ))
    .map_err(|error| postgres_error("migrate first-class session workspaces", error))
}

fn apply_postgres_provider_state_compatibility_lineage(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.provider_wire_states
             ADD COLUMN IF NOT EXISTS compatibility_snapshot_json TEXT;
         ALTER TABLE {schema}.provider_wire_states
             ADD COLUMN IF NOT EXISTS compatibility_plan_json TEXT;"
    ))
    .map_err(|error| postgres_error("add PostgreSQL provider state compatibility lineage", error))
}

fn apply_postgres_responses_provider_dialect(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.model_providers
            SET provider_json = (
                provider_json::jsonb || jsonb_build_object(
                    'responses_dialect',
                    CASE
                        WHEN provider_json::jsonb ->> 'provider_kind' = 'openai'
                            THEN 'openai_stateful'
                        WHEN provider_json::jsonb ->> 'provider_kind' = 'deepseek'
                            THEN 'deepseek'
                        ELSE 'generic_stateless'
                    END
                )
            )::text
          WHERE protocol = 'responses'
            AND NOT (provider_json::jsonb ? 'responses_dialect');"
    ))
    .map_err(|error| postgres_error("add PostgreSQL Responses provider dialect", error))
}

fn apply_postgres_external_runtime_event_retention(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    let record_jsonb = postgres_external_event_record_jsonb_expression();
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.external_runtime_events ADD COLUMN native_thread_id TEXT;
         ALTER TABLE {schema}.external_runtime_events ADD COLUMN native_turn_id TEXT;
         UPDATE {schema}.external_runtime_events
            SET native_thread_id = {record_jsonb} ->> 'nativeThreadId',
                native_turn_id = {record_jsonb} ->> 'nativeTurnId';
         CREATE INDEX external_runtime_events_turn_cursor_idx
            ON {schema}.external_runtime_events(runtime_id, native_turn_id, sequence_id)
            WHERE native_turn_id IS NOT NULL;
         CREATE INDEX external_runtime_events_created_cursor_idx
            ON {schema}.external_runtime_events(runtime_id, created_at, sequence_id);
         CREATE INDEX external_turns_terminal_retention_idx
            ON {schema}.external_turns(phase, updated_at, runtime_id, native_turn_id)
            WHERE native_turn_id IS NOT NULL;
         CREATE TABLE {schema}.external_runtime_event_cursors (
            runtime_id TEXT PRIMARY KEY REFERENCES {schema}.external_runtime_registrations(runtime_id),
            next_sequence_id BIGINT NOT NULL
         );
         INSERT INTO {schema}.external_runtime_event_cursors(runtime_id, next_sequence_id)
         SELECT registration.runtime_id, COALESCE(MAX(event.sequence_id), 0) + 1
           FROM {schema}.external_runtime_registrations registration
           LEFT JOIN {schema}.external_runtime_events event ON event.runtime_id = registration.runtime_id
          GROUP BY registration.runtime_id;
         CREATE TABLE {schema}.external_runtime_event_checkpoints (
            runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
            native_turn_id TEXT NOT NULL,
            native_thread_id TEXT NOT NULL,
            session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            terminal_phase TEXT NOT NULL,
            terminal_at TEXT NOT NULL,
            first_sequence_id BIGINT NOT NULL,
            last_sequence_id BIGINT NOT NULL,
            compacted_event_count BIGINT NOT NULL,
            estimated_compacted_bytes BIGINT NOT NULL,
            kind_counts_json TEXT NOT NULL,
            checkpointed_at TEXT NOT NULL,
            policy_cutoff TEXT NOT NULL,
            PRIMARY KEY(runtime_id, native_turn_id)
         );
         CREATE INDEX external_runtime_event_checkpoints_time_idx
            ON {schema}.external_runtime_event_checkpoints(checkpointed_at, runtime_id, native_turn_id);"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL external runtime event retention checkpoints",
            error,
        )
    })
}

fn postgres_external_event_record_jsonb_expression() -> &'static str {
    // PostgreSQL JSONB cannot represent NUL, while historical JSON text can
    // validly contain its escaped form. Match only an active JSON escape: an
    // even-length run of preceding backslashes is structural, while an odd
    // run denotes literal backslash text. Normalize only the migration read;
    // the durable event payload remains byte-for-byte unchanged.
    r#"regexp_replace(record_json, $$((?<!\\)(?:\\\\)*)\\u0000$$, $$\1\\ufffd$$, 'g')::jsonb"#
}

fn apply_postgres_agent_routes(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE {schema}.agent_routes (
            route_key TEXT PRIMARY KEY,
            enabled BOOLEAN NOT NULL,
            target_kind TEXT NOT NULL,
            target_agent_id TEXT NOT NULL,
            target_session_id TEXT,
            target_binding_id TEXT,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX agent_routes_enabled_idx
            ON {schema}.agent_routes(enabled, route_key);
         CREATE INDEX agent_routes_direct_target_idx
            ON {schema}.agent_routes(target_agent_id, target_session_id);
         CREATE INDEX agent_routes_external_target_idx
            ON {schema}.agent_routes(target_binding_id)
            WHERE target_binding_id IS NOT NULL;"
    ))
    .map_err(|error| postgres_error("create PostgreSQL agent route tables", error))?;
    Ok(())
}

fn apply_postgres_agent_delivery_requested_address(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.agent_message_delivery_receipts
            SET record_json = jsonb_set(
                record_json::jsonb,
                '{{request,requestedAddress}}',
                record_json::jsonb #> '{{request,toAgentId}}',
                true
            )::text
          WHERE record_json::jsonb #> '{{request,requestedAddress}}' IS NULL;
         UPDATE {schema}.event_history
            SET event_json = jsonb_set(
                event_json::jsonb,
                '{{receipt,request,requestedAddress}}',
                event_json::jsonb #> '{{receipt,request,toAgentId}}',
                true
            )::text
          WHERE event_json::jsonb #> '{{receipt,request}}' IS NOT NULL
            AND event_json::jsonb #> '{{receipt,request,requestedAddress}}' IS NULL;"
    ))
    .map_err(|error| {
        postgres_error(
            "migrate PostgreSQL agent delivery requested addresses",
            error,
        )
    })?;
    Ok(())
}

fn apply_postgres_service_credentials(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE {schema}.service_credentials (
            credential_id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            provider_kind TEXT NOT NULL,
            credential_kind TEXT NOT NULL,
            secret_ciphertext TEXT,
            secret_updated_at TEXT,
            revision BIGINT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX service_credentials_provider_kind_idx
            ON {schema}.service_credentials(provider_kind, updated_at DESC, credential_id);
         ALTER TABLE {schema}.model_providers
            ADD COLUMN credential_id TEXT REFERENCES {schema}.service_credentials(credential_id);
         CREATE INDEX model_providers_credential_idx
            ON {schema}.model_providers(credential_id, alias);
         INSERT INTO {schema}.service_credentials (
            credential_id, display_name, provider_kind, credential_kind,
            secret_ciphertext, secret_updated_at, revision, created_at, updated_at
         )
         SELECT
            'provider:' || alias,
            COALESCE(NULLIF(provider_json::jsonb ->> 'display_name', ''), alias),
            provider_json::jsonb ->> 'provider_kind',
            CASE
                WHEN secret_ciphertext LIKE '{{%'
                 AND secret_ciphertext::jsonb ->> 'kind' = 'openai_oauth'
                THEN 'openai_oauth'
                ELSE 'api_key'
            END,
            secret_ciphertext,
            secret_updated_at,
            1,
            created_at,
            COALESCE(secret_updated_at, updated_at)
         FROM {schema}.model_providers
         WHERE secret_ciphertext IS NOT NULL;
         UPDATE {schema}.model_providers
            SET credential_id = 'provider:' || alias,
                secret_ciphertext = NULL,
                secret_updated_at = NULL
          WHERE secret_ciphertext IS NOT NULL;"
    ))
    .map_err(|error| postgres_error("add PostgreSQL service credentials", error))
}

fn apply_postgres_agent_message_event_input_kind(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.agent_message_delivery_receipts
            SET record_json = jsonb_set(
                record_json::jsonb,
                '{{request,inputKind}}',
                CASE
                    WHEN record_json::jsonb #>> '{{request,fromAgentId}}' = 'rusty-view-operator'
                    THEN '\"operator\"'::jsonb
                    ELSE '\"routed_agent_message\"'::jsonb
                END,
                true
            )::text;
         UPDATE {schema}.event_history
            SET event_json = jsonb_set(
                event_json::jsonb,
                '{{receipt,request,inputKind}}',
                CASE
                    WHEN event_json::jsonb #>> '{{receipt,request,fromAgentId}}' = 'rusty-view-operator'
                    THEN '\"operator\"'::jsonb
                    ELSE '\"routed_agent_message\"'::jsonb
                END,
                true
            )::text
          WHERE event_json::jsonb #> '{{receipt,request}}' IS NOT NULL;"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL agent message input kind to event history",
            error,
        )
    })
}

fn apply_postgres_agent_message_input_kind(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "DELETE FROM {schema}.queued_messages
            WHERE state_reason LIKE 'agent_delivery:%';
         UPDATE {schema}.agent_message_delivery_receipts
            SET record_json = jsonb_set(
                record_json::jsonb,
                '{{request,inputKind}}',
                CASE
                    WHEN record_json::jsonb #>> '{{request,fromAgentId}}' = 'rusty-view-operator'
                    THEN '\"operator\"'::jsonb
                    ELSE '\"routed_agent_message\"'::jsonb
                END,
                true
            )::text;"
    ))
    .map_err(|error| postgres_error("add PostgreSQL agent message input kind", error))
}

fn apply_postgres_agent_message_reply_links(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "DELETE FROM {schema}.queued_messages
            WHERE body LIKE '[Rusty Crew routed message]%';
         DELETE FROM {schema}.agent_message_delivery_receipts;
         ALTER TABLE {schema}.agent_message_delivery_receipts
            ADD COLUMN IF NOT EXISTS from_session_id TEXT;
         ALTER TABLE {schema}.agent_message_delivery_receipts
            ADD COLUMN IF NOT EXISTS to_session_id TEXT;
         ALTER TABLE {schema}.agent_message_delivery_receipts
            ADD COLUMN IF NOT EXISTS reply_to_message_id TEXT;
         ALTER TABLE {schema}.agent_message_delivery_receipts
            ADD COLUMN IF NOT EXISTS created_at TEXT;
         CREATE UNIQUE INDEX IF NOT EXISTS agent_message_delivery_reply_once_idx
            ON {schema}.agent_message_delivery_receipts(reply_to_message_id)
            WHERE reply_to_message_id IS NOT NULL;
         CREATE INDEX IF NOT EXISTS agent_message_delivery_recipient_session_idx
            ON {schema}.agent_message_delivery_receipts(to_session_id, status, expires_at);"
    ))
    .map_err(|error| postgres_error("add PostgreSQL agent message reply linkage", error))
}

fn apply_postgres_rename_chat_completions_brain(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.profile_registry
            SET record_json = (
                CASE
                    WHEN record_json::jsonb #>> '{{active_runtime_settings_json,brain,module}}' = 'pi-agent'
                    THEN jsonb_set(
                        record_json::jsonb,
                        '{{active_runtime_settings_json,brain,module}}',
                        '\"chat-completions\"'::jsonb,
                        false
                    )
                    ELSE record_json::jsonb
                END
            )::text
          WHERE record_json::jsonb #>> '{{active_runtime_settings_json,brain,module}}' = 'pi-agent';

         UPDATE {schema}.profile_registry
            SET record_json = (
                CASE
                    WHEN record_json::jsonb #>> '{{active_runtime_settings_json,profile,brain,module}}' = 'pi-agent'
                    THEN jsonb_set(
                        record_json::jsonb,
                        '{{active_runtime_settings_json,profile,brain,module}}',
                        '\"chat-completions\"'::jsonb,
                        false
                    )
                    ELSE record_json::jsonb
                END
            )::text
          WHERE record_json::jsonb #>> '{{active_runtime_settings_json,profile,brain,module}}' = 'pi-agent';

         DELETE FROM {schema}.provider_wire_states WHERE module_id = 'pi-agent';"
    ))
    .map_err(|error| postgres_error("rename PostgreSQL chat completions brain identity", error))?;
    Ok(())
}

fn apply_postgres_external_runtime_certifications(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.external_runtime_certifications (
            certification_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            runtime_kind TEXT NOT NULL,
            observed_cli_version TEXT NOT NULL,
            consumed_contract_revision TEXT NOT NULL,
            probe_suite_revision TEXT NOT NULL,
            status TEXT NOT NULL,
            revision BIGINT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS external_runtime_certifications_identity_idx
            ON {schema}.external_runtime_certifications(
                runtime_kind,
                observed_cli_version,
                consumed_contract_revision,
                probe_suite_revision,
                status
            );
         CREATE TABLE IF NOT EXISTS {schema}.external_runtime_probe_evidence (
            runtime_id TEXT PRIMARY KEY REFERENCES {schema}.external_runtime_registrations(runtime_id),
            observed_cli_version TEXT NOT NULL,
            consumed_contract_revision TEXT NOT NULL,
            probe_suite_revision TEXT NOT NULL,
            record_json TEXT NOT NULL
         );"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL external runtime compatibility certifications",
            error,
        )
    })
}

fn apply_postgres_external_runtime_compatibility_probe(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.external_runtime_registrations
         SET observed_state = '\"disconnected\"',
             record_json = (
                 record_json::jsonb || jsonb_build_object(
                     'observedCliVersion', NULL,
                     'consumedContractRevision', NULL,
                     'compatibilityState', 'unassessed',
                     'lastCompatibilityProbe', NULL,
                     'observedState', 'disconnected',
                     'observedReasonCode', NULL
                 )
             )::text;"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL external runtime compatibility probe diagnostics",
            error,
        )
    })
}

fn apply_postgres_external_runtime_compatibility_state(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "UPDATE {schema}.external_runtime_registrations
         SET observed_state = '\"disconnected\"',
             record_json = ((
                 record_json::jsonb
                 - 'expectedCliVersion'
                 - 'executableSha256'
                 - 'protocolSchemaSha256'
             ) || jsonb_build_object(
                 'observedCliVersion', NULL,
                 'consumedContractRevision', NULL,
                 'compatibilityState', 'unassessed',
                 'observedState', 'disconnected',
                 'observedReasonCode', NULL
             ))::text;"
    ))
    .map_err(|error| {
        postgres_error(
            "replace PostgreSQL external runtime pins with compatibility state",
            error,
        )
    })
}

fn apply_postgres_roleplay_mechanic_sessions_and_diagnostics(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_mechanic_sessions (
            mechanic_session_id TEXT PRIMARY KEY,
            mechanic_profile_id TEXT NOT NULL,
            roleplay_session_id TEXT,
            roleplay_profile_id TEXT,
            revision BIGINT NOT NULL,
            record_json JSONB NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_sessions_profile_idx
            ON {schema}.module_roleplay_mechanic_sessions(mechanic_profile_id, updated_at DESC, mechanic_session_id);
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_sessions_roleplay_idx
            ON {schema}.module_roleplay_mechanic_sessions(roleplay_session_id, updated_at DESC, mechanic_session_id);

         CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_mechanic_diagnostics (
            diagnostic_id TEXT PRIMARY KEY,
            mechanic_session_id TEXT NOT NULL,
            mechanic_profile_id TEXT NOT NULL,
            roleplay_session_id TEXT NOT NULL,
            roleplay_profile_id TEXT NOT NULL,
            outcome TEXT NOT NULL,
            revision BIGINT NOT NULL,
            record_json JSONB NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_diagnostics_mechanic_idx
            ON {schema}.module_roleplay_mechanic_diagnostics(mechanic_session_id, updated_at DESC, diagnostic_id);
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_diagnostics_roleplay_outcome_idx
            ON {schema}.module_roleplay_mechanic_diagnostics(roleplay_session_id, outcome, updated_at DESC, diagnostic_id);
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_diagnostics_profile_idx
            ON {schema}.module_roleplay_mechanic_diagnostics(roleplay_profile_id, updated_at DESC, diagnostic_id);

         CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_mechanic_diagnostic_proposals (
            diagnostic_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_mechanic_diagnostics(diagnostic_id) ON DELETE CASCADE,
            proposal_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_mechanic_proposals(proposal_id) ON DELETE RESTRICT,
            applied BOOLEAN NOT NULL,
            PRIMARY KEY(diagnostic_id, proposal_id)
         );
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_diagnostic_proposals_proposal_idx
            ON {schema}.module_roleplay_mechanic_diagnostic_proposals(proposal_id, diagnostic_id);"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL roleplay mechanic session associations and diagnostics",
            error,
        )
    })
}

fn apply_postgres_roleplay_mechanic_proposals(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_mechanic_proposals (
            proposal_id TEXT PRIMARY KEY,
            mechanic_session_id TEXT NOT NULL,
            roleplay_session_id TEXT NOT NULL,
            profile_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            target_id TEXT,
            target_revision BIGINT,
            revision BIGINT NOT NULL,
            record_json JSONB NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_proposals_session_status_idx
            ON {schema}.module_roleplay_mechanic_proposals(roleplay_session_id, status, updated_at DESC, proposal_id);
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_proposals_mechanic_idx
            ON {schema}.module_roleplay_mechanic_proposals(mechanic_session_id, updated_at DESC, proposal_id);
         CREATE INDEX IF NOT EXISTS roleplay_mechanic_proposals_profile_kind_idx
            ON {schema}.module_roleplay_mechanic_proposals(profile_id, kind, updated_at DESC, proposal_id);"
    ))
    .map_err(|error| postgres_error("add PostgreSQL roleplay mechanic proposals", error))
}

fn apply_postgres_roleplay_lore_recall_entry_decisions(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.module_roleplay_lore_recall_traces
            ADD COLUMN IF NOT EXISTS entry_decisions JSONB NOT NULL DEFAULT '[]'::jsonb;"
    ))
    .map_err(|error| {
        postgres_error(
            "apply PostgreSQL roleplay lore recall entry decisions migration",
            error,
        )
    })
}

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

fn apply_postgres_external_runtime(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.external_runtime_registrations (
            runtime_id TEXT PRIMARY KEY, observed_state TEXT NOT NULL,
            revision BIGINT NOT NULL, record_json TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS {schema}.external_controller_leases (
            runtime_id TEXT PRIMARY KEY REFERENCES {schema}.external_runtime_registrations(runtime_id),
            holder_instance_id TEXT NOT NULL, generation BIGINT NOT NULL,
            expires_at TEXT NOT NULL, revision BIGINT NOT NULL, record_json TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS external_controller_leases_expiry_idx
            ON {schema}.external_controller_leases(expires_at, runtime_id);
         CREATE TABLE IF NOT EXISTS {schema}.external_agent_bindings (
            binding_id TEXT PRIMARY KEY,
            runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
            session_id TEXT REFERENCES {schema}.sessions(session_id), agent_id TEXT,
            purpose TEXT NOT NULL, status TEXT NOT NULL, native_thread_id TEXT,
            revision BIGINT NOT NULL, record_json TEXT NOT NULL);
         CREATE UNIQUE INDEX IF NOT EXISTS external_agent_bindings_active_agent_idx
            ON {schema}.external_agent_bindings(agent_id)
            WHERE purpose = 'crew_agent' AND status = 'active';
         CREATE UNIQUE INDEX IF NOT EXISTS external_agent_bindings_runtime_thread_idx
            ON {schema}.external_agent_bindings(runtime_id, native_thread_id)
            WHERE native_thread_id IS NOT NULL;
         CREATE INDEX IF NOT EXISTS external_agent_bindings_session_idx
            ON {schema}.external_agent_bindings(session_id, status);
         CREATE SEQUENCE IF NOT EXISTS {schema}.external_turn_creation_ordinal_seq;
         CREATE TABLE IF NOT EXISTS {schema}.external_turns (
            request_id TEXT PRIMARY KEY, idempotency_key TEXT NOT NULL UNIQUE,
            runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
            binding_id TEXT NOT NULL REFERENCES {schema}.external_agent_bindings(binding_id),
            session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            native_thread_id TEXT NOT NULL, native_turn_id TEXT, phase TEXT NOT NULL,
            revision BIGINT NOT NULL, created_at TEXT NOT NULL,
            creation_ordinal BIGINT NOT NULL DEFAULT nextval('{schema}.external_turn_creation_ordinal_seq'),
            updated_at TEXT NOT NULL, record_json TEXT NOT NULL);
         CREATE UNIQUE INDEX IF NOT EXISTS external_turns_native_turn_idx
            ON {schema}.external_turns(runtime_id, native_turn_id)
            WHERE native_turn_id IS NOT NULL;
         CREATE INDEX IF NOT EXISTS external_turns_native_thread_idx
            ON {schema}.external_turns(runtime_id, native_thread_id, updated_at);
         CREATE INDEX IF NOT EXISTS external_turns_creation_cursor_idx
            ON {schema}.external_turns(runtime_id, native_thread_id, created_at, request_id);
         CREATE UNIQUE INDEX IF NOT EXISTS external_turns_creation_ordinal_idx
            ON {schema}.external_turns(creation_ordinal);
         CREATE INDEX IF NOT EXISTS external_turns_thread_creation_ordinal_idx
            ON {schema}.external_turns(runtime_id, native_thread_id, creation_ordinal);
         CREATE INDEX IF NOT EXISTS external_turns_active_session_idx
            ON {schema}.external_turns(session_id, phase, updated_at);
         CREATE TABLE IF NOT EXISTS {schema}.external_control_receipts (
            control_id TEXT PRIMARY KEY, idempotency_key TEXT NOT NULL UNIQUE,
            binding_id TEXT NOT NULL REFERENCES {schema}.external_agent_bindings(binding_id),
            request_fingerprint TEXT NOT NULL, status TEXT NOT NULL,
            revision BIGINT NOT NULL, record_json TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS {schema}.external_interactions (
            interaction_id TEXT PRIMARY KEY,
            runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
            binding_id TEXT NOT NULL REFERENCES {schema}.external_agent_bindings(binding_id),
            request_id TEXT NOT NULL REFERENCES {schema}.external_turns(request_id),
            native_request_id TEXT NOT NULL, status TEXT NOT NULL, expires_at TEXT NOT NULL,
            revision BIGINT NOT NULL, record_json TEXT NOT NULL,
            UNIQUE(runtime_id, native_request_id));
         CREATE INDEX IF NOT EXISTS external_interactions_pending_idx
            ON {schema}.external_interactions(status, expires_at);
         CREATE TABLE IF NOT EXISTS {schema}.external_runtime_events (
            event_id TEXT PRIMARY KEY,
            runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
            session_id TEXT REFERENCES {schema}.sessions(session_id),
            sequence_id BIGINT NOT NULL, kind TEXT NOT NULL, created_at TEXT NOT NULL,
            record_json TEXT NOT NULL, UNIQUE(runtime_id, sequence_id));
         CREATE INDEX IF NOT EXISTS external_runtime_events_session_cursor_idx
            ON {schema}.external_runtime_events(session_id, sequence_id);
         CREATE TABLE IF NOT EXISTS {schema}.external_correlated_rounds (
            round_id TEXT PRIMARY KEY, idempotency_key TEXT NOT NULL UNIQUE,
            sender_agent_id TEXT NOT NULL,
            sender_session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            recipient_agent_id TEXT NOT NULL,
            recipient_session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            status TEXT NOT NULL, expires_at TEXT NOT NULL,
            revision BIGINT NOT NULL, record_json TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS external_correlated_rounds_pending_idx
            ON {schema}.external_correlated_rounds(status, expires_at, recipient_agent_id);"
    ))
    .map_err(|error| postgres_error("apply PostgreSQL external runtime migration", error))
}

fn apply_postgres_agent_coordination(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "DROP TABLE IF EXISTS {schema}.external_correlated_rounds;
         DROP TABLE IF EXISTS {schema}.agent_correlated_rounds;
         CREATE TABLE IF NOT EXISTS {schema}.agent_message_delivery_receipts (
            delivery_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            message_id TEXT NOT NULL UNIQUE,
            from_agent_id TEXT NOT NULL,
            to_agent_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision BIGINT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS agent_message_delivery_status_expiry_idx
            ON {schema}.agent_message_delivery_receipts(status, expires_at, to_agent_id);
         CREATE TABLE IF NOT EXISTS {schema}.agent_correlated_rounds (
            round_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            sender_agent_id TEXT NOT NULL,
            sender_session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            recipient_agent_id TEXT NOT NULL,
            recipient_session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            correlation_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision BIGINT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE UNIQUE INDEX IF NOT EXISTS agent_correlated_rounds_pending_correlation_idx
            ON {schema}.agent_correlated_rounds(sender_agent_id, recipient_agent_id, correlation_id)
            WHERE status = 'pending';
         CREATE INDEX IF NOT EXISTS agent_correlated_rounds_pending_idx
            ON {schema}.agent_correlated_rounds(status, expires_at, recipient_agent_id);"
    ))
    .map_err(|error| postgres_error("apply PostgreSQL agent coordination migration", error))
}

fn apply_postgres_operator_agent_rounds(tx: &mut Transaction<'_>, schema: &str) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.agent_correlated_rounds
            ALTER COLUMN sender_session_id DROP NOT NULL;"
    ))
    .map_err(|error| postgres_error("apply PostgreSQL operator agent rounds migration", error))
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

fn apply_postgres_chat_message_ingest_receipts(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "ALTER TABLE {schema}.message_slots
            ADD COLUMN IF NOT EXISTS ingest_idempotency_key TEXT;
         CREATE UNIQUE INDEX IF NOT EXISTS message_slots_session_ingest_key_idx
            ON {schema}.message_slots(session_id, ingest_idempotency_key)
            WHERE ingest_idempotency_key IS NOT NULL;
         CREATE TABLE IF NOT EXISTS {schema}.chat_message_ingest_receipts (
            session_id TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            slot_id TEXT NOT NULL,
            branch_id TEXT NOT NULL,
            state TEXT NOT NULL CHECK (state IN ('reserved', 'finalized')),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            PRIMARY KEY(session_id, idempotency_key)
         );
         CREATE INDEX IF NOT EXISTS chat_message_ingest_receipts_expiry_idx
            ON {schema}.chat_message_ingest_receipts(state, expires_at);
         CREATE INDEX IF NOT EXISTS chat_message_ingest_receipts_slot_idx
            ON {schema}.chat_message_ingest_receipts(slot_id);"
    ))
    .map_err(|error| postgres_error("add PostgreSQL chat message ingest receipts", error))
}

fn apply_postgres_external_agent_session_creations(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.external_agent_session_creations (
            creation_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            request_fingerprint TEXT NOT NULL,
            runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
            profile_id TEXT NOT NULL REFERENCES {schema}.profile_registry(profile_id),
            session_id TEXT NOT NULL REFERENCES {schema}.sessions(session_id),
            binding_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            native_thread_id TEXT,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS external_agent_session_creations_phase_idx
            ON {schema}.external_agent_session_creations(phase, updated_at, creation_id);"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL external agent session creation records",
            error,
        )
    })
}

fn apply_postgres_context_compaction_intent_unique(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    // Deduplicate legacy duplicates before adding the unique constraint.
    tx.batch_execute(&format!(
        "DELETE FROM {schema}.context_compaction_artifacts
            WHERE (record_json::jsonb->>'intent_key') IS NOT NULL
              AND ctid NOT IN (
                SELECT MAX(ctid)
                FROM {schema}.context_compaction_artifacts
                WHERE (record_json::jsonb->>'intent_key') IS NOT NULL
                GROUP BY session_id, (record_json::jsonb->>'intent_key')
              );
         CREATE UNIQUE INDEX IF NOT EXISTS context_compaction_session_intent_idx
            ON {schema}.context_compaction_artifacts(session_id, (record_json::jsonb->>'intent_key'))
            WHERE record_json::jsonb->>'intent_key' IS NOT NULL;"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL context compaction intent unique index",
            error,
        )
    })
}

fn apply_postgres_context_compaction_intent_projection_aware(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "DROP INDEX IF EXISTS {schema}.context_compaction_session_intent_idx;
         DELETE FROM {schema}.context_compaction_artifacts
            WHERE (record_json::jsonb->>'intent_key') IS NOT NULL
              AND ctid NOT IN (
                SELECT MAX(ctid)
                FROM {schema}.context_compaction_artifacts
                WHERE (record_json::jsonb->>'intent_key') IS NOT NULL
                GROUP BY session_id, (record_json::jsonb->>'intent_key'), COALESCE(record_json::jsonb->>'source_projection_fingerprint','')
              );
         CREATE UNIQUE INDEX IF NOT EXISTS context_compaction_session_intent_projection_idx
            ON {schema}.context_compaction_artifacts(session_id, (record_json::jsonb->>'intent_key'), COALESCE(record_json::jsonb->>'source_projection_fingerprint',''))
            WHERE record_json::jsonb->>'intent_key' IS NOT NULL;"
    ))
    .map_err(|error| {
        postgres_error(
            "add PostgreSQL context compaction projection-aware index",
            error,
        )
    })
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
    use postgres::NoTls;

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

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_fresh_schema_has_external_runtime_thread_cursor_index() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_thread_cursor_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut client = postgres::Client::connect(&database_url, NoTls).unwrap();
        prepare_postgres_migration_metadata(&mut client, &schema).unwrap();
        apply_postgres_schema_migrations(&mut client, &schema).unwrap();

        let row = client
            .query_one(
                "SELECT indexdef FROM pg_indexes
                  WHERE schemaname = $1
                    AND indexname = 'external_runtime_events_thread_cursor_idx'",
                &[&schema],
            )
            .unwrap();
        let indexdef = row.get::<_, String>(0);
        assert!(indexdef.contains("runtime_id, native_thread_id, sequence_id"));
        assert!(indexdef.contains("native_thread_id IS NOT NULL"));
        assert_eq!(
            current_postgres_schema_version(&mut client, &schema).unwrap(),
            POSTGRES_SCHEMA_VERSION
        );
        client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_version_58_repairs_recorded_ordinal_schema_drift_with_quoted_schema() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_ordinal_repair_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let quoted_schema = quote_postgres_identifier(&schema);
        prepare_postgres_migration_metadata(&mut client, &quoted_schema).unwrap();
        apply_postgres_schema_migrations(&mut client, &quoted_schema).unwrap();
        client
            .batch_execute(&format!(
                "DELETE FROM {quoted_schema}.schema_migrations WHERE version >= 58;
                 ALTER TABLE {schema}.external_turns DROP COLUMN creation_ordinal;
                 ALTER TABLE {schema}.external_turns DROP COLUMN created_at;
                 DROP SEQUENCE IF EXISTS {schema}.external_turn_creation_ordinal_seq;"
            ))
            .unwrap();

        apply_postgres_schema_migrations(&mut client, &quoted_schema).unwrap();

        assert_eq!(
            current_postgres_schema_version(&mut client, &quoted_schema).unwrap(),
            POSTGRES_SCHEMA_VERSION
        );
        assert!(client
            .query_opt(
                "SELECT 1 FROM information_schema.columns
                  WHERE table_schema = $1
                    AND table_name = 'external_turns'
                    AND column_name = 'creation_ordinal'",
                &[&schema],
            )
            .unwrap()
            .is_some());
        assert!(client
            .query_opt(
                "SELECT 1 FROM information_schema.columns
                  WHERE table_schema = $1
                    AND table_name = 'external_turns'
                    AND column_name = 'created_at'",
                &[&schema],
            )
            .unwrap()
            .is_some());
        client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_version_50_migrates_legacy_delegated_workspace_constraints() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_delegated_workspace_migration_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        prepare_postgres_migration_metadata(&mut client, &schema).unwrap();
        let mut tx = client.transaction().unwrap();
        apply_postgres_baseline_schema(&mut tx, &schema).unwrap();
        for migration in &POSTGRES_SCHEMA_MIGRATIONS[..50] {
            insert_postgres_schema_migration(&mut tx, &schema, migration).unwrap();
        }
        tx.commit().unwrap();

        let delegated_state = |session_id: &str,
                               constraint: Option<&str>|
         -> rusty_crew_core_protocol::SessionState {
            rusty_crew_core_protocol::SessionState {
                handle: rusty_crew_core_protocol::SessionHandle::new(1),
                session_id: rusty_crew_core_protocol::SessionId::new(session_id),
                agent_id: rusty_crew_core_protocol::AgentId::new("delegated-agent"),
                profile_id: rusty_crew_core_protocol::ProfileId::new("delegated-profile"),
                kind: rusty_crew_core_protocol::SessionKind::Delegated,
                delegation: Some(rusty_crew_core_protocol::DelegationLineage {
                    parent_session_id: rusty_crew_core_protocol::SessionId::new("parent-session"),
                    parent_agent_id: rusty_crew_core_protocol::AgentId::new("parent-agent"),
                    source_wake_id: "wake-1".to_string(),
                    source_action_index: 0,
                    requested_task_id: None,
                    correlation_id: format!("delegation:{session_id}"),
                    workspace_constraint: constraint.map(|cwd| {
                        rusty_crew_core_protocol::DelegatedWorkspaceConstraint {
                            cwd: cwd.to_string(),
                        }
                    }),
                }),
                workspace: None,
                resource_limits: rusty_crew_core_protocol::ResourceLimits {
                    max_duration_ms: None,
                    max_delegation_depth: None,
                },
                tool_profile: rusty_crew_core_protocol::ToolProfile { tools: Vec::new() },
                history_window: None,
                inference_overrides: Default::default(),
                status: rusty_crew_core_protocol::SessionStatus::Idle,
                brain_turn_count: 0,
                created_at: "2026-08-08T00:00:00Z".to_string(),
                last_active_at: "2026-08-08T00:00:00Z".to_string(),
            }
        };

        for (session_id, existing_constraint) in [
            ("legacy-delegated", None),
            ("typed-delegated", Some("/typed")),
        ] {
            let state = delegated_state(session_id, existing_constraint);
            let mut state_json = serde_json::to_value(&state).unwrap();
            state_json["resource_limits"]["workdir"] = serde_json::json!("/legacy");
            let mut config_json = serde_json::to_value(rusty_crew_core_protocol::SessionConfig {
                session_id: state.session_id.clone(),
                agent_id: state.agent_id.clone(),
                profile_id: state.profile_id.clone(),
                kind: state.kind.clone(),
                delegation: state.delegation.clone(),
                workspace: state.workspace.clone(),
                resource_limits: state.resource_limits.clone(),
                tool_profile: state.tool_profile.clone(),
                history_window: state.history_window.clone(),
            })
            .unwrap();
            config_json["resource_limits"]["workdir"] = serde_json::json!("/legacy");
            client
                .execute(
                    &format!(
                        "INSERT INTO {schema}.sessions(
                            session_id, handle, agent_id, profile_id, kind, status,
                            state_json, created_at, last_active_at
                         ) VALUES ($1, 1, 'delegated-agent', 'delegated-profile', 'delegated',
                             'idle', $2, '2026-08-08T00:00:00Z', '2026-08-08T00:00:00Z')"
                    ),
                    &[&session_id, &serde_json::to_string(&state_json).unwrap()],
                )
                .unwrap();
            client
                .execute(
                    &format!(
                        "INSERT INTO {schema}.session_configs(
                            session_id, profile_id, kind, record_json, created_at
                         ) VALUES ($1, 'delegated-profile', 'delegated', $2,
                             '2026-08-08T00:00:00Z')"
                    ),
                    &[&session_id, &serde_json::to_string(&config_json).unwrap()],
                )
                .unwrap();
        }

        apply_postgres_schema_migrations(&mut client, &schema).unwrap();
        apply_postgres_schema_migrations(&mut client, &schema).unwrap();

        for (session_id, expected_constraint) in [
            ("legacy-delegated", "/legacy"),
            ("typed-delegated", "/typed"),
        ] {
            let row = client
                .query_one(
                    &format!(
                        "SELECT state_json, record_json
                           FROM {schema}.sessions
                           JOIN {schema}.session_configs USING(session_id)
                          WHERE session_id = $1"
                    ),
                    &[&session_id],
                )
                .unwrap();
            let state_json = row.get::<_, String>(0);
            let config_json = row.get::<_, String>(1);
            let state: rusty_crew_core_protocol::SessionState =
                serde_json::from_str(&state_json).unwrap();
            let config: rusty_crew_core_protocol::SessionConfig =
                serde_json::from_str(&config_json).unwrap();
            assert_eq!(
                state
                    .delegation
                    .as_ref()
                    .and_then(|lineage| lineage.workspace_constraint.as_ref())
                    .map(|constraint| constraint.cwd.as_str()),
                Some(expected_constraint)
            );
            assert_eq!(
                config
                    .delegation
                    .as_ref()
                    .and_then(|lineage| lineage.workspace_constraint.as_ref())
                    .map(|constraint| constraint.cwd.as_str()),
                Some(expected_constraint)
            );
            assert!(!state_json.contains("workdir"));
            assert!(!config_json.contains("workdir"));
        }

        assert_eq!(
            current_postgres_schema_version(&mut client, &schema).unwrap(),
            POSTGRES_SCHEMA_VERSION
        );
        client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_version_51_migrates_legacy_session_created_events() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_workspace_event_migration_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        prepare_postgres_migration_metadata(&mut client, &schema).unwrap();
        let mut tx = client.transaction().unwrap();
        apply_postgres_baseline_schema(&mut tx, &schema).unwrap();
        for migration in &POSTGRES_SCHEMA_MIGRATIONS[..51] {
            insert_postgres_schema_migration(&mut tx, &schema, migration).unwrap();
        }
        tx.commit().unwrap();

        let session_created =
            |session_id: &str, kind: &str, delegation: serde_json::Value, workdir: &str| {
                serde_json::json!({
                    "type": "session_created",
                    "state": {
                        "handle": 1,
                        "session_id": session_id,
                        "agent_id": "migration-agent",
                        "profile_id": "migration-profile",
                        "kind": kind,
                        "delegation": delegation,
                        "resource_limits": {
                            "workdir": workdir,
                            "max_duration_ms": null,
                            "max_delegation_depth": null
                        },
                        "tool_profile": { "tools": [] },
                        "history_window": null,
                        "inference_overrides": {},
                        "status": "idle",
                        "brain_turn_count": 0,
                        "created_at": "2026-08-08T00:00:00Z",
                        "last_active_at": "2026-08-08T00:01:00Z"
                    }
                })
            };
        let delegated_lineage = |constraint: Option<&str>| {
            let mut lineage = serde_json::json!({
                "parent_session_id": "parent-session",
                "parent_agent_id": "parent-agent",
                "source_wake_id": "wake-1",
                "source_action_index": 0,
                "requested_task_id": null,
                "correlation_id": "delegation:migration"
            });
            if let Some(cwd) = constraint {
                lineage["workspace_constraint"] = serde_json::json!({ "cwd": cwd });
            }
            lineage
        };
        let events = [
            (
                1_i64,
                session_created("legacy-full", "full", serde_json::Value::Null, "/full"),
            ),
            (
                2_i64,
                session_created(
                    "legacy-delegated",
                    "delegated",
                    delegated_lineage(None),
                    "/delegated",
                ),
            ),
            (
                3_i64,
                session_created(
                    "typed-delegated",
                    "delegated",
                    delegated_lineage(Some("/typed")),
                    "/legacy",
                ),
            ),
        ];
        for (sequence, event) in events {
            client
                .execute(
                    &format!(
                        "INSERT INTO {schema}.event_history(sequence, event_kind, event_json)
                         VALUES ($1, 'SessionCreated', $2)"
                    ),
                    &[&sequence, &serde_json::to_string(&event).unwrap()],
                )
                .unwrap();
        }

        apply_postgres_schema_migrations(&mut client, &schema).unwrap();
        apply_postgres_schema_migrations(&mut client, &schema).unwrap();

        for (sequence, expected_cwd) in [(1_i64, "/full"), (2, "/delegated"), (3, "/typed")] {
            let event_json = client
                .query_one(
                    &format!("SELECT event_json FROM {schema}.event_history WHERE sequence = $1"),
                    &[&sequence],
                )
                .unwrap()
                .get::<_, String>(0);
            assert!(!event_json.contains("workdir"));
            let event: rusty_crew_core_protocol::CoreEvent =
                serde_json::from_str(&event_json).unwrap();
            let rusty_crew_core_protocol::CoreEvent::SessionCreated { state } = event else {
                panic!("expected migrated session-created event");
            };
            let migrated_cwd = match state.kind {
                rusty_crew_core_protocol::SessionKind::Full => state
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.cwd.as_str()),
                rusty_crew_core_protocol::SessionKind::Delegated => state
                    .delegation
                    .as_ref()
                    .and_then(|lineage| lineage.workspace_constraint.as_ref())
                    .map(|constraint| constraint.cwd.as_str()),
                rusty_crew_core_protocol::SessionKind::Worker => None,
            };
            assert_eq!(migrated_cwd, Some(expected_cwd));
        }

        assert_eq!(
            current_postgres_schema_version(&mut client, &schema).unwrap(),
            POSTGRES_SCHEMA_VERSION
        );
        client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .unwrap();
    }

    #[test]
    fn external_runtime_retention_jsonb_read_normalizes_only_unescaped_nul() {
        assert_eq!(
            postgres_external_event_record_jsonb_expression(),
            r#"regexp_replace(record_json, $$((?<!\\)(?:\\\\)*)\\u0000$$, $$\1\\ufffd$$, 'g')::jsonb"#
        );
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_external_runtime_retention_migrates_escaped_nul_event_without_rewriting_it() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_event_retention_migration_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        client
            .batch_execute(&format!(
                "CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.sessions (session_id TEXT PRIMARY KEY);
                 CREATE TABLE {schema}.external_runtime_registrations (
                    runtime_id TEXT PRIMARY KEY
                 );
                 CREATE TABLE {schema}.external_runtime_events (
                    event_id TEXT PRIMARY KEY,
                    runtime_id TEXT NOT NULL REFERENCES {schema}.external_runtime_registrations(runtime_id),
                    session_id TEXT REFERENCES {schema}.sessions(session_id),
                    sequence_id BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    UNIQUE(runtime_id, sequence_id)
                 );
                 CREATE TABLE {schema}.external_turns (
                    runtime_id TEXT NOT NULL,
                    native_turn_id TEXT,
                    phase TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 INSERT INTO {schema}.sessions(session_id) VALUES ('session-1');
                 INSERT INTO {schema}.external_runtime_registrations(runtime_id) VALUES ('runtime-1');"
            ))
            .unwrap();
        let original_with_escaped_nul = r#"{"nativeThreadId":"thread-1","nativeTurnId":"turn-1","diagnostic":"before\u0000after"}"#;
        let original_with_literal_escape =
            r#"{"nativeThreadId":"thread-\\u0000-x","nativeTurnId":"turn-\\u0000-y"}"#;
        client
            .execute(
                &format!(
                    "INSERT INTO {schema}.external_runtime_events(
                        event_id, runtime_id, session_id, sequence_id, kind, created_at, record_json
                     ) VALUES
                        ('event-1', 'runtime-1', 'session-1', 1, 'item', '2026-08-02T00:00:00Z', $1),
                        ('event-2', 'runtime-1', 'session-1', 2, 'item', '2026-08-02T00:00:00Z', $2)"
                ),
                &[&original_with_escaped_nul, &original_with_literal_escape],
            )
            .unwrap();

        let mut tx = client.transaction().unwrap();
        apply_postgres_external_runtime_event_retention(&mut tx, &schema).unwrap();
        tx.commit().unwrap();

        let escaped_nul_row = client
            .query_one(
                &format!(
                    "SELECT native_thread_id, native_turn_id, record_json
                       FROM {schema}.external_runtime_events
                      WHERE event_id = 'event-1'"
                ),
                &[],
            )
            .unwrap();
        assert_eq!(
            escaped_nul_row.get::<_, Option<String>>(0).as_deref(),
            Some("thread-1")
        );
        assert_eq!(
            escaped_nul_row.get::<_, Option<String>>(1).as_deref(),
            Some("turn-1")
        );
        assert_eq!(
            escaped_nul_row.get::<_, String>(2),
            original_with_escaped_nul
        );

        let literal_escape_row = client
            .query_one(
                &format!(
                    "SELECT native_thread_id, native_turn_id, record_json
                       FROM {schema}.external_runtime_events
                      WHERE event_id = 'event-2'"
                ),
                &[],
            )
            .unwrap();
        assert_eq!(
            literal_escape_row.get::<_, Option<String>>(0).as_deref(),
            Some(r#"thread-\u0000-x"#)
        );
        assert_eq!(
            literal_escape_row.get::<_, Option<String>>(1).as_deref(),
            Some(r#"turn-\u0000-y"#)
        );
        assert_eq!(
            literal_escape_row.get::<_, String>(2),
            original_with_literal_escape
        );

        client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_service_credential_migration_imports_inline_provider_secrets_once() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_service_credential_migration_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        prepare_postgres_migration_metadata(&mut client, &schema).unwrap();
        for migration in &POSTGRES_SCHEMA_MIGRATIONS[..35] {
            let mut tx = client.transaction().unwrap();
            if let Some(apply) = migration.apply {
                apply(&mut tx, &schema).unwrap();
            }
            insert_postgres_schema_migration(&mut tx, &schema, migration).unwrap();
            tx.commit().unwrap();
        }
        let oauth_secret = ModelProviderSecretEnvelope::OpenAiOauth {
            version: rusty_crew_core_protocol::MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
            issuer: "https://auth.openai.com".to_string(),
            client_id: "app-client".to_string(),
            id_token: "id-token".to_string(),
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            exchanged_api_token: None,
            last_refresh_at: None,
            account_id: None,
            email: None,
            plan_type: None,
            is_fedramp_account: false,
            access_token_expires_at: None,
        }
        .to_storage_text()
        .unwrap();
        client
            .execute(
                &format!(
                    "INSERT INTO {schema}.model_providers (
                        alias, status, protocol, provider_json, secret_ciphertext,
                        secret_updated_at, revision, created_at, updated_at
                     ) VALUES ($1, 'active', 'responses', $2, $3, $4, 4, $4, $4)"
                ),
                &[
                    &"imported-oauth",
                    &r#"{"display_name":"Imported OAuth","provider_kind":"openai"}"#,
                    &oauth_secret,
                    &"2026-07-16T00:00:00Z",
                ],
            )
            .unwrap();

        apply_postgres_schema_migrations(&mut client, &schema).unwrap();
        let imported = client
            .query_one(
                &format!(
                    "SELECT credential_id, credential_kind, secret_ciphertext, revision
                       FROM {schema}.service_credentials
                      WHERE credential_id = 'provider:imported-oauth'"
                ),
                &[],
            )
            .unwrap();
        assert_eq!(imported.get::<_, String>(0), "provider:imported-oauth");
        assert_eq!(imported.get::<_, String>(1), "openai_oauth");
        assert_eq!(imported.get::<_, Option<String>>(2), Some(oauth_secret));
        assert_eq!(imported.get::<_, i64>(3), 1);
        let inline_secret: Option<String> = client
            .query_one(
                &format!(
                    "SELECT secret_ciphertext FROM {schema}.model_providers
                      WHERE alias = 'imported-oauth'"
                ),
                &[],
            )
            .unwrap()
            .get(0);
        assert!(inline_secret.is_none());
        client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_chat_completions_brain_identity_migration_is_complete_and_idempotent() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_chat_brain_migration_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let store = PostgresBackendStore::connect(&database_url, &schema).unwrap();
        let quoted_schema = store.quoted_schema();
        let old_record = serde_json::json!({
            "profile_id": "legacy-chat-profile",
            "lifecycle_status": "active",
            "active_runtime_settings_json": {
                "brain": {"module": "pi-agent", "strategy": "default"},
                "profile": {
                    "brain": {"module": "pi-agent", "strategy": "default"}
                }
            }
        })
        .to_string();
        let mut client = store.client().unwrap();
        client
            .execute(
                &format!(
                    "INSERT INTO {quoted_schema}.profile_registry
                        (profile_id, lifecycle_status, record_json, created_at, updated_at)
                     VALUES ($1, 'active', $2, $3, $3)"
                ),
                &[&"legacy-chat-profile", &old_record, &"2026-07-14T00:00:00Z"],
            )
            .unwrap();
        client
            .execute(
                &format!(
                    "INSERT INTO {quoted_schema}.provider_wire_states (
                        session_id, module_id, strategy_id, profile_fingerprint,
                        provider_fingerprint, payload_version, payload_json,
                        created_at, updated_at
                     ) VALUES ($1, 'pi-agent', 'default', 'profile', 'provider', 'v1', '{{}}', $2, $2)"
                ),
                &[&"legacy-chat-session", &"2026-07-14T00:00:00Z"],
            )
            .unwrap();

        for _ in 0..2 {
            let mut tx = client.transaction().unwrap();
            apply_postgres_rename_chat_completions_brain(&mut tx, &quoted_schema).unwrap();
            tx.commit().unwrap();
        }

        let row = client
            .query_one(
                &format!(
                    "SELECT record_json FROM {quoted_schema}.profile_registry WHERE profile_id = $1"
                ),
                &[&"legacy-chat-profile"],
            )
            .unwrap();
        let raw: String = row.get(0);
        let settings: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            settings["active_runtime_settings_json"]["brain"]["module"],
            "chat-completions"
        );
        assert_eq!(
            settings["active_runtime_settings_json"]["profile"]["brain"]["module"],
            "chat-completions"
        );
        let old_state_count: i64 = client
            .query_one(
                &format!(
                    "SELECT COUNT(*) FROM {quoted_schema}.provider_wire_states WHERE module_id = 'pi-agent'"
                ),
                &[],
            )
            .unwrap()
            .get(0);
        assert_eq!(old_state_count, 0);

        drop(client);
        store.drop_schema_for_test().unwrap();
    }
}
