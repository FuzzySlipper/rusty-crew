//! SQLite repository for Rust-owned external-agent runtime lifecycle state.

use super::super::*;
use rusty_crew_core_protocol::{
    validate_external_agent_binding_transition,
    validate_external_runtime_certification_invalidation,
    validate_external_runtime_certification_record, validate_external_runtime_probe_evidence,
    validate_external_runtime_registration, validate_external_turn_transition, AgentActivation,
    AgentCorrelatedRound, AgentId, AgentMessageDeliveryReceipt, AgentMessageDeliveryStatus,
    AgentRoundStatus, ExternalAgentBinding, ExternalAgentSessionCreationId,
    ExternalAgentSessionCreationRecord, ExternalBindingId, ExternalControlId,
    ExternalControlReceipt, ExternalControllerLease, ExternalInteractionRecord,
    ExternalInteractionStatus, ExternalRuntimeCertificationInvalidation,
    ExternalRuntimeCertificationRecord, ExternalRuntimeCertificationStatus,
    ExternalRuntimeEventInput, ExternalRuntimeId, ExternalRuntimeProbeEvidenceRecord,
    ExternalRuntimeRegistration, ExternalTurnCorrelation, ExternalTurnRequestId,
    NormalizedExternalRuntimeEvent,
};

pub(crate) fn migrate_v35_add_external_runtime(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_runtime_registrations (
            runtime_id TEXT PRIMARY KEY,
            observed_state TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS external_controller_leases (
            runtime_id TEXT PRIMARY KEY,
            holder_instance_id TEXT NOT NULL,
            generation INTEGER NOT NULL,
            expires_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id)
         );
         CREATE INDEX IF NOT EXISTS external_controller_leases_expiry_idx
            ON external_controller_leases(expires_at, runtime_id);
         CREATE TABLE IF NOT EXISTS external_agent_bindings (
            binding_id TEXT PRIMARY KEY,
            runtime_id TEXT NOT NULL,
            session_id TEXT,
            agent_id TEXT,
            purpose TEXT NOT NULL,
            status TEXT NOT NULL,
            native_thread_id TEXT,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id),
            FOREIGN KEY(session_id) REFERENCES sessions(session_id)
         );
         CREATE UNIQUE INDEX IF NOT EXISTS external_agent_bindings_active_agent_idx
            ON external_agent_bindings(agent_id)
            WHERE purpose = 'crew_agent' AND status = 'active';
         CREATE UNIQUE INDEX IF NOT EXISTS external_agent_bindings_runtime_thread_idx
            ON external_agent_bindings(runtime_id, native_thread_id)
            WHERE native_thread_id IS NOT NULL;
         CREATE INDEX IF NOT EXISTS external_agent_bindings_session_idx
            ON external_agent_bindings(session_id, status);
         CREATE TABLE IF NOT EXISTS external_turns (
            request_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            runtime_id TEXT NOT NULL,
            binding_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            native_thread_id TEXT NOT NULL,
            native_turn_id TEXT,
            phase TEXT NOT NULL,
            revision INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id),
            FOREIGN KEY(binding_id) REFERENCES external_agent_bindings(binding_id),
            FOREIGN KEY(session_id) REFERENCES sessions(session_id)
         );
         CREATE UNIQUE INDEX IF NOT EXISTS external_turns_native_turn_idx
            ON external_turns(runtime_id, native_turn_id)
            WHERE native_turn_id IS NOT NULL;
         CREATE INDEX IF NOT EXISTS external_turns_native_thread_idx
            ON external_turns(runtime_id, native_thread_id, updated_at);
         CREATE INDEX IF NOT EXISTS external_turns_active_session_idx
            ON external_turns(session_id, phase, updated_at);
         CREATE TABLE IF NOT EXISTS external_control_receipts (
            control_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            binding_id TEXT NOT NULL,
            request_fingerprint TEXT NOT NULL,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(binding_id) REFERENCES external_agent_bindings(binding_id)
         );
         CREATE TABLE IF NOT EXISTS external_interactions (
            interaction_id TEXT PRIMARY KEY,
            runtime_id TEXT NOT NULL,
            binding_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            native_request_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            UNIQUE(runtime_id, native_request_id),
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id),
            FOREIGN KEY(binding_id) REFERENCES external_agent_bindings(binding_id),
            FOREIGN KEY(request_id) REFERENCES external_turns(request_id)
         );
         CREATE INDEX IF NOT EXISTS external_interactions_pending_idx
            ON external_interactions(status, expires_at);
         CREATE TABLE IF NOT EXISTS external_runtime_events (
            event_id TEXT PRIMARY KEY,
            runtime_id TEXT NOT NULL,
            session_id TEXT,
            sequence_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            created_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            UNIQUE(runtime_id, sequence_id),
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id),
            FOREIGN KEY(session_id) REFERENCES sessions(session_id)
         );
         CREATE INDEX IF NOT EXISTS external_runtime_events_session_cursor_idx
            ON external_runtime_events(session_id, sequence_id);
         CREATE TABLE IF NOT EXISTS external_correlated_rounds (
            round_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            sender_agent_id TEXT NOT NULL,
            sender_session_id TEXT NOT NULL,
            recipient_agent_id TEXT NOT NULL,
            recipient_session_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(sender_session_id) REFERENCES sessions(session_id),
            FOREIGN KEY(recipient_session_id) REFERENCES sessions(session_id)
         );
         CREATE INDEX IF NOT EXISTS external_correlated_rounds_pending_idx
            ON external_correlated_rounds(status, expires_at, recipient_agent_id);",
    )
    .map_err(|error| persistence_error("apply schema migration 35", error))
}

pub(crate) fn migrate_v36_add_agent_coordination(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "DROP TABLE IF EXISTS external_correlated_rounds;
         DROP TABLE IF EXISTS agent_correlated_rounds;
         CREATE TABLE IF NOT EXISTS agent_message_delivery_receipts (
            delivery_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            message_id TEXT NOT NULL UNIQUE,
            from_agent_id TEXT NOT NULL,
            to_agent_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS agent_message_delivery_status_expiry_idx
            ON agent_message_delivery_receipts(status, expires_at, to_agent_id);
         CREATE TABLE IF NOT EXISTS agent_correlated_rounds (
            round_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            sender_agent_id TEXT NOT NULL,
            sender_session_id TEXT NOT NULL,
            recipient_agent_id TEXT NOT NULL,
            recipient_session_id TEXT NOT NULL,
            correlation_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(sender_session_id) REFERENCES sessions(session_id),
            FOREIGN KEY(recipient_session_id) REFERENCES sessions(session_id)
         );
         CREATE UNIQUE INDEX IF NOT EXISTS agent_correlated_rounds_pending_correlation_idx
            ON agent_correlated_rounds(sender_agent_id, recipient_agent_id, correlation_id)
            WHERE status = 'pending';
         CREATE INDEX IF NOT EXISTS agent_correlated_rounds_pending_idx
            ON agent_correlated_rounds(status, expires_at, recipient_agent_id);",
    )
    .map_err(|error| persistence_error("apply schema migration 36", error))
}

pub(crate) fn migrate_v58_add_external_runtime_event_retention(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "ALTER TABLE external_runtime_events ADD COLUMN native_thread_id TEXT;
         ALTER TABLE external_runtime_events ADD COLUMN native_turn_id TEXT;
         UPDATE external_runtime_events
            SET native_thread_id = json_extract(record_json, '$.nativeThreadId'),
                native_turn_id = json_extract(record_json, '$.nativeTurnId');
         CREATE INDEX external_runtime_events_turn_cursor_idx
            ON external_runtime_events(runtime_id, native_turn_id, sequence_id)
            WHERE native_turn_id IS NOT NULL;
         CREATE INDEX external_runtime_events_created_cursor_idx
            ON external_runtime_events(runtime_id, created_at, sequence_id);
         CREATE INDEX external_turns_terminal_retention_idx
            ON external_turns(phase, updated_at, runtime_id, native_turn_id)
            WHERE native_turn_id IS NOT NULL;
         CREATE TABLE external_runtime_event_cursors (
            runtime_id TEXT PRIMARY KEY,
            next_sequence_id INTEGER NOT NULL,
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id)
         );
         INSERT INTO external_runtime_event_cursors(runtime_id, next_sequence_id)
         SELECT registration.runtime_id, COALESCE(MAX(event.sequence_id), 0) + 1
           FROM external_runtime_registrations registration
           LEFT JOIN external_runtime_events event ON event.runtime_id = registration.runtime_id
          GROUP BY registration.runtime_id;
         CREATE TABLE external_runtime_event_checkpoints (
            runtime_id TEXT NOT NULL,
            native_turn_id TEXT NOT NULL,
            native_thread_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            terminal_phase TEXT NOT NULL,
            terminal_at TEXT NOT NULL,
            first_sequence_id INTEGER NOT NULL,
            last_sequence_id INTEGER NOT NULL,
            compacted_event_count INTEGER NOT NULL,
            estimated_compacted_bytes INTEGER NOT NULL,
            kind_counts_json TEXT NOT NULL,
            checkpointed_at TEXT NOT NULL,
            policy_cutoff TEXT NOT NULL,
            PRIMARY KEY(runtime_id, native_turn_id),
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id),
            FOREIGN KEY(session_id) REFERENCES sessions(session_id)
         );
         CREATE INDEX external_runtime_event_checkpoints_time_idx
            ON external_runtime_event_checkpoints(checkpointed_at, runtime_id, native_turn_id);",
    )
    .map_err(|error| persistence_error("apply schema migration 58", error))
}

pub(crate) fn migrate_v71_add_external_runtime_thread_cursor(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS external_runtime_events_thread_cursor_idx
            ON external_runtime_events(runtime_id, native_thread_id, sequence_id)
            WHERE native_thread_id IS NOT NULL;",
    )
    .map_err(|error| persistence_error("apply schema migration 71", error))
}

pub(crate) fn migrate_v73_add_external_turn_creation_cursor(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "ALTER TABLE external_turns ADD COLUMN created_at TEXT;
         UPDATE external_turns
            SET created_at = json_extract(record_json, '$.request.createdAt')
          WHERE created_at IS NULL;
         CREATE INDEX external_turns_creation_cursor_idx
            ON external_turns(runtime_id, native_thread_id, created_at, request_id);",
    )
    .map_err(|error| persistence_error("add external turn creation cursor", error))
}

pub(crate) fn migrate_v74_add_external_turn_creation_ordinal(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "ALTER TABLE external_turns ADD COLUMN creation_ordinal INTEGER;
         WITH ranked AS (
            SELECT request_id,
                   ROW_NUMBER() OVER (ORDER BY created_at, request_id) AS ordinal
              FROM external_turns
         )
         UPDATE external_turns
            SET creation_ordinal = (
                SELECT ordinal FROM ranked WHERE ranked.request_id = external_turns.request_id
            );
         CREATE UNIQUE INDEX external_turns_creation_ordinal_idx
            ON external_turns(creation_ordinal);
         CREATE INDEX external_turns_thread_creation_ordinal_idx
            ON external_turns(runtime_id, native_thread_id, creation_ordinal);",
    )
    .map_err(|error| persistence_error("add external turn creation ordinal", error))
}

pub(crate) fn compact_terminal_external_runtime_events_in_tx(
    tx: &rusqlite::Transaction<'_>,
    cutoff: &IsoTimestamp,
    checkpointed_at: &IsoTimestamp,
    terminal_turn_batch_size: u32,
) -> CoreResult<ExternalRuntimeEventRetentionReport> {
    if terminal_turn_batch_size == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external runtime event terminal turn batch size must be greater than zero",
        ));
    }
    let mut candidates_statement = tx
        .prepare(
            "SELECT turn.runtime_id, turn.native_thread_id, turn.native_turn_id,
                    turn.session_id, turn.phase, turn.updated_at
               FROM external_turns turn
              WHERE turn.native_turn_id IS NOT NULL
                AND turn.phase IN ('completed', 'failed', 'interrupted', 'outcome_unknown')
                AND turn.updated_at < ?1
                AND EXISTS (
                    SELECT 1 FROM external_runtime_events event
                     WHERE event.runtime_id = turn.runtime_id
                       AND event.native_turn_id = turn.native_turn_id
                       AND event.kind IN (
                           'assistant_text_delta', 'reasoning_delta', 'plan_delta',
                           'item_lifecycle', 'command_activity', 'file_activity',
                           'mcp_activity', 'dynamic_tool_activity'
                       )
                )
              ORDER BY turn.updated_at, turn.runtime_id, turn.native_turn_id
              LIMIT ?2",
        )
        .map_err(|error| persistence_error("prepare terminal external turn retention", error))?;
    let candidates = candidates_statement
        .query_map(params![cutoff, terminal_turn_batch_size as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| persistence_error("query terminal external turn retention", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("collect terminal external turn retention", error))?;
    drop(candidates_statement);

    let mut report = ExternalRuntimeEventRetentionReport {
        enabled: true,
        cutoff: Some(cutoff.clone()),
        terminal_turn_batch_size: Some(terminal_turn_batch_size),
        terminal_turns_inspected: candidates.len() as u64,
        ..ExternalRuntimeEventRetentionReport::default()
    };
    for (runtime_id, native_thread_id, native_turn_id, session_id, phase, terminal_at) in candidates
    {
        let mut kind_counts = BTreeMap::<String, u64>::new();
        let mut first_sequence = None::<u64>;
        let mut last_sequence = None::<u64>;
        let mut event_count = 0_u64;
        let mut estimated_bytes = 0_u64;
        let mut aggregate_statement = tx
            .prepare(
                "SELECT kind, COUNT(*), MIN(sequence_id), MAX(sequence_id),
                        COALESCE(SUM(LENGTH(record_json)), 0)
                   FROM external_runtime_events
                  WHERE runtime_id = ?1 AND native_turn_id = ?2
                    AND kind IN (
                        'assistant_text_delta', 'reasoning_delta', 'plan_delta',
                        'item_lifecycle', 'command_activity', 'file_activity',
                        'mcp_activity', 'dynamic_tool_activity'
                    )
                  GROUP BY kind",
            )
            .map_err(|error| {
                persistence_error("prepare external event compaction summary", error)
            })?;
        let aggregates = aggregate_statement
            .query_map(params![runtime_id, native_turn_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                ))
            })
            .map_err(|error| persistence_error("query external event compaction summary", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                persistence_error("collect external event compaction summary", error)
            })?;
        drop(aggregate_statement);
        for (kind, count, first, last, bytes) in aggregates {
            kind_counts.insert(kind, count);
            event_count += count;
            estimated_bytes += bytes;
            first_sequence = Some(first_sequence.map_or(first, |current| current.min(first)));
            last_sequence = Some(last_sequence.map_or(last, |current| current.max(last)));
        }
        let (Some(first_sequence), Some(last_sequence)) = (first_sequence, last_sequence) else {
            continue;
        };
        let deleted_estimated_bytes = estimated_bytes;
        let existing = tx
            .query_row(
                "SELECT kind_counts_json, compacted_event_count, estimated_compacted_bytes,
                        first_sequence_id, last_sequence_id
                   FROM external_runtime_event_checkpoints
                  WHERE runtime_id = ?1 AND native_turn_id = ?2",
                params![runtime_id, native_turn_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? as u64,
                        row.get::<_, i64>(2)? as u64,
                        row.get::<_, i64>(3)? as u64,
                        row.get::<_, i64>(4)? as u64,
                    ))
                },
            )
            .optional()
            .map_err(|error| persistence_error("load external event checkpoint", error))?;
        let checkpoint_created = existing.is_none();
        let (first_sequence, last_sequence, event_count, estimated_bytes) = if let Some((
            existing_counts,
            existing_events,
            existing_bytes,
            existing_first,
            existing_last,
        )) = existing
        {
            let existing_counts: BTreeMap<String, u64> = from_json_text(&existing_counts)
                .map_err(|error| persistence_error("parse external event checkpoint", error))?;
            for (kind, count) in existing_counts {
                *kind_counts.entry(kind).or_default() += count;
            }
            (
                first_sequence.min(existing_first),
                last_sequence.max(existing_last),
                event_count + existing_events,
                estimated_bytes + existing_bytes,
            )
        } else {
            (first_sequence, last_sequence, event_count, estimated_bytes)
        };
        tx.execute(
            "INSERT INTO external_runtime_event_checkpoints (
                runtime_id, native_turn_id, native_thread_id, session_id, terminal_phase,
                terminal_at, first_sequence_id, last_sequence_id, compacted_event_count,
                estimated_compacted_bytes, kind_counts_json, checkpointed_at, policy_cutoff
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(runtime_id, native_turn_id) DO UPDATE SET
                native_thread_id = excluded.native_thread_id,
                session_id = excluded.session_id,
                terminal_phase = excluded.terminal_phase,
                terminal_at = excluded.terminal_at,
                first_sequence_id = excluded.first_sequence_id,
                last_sequence_id = excluded.last_sequence_id,
                compacted_event_count = excluded.compacted_event_count,
                estimated_compacted_bytes = excluded.estimated_compacted_bytes,
                kind_counts_json = excluded.kind_counts_json,
                checkpointed_at = excluded.checkpointed_at,
                policy_cutoff = excluded.policy_cutoff",
            params![
                runtime_id,
                native_turn_id,
                native_thread_id,
                session_id,
                phase,
                terminal_at,
                first_sequence as i64,
                last_sequence as i64,
                event_count as i64,
                estimated_bytes as i64,
                to_json_text(&kind_counts)?,
                checkpointed_at,
                cutoff,
            ],
        )
        .map_err(|error| persistence_error("save external event checkpoint", error))?;
        let deleted = tx
            .execute(
                "DELETE FROM external_runtime_events
                  WHERE runtime_id = ?1 AND native_turn_id = ?2
                    AND kind IN (
                        'assistant_text_delta', 'reasoning_delta', 'plan_delta',
                        'item_lifecycle', 'command_activity', 'file_activity',
                        'mcp_activity', 'dynamic_tool_activity'
                    )",
                params![runtime_id, native_turn_id],
            )
            .map_err(|error| persistence_error("compact external runtime events", error))?;
        report.terminal_turns_compacted += 1;
        if checkpoint_created {
            report.checkpoints_created += 1;
        }
        report.events_deleted += deleted as u64;
        report.estimated_reclaimed_bytes += deleted_estimated_bytes;
    }
    let oldest = tx
        .query_row(
            "SELECT sequence_id, created_at FROM external_runtime_events
             ORDER BY created_at, sequence_id LIMIT 1",
            [],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| persistence_error("load oldest retained external event", error))?;
    if let Some((sequence, created_at)) = oldest {
        report.oldest_retained_sequence = Some(sequence);
        report.oldest_retained_at = Some(created_at);
    }
    Ok(report)
}

pub(crate) fn migrate_v38_add_external_agent_session_creations(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_agent_session_creations (
            creation_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            request_fingerprint TEXT NOT NULL,
            runtime_id TEXT NOT NULL,
            profile_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            binding_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            native_thread_id TEXT,
            revision INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id),
            FOREIGN KEY(profile_id) REFERENCES profile_registry(profile_id),
            FOREIGN KEY(session_id) REFERENCES sessions(session_id)
         );
         CREATE INDEX IF NOT EXISTS external_agent_session_creations_phase_idx
            ON external_agent_session_creations(phase, updated_at, creation_id);",
    )
    .map_err(|error| persistence_error("apply schema migration 38", error))
}

pub(crate) fn migrate_v39_allow_operator_agent_rounds(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE agent_correlated_rounds_v39 (
            round_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            sender_agent_id TEXT NOT NULL,
            sender_session_id TEXT,
            recipient_agent_id TEXT NOT NULL,
            recipient_session_id TEXT NOT NULL,
            correlation_id TEXT NOT NULL,
            status TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(sender_session_id) REFERENCES sessions(session_id),
            FOREIGN KEY(recipient_session_id) REFERENCES sessions(session_id)
         );
         INSERT INTO agent_correlated_rounds_v39
            SELECT * FROM agent_correlated_rounds;
         DROP TABLE agent_correlated_rounds;
         ALTER TABLE agent_correlated_rounds_v39 RENAME TO agent_correlated_rounds;
         CREATE UNIQUE INDEX agent_correlated_rounds_pending_correlation_idx
            ON agent_correlated_rounds(sender_agent_id, recipient_agent_id, correlation_id)
            WHERE status = 'pending';
         CREATE INDEX agent_correlated_rounds_pending_idx
            ON agent_correlated_rounds(status, expires_at, recipient_agent_id);",
    )
    .map_err(|error| persistence_error("apply schema migration 39", error))
}

pub(crate) fn migrate_v43_external_runtime_compatibility_state(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "UPDATE external_runtime_registrations
         SET observed_state = '\"disconnected\"',
             record_json = json_remove(
                 json_set(
                     record_json,
                     '$.observedCliVersion', NULL,
                     '$.consumedContractRevision', NULL,
                     '$.compatibilityState', 'unassessed',
                     '$.observedState', 'disconnected',
                     '$.observedReasonCode', NULL
                 ),
                 '$.expectedCliVersion',
                 '$.executableSha256',
                 '$.protocolSchemaSha256'
             );",
    )
    .map_err(|error| persistence_error("apply schema migration 43", error))
}

pub(crate) fn migrate_v44_external_runtime_compatibility_probe(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "UPDATE external_runtime_registrations
         SET observed_state = '\"disconnected\"',
             record_json = json_set(
                 record_json,
                 '$.observedCliVersion', NULL,
                 '$.consumedContractRevision', NULL,
                 '$.compatibilityState', 'unassessed',
                 '$.lastCompatibilityProbe', NULL,
                 '$.observedState', 'disconnected',
                 '$.observedReasonCode', NULL
             );",
    )
    .map_err(|error| persistence_error("apply schema migration 44", error))
}

pub(crate) fn migrate_v45_external_runtime_certifications(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_runtime_certifications (
            certification_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            runtime_kind TEXT NOT NULL,
            observed_cli_version TEXT NOT NULL,
            consumed_contract_revision TEXT NOT NULL,
            probe_suite_revision TEXT NOT NULL,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS external_runtime_certifications_identity_idx
            ON external_runtime_certifications(
                runtime_kind,
                observed_cli_version,
                consumed_contract_revision,
                probe_suite_revision,
                status
            );
         CREATE TABLE IF NOT EXISTS external_runtime_probe_evidence (
            runtime_id TEXT PRIMARY KEY,
            observed_cli_version TEXT NOT NULL,
            consumed_contract_revision TEXT NOT NULL,
            probe_suite_revision TEXT NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(runtime_id) REFERENCES external_runtime_registrations(runtime_id)
         );",
    )
    .map_err(|error| persistence_error("apply schema migration 45", error))
}

pub(crate) fn migrate_v47_add_agent_message_reply_links(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "DELETE FROM queued_messages WHERE body LIKE '[Rusty Crew routed message]%';
         DELETE FROM agent_message_delivery_receipts;
         ALTER TABLE agent_message_delivery_receipts ADD COLUMN from_session_id TEXT;
         ALTER TABLE agent_message_delivery_receipts ADD COLUMN to_session_id TEXT;
         ALTER TABLE agent_message_delivery_receipts ADD COLUMN reply_to_message_id TEXT;
         ALTER TABLE agent_message_delivery_receipts ADD COLUMN created_at TEXT;
         CREATE UNIQUE INDEX agent_message_delivery_reply_once_idx
            ON agent_message_delivery_receipts(reply_to_message_id)
            WHERE reply_to_message_id IS NOT NULL;
         CREATE INDEX agent_message_delivery_recipient_session_idx
            ON agent_message_delivery_receipts(to_session_id, status, expires_at);",
    )
    .map_err(|error| persistence_error("add agent message session and reply linkage", error))
}

pub(crate) fn migrate_v48_add_agent_message_input_kind(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "DELETE FROM queued_messages WHERE state_reason LIKE 'agent_delivery:%';
         UPDATE agent_message_delivery_receipts
            SET record_json = json_set(
                record_json,
                '$.request.inputKind',
                CASE
                    WHEN json_extract(record_json, '$.request.fromAgentId') = 'rusty-view-operator'
                    THEN 'operator'
                    ELSE 'routed_agent_message'
                END
            );",
    )
    .map_err(|error| persistence_error("add explicit agent message input kind", error))
}

pub(crate) fn migrate_v49_add_agent_message_event_input_kind(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "UPDATE agent_message_delivery_receipts
            SET record_json = json_set(
                record_json,
                '$.request.inputKind',
                CASE
                    WHEN json_extract(record_json, '$.request.fromAgentId') = 'rusty-view-operator'
                    THEN 'operator'
                    ELSE 'routed_agent_message'
                END
            );
         UPDATE event_history
            SET event_json = json_set(
                event_json,
                '$.receipt.request.inputKind',
                CASE
                    WHEN json_extract(event_json, '$.receipt.request.fromAgentId') = 'rusty-view-operator'
                    THEN 'operator'
                    ELSE 'routed_agent_message'
                END
            )
          WHERE json_type(event_json, '$.receipt.request') IS NOT NULL;",
    )
    .map_err(|error| persistence_error("add agent message input kind to event history", error))
}

pub(crate) fn migrate_v50_repair_agent_message_event_input_kind(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "UPDATE event_history
            SET event_json = json_set(
                event_json,
                '$.receipt.request.inputKind',
                CASE
                    WHEN json_extract(event_json, '$.receipt.request.fromAgentId') = 'rusty-view-operator'
                    THEN 'operator'
                    ELSE 'routed_agent_message'
                END
            )
          WHERE json_type(event_json, '$.receipt.request') IS NOT NULL;",
    )
    .map_err(|error| persistence_error("repair agent message event input kind", error))
}

impl CoordinationStore {
    pub fn put_external_runtime_registration(
        &self,
        record: &ExternalRuntimeRegistration,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalRuntimeRegistration> {
        validate_external_runtime_registration(record)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external runtime registration", error))?;
        let current = load_json_optional::<ExternalRuntimeRegistration, _>(
            &tx,
            "SELECT record_json FROM external_runtime_registrations WHERE runtime_id = ?1",
            params![record.runtime_id.0.as_str()],
            "load external runtime registration",
        )?;
        validate_expected_revision(
            "external runtime",
            &record.runtime_id.0,
            current.as_ref().map(|value| value.revision),
            expected_revision,
        )?;
        let mut saved = record.clone();
        saved.revision = current.map(|value| value.revision + 1).unwrap_or(1);
        tx.execute(
            "INSERT INTO external_runtime_registrations
                (runtime_id, observed_state, revision, record_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(runtime_id) DO UPDATE SET
                observed_state = excluded.observed_state,
                revision = excluded.revision,
                record_json = excluded.record_json",
            params![
                saved.runtime_id.0,
                enum_json(&saved.observed_state)?,
                saved.revision as i64,
                to_json_text(&saved)?,
            ],
        )
        .map_err(|error| persistence_error("save external runtime registration", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external runtime registration", error))?;
        Ok(saved)
    }

    pub fn get_external_runtime_registration(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeRegistration>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_runtime_registrations WHERE runtime_id = ?1",
            params![runtime_id.0.as_str()],
            "load external runtime registration",
        )
    }

    pub fn list_external_runtime_registrations(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeRegistration>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_runtime_registrations ORDER BY runtime_id",
            [],
            "list external runtime registrations",
        )
    }

    pub fn record_external_runtime_certification(
        &self,
        record: &ExternalRuntimeCertificationRecord,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        validate_external_runtime_certification_record(record)?;
        if record.status != ExternalRuntimeCertificationStatus::Active || record.revision != 0 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "new external runtime certification must be active at revision zero",
            ));
        }
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external runtime certification", error))?;
        let by_id = load_json_optional::<ExternalRuntimeCertificationRecord, _>(
            &tx,
            "SELECT record_json FROM external_runtime_certifications WHERE certification_id = ?1",
            params![record.certification_id.as_str()],
            "load certification by identifier",
        )?;
        let by_key = load_json_optional::<ExternalRuntimeCertificationRecord, _>(
            &tx,
            "SELECT record_json FROM external_runtime_certifications WHERE idempotency_key = ?1",
            params![record.idempotency_key.as_str()],
            "load certification by idempotency key",
        )?;
        if let Some(existing) = by_id.or(by_key) {
            if same_certification_request(&existing, record) {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external runtime certification identifier or idempotency key was reused",
            ));
        }

        let active = load_json_list::<ExternalRuntimeCertificationRecord, _>(
            &tx,
            "SELECT record_json FROM external_runtime_certifications
             WHERE runtime_kind = ?1
               AND observed_cli_version = ?2
               AND consumed_contract_revision = ?3
               AND probe_suite_revision = ?4
               AND status = 'active'",
            params![
                enum_json(&record.runtime_kind)?,
                record.observed_cli_version.as_str(),
                record.consumed_contract_revision.as_str(),
                record.probe_suite_revision.as_str(),
            ],
            "load active external runtime certifications",
        )?;
        for mut previous in active {
            previous.status = ExternalRuntimeCertificationStatus::Superseded;
            previous.superseded_by_certification_id = Some(record.certification_id.clone());
            previous.revision += 1;
            previous.updated_at = record.created_at.clone();
            validate_external_runtime_certification_record(&previous)?;
            tx.execute(
                "UPDATE external_runtime_certifications
                 SET status = 'superseded', revision = ?2, record_json = ?3
                 WHERE certification_id = ?1",
                params![
                    previous.certification_id,
                    previous.revision as i64,
                    to_json_text(&previous)?,
                ],
            )
            .map_err(|error| persistence_error("supersede runtime certification", error))?;
        }

        let mut saved = record.clone();
        saved.revision = 1;
        tx.execute(
            "INSERT INTO external_runtime_certifications (
                certification_id, idempotency_key, runtime_kind,
                observed_cli_version, consumed_contract_revision,
                probe_suite_revision, status, revision, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8)",
            params![
                saved.certification_id,
                saved.idempotency_key,
                enum_json(&saved.runtime_kind)?,
                saved.observed_cli_version,
                saved.consumed_contract_revision,
                saved.probe_suite_revision,
                saved.revision as i64,
                to_json_text(&saved)?,
            ],
        )
        .map_err(|error| persistence_error("insert external runtime certification", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external runtime certification", error))?;
        Ok(saved)
    }

    pub fn put_external_runtime_probe_evidence(
        &self,
        evidence: &ExternalRuntimeProbeEvidenceRecord,
    ) -> CoreResult<()> {
        validate_external_runtime_probe_evidence(evidence)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO external_runtime_probe_evidence (
                runtime_id, observed_cli_version, consumed_contract_revision,
                probe_suite_revision, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(runtime_id) DO UPDATE SET
                observed_cli_version = excluded.observed_cli_version,
                consumed_contract_revision = excluded.consumed_contract_revision,
                probe_suite_revision = excluded.probe_suite_revision,
                record_json = excluded.record_json",
            params![
                evidence.runtime_id.0,
                evidence.observed_cli_version,
                evidence.consumed_contract_revision,
                evidence.probe_report.suite_revision,
                to_json_text(evidence)?,
            ],
        )
        .map_err(|error| persistence_error("save runtime probe evidence", error))?;
        Ok(())
    }

    pub fn get_external_runtime_probe_evidence(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeProbeEvidenceRecord>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_runtime_probe_evidence WHERE runtime_id = ?1",
            params![runtime_id.0.as_str()],
            "load runtime probe evidence",
        )
    }

    pub fn get_external_runtime_certification(
        &self,
        certification_id: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_runtime_certifications WHERE certification_id = ?1",
            params![certification_id],
            "load external runtime certification",
        )
    }

    pub fn list_external_runtime_certifications(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeCertificationRecord>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_runtime_certifications
             ORDER BY certification_id",
            [],
            "list external runtime certifications",
        )
    }

    pub fn find_active_external_runtime_certification(
        &self,
        runtime_kind: &rusty_crew_core_protocol::ExternalRuntimeKind,
        observed_cli_version: &str,
        consumed_contract_revision: &str,
        probe_suite_revision: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_runtime_certifications
             WHERE runtime_kind = ?1
               AND observed_cli_version = ?2
               AND consumed_contract_revision = ?3
               AND probe_suite_revision = ?4
               AND status = 'active'
             ORDER BY certification_id DESC LIMIT 1",
            params![
                enum_json(runtime_kind)?,
                observed_cli_version,
                consumed_contract_revision,
                probe_suite_revision,
            ],
            "find active external runtime certification",
        )
    }

    pub fn invalidate_external_runtime_certification(
        &self,
        invalidation: &ExternalRuntimeCertificationInvalidation,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        validate_external_runtime_certification_invalidation(invalidation)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start certification invalidation", error))?;
        let mut current = load_json_optional::<ExternalRuntimeCertificationRecord, _>(
            &tx,
            "SELECT record_json FROM external_runtime_certifications WHERE certification_id = ?1",
            params![invalidation.certification_id.as_str()],
            "load certification for invalidation",
        )?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "certification was not found"))?;
        validate_expected_revision(
            "external runtime certification",
            &current.certification_id,
            Some(current.revision),
            Some(invalidation.expected_revision),
        )?;
        if current.status != ExternalRuntimeCertificationStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "only an active certification can be invalidated",
            ));
        }
        current.status = ExternalRuntimeCertificationStatus::Invalidated;
        current.invalidated_at = Some(invalidation.invalidated_at.clone());
        current.invalidation_reason = Some(invalidation.reason.clone());
        current.updated_at = invalidation.invalidated_at.clone();
        current.revision += 1;
        validate_external_runtime_certification_record(&current)?;
        tx.execute(
            "UPDATE external_runtime_certifications
             SET status = 'invalidated', revision = ?2, record_json = ?3
             WHERE certification_id = ?1",
            params![
                current.certification_id,
                current.revision as i64,
                to_json_text(&current)?,
            ],
        )
        .map_err(|error| persistence_error("invalidate runtime certification", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit certification invalidation", error))?;
        Ok(current)
    }

    pub fn acquire_external_controller_lease(
        &self,
        candidate: &ExternalControllerLease,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external controller lease", error))?;
        let current = load_json_optional::<ExternalControllerLease, _>(
            &tx,
            "SELECT record_json FROM external_controller_leases WHERE runtime_id = ?1",
            params![candidate.runtime_id.0.as_str()],
            "load external controller lease",
        )?;
        if let Some(current) = &current {
            let held_by_other = current.holder_instance_id != candidate.holder_instance_id;
            if held_by_other && current.expires_at > *now {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "external runtime {} controller lease is held by another instance",
                        candidate.runtime_id.0
                    ),
                ));
            }
        }
        let mut saved = candidate.clone();
        saved.generation = current
            .as_ref()
            .map(|value| {
                if value.holder_instance_id == candidate.holder_instance_id {
                    value.generation
                } else {
                    value.generation.saturating_add(1)
                }
            })
            .unwrap_or(1);
        saved.revision = current.map(|value| value.revision + 1).unwrap_or(1);
        tx.execute(
            "INSERT INTO external_controller_leases
                (runtime_id, holder_instance_id, generation, expires_at, revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(runtime_id) DO UPDATE SET
                holder_instance_id = excluded.holder_instance_id,
                generation = excluded.generation,
                expires_at = excluded.expires_at,
                revision = excluded.revision,
                record_json = excluded.record_json",
            params![
                saved.runtime_id.0,
                saved.holder_instance_id,
                saved.generation as i64,
                saved.expires_at,
                saved.revision as i64,
                to_json_text(&saved)?,
            ],
        )
        .map_err(|error| persistence_error("save external controller lease", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external controller lease", error))?;
        Ok(saved)
    }

    pub fn release_external_controller_lease(
        &self,
        runtime_id: &ExternalRuntimeId,
        holder_instance_id: &str,
        generation: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start release external controller lease", error))?;
        let current = load_json_required::<ExternalControllerLease, _>(
            &tx,
            "SELECT record_json FROM external_controller_leases WHERE runtime_id = ?1",
            params![runtime_id.0.as_str()],
            "external controller lease",
        )?;
        if current.holder_instance_id != holder_instance_id || current.generation != generation {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "stale external controller cannot release the current lease",
            ));
        }
        let mut released = current.clone();
        released.renewed_at = now.clone();
        released.expires_at = now.clone();
        released.revision += 1;
        tx.execute(
            "UPDATE external_controller_leases SET expires_at = ?1, revision = ?2,
                record_json = ?3 WHERE runtime_id = ?4 AND revision = ?5",
            params![
                released.expires_at,
                released.revision as i64,
                to_json_text(&released)?,
                runtime_id.0,
                current.revision as i64,
            ],
        )
        .map_err(|error| persistence_error("release external controller lease", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external controller release", error))?;
        Ok(released)
    }

    pub fn get_external_controller_lease(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalControllerLease>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_controller_leases WHERE runtime_id = ?1",
            params![runtime_id.0.as_str()],
            "load external controller lease",
        )
    }

    pub fn put_external_agent_binding(
        &self,
        record: &ExternalAgentBinding,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalAgentBinding> {
        record.validate()?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external agent binding", error))?;
        let current = load_json_optional::<ExternalAgentBinding, _>(
            &tx,
            "SELECT record_json FROM external_agent_bindings WHERE binding_id = ?1",
            params![record.binding_id.0.as_str()],
            "load external agent binding",
        )?;
        if let Some(current) = current.as_ref() {
            let mut replay = record.clone();
            replay.revision = current.revision;
            if current.lineage.is_some() && &replay == current {
                return Ok(current.clone());
            }
        }
        validate_expected_revision(
            "external binding",
            &record.binding_id.0,
            current.as_ref().map(|value| value.revision),
            expected_revision,
        )?;
        let predecessor = match record.lineage.as_ref() {
            Some(lineage) => load_json_optional::<ExternalAgentBinding, _>(
                &tx,
                "SELECT record_json FROM external_agent_bindings WHERE binding_id = ?1",
                params![lineage.predecessor_binding_id.0.as_str()],
                "load external binding lineage predecessor",
            )?,
            None => None,
        };
        let referenced_as_predecessor = load_json_optional::<ExternalAgentBinding, _>(
            &tx,
            "SELECT record_json FROM external_agent_bindings
             WHERE json_extract(record_json, '$.lineage.predecessorBindingId') = ?1
             LIMIT 1",
            params![record.binding_id.0.as_str()],
            "load external binding lineage successor",
        )?
        .is_some();
        validate_external_agent_binding_transition(
            current.as_ref(),
            predecessor.as_ref(),
            referenced_as_predecessor,
            record,
        )?;
        let mut saved = record.clone();
        saved.revision = current
            .as_ref()
            .map(|value| value.revision + 1)
            .unwrap_or(1);
        tx.execute(
            "INSERT INTO external_agent_bindings
                (binding_id, runtime_id, session_id, agent_id, purpose, status,
                 native_thread_id, revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(binding_id) DO UPDATE SET
                runtime_id = excluded.runtime_id,
                session_id = excluded.session_id,
                agent_id = excluded.agent_id,
                purpose = excluded.purpose,
                status = excluded.status,
                native_thread_id = excluded.native_thread_id,
                revision = excluded.revision,
                record_json = excluded.record_json",
            params![
                saved.binding_id.0,
                saved.runtime_id.0,
                saved.session_id.as_ref().map(|value| value.0.as_str()),
                saved.agent_id.as_ref().map(|value| value.0.as_str()),
                enum_json(&saved.purpose)?,
                enum_json(&saved.status)?,
                saved.native_thread_id,
                saved.revision as i64,
                to_json_text(&saved)?,
            ],
        )
        .map_err(|error| persistence_error("save external agent binding", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external agent binding", error))?;
        Ok(saved)
    }

    pub fn get_external_binding_for_agent(
        &self,
        agent_id: &AgentId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        let conn = self.conn()?;
        let binding = load_json_optional::<ExternalAgentBinding, _>(
            &conn,
            "SELECT record_json FROM external_agent_bindings
             WHERE agent_id = ?1 AND purpose = 'crew_agent' AND status = 'active'",
            params![agent_id.0.as_str()],
            "load routable external agent binding",
        )?;
        Ok(binding.filter(ExternalAgentBinding::is_routable))
    }

    pub fn get_external_agent_binding(
        &self,
        binding_id: &ExternalBindingId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_agent_bindings WHERE binding_id = ?1",
            params![binding_id.0.as_str()],
            "load external agent binding",
        )
    }

    pub fn list_external_agent_bindings(&self) -> CoreResult<Vec<ExternalAgentBinding>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_agent_bindings ORDER BY binding_id",
            [],
            "list external agent bindings",
        )
    }

    pub fn create_external_agent_session_creation(
        &self,
        record: &ExternalAgentSessionCreationRecord,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external agent session creation", error))?;
        let existing = load_json_optional::<ExternalAgentSessionCreationRecord, _>(
            &tx,
            "SELECT record_json FROM external_agent_session_creations
             WHERE idempotency_key = ?1",
            params![record.request.idempotency_key.as_str()],
            "load idempotent external agent session creation",
        )?;
        if let Some(existing) = existing {
            if existing.request_fingerprint == record.request_fingerprint {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external_agent_creation_idempotency_conflict: idempotency key was reused with a different payload",
            ));
        }
        tx.execute(
            "INSERT INTO external_agent_session_creations
                (creation_id, idempotency_key, request_fingerprint, runtime_id,
                 profile_id, session_id, binding_id, phase, native_thread_id,
                 revision, updated_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.creation_id.0,
                record.request.idempotency_key,
                record.request_fingerprint,
                record.request.runtime_id.0,
                record.request.profile_id.0,
                record.session.session_id.0,
                record.binding.binding_id.0,
                enum_json(&record.phase)?,
                record.native_thread_id,
                record.revision as i64,
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save external agent session creation", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external agent session creation", error))?;
        Ok(record.clone())
    }

    pub fn get_external_agent_session_creation(
        &self,
        creation_id: &ExternalAgentSessionCreationId,
    ) -> CoreResult<Option<ExternalAgentSessionCreationRecord>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_agent_session_creations WHERE creation_id = ?1",
            params![creation_id.0.as_str()],
            "load external agent session creation",
        )
    }

    pub fn list_external_agent_session_creations(
        &self,
    ) -> CoreResult<Vec<ExternalAgentSessionCreationRecord>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_agent_session_creations ORDER BY creation_id",
            [],
            "list external agent session creations",
        )
    }

    pub fn update_external_agent_session_creation(
        &self,
        next: &ExternalAgentSessionCreationRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start update external agent session creation", error)
        })?;
        let current = load_json_required::<ExternalAgentSessionCreationRecord, _>(
            &tx,
            "SELECT record_json FROM external_agent_session_creations WHERE creation_id = ?1",
            params![next.creation_id.0.as_str()],
            "external agent session creation",
        )?;
        if current == *next {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return revision_conflict(
                "external agent session creation",
                expected_revision,
                current.revision,
            );
        }
        if !current.phase.can_transition_to(next.phase) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_phase_conflict: invalid creation phase transition",
            ));
        }
        if current.phase == rusty_crew_core_protocol::ExternalAgentSessionCreationPhase::Ready {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_ready_immutable: completed creation is immutable",
            ));
        }
        if current.creation_id != next.creation_id
            || current.request != next.request
            || current.request_fingerprint != next.request_fingerprint
            || current.session.session_id != next.session.session_id
            || current.binding.binding_id != next.binding.binding_id
            || current.native_thread_source != next.native_thread_source
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_identity_conflict: creation identity fields are immutable",
            ));
        }
        if current.native_thread_id.is_some() && current.native_thread_id != next.native_thread_id {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_native_thread_conflict: native thread cannot be rebound",
            ));
        }
        let mut saved = next.clone();
        saved.revision = current.revision + 1;
        tx.execute(
            "UPDATE external_agent_session_creations SET phase = ?1,
                native_thread_id = ?2, revision = ?3, updated_at = ?4, record_json = ?5
             WHERE creation_id = ?6 AND revision = ?7",
            params![
                enum_json(&saved.phase)?,
                saved.native_thread_id,
                saved.revision as i64,
                saved.updated_at,
                to_json_text(&saved)?,
                saved.creation_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update external agent session creation", error))?;
        tx.commit().map_err(|error| {
            persistence_error("commit update external agent session creation", error)
        })?;
        Ok(saved)
    }

    pub fn create_external_turn(
        &self,
        record: &ExternalTurnCorrelation,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external turn", error))?;
        let existing = load_json_optional::<ExternalTurnCorrelation, _>(
            &tx,
            "SELECT record_json FROM external_turns WHERE idempotency_key = ?1",
            params![record.request.idempotency_key.as_str()],
            "load idempotent external turn",
        )?;
        if let Some(existing) = existing {
            if existing == *record {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external turn idempotency key conflicts with a different request",
            ));
        }
        tx.execute(
            "INSERT INTO external_turns
                (request_id, idempotency_key, runtime_id, binding_id, session_id,
                 native_thread_id, native_turn_id, phase, revision, created_at,
                 creation_ordinal, updated_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                     (SELECT COALESCE(MAX(creation_ordinal), 0) + 1 FROM external_turns), ?11, ?12)",
            params![
                record.request.request_id.0,
                record.request.idempotency_key,
                record.runtime_id.0,
                record.request.binding_id.0,
                record.request.session_id.0,
                record.native_thread_id,
                record.native_turn_id,
                enum_json(&record.phase)?,
                record.revision as i64,
                record.request.created_at,
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save external turn", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external turn", error))?;
        Ok(record.clone())
    }

    pub fn promote_queued_message_to_external_turn(
        &self,
        queued_message_id: &str,
        now: &IsoTimestamp,
        record: &ExternalTurnCorrelation,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start queued external turn promotion", error))?;
        let existing = load_json_optional::<ExternalTurnCorrelation, _>(
            &tx,
            "SELECT record_json FROM external_turns WHERE idempotency_key = ?1",
            params![record.request.idempotency_key.as_str()],
            "load idempotent queued external turn",
        )?;
        if let Some(existing) = existing {
            if existing == *record {
                return Ok(Some(existing));
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external turn idempotency key conflicts with a different queued request",
            ));
        }
        let Some(mut queued) = load_queued_messages_in_tx(
            &tx,
            &QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(record.request.session_id.clone()),
                owner_agent_id: None,
                limit: None,
            },
        )?
        .into_iter()
        .find(|queued| queued.message_id == queued_message_id) else {
            return Ok(None);
        };
        if queued.expires_at <= *now {
            queued.state = QueuedMessageState::Expired;
            queued.terminal_at = Some(now.clone());
            queued.state_reason = Some("ttl_expired_before_external_turn_claim".into());
            save_queued_message_in_tx(&tx, &queued)?;
            tx.commit()
                .map_err(|error| persistence_error("commit expired queued external turn", error))?;
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO external_turns
                (request_id, idempotency_key, runtime_id, binding_id, session_id,
                 native_thread_id, native_turn_id, phase, revision, created_at,
                 creation_ordinal, updated_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                     (SELECT COALESCE(MAX(creation_ordinal), 0) + 1 FROM external_turns), ?11, ?12)",
            params![
                record.request.request_id.0,
                record.request.idempotency_key,
                record.runtime_id.0,
                record.request.binding_id.0,
                record.request.session_id.0,
                record.native_thread_id,
                record.native_turn_id,
                enum_json(&record.phase)?,
                record.revision as i64,
                record.request.created_at,
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save promoted external turn", error))?;
        queued.state = QueuedMessageState::Delivered;
        queued.delivery_attempts += 1;
        queued.terminal_at = Some(now.clone());
        queued.state_reason = Some(format!(
            "promoted_to_external_turn:{}",
            record.request.request_id.0
        ));
        save_queued_message_in_tx(&tx, &queued)?;
        tx.commit()
            .map_err(|error| persistence_error("commit queued external turn promotion", error))?;
        Ok(Some(record.clone()))
    }

    pub fn update_external_turn(
        &self,
        next: &ExternalTurnCorrelation,
        expected_revision: u64,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update external turn", error))?;
        let current = load_json_required::<ExternalTurnCorrelation, _>(
            &tx,
            "SELECT record_json FROM external_turns WHERE request_id = ?1",
            params![next.request.request_id.0.as_str()],
            "external turn",
        )?;
        if current == *next {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return revision_conflict("external turn", expected_revision, current.revision);
        }
        validate_external_turn_transition(current.phase, next.phase)?;
        if next.phase.is_terminal() && next.capacity_lease_id.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "terminal external turn must release capacity",
            ));
        }
        if current.native_turn_id.is_some() && next.native_turn_id != current.native_turn_id {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external turn native_turn_id cannot be rebound",
            ));
        }
        if current.phase.is_terminal() && current != *next {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "terminal external turn is immutable",
            ));
        }
        let mut saved = next.clone();
        saved.revision = current.revision + 1;
        tx.execute(
            "UPDATE external_turns SET native_turn_id = ?1, phase = ?2,
                revision = ?3, updated_at = ?4, record_json = ?5
             WHERE request_id = ?6 AND revision = ?7",
            params![
                saved.native_turn_id,
                enum_json(&saved.phase)?,
                saved.revision as i64,
                saved.updated_at,
                to_json_text(&saved)?,
                saved.request.request_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update external turn", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit update external turn", error))?;
        Ok(saved)
    }

    pub fn get_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_turns WHERE request_id = ?1",
            params![request_id.0.as_str()],
            "load external turn",
        )
    }

    pub fn list_external_turns_for_native_thread(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_thread_id: &str,
    ) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_turns
             WHERE runtime_id = ?1 AND native_thread_id = ?2
             ORDER BY updated_at, request_id",
            params![runtime_id.0.as_str(), native_thread_id],
            "list external turns for native thread",
        )
    }

    pub fn query_external_turn_page(
        &self,
        query: &rusty_crew_core_protocol::ExternalTurnPageQuery,
    ) -> CoreResult<rusty_crew_core_protocol::ExternalTurnPage> {
        let limit = query.limit.clamp(1, 100);
        let conn = self.conn()?;
        if let Some(before) = query.before.as_ref() {
            let stored_ordinal = conn
                .query_row(
                    "SELECT creation_ordinal FROM external_turns
                     WHERE runtime_id = ?1 AND native_thread_id = ?2 AND request_id = ?3",
                    params![
                        query.runtime_id.0,
                        query.native_thread_id,
                        before.request_id.0
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|error| persistence_error("validate external turn page cursor", error))?;
            if stored_ordinal != Some(before.creation_ordinal as i64) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "external_turn_page_cursor_mismatch",
                ));
            }
        }
        let mut items = if let Some(before) = query.before.as_ref() {
            load_external_turn_page_entries(
                &conn,
                "SELECT creation_ordinal, record_json FROM external_turns
                 WHERE runtime_id = ?1 AND native_thread_id = ?2
                   AND creation_ordinal < ?3
                 ORDER BY creation_ordinal DESC LIMIT ?4",
                params![
                    query.runtime_id.0,
                    query.native_thread_id,
                    before.creation_ordinal as i64,
                    i64::from(limit + 1),
                ],
                "query external turn page",
            )?
        } else {
            load_external_turn_page_entries(
                &conn,
                "SELECT creation_ordinal, record_json FROM external_turns
                 WHERE runtime_id = ?1 AND native_thread_id = ?2
                 ORDER BY creation_ordinal DESC LIMIT ?3",
                params![
                    query.runtime_id.0,
                    query.native_thread_id,
                    i64::from(limit + 1)
                ],
                "query external turn page",
            )?
        };
        let has_more_before = items.len() > limit as usize;
        items.truncate(limit as usize);
        items.reverse();
        Ok(rusty_crew_core_protocol::ExternalTurnPage {
            items,
            has_more_before,
        })
    }

    pub fn list_nonterminal_external_turns(&self) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_turns
             WHERE phase IN ('accepted', 'starting', 'active', 'waiting_interaction')
             ORDER BY updated_at, request_id",
            [],
            "list nonterminal external turns",
        )
    }

    pub fn put_external_control_receipt(
        &self,
        receipt: &ExternalControlReceipt,
    ) -> CoreResult<ExternalControlReceipt> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external control receipt", error))?;
        let existing = load_json_optional::<ExternalControlReceipt, _>(
            &tx,
            "SELECT record_json FROM external_control_receipts WHERE idempotency_key = ?1",
            params![receipt.request.idempotency_key.as_str()],
            "load external control receipt",
        )?;
        if let Some(existing) = existing {
            if existing.request_fingerprint == receipt.request_fingerprint {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external control idempotency key conflicts with a different payload",
            ));
        }
        tx.execute(
            "INSERT INTO external_control_receipts
                (control_id, idempotency_key, binding_id, request_fingerprint,
                 status, revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                receipt.request.control_id.0,
                receipt.request.idempotency_key,
                receipt.request.binding_id.0,
                receipt.request_fingerprint,
                enum_json(&receipt.status)?,
                receipt.revision as i64,
                to_json_text(receipt)?,
            ],
        )
        .map_err(|error| persistence_error("save external control receipt", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external control receipt", error))?;
        Ok(receipt.clone())
    }

    pub fn get_external_control_receipt(
        &self,
        control_id: &ExternalControlId,
    ) -> CoreResult<Option<ExternalControlReceipt>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM external_control_receipts WHERE control_id = ?1",
            params![control_id.0.as_str()],
            "load external control receipt",
        )
    }

    pub fn update_external_control_receipt(
        &self,
        next: &ExternalControlReceipt,
        expected_revision: u64,
    ) -> CoreResult<ExternalControlReceipt> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update external control", error))?;
        let current = load_json_required::<ExternalControlReceipt, _>(
            &tx,
            "SELECT record_json FROM external_control_receipts WHERE control_id = ?1",
            params![next.request.control_id.0.as_str()],
            "external control receipt",
        )?;
        if current == *next {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return revision_conflict("external control", expected_revision, current.revision);
        }
        if current.status.is_terminal() && current != *next {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "terminal external control receipt is immutable",
            ));
        }
        if !current.status.is_terminal() && !next.status.is_terminal() && current != *next {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "pending external control may only transition to terminal",
            ));
        }
        let mut saved = next.clone();
        saved.revision = current.revision + 1;
        tx.execute(
            "UPDATE external_control_receipts SET status = ?1, revision = ?2,
                record_json = ?3 WHERE control_id = ?4 AND revision = ?5",
            params![
                enum_json(&saved.status)?,
                saved.revision as i64,
                to_json_text(&saved)?,
                saved.request.control_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update external control receipt", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit update external control", error))?;
        Ok(saved)
    }

    pub fn put_external_interaction(
        &self,
        record: &ExternalInteractionRecord,
    ) -> CoreResult<ExternalInteractionRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external interaction", error))?;
        let existing = load_json_optional::<ExternalInteractionRecord, _>(
            &tx,
            "SELECT record_json FROM external_interactions
             WHERE interaction_id = ?1 OR (runtime_id = ?2 AND native_request_id = ?3)",
            params![
                record.interaction_id.0.as_str(),
                record.runtime_id.0.as_str(),
                record.native_request_id.as_str(),
            ],
            "load idempotent external interaction",
        )?;
        if let Some(existing) = existing {
            if existing == *record {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external interaction identity conflicts with a different request",
            ));
        }
        tx.execute(
            "INSERT INTO external_interactions
                (interaction_id, runtime_id, binding_id, request_id, native_request_id,
                 status, expires_at, revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.interaction_id.0,
                record.runtime_id.0,
                record.binding_id.0,
                record.request_id.0,
                record.native_request_id,
                enum_json(&record.status)?,
                record.expires_at,
                record.revision as i64,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save external interaction", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external interaction", error))?;
        Ok(record.clone())
    }

    pub fn update_external_interaction(
        &self,
        next: &ExternalInteractionRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalInteractionRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update external interaction", error))?;
        let current = load_json_required::<ExternalInteractionRecord, _>(
            &tx,
            "SELECT record_json FROM external_interactions WHERE interaction_id = ?1",
            params![next.interaction_id.0.as_str()],
            "external interaction",
        )?;
        if current == *next {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return revision_conflict("external interaction", expected_revision, current.revision);
        }
        if current.status != ExternalInteractionStatus::Pending && current != *next {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "terminal external interaction is immutable",
            ));
        }
        if current.status == ExternalInteractionStatus::Pending
            && next.status == ExternalInteractionStatus::Pending
            && current != *next
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "pending external interaction may only transition to terminal",
            ));
        }
        let mut saved = next.clone();
        saved.revision = current.revision + 1;
        tx.execute(
            "UPDATE external_interactions SET status = ?1, revision = ?2,
                record_json = ?3 WHERE interaction_id = ?4 AND revision = ?5",
            params![
                enum_json(&saved.status)?,
                saved.revision as i64,
                to_json_text(&saved)?,
                saved.interaction_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update external interaction", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit update external interaction", error))?;
        Ok(saved)
    }

    pub fn list_pending_external_interactions(&self) -> CoreResult<Vec<ExternalInteractionRecord>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_interactions
             WHERE status = 'pending' ORDER BY expires_at, interaction_id",
            [],
            "list pending external interactions",
        )
    }

    pub fn append_external_runtime_event(
        &self,
        event: &NormalizedExternalRuntimeEvent,
    ) -> CoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start append external runtime event", error))?;
        let existing = load_json_optional::<NormalizedExternalRuntimeEvent, _>(
            &tx,
            "SELECT record_json FROM external_runtime_events
             WHERE event_id = ?1 OR (runtime_id = ?2 AND sequence_id = ?3)",
            params![
                event.event_id.as_str(),
                event.runtime_id.0.as_str(),
                event.sequence_id as i64
            ],
            "load idempotent external runtime event",
        )?;
        if let Some(existing) = existing {
            if existing == *event {
                return Ok(());
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external runtime event identity conflicts with a different payload",
            ));
        }
        tx.execute(
            "INSERT INTO external_runtime_events
                (event_id, runtime_id, session_id, sequence_id, kind, created_at,
                 native_thread_id, native_turn_id, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.event_id,
                event.runtime_id.0,
                event.session_id.as_ref().map(|value| value.0.as_str()),
                event.sequence_id as i64,
                event.kind,
                event.created_at,
                event.native_thread_id,
                event.native_turn_id,
                to_json_text(event)?,
            ],
        )
        .map_err(|error| persistence_error("append external runtime event", error))?;
        tx.execute(
            "INSERT INTO external_runtime_event_cursors(runtime_id, next_sequence_id)
             VALUES (?1, ?2)
             ON CONFLICT(runtime_id) DO UPDATE SET next_sequence_id =
                MAX(external_runtime_event_cursors.next_sequence_id, excluded.next_sequence_id)",
            params![
                event.runtime_id.0,
                event.sequence_id.saturating_add(1) as i64
            ],
        )
        .map_err(|error| persistence_error("advance external runtime event cursor", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external runtime event", error))?;
        Ok(())
    }

    pub fn append_external_runtime_event_allocated(
        &self,
        input: &ExternalRuntimeEventInput,
    ) -> CoreResult<NormalizedExternalRuntimeEvent> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start allocated external runtime event", error))?;
        let existing = load_json_optional::<NormalizedExternalRuntimeEvent, _>(
            &tx,
            "SELECT record_json FROM external_runtime_events WHERE event_id = ?1",
            params![input.event_id.as_str()],
            "load allocated external runtime event",
        )?;
        if let Some(existing) = existing {
            if external_event_matches_input(&existing, input) {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external runtime event id conflicts with a different payload",
            ));
        }
        tx.execute(
            "INSERT OR IGNORE INTO external_runtime_event_cursors(runtime_id, next_sequence_id)
             SELECT ?1, COALESCE(MAX(sequence_id), 0) + 1
               FROM external_runtime_events WHERE runtime_id = ?1",
            params![input.runtime_id.0.as_str()],
        )
        .map_err(|error| persistence_error("initialize external runtime event cursor", error))?;
        let next_sequence = tx
            .query_row(
                "SELECT next_sequence_id FROM external_runtime_event_cursors
                 WHERE runtime_id = ?1",
                params![input.runtime_id.0.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| persistence_error("allocate external runtime event sequence", error))?
            as u64;
        tx.execute(
            "UPDATE external_runtime_event_cursors SET next_sequence_id = ?2
             WHERE runtime_id = ?1",
            params![
                input.runtime_id.0.as_str(),
                next_sequence.saturating_add(1) as i64
            ],
        )
        .map_err(|error| persistence_error("advance external runtime event cursor", error))?;
        let event = normalized_event_from_input(input, next_sequence);
        tx.execute(
            "INSERT INTO external_runtime_events
                (event_id, runtime_id, session_id, sequence_id, kind, created_at,
                 native_thread_id, native_turn_id, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.event_id,
                event.runtime_id.0,
                event.session_id.as_ref().map(|value| value.0.as_str()),
                event.sequence_id as i64,
                event.kind,
                event.created_at,
                event.native_thread_id,
                event.native_turn_id,
                to_json_text(&event)?,
            ],
        )
        .map_err(|error| persistence_error("append allocated external runtime event", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit allocated external runtime event", error))?;
        Ok(event)
    }

    pub fn query_external_runtime_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_runtime_events
             WHERE runtime_id = ?1 AND sequence_id > ?2
             ORDER BY sequence_id LIMIT ?3",
            params![
                runtime_id.0.as_str(),
                after_sequence as i64,
                limit.clamp(1, 1_000)
            ],
            "query external runtime events",
        )
    }

    pub fn query_external_runtime_thread_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_thread_id: &str,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_runtime_events
             WHERE runtime_id = ?1 AND native_thread_id = ?2 AND sequence_id > ?3
             ORDER BY sequence_id LIMIT ?4",
            params![
                runtime_id.0.as_str(),
                native_thread_id,
                after_sequence as i64,
                limit.clamp(1, 1_000)
            ],
            "query external runtime thread events",
        )
    }

    pub fn query_external_runtime_turn_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_turn_id: &str,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM external_runtime_events
             WHERE runtime_id = ?1 AND native_turn_id = ?2 AND sequence_id > ?3
             ORDER BY sequence_id LIMIT ?4",
            params![
                runtime_id.0,
                native_turn_id,
                after_sequence as i64,
                limit.clamp(1, 512)
            ],
            "query external runtime turn events",
        )
    }

    pub fn query_external_runtime_event_tail(
        &self,
        runtime_id: &ExternalRuntimeId,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        let conn = self.conn()?;
        let mut events = load_json_list(
            &conn,
            "SELECT record_json FROM external_runtime_events
             WHERE runtime_id = ?1
             ORDER BY sequence_id DESC LIMIT ?2",
            params![runtime_id.0.as_str(), limit.clamp(1, 1_000)],
            "query external runtime event tail",
        )?;
        events.reverse();
        Ok(events)
    }

    pub fn create_agent_correlated_round(
        &self,
        record: &AgentCorrelatedRound,
    ) -> CoreResult<AgentCorrelatedRound> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start external correlated round", error))?;
        let existing = load_json_optional::<AgentCorrelatedRound, _>(
            &tx,
            "SELECT record_json FROM agent_correlated_rounds WHERE idempotency_key = ?1",
            params![record.idempotency_key.as_str()],
            "load external correlated round",
        )?;
        if let Some(existing) = existing {
            if existing == *record {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external round idempotency key conflicts with a different request",
            ));
        }
        tx.execute(
            "INSERT INTO agent_correlated_rounds
                (round_id, idempotency_key, sender_agent_id, sender_session_id,
                 recipient_agent_id, recipient_session_id, correlation_id, status, expires_at,
                 revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.round_id.0,
                record.idempotency_key,
                record.sender_agent_id.0,
                record.sender_session_id.as_ref().map(|id| id.0.as_str()),
                record.recipient_agent_id.0,
                record.recipient_session_id.0,
                record.correlation_id,
                enum_json(&record.status)?,
                record.expires_at,
                record.revision as i64,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save external correlated round", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external correlated round", error))?;
        Ok(record.clone())
    }

    pub fn create_agent_message_delivery(
        &self,
        record: &AgentMessageDeliveryReceipt,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start agent message delivery", error))?;
        let existing = load_json_optional::<AgentMessageDeliveryReceipt, _>(
            &tx,
            "SELECT record_json FROM agent_message_delivery_receipts WHERE idempotency_key = ?1",
            params![record.request.idempotency_key.as_str()],
            "load agent message delivery",
        )?;
        if let Some(existing) = existing {
            if existing.request == record.request {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "agent message delivery idempotency key conflicts with a different request",
            ));
        }
        tx.execute(
            "INSERT INTO agent_message_delivery_receipts
                (delivery_id, idempotency_key, message_id, from_agent_id, from_session_id,
                 to_agent_id, to_session_id, reply_to_message_id, status, created_at,
                 expires_at, revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.request.delivery_id.0,
                record.request.idempotency_key,
                record.request.message_id,
                record.request.from_agent_id.0,
                record
                    .request
                    .from_session_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                record.request.to_agent_id.0,
                record
                    .request
                    .to_session_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                record.request.reply_to_message_id,
                enum_json(&record.status)?,
                record.request.created_at,
                record.request.expires_at,
                record.revision as i64,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save agent message delivery", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit agent message delivery", error))?;
        Ok(record.clone())
    }

    pub fn update_agent_message_delivery(
        &self,
        next: &AgentMessageDeliveryReceipt,
        expected_revision: u64,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update agent message delivery", error))?;
        let current = load_json_required::<AgentMessageDeliveryReceipt, _>(
            &tx,
            "SELECT record_json FROM agent_message_delivery_receipts WHERE delivery_id = ?1",
            params![next.request.delivery_id.0.as_str()],
            "agent message delivery",
        )?;
        if current == *next {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return revision_conflict(
                "agent message delivery",
                expected_revision,
                current.revision,
            );
        }
        if current.status.is_terminal() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "terminal agent message delivery is immutable",
            ));
        }
        let attaches_initial_steer = next.status == AgentMessageDeliveryStatus::Pending
            && current.status == AgentMessageDeliveryStatus::Pending
            && current.activation.is_none()
            && matches!(
                next.activation.as_ref(),
                Some(AgentActivation::ExternalTurnSteerRequested { .. })
            )
            && next.sequence.is_some()
            && next.reason_code.is_none()
            && next.terminal_at.is_none();
        if next.status == AgentMessageDeliveryStatus::Pending && !attaches_initial_steer {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "pending agent message delivery may only attach its initial steer activation",
            ));
        }
        let mut saved = next.clone();
        saved.revision = current.revision + 1;
        tx.execute(
            "UPDATE agent_message_delivery_receipts SET status = ?1, revision = ?2,
                record_json = ?3 WHERE delivery_id = ?4 AND revision = ?5",
            params![
                enum_json(&saved.status)?,
                saved.revision as i64,
                to_json_text(&saved)?,
                saved.request.delivery_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update agent message delivery", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit update agent message delivery", error))?;
        Ok(saved)
    }

    pub fn get_agent_message_delivery(
        &self,
        delivery_id: &rusty_crew_core_protocol::AgentMessageDeliveryId,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM agent_message_delivery_receipts WHERE delivery_id = ?1",
            params![delivery_id.0.as_str()],
            "load agent message delivery",
        )
    }

    pub fn get_agent_message_delivery_by_message_id(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM agent_message_delivery_receipts WHERE message_id = ?1",
            params![message_id],
            "load agent message delivery by message id",
        )
    }

    pub fn get_agent_message_reply(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM agent_message_delivery_receipts
             WHERE reply_to_message_id = ?1",
            params![message_id],
            "load agent message reply",
        )
    }

    pub fn list_agent_message_inbox_deliveries(
        &self,
        query: &rusty_crew_core_protocol::AgentMessageInboxQuery,
        limit: u32,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM agent_message_delivery_receipts
             WHERE reply_to_message_id IS NULL
               AND (?1 IS NULL OR to_agent_id = ?1)
               AND (?2 IS NULL OR to_session_id = ?2)
               AND (?3 IS NULL OR from_agent_id = ?3)
               AND (?4 IS NULL OR from_session_id = ?4)
               AND (?5 IS NULL OR json_extract(record_json, '$.request.correlationId') = ?5)
               AND (?6 IS NULL OR message_id = ?6)
             ORDER BY created_at, delivery_id LIMIT ?7",
            params![
                query.to_agent_id.as_ref().map(|value| value.0.as_str()),
                query.to_session_id.as_ref().map(|value| value.0.as_str()),
                query.from_agent_id.as_ref().map(|value| value.0.as_str()),
                query.from_session_id.as_ref().map(|value| value.0.as_str()),
                query.correlation_id.as_deref(),
                query.message_id.as_deref(),
                i64::from(limit)
            ],
            "list agent message inbox deliveries",
        )
    }

    pub fn list_agent_message_traffic_deliveries(
        &self,
        query: &rusty_crew_core_protocol::AgentMessageInboxQuery,
        limit: u32,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM agent_message_delivery_receipts
             WHERE (?1 IS NULL OR to_agent_id = ?1)
               AND (?2 IS NULL OR to_session_id = ?2)
               AND (?3 IS NULL OR from_agent_id = ?3)
               AND (?4 IS NULL OR from_session_id = ?4)
               AND (?5 IS NULL OR json_extract(record_json, '$.request.correlationId') = ?5)
               AND (?6 IS NULL OR message_id = ?6)
             ORDER BY created_at, delivery_id LIMIT ?7",
            params![
                query.to_agent_id.as_ref().map(|value| value.0.as_str()),
                query.to_session_id.as_ref().map(|value| value.0.as_str()),
                query.from_agent_id.as_ref().map(|value| value.0.as_str()),
                query.from_session_id.as_ref().map(|value| value.0.as_str()),
                query.correlation_id.as_deref(),
                query.message_id.as_deref(),
                i64::from(limit)
            ],
            "list agent message traffic deliveries",
        )
    }

    pub fn get_agent_correlated_round(
        &self,
        round_id: &rusty_crew_core_protocol::AgentRoundId,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        let conn = self.conn()?;
        load_json_optional(
            &conn,
            "SELECT record_json FROM agent_correlated_rounds WHERE round_id = ?1",
            params![round_id.0.as_str()],
            "load agent correlated round",
        )
    }

    pub fn list_pending_agent_message_deliveries(
        &self,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM agent_message_delivery_receipts
             WHERE status = 'pending' ORDER BY expires_at, delivery_id",
            [],
            "list pending agent message deliveries",
        )
    }

    pub fn update_agent_correlated_round(
        &self,
        next: &AgentCorrelatedRound,
        expected_revision: u64,
    ) -> CoreResult<AgentCorrelatedRound> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update external round", error))?;
        let current = load_json_required::<AgentCorrelatedRound, _>(
            &tx,
            "SELECT record_json FROM agent_correlated_rounds WHERE round_id = ?1",
            params![next.round_id.0.as_str()],
            "external correlated round",
        )?;
        if current == *next {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return revision_conflict("external round", expected_revision, current.revision);
        }
        if current.status != AgentRoundStatus::Pending && current != *next {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "terminal external correlated round is immutable",
            ));
        }
        if current.status == AgentRoundStatus::Pending
            && next.status == AgentRoundStatus::Pending
            && current != *next
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "pending external round may only transition to a terminal status",
            ));
        }
        let mut saved = next.clone();
        saved.revision = current.revision + 1;
        tx.execute(
            "UPDATE agent_correlated_rounds SET status = ?1, revision = ?2,
                record_json = ?3 WHERE round_id = ?4 AND revision = ?5",
            params![
                enum_json(&saved.status)?,
                saved.revision as i64,
                to_json_text(&saved)?,
                saved.round_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update external correlated round", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external correlated round", error))?;
        Ok(saved)
    }

    pub fn list_pending_agent_rounds(&self) -> CoreResult<Vec<AgentCorrelatedRound>> {
        let conn = self.conn()?;
        load_json_list(
            &conn,
            "SELECT record_json FROM agent_correlated_rounds
             WHERE status = 'pending' ORDER BY expires_at, round_id",
            [],
            "list pending external correlated rounds",
        )
    }
}

fn normalized_event_from_input(
    input: &ExternalRuntimeEventInput,
    sequence_id: u64,
) -> NormalizedExternalRuntimeEvent {
    NormalizedExternalRuntimeEvent {
        event_id: input.event_id.clone(),
        session_id: input.session_id.clone(),
        sequence_id,
        created_at: input.created_at.clone(),
        kind: input.kind.clone(),
        runtime_id: input.runtime_id.clone(),
        native_thread_id: input.native_thread_id.clone(),
        native_turn_id: input.native_turn_id.clone(),
        item_id: input.item_id.clone(),
        request_id: input.request_id.clone(),
        payload: input.payload.clone(),
        raw_detail_ref: input.raw_detail_ref.clone(),
    }
}

fn external_event_matches_input(
    event: &NormalizedExternalRuntimeEvent,
    input: &ExternalRuntimeEventInput,
) -> bool {
    normalized_event_from_input(input, event.sequence_id) == *event
}

fn enum_json<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_value(value)
        .map_err(|error| CoreError::new(CoreErrorKind::InternalError, error.to_string()))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "enum did not serialize as string",
            )
        })
}

fn load_json_optional<T: DeserializeOwned, P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    context: &str,
) -> CoreResult<Option<T>> {
    let json = conn
        .query_row(sql, params, |row| row.get::<_, String>(0))
        .optional()
        .map_err(|error| persistence_error(context, error))?;
    json.map(|json| from_json_text(&json).map_err(|error| persistence_error(context, error)))
        .transpose()
}

fn load_json_required<T: DeserializeOwned, P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    label: &str,
) -> CoreResult<T> {
    load_json_optional(conn, sql, params, &format!("load {label}"))?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, format!("{label} was not found")))
}

fn load_json_list<T: DeserializeOwned, P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    context: &str,
) -> CoreResult<Vec<T>> {
    let mut statement = conn
        .prepare(sql)
        .map_err(|error| persistence_error(&format!("prepare {context}"), error))?;
    let rows = statement
        .query_map(params, |row| row.get::<_, String>(0))
        .map_err(|error| persistence_error(context, error))?;
    rows.map(|row| {
        let json = row.map_err(|error| persistence_error(context, error))?;
        from_json_text(&json).map_err(|error| persistence_error(context, error))
    })
    .collect()
}

fn load_external_turn_page_entries<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    context: &str,
) -> CoreResult<Vec<rusty_crew_core_protocol::ExternalTurnPageEntry>> {
    let mut statement = conn
        .prepare(sql)
        .map_err(|error| persistence_error(&format!("prepare {context}"), error))?;
    let rows = statement
        .query_map(params, |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| persistence_error(context, error))?;
    rows.map(|row| {
        let (creation_ordinal, json) = row.map_err(|error| persistence_error(context, error))?;
        let turn: ExternalTurnCorrelation =
            from_json_text(&json).map_err(|error| persistence_error(context, error))?;
        Ok(rusty_crew_core_protocol::ExternalTurnPageEntry {
            cursor: rusty_crew_core_protocol::ExternalTurnPageCursor {
                creation_ordinal: creation_ordinal as u64,
                request_id: turn.request.request_id.clone(),
            },
            turn,
        })
    })
    .collect()
}

fn validate_expected_revision(
    label: &str,
    id: &str,
    current: Option<u64>,
    expected: Option<u64>,
) -> CoreResult<()> {
    match (current, expected) {
        (None, None) => Ok(()),
        (Some(found), Some(expected)) if found == expected => Ok(()),
        (None, Some(expected)) => Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("{label} {id} revision mismatch: expected {expected}, record is missing"),
        )),
        (Some(found), expected) => Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("{label} {id} revision mismatch: expected {expected:?}, found {found}"),
        )),
    }
}

fn same_certification_request(
    current: &ExternalRuntimeCertificationRecord,
    candidate: &ExternalRuntimeCertificationRecord,
) -> bool {
    current.certification_id == candidate.certification_id
        && current.idempotency_key == candidate.idempotency_key
        && current.certified_runtime_id == candidate.certified_runtime_id
        && current.runtime_kind == candidate.runtime_kind
        && current.observed_cli_version == candidate.observed_cli_version
        && current.consumed_contract_revision == candidate.consumed_contract_revision
        && current.probe_suite_revision == candidate.probe_suite_revision
        && current.evidence_summary == candidate.evidence_summary
}

fn revision_conflict<T>(label: &str, expected: u64, found: u64) -> CoreResult<T> {
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!("{label} revision mismatch: expected {expected}, found {found}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AgentRoundId, ExternalBindingPurpose, ExternalBindingStatus, ExternalControlId,
        ExternalControlKind, ExternalControlReceipt, ExternalControlRequest, ExternalControlStatus,
        ExternalEndpoint, ExternalEndpointTransport, ExternalInteractionId,
        ExternalInteractionKind, ExternalInteractionRecord, ExternalInteractionStatus,
        ExternalProcessOwnership, ExternalRuntimeCompatibilityProbeOutcome,
        ExternalRuntimeCompatibilityProbeReport, ExternalRuntimeCompatibilityProbeStep,
        ExternalRuntimeCompatibilityProbeStepStatus, ExternalRuntimeCompatibilityState,
        ExternalRuntimeDesiredState, ExternalRuntimeKind, ExternalRuntimeObservedState,
        ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnTerminalError, SessionHandle,
        SessionKind, SessionState, SessionStatus, ToolProfile, TurnInputProvenance,
        TurnInputProvenanceKind,
    };
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn sqlite_external_turn_pages_remain_bounded_and_stable_at_ten_thousand_turns() {
        let path = temp_db_path("turn-page-10k");
        let store = CoordinationStore::open_file(&path).unwrap();
        store
            .save_session(&session("agent-a", "session-a"))
            .unwrap();
        store
            .put_external_runtime_registration(&runtime(), None)
            .unwrap();
        store.put_external_agent_binding(&binding(), None).unwrap();

        let mut conn = store.conn().unwrap();
        let tx = conn.transaction().unwrap();
        for index in 0..10_000_u32 {
            let mut record = turn();
            record.request.request_id = ExternalTurnRequestId::new(format!("request-{index:05}"));
            record.request.idempotency_key = format!("idempotency-{index:05}");
            record.request.created_at = "2026-07-10T00:00:00Z".into();
            record.native_turn_id = Some(format!("native-turn-{index:05}"));
            tx.execute(
                "INSERT INTO external_turns
                    (request_id, idempotency_key, runtime_id, binding_id, session_id,
                     native_thread_id, native_turn_id, phase, revision, created_at,
                     creation_ordinal, updated_at, record_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    record.request.request_id.0,
                    record.request.idempotency_key,
                    record.runtime_id.0,
                    record.request.binding_id.0,
                    record.request.session_id.0,
                    record.native_thread_id,
                    record.native_turn_id,
                    enum_json(&record.phase).unwrap(),
                    record.revision as i64,
                    record.request.created_at,
                    i64::from(index + 1),
                    record.updated_at,
                    to_json_text(&record).unwrap(),
                ],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        drop(conn);

        let started = Instant::now();
        let recent = store
            .query_external_turn_page(&rusty_crew_core_protocol::ExternalTurnPageQuery {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                native_thread_id: "native-thread-a".into(),
                before: None,
                limit: 50,
            })
            .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(recent.items.len(), 50);
        assert!(recent.has_more_before);
        assert_eq!(recent.items[0].turn.request.request_id.0, "request-09950");
        assert_eq!(recent.items[49].turn.request.request_id.0, "request-09999");
        assert!(serde_json::to_vec(&recent).unwrap().len() < 256 * 1024);
        assert!(elapsed.as_millis() < 250, "10k page query took {elapsed:?}");

        let before = recent.items[0].cursor.clone();
        let older = store
            .query_external_turn_page(&rusty_crew_core_protocol::ExternalTurnPageQuery {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                native_thread_id: "native-thread-a".into(),
                before: Some(before.clone()),
                limit: 50,
            })
            .unwrap();
        assert_eq!(older.items[0].turn.request.request_id.0, "request-09900");
        assert_eq!(older.items[49].turn.request.request_id.0, "request-09949");
        assert!(older.items.iter().all(|item| !recent.items.contains(item)));

        for (suffix, request_id, created_at) in [
            ("lower", "request-00000-later", "2026-07-10T00:00:00Z"),
            ("higher", "zzzz-later", "2026-07-10T00:00:00Z"),
            ("offset", "adjacent-offset", "2026-07-10T00:00:00+00:00"),
            ("fraction", "adjacent-fraction", "2026-07-10T00:00:00.000Z"),
        ] {
            let mut record = turn();
            record.request.request_id = ExternalTurnRequestId::new(request_id);
            record.request.idempotency_key = format!("later-{suffix}");
            record.request.created_at = created_at.into();
            record.native_turn_id = Some(format!("native-turn-later-{suffix}"));
            store.create_external_turn(&record).unwrap();
        }
        let replayed_older = store
            .query_external_turn_page(&rusty_crew_core_protocol::ExternalTurnPageQuery {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                native_thread_id: "native-thread-a".into(),
                before: Some(before),
                limit: 50,
            })
            .unwrap();
        assert_eq!(replayed_older, older);

        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sqlite_external_runtime_lease_turn_and_restart_contract() {
        let path = temp_db_path("lifecycle");
        let store = CoordinationStore::open_file(&path).unwrap();
        store
            .save_session(&session("agent-a", "session-a"))
            .unwrap();
        let mut runtime_write = runtime();
        runtime_write.observed_cli_version = Some("0.200.0".into());
        runtime_write.consumed_contract_revision = Some("contract-v1".into());
        runtime_write.compatibility_state =
            ExternalRuntimeCompatibilityState::CompatibleUncertified;
        runtime_write.observed_state = ExternalRuntimeObservedState::Ready;
        runtime_write.last_compatibility_probe = Some(ExternalRuntimeCompatibilityProbeReport {
            suite_revision: "codex-required-capabilities-v1".into(),
            outcome: ExternalRuntimeCompatibilityProbeOutcome::Passed,
            steps: vec![ExternalRuntimeCompatibilityProbeStep {
                step_id: "model_list".into(),
                status: ExternalRuntimeCompatibilityProbeStepStatus::Passed,
                duration_ms: 3,
                reason_code: None,
                detail: None,
            }],
            completed_at: "2026-07-10T00:00:00Z".into(),
        });
        let runtime = store
            .put_external_runtime_registration(&runtime_write, None)
            .unwrap();
        assert_eq!(runtime.revision, 1);

        let lease_a = store
            .acquire_external_controller_lease(
                &lease("controller-a", "2026-07-10T00:10:00Z"),
                &"2026-07-10T00:00:00Z".into(),
            )
            .unwrap();
        assert_eq!(lease_a.generation, 1);
        assert!(store
            .acquire_external_controller_lease(
                &lease("controller-b", "2026-07-10T00:20:00Z"),
                &"2026-07-10T00:05:00Z".into(),
            )
            .is_err());
        assert!(store
            .release_external_controller_lease(
                &ExternalRuntimeId::new("codex-local"),
                "controller-a",
                99,
                &"2026-07-10T00:05:00Z".into(),
            )
            .is_err());
        store
            .release_external_controller_lease(
                &ExternalRuntimeId::new("codex-local"),
                "controller-a",
                lease_a.generation,
                &"2026-07-10T00:05:00Z".into(),
            )
            .unwrap();
        let lease_b = store
            .acquire_external_controller_lease(
                &lease("controller-b", "2026-07-10T00:30:00Z"),
                &"2026-07-10T00:05:00Z".into(),
            )
            .unwrap();
        assert_eq!(lease_b.generation, 2);

        let binding = store.put_external_agent_binding(&binding(), None).unwrap();
        assert!(binding.is_routable());
        assert_eq!(
            store
                .get_external_binding_for_agent(&AgentId::new("agent-a"))
                .unwrap()
                .unwrap()
                .binding_id,
            binding.binding_id
        );

        let turn = turn();
        assert_eq!(store.create_external_turn(&turn).unwrap(), turn);
        assert_eq!(store.create_external_turn(&turn).unwrap(), turn);
        let mut active = turn.clone();
        active.phase = ExternalTurnPhase::Starting;
        active.updated_at = "2026-07-10T00:01:00Z".into();
        let active = store.update_external_turn(&active, 1).unwrap();
        let mut active_with_native = active.clone();
        active_with_native.phase = ExternalTurnPhase::Active;
        active_with_native.native_turn_id = Some("native-turn-a".into());
        active_with_native.updated_at = "2026-07-10T00:02:00Z".into();
        let active = store
            .update_external_turn(&active_with_native, active.revision)
            .unwrap();
        let mut completed = active.clone();
        completed.phase = ExternalTurnPhase::Completed;
        completed.capacity_lease_id = None;
        completed.terminal_error = Some(ExternalTurnTerminalError {
            message: "stream closed".into(),
            code: Some("responseStreamDisconnected".into()),
            additional_details: None,
            will_retry: Some(false),
        });
        completed.updated_at = "2026-07-10T00:03:00Z".into();
        let completed = store
            .update_external_turn(&completed, active.revision)
            .unwrap();
        let mut resurrected = completed.clone();
        resurrected.phase = ExternalTurnPhase::Active;
        assert!(store
            .update_external_turn(&resurrected, completed.revision)
            .is_err());
        drop(store);

        let reopened = CoordinationStore::open_file(&path).unwrap();
        assert_eq!(
            reopened
                .get_external_turn(&ExternalTurnRequestId::new("request-a"))
                .unwrap()
                .unwrap()
                .phase,
            ExternalTurnPhase::Completed
        );
        assert_eq!(
            reopened
                .list_external_turns_for_native_thread(
                    &ExternalRuntimeId::new("codex-local"),
                    "native-thread-a",
                )
                .unwrap(),
            vec![completed]
        );
        assert!(reopened
            .list_nonterminal_external_turns()
            .unwrap()
            .is_empty());
        assert_eq!(
            reopened
                .get_external_runtime_registration(&ExternalRuntimeId::new("codex-local"))
                .unwrap()
                .unwrap()
                .last_compatibility_probe
                .unwrap()
                .outcome,
            ExternalRuntimeCompatibilityProbeOutcome::Passed
        );
        remove_temp_db(&path);
    }

    #[test]
    fn sqlite_agent_rounds_are_idempotent_and_terminal() {
        let path = temp_db_path("rounds");
        let store = CoordinationStore::open_file(&path).unwrap();
        store
            .save_session(&session("agent-a", "session-a"))
            .unwrap();
        store
            .save_session(&session("agent-b", "session-b"))
            .unwrap();
        let round = AgentCorrelatedRound {
            round_id: AgentRoundId::new("round-a"),
            idempotency_key: "round-key-a".into(),
            sender_agent_id: AgentId::new("agent-a"),
            sender_session_id: Some(SessionId::new("session-a")),
            recipient_agent_id: AgentId::new("agent-b"),
            recipient_session_id: SessionId::new("session-b"),
            sender_request_id: None,
            message_id: "message-a".into(),
            correlation_id: "correlation-a".into(),
            reply_message_id: None,
            status: AgentRoundStatus::Pending,
            outcome: None,
            terminal_reason_code: None,
            created_at: "2026-07-10T00:00:00Z".into(),
            expires_at: "2026-07-10T00:10:00Z".into(),
            terminal_at: None,
            revision: 1,
        };
        assert_eq!(store.create_agent_correlated_round(&round).unwrap(), round);
        assert_eq!(store.create_agent_correlated_round(&round).unwrap(), round);
        drop(store);
        let store = CoordinationStore::open_file(&path).unwrap();
        assert_eq!(
            store.list_pending_agent_rounds().unwrap(),
            vec![round.clone()]
        );
        let mut replied = round.clone();
        replied.status = AgentRoundStatus::Replied;
        replied.reply_message_id = Some("message-b".into());
        replied.terminal_at = Some("2026-07-10T00:01:00Z".into());
        let replied = store.update_agent_correlated_round(&replied, 1).unwrap();
        let mut late = replied.clone();
        late.reply_message_id = Some("message-late".into());
        assert!(store
            .update_agent_correlated_round(&late, replied.revision)
            .is_err());
        assert!(store.list_pending_agent_rounds().unwrap().is_empty());
        remove_temp_db(&path);
    }

    #[test]
    fn sqlite_external_controls_interactions_and_events_are_replay_safe() {
        let path = temp_db_path("control-events");
        let store = CoordinationStore::open_file(&path).unwrap();
        store
            .save_session(&session("agent-a", "session-a"))
            .unwrap();
        store
            .put_external_runtime_registration(&runtime(), None)
            .unwrap();
        store.put_external_agent_binding(&binding(), None).unwrap();
        store.create_external_turn(&turn()).unwrap();

        let control = ExternalControlReceipt {
            request: ExternalControlRequest {
                control_id: ExternalControlId::new("control-a"),
                idempotency_key: "control-key-a".into(),
                binding_id: ExternalBindingId::new("binding-a"),
                expected_binding_revision: 1,
                expected_native_turn_id: None,
                kind: ExternalControlKind::StartTurn,
                payload: json!({"requestId": "request-a"}),
                requested_at: "2026-07-10T00:00:00Z".into(),
            },
            request_fingerprint: "control-fingerprint-a".into(),
            status: ExternalControlStatus::Pending,
            outcome: None,
            reason_code: None,
            revision: 1,
            updated_at: "2026-07-10T00:00:00Z".into(),
        };
        assert_eq!(
            store.put_external_control_receipt(&control).unwrap(),
            control
        );
        let mut applied = control.clone();
        applied.status = ExternalControlStatus::Applied;
        applied.outcome = Some(json!({"nativeTurnId": "native-turn-a"}));
        applied.updated_at = "2026-07-10T00:00:01Z".into();
        let applied = store.update_external_control_receipt(&applied, 1).unwrap();
        assert_eq!(
            store
                .update_external_control_receipt(&applied, applied.revision)
                .unwrap(),
            applied
        );
        let mut changed = applied.clone();
        changed.outcome = Some(json!({"nativeTurnId": "different"}));
        assert!(store
            .update_external_control_receipt(&changed, applied.revision)
            .is_err());

        let interaction = ExternalInteractionRecord {
            interaction_id: ExternalInteractionId::new("interaction-a"),
            runtime_id: ExternalRuntimeId::new("codex-local"),
            binding_id: ExternalBindingId::new("binding-a"),
            request_id: ExternalTurnRequestId::new("request-a"),
            native_thread_id: "native-thread-a".into(),
            native_turn_id: "native-turn-a".into(),
            native_request_id: "native-request-a".into(),
            kind: ExternalInteractionKind::RequestUserInput,
            prompt: json!({"question": "continue?"}),
            allowed_responses: vec!["continue".into()],
            status: ExternalInteractionStatus::Pending,
            resolution_idempotency_key: None,
            outcome: None,
            raw_detail_ref: None,
            requested_at: "2026-07-10T00:00:00Z".into(),
            expires_at: "2026-07-10T00:10:00Z".into(),
            resolved_at: None,
            revision: 1,
        };
        assert_eq!(
            store.put_external_interaction(&interaction).unwrap(),
            interaction
        );
        assert_eq!(
            store.put_external_interaction(&interaction).unwrap(),
            interaction
        );
        let mut expired = interaction.clone();
        expired.status = ExternalInteractionStatus::Expired;
        expired.resolved_at = Some("2026-07-10T00:11:00Z".into());
        let expired = store.update_external_interaction(&expired, 1).unwrap();
        let mut late = expired.clone();
        late.status = ExternalInteractionStatus::Resolved;
        assert!(store
            .update_external_interaction(&late, expired.revision)
            .is_err());

        let event = NormalizedExternalRuntimeEvent {
            event_id: "event-a".into(),
            session_id: Some(SessionId::new("session-a")),
            sequence_id: 1,
            created_at: "2026-07-10T00:00:00Z".into(),
            kind: "turn_started".into(),
            runtime_id: ExternalRuntimeId::new("codex-local"),
            native_thread_id: Some("native-thread-a".into()),
            native_turn_id: Some("native-turn-a".into()),
            item_id: None,
            request_id: Some("request-a".into()),
            payload: json!({"phase": "active"}),
            raw_detail_ref: None,
        };
        store.append_external_runtime_event(&event).unwrap();
        store.append_external_runtime_event(&event).unwrap();
        let mut conflicting_event = event.clone();
        conflicting_event.payload = json!({"phase": "different"});
        assert!(store
            .append_external_runtime_event(&conflicting_event)
            .is_err());
        for sequence_id in [2, 3] {
            let mut later_event = event.clone();
            later_event.event_id = format!("event-{sequence_id}");
            later_event.sequence_id = sequence_id;
            store.append_external_runtime_event(&later_event).unwrap();
        }
        assert_eq!(
            store
                .query_external_runtime_events(&ExternalRuntimeId::new("codex-local"), 0, 10)
                .unwrap(),
            vec![
                event.clone(),
                {
                    let mut later = event.clone();
                    later.event_id = "event-2".into();
                    later.sequence_id = 2;
                    later
                },
                {
                    let mut later = event.clone();
                    later.event_id = "event-3".into();
                    later.sequence_id = 3;
                    later
                },
            ]
        );
        assert_eq!(
            store
                .query_external_runtime_event_tail(&ExternalRuntimeId::new("codex-local"), 2)
                .unwrap()
                .into_iter()
                .map(|event| event.sequence_id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(
            store
                .query_external_runtime_thread_events(
                    &ExternalRuntimeId::new("codex-local"),
                    "native-thread-a",
                    1,
                    1,
                )
                .unwrap()
                .into_iter()
                .map(|event| event.sequence_id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(store
            .query_external_runtime_thread_events(
                &ExternalRuntimeId::new("codex-local"),
                "native-thread-missing",
                0,
                10,
            )
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .query_external_runtime_turn_events(
                    &ExternalRuntimeId::new("codex-local"),
                    "native-turn-a",
                    0,
                    10,
                )
                .unwrap()
                .into_iter()
                .map(|event| event.sequence_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let plan = {
            let conn = store.conn().unwrap();
            let mut stmt = conn
                .prepare(
                    "EXPLAIN QUERY PLAN
                     SELECT record_json FROM external_runtime_events
                     WHERE runtime_id = ?1
                       AND native_thread_id = ?2
                       AND sequence_id > ?3
                     ORDER BY sequence_id LIMIT ?4",
                )
                .unwrap();
            stmt.query_map(
                params!["codex-local", "native-thread-a", 0_i64, 100_i64],
                |row| row.get::<_, String>(3),
            )
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join(" | ")
        };
        assert!(
            plan.contains("external_runtime_events_thread_cursor_idx"),
            "thread replay must use its composite cursor index: {plan}"
        );
        remove_temp_db(&path);
    }

    #[test]
    fn sqlite_external_event_retention_preserves_active_turns_and_monotonic_cursors() {
        let path = temp_db_path("event-retention");
        let store = CoordinationStore::open_file_with_diagnostics(&path, Some(100)).unwrap();
        store
            .save_session(&session("agent-a", "session-a"))
            .unwrap();
        store
            .put_external_runtime_registration(&runtime(), None)
            .unwrap();
        store.put_external_agent_binding(&binding(), None).unwrap();
        let accepted = store.create_external_turn(&turn()).unwrap();
        let mut starting = accepted.clone();
        starting.phase = ExternalTurnPhase::Starting;
        starting.updated_at = "2026-07-10T00:01:00Z".into();
        let starting = store
            .update_external_turn(&starting, accepted.revision)
            .unwrap();
        let mut active = starting.clone();
        active.phase = ExternalTurnPhase::Active;
        active.native_turn_id = Some("native-turn-a".into());
        active.updated_at = "2026-07-10T00:02:00Z".into();
        let active = store
            .update_external_turn(&active, starting.revision)
            .unwrap();

        for (event_id, kind) in [
            ("event-delta", "assistant_text_delta"),
            ("event-command", "command_activity"),
            ("event-lifecycle", "turn_lifecycle"),
        ] {
            store
                .append_external_runtime_event_allocated(&external_event_input(event_id, kind))
                .unwrap();
        }
        let active_report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                compact_terminal_external_runtime_events_before: Some(
                    "2026-07-11T00:00:00Z".into(),
                ),
                external_runtime_event_retention_at: Some("2026-07-12T00:00:00Z".into()),
                external_runtime_event_terminal_turn_batch_size: Some(10),
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        assert_eq!(
            active_report
                .external_runtime_event_retention
                .events_deleted,
            0
        );

        let mut completed = active.clone();
        completed.phase = ExternalTurnPhase::Completed;
        completed.capacity_lease_id = None;
        completed.updated_at = "2026-07-10T00:03:00Z".into();
        store
            .update_external_turn(&completed, active.revision)
            .unwrap();
        assert!(store
            .run_maintenance(&RuntimeMaintenancePolicy {
                compact_terminal_external_runtime_events_before: Some(
                    "2026-07-11T00:00:00Z".into(),
                ),
                ..RuntimeMaintenancePolicy::default()
            })
            .is_err());
        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                compact_terminal_external_runtime_events_before: Some(
                    "2026-07-11T00:00:00Z".into(),
                ),
                external_runtime_event_retention_at: Some("2026-07-12T00:00:00Z".into()),
                external_runtime_event_terminal_turn_batch_size: Some(10),
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        assert_eq!(
            report
                .external_runtime_event_retention
                .terminal_turns_compacted,
            1
        );
        assert_eq!(report.external_runtime_event_retention.events_deleted, 2);
        assert!(
            report
                .external_runtime_event_retention
                .estimated_reclaimed_bytes
                > 0
        );
        let retained = store
            .query_external_runtime_events(&ExternalRuntimeId::new("codex-local"), 0, 10)
            .unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].kind, "turn_lifecycle");
        assert_eq!(retained[0].sequence_id, 3);
        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.external_runtime_events.event_rows, 1);
        assert_eq!(diagnostics.external_runtime_events.checkpoint_rows, 1);
        assert!(diagnostics.filesystem_headroom.available);
        assert!(diagnostics.filesystem_headroom.warning_active);

        let after_compaction = store
            .append_external_runtime_event_allocated(&external_event_input(
                "event-after-compaction",
                "runtime_status",
            ))
            .unwrap();
        assert_eq!(after_compaction.sequence_id, 4);
        drop(store);
        let reopened = CoordinationStore::open_file(&path).unwrap();
        let after_restart = reopened
            .append_external_runtime_event_allocated(&external_event_input(
                "event-after-restart",
                "runtime_status",
            ))
            .unwrap();
        assert_eq!(after_restart.sequence_id, 5);
        assert_eq!(
            reopened
                .query_external_runtime_events(&ExternalRuntimeId::new("codex-local"), 3, 10)
                .unwrap()
                .iter()
                .map(|event| event.sequence_id)
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
        remove_temp_db(&path);
    }

    fn external_event_input(event_id: &str, kind: &str) -> ExternalRuntimeEventInput {
        ExternalRuntimeEventInput {
            event_id: event_id.into(),
            session_id: Some(SessionId::new("session-a")),
            created_at: "2026-07-10T00:02:30Z".into(),
            kind: kind.into(),
            runtime_id: ExternalRuntimeId::new("codex-local"),
            native_thread_id: Some("native-thread-a".into()),
            native_turn_id: Some("native-turn-a".into()),
            item_id: None,
            request_id: Some("request-a".into()),
            payload: json!({"value": "retention-proof"}),
            raw_detail_ref: None,
        }
    }

    fn runtime() -> ExternalRuntimeRegistration {
        ExternalRuntimeRegistration {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            kind: ExternalRuntimeKind::CodexAppServer,
            endpoint: ExternalEndpoint {
                transport: ExternalEndpointTransport::UnixWebSocket,
                address: "/run/user/1001/codex.sock".into(),
            },
            process_ownership: ExternalProcessOwnership::Attached,
            codex_home_ref: Some("/home/agent/.codex".into()),
            observed_cli_version: None,
            consumed_contract_revision: None,
            compatibility_state: ExternalRuntimeCompatibilityState::Unassessed,
            last_compatibility_probe: None,
            desired_state: ExternalRuntimeDesiredState::Enabled,
            observed_state: ExternalRuntimeObservedState::Disconnected,
            observed_reason_code: None,
            revision: 0,
            created_at: "2026-07-10T00:00:00Z".into(),
            updated_at: "2026-07-10T00:00:00Z".into(),
        }
    }

    fn lease(holder: &str, expires_at: &str) -> ExternalControllerLease {
        ExternalControllerLease {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            holder_instance_id: holder.into(),
            generation: 0,
            acquired_at: "2026-07-10T00:00:00Z".into(),
            renewed_at: "2026-07-10T00:00:00Z".into(),
            expires_at: expires_at.into(),
            revision: 0,
        }
    }

    fn session(agent_id: &str, session_id: &str) -> SessionState {
        SessionState {
            handle: SessionHandle::new(if session_id.ends_with('a') { 1 } else { 2 }),
            session_id: SessionId::new(session_id),
            agent_id: AgentId::new(agent_id),
            profile_id: ProfileId::new(format!("{agent_id}-profile")),
            kind: SessionKind::Full,
            delegation: None,
            workspace: None,
            resource_limits: ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
            inference_overrides: Default::default(),
            status: SessionStatus::Idle,
            brain_turn_count: 0,
            created_at: "2026-07-10T00:00:00Z".into(),
            last_active_at: "2026-07-10T00:00:00Z".into(),
        }
    }

    fn binding() -> ExternalAgentBinding {
        ExternalAgentBinding {
            binding_id: ExternalBindingId::new("binding-a"),
            runtime_id: ExternalRuntimeId::new("codex-local"),
            session_id: Some(SessionId::new("session-a")),
            agent_id: Some(AgentId::new("agent-a")),
            profile_id: Some(ProfileId::new("profile-a")),
            profile_revision: Some(1),
            profile_prompt_hash: Some("profile-prompt-hash".into()),
            profile_prompt_snapshot: Some("profile prompt".into()),
            dynamic_tool_catalog_fingerprint: None,
            message_delivery_policy:
                rusty_crew_core_protocol::ExternalMessageDeliveryPolicy::ImmediateSteer,
            purpose: ExternalBindingPurpose::CrewAgent,
            native_thread_id: Some("native-thread-a".into()),
            cwd: Some("/home/dev/rusty-crew".into()),
            label: None,
            task_ref: None,
            lineage: None,
            effective_config_fingerprint: "config-a".into(),
            status: ExternalBindingStatus::Active,
            revision: 0,
            created_at: "2026-07-10T00:00:00Z".into(),
            updated_at: "2026-07-10T00:00:00Z".into(),
        }
    }

    fn turn() -> ExternalTurnCorrelation {
        ExternalTurnCorrelation {
            request: rusty_crew_core_protocol::SessionTurnRequested {
                request_id: ExternalTurnRequestId::new("request-a"),
                idempotency_key: "turn-key-a".into(),
                session_id: SessionId::new("session-a"),
                run_id: None,
                binding_id: ExternalBindingId::new("binding-a"),
                input: vec![ExternalTurnInputPart::Text {
                    text: "inspect the repository".into(),
                }],
                collaboration_mode: None,
                provenance: TurnInputProvenance {
                    kind: TurnInputProvenanceKind::Operator,
                    source_id: None,
                    correlation_id: None,
                },
                created_at: "2026-07-10T00:00:00Z".into(),
                expires_at: None,
            },
            runtime_id: ExternalRuntimeId::new("codex-local"),
            native_thread_id: "native-thread-a".into(),
            native_turn_id: None,
            task_ref: None,
            phase: ExternalTurnPhase::Accepted,
            capacity_lease_id: Some("capacity-a".into()),
            terminal_reason_code: None,
            terminal_error: None,
            revision: 1,
            updated_at: "2026-07-10T00:00:00Z".into(),
        }
    }

    fn temp_db_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rusty-crew-external-runtime-{label}-{}-{nonce}.sqlite3",
            std::process::id()
        ))
    }

    fn remove_temp_db(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }
}
