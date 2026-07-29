use super::*;
use crate::repos::logical_turns::{
    continuation_yield_reason_as_str, lifecycle_for_record, logical_turn_lifecycle_kind_as_str,
    logical_turn_operation_kind_as_str, logical_turn_operation_phase_as_str,
    logical_turn_phase_as_str, require_revision, require_running_fence, validate_admission,
    validate_attention, validate_attention_resolution, validate_lifecycle_identity, validate_yield,
};
use crate::{
    LogicalTurnAdmissionWrite, LogicalTurnCompletionRequest, LogicalTurnContentWrite,
    LogicalTurnContinuationTicket, LogicalTurnOutboxRecord,
};
use postgres::GenericClient;
use rusty_crew_core_protocol::{
    ContinuationId, ContinuationYieldReason, LogicalTurnAdmission, LogicalTurnAttentionReceipt,
    LogicalTurnAttentionRequest, LogicalTurnAttentionResolutionReceipt,
    LogicalTurnAttentionResolutionRequest, LogicalTurnCancelRequest,
    LogicalTurnCancellationReceipt, LogicalTurnCheckpoint, LogicalTurnClaimRequest,
    LogicalTurnContinuationClaim, LogicalTurnDiagnosticQuery, LogicalTurnHydrationReport,
    LogicalTurnId, LogicalTurnLifecycleEvent, LogicalTurnLifecycleEventKind,
    LogicalTurnOperationRecord, LogicalTurnPhase, LogicalTurnRecord, LogicalTurnYieldReceipt,
    LogicalTurnYieldRequest,
};

pub(super) fn apply_postgres_logical_turns(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE {schema}.logical_brain_turns (
            logical_turn_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            source_wake_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            current_continuation_id TEXT NOT NULL,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            terminal_at TEXT,
            record_json TEXT NOT NULL
         );
         CREATE INDEX logical_brain_turns_session_phase_idx
            ON {schema}.logical_brain_turns(session_id, phase, updated_at, logical_turn_id);
         CREATE INDEX logical_brain_turns_phase_idx
            ON {schema}.logical_brain_turns(phase, updated_at, logical_turn_id);

         CREATE TABLE {schema}.logical_brain_turn_checkpoints (
            continuation_id TEXT PRIMARY KEY,
            logical_turn_id TEXT NOT NULL REFERENCES {schema}.logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
            sequence BIGINT NOT NULL,
            parent_continuation_id TEXT,
            completed_epoch_id TEXT,
            created_at TEXT NOT NULL,
            checkpoint_json TEXT NOT NULL,
            UNIQUE(logical_turn_id, sequence)
         );
         CREATE INDEX logical_brain_turn_checkpoints_turn_idx
            ON {schema}.logical_brain_turn_checkpoints(logical_turn_id, sequence);

         CREATE TABLE {schema}.logical_brain_turn_operations (
            operation_id TEXT PRIMARY KEY,
            logical_turn_id TEXT NOT NULL REFERENCES {schema}.logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
            continuation_id TEXT NOT NULL,
            execution_epoch_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            phase TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            lease_expires_at TEXT,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            UNIQUE(logical_turn_id, idempotency_key)
         );
         CREATE INDEX logical_brain_turn_operations_turn_phase_idx
            ON {schema}.logical_brain_turn_operations(logical_turn_id, phase, operation_id);

         CREATE TABLE {schema}.logical_brain_turn_projection_outbox (
            projection_id TEXT PRIMARY KEY,
            logical_turn_id TEXT NOT NULL REFERENCES {schema}.logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
            session_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            phase TEXT NOT NULL,
            occurred_at TEXT NOT NULL,
            delivered_at TEXT,
            event_json TEXT NOT NULL
         );
         CREATE INDEX logical_brain_turn_outbox_pending_idx
            ON {schema}.logical_brain_turn_projection_outbox(delivered_at, occurred_at, projection_id);

         CREATE TABLE {schema}.logical_brain_turn_tickets (
            logical_turn_id TEXT PRIMARY KEY REFERENCES {schema}.logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
            continuation_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL,
            ticket_json TEXT NOT NULL
         );
         CREATE INDEX logical_brain_turn_tickets_created_idx
            ON {schema}.logical_brain_turn_tickets(created_at, logical_turn_id);

         CREATE TABLE {schema}.logical_brain_turn_cancel_receipts (
            logical_turn_id TEXT NOT NULL REFERENCES {schema}.logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
            idempotency_key TEXT NOT NULL,
            created_at TEXT NOT NULL,
            receipt_json TEXT NOT NULL,
            PRIMARY KEY(logical_turn_id, idempotency_key)
         );

         CREATE TABLE {schema}.logical_brain_turn_blobs (
            content_ref TEXT PRIMARY KEY,
            fingerprint TEXT NOT NULL,
            content_kind TEXT NOT NULL,
            content BYTEA NOT NULL,
            created_at TEXT NOT NULL
         );"
    ))
    .map_err(|error| postgres_error("create PostgreSQL logical turn tables", error))
}

impl PostgresBackendStore {
    pub fn insert_logical_turn_admission(
        &self,
        write: &LogicalTurnAdmissionWrite,
    ) -> CoreResult<LogicalTurnAdmission> {
        validate_admission(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn admission", error))?;
        for content in &write.frozen_content {
            insert_content_pg(&mut tx, &schema, content)?;
        }
        let record = &write.admission.record;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.logical_brain_turns (
                    logical_turn_id, session_id, source_wake_id, phase,
                    current_continuation_id, revision, updated_at, terminal_at, record_json
                 ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
            ),
            &[
                &record.logical_turn_id.0,
                &record.session_id.0,
                &record.source_wake_id,
                &logical_turn_phase_as_str(record.phase),
                &record.current_continuation_id.0,
                &(record.revision as i64),
                &record.updated_at,
                &record.terminal_at,
                &to_json_text(record)?,
            ],
        )
        .map_err(|error| {
            if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!("logical turn {} already exists", record.logical_turn_id.0),
                )
            } else {
                postgres_error("insert PostgreSQL logical turn", error)
            }
        })?;
        insert_checkpoint_pg(&mut tx, &schema, &write.admission.initial_checkpoint)?;
        insert_outbox_pg(&mut tx, &schema, &write.admission.lifecycle_event)?;
        upsert_ticket_pg(
            &mut tx,
            &schema,
            &LogicalTurnContinuationTicket {
                logical_turn_id: record.logical_turn_id.clone(),
                continuation_id: record.current_continuation_id.clone(),
                session_id: record.session_id.clone(),
                reason: ContinuationYieldReason::InitialAdmission,
                created_at: record.admitted_at.clone(),
            },
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL logical turn admission", error))?;
        Ok(write.admission.clone())
    }

    pub fn get_logical_turn(
        &self,
        logical_turn_id: &LogicalTurnId,
    ) -> CoreResult<Option<LogicalTurnRecord>> {
        let schema = self.quoted_schema();
        load_turn_pg(&mut *self.client()?, &schema, logical_turn_id)
    }

    pub fn list_logical_turns(
        &self,
        query: &LogicalTurnDiagnosticQuery,
    ) -> CoreResult<Vec<LogicalTurnRecord>> {
        let schema = self.quoted_schema();
        let logical_turn_id = query.logical_turn_id.as_ref().map(|value| value.0.as_str());
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.logical_brain_turns
                     WHERE ($1::text IS NULL OR logical_turn_id = $1)
                       AND ($2::text IS NULL OR session_id = $2)
                       AND ($3 OR terminal_at IS NULL)
                     ORDER BY updated_at DESC, logical_turn_id DESC
                     LIMIT $4"
                ),
                &[
                    &logical_turn_id,
                    &session_id,
                    &query.include_terminal,
                    &(i64::from(query.limit.clamp(1, 500))),
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL logical turn diagnostics", error))?
            .into_iter()
            .map(|row| decode_pg(row.get(0), "logical turn diagnostic"))
            .collect()
    }

    pub fn get_logical_turn_checkpoint(
        &self,
        continuation_id: &ContinuationId,
    ) -> CoreResult<Option<LogicalTurnCheckpoint>> {
        let schema = self.quoted_schema();
        load_checkpoint_pg(&mut *self.client()?, &schema, continuation_id)
    }

    pub fn load_logical_turn_content(&self, content_ref: &str) -> CoreResult<Option<Vec<u8>>> {
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT content FROM {schema}.logical_brain_turn_blobs WHERE content_ref = $1"
                ),
                &[&content_ref],
            )
            .map_err(|error| postgres_error("load PostgreSQL logical turn content", error))
            .map(|row| row.map(|row| row.get(0)))
    }

    pub fn claim_logical_turn(
        &self,
        request: &LogicalTurnClaimRequest,
    ) -> CoreResult<LogicalTurnContinuationClaim> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn claim", error))?;
        let mut record = load_turn_pg_for_update(&mut tx, &schema, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        let checkpoint = load_checkpoint_pg(&mut tx, &schema, &request.continuation_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "logical turn checkpoint not found")
            })?;
        if record.phase == LogicalTurnPhase::Running
            && record.active_epoch_id.as_ref() == Some(&request.execution_epoch_id)
            && record.claim_holder.as_deref() == Some(request.claim_holder.as_str())
        {
            return Ok(LogicalTurnContinuationClaim {
                claim_generation: record.claim_generation.unwrap_or(0),
                record,
                checkpoint,
                replayed: true,
            });
        }
        require_revision(&record, request.expected_revision)?;
        if !record.phase.is_runnable()
            || record.current_continuation_id != request.continuation_id
            || checkpoint.logical_turn_id != record.logical_turn_id
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn is not claimable at the requested continuation",
            ));
        }
        let expected_revision = record.revision;
        let claim_generation = record.claim_generation.unwrap_or(0) + 1;
        record.phase = LogicalTurnPhase::Running;
        record.active_epoch_id = Some(request.execution_epoch_id.clone());
        record.claim_generation = Some(claim_generation);
        record.claim_holder = Some(request.claim_holder.clone());
        record.claim_expires_at = Some(request.claim_expires_at.clone());
        record.updated_at = request.now.clone();
        record.revision += 1;
        update_turn_pg(&mut tx, &schema, &record, expected_revision)?;
        tx.execute(
            &format!("DELETE FROM {schema}.logical_brain_turn_tickets WHERE logical_turn_id = $1"),
            &[&record.logical_turn_id.0],
        )
        .map_err(|error| postgres_error("consume PostgreSQL logical turn ticket", error))?;
        insert_outbox_pg(
            &mut tx,
            &schema,
            &lifecycle_for_record(
                &record,
                checkpoint.progress.clone(),
                LogicalTurnLifecycleEventKind::ContinuationClaimed,
                "continuation_claimed",
                "logical turn continuation claimed for an execution epoch",
                request.now.clone(),
            ),
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL logical turn claim", error))?;
        Ok(LogicalTurnContinuationClaim {
            record,
            checkpoint,
            claim_generation,
            replayed: false,
        })
    }

    pub fn yield_logical_turn(
        &self,
        request: &LogicalTurnYieldRequest,
    ) -> CoreResult<LogicalTurnYieldReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn yield", error))?;
        let mut record = load_turn_pg_for_update(&mut tx, &schema, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        if let Some(existing) =
            load_checkpoint_pg(&mut tx, &schema, &request.checkpoint.continuation_id)?
        {
            if existing == request.checkpoint
                && record.current_continuation_id == existing.continuation_id
            {
                return Ok(LogicalTurnYieldReceipt {
                    record,
                    checkpoint: existing,
                    replayed: true,
                });
            }
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn continuation id already has different checkpoint content",
            ));
        }
        require_running_fence(
            &record,
            request.expected_revision,
            &request.expected_epoch_id,
            request.expected_claim_generation,
            request.expected_cancellation_generation,
        )?;
        validate_yield(&record, request)?;
        let expected_revision = record.revision;
        record.phase = LogicalTurnPhase::Yielded;
        record.current_continuation_id = request.checkpoint.continuation_id.clone();
        record.continuation_sequence = request.checkpoint.sequence;
        record.active_epoch_id = None;
        record.claim_holder = None;
        record.claim_expires_at = None;
        record.updated_at = request.now.clone();
        record.revision += 1;
        update_turn_pg(&mut tx, &schema, &record, expected_revision)?;
        insert_checkpoint_pg(&mut tx, &schema, &request.checkpoint)?;
        insert_outbox_pg(&mut tx, &schema, &request.lifecycle_event)?;
        upsert_ticket_pg(
            &mut tx,
            &schema,
            &LogicalTurnContinuationTicket {
                logical_turn_id: record.logical_turn_id.clone(),
                continuation_id: record.current_continuation_id.clone(),
                session_id: record.session_id.clone(),
                reason: request.checkpoint.yield_reason,
                created_at: request.now.clone(),
            },
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL logical turn yield", error))?;
        Ok(LogicalTurnYieldReceipt {
            record,
            checkpoint: request.checkpoint.clone(),
            replayed: false,
        })
    }

    pub fn require_logical_turn_attention(
        &self,
        request: &LogicalTurnAttentionRequest,
    ) -> CoreResult<LogicalTurnAttentionReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn attention", error))?;
        let mut record = load_turn_pg_for_update(&mut tx, &schema, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        if let Some(existing) =
            load_checkpoint_pg(&mut tx, &schema, &request.checkpoint.continuation_id)?
        {
            if existing == request.checkpoint
                && record.current_continuation_id == existing.continuation_id
                && record.phase == LogicalTurnPhase::AttentionRequired
                && record.attention.as_ref() == Some(&request.attention)
            {
                return Ok(LogicalTurnAttentionReceipt {
                    record,
                    checkpoint: existing,
                    replayed: true,
                });
            }
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn attention continuation id already has different content",
            ));
        }
        require_running_fence(
            &record,
            request.expected_revision,
            &request.expected_epoch_id,
            request.expected_claim_generation,
            request.expected_cancellation_generation,
        )?;
        validate_attention(&record, request)?;
        let expected_revision = record.revision;
        record.phase = LogicalTurnPhase::AttentionRequired;
        record.current_continuation_id = request.checkpoint.continuation_id.clone();
        record.continuation_sequence = request.checkpoint.sequence;
        record.active_epoch_id = None;
        record.claim_holder = None;
        record.claim_expires_at = None;
        record.attention = Some(request.attention.clone());
        record.updated_at = request.now.clone();
        record.revision += 1;
        update_turn_pg(&mut tx, &schema, &record, expected_revision)?;
        insert_checkpoint_pg(&mut tx, &schema, &request.checkpoint)?;
        insert_outbox_pg(&mut tx, &schema, &request.lifecycle_event)?;
        delete_ticket_pg(&mut tx, &schema, &record.logical_turn_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL logical turn attention", error))?;
        Ok(LogicalTurnAttentionReceipt {
            record,
            checkpoint: request.checkpoint.clone(),
            replayed: false,
        })
    }

    pub fn resolve_logical_turn_attention(
        &self,
        request: &LogicalTurnAttentionResolutionRequest,
    ) -> CoreResult<LogicalTurnAttentionResolutionReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("begin PostgreSQL logical turn attention resolution", error)
        })?;
        let mut record = load_turn_pg_for_update(&mut tx, &schema, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        validate_attention_resolution(&record, request)?;
        let checkpoint = load_checkpoint_pg(&mut tx, &schema, &record.current_continuation_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "attention-required logical turn has no current checkpoint",
                )
            })?;
        let expected_revision = record.revision;
        record.phase = LogicalTurnPhase::Runnable;
        record.attention = None;
        record.updated_at = request.now.clone();
        record.revision += 1;
        update_turn_pg(&mut tx, &schema, &record, expected_revision)?;
        insert_outbox_pg(&mut tx, &schema, &request.lifecycle_event)?;
        upsert_ticket_pg(
            &mut tx,
            &schema,
            &LogicalTurnContinuationTicket {
                logical_turn_id: record.logical_turn_id.clone(),
                continuation_id: record.current_continuation_id.clone(),
                session_id: record.session_id.clone(),
                reason: ContinuationYieldReason::OperatorRequested,
                created_at: request.now.clone(),
            },
        )?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL logical turn attention resolution", error)
        })?;
        Ok(LogicalTurnAttentionResolutionReceipt {
            record,
            checkpoint,
            action: request.action,
            replayed: false,
        })
    }

    pub fn complete_logical_turn(
        &self,
        request: &LogicalTurnCompletionRequest,
    ) -> CoreResult<LogicalTurnRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn completion", error))?;
        let mut record = load_turn_pg_for_update(&mut tx, &schema, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        if record.phase.is_terminal() {
            return Ok(record);
        }
        require_running_fence(
            &record,
            request.expected_revision,
            &request.expected_epoch_id,
            request.expected_claim_generation,
            request.expected_cancellation_generation,
        )?;
        if !matches!(
            request.lifecycle_event.kind,
            LogicalTurnLifecycleEventKind::Completed | LogicalTurnLifecycleEventKind::Failed
        ) || !matches!(
            request.lifecycle_event.phase,
            LogicalTurnPhase::Completed | LogicalTurnPhase::Failed
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "logical turn completion requires completed or failed lifecycle event",
            ));
        }
        validate_lifecycle_identity(&record, &request.lifecycle_event)?;
        if request.lifecycle_event.continuation_id != record.current_continuation_id
            || request.lifecycle_event.execution_epoch_id.as_ref()
                != Some(&request.expected_epoch_id)
            || request.lifecycle_event.logical_turn_revision != record.revision + 1
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "logical turn completion lifecycle does not describe the active execution fence",
            ));
        }
        let expected_revision = record.revision;
        let mut checkpoint = load_checkpoint_pg(&mut tx, &schema, &record.current_continuation_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "logical turn completion checkpoint is missing",
                )
            })?;
        checkpoint.progress = request.lifecycle_event.progress.clone();
        tx.execute(
            &format!(
                "UPDATE {schema}.logical_brain_turn_checkpoints
                 SET checkpoint_json = $1 WHERE continuation_id = $2"
            ),
            &[&to_json_text(&checkpoint)?, &checkpoint.continuation_id.0],
        )
        .map_err(|error| {
            postgres_error("update completed PostgreSQL logical turn checkpoint", error)
        })?;
        record.phase = request.lifecycle_event.phase;
        record.active_epoch_id = None;
        record.claim_holder = None;
        record.claim_expires_at = None;
        record.updated_at = request.now.clone();
        record.terminal_at = Some(request.now.clone());
        record.revision += 1;
        update_turn_pg(&mut tx, &schema, &record, expected_revision)?;
        insert_outbox_pg(&mut tx, &schema, &request.lifecycle_event)?;
        tx.execute(
            &format!("DELETE FROM {schema}.logical_brain_turn_tickets WHERE logical_turn_id = $1"),
            &[&record.logical_turn_id.0],
        )
        .map_err(|error| {
            postgres_error("remove completed PostgreSQL logical turn ticket", error)
        })?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL logical turn completion", error))?;
        Ok(record)
    }

    pub fn cancel_logical_turn(
        &self,
        request: &LogicalTurnCancelRequest,
    ) -> CoreResult<LogicalTurnCancellationReceipt> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn cancellation", error))?;
        if let Some(row) = tx
            .query_opt(
                &format!(
                    "SELECT receipt_json FROM {schema}.logical_brain_turn_cancel_receipts
                     WHERE logical_turn_id = $1 AND idempotency_key = $2"
                ),
                &[&request.logical_turn_id.0, &request.idempotency_key],
            )
            .map_err(|error| postgres_error("load PostgreSQL cancellation receipt", error))?
        {
            let mut receipt: LogicalTurnCancellationReceipt =
                decode_pg(row.get(0), "logical turn cancellation receipt")?;
            receipt.replayed = true;
            return Ok(receipt);
        }
        let mut record = load_turn_pg_for_update(&mut tx, &schema, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        let already_terminal = record.phase.is_terminal();
        if !already_terminal {
            require_revision(&record, request.expected_revision)?;
            let expected_revision = record.revision;
            let progress = current_progress_pg(&mut tx, &schema, &record)?;
            let mut cancelling = record.clone();
            cancelling.phase = LogicalTurnPhase::CancelRequested;
            cancelling.cancellation_generation += 1;
            cancelling.updated_at = request.now.clone();
            cancelling.revision += 1;
            insert_outbox_pg(
                &mut tx,
                &schema,
                &lifecycle_for_record(
                    &cancelling,
                    progress.clone(),
                    LogicalTurnLifecycleEventKind::CancelRequested,
                    "operator_cancel_requested",
                    "logical turn cancellation requested",
                    request.now.clone(),
                ),
            )?;
            record.phase = LogicalTurnPhase::Cancelled;
            record.cancellation_generation += 1;
            record.active_epoch_id = None;
            record.claim_holder = None;
            record.claim_expires_at = None;
            record.attention = None;
            record.updated_at = request.now.clone();
            record.terminal_at = Some(request.now.clone());
            record.revision += 1;
            update_turn_pg(&mut tx, &schema, &record, expected_revision)?;
            insert_outbox_pg(
                &mut tx,
                &schema,
                &lifecycle_for_record(
                    &record,
                    progress,
                    LogicalTurnLifecycleEventKind::Cancelled,
                    &request.reason_code,
                    &request.summary,
                    request.now.clone(),
                ),
            )?;
            tx.execute(
                &format!(
                    "DELETE FROM {schema}.logical_brain_turn_tickets WHERE logical_turn_id = $1"
                ),
                &[&record.logical_turn_id.0],
            )
            .map_err(|error| {
                postgres_error("remove cancelled PostgreSQL logical turn ticket", error)
            })?;
        }
        let receipt = LogicalTurnCancellationReceipt {
            record,
            replayed: false,
            already_terminal,
        };
        tx.execute(
            &format!(
                "INSERT INTO {schema}.logical_brain_turn_cancel_receipts (
                    logical_turn_id, idempotency_key, created_at, receipt_json
                 ) VALUES ($1,$2,$3,$4)"
            ),
            &[
                &request.logical_turn_id.0,
                &request.idempotency_key,
                &request.now,
                &to_json_text(&receipt)?,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL cancellation receipt", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL logical turn cancellation", error)
        })?;
        Ok(receipt)
    }

    pub fn insert_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.logical_brain_turn_operations (
                        operation_id, logical_turn_id, continuation_id, execution_epoch_id,
                        kind, phase, idempotency_key, lease_expires_at, revision, updated_at, record_json
                     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"
                ),
                &[
                    &operation.operation_id.0,
                    &operation.logical_turn_id.0,
                    &operation.continuation_id.0,
                    &operation.execution_epoch_id.0,
                    &logical_turn_operation_kind_as_str(operation.kind),
                    &logical_turn_operation_phase_as_str(operation.phase),
                    &operation.idempotency_key,
                    &operation.lease_expires_at,
                    &(operation.revision as i64),
                    &operation.updated_at,
                    &to_json_text(operation)?,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL logical turn operation", error))?;
        Ok(operation.clone())
    }

    pub fn update_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
        expected_revision: u64,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        let schema = self.quoted_schema();
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.logical_brain_turn_operations
                 SET phase=$1, lease_expires_at=$2, revision=$3, updated_at=$4, record_json=$5
                 WHERE operation_id=$6 AND revision=$7"
                ),
                &[
                    &logical_turn_operation_phase_as_str(operation.phase),
                    &operation.lease_expires_at,
                    &(operation.revision as i64),
                    &operation.updated_at,
                    &to_json_text(operation)?,
                    &operation.operation_id.0,
                    &(expected_revision as i64),
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL logical turn operation", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn operation revision mismatch or missing",
            ));
        }
        Ok(operation.clone())
    }

    pub fn list_logical_turn_operations(
        &self,
        logical_turn_id: &LogicalTurnId,
    ) -> CoreResult<Vec<LogicalTurnOperationRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query(
                &format!("SELECT record_json FROM {schema}.logical_brain_turn_operations WHERE logical_turn_id=$1 ORDER BY operation_id"),
                &[&logical_turn_id.0],
            )
            .map_err(|error| postgres_error("query PostgreSQL logical turn operations", error))?
            .iter()
            .map(|row| decode_pg(row.get(0), "logical turn operation"))
            .collect()
    }

    pub fn list_logical_turn_tickets(&self) -> CoreResult<Vec<LogicalTurnContinuationTicket>> {
        let schema = self.quoted_schema();
        self.client()?
            .query(
                &format!("SELECT ticket_json FROM {schema}.logical_brain_turn_tickets ORDER BY created_at, logical_turn_id"),
                &[],
            )
            .map_err(|error| postgres_error("query PostgreSQL logical turn tickets", error))?
            .iter()
            .map(|row| decode_pg(row.get(0), "logical turn ticket"))
            .collect()
    }

    pub fn list_pending_logical_turn_outbox(&self) -> CoreResult<Vec<LogicalTurnOutboxRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query(
                &format!("SELECT event_json, delivered_at FROM {schema}.logical_brain_turn_projection_outbox WHERE delivered_at IS NULL ORDER BY occurred_at, projection_id"),
                &[],
            )
            .map_err(|error| postgres_error("query PostgreSQL logical turn outbox", error))?
            .iter()
            .map(|row| Ok(LogicalTurnOutboxRecord {
                event: decode_pg(row.get(0), "logical turn outbox event")?,
                delivered_at: row.get(1),
            }))
            .collect()
    }

    pub fn mark_logical_turn_outbox_delivered(
        &self,
        projection_id: &str,
        delivered_at: &IsoTimestamp,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?.execute(
            &format!("UPDATE {schema}.logical_brain_turn_projection_outbox SET delivered_at=$1 WHERE projection_id=$2 AND delivered_at IS NULL"),
            &[delivered_at, &projection_id],
        ).map_err(|error| postgres_error("mark PostgreSQL logical turn outbox delivered", error))?;
        Ok(())
    }

    pub fn hydrate_logical_turns(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<LogicalTurnHydrationReport> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("begin PostgreSQL logical turn hydration", error))?;
        let rows = tx
            .query(
                &format!("SELECT record_json FROM {schema}.logical_brain_turns ORDER BY updated_at, logical_turn_id FOR UPDATE"),
                &[],
            )
            .map_err(|error| postgres_error("query PostgreSQL logical turns for hydration", error))?;
        let mut records = rows
            .iter()
            .map(|row| decode_pg(row.get(0), "logical turn"))
            .collect::<CoreResult<Vec<LogicalTurnRecord>>>()?;
        let mut report = LogicalTurnHydrationReport {
            inspected: records.len() as u32,
            made_runnable: 0,
            attention_required: 0,
            already_runnable: 0,
            terminal_skipped: 0,
            hydrated_at: now.clone(),
        };
        for record in &mut records {
            if record.phase.is_terminal() {
                report.terminal_skipped += 1;
                continue;
            }
            if record.phase == LogicalTurnPhase::AttentionRequired {
                report.attention_required += 1;
                continue;
            }
            if record.phase == LogicalTurnPhase::CancelRequested {
                let expected_revision = record.revision;
                record.phase = LogicalTurnPhase::Cancelled;
                record.cancellation_generation += 1;
                clear_claim(record);
                record.updated_at = now.clone();
                record.terminal_at = Some(now.clone());
                record.revision += 1;
                update_turn_pg(&mut tx, &schema, record, expected_revision)?;
                delete_ticket_pg(&mut tx, &schema, &record.logical_turn_id)?;
                let progress = current_progress_pg(&mut tx, &schema, record)?;
                insert_outbox_pg(
                    &mut tx,
                    &schema,
                    &lifecycle_for_record(
                        record,
                        progress,
                        LogicalTurnLifecycleEventKind::Cancelled,
                        "cancelled_during_restart_hydration",
                        "logical turn cancellation was finalized during restart hydration",
                        now.clone(),
                    ),
                )?;
                continue;
            }
            let leased_host_tool: bool = tx.query_one(
                &format!("SELECT EXISTS(SELECT 1 FROM {schema}.logical_brain_turn_operations WHERE logical_turn_id=$1 AND kind='host_tool_execution' AND phase='leased')"),
                &[&record.logical_turn_id.0],
            ).map_err(|error| postgres_error("inspect PostgreSQL logical turn leased tools", error))?.get(0);
            if record.phase == LogicalTurnPhase::Running && leased_host_tool {
                let expected_revision = record.revision;
                record.phase = LogicalTurnPhase::AttentionRequired;
                record.attention = Some(rusty_crew_core_protocol::LogicalTurnAttention {
                    reason: rusty_crew_core_protocol::LogicalTurnAttentionReason::ToolOutcomeUnknown,
                    reason_code: "host_tool_outcome_unknown_after_restart".into(),
                    summary: "a host tool lease was active when the service stopped".into(),
                    evidence_refs: Vec::new(),
                    resolution_actions: vec![
                        rusty_crew_core_protocol::LogicalTurnResolutionAction::ConfirmToolCompleted,
                        rusty_crew_core_protocol::LogicalTurnResolutionAction::ConfirmToolNotCompleted,
                        rusty_crew_core_protocol::LogicalTurnResolutionAction::Cancel,
                    ],
                    retry_unchanged_safe: false,
                    required_at: now.clone(),
                });
                clear_claim(record);
                record.updated_at = now.clone();
                record.revision += 1;
                update_turn_pg(&mut tx, &schema, record, expected_revision)?;
                let progress = current_progress_pg(&mut tx, &schema, record)?;
                insert_outbox_pg(
                    &mut tx,
                    &schema,
                    &lifecycle_for_record(
                        record,
                        progress,
                        LogicalTurnLifecycleEventKind::AttentionRequired,
                        "host_tool_outcome_unknown_after_restart",
                        "logical turn requires operator resolution before it can continue",
                        now.clone(),
                    ),
                )?;
                report.attention_required += 1;
                continue;
            }
            if record.phase == LogicalTurnPhase::Runnable {
                report.already_runnable += 1;
            } else {
                let expected_revision = record.revision;
                record.phase = LogicalTurnPhase::Runnable;
                clear_claim(record);
                record.updated_at = now.clone();
                record.revision += 1;
                update_turn_pg(&mut tx, &schema, record, expected_revision)?;
                let progress = current_progress_pg(&mut tx, &schema, record)?;
                insert_outbox_pg(
                    &mut tx,
                    &schema,
                    &lifecycle_for_record(
                        record,
                        progress,
                        LogicalTurnLifecycleEventKind::ContinuationResumed,
                        "restart_recovery",
                        "logical turn continuation recovered after service restart",
                        now.clone(),
                    ),
                )?;
                report.made_runnable += 1;
            }
            upsert_ticket_pg(
                &mut tx,
                &schema,
                &LogicalTurnContinuationTicket {
                    logical_turn_id: record.logical_turn_id.clone(),
                    continuation_id: record.current_continuation_id.clone(),
                    session_id: record.session_id.clone(),
                    reason: ContinuationYieldReason::RestartRecovery,
                    created_at: now.clone(),
                },
            )?;
        }
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL logical turn hydration", error))?;
        Ok(report)
    }
}

fn clear_claim(record: &mut LogicalTurnRecord) {
    record.active_epoch_id = None;
    record.claim_holder = None;
    record.claim_expires_at = None;
}

fn load_turn_pg(
    client: &mut impl GenericClient,
    schema: &str,
    logical_turn_id: &LogicalTurnId,
) -> CoreResult<Option<LogicalTurnRecord>> {
    client
        .query_opt(
            &format!(
                "SELECT record_json FROM {schema}.logical_brain_turns WHERE logical_turn_id=$1"
            ),
            &[&logical_turn_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL logical turn", error))?
        .map(|row| decode_pg(row.get(0), "logical turn"))
        .transpose()
}

fn load_turn_pg_for_update(
    client: &mut impl GenericClient,
    schema: &str,
    logical_turn_id: &LogicalTurnId,
) -> CoreResult<Option<LogicalTurnRecord>> {
    client.query_opt(
        &format!("SELECT record_json FROM {schema}.logical_brain_turns WHERE logical_turn_id=$1 FOR UPDATE"),
        &[&logical_turn_id.0],
    ).map_err(|error| postgres_error("lock PostgreSQL logical turn", error))?
        .map(|row| decode_pg(row.get(0), "logical turn"))
        .transpose()
}

fn load_checkpoint_pg(
    client: &mut impl GenericClient,
    schema: &str,
    continuation_id: &ContinuationId,
) -> CoreResult<Option<LogicalTurnCheckpoint>> {
    client.query_opt(
        &format!("SELECT checkpoint_json FROM {schema}.logical_brain_turn_checkpoints WHERE continuation_id=$1"),
        &[&continuation_id.0],
    ).map_err(|error| postgres_error("load PostgreSQL logical turn checkpoint", error))?
        .map(|row| decode_pg(row.get(0), "logical turn checkpoint"))
        .transpose()
}

fn current_progress_pg(
    client: &mut impl GenericClient,
    schema: &str,
    record: &LogicalTurnRecord,
) -> CoreResult<rusty_crew_core_protocol::LogicalTurnProgress> {
    load_checkpoint_pg(client, schema, &record.current_continuation_id)?
        .map(|checkpoint| checkpoint.progress)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "logical turn {} current checkpoint {} is missing",
                    record.logical_turn_id.0, record.current_continuation_id.0
                ),
            )
        })
}

fn update_turn_pg(
    client: &mut impl GenericClient,
    schema: &str,
    record: &LogicalTurnRecord,
    expected_revision: u64,
) -> CoreResult<()> {
    let changed = client.execute(
        &format!("UPDATE {schema}.logical_brain_turns SET phase=$1,current_continuation_id=$2,revision=$3,updated_at=$4,terminal_at=$5,record_json=$6 WHERE logical_turn_id=$7 AND revision=$8"),
        &[
            &logical_turn_phase_as_str(record.phase),
            &record.current_continuation_id.0,
            &(record.revision as i64),
            &record.updated_at,
            &record.terminal_at,
            &to_json_text(record)?,
            &record.logical_turn_id.0,
            &(expected_revision as i64),
        ],
    ).map_err(|error| postgres_error("update PostgreSQL logical turn", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "logical turn changed during atomic transition",
        ));
    }
    Ok(())
}

fn insert_checkpoint_pg(
    client: &mut impl GenericClient,
    schema: &str,
    checkpoint: &LogicalTurnCheckpoint,
) -> CoreResult<()> {
    client.execute(
        &format!("INSERT INTO {schema}.logical_brain_turn_checkpoints (continuation_id,logical_turn_id,sequence,parent_continuation_id,completed_epoch_id,created_at,checkpoint_json) VALUES ($1,$2,$3,$4,$5,$6,$7)"),
        &[
            &checkpoint.continuation_id.0,
            &checkpoint.logical_turn_id.0,
            &(checkpoint.sequence as i64),
            &checkpoint.parent_continuation_id.as_ref().map(|id| id.0.as_str()),
            &checkpoint.completed_epoch_id.as_ref().map(|id| id.0.as_str()),
            &checkpoint.created_at,
            &to_json_text(checkpoint)?,
        ],
    ).map_err(|error| postgres_error("insert PostgreSQL logical turn checkpoint", error))?;
    Ok(())
}

fn insert_content_pg(
    client: &mut impl GenericClient,
    schema: &str,
    content: &LogicalTurnContentWrite,
) -> CoreResult<()> {
    if let Some(row) = client.query_opt(
        &format!("SELECT fingerprint,content_kind,content FROM {schema}.logical_brain_turn_blobs WHERE content_ref=$1"),
        &[&content.content_ref],
    ).map_err(|error| postgres_error("inspect PostgreSQL logical turn content", error))? {
        let existing: (String, String, Vec<u8>) = (row.get(0), row.get(1), row.get(2));
        if existing != (content.fingerprint.clone(), content.content_kind.clone(), content.content.clone()) {
            return Err(CoreError::new(CoreErrorKind::ActionRejected, "logical turn content ref collides with different bytes"));
        }
        return Ok(());
    }
    client.execute(
        &format!("INSERT INTO {schema}.logical_brain_turn_blobs (content_ref,fingerprint,content_kind,content,created_at) VALUES ($1,$2,$3,$4,$5)"),
        &[&content.content_ref,&content.fingerprint,&content.content_kind,&content.content,&content.created_at],
    ).map_err(|error| postgres_error("insert PostgreSQL logical turn content", error))?;
    Ok(())
}

fn insert_outbox_pg(
    client: &mut impl GenericClient,
    schema: &str,
    event: &LogicalTurnLifecycleEvent,
) -> CoreResult<()> {
    let event_json = to_json_text(event)?;
    if let Some(row) = client
        .query_opt(
            &format!(
                "SELECT event_json FROM {schema}.logical_brain_turn_projection_outbox WHERE projection_id=$1"
            ),
            &[&event.projection_id.0],
        )
        .map_err(|error| postgres_error("inspect PostgreSQL logical turn projection", error))?
    {
        let existing: String = row.get(0);
        if existing != event_json {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn projection id collides with a different lifecycle event",
            ));
        }
        return Ok(());
    }
    client.execute(
        &format!("INSERT INTO {schema}.logical_brain_turn_projection_outbox (projection_id,logical_turn_id,session_id,kind,phase,occurred_at,delivered_at,event_json) VALUES ($1,$2,$3,$4,$5,$6,NULL,$7)"),
        &[
            &event.projection_id.0,&event.logical_turn_id.0,&event.session_id.0,
            &logical_turn_lifecycle_kind_as_str(event.kind),&logical_turn_phase_as_str(event.phase),
            &event.occurred_at,&event_json,
        ],
    ).map_err(|error| postgres_error("insert PostgreSQL logical turn outbox event", error))?;
    Ok(())
}

fn upsert_ticket_pg(
    client: &mut impl GenericClient,
    schema: &str,
    ticket: &LogicalTurnContinuationTicket,
) -> CoreResult<()> {
    client.execute(
        &format!("INSERT INTO {schema}.logical_brain_turn_tickets (logical_turn_id,continuation_id,session_id,reason,created_at,ticket_json) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT(logical_turn_id) DO UPDATE SET continuation_id=EXCLUDED.continuation_id,session_id=EXCLUDED.session_id,reason=EXCLUDED.reason,created_at=EXCLUDED.created_at,ticket_json=EXCLUDED.ticket_json"),
        &[&ticket.logical_turn_id.0,&ticket.continuation_id.0,&ticket.session_id.0,&continuation_yield_reason_as_str(ticket.reason),&ticket.created_at,&to_json_text(ticket)?],
    ).map_err(|error| postgres_error("upsert PostgreSQL logical turn ticket", error))?;
    Ok(())
}

fn delete_ticket_pg(
    client: &mut impl GenericClient,
    schema: &str,
    logical_turn_id: &LogicalTurnId,
) -> CoreResult<()> {
    client
        .execute(
            &format!("DELETE FROM {schema}.logical_brain_turn_tickets WHERE logical_turn_id=$1"),
            &[&logical_turn_id.0],
        )
        .map_err(|error| postgres_error("delete PostgreSQL logical turn ticket", error))?;
    Ok(())
}

fn decode_pg<T: serde::de::DeserializeOwned>(raw: &str, label: &str) -> CoreResult<T> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode PostgreSQL {label}: {error}"),
        )
    })
}
