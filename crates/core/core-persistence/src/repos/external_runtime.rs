//! SQLite repository for Rust-owned external-agent runtime lifecycle state.

use super::super::*;
use rusty_crew_core_protocol::{
    validate_external_runtime_certification_invalidation,
    validate_external_runtime_certification_record, validate_external_runtime_probe_evidence,
    validate_external_runtime_registration, validate_external_turn_transition,
    AgentCorrelatedRound, AgentId, AgentMessageDeliveryReceipt, AgentMessageDeliveryStatus,
    AgentRoundStatus, ExternalAgentBinding, ExternalAgentSessionCreationId,
    ExternalAgentSessionCreationRecord, ExternalBindingId, ExternalControlId,
    ExternalControlReceipt, ExternalControllerLease, ExternalInteractionRecord,
    ExternalInteractionStatus, ExternalRuntimeCertificationInvalidation,
    ExternalRuntimeCertificationRecord, ExternalRuntimeCertificationStatus, ExternalRuntimeId,
    ExternalRuntimeProbeEvidenceRecord, ExternalRuntimeRegistration, ExternalTurnCorrelation,
    ExternalTurnRequestId, NormalizedExternalRuntimeEvent,
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
        validate_expected_revision(
            "external binding",
            &record.binding_id.0,
            current.as_ref().map(|value| value.revision),
            expected_revision,
        )?;
        let mut saved = record.clone();
        saved.revision = current.map(|value| value.revision + 1).unwrap_or(1);
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
                 native_thread_id, native_turn_id, phase, revision, updated_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| persistence_error("save external turn", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit external turn", error))?;
        Ok(record.clone())
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
                (event_id, runtime_id, session_id, sequence_id, kind, created_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.event_id,
                event.runtime_id.0,
                event.session_id.as_ref().map(|value| value.0.as_str()),
                event.sequence_id as i64,
                event.kind,
                event.created_at,
                to_json_text(event)?,
            ],
        )
        .map_err(|error| persistence_error("append external runtime event", error))?;
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
        let next_sequence = tx
            .query_row(
                "SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM external_runtime_events
                 WHERE runtime_id = ?1",
                params![input.runtime_id.0.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| persistence_error("allocate external runtime event sequence", error))?
            as u64;
        let event = normalized_event_from_input(input, next_sequence);
        tx.execute(
            "INSERT INTO external_runtime_events
                (event_id, runtime_id, session_id, sequence_id, kind, created_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.event_id,
                event.runtime_id.0,
                event.session_id.as_ref().map(|value| value.0.as_str()),
                event.sequence_id as i64,
                event.kind,
                event.created_at,
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
                (delivery_id, idempotency_key, message_id, from_agent_id, to_agent_id,
                 status, expires_at, revision, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.request.delivery_id.0,
                record.request.idempotency_key,
                record.request.message_id,
                record.request.from_agent_id.0,
                record.request.to_agent_id.0,
                enum_json(&record.status)?,
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
        if next.status == AgentMessageDeliveryStatus::Pending {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent message delivery must transition to a terminal status",
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
        ExternalTurnInputPart, ExternalTurnPhase, SessionHandle, SessionKind, SessionState,
        SessionStatus, ToolProfile, TurnInputProvenance, TurnInputProvenanceKind,
    };
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert_eq!(
            store
                .query_external_runtime_events(&ExternalRuntimeId::new("codex-local"), 0, 10)
                .unwrap(),
            vec![event]
        );
        remove_temp_db(&path);
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
            resource_limits: ResourceLimits {
                workdir: None,
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
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
            purpose: ExternalBindingPurpose::CrewAgent,
            native_thread_id: Some("native-thread-a".into()),
            cwd: Some("/home/dev/rusty-crew".into()),
            label: None,
            task_ref: None,
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
