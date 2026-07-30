use super::super::*;

pub(crate) fn migrate_v57_add_logical_turns(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(LOGICAL_TURN_SQLITE_SCHEMA)
        .map_err(|error| persistence_error("apply schema migration 57", error))
}

const LOGICAL_TURN_SQLITE_SCHEMA: &str = r#"
CREATE TABLE logical_brain_turns (
    logical_turn_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    source_wake_id TEXT NOT NULL,
    phase TEXT NOT NULL,
    current_continuation_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    terminal_at TEXT,
    record_json TEXT NOT NULL
);
CREATE INDEX idx_logical_brain_turns_session_phase
    ON logical_brain_turns(session_id, phase, updated_at, logical_turn_id);
CREATE INDEX idx_logical_brain_turns_phase
    ON logical_brain_turns(phase, updated_at, logical_turn_id);

CREATE TABLE logical_brain_turn_checkpoints (
    continuation_id TEXT PRIMARY KEY,
    logical_turn_id TEXT NOT NULL REFERENCES logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    parent_continuation_id TEXT,
    completed_epoch_id TEXT,
    created_at TEXT NOT NULL,
    checkpoint_json TEXT NOT NULL,
    UNIQUE(logical_turn_id, sequence)
);
CREATE INDEX idx_logical_brain_turn_checkpoints_turn
    ON logical_brain_turn_checkpoints(logical_turn_id, sequence);

CREATE TABLE logical_brain_turn_operations (
    operation_id TEXT PRIMARY KEY,
    logical_turn_id TEXT NOT NULL REFERENCES logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
    continuation_id TEXT NOT NULL,
    execution_epoch_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    phase TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    lease_expires_at TEXT,
    revision INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    record_json TEXT NOT NULL,
    UNIQUE(logical_turn_id, idempotency_key)
);
CREATE INDEX idx_logical_brain_turn_operations_turn_phase
    ON logical_brain_turn_operations(logical_turn_id, phase, operation_id);

CREATE TABLE logical_brain_turn_projection_outbox (
    projection_id TEXT PRIMARY KEY,
    logical_turn_id TEXT NOT NULL REFERENCES logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    phase TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    delivered_at TEXT,
    event_json TEXT NOT NULL
);
CREATE INDEX idx_logical_brain_turn_outbox_pending
    ON logical_brain_turn_projection_outbox(delivered_at, occurred_at, projection_id);

CREATE TABLE logical_brain_turn_tickets (
    logical_turn_id TEXT PRIMARY KEY REFERENCES logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
    continuation_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL,
    ticket_json TEXT NOT NULL
);
CREATE INDEX idx_logical_brain_turn_tickets_created
    ON logical_brain_turn_tickets(created_at, logical_turn_id);

CREATE TABLE logical_brain_turn_cancel_receipts (
    logical_turn_id TEXT NOT NULL REFERENCES logical_brain_turns(logical_turn_id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(logical_turn_id, idempotency_key)
);

CREATE TABLE logical_brain_turn_blobs (
    content_ref TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    content_kind TEXT NOT NULL,
    content BLOB NOT NULL,
    created_at TEXT NOT NULL
);
"#;

impl CoordinationStore {
    pub fn insert_logical_turn_admission(
        &self,
        write: &LogicalTurnAdmissionWrite,
    ) -> CoreResult<LogicalTurnAdmission> {
        validate_admission(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn admission", error))?;
        for content in &write.frozen_content {
            insert_content_blob(&tx, content)?;
        }
        let record = &write.admission.record;
        tx.execute(
            "INSERT INTO logical_brain_turns (
                logical_turn_id, session_id, source_wake_id, phase,
                current_continuation_id, revision, updated_at, terminal_at, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.logical_turn_id.0,
                record.session_id.0,
                record.source_wake_id,
                logical_turn_phase_as_str(record.phase),
                record.current_continuation_id.0,
                record.revision as i64,
                record.updated_at,
                record.terminal_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| {
            logical_turn_insert_error("logical turn", &record.logical_turn_id, error)
        })?;
        insert_checkpoint(&tx, &write.admission.initial_checkpoint)?;
        insert_outbox(&tx, &write.admission.lifecycle_event)?;
        upsert_ticket(
            &tx,
            &LogicalTurnContinuationTicket {
                logical_turn_id: record.logical_turn_id.clone(),
                continuation_id: record.current_continuation_id.clone(),
                session_id: record.session_id.clone(),
                reason: ContinuationYieldReason::InitialAdmission,
                created_at: record.admitted_at.clone(),
            },
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn admission", error))?;
        Ok(write.admission.clone())
    }

    pub fn get_logical_turn(
        &self,
        logical_turn_id: &LogicalTurnId,
    ) -> CoreResult<Option<LogicalTurnRecord>> {
        let conn = self.conn()?;
        load_turn(&conn, logical_turn_id)
    }

    pub fn list_logical_turns(
        &self,
        query: &LogicalTurnDiagnosticQuery,
    ) -> CoreResult<Vec<LogicalTurnRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM logical_brain_turns
                 WHERE (?1 IS NULL OR logical_turn_id = ?1)
                   AND (?2 IS NULL OR session_id = ?2)
                   AND (?3 = 1 OR terminal_at IS NULL)
                 ORDER BY updated_at DESC, logical_turn_id DESC
                 LIMIT ?4",
            )
            .map_err(|error| persistence_error("prepare logical turn diagnostic query", error))?;
        let rows = stmt
            .query_map(
                params![
                    query.logical_turn_id.as_ref().map(|value| value.0.as_str()),
                    query.session_id.as_ref().map(|value| value.0.as_str()),
                    i64::from(query.include_terminal),
                    i64::from(query.limit.clamp(1, 500)),
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query logical turn diagnostics", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read logical turn diagnostic", error))
                .and_then(|raw| decode(&raw, "logical turn diagnostic"))
        })
        .collect()
    }

    pub fn get_logical_turn_checkpoint(
        &self,
        continuation_id: &ContinuationId,
    ) -> CoreResult<Option<LogicalTurnCheckpoint>> {
        let conn = self.conn()?;
        load_checkpoint(&conn, continuation_id)
    }

    pub fn load_logical_turn_content(&self, content_ref: &str) -> CoreResult<Option<Vec<u8>>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT content FROM logical_brain_turn_blobs WHERE content_ref = ?1",
            params![content_ref],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| persistence_error("load logical turn content", error))
    }

    pub fn claim_logical_turn(
        &self,
        request: &LogicalTurnClaimRequest,
    ) -> CoreResult<LogicalTurnContinuationClaim> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn claim", error))?;
        let mut record = load_turn(&tx, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        let checkpoint = load_checkpoint(&tx, &request.continuation_id)?.ok_or_else(|| {
            CoreError::new(CoreErrorKind::NotFound, "logical turn checkpoint not found")
        })?;
        if record.phase == LogicalTurnPhase::Running
            && record.active_epoch_id.as_ref() == Some(&request.execution_epoch_id)
            && record.claim_holder.as_deref() == Some(request.claim_holder.as_str())
        {
            let claim_generation = record.claim_generation.unwrap_or(0);
            return Ok(LogicalTurnContinuationClaim {
                record,
                checkpoint,
                claim_generation,
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
        update_turn(&tx, &record, expected_revision)?;
        tx.execute(
            "DELETE FROM logical_brain_turn_tickets WHERE logical_turn_id = ?1",
            params![record.logical_turn_id.0],
        )
        .map_err(|error| persistence_error("consume logical turn ticket", error))?;
        let lifecycle = lifecycle_for_record(
            &record,
            checkpoint.progress.clone(),
            LogicalTurnLifecycleEventKind::ContinuationClaimed,
            "continuation_claimed",
            "logical turn continuation claimed for an execution epoch",
            request.now.clone(),
        );
        insert_outbox(&tx, &lifecycle)?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn claim", error))?;
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
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn yield", error))?;
        let mut record = load_turn(&tx, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        if let Some(existing) = load_checkpoint(&tx, &request.checkpoint.continuation_id)? {
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
        update_turn(&tx, &record, expected_revision)?;
        insert_checkpoint(&tx, &request.checkpoint)?;
        insert_outbox(&tx, &request.lifecycle_event)?;
        upsert_ticket(
            &tx,
            &LogicalTurnContinuationTicket {
                logical_turn_id: record.logical_turn_id.clone(),
                continuation_id: record.current_continuation_id.clone(),
                session_id: record.session_id.clone(),
                reason: request.checkpoint.yield_reason,
                created_at: request.now.clone(),
            },
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn yield", error))?;
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
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn attention", error))?;
        let mut record = load_turn(&tx, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        if let Some(existing) = load_checkpoint(&tx, &request.checkpoint.continuation_id)? {
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
        update_turn(&tx, &record, expected_revision)?;
        insert_checkpoint(&tx, &request.checkpoint)?;
        insert_outbox(&tx, &request.lifecycle_event)?;
        tx.execute(
            "DELETE FROM logical_brain_turn_tickets WHERE logical_turn_id = ?1",
            params![record.logical_turn_id.0],
        )
        .map_err(|error| persistence_error("delete logical turn attention ticket", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn attention", error))?;
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
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn attention resolution", error))?;
        let mut record = load_turn(&tx, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        validate_attention_resolution(&record, request)?;
        let checkpoint =
            load_checkpoint(&tx, &record.current_continuation_id)?.ok_or_else(|| {
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
        update_turn(&tx, &record, expected_revision)?;
        insert_outbox(&tx, &request.lifecycle_event)?;
        upsert_ticket(
            &tx,
            &LogicalTurnContinuationTicket {
                logical_turn_id: record.logical_turn_id.clone(),
                continuation_id: record.current_continuation_id.clone(),
                session_id: record.session_id.clone(),
                reason: ContinuationYieldReason::OperatorRequested,
                created_at: request.now.clone(),
            },
        )?;
        tx.commit().map_err(|error| {
            persistence_error("commit logical turn attention resolution", error)
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
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn completion", error))?;
        let mut record = load_turn(&tx, &request.logical_turn_id)?
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
        let mut checkpoint =
            load_checkpoint(&tx, &record.current_continuation_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "logical turn completion checkpoint is missing",
                )
            })?;
        checkpoint.progress = request.lifecycle_event.progress.clone();
        tx.execute(
            "UPDATE logical_brain_turn_checkpoints SET checkpoint_json = ?1
             WHERE continuation_id = ?2",
            params![to_json_text(&checkpoint)?, checkpoint.continuation_id.0],
        )
        .map_err(|error| persistence_error("update completed logical turn checkpoint", error))?;
        record.phase = request.lifecycle_event.phase;
        record.active_epoch_id = None;
        record.claim_holder = None;
        record.claim_expires_at = None;
        record.updated_at = request.now.clone();
        record.terminal_at = Some(request.now.clone());
        record.revision += 1;
        update_turn(&tx, &record, expected_revision)?;
        insert_outbox(&tx, &request.lifecycle_event)?;
        tx.execute(
            "DELETE FROM logical_brain_turn_tickets WHERE logical_turn_id = ?1",
            params![record.logical_turn_id.0],
        )
        .map_err(|error| persistence_error("remove completed logical turn ticket", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn completion", error))?;
        Ok(record)
    }

    pub fn cancel_logical_turn(
        &self,
        request: &LogicalTurnCancelRequest,
    ) -> CoreResult<LogicalTurnCancellationReceipt> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn cancellation", error))?;
        if let Some(receipt) = tx
            .query_row(
                "SELECT receipt_json FROM logical_brain_turn_cancel_receipts
                 WHERE logical_turn_id = ?1 AND idempotency_key = ?2",
                params![request.logical_turn_id.0, request.idempotency_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| persistence_error("load logical turn cancellation receipt", error))?
        {
            return decode(&receipt, "logical turn cancellation receipt").map(
                |mut receipt: LogicalTurnCancellationReceipt| {
                    receipt.replayed = true;
                    receipt
                },
            );
        }
        let mut record = load_turn(&tx, &request.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        let already_terminal = record.phase.is_terminal();
        if !already_terminal {
            require_revision(&record, request.expected_revision)?;
            let expected_revision = record.revision;
            let progress = current_progress(&tx, &record)?;
            let mut cancelling = record.clone();
            cancelling.phase = LogicalTurnPhase::CancelRequested;
            cancelling.cancellation_generation += 1;
            cancelling.updated_at = request.now.clone();
            cancelling.revision += 1;
            insert_outbox(
                &tx,
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
            update_turn(&tx, &record, expected_revision)?;
            let lifecycle = lifecycle_for_record(
                &record,
                progress,
                LogicalTurnLifecycleEventKind::Cancelled,
                &request.reason_code,
                &request.summary,
                request.now.clone(),
            );
            insert_outbox(&tx, &lifecycle)?;
            tx.execute(
                "DELETE FROM logical_brain_turn_tickets WHERE logical_turn_id = ?1",
                params![record.logical_turn_id.0],
            )
            .map_err(|error| persistence_error("remove cancelled logical turn ticket", error))?;
        }
        let receipt = LogicalTurnCancellationReceipt {
            record,
            replayed: false,
            already_terminal,
        };
        tx.execute(
            "INSERT INTO logical_brain_turn_cancel_receipts (
                logical_turn_id, idempotency_key, created_at, receipt_json
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                request.logical_turn_id.0,
                request.idempotency_key,
                request.now,
                to_json_text(&receipt)?,
            ],
        )
        .map_err(|error| persistence_error("save logical turn cancellation receipt", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn cancellation", error))?;
        Ok(receipt)
    }

    pub fn insert_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO logical_brain_turn_operations (
                operation_id, logical_turn_id, continuation_id, execution_epoch_id,
                kind, phase, idempotency_key, lease_expires_at, revision, updated_at, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                operation.operation_id.0,
                operation.logical_turn_id.0,
                operation.continuation_id.0,
                operation.execution_epoch_id.0,
                logical_turn_operation_kind_as_str(operation.kind),
                logical_turn_operation_phase_as_str(operation.phase),
                operation.idempotency_key,
                operation.lease_expires_at,
                operation.revision as i64,
                operation.updated_at,
                to_json_text(operation)?,
            ],
        )
        .map_err(|error| persistence_error("insert logical turn operation", error))?;
        Ok(operation.clone())
    }

    pub fn lease_logical_turn_operation(
        &self,
        request: &LogicalTurnOperationLeaseRequest,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn operation lease", error))?;
        let turn = load_turn(&tx, &request.operation.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        require_running_fence(
            &turn,
            request.expected_turn_revision,
            &request.operation.execution_epoch_id,
            request.expected_claim_generation,
            request.expected_cancellation_generation,
        )?;
        if request.operation.phase != LogicalTurnOperationPhase::Leased
            || request.operation.revision != 1
            || request.operation.result_ref.is_some()
            || request.operation.result_payload.is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "logical turn operation lease must start at leased revision 1 without a result",
            ));
        }
        if let Some(existing) = load_operation(&tx, &request.operation.operation_id)? {
            if existing == request.operation {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "logical turn operation id already exists with different content",
            ));
        }
        insert_operation(&tx, &request.operation)?;
        tx.commit()
            .map_err(|error| persistence_error("commit logical turn operation lease", error))?;
        Ok(request.operation.clone())
    }

    pub fn complete_logical_turn_operation(
        &self,
        request: &LogicalTurnOperationCompletionRequest,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn operation completion", error))?;
        let turn = load_turn(&tx, &request.operation.logical_turn_id)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "logical turn not found"))?;
        let mut completed = request.operation.clone();
        let active_fence_matches = require_running_fence(
            &turn,
            request.expected_turn_revision,
            &completed.execution_epoch_id,
            request.expected_claim_generation,
            request.expected_cancellation_generation,
        )
        .is_ok();
        if active_fence_matches {
            if completed.phase != LogicalTurnOperationPhase::Completed {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "active logical turn operation completion must use completed phase",
                ));
            }
        } else if turn.cancellation_generation != request.expected_cancellation_generation
            || matches!(
                turn.phase,
                LogicalTurnPhase::CancelRequested | LogicalTurnPhase::Cancelled
            )
        {
            completed.phase = LogicalTurnOperationPhase::CompletedAfterCancel;
        } else {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn operation completion no longer matches the active execution fence",
            ));
        }
        if completed.revision != request.expected_operation_revision + 1
            || completed.result_ref.is_none()
            || completed.result_payload.is_none()
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "logical turn operation completion requires the next revision and durable result",
            ));
        }
        let existing = load_operation(&tx, &completed.operation_id)?.ok_or_else(|| {
            CoreError::new(CoreErrorKind::NotFound, "logical turn operation not found")
        })?;
        if existing.revision != request.expected_operation_revision
            || existing.logical_turn_id != completed.logical_turn_id
            || existing.continuation_id != completed.continuation_id
            || existing.execution_epoch_id != completed.execution_epoch_id
            || existing.request_fingerprint != completed.request_fingerprint
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn operation changed before completion",
            ));
        }
        update_operation(&tx, &completed, request.expected_operation_revision)?;
        tx.commit().map_err(|error| {
            persistence_error("commit logical turn operation completion", error)
        })?;
        Ok(completed)
    }

    pub fn update_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
        expected_revision: u64,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE logical_brain_turn_operations
                 SET phase = ?1, lease_expires_at = ?2, revision = ?3,
                     updated_at = ?4, record_json = ?5
                 WHERE operation_id = ?6 AND revision = ?7",
                params![
                    logical_turn_operation_phase_as_str(operation.phase),
                    operation.lease_expires_at,
                    operation.revision as i64,
                    operation.updated_at,
                    to_json_text(operation)?,
                    operation.operation_id.0,
                    expected_revision as i64,
                ],
            )
            .map_err(|error| persistence_error("update logical turn operation", error))?;
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
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM logical_brain_turn_operations
                 WHERE logical_turn_id = ?1 ORDER BY operation_id",
            )
            .map_err(|error| persistence_error("prepare logical turn operation query", error))?;
        let rows = stmt
            .query_map(params![logical_turn_id.0], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query logical turn operations", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read logical turn operation", error))
                .and_then(|raw| decode(&raw, "logical turn operation"))
        })
        .collect()
    }

    pub fn list_logical_turn_tickets(&self) -> CoreResult<Vec<LogicalTurnContinuationTicket>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT ticket_json FROM logical_brain_turn_tickets
                 ORDER BY created_at, logical_turn_id",
            )
            .map_err(|error| persistence_error("prepare logical turn ticket query", error))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query logical turn tickets", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read logical turn ticket", error))
                .and_then(|raw| decode(&raw, "logical turn ticket"))
        })
        .collect()
    }

    pub fn list_pending_logical_turn_outbox(&self) -> CoreResult<Vec<LogicalTurnOutboxRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT event_json, delivered_at FROM logical_brain_turn_projection_outbox
                 WHERE delivered_at IS NULL ORDER BY occurred_at, projection_id",
            )
            .map_err(|error| persistence_error("prepare logical turn outbox query", error))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(|error| persistence_error("query logical turn outbox", error))?;
        rows.map(|row| {
            let (raw, delivered_at) =
                row.map_err(|error| persistence_error("read logical turn outbox", error))?;
            Ok(LogicalTurnOutboxRecord {
                event: decode(&raw, "logical turn outbox event")?,
                delivered_at,
            })
        })
        .collect()
    }

    pub fn mark_logical_turn_outbox_delivered(
        &self,
        projection_id: &str,
        delivered_at: &IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE logical_brain_turn_projection_outbox SET delivered_at = ?1
             WHERE projection_id = ?2 AND delivered_at IS NULL",
            params![delivered_at, projection_id],
        )
        .map_err(|error| persistence_error("mark logical turn outbox delivered", error))?;
        Ok(())
    }

    pub fn hydrate_logical_turns(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<LogicalTurnHydrationReport> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin logical turn hydration", error))?;
        let mut records = load_all_turns(&tx)?;
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
                record.active_epoch_id = None;
                record.claim_holder = None;
                record.claim_expires_at = None;
                record.updated_at = now.clone();
                record.terminal_at = Some(now.clone());
                record.revision += 1;
                update_turn(&tx, record, expected_revision)?;
                tx.execute(
                    "DELETE FROM logical_brain_turn_tickets WHERE logical_turn_id = ?1",
                    params![record.logical_turn_id.0],
                )
                .map_err(|error| persistence_error("remove cancelled hydration ticket", error))?;
                let event = lifecycle_for_record(
                    record,
                    current_progress(&tx, record)?,
                    LogicalTurnLifecycleEventKind::Cancelled,
                    "cancelled_during_restart_hydration",
                    "logical turn cancellation was finalized during restart hydration",
                    now.clone(),
                );
                insert_outbox(&tx, &event)?;
                continue;
            }
            let leased_host_tool = has_leased_host_tool(&tx, &record.logical_turn_id)?;
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
                record.active_epoch_id = None;
                record.claim_holder = None;
                record.claim_expires_at = None;
                record.updated_at = now.clone();
                record.revision += 1;
                update_turn(&tx, record, expected_revision)?;
                let event = lifecycle_for_record(
                    record,
                    current_progress(&tx, record)?,
                    LogicalTurnLifecycleEventKind::AttentionRequired,
                    "host_tool_outcome_unknown_after_restart",
                    "logical turn requires operator resolution before it can continue",
                    now.clone(),
                );
                insert_outbox(&tx, &event)?;
                report.attention_required += 1;
                continue;
            }
            if record.phase == LogicalTurnPhase::Runnable {
                report.already_runnable += 1;
            } else {
                let expected_revision = record.revision;
                record.phase = LogicalTurnPhase::Runnable;
                record.active_epoch_id = None;
                record.claim_holder = None;
                record.claim_expires_at = None;
                record.updated_at = now.clone();
                record.revision += 1;
                update_turn(&tx, record, expected_revision)?;
                let event = lifecycle_for_record(
                    record,
                    current_progress(&tx, record)?,
                    LogicalTurnLifecycleEventKind::ContinuationResumed,
                    "restart_recovery",
                    "logical turn continuation recovered after service restart",
                    now.clone(),
                );
                insert_outbox(&tx, &event)?;
                report.made_runnable += 1;
            }
            upsert_ticket(
                &tx,
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
            .map_err(|error| persistence_error("commit logical turn hydration", error))?;
        Ok(report)
    }
}

pub(crate) fn validate_admission(write: &LogicalTurnAdmissionWrite) -> CoreResult<()> {
    let admission = &write.admission;
    let record = &admission.record;
    let checkpoint = &admission.initial_checkpoint;
    if record.phase != LogicalTurnPhase::Admitted
        || record.logical_turn_id != checkpoint.logical_turn_id
        || record.current_continuation_id != checkpoint.continuation_id
        || checkpoint.sequence != 0
        || checkpoint.parent_continuation_id.is_some()
        || admission.lifecycle_event.kind != LogicalTurnLifecycleEventKind::Admitted
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical turn admission contract is internally inconsistent",
        ));
    }
    validate_lifecycle_identity(record, &admission.lifecycle_event)?;
    if admission.lifecycle_event.continuation_id != record.current_continuation_id
        || admission.lifecycle_event.logical_turn_revision != record.revision
        || admission.lifecycle_event.phase != record.phase
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical turn admission lifecycle does not describe the admitted revision",
        ));
    }
    let required_refs = [
        checkpoint.frozen_input.body_state_ref.as_str(),
        checkpoint.frozen_input.system_prompt_ref.as_str(),
        checkpoint.frozen_input.role_assembly_ref.as_str(),
    ];
    if required_refs.iter().any(|required| {
        !write
            .frozen_content
            .iter()
            .any(|content| content.content_ref == *required)
    }) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical turn admission is missing frozen input content",
        ));
    }
    Ok(())
}

pub(crate) fn validate_yield(
    record: &LogicalTurnRecord,
    request: &LogicalTurnYieldRequest,
) -> CoreResult<()> {
    let checkpoint = &request.checkpoint;
    if checkpoint.logical_turn_id != record.logical_turn_id
        || checkpoint.parent_continuation_id.as_ref() != Some(&record.current_continuation_id)
        || checkpoint.completed_epoch_id.as_ref() != Some(&request.expected_epoch_id)
        || checkpoint.sequence != record.continuation_sequence + 1
        || checkpoint.binding_generation != record.binding_generation
        || request.lifecycle_event.kind != LogicalTurnLifecycleEventKind::ContinuationYielded
        || request.lifecycle_event.phase != LogicalTurnPhase::Yielded
        || request.lifecycle_event.continuation_id != checkpoint.continuation_id
        || request.lifecycle_event.execution_epoch_id.as_ref() != Some(&request.expected_epoch_id)
        || request.lifecycle_event.logical_turn_revision != record.revision + 1
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical turn yield contract is internally inconsistent",
        ));
    }
    validate_lifecycle_identity(record, &request.lifecycle_event)
}

pub(crate) fn validate_attention(
    record: &LogicalTurnRecord,
    request: &LogicalTurnAttentionRequest,
) -> CoreResult<()> {
    let checkpoint = &request.checkpoint;
    if checkpoint.logical_turn_id != record.logical_turn_id
        || checkpoint.parent_continuation_id.as_ref() != Some(&record.current_continuation_id)
        || checkpoint.completed_epoch_id.as_ref() != Some(&request.expected_epoch_id)
        || checkpoint.sequence != record.continuation_sequence + 1
        || checkpoint.binding_generation != record.binding_generation
        || request.lifecycle_event.kind != LogicalTurnLifecycleEventKind::AttentionRequired
        || request.lifecycle_event.phase != LogicalTurnPhase::AttentionRequired
        || request.lifecycle_event.continuation_id != checkpoint.continuation_id
        || request.lifecycle_event.execution_epoch_id.as_ref() != Some(&request.expected_epoch_id)
        || request.lifecycle_event.logical_turn_revision != record.revision + 1
        || request.attention.reason_code != request.lifecycle_event.reason_code
        || request.attention.summary != request.lifecycle_event.summary
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical turn attention contract is internally inconsistent",
        ));
    }
    validate_lifecycle_identity(record, &request.lifecycle_event)
}

pub(crate) fn validate_attention_resolution(
    record: &LogicalTurnRecord,
    request: &LogicalTurnAttentionResolutionRequest,
) -> CoreResult<()> {
    require_revision(record, request.expected_revision)?;
    let attention = record.attention.as_ref().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::ActionRejected,
            "logical turn does not require operator attention",
        )
    })?;
    if record.phase != LogicalTurnPhase::AttentionRequired
        || !attention.resolution_actions.contains(&request.action)
        || !matches!(
            request.action,
            rusty_crew_core_protocol::LogicalTurnResolutionAction::RetryUnchanged
                | rusty_crew_core_protocol::LogicalTurnResolutionAction::RetryProviderOperation
        )
        || (request.action == rusty_crew_core_protocol::LogicalTurnResolutionAction::RetryUnchanged
            && !attention.retry_unchanged_safe)
        || request.lifecycle_event.kind != LogicalTurnLifecycleEventKind::ContinuationResumed
        || request.lifecycle_event.phase != LogicalTurnPhase::Runnable
        || request.lifecycle_event.continuation_id != record.current_continuation_id
        || request.lifecycle_event.execution_epoch_id.is_some()
        || request.lifecycle_event.logical_turn_revision != record.revision + 1
    {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "logical turn attention resolution is not valid for the current state",
        ));
    }
    validate_lifecycle_identity(record, &request.lifecycle_event)
}

pub(crate) fn validate_lifecycle_identity(
    record: &LogicalTurnRecord,
    event: &LogicalTurnLifecycleEvent,
) -> CoreResult<()> {
    if event.logical_turn_id != record.logical_turn_id
        || event.session_id != record.session_id
        || event.wake_id != record.source_wake_id
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical turn lifecycle event identity does not match the turn",
        ));
    }
    Ok(())
}

pub(crate) fn require_revision(record: &LogicalTurnRecord, expected: u64) -> CoreResult<()> {
    if record.revision != expected {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "logical turn {} revision mismatch: expected {}, found {}",
                record.logical_turn_id.0, expected, record.revision
            ),
        ));
    }
    Ok(())
}

pub(crate) fn require_running_fence(
    record: &LogicalTurnRecord,
    expected_revision: u64,
    expected_epoch_id: &ExecutionEpochId,
    expected_claim_generation: u64,
    expected_cancellation_generation: u64,
) -> CoreResult<()> {
    require_revision(record, expected_revision)?;
    if record.phase != LogicalTurnPhase::Running
        || record.active_epoch_id.as_ref() != Some(expected_epoch_id)
        || record.claim_generation != Some(expected_claim_generation)
        || record.cancellation_generation != expected_cancellation_generation
    {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "logical turn execution fence no longer matches the active claim",
        ));
    }
    Ok(())
}

fn insert_content_blob(
    tx: &rusqlite::Transaction<'_>,
    content: &LogicalTurnContentWrite,
) -> CoreResult<()> {
    let existing = tx
        .query_row(
            "SELECT fingerprint, content_kind, content FROM logical_brain_turn_blobs
             WHERE content_ref = ?1",
            params![content.content_ref],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("inspect logical turn content", error))?;
    if let Some(existing) = existing {
        if existing
            != (
                content.fingerprint.clone(),
                content.content_kind.clone(),
                content.content.clone(),
            )
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn content ref collides with different bytes",
            ));
        }
        return Ok(());
    }
    tx.execute(
        "INSERT INTO logical_brain_turn_blobs (
            content_ref, fingerprint, content_kind, content, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            content.content_ref,
            content.fingerprint,
            content.content_kind,
            content.content,
            content.created_at
        ],
    )
    .map_err(|error| persistence_error("insert logical turn content", error))?;
    Ok(())
}

fn load_operation(
    conn: &rusqlite::Connection,
    operation_id: &rusty_crew_core_protocol::BrainOperationId,
) -> CoreResult<Option<LogicalTurnOperationRecord>> {
    conn.query_row(
        "SELECT record_json FROM logical_brain_turn_operations WHERE operation_id = ?1",
        params![operation_id.0],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load logical turn operation", error))?
    .map(|raw| decode(&raw, "logical turn operation"))
    .transpose()
}

fn insert_operation(
    conn: &rusqlite::Connection,
    operation: &LogicalTurnOperationRecord,
) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO logical_brain_turn_operations (
            operation_id, logical_turn_id, continuation_id, execution_epoch_id,
            kind, phase, idempotency_key, lease_expires_at, revision, updated_at, record_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            operation.operation_id.0,
            operation.logical_turn_id.0,
            operation.continuation_id.0,
            operation.execution_epoch_id.0,
            logical_turn_operation_kind_as_str(operation.kind),
            logical_turn_operation_phase_as_str(operation.phase),
            operation.idempotency_key,
            operation.lease_expires_at,
            operation.revision as i64,
            operation.updated_at,
            to_json_text(operation)?,
        ],
    )
    .map_err(|error| persistence_error("insert logical turn operation", error))?;
    Ok(())
}

fn update_operation(
    conn: &rusqlite::Connection,
    operation: &LogicalTurnOperationRecord,
    expected_revision: u64,
) -> CoreResult<()> {
    let changed = conn
        .execute(
            "UPDATE logical_brain_turn_operations
             SET phase = ?1, lease_expires_at = ?2, revision = ?3,
                 updated_at = ?4, record_json = ?5
             WHERE operation_id = ?6 AND revision = ?7",
            params![
                logical_turn_operation_phase_as_str(operation.phase),
                operation.lease_expires_at,
                operation.revision as i64,
                operation.updated_at,
                to_json_text(operation)?,
                operation.operation_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update logical turn operation", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "logical turn operation revision mismatch or missing",
        ));
    }
    Ok(())
}

fn insert_checkpoint(
    tx: &rusqlite::Transaction<'_>,
    checkpoint: &LogicalTurnCheckpoint,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO logical_brain_turn_checkpoints (
            continuation_id, logical_turn_id, sequence, parent_continuation_id,
            completed_epoch_id, created_at, checkpoint_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            checkpoint.continuation_id.0,
            checkpoint.logical_turn_id.0,
            checkpoint.sequence as i64,
            checkpoint
                .parent_continuation_id
                .as_ref()
                .map(|id| id.0.as_str()),
            checkpoint
                .completed_epoch_id
                .as_ref()
                .map(|id| id.0.as_str()),
            checkpoint.created_at,
            to_json_text(checkpoint)?,
        ],
    )
    .map_err(|error| persistence_error("insert logical turn checkpoint", error))?;
    Ok(())
}

fn insert_outbox(
    tx: &rusqlite::Transaction<'_>,
    event: &LogicalTurnLifecycleEvent,
) -> CoreResult<()> {
    let event_json = to_json_text(event)?;
    if let Some(existing) = tx
        .query_row(
            "SELECT event_json FROM logical_brain_turn_projection_outbox WHERE projection_id = ?1",
            params![event.projection_id.0],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| persistence_error("inspect logical turn outbox projection", error))?
    {
        if existing != event_json {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn projection id collides with a different lifecycle event",
            ));
        }
        return Ok(());
    }
    tx.execute(
        "INSERT INTO logical_brain_turn_projection_outbox (
            projection_id, logical_turn_id, session_id, kind, phase,
            occurred_at, delivered_at, event_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
        params![
            event.projection_id.0,
            event.logical_turn_id.0,
            event.session_id.0,
            logical_turn_lifecycle_kind_as_str(event.kind),
            logical_turn_phase_as_str(event.phase),
            event.occurred_at,
            event_json,
        ],
    )
    .map_err(|error| persistence_error("insert logical turn outbox event", error))?;
    Ok(())
}

fn upsert_ticket(
    tx: &rusqlite::Transaction<'_>,
    ticket: &LogicalTurnContinuationTicket,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO logical_brain_turn_tickets (
            logical_turn_id, continuation_id, session_id, reason, created_at, ticket_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(logical_turn_id) DO UPDATE SET
            continuation_id = excluded.continuation_id,
            session_id = excluded.session_id,
            reason = excluded.reason,
            created_at = excluded.created_at,
            ticket_json = excluded.ticket_json",
        params![
            ticket.logical_turn_id.0,
            ticket.continuation_id.0,
            ticket.session_id.0,
            continuation_yield_reason_as_str(ticket.reason),
            ticket.created_at,
            to_json_text(ticket)?,
        ],
    )
    .map_err(|error| persistence_error("upsert logical turn ticket", error))?;
    Ok(())
}

fn update_turn(
    tx: &rusqlite::Transaction<'_>,
    record: &LogicalTurnRecord,
    expected_revision: u64,
) -> CoreResult<()> {
    let changed = tx
        .execute(
            "UPDATE logical_brain_turns
             SET phase = ?1, current_continuation_id = ?2, revision = ?3,
                 updated_at = ?4, terminal_at = ?5, record_json = ?6
             WHERE logical_turn_id = ?7 AND revision = ?8",
            params![
                logical_turn_phase_as_str(record.phase),
                record.current_continuation_id.0,
                record.revision as i64,
                record.updated_at,
                record.terminal_at,
                to_json_text(record)?,
                record.logical_turn_id.0,
                expected_revision as i64,
            ],
        )
        .map_err(|error| persistence_error("update logical turn", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "logical turn changed during atomic transition",
        ));
    }
    Ok(())
}

fn load_turn(
    conn: &rusqlite::Connection,
    logical_turn_id: &LogicalTurnId,
) -> CoreResult<Option<LogicalTurnRecord>> {
    conn.query_row(
        "SELECT record_json FROM logical_brain_turns WHERE logical_turn_id = ?1",
        params![logical_turn_id.0],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load logical turn", error))?
    .map(|raw| decode(&raw, "logical turn"))
    .transpose()
}

fn load_checkpoint(
    conn: &rusqlite::Connection,
    continuation_id: &ContinuationId,
) -> CoreResult<Option<LogicalTurnCheckpoint>> {
    conn.query_row(
        "SELECT checkpoint_json FROM logical_brain_turn_checkpoints WHERE continuation_id = ?1",
        params![continuation_id.0],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load logical turn checkpoint", error))?
    .map(|raw| decode(&raw, "logical turn checkpoint"))
    .transpose()
}

fn load_all_turns(conn: &rusqlite::Connection) -> CoreResult<Vec<LogicalTurnRecord>> {
    let mut stmt = conn
        .prepare("SELECT record_json FROM logical_brain_turns ORDER BY updated_at, logical_turn_id")
        .map_err(|error| persistence_error("prepare logical turn hydration query", error))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| persistence_error("query logical turns for hydration", error))?;
    rows.map(|row| {
        row.map_err(|error| persistence_error("read logical turn for hydration", error))
            .and_then(|raw| decode(&raw, "logical turn"))
    })
    .collect()
}

fn has_leased_host_tool(
    tx: &rusqlite::Transaction<'_>,
    logical_turn_id: &LogicalTurnId,
) -> CoreResult<bool> {
    tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM logical_brain_turn_operations
            WHERE logical_turn_id = ?1 AND kind = 'host_tool_execution' AND phase = 'leased'
         )",
        params![logical_turn_id.0],
        |row| row.get(0),
    )
    .map_err(|error| persistence_error("inspect logical turn leased tools", error))
}

pub(crate) fn lifecycle_for_record(
    record: &LogicalTurnRecord,
    progress: LogicalTurnProgress,
    kind: LogicalTurnLifecycleEventKind,
    reason_code: &str,
    summary: &str,
    occurred_at: IsoTimestamp,
) -> LogicalTurnLifecycleEvent {
    LogicalTurnLifecycleEvent {
        projection_id: rusty_crew_core_protocol::TurnProjectionId::new(format!(
            "projection:{}:{}:{}",
            record.logical_turn_id.0,
            record.revision,
            logical_turn_lifecycle_kind_as_str(kind)
        )),
        logical_turn_id: record.logical_turn_id.clone(),
        session_id: record.session_id.clone(),
        wake_id: record.source_wake_id.clone(),
        continuation_id: record.current_continuation_id.clone(),
        continuation_count: record.continuation_sequence.saturating_add(1),
        execution_epoch_id: record.active_epoch_id.clone(),
        kind,
        phase: record.phase,
        operator_state: rusty_crew_core_protocol::LogicalTurnOperatorState::for_phase(record.phase),
        progress_classification:
            rusty_crew_core_protocol::LogicalTurnProgressClassification::for_state(
                record.phase,
                record.attention.is_some(),
                &progress,
            ),
        progress,
        reason_code: reason_code.to_string(),
        summary: summary.to_string(),
        occurred_at,
        logical_turn_revision: record.revision,
    }
}

fn current_progress(
    conn: &rusqlite::Connection,
    record: &LogicalTurnRecord,
) -> CoreResult<LogicalTurnProgress> {
    load_checkpoint(conn, &record.current_continuation_id)?
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

fn decode<T: DeserializeOwned>(raw: &str, label: &str) -> CoreResult<T> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode {label}: {error}"),
        )
    })
}

fn logical_turn_insert_error(
    label: &str,
    logical_turn_id: &LogicalTurnId,
    error: rusqlite::Error,
) -> CoreError {
    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        CoreError::new(
            CoreErrorKind::AlreadyExists,
            format!("{label} {} already exists", logical_turn_id.0),
        )
    } else {
        persistence_error(&format!("insert {label}"), error)
    }
}

pub(crate) const fn logical_turn_phase_as_str(value: LogicalTurnPhase) -> &'static str {
    match value {
        LogicalTurnPhase::Admitted => "admitted",
        LogicalTurnPhase::Runnable => "runnable",
        LogicalTurnPhase::Running => "running",
        LogicalTurnPhase::Yielded => "yielded",
        LogicalTurnPhase::AttentionRequired => "attention_required",
        LogicalTurnPhase::CancelRequested => "cancel_requested",
        LogicalTurnPhase::Completed => "completed",
        LogicalTurnPhase::Cancelled => "cancelled",
        LogicalTurnPhase::Failed => "failed",
    }
}

pub(crate) const fn logical_turn_operation_kind_as_str(
    value: LogicalTurnOperationKind,
) -> &'static str {
    match value {
        LogicalTurnOperationKind::ProviderRequest => "provider_request",
        LogicalTurnOperationKind::HostToolExecution => "host_tool_execution",
    }
}

pub(crate) const fn logical_turn_operation_phase_as_str(
    value: LogicalTurnOperationPhase,
) -> &'static str {
    match value {
        LogicalTurnOperationPhase::Planned => "planned",
        LogicalTurnOperationPhase::Leased => "leased",
        LogicalTurnOperationPhase::Completed => "completed",
        LogicalTurnOperationPhase::OutcomeUnknown => "outcome_unknown",
        LogicalTurnOperationPhase::Superseded => "superseded",
        LogicalTurnOperationPhase::CompletedAfterCancel => "completed_after_cancel",
    }
}

pub(crate) const fn logical_turn_lifecycle_kind_as_str(
    value: LogicalTurnLifecycleEventKind,
) -> &'static str {
    match value {
        LogicalTurnLifecycleEventKind::Admitted => "admitted",
        LogicalTurnLifecycleEventKind::ContinuationClaimed => "continuation_claimed",
        LogicalTurnLifecycleEventKind::ContinuationProgress => "continuation_progress",
        LogicalTurnLifecycleEventKind::ContinuationCheckpointed => "continuation_checkpointed",
        LogicalTurnLifecycleEventKind::ContinuationYielded => "continuation_yielded",
        LogicalTurnLifecycleEventKind::ContinuationResumed => "continuation_resumed",
        LogicalTurnLifecycleEventKind::AttentionRequired => "attention_required",
        LogicalTurnLifecycleEventKind::RebindRequested => "rebind_requested",
        LogicalTurnLifecycleEventKind::Rebound => "rebound",
        LogicalTurnLifecycleEventKind::CancelRequested => "cancel_requested",
        LogicalTurnLifecycleEventKind::Completed => "completed",
        LogicalTurnLifecycleEventKind::Cancelled => "cancelled",
        LogicalTurnLifecycleEventKind::Failed => "failed",
    }
}

pub(crate) const fn continuation_yield_reason_as_str(
    value: ContinuationYieldReason,
) -> &'static str {
    match value {
        ContinuationYieldReason::InitialAdmission => "initial_admission",
        ContinuationYieldReason::WorkQuantumReached => "work_quantum_reached",
        ContinuationYieldReason::SchedulerFairness => "scheduler_fairness",
        ContinuationYieldReason::ProviderRetry => "provider_retry",
        ContinuationYieldReason::BufferPressure => "buffer_pressure",
        ContinuationYieldReason::RestartRecovery => "restart_recovery",
        ContinuationYieldReason::OperatorRequested => "operator_requested",
    }
}
