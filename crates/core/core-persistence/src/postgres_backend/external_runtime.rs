//! PostgreSQL repository for Rust-owned external-agent runtime lifecycle state.

use super::*;
use rusty_crew_core_protocol::{
    validate_external_runtime_certification_invalidation,
    validate_external_runtime_certification_record, validate_external_runtime_probe_evidence,
    validate_external_runtime_registration, validate_external_turn_transition, AgentActivation,
    AgentCorrelatedRound, AgentId, AgentMessageDeliveryReceipt, AgentMessageDeliveryStatus,
    AgentRoundStatus, ExternalAgentBinding, ExternalAgentSessionCreationId,
    ExternalAgentSessionCreationPhase, ExternalAgentSessionCreationRecord, ExternalBindingId,
    ExternalControlId, ExternalControlReceipt, ExternalControllerLease, ExternalInteractionRecord,
    ExternalInteractionStatus, ExternalRuntimeCertificationInvalidation,
    ExternalRuntimeCertificationRecord, ExternalRuntimeCertificationStatus,
    ExternalRuntimeEventInput, ExternalRuntimeId, ExternalRuntimeProbeEvidenceRecord,
    ExternalRuntimeRegistration, ExternalTurnCorrelation, ExternalTurnRequestId,
    NormalizedExternalRuntimeEvent,
};

impl PostgresBackendStore {
    pub fn put_external_runtime_registration(
        &self,
        record: &ExternalRuntimeRegistration,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalRuntimeRegistration> {
        validate_external_runtime_registration(record)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL external runtime registration", error)
        })?;
        let current = load_optional::<ExternalRuntimeRegistration>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_registrations
                 WHERE runtime_id = $1 FOR UPDATE"
            ),
            &[&record.runtime_id.0],
            "load PostgreSQL external runtime registration",
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
            &format!(
                "INSERT INTO {schema}.external_runtime_registrations
                    (runtime_id, observed_state, revision, record_json)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT(runtime_id) DO UPDATE SET
                    observed_state = EXCLUDED.observed_state,
                    revision = EXCLUDED.revision,
                    record_json = EXCLUDED.record_json"
            ),
            &[
                &saved.runtime_id.0,
                &enum_json(&saved.observed_state)?,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external runtime", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external runtime", error))?;
        Ok(saved)
    }

    pub fn get_external_runtime_registration(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeRegistration>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_registrations WHERE runtime_id = $1"
            ),
            &[&runtime_id.0],
            "load PostgreSQL external runtime registration",
        )
    }

    pub fn list_external_runtime_registrations(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeRegistration>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_registrations ORDER BY runtime_id"
            ),
            &[],
            "list PostgreSQL external runtimes",
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
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL external runtime certification", error)
        })?;
        let by_id = load_optional::<ExternalRuntimeCertificationRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 WHERE certification_id = $1 FOR UPDATE"
            ),
            &[&record.certification_id],
            "load PostgreSQL certification by identifier",
        )?;
        let by_key = load_optional::<ExternalRuntimeCertificationRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&record.idempotency_key],
            "load PostgreSQL certification by idempotency key",
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
        let active = load_list::<ExternalRuntimeCertificationRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 WHERE runtime_kind = $1
                   AND observed_cli_version = $2
                   AND consumed_contract_revision = $3
                   AND probe_suite_revision = $4
                   AND status = 'active'
                 FOR UPDATE"
            ),
            &[
                &enum_json(&record.runtime_kind)?,
                &record.observed_cli_version,
                &record.consumed_contract_revision,
                &record.probe_suite_revision,
            ],
            "load active PostgreSQL runtime certifications",
        )?;
        for mut previous in active {
            previous.status = ExternalRuntimeCertificationStatus::Superseded;
            previous.superseded_by_certification_id = Some(record.certification_id.clone());
            previous.revision += 1;
            previous.updated_at = record.created_at.clone();
            validate_external_runtime_certification_record(&previous)?;
            tx.execute(
                &format!(
                    "UPDATE {schema}.external_runtime_certifications
                     SET status = 'superseded', revision = $2, record_json = $3
                     WHERE certification_id = $1"
                ),
                &[
                    &previous.certification_id,
                    &(previous.revision as i64),
                    &to_json_text(&previous)?,
                ],
            )
            .map_err(|error| postgres_error("supersede PostgreSQL runtime certification", error))?;
        }
        let mut saved = record.clone();
        saved.revision = 1;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_runtime_certifications (
                    certification_id, idempotency_key, runtime_kind,
                    observed_cli_version, consumed_contract_revision,
                    probe_suite_revision, status, revision, record_json
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8)"
            ),
            &[
                &saved.certification_id,
                &saved.idempotency_key,
                &enum_json(&saved.runtime_kind)?,
                &saved.observed_cli_version,
                &saved.consumed_contract_revision,
                &saved.probe_suite_revision,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
            ],
        )
        .map_err(|error| postgres_error("insert PostgreSQL runtime certification", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL external runtime certification", error)
        })?;
        Ok(saved)
    }

    pub fn put_external_runtime_probe_evidence(
        &self,
        evidence: &ExternalRuntimeProbeEvidenceRecord,
    ) -> CoreResult<()> {
        validate_external_runtime_probe_evidence(evidence)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.external_runtime_probe_evidence (
                        runtime_id, observed_cli_version, consumed_contract_revision,
                        probe_suite_revision, record_json
                     ) VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT(runtime_id) DO UPDATE SET
                        observed_cli_version = EXCLUDED.observed_cli_version,
                        consumed_contract_revision = EXCLUDED.consumed_contract_revision,
                        probe_suite_revision = EXCLUDED.probe_suite_revision,
                        record_json = EXCLUDED.record_json"
                ),
                &[
                    &evidence.runtime_id.0,
                    &evidence.observed_cli_version,
                    &evidence.consumed_contract_revision,
                    &evidence.probe_report.suite_revision,
                    &to_json_text(evidence)?,
                ],
            )
            .map_err(|error| postgres_error("save PostgreSQL runtime probe evidence", error))?;
        Ok(())
    }

    pub fn get_external_runtime_probe_evidence(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeProbeEvidenceRecord>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_probe_evidence
                 WHERE runtime_id = $1"
            ),
            &[&runtime_id.0],
            "load PostgreSQL runtime probe evidence",
        )
    }

    pub fn get_external_runtime_certification(
        &self,
        certification_id: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 WHERE certification_id = $1"
            ),
            &[&certification_id],
            "load PostgreSQL external runtime certification",
        )
    }

    pub fn list_external_runtime_certifications(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeCertificationRecord>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 ORDER BY certification_id"
            ),
            &[],
            "list PostgreSQL external runtime certifications",
        )
    }

    pub fn find_active_external_runtime_certification(
        &self,
        runtime_kind: &rusty_crew_core_protocol::ExternalRuntimeKind,
        observed_cli_version: &str,
        consumed_contract_revision: &str,
        probe_suite_revision: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 WHERE runtime_kind = $1
                   AND observed_cli_version = $2
                   AND consumed_contract_revision = $3
                   AND probe_suite_revision = $4
                   AND status = 'active'
                 ORDER BY certification_id DESC LIMIT 1"
            ),
            &[
                &enum_json(runtime_kind)?,
                &observed_cli_version,
                &consumed_contract_revision,
                &probe_suite_revision,
            ],
            "find active PostgreSQL runtime certification",
        )
    }

    pub fn invalidate_external_runtime_certification(
        &self,
        invalidation: &ExternalRuntimeCertificationInvalidation,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        validate_external_runtime_certification_invalidation(invalidation)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL certification invalidation", error)
        })?;
        let mut current = load_optional::<ExternalRuntimeCertificationRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_certifications
                 WHERE certification_id = $1 FOR UPDATE"
            ),
            &[&invalidation.certification_id],
            "load PostgreSQL certification for invalidation",
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
            &format!(
                "UPDATE {schema}.external_runtime_certifications
                 SET status = 'invalidated', revision = $2, record_json = $3
                 WHERE certification_id = $1"
            ),
            &[
                &current.certification_id,
                &(current.revision as i64),
                &to_json_text(&current)?,
            ],
        )
        .map_err(|error| postgres_error("invalidate PostgreSQL certification", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL certification invalidation", error)
        })?;
        Ok(current)
    }

    pub fn acquire_external_controller_lease(
        &self,
        candidate: &ExternalControllerLease,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL external controller lease", error))?;
        let current = load_optional::<ExternalControllerLease>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_controller_leases
                 WHERE runtime_id = $1 FOR UPDATE"
            ),
            &[&candidate.runtime_id.0],
            "load PostgreSQL external controller lease",
        )?;
        if let Some(current) = &current {
            if current.holder_instance_id != candidate.holder_instance_id
                && current.expires_at > *now
            {
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
            &format!(
                "INSERT INTO {schema}.external_controller_leases
                    (runtime_id, holder_instance_id, generation, expires_at, revision, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(runtime_id) DO UPDATE SET
                    holder_instance_id = EXCLUDED.holder_instance_id,
                    generation = EXCLUDED.generation,
                    expires_at = EXCLUDED.expires_at,
                    revision = EXCLUDED.revision,
                    record_json = EXCLUDED.record_json"
            ),
            &[
                &saved.runtime_id.0,
                &saved.holder_instance_id,
                &(saved.generation as i64),
                &saved.expires_at,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external controller lease", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL external controller lease", error)
        })?;
        Ok(saved)
    }

    pub fn release_external_controller_lease(
        &self,
        runtime_id: &ExternalRuntimeId,
        holder_instance_id: &str,
        generation: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start release PostgreSQL external controller lease", error)
        })?;
        let current = load_required::<ExternalControllerLease>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_controller_leases
                 WHERE runtime_id = $1 FOR UPDATE"
            ),
            &[&runtime_id.0],
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
            &format!(
                "UPDATE {schema}.external_controller_leases SET expires_at = $1,
                    revision = $2, record_json = $3
                 WHERE runtime_id = $4 AND revision = $5"
            ),
            &[
                &released.expires_at,
                &(released.revision as i64),
                &to_json_text(&released)?,
                &runtime_id.0,
                &(current.revision as i64),
            ],
        )
        .map_err(|error| postgres_error("release PostgreSQL external controller lease", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit release PostgreSQL external controller lease", error)
        })?;
        Ok(released)
    }

    pub fn get_external_controller_lease(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalControllerLease>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_controller_leases WHERE runtime_id = $1"
            ),
            &[&runtime_id.0],
            "load PostgreSQL external controller lease",
        )
    }

    pub fn put_external_agent_binding(
        &self,
        record: &ExternalAgentBinding,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalAgentBinding> {
        record.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL external binding", error))?;
        let current = load_optional::<ExternalAgentBinding>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_bindings
                 WHERE binding_id = $1 FOR UPDATE"
            ),
            &[&record.binding_id.0],
            "load PostgreSQL external binding",
        )?;
        validate_expected_revision(
            "external binding",
            &record.binding_id.0,
            current.as_ref().map(|value| value.revision),
            expected_revision,
        )?;
        let mut saved = record.clone();
        saved.revision = current.map(|value| value.revision + 1).unwrap_or(1);
        let session_id = saved.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = saved.agent_id.as_ref().map(|value| value.0.as_str());
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_agent_bindings
                    (binding_id, runtime_id, session_id, agent_id, purpose, status,
                     native_thread_id, revision, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(binding_id) DO UPDATE SET
                    runtime_id = EXCLUDED.runtime_id, session_id = EXCLUDED.session_id,
                    agent_id = EXCLUDED.agent_id, purpose = EXCLUDED.purpose,
                    status = EXCLUDED.status, native_thread_id = EXCLUDED.native_thread_id,
                    revision = EXCLUDED.revision, record_json = EXCLUDED.record_json"
            ),
            &[
                &saved.binding_id.0,
                &saved.runtime_id.0,
                &session_id,
                &agent_id,
                &enum_json(&saved.purpose)?,
                &enum_json(&saved.status)?,
                &saved.native_thread_id,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external binding", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external binding", error))?;
        Ok(saved)
    }

    pub fn get_external_binding_for_agent(
        &self,
        agent_id: &AgentId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        let schema = self.quoted_schema();
        let binding = load_optional::<ExternalAgentBinding>(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_bindings
                 WHERE agent_id = $1 AND purpose = 'crew_agent' AND status = 'active'"
            ),
            &[&agent_id.0],
            "load PostgreSQL routable external binding",
        )?;
        Ok(binding.filter(ExternalAgentBinding::is_routable))
    }

    pub fn get_external_agent_binding(
        &self,
        binding_id: &ExternalBindingId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_bindings WHERE binding_id = $1"
            ),
            &[&binding_id.0],
            "load PostgreSQL external binding",
        )
    }

    pub fn list_external_agent_bindings(&self) -> CoreResult<Vec<ExternalAgentBinding>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_bindings ORDER BY binding_id"
            ),
            &[],
            "list PostgreSQL external bindings",
        )
    }

    pub fn create_external_agent_session_creation(
        &self,
        record: &ExternalAgentSessionCreationRecord,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL external agent session creation", error)
        })?;
        let existing = load_optional::<ExternalAgentSessionCreationRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_session_creations
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&record.request.idempotency_key],
            "load PostgreSQL idempotent external agent session creation",
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
            &format!(
                "INSERT INTO {schema}.external_agent_session_creations
                    (creation_id, idempotency_key, request_fingerprint, runtime_id,
                     profile_id, session_id, binding_id, phase, native_thread_id,
                     revision, updated_at, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
            ),
            &[
                &record.creation_id.0,
                &record.request.idempotency_key,
                &record.request_fingerprint,
                &record.request.runtime_id.0,
                &record.request.profile_id.0,
                &record.session.session_id.0,
                &record.binding.binding_id.0,
                &enum_json(&record.phase)?,
                &record.native_thread_id,
                &(record.revision as i64),
                &record.updated_at,
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| {
            postgres_error("save PostgreSQL external agent session creation", error)
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL external agent session creation", error)
        })?;
        Ok(record.clone())
    }

    pub fn get_external_agent_session_creation(
        &self,
        creation_id: &ExternalAgentSessionCreationId,
    ) -> CoreResult<Option<ExternalAgentSessionCreationRecord>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_session_creations
                 WHERE creation_id = $1"
            ),
            &[&creation_id.0],
            "load PostgreSQL external agent session creation",
        )
    }

    pub fn update_external_agent_session_creation(
        &self,
        next: &ExternalAgentSessionCreationRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error(
                "start update PostgreSQL external agent session creation",
                error,
            )
        })?;
        let current = load_required::<ExternalAgentSessionCreationRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_agent_session_creations
                 WHERE creation_id = $1 FOR UPDATE"
            ),
            &[&next.creation_id.0],
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
        if current.phase == ExternalAgentSessionCreationPhase::Ready {
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
            &format!(
                "UPDATE {schema}.external_agent_session_creations SET phase = $1,
                    native_thread_id = $2, revision = $3, updated_at = $4, record_json = $5
                 WHERE creation_id = $6 AND revision = $7"
            ),
            &[
                &enum_json(&saved.phase)?,
                &saved.native_thread_id,
                &(saved.revision as i64),
                &saved.updated_at,
                &to_json_text(&saved)?,
                &saved.creation_id.0,
                &(expected_revision as i64),
            ],
        )
        .map_err(|error| {
            postgres_error("update PostgreSQL external agent session creation", error)
        })?;
        tx.commit().map_err(|error| {
            postgres_error(
                "commit update PostgreSQL external agent session creation",
                error,
            )
        })?;
        Ok(saved)
    }

    pub fn create_external_turn(
        &self,
        record: &ExternalTurnCorrelation,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL external turn", error))?;
        let existing = load_optional::<ExternalTurnCorrelation>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_turns
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&record.request.idempotency_key],
            "load PostgreSQL idempotent external turn",
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
            &format!(
                "INSERT INTO {schema}.external_turns
                    (request_id, idempotency_key, runtime_id, binding_id, session_id,
                     native_thread_id, native_turn_id, phase, revision, updated_at, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
            ),
            &[
                &record.request.request_id.0,
                &record.request.idempotency_key,
                &record.runtime_id.0,
                &record.request.binding_id.0,
                &record.request.session_id.0,
                &record.native_thread_id,
                &record.native_turn_id,
                &enum_json(&record.phase)?,
                &(record.revision as i64),
                &record.updated_at,
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external turn", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external turn", error))?;
        Ok(record.clone())
    }

    pub fn promote_queued_message_to_external_turn(
        &self,
        queued_message_id: &str,
        now: &IsoTimestamp,
        record: &ExternalTurnCorrelation,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL queued external turn promotion", error)
        })?;
        let existing = load_optional::<ExternalTurnCorrelation>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_turns
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&record.request.idempotency_key],
            "load PostgreSQL idempotent queued external turn",
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
        let Some(mut queued) = self
            .load_queued_messages_in_tx(
                &mut tx,
                &QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(record.request.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                },
            )?
            .into_iter()
            .find(|queued| queued.message_id == queued_message_id)
        else {
            return Ok(None);
        };
        if queued.expires_at <= *now {
            queued.state = QueuedMessageState::Expired;
            queued.terminal_at = Some(now.clone());
            queued.state_reason = Some("ttl_expired_before_external_turn_claim".into());
            self.save_queued_message_in_tx(&mut tx, &queued)?;
            tx.commit().map_err(|error| {
                postgres_error("commit expired PostgreSQL queued external turn", error)
            })?;
            return Ok(None);
        }
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_turns
                    (request_id, idempotency_key, runtime_id, binding_id, session_id,
                     native_thread_id, native_turn_id, phase, revision, updated_at, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
            ),
            &[
                &record.request.request_id.0,
                &record.request.idempotency_key,
                &record.runtime_id.0,
                &record.request.binding_id.0,
                &record.request.session_id.0,
                &record.native_thread_id,
                &record.native_turn_id,
                &enum_json(&record.phase)?,
                &(record.revision as i64),
                &record.updated_at,
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| postgres_error("save promoted PostgreSQL external turn", error))?;
        queued.state = QueuedMessageState::Delivered;
        queued.delivery_attempts += 1;
        queued.terminal_at = Some(now.clone());
        queued.state_reason = Some(format!(
            "promoted_to_external_turn:{}",
            record.request.request_id.0
        ));
        self.save_queued_message_in_tx(&mut tx, &queued)?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL queued external turn promotion", error)
        })?;
        Ok(Some(record.clone()))
    }

    pub fn update_external_turn(
        &self,
        next: &ExternalTurnCorrelation,
        expected_revision: u64,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start update PostgreSQL external turn", error))?;
        let current = load_required::<ExternalTurnCorrelation>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_turns
                 WHERE request_id = $1 FOR UPDATE"
            ),
            &[&next.request.request_id.0],
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
            &format!(
                "UPDATE {schema}.external_turns SET native_turn_id = $1, phase = $2,
                    revision = $3, updated_at = $4, record_json = $5
                 WHERE request_id = $6 AND revision = $7"
            ),
            &[
                &saved.native_turn_id,
                &enum_json(&saved.phase)?,
                &(saved.revision as i64),
                &saved.updated_at,
                &to_json_text(&saved)?,
                &saved.request.request_id.0,
                &(expected_revision as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL external turn", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit update PostgreSQL external turn", error))?;
        Ok(saved)
    }

    pub fn get_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!("SELECT record_json FROM {schema}.external_turns WHERE request_id = $1"),
            &[&request_id.0],
            "load PostgreSQL external turn",
        )
    }

    pub fn list_external_turns_for_native_thread(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_thread_id: &str,
    ) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_turns
                 WHERE runtime_id = $1 AND native_thread_id = $2
                 ORDER BY updated_at, request_id"
            ),
            &[&runtime_id.0, &native_thread_id],
            "list PostgreSQL external turns for native thread",
        )
    }

    pub fn list_nonterminal_external_turns(&self) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_turns
                 WHERE phase IN ('accepted', 'starting', 'active', 'waiting_interaction')
                 ORDER BY updated_at, request_id"
            ),
            &[],
            "list PostgreSQL nonterminal external turns",
        )
    }

    pub fn put_external_control_receipt(
        &self,
        receipt: &ExternalControlReceipt,
    ) -> CoreResult<ExternalControlReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL external control", error))?;
        let existing = load_optional::<ExternalControlReceipt>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_control_receipts
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&receipt.request.idempotency_key],
            "load PostgreSQL external control",
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
            &format!(
                "INSERT INTO {schema}.external_control_receipts
                    (control_id, idempotency_key, binding_id, request_fingerprint,
                     status, revision, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)"
            ),
            &[
                &receipt.request.control_id.0,
                &receipt.request.idempotency_key,
                &receipt.request.binding_id.0,
                &receipt.request_fingerprint,
                &enum_json(&receipt.status)?,
                &(receipt.revision as i64),
                &to_json_text(receipt)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external control", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external control", error))?;
        Ok(receipt.clone())
    }

    pub fn get_external_control_receipt(
        &self,
        control_id: &ExternalControlId,
    ) -> CoreResult<Option<ExternalControlReceipt>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_control_receipts WHERE control_id = $1"
            ),
            &[&control_id.0],
            "load PostgreSQL external control",
        )
    }

    pub fn update_external_control_receipt(
        &self,
        next: &ExternalControlReceipt,
        expected_revision: u64,
    ) -> CoreResult<ExternalControlReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start update PostgreSQL external control", error))?;
        let current = load_required::<ExternalControlReceipt>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_control_receipts
                 WHERE control_id = $1 FOR UPDATE"
            ),
            &[&next.request.control_id.0],
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
            &format!(
                "UPDATE {schema}.external_control_receipts SET status = $1,
                    revision = $2, record_json = $3
                 WHERE control_id = $4 AND revision = $5"
            ),
            &[
                &enum_json(&saved.status)?,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
                &saved.request.control_id.0,
                &(expected_revision as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL external control", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit update PostgreSQL external control", error))?;
        Ok(saved)
    }

    pub fn put_external_interaction(
        &self,
        record: &ExternalInteractionRecord,
    ) -> CoreResult<ExternalInteractionRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL external interaction", error))?;
        let existing = load_optional::<ExternalInteractionRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_interactions
                 WHERE interaction_id = $1 OR (runtime_id = $2 AND native_request_id = $3)
                 FOR UPDATE"
            ),
            &[
                &record.interaction_id.0,
                &record.runtime_id.0,
                &record.native_request_id,
            ],
            "load PostgreSQL idempotent external interaction",
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
            &format!(
                "INSERT INTO {schema}.external_interactions
                        (interaction_id, runtime_id, binding_id, request_id, native_request_id,
                         status, expires_at, revision, record_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
            ),
            &[
                &record.interaction_id.0,
                &record.runtime_id.0,
                &record.binding_id.0,
                &record.request_id.0,
                &record.native_request_id,
                &enum_json(&record.status)?,
                &record.expires_at,
                &(record.revision as i64),
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external interaction", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external interaction", error))?;
        Ok(record.clone())
    }

    pub fn update_external_interaction(
        &self,
        next: &ExternalInteractionRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalInteractionRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start update PostgreSQL external interaction", error)
        })?;
        let current = load_required::<ExternalInteractionRecord>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_interactions
                 WHERE interaction_id = $1 FOR UPDATE"
            ),
            &[&next.interaction_id.0],
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
            &format!(
                "UPDATE {schema}.external_interactions SET status = $1,
                    revision = $2, record_json = $3
                 WHERE interaction_id = $4 AND revision = $5"
            ),
            &[
                &enum_json(&saved.status)?,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
                &saved.interaction_id.0,
                &(expected_revision as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL external interaction", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit update PostgreSQL external interaction", error)
        })?;
        Ok(saved)
    }

    pub fn list_pending_external_interactions(&self) -> CoreResult<Vec<ExternalInteractionRecord>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_interactions
                 WHERE status = 'pending' ORDER BY expires_at, interaction_id"
            ),
            &[],
            "list PostgreSQL pending external interactions",
        )
    }

    pub fn append_external_runtime_event(
        &self,
        event: &NormalizedExternalRuntimeEvent,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start append PostgreSQL external runtime event", error)
        })?;
        let existing = load_optional::<NormalizedExternalRuntimeEvent>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_events
                 WHERE event_id = $1 OR (runtime_id = $2 AND sequence_id = $3)
                 FOR UPDATE"
            ),
            &[
                &event.event_id,
                &event.runtime_id.0,
                &(event.sequence_id as i64),
            ],
            "load PostgreSQL idempotent external runtime event",
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
        let session_id = event.session_id.as_ref().map(|value| value.0.as_str());
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_runtime_events
                        (event_id, runtime_id, session_id, sequence_id, kind, created_at,
                         native_thread_id, native_turn_id, record_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
            ),
            &[
                &event.event_id,
                &event.runtime_id.0,
                &session_id,
                &(event.sequence_id as i64),
                &event.kind,
                &event.created_at,
                &event.native_thread_id,
                &event.native_turn_id,
                &to_json_text(event)?,
            ],
        )
        .map_err(|error| postgres_error("append PostgreSQL external runtime event", error))?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_runtime_event_cursors(runtime_id, next_sequence_id)
                 VALUES ($1, $2)
                 ON CONFLICT(runtime_id) DO UPDATE SET next_sequence_id =
                    GREATEST({schema}.external_runtime_event_cursors.next_sequence_id,
                             EXCLUDED.next_sequence_id)"
            ),
            &[
                &event.runtime_id.0,
                &(event.sequence_id.saturating_add(1) as i64),
            ],
        )
        .map_err(|error| {
            postgres_error("advance PostgreSQL external runtime event cursor", error)
        })?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external runtime event", error))?;
        Ok(())
    }

    pub fn append_external_runtime_event_allocated(
        &self,
        input: &ExternalRuntimeEventInput,
    ) -> CoreResult<NormalizedExternalRuntimeEvent> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start allocated PostgreSQL external runtime event", error)
        })?;
        tx.query_one(
            &format!(
                "SELECT runtime_id FROM {schema}.external_runtime_registrations
                 WHERE runtime_id = $1 FOR UPDATE"
            ),
            &[&input.runtime_id.0],
        )
        .map_err(|error| postgres_error("lock external runtime event sequence", error))?;
        let existing = load_optional::<NormalizedExternalRuntimeEvent>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_events WHERE event_id = $1"
            ),
            &[&input.event_id],
            "load allocated PostgreSQL external runtime event",
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
            &format!(
                "INSERT INTO {schema}.external_runtime_event_cursors(runtime_id, next_sequence_id)
                 SELECT $1, COALESCE(MAX(sequence_id), 0) + 1
                   FROM {schema}.external_runtime_events WHERE runtime_id = $1
                 ON CONFLICT(runtime_id) DO NOTHING"
            ),
            &[&input.runtime_id.0],
        )
        .map_err(|error| {
            postgres_error("initialize PostgreSQL external runtime event cursor", error)
        })?;
        let next_sequence = tx
            .query_one(
                &format!(
                    "UPDATE {schema}.external_runtime_event_cursors
                        SET next_sequence_id = next_sequence_id + 1
                      WHERE runtime_id = $1
                      RETURNING next_sequence_id - 1"
                ),
                &[&input.runtime_id.0],
            )
            .map_err(|error| {
                postgres_error("allocate PostgreSQL external runtime event sequence", error)
            })?
            .get::<_, i64>(0) as u64;
        let event = normalized_event_from_input(input, next_sequence);
        let session_id = event.session_id.as_ref().map(|value| value.0.as_str());
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_runtime_events
                    (event_id, runtime_id, session_id, sequence_id, kind, created_at,
                     native_thread_id, native_turn_id, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
            ),
            &[
                &event.event_id,
                &event.runtime_id.0,
                &session_id,
                &(event.sequence_id as i64),
                &event.kind,
                &event.created_at,
                &event.native_thread_id,
                &event.native_turn_id,
                &to_json_text(&event)?,
            ],
        )
        .map_err(|error| {
            postgres_error("append allocated PostgreSQL external runtime event", error)
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit allocated PostgreSQL external runtime event", error)
        })?;
        Ok(event)
    }

    pub fn query_external_runtime_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.external_runtime_events
                 WHERE runtime_id = $1 AND sequence_id > $2
                 ORDER BY sequence_id LIMIT $3"
            ),
            &[
                &runtime_id.0,
                &(after_sequence as i64),
                &(limit.clamp(1, 1_000) as i64),
            ],
            "query PostgreSQL external runtime events",
        )
    }

    pub fn create_agent_correlated_round(
        &self,
        record: &AgentCorrelatedRound,
    ) -> CoreResult<AgentCorrelatedRound> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL external round", error))?;
        let existing = load_optional::<AgentCorrelatedRound>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.agent_correlated_rounds
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&record.idempotency_key],
            "load PostgreSQL external round",
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
            &format!(
                "INSERT INTO {schema}.agent_correlated_rounds
                    (round_id, idempotency_key, sender_agent_id, sender_session_id,
                     recipient_agent_id, recipient_session_id, correlation_id, status, expires_at,
                     revision, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
            ),
            &[
                &record.round_id.0,
                &record.idempotency_key,
                &record.sender_agent_id.0,
                &record.sender_session_id.as_ref().map(|id| id.0.as_str()),
                &record.recipient_agent_id.0,
                &record.recipient_session_id.0,
                &record.correlation_id,
                &enum_json(&record.status)?,
                &record.expires_at,
                &(record.revision as i64),
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external round", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external round", error))?;
        Ok(record.clone())
    }

    pub fn create_agent_message_delivery(
        &self,
        record: &AgentMessageDeliveryReceipt,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL agent message delivery", error))?;
        let existing = load_optional::<AgentMessageDeliveryReceipt>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE idempotency_key = $1 FOR UPDATE"
            ),
            &[&record.request.idempotency_key],
            "load PostgreSQL agent message delivery",
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
            &format!(
                "INSERT INTO {schema}.agent_message_delivery_receipts
                    (delivery_id, idempotency_key, message_id, from_agent_id, from_session_id,
                     to_agent_id, to_session_id, reply_to_message_id, status, created_at,
                     expires_at, revision, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"
            ),
            &[
                &record.request.delivery_id.0,
                &record.request.idempotency_key,
                &record.request.message_id,
                &record.request.from_agent_id.0,
                &record
                    .request
                    .from_session_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &record.request.to_agent_id.0,
                &record
                    .request
                    .to_session_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &record.request.reply_to_message_id,
                &enum_json(&record.status)?,
                &record.request.created_at,
                &record.request.expires_at,
                &(record.revision as i64),
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL agent message delivery", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL agent message delivery", error))?;
        Ok(record.clone())
    }

    pub fn update_agent_message_delivery(
        &self,
        next: &AgentMessageDeliveryReceipt,
        expected_revision: u64,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start update PostgreSQL agent message delivery", error)
        })?;
        let current = load_required::<AgentMessageDeliveryReceipt>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE delivery_id = $1 FOR UPDATE"
            ),
            &[&next.request.delivery_id.0],
            "PostgreSQL agent message delivery",
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
            &format!(
                "UPDATE {schema}.agent_message_delivery_receipts SET status = $1,
                    revision = $2, record_json = $3
                 WHERE delivery_id = $4 AND revision = $5"
            ),
            &[
                &enum_json(&saved.status)?,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
                &saved.request.delivery_id.0,
                &(expected_revision as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL agent message delivery", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit update PostgreSQL agent message delivery", error)
        })?;
        Ok(saved)
    }

    pub fn get_agent_correlated_round(
        &self,
        round_id: &rusty_crew_core_protocol::AgentRoundId,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_correlated_rounds WHERE round_id = $1"
            ),
            &[&round_id.0],
            "load PostgreSQL agent correlated round",
        )
    }

    pub fn get_agent_message_delivery(
        &self,
        delivery_id: &rusty_crew_core_protocol::AgentMessageDeliveryId,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE delivery_id = $1"
            ),
            &[&delivery_id.0],
            "load PostgreSQL agent message delivery",
        )
    }

    pub fn get_agent_message_delivery_by_message_id(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE message_id = $1"
            ),
            &[&message_id],
            "load PostgreSQL agent message delivery by message id",
        )
    }

    pub fn get_agent_message_reply(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        load_optional(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE reply_to_message_id = $1"
            ),
            &[&message_id],
            "load PostgreSQL agent message reply",
        )
    }

    pub fn list_agent_message_inbox_deliveries(
        &self,
        query: &rusty_crew_core_protocol::AgentMessageInboxQuery,
        limit: u32,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        let to_agent = query.to_agent_id.as_ref().map(|value| value.0.as_str());
        let to_session = query.to_session_id.as_ref().map(|value| value.0.as_str());
        let from_agent = query.from_agent_id.as_ref().map(|value| value.0.as_str());
        let from_session = query.from_session_id.as_ref().map(|value| value.0.as_str());
        let correlation_id = query.correlation_id.as_deref();
        let message_id = query.message_id.as_deref();
        let limit = i64::from(limit);
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE reply_to_message_id IS NULL
                   AND ($1::TEXT IS NULL OR to_agent_id = $1)
                   AND ($2::TEXT IS NULL OR to_session_id = $2)
                   AND ($3::TEXT IS NULL OR from_agent_id = $3)
                   AND ($4::TEXT IS NULL OR from_session_id = $4)
                   AND ($5::TEXT IS NULL OR record_json::jsonb #>> '{{request,correlationId}}' = $5)
                   AND ($6::TEXT IS NULL OR message_id = $6)
                 ORDER BY created_at, delivery_id LIMIT $7"
            ),
            &[
                &to_agent,
                &to_session,
                &from_agent,
                &from_session,
                &correlation_id,
                &message_id,
                &limit,
            ],
            "list PostgreSQL agent message inbox deliveries",
        )
    }

    pub fn list_agent_message_traffic_deliveries(
        &self,
        query: &rusty_crew_core_protocol::AgentMessageInboxQuery,
        limit: u32,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        let to_agent = query.to_agent_id.as_ref().map(|value| value.0.as_str());
        let to_session = query.to_session_id.as_ref().map(|value| value.0.as_str());
        let from_agent = query.from_agent_id.as_ref().map(|value| value.0.as_str());
        let from_session = query.from_session_id.as_ref().map(|value| value.0.as_str());
        let correlation_id = query.correlation_id.as_deref();
        let message_id = query.message_id.as_deref();
        let limit = i64::from(limit);
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE ($1::TEXT IS NULL OR to_agent_id = $1)
                   AND ($2::TEXT IS NULL OR to_session_id = $2)
                   AND ($3::TEXT IS NULL OR from_agent_id = $3)
                   AND ($4::TEXT IS NULL OR from_session_id = $4)
                   AND ($5::TEXT IS NULL OR record_json::jsonb #>> '{{request,correlationId}}' = $5)
                   AND ($6::TEXT IS NULL OR message_id = $6)
                 ORDER BY created_at, delivery_id LIMIT $7"
            ),
            &[
                &to_agent,
                &to_session,
                &from_agent,
                &from_session,
                &correlation_id,
                &message_id,
                &limit,
            ],
            "list PostgreSQL agent message traffic deliveries",
        )
    }

    pub fn list_pending_agent_message_deliveries(
        &self,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                 WHERE status = 'pending' ORDER BY expires_at, delivery_id"
            ),
            &[],
            "list PostgreSQL pending agent message deliveries",
        )
    }

    pub fn update_agent_correlated_round(
        &self,
        next: &AgentCorrelatedRound,
        expected_revision: u64,
    ) -> CoreResult<AgentCorrelatedRound> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start update PostgreSQL external round", error))?;
        let current = load_required::<AgentCorrelatedRound>(
            &mut tx,
            &format!(
                "SELECT record_json FROM {schema}.agent_correlated_rounds
                 WHERE round_id = $1 FOR UPDATE"
            ),
            &[&next.round_id.0],
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
            &format!(
                "UPDATE {schema}.agent_correlated_rounds SET status = $1,
                    revision = $2, record_json = $3
                 WHERE round_id = $4 AND revision = $5"
            ),
            &[
                &enum_json(&saved.status)?,
                &(saved.revision as i64),
                &to_json_text(&saved)?,
                &saved.round_id.0,
                &(expected_revision as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL external round", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit update PostgreSQL external round", error))?;
        Ok(saved)
    }

    pub fn list_pending_agent_rounds(&self) -> CoreResult<Vec<AgentCorrelatedRound>> {
        let schema = self.quoted_schema();
        load_list(
            &mut *self.client()?,
            &format!(
                "SELECT record_json FROM {schema}.agent_correlated_rounds
                 WHERE status = 'pending' ORDER BY expires_at, round_id"
            ),
            &[],
            "list PostgreSQL pending external rounds",
        )
    }
}

pub(super) fn compact_terminal_external_runtime_events_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
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
    let candidates = tx
        .query(
            &format!(
                "SELECT turn.runtime_id, turn.native_thread_id, turn.native_turn_id,
                        turn.session_id, turn.phase, turn.updated_at
                   FROM {schema}.external_turns turn
                  WHERE turn.native_turn_id IS NOT NULL
                    AND turn.phase IN ('completed', 'failed', 'interrupted', 'outcome_unknown')
                    AND turn.updated_at < $1
                    AND EXISTS (
                        SELECT 1 FROM {schema}.external_runtime_events event
                         WHERE event.runtime_id = turn.runtime_id
                           AND event.native_turn_id = turn.native_turn_id
                           AND event.kind IN (
                               'assistant_text_delta', 'reasoning_delta', 'plan_delta',
                               'item_lifecycle', 'command_activity', 'file_activity',
                               'mcp_activity', 'dynamic_tool_activity'
                           )
                    )
                  ORDER BY turn.updated_at, turn.runtime_id, turn.native_turn_id
                  LIMIT $2"
            ),
            &[cutoff, &(terminal_turn_batch_size as i64)],
        )
        .map_err(|error| {
            postgres_error("query PostgreSQL terminal external turn retention", error)
        })?;
    let mut report = ExternalRuntimeEventRetentionReport {
        enabled: true,
        cutoff: Some(cutoff.clone()),
        terminal_turn_batch_size: Some(terminal_turn_batch_size),
        terminal_turns_inspected: candidates.len() as u64,
        ..ExternalRuntimeEventRetentionReport::default()
    };
    for candidate in candidates {
        let runtime_id = candidate.get::<_, String>(0);
        let native_thread_id = candidate.get::<_, String>(1);
        let native_turn_id = candidate.get::<_, String>(2);
        let session_id = candidate.get::<_, String>(3);
        let phase = candidate.get::<_, String>(4);
        let terminal_at = candidate.get::<_, String>(5);
        let compacted_events = tx
            .query(
                &format!(
                    "DELETE FROM {schema}.external_runtime_events
                      WHERE runtime_id = $1 AND native_turn_id = $2
                        AND kind IN (
                            'assistant_text_delta', 'reasoning_delta', 'plan_delta',
                            'item_lifecycle', 'command_activity', 'file_activity',
                            'mcp_activity', 'dynamic_tool_activity'
                        )
                      RETURNING kind, sequence_id, LENGTH(record_json)::BIGINT"
                ),
                &[&runtime_id, &native_turn_id],
            )
            .map_err(|error| postgres_error("compact PostgreSQL external runtime events", error))?;
        let mut kind_counts = BTreeMap::<String, u64>::new();
        let mut first_sequence = None::<u64>;
        let mut last_sequence = None::<u64>;
        let mut event_count = 0_u64;
        let mut estimated_bytes = 0_u64;
        for compacted in &compacted_events {
            let kind = compacted.get::<_, String>(0);
            let sequence = compacted.get::<_, i64>(1) as u64;
            let bytes = compacted.get::<_, i64>(2) as u64;
            *kind_counts.entry(kind).or_default() += 1;
            event_count += 1;
            estimated_bytes += bytes;
            first_sequence = Some(first_sequence.map_or(sequence, |current| current.min(sequence)));
            last_sequence = Some(last_sequence.map_or(sequence, |current| current.max(sequence)));
        }
        let (Some(first_sequence), Some(last_sequence)) = (first_sequence, last_sequence) else {
            continue;
        };
        let deleted_estimated_bytes = estimated_bytes;
        let existing = tx
            .query_opt(
                &format!(
                    "SELECT kind_counts_json, compacted_event_count, estimated_compacted_bytes,
                            first_sequence_id, last_sequence_id
                       FROM {schema}.external_runtime_event_checkpoints
                      WHERE runtime_id = $1 AND native_turn_id = $2
                      FOR UPDATE"
                ),
                &[&runtime_id, &native_turn_id],
            )
            .map_err(|error| postgres_error("load PostgreSQL external event checkpoint", error))?;
        let checkpoint_created = existing.is_none();
        let (first_sequence, last_sequence, event_count, estimated_bytes) =
            if let Some(existing) = existing {
                let existing_counts: BTreeMap<String, u64> = parse_postgres_json(
                    &existing.get::<_, String>(0),
                    "parse PostgreSQL external event checkpoint",
                )?;
                for (kind, count) in existing_counts {
                    *kind_counts.entry(kind).or_default() += count;
                }
                (
                    first_sequence.min(existing.get::<_, i64>(3) as u64),
                    last_sequence.max(existing.get::<_, i64>(4) as u64),
                    event_count + existing.get::<_, i64>(1) as u64,
                    estimated_bytes + existing.get::<_, i64>(2) as u64,
                )
            } else {
                (first_sequence, last_sequence, event_count, estimated_bytes)
            };
        tx.execute(
            &format!(
                "INSERT INTO {schema}.external_runtime_event_checkpoints (
                    runtime_id, native_turn_id, native_thread_id, session_id, terminal_phase,
                    terminal_at, first_sequence_id, last_sequence_id, compacted_event_count,
                    estimated_compacted_bytes, kind_counts_json, checkpointed_at, policy_cutoff
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(runtime_id, native_turn_id) DO UPDATE SET
                    native_thread_id = EXCLUDED.native_thread_id,
                    session_id = EXCLUDED.session_id,
                    terminal_phase = EXCLUDED.terminal_phase,
                    terminal_at = EXCLUDED.terminal_at,
                    first_sequence_id = EXCLUDED.first_sequence_id,
                    last_sequence_id = EXCLUDED.last_sequence_id,
                    compacted_event_count = EXCLUDED.compacted_event_count,
                    estimated_compacted_bytes = EXCLUDED.estimated_compacted_bytes,
                    kind_counts_json = EXCLUDED.kind_counts_json,
                    checkpointed_at = EXCLUDED.checkpointed_at,
                    policy_cutoff = EXCLUDED.policy_cutoff"
            ),
            &[
                &runtime_id,
                &native_turn_id,
                &native_thread_id,
                &session_id,
                &phase,
                &terminal_at,
                &(first_sequence as i64),
                &(last_sequence as i64),
                &(event_count as i64),
                &(estimated_bytes as i64),
                &to_json_text(&kind_counts)?,
                checkpointed_at,
                cutoff,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL external event checkpoint", error))?;
        report.terminal_turns_compacted += 1;
        if checkpoint_created {
            report.checkpoints_created += 1;
        }
        report.events_deleted += compacted_events.len() as u64;
        report.estimated_reclaimed_bytes += deleted_estimated_bytes;
    }
    if let Some(oldest) = tx
        .query_opt(
            &format!(
                "SELECT sequence_id, created_at FROM {schema}.external_runtime_events
                 ORDER BY created_at, sequence_id LIMIT 1"
            ),
            &[],
        )
        .map_err(|error| postgres_error("load PostgreSQL oldest retained external event", error))?
    {
        report.oldest_retained_sequence = Some(oldest.get::<_, i64>(0) as u64);
        report.oldest_retained_at = Some(oldest.get(1));
    }
    Ok(report)
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

fn enum_json<T: serde::Serialize>(value: &T) -> CoreResult<String> {
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

fn load_optional<T: serde::de::DeserializeOwned>(
    client: &mut impl postgres::GenericClient,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
    context: &str,
) -> CoreResult<Option<T>> {
    client
        .query_opt(sql, params)
        .map_err(|error| postgres_error(context, error))?
        .map(|row| {
            let json: String = row.get(0);
            parse_postgres_json(&json, context)
        })
        .transpose()
}

fn load_required<T: serde::de::DeserializeOwned>(
    client: &mut impl postgres::GenericClient,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
    label: &str,
) -> CoreResult<T> {
    load_optional(client, sql, params, &format!("load PostgreSQL {label}"))?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, format!("{label} was not found")))
}

fn load_list<T: serde::de::DeserializeOwned>(
    client: &mut impl postgres::GenericClient,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
    context: &str,
) -> CoreResult<Vec<T>> {
    client
        .query(sql, params)
        .map_err(|error| postgres_error(context, error))?
        .into_iter()
        .map(|row| {
            let json: String = row.get(0);
            parse_postgres_json(&json, context)
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
