use super::*;
use sha2::{Digest, Sha256};

const LOGICAL_TURN_CLAIM_EXPIRES_AT: &str = "9999-12-31T23:59:59Z";

#[derive(Debug, Clone)]
pub struct LogicalTurnWakePreparation {
    pub body_state_json: Vec<u8>,
    pub system_prompt: String,
    pub role_assembly_json: Vec<u8>,
    pub continuation_state: Option<BrainContinuationPayload>,
    pub claim: LogicalTurnContinuationClaim,
}

#[derive(Debug, Clone)]
pub enum LogicalTurnEpochResult {
    Completed,
    Yielded(BrainContinuationPayload),
    Failed {
        reason_code: String,
        summary: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalTurnEpochSettlement {
    pub outcome: BrainWakeOutcome,
    pub phase: LogicalTurnPhase,
}

impl CoreEngine {
    pub fn prepare_logical_turn_wake(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        source_wake_id: &str,
        system_prompt: String,
        role_assembly_json: Vec<u8>,
    ) -> CoreResult<LogicalTurnWakePreparation> {
        let tickets = self
            .store
            .list_logical_turn_tickets()?
            .into_iter()
            .filter(|ticket| &ticket.session_id == session_id)
            .collect::<Vec<_>>();
        if tickets.len() > 1 {
            return Err(CoreError::new(
                CoreErrorKind::InternalError,
                format!(
                    "session {} has more than one runnable logical turn",
                    session_id.0
                ),
            ));
        }

        let (claim, body_state_json, system_prompt, role_assembly_json, resumed) =
            if let Some(ticket) = tickets.first() {
                let record = self
                    .store
                    .get_logical_turn(&ticket.logical_turn_id)?
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::InternalError,
                            "logical turn ticket references a missing turn",
                        )
                    })?;
                let epoch_id = execution_epoch_id(&record, self.service_instance_id());
                let claim = self.claim_logical_turn(&LogicalTurnClaimRequest {
                    logical_turn_id: record.logical_turn_id.clone(),
                    expected_revision: record.revision,
                    continuation_id: ticket.continuation_id.clone(),
                    execution_epoch_id: epoch_id,
                    claim_holder: self.service_instance_id().to_string(),
                    claim_expires_at: LOGICAL_TURN_CLAIM_EXPIRES_AT.to_string(),
                    now: self.now(),
                })?;
                let frozen = claim.checkpoint.frozen_input.clone();
                let body_state_json = self.required_logical_turn_content(&frozen.body_state_ref)?;
                let frozen_system_prompt = String::from_utf8(
                    self.required_logical_turn_content(&frozen.system_prompt_ref)?,
                )
                .map_err(|error| {
                    CoreError::new(
                        CoreErrorKind::InternalError,
                        format!("logical turn system prompt is not UTF-8: {error}"),
                    )
                })?;
                let frozen_role_assembly =
                    self.required_logical_turn_content(&frozen.role_assembly_ref)?;
                (
                    claim,
                    body_state_json,
                    frozen_system_prompt,
                    frozen_role_assembly,
                    true,
                )
            } else {
                let body_state = self.prepare_body_state_for_wake(session_id)?;
                let body_state_json = serde_json::to_vec(&body_state).map_err(|error| {
                    CoreError::new(
                        CoreErrorKind::InternalError,
                        format!("serialize logical turn body state: {error}"),
                    )
                })?;
                let admission = self.logical_turn_admission(
                    registration,
                    session_id,
                    source_wake_id,
                    &body_state_json,
                    &system_prompt,
                    &role_assembly_json,
                )?;
                let admitted = self.admit_logical_turn(&admission)?;
                let epoch_id = execution_epoch_id(&admitted.record, self.service_instance_id());
                let claim = self.claim_logical_turn(&LogicalTurnClaimRequest {
                    logical_turn_id: admitted.record.logical_turn_id.clone(),
                    expected_revision: admitted.record.revision,
                    continuation_id: admitted.record.current_continuation_id.clone(),
                    execution_epoch_id: epoch_id,
                    claim_holder: self.service_instance_id().to_string(),
                    claim_expires_at: LOGICAL_TURN_CLAIM_EXPIRES_AT.to_string(),
                    now: self.now(),
                })?;
                (
                    claim,
                    body_state_json,
                    system_prompt,
                    role_assembly_json,
                    false,
                )
            };

        let expected_module = brain_module_id(registration);
        if claim.checkpoint.module_state.module_id != expected_module {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "logical turn checkpoint belongs to brain module {}, not {}",
                    claim.checkpoint.module_state.module_id, expected_module
                ),
            ));
        }
        let continuation_state = resumed.then(|| claim.checkpoint.module_state.clone());
        Ok(LogicalTurnWakePreparation {
            body_state_json,
            system_prompt,
            role_assembly_json,
            continuation_state,
            claim,
        })
    }

    pub fn settle_logical_turn_epoch(
        &self,
        claim: &LogicalTurnContinuationClaim,
        result: LogicalTurnEpochResult,
    ) -> CoreResult<LogicalTurnEpochSettlement> {
        let now = self.now();
        let epoch_id = claim.record.active_epoch_id.clone().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical turn claim has no active execution epoch",
            )
        })?;
        match result {
            LogicalTurnEpochResult::Yielded(module_state) => {
                if module_state.module_id != claim.record.binding.brain_module_id {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "logical turn yield module does not match the bound brain module",
                    ));
                }
                let payload_fingerprint = json_fingerprint(&module_state.payload)?;
                if payload_fingerprint != module_state.payload_fingerprint {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "logical turn continuation payload fingerprint mismatch",
                    ));
                }
                let sequence = claim.record.continuation_sequence + 1;
                let continuation_id = ContinuationId::new(format!(
                    "continuation:{}",
                    sha256_hex(
                        format!(
                            "{}|{}|{}|{}",
                            claim.record.logical_turn_id.0,
                            claim.record.current_continuation_id.0,
                            sequence,
                            module_state.payload_fingerprint
                        )
                        .as_bytes()
                    )
                ));
                let mut progress = claim.checkpoint.progress.clone();
                progress.semantic_revision = progress.semantic_revision.saturating_add(1);
                progress.state_fingerprint = module_state.payload_fingerprint.clone();
                progress.last_liveness_at = now.clone();
                progress.last_semantic_progress_at = now.clone();
                progress.consecutive_no_progress_samples = 0;
                let checkpoint = LogicalTurnCheckpoint {
                    continuation_id: continuation_id.clone(),
                    logical_turn_id: claim.record.logical_turn_id.clone(),
                    sequence,
                    parent_continuation_id: Some(claim.record.current_continuation_id.clone()),
                    completed_epoch_id: Some(epoch_id.clone()),
                    binding_generation: claim.record.binding_generation,
                    frozen_input: claim.checkpoint.frozen_input.clone(),
                    module_state,
                    operation_cursor: claim.checkpoint.operation_cursor,
                    projection_cursor: claim.checkpoint.projection_cursor.saturating_add(1),
                    progress: progress.clone(),
                    yield_reason: ContinuationYieldReason::WorkQuantumReached,
                    created_at: now.clone(),
                };
                let receipt = self.yield_logical_turn(&LogicalTurnYieldRequest {
                    logical_turn_id: claim.record.logical_turn_id.clone(),
                    expected_revision: claim.record.revision,
                    expected_epoch_id: epoch_id.clone(),
                    expected_claim_generation: claim.claim_generation,
                    expected_cancellation_generation: claim.record.cancellation_generation,
                    checkpoint: checkpoint.clone(),
                    lifecycle_event: lifecycle_event(
                        &claim.record,
                        LifecycleEventInput {
                            continuation_id,
                            execution_epoch_id: Some(epoch_id),
                            kind: LogicalTurnLifecycleEventKind::ContinuationYielded,
                            phase: LogicalTurnPhase::Yielded,
                            progress,
                            reason_code: "work_quantum_reached",
                            summary:
                                "logical turn reached its scheduling quantum and will continue",
                            occurred_at: now.clone(),
                        },
                    ),
                    now,
                })?;
                Ok(LogicalTurnEpochSettlement {
                    outcome: BrainWakeOutcome::Continuing,
                    phase: receipt.record.phase,
                })
            }
            LogicalTurnEpochResult::Completed => self.finish_logical_turn_epoch(
                claim,
                epoch_id,
                LogicalTurnLifecycleEventKind::Completed,
                LogicalTurnPhase::Completed,
                "completed",
                "logical turn completed",
                now,
            ),
            LogicalTurnEpochResult::Failed {
                reason_code,
                summary,
            } => self.finish_logical_turn_epoch(
                claim,
                epoch_id,
                LogicalTurnLifecycleEventKind::Failed,
                LogicalTurnPhase::Failed,
                &reason_code,
                &summary,
                now,
            ),
        }
    }

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

    fn required_logical_turn_content(&self, content_ref: &str) -> CoreResult<Vec<u8>> {
        self.load_logical_turn_frozen_content(content_ref)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    format!("logical turn frozen content {content_ref} is missing"),
                )
            })
    }

    fn logical_turn_admission(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        source_wake_id: &str,
        body_state_json: &[u8],
        system_prompt: &str,
        role_assembly_json: &[u8],
    ) -> CoreResult<LogicalTurnAdmissionWrite> {
        let now = self.now();
        let logical_turn_id = LogicalTurnId::new(format!(
            "turn:{}",
            sha256_hex(format!("{}|{source_wake_id}", session_id.0).as_bytes())
        ));
        let continuation_id = ContinuationId::new(format!(
            "continuation:{}",
            sha256_hex(format!("{}|0", logical_turn_id.0).as_bytes())
        ));
        let body_fingerprint = sha256_hex(body_state_json);
        let prompt_fingerprint = sha256_hex(system_prompt.as_bytes());
        let role_fingerprint = sha256_hex(role_assembly_json);
        let frozen_input = LogicalTurnFrozenInput {
            body_state_ref: format!("sha256:{body_fingerprint}"),
            body_state_fingerprint: body_fingerprint.clone(),
            system_prompt_ref: format!("sha256:{prompt_fingerprint}"),
            system_prompt_fingerprint: prompt_fingerprint.clone(),
            role_assembly_ref: format!("sha256:{role_fingerprint}"),
            role_assembly_fingerprint: role_fingerprint.clone(),
            transcript_cursor: 0,
            attachment_refs: Vec::new(),
        };
        let binding =
            self.logical_turn_binding(registration, &prompt_fingerprint, &role_fingerprint)?;
        let initial_payload = serde_json::json!({});
        let initial_payload_fingerprint = json_fingerprint(&initial_payload)?;
        let progress = LogicalTurnProgress {
            state_fingerprint: initial_payload_fingerprint.clone(),
            last_liveness_at: now.clone(),
            last_semantic_progress_at: now.clone(),
            ..LogicalTurnProgress::default()
        };
        let record = LogicalTurnRecord {
            logical_turn_id: logical_turn_id.clone(),
            session_id: session_id.clone(),
            source_wake_id: source_wake_id.to_string(),
            phase: LogicalTurnPhase::Admitted,
            binding,
            current_continuation_id: continuation_id.clone(),
            continuation_sequence: 0,
            binding_generation: 1,
            cancellation_generation: 0,
            active_epoch_id: None,
            claim_generation: None,
            claim_holder: None,
            claim_expires_at: None,
            attention: None,
            revision: 1,
            admitted_at: now.clone(),
            updated_at: now.clone(),
            terminal_at: None,
        };
        let checkpoint = LogicalTurnCheckpoint {
            continuation_id: continuation_id.clone(),
            logical_turn_id: logical_turn_id.clone(),
            sequence: 0,
            parent_continuation_id: None,
            completed_epoch_id: None,
            binding_generation: 1,
            frozen_input,
            module_state: BrainContinuationPayload {
                module_id: brain_module_id(registration),
                payload_version: "1".to_string(),
                payload_fingerprint: initial_payload_fingerprint,
                payload: initial_payload,
            },
            operation_cursor: 0,
            projection_cursor: 0,
            progress: progress.clone(),
            yield_reason: ContinuationYieldReason::InitialAdmission,
            created_at: now.clone(),
        };
        let lifecycle_event = lifecycle_event(
            &record,
            LifecycleEventInput {
                continuation_id,
                execution_epoch_id: None,
                kind: LogicalTurnLifecycleEventKind::Admitted,
                phase: LogicalTurnPhase::Admitted,
                progress,
                reason_code: "initial_admission",
                summary: "logical turn admitted with frozen input",
                occurred_at: now.clone(),
            },
        );
        Ok(LogicalTurnAdmissionWrite {
            admission: LogicalTurnAdmission {
                record,
                initial_checkpoint: checkpoint,
                lifecycle_event,
            },
            frozen_content: vec![
                logical_turn_content(body_state_json, &body_fingerprint, "body_state", &now),
                logical_turn_content(
                    system_prompt.as_bytes(),
                    &prompt_fingerprint,
                    "system_prompt",
                    &now,
                ),
                logical_turn_content(role_assembly_json, &role_fingerprint, "role_assembly", &now),
            ],
        })
    }

    fn logical_turn_binding(
        &self,
        registration: &BrainImplementationRegistration,
        prompt_fingerprint: &str,
        role_fingerprint: &str,
    ) -> CoreResult<LogicalTurnBindingSnapshot> {
        let profile = self.get_profile_registry_record(&registration.profile_id)?;
        let provider_alias = profile
            .as_ref()
            .and_then(profile_provider_alias)
            .unwrap_or_else(|| registration.model_config.provider.clone());
        let provider = self.get_model_provider(&provider_alias)?;
        let credential_revision = match provider
            .as_ref()
            .and_then(|provider| provider.credential_id.as_deref())
        {
            Some(credential_id) => self
                .get_service_credential(credential_id)?
                .map(|credential| credential.revision),
            None => None,
        };
        let tool_fingerprint = json_fingerprint(
            &serde_json::to_value(&registration.tool_profile).map_err(|error| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    format!("serialize logical turn tool profile: {error}"),
                )
            })?,
        )?;
        let profile_fingerprint = registration
            .provider_state_scope
            .as_ref()
            .map(|scope| scope.profile_fingerprint.clone())
            .unwrap_or_else(|| {
                sha256_hex(format!("{prompt_fingerprint}|{role_fingerprint}").as_bytes())
            });
        let provider_fingerprint = registration
            .provider_state_scope
            .as_ref()
            .map(|scope| scope.provider_fingerprint.clone())
            .or_else(|| {
                provider.as_ref().and_then(|provider| {
                    serde_json::to_vec(provider)
                        .ok()
                        .map(|bytes| sha256_hex(&bytes))
                })
            })
            .unwrap_or_else(|| {
                sha256_hex(
                    format!(
                        "{}|{}",
                        registration.model_config.provider, registration.model_config.model_name
                    )
                    .as_bytes(),
                )
            });
        Ok(LogicalTurnBindingSnapshot {
            profile_id: registration.profile_id.clone(),
            profile_revision: profile.as_ref().map_or(0, |profile| profile.revision),
            prompt_fingerprint: profile_fingerprint,
            tool_selection_fingerprint: tool_fingerprint.clone(),
            tool_registry_revision: tool_fingerprint,
            brain_module_id: brain_module_id(registration),
            brain_strategy_id: registration
                .strategy
                .as_ref()
                .map(|strategy| strategy.strategy_id.clone())
                .unwrap_or_else(|| "default".to_string()),
            provider_alias,
            provider_revision: provider.as_ref().map_or(0, |provider| provider.revision),
            provider_fingerprint,
            credential_id: provider
                .as_ref()
                .and_then(|provider| provider.credential_id.clone()),
            credential_revision,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_logical_turn_epoch(
        &self,
        claim: &LogicalTurnContinuationClaim,
        epoch_id: ExecutionEpochId,
        kind: LogicalTurnLifecycleEventKind,
        phase: LogicalTurnPhase,
        reason_code: &str,
        summary: &str,
        now: IsoTimestamp,
    ) -> CoreResult<LogicalTurnEpochSettlement> {
        let record = self.complete_logical_turn(&LogicalTurnCompletionRequest {
            logical_turn_id: claim.record.logical_turn_id.clone(),
            expected_revision: claim.record.revision,
            expected_epoch_id: epoch_id.clone(),
            expected_claim_generation: claim.claim_generation,
            expected_cancellation_generation: claim.record.cancellation_generation,
            lifecycle_event: lifecycle_event(
                &claim.record,
                LifecycleEventInput {
                    continuation_id: claim.record.current_continuation_id.clone(),
                    execution_epoch_id: Some(epoch_id),
                    kind,
                    phase,
                    progress: claim.checkpoint.progress.clone(),
                    reason_code,
                    summary,
                    occurred_at: now.clone(),
                },
            ),
            now,
        })?;
        Ok(LogicalTurnEpochSettlement {
            outcome: BrainWakeOutcome::Completed,
            phase: record.phase,
        })
    }
}

struct LifecycleEventInput<'a> {
    continuation_id: ContinuationId,
    execution_epoch_id: Option<ExecutionEpochId>,
    kind: LogicalTurnLifecycleEventKind,
    phase: LogicalTurnPhase,
    progress: LogicalTurnProgress,
    reason_code: &'a str,
    summary: &'a str,
    occurred_at: IsoTimestamp,
}

fn lifecycle_event(
    record: &LogicalTurnRecord,
    input: LifecycleEventInput<'_>,
) -> LogicalTurnLifecycleEvent {
    let revision = if input.kind == LogicalTurnLifecycleEventKind::Admitted {
        record.revision
    } else {
        record.revision.saturating_add(1)
    };
    LogicalTurnLifecycleEvent {
        projection_id: TurnProjectionId::new(format!(
            "projection:{}:{}:{reason_code}",
            record.logical_turn_id.0,
            revision,
            reason_code = input.reason_code
        )),
        logical_turn_id: record.logical_turn_id.clone(),
        session_id: record.session_id.clone(),
        wake_id: record.source_wake_id.clone(),
        continuation_id: input.continuation_id,
        execution_epoch_id: input.execution_epoch_id,
        kind: input.kind,
        phase: input.phase,
        progress: input.progress,
        reason_code: input.reason_code.to_string(),
        summary: input.summary.to_string(),
        occurred_at: input.occurred_at,
        logical_turn_revision: revision,
    }
}

fn logical_turn_content(
    content: &[u8],
    fingerprint: &str,
    content_kind: &str,
    created_at: &str,
) -> LogicalTurnContentWrite {
    LogicalTurnContentWrite {
        content_ref: format!("sha256:{fingerprint}"),
        fingerprint: fingerprint.to_string(),
        content_kind: content_kind.to_string(),
        content: content.to_vec(),
        created_at: created_at.to_string(),
    }
}

fn execution_epoch_id(record: &LogicalTurnRecord, service_instance_id: &str) -> ExecutionEpochId {
    ExecutionEpochId::new(format!(
        "epoch:{}",
        sha256_hex(
            format!(
                "{}|{}|{}|{}",
                record.logical_turn_id.0,
                record.current_continuation_id.0,
                record.revision,
                service_instance_id
            )
            .as_bytes()
        )
    ))
}

fn brain_module_id(registration: &BrainImplementationRegistration) -> String {
    registration
        .strategy
        .as_ref()
        .map(|strategy| strategy.module_id.clone())
        .unwrap_or_else(|| registration.implementation_id.0.clone())
}

fn profile_provider_alias(profile: &ProfileRegistryRecord) -> Option<String> {
    profile
        .active_runtime_settings_json
        .get("providerAlias")
        .or_else(|| profile.active_runtime_settings_json.get("provider_alias"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn json_fingerprint(value: &serde_json::Value) -> CoreResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("fingerprint logical turn JSON: {error}"),
            )
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
