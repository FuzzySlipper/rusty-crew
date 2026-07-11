//! PostgreSQL repository for Rust-owned external-agent runtime lifecycle state.

use super::*;
use rusty_crew_core_protocol::{
    validate_external_runtime_registration, validate_external_turn_transition,
    AgentCorrelatedRound, AgentId, AgentMessageDeliveryReceipt, AgentMessageDeliveryStatus,
    AgentRoundStatus, ExternalAgentBinding, ExternalBindingId, ExternalControlId,
    ExternalControlReceipt, ExternalControllerLease, ExternalInteractionRecord,
    ExternalInteractionStatus, ExternalRuntimeId, ExternalRuntimeRegistration,
    ExternalTurnCorrelation, ExternalTurnRequestId, NormalizedExternalRuntimeEvent,
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
            if existing.request_fingerprint == receipt.request_fingerprint
                && existing.request == receipt.request
            {
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
                        (event_id, runtime_id, session_id, sequence_id, kind, created_at, record_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)"
                ),
                &[
                    &event.event_id,
                    &event.runtime_id.0,
                    &session_id,
                    &(event.sequence_id as i64),
                    &event.kind,
                    &event.created_at,
                    &to_json_text(event)?,
                ],
            )
            .map_err(|error| postgres_error("append PostgreSQL external runtime event", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL external runtime event", error))?;
        Ok(())
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
                &record.sender_session_id.0,
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
                    (delivery_id, idempotency_key, message_id, from_agent_id, to_agent_id,
                     status, expires_at, revision, record_json)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
            ),
            &[
                &record.request.delivery_id.0,
                &record.request.idempotency_key,
                &record.request.message_id,
                &record.request.from_agent_id.0,
                &record.request.to_agent_id.0,
                &enum_json(&record.status)?,
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
        if next.status == AgentMessageDeliveryStatus::Pending {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent message delivery must transition to a terminal status",
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

fn revision_conflict<T>(label: &str, expected: u64, found: u64) -> CoreResult<T> {
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!("{label} revision mismatch: expected {expected}, found {found}"),
    ))
}
