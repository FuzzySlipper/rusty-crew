use super::*;

impl CoreEngine {
    pub fn admit_logical_turn(
        &self,
        write: &LogicalTurnAdmissionWrite,
    ) -> CoreResult<LogicalTurnAdmission> {
        self.ensure_logical_turn_session(&write.admission.record)?;
        let admission = self.store.insert_logical_turn_admission(write)?;
        self.flush_logical_turn_outbox()?;
        Ok(admission)
    }

    pub fn get_logical_turn(
        &self,
        logical_turn_id: &LogicalTurnId,
    ) -> CoreResult<Option<LogicalTurnRecord>> {
        self.store.get_logical_turn(logical_turn_id)
    }

    pub fn claim_logical_turn(
        &self,
        request: &LogicalTurnClaimRequest,
    ) -> CoreResult<LogicalTurnContinuationClaim> {
        let claim = self.store.claim_logical_turn(request)?;
        self.flush_logical_turn_outbox()?;
        Ok(claim)
    }

    pub fn yield_logical_turn(
        &self,
        request: &LogicalTurnYieldRequest,
    ) -> CoreResult<LogicalTurnYieldReceipt> {
        let receipt = self.store.yield_logical_turn(request)?;
        self.flush_logical_turn_outbox()?;
        if !receipt.replayed {
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: receipt.record.session_id.clone(),
            })?;
        }
        Ok(receipt)
    }

    pub fn complete_logical_turn(
        &self,
        request: &LogicalTurnCompletionRequest,
    ) -> CoreResult<LogicalTurnRecord> {
        let record = self.store.complete_logical_turn(request)?;
        self.flush_logical_turn_outbox()?;
        Ok(record)
    }

    pub fn cancel_logical_turn(
        &self,
        request: &LogicalTurnCancelRequest,
    ) -> CoreResult<LogicalTurnCancellationReceipt> {
        let receipt = self.store.cancel_logical_turn(request)?;
        self.flush_logical_turn_outbox()?;
        Ok(receipt)
    }

    pub fn insert_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        self.store.insert_logical_turn_operation(operation)
    }

    pub fn update_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
        expected_revision: u64,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        self.store
            .update_logical_turn_operation(operation, expected_revision)
    }

    pub fn logical_turn_continuation_tickets(
        &self,
    ) -> CoreResult<Vec<LogicalTurnContinuationTicket>> {
        self.store.list_logical_turn_tickets()
    }

    pub fn load_logical_turn_frozen_content(
        &self,
        content_ref: &str,
    ) -> CoreResult<Option<Vec<u8>>> {
        self.store.load_logical_turn_content(content_ref)
    }

    pub(crate) fn hydrate_logical_turn_continuations(
        &self,
    ) -> CoreResult<LogicalTurnHydrationReport> {
        let now = self.now();
        let report = self.store.hydrate_logical_turns(&now)?;
        self.flush_logical_turn_outbox()?;
        for ticket in self.store.list_logical_turn_tickets()? {
            if self
                .sessions
                .get_session(&ticket.session_id)
                .is_ok_and(|session| session.status != SessionStatus::Archived)
            {
                self.bus.publish(CoreEvent::BrainWakeRequested {
                    session_id: ticket.session_id,
                })?;
            }
        }
        Ok(report)
    }

    fn ensure_logical_turn_session(&self, record: &LogicalTurnRecord) -> CoreResult<()> {
        let session = self.sessions.get_session(&record.session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "cannot admit logical turn {} for archived session {}",
                    record.logical_turn_id.0, record.session_id.0
                ),
            ));
        }
        if session.profile_id != record.binding.profile_id {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn binding profile does not match the session profile",
            ));
        }
        Ok(())
    }

    fn flush_logical_turn_outbox(&self) -> CoreResult<()> {
        let pending: Vec<LogicalTurnOutboxRecord> =
            self.store.list_pending_logical_turn_outbox()?;
        for record in pending {
            self.bus.publish(CoreEvent::LogicalTurnLifecycleObserved {
                lifecycle: record.event.clone(),
            })?;
            self.store
                .mark_logical_turn_outbox_delivered(&record.event.projection_id.0, &self.now())?;
        }
        Ok(())
    }
}
