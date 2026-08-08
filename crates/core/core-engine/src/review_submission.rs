use super::*;
use rusty_crew_core_protocol::{
    AgentCoordinationCaller, ProjectId, ReviewSubmissionPhase, ReviewSubmissionQuery,
    ReviewSubmissionRecord, ReviewSubmissionRequest, ReviewSubmissionTransition,
    ReviewSubmissionTransitionRequest, TaskId,
};
use sha2::{Digest, Sha256};

impl CoreEngine {
    pub(crate) fn resolve_review_submission_caller(
        &self,
        submission_id: &str,
    ) -> CoreResult<(AgentId, Option<SessionId>)> {
        let record =
            load_review_submission_record(&self.store, submission_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "review_submission_caller_not_found",
                )
            })?;
        match record.phase {
            ReviewSubmissionPhase::ReviewerDispatchPending => {
                if let Some(session_id) = record.submitter_session_id.as_ref() {
                    let session = self.sessions.get_session(session_id)?;
                    if session.status == SessionStatus::Archived
                        || session.agent_id != record.submitter_agent_id
                    {
                        return Err(CoreError::new(
                            CoreErrorKind::SessionExpired,
                            "review_submission_submitter_session_changed",
                        ));
                    }
                }
                Ok((record.submitter_agent_id, record.submitter_session_id))
            }
            ReviewSubmissionPhase::ReviewerDispatched
            | ReviewSubmissionPhase::DenFinalizationPending
            | ReviewSubmissionPhase::DenFinalized
            | ReviewSubmissionPhase::ReplyPending => {
                let reviewer_session_id = record.reviewer_session_id.ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "review_submission_reviewer_session_missing",
                    )
                })?;
                let session = self.sessions.get_session(&reviewer_session_id)?;
                if session.status == SessionStatus::Archived {
                    return Err(CoreError::new(
                        CoreErrorKind::SessionExpired,
                        "review_submission_reviewer_session_archived",
                    ));
                }
                Ok((session.agent_id, Some(reviewer_session_id)))
            }
            _ => Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "review_submission_caller_not_replyable",
            )),
        }
    }

    pub fn begin_review_submission(
        &self,
        request: ReviewSubmissionRequest,
    ) -> CoreResult<ReviewSubmissionRecord> {
        let _guard = self.review_submission_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "review submission lock poisoned",
            )
        })?;
        validate_review_submission_request(&request)?;
        let (submitter_agent_id, submitter_session_id) = match &request.caller {
            AgentCoordinationCaller::ExternalCli { client_id, .. } => {
                (AgentId::new(format!("external-cli:{client_id}")), None)
            }
            _ => {
                let (agent_id, session_id, _) =
                    self.resolve_coordination_caller(&request.caller)?;
                let session_id = session_id.ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "review_submission_requires_session_caller",
                    )
                })?;
                (agent_id, Some(session_id))
            }
        };
        let submission_id = review_submission_id(
            &request.project_id,
            &request.task_id,
            &request.commit_sha,
            &request.caller,
        );
        if let Some(mut existing) = load_review_submission_record(&self.store, &submission_id)? {
            validate_duplicate_review_submission(&existing, &request)?;
            if existing.base_commit.is_none() && request.base_commit.is_some() {
                existing.base_commit = request.base_commit;
                existing.updated_at = request.now;
                existing.revision += 1;
                save_review_submission_record(&self.store, &existing)?;
            }
            return Ok(existing);
        }

        for mut existing in list_review_submission_records(&self.store)? {
            if existing.task_id == request.task_id
                && !review_submission_terminal(existing.phase)
                && !existing
                    .commit_sha
                    .eq_ignore_ascii_case(&request.commit_sha)
            {
                existing.phase = ReviewSubmissionPhase::Superseded;
                existing.terminal_reason = Some("newer_review_submission".to_string());
                existing.updated_at = request.now.clone();
                existing.revision += 1;
                save_review_submission_record(&self.store, &existing)?;
            }
        }

        let record = ReviewSubmissionRecord {
            submission_id,
            project_id: request.project_id,
            task_id: request.task_id,
            repository: request.repository,
            commit_sha: request.commit_sha.to_ascii_lowercase(),
            git_ref: request.git_ref,
            required_checks: request.required_checks,
            base_commit: request.base_commit,
            review_summary_md: request.review_summary_md,
            reviewer: request.reviewer,
            submitter_agent_id,
            submitter_session_id,
            caller: request.caller,
            phase: ReviewSubmissionPhase::Submitted,
            review_round_id: None,
            gate_id: None,
            gate_status: None,
            reviewer_session_id: None,
            dispatch_message_id: None,
            dispatch_delivery_id: None,
            review_result_digest: None,
            review_result_json: None,
            review_finalization_id: None,
            review_packet_id: None,
            review_packet_message_id: None,
            review_exact_head_commit: None,
            review_verdict: None,
            review_finding_statuses: Vec::new(),
            review_task_status: None,
            review_material_digest: None,
            reply_message_id: None,
            reply_delivery_id: None,
            reply_status: None,
            reply_reason_code: None,
            terminal_reason: None,
            last_adapter_error: None,
            created_at: request.now.clone(),
            updated_at: request.now,
            revision: 1,
        };
        save_review_submission_record(&self.store, &record)?;
        Ok(record)
    }

    pub fn transition_review_submission(
        &self,
        request: ReviewSubmissionTransitionRequest,
    ) -> CoreResult<ReviewSubmissionRecord> {
        let _guard = self.review_submission_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "review submission lock poisoned",
            )
        })?;
        let mut record = load_review_submission_record(&self.store, &request.submission_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "review_submission_not_found")
            })?;
        if record.revision != request.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "review_submission_revision_mismatch: expected {}, found {}",
                    request.expected_revision, record.revision
                ),
            ));
        }
        match request.transition {
            ReviewSubmissionTransition::DenHandoffRecorded { review_round_id } => {
                require_review_phase(&record, ReviewSubmissionPhase::Submitted)?;
                if review_round_id == 0 {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "review_round_id must be positive",
                    ));
                }
                record.review_round_id = Some(review_round_id);
                record.phase = ReviewSubmissionPhase::DenHandoffRecorded;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::GateRegistered { gate_id } => {
                require_review_phase(&record, ReviewSubmissionPhase::DenHandoffRecorded)?;
                if gate_id == 0 {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "gate_id must be positive",
                    ));
                }
                record.gate_id = Some(gate_id);
                record.phase = ReviewSubmissionPhase::GatePending;
                record.last_adapter_error = None;
                if let Some(session_id) = record.submitter_session_id.clone() {
                    save_github_gate_wait(
                        &self.store,
                        &GitHubGateWaitRecord {
                            session_id,
                            run_id: None,
                            provider_thread_id: provider_thread_id(&record.caller),
                            project_id: record.project_id.clone(),
                            task_id: record.task_id.clone(),
                            gate_id,
                            commit_sha: record.commit_sha.clone(),
                            phase: GitHubGateWaitPhase::Waiting,
                            terminal_event_id: None,
                            created_at: request.now.clone(),
                            updated_at: request.now.clone(),
                        },
                    )?;
                }
            }
            ReviewSubmissionTransition::GateTerminal {
                gate_status,
                terminal_reason,
            } => {
                require_review_phase(&record, ReviewSubmissionPhase::GatePending)?;
                if !matches!(
                    gate_status.as_str(),
                    "passed" | "failed" | "timed_out" | "superseded"
                ) || terminal_reason.trim().is_empty()
                {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "invalid Review GitHub gate terminal state",
                    ));
                }
                record.gate_status = Some(gate_status.clone());
                record.terminal_reason = Some(terminal_reason.clone());
                record.phase = if gate_status == "passed" {
                    ReviewSubmissionPhase::ReviewerDispatchPending
                } else {
                    ReviewSubmissionPhase::GateFailed
                };
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::AdapterFailed {
                reason_code,
                summary,
            } => {
                let failure = format!("{reason_code}: {summary}");
                record.last_adapter_error = Some(failure);
                if record.phase == ReviewSubmissionPhase::ReviewerDispatchPending {
                    self.schedule_review_submission_failure_wake(
                        &record,
                        &reason_code,
                        &summary,
                        &request.now,
                    )?;
                }
            }
            ReviewSubmissionTransition::ReviewerDispatched {
                reviewer_session_id,
                dispatch_message_id,
                dispatch_delivery_id,
            } => {
                require_review_phase(&record, ReviewSubmissionPhase::ReviewerDispatchPending)?;
                record.reviewer_session_id = Some(reviewer_session_id);
                record.dispatch_message_id = Some(dispatch_message_id);
                record.dispatch_delivery_id = Some(dispatch_delivery_id);
                record.phase = ReviewSubmissionPhase::ReviewerDispatched;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::DenFinalizationPending {
                result_digest,
                result_json,
            } => {
                require_review_phase(&record, ReviewSubmissionPhase::ReviewerDispatched)?;
                if result_digest.trim().is_empty() || result_json.trim().is_empty() {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "review result digest and JSON are required",
                    ));
                }
                if result_json.len() > 8192 {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "review result JSON exceeds 8192 bytes",
                    ));
                }
                record.review_result_digest = Some(result_digest);
                record.review_result_json = Some(result_json);
                record.phase = ReviewSubmissionPhase::DenFinalizationPending;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::DenFinalized {
                finalization_id,
                packet_id,
                packet_message_id,
                exact_head_commit,
                verdict,
                finding_statuses,
                task_status,
                material_digest,
            } => {
                require_review_phase(&record, ReviewSubmissionPhase::DenFinalizationPending)?;
                if finalization_id == 0
                    || packet_id == 0
                    || packet_message_id == 0
                    || exact_head_commit.trim().is_empty()
                    || verdict.trim().is_empty()
                    || task_status.trim().is_empty()
                {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "incomplete Den review finalization receipt",
                    ));
                }
                record.review_finalization_id = Some(finalization_id);
                record.review_packet_id = Some(packet_id);
                record.review_packet_message_id = Some(packet_message_id);
                record.review_exact_head_commit = Some(exact_head_commit);
                record.review_verdict = Some(verdict);
                record.review_finding_statuses = finding_statuses;
                record.review_task_status = Some(task_status);
                record.review_material_digest = material_digest;
                record.phase = ReviewSubmissionPhase::DenFinalized;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::DenAlreadyFinalized {
                review_round_id,
                exact_head_commit,
                verdict,
                terminal_reason,
            } => {
                if review_submission_terminal(record.phase) {
                    return Ok(record);
                }
                if review_round_id == 0
                    || !exact_head_commit.eq_ignore_ascii_case(&record.commit_sha)
                    || !matches!(verdict.as_str(), "looks_good" | "changes_requested")
                    || terminal_reason.trim().is_empty()
                {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "invalid already-finalized Den review state",
                    ));
                }
                record.review_round_id = Some(review_round_id);
                record.review_exact_head_commit = Some(exact_head_commit);
                record.review_verdict = Some(verdict);
                record.terminal_reason = Some(terminal_reason);
                record.phase = ReviewSubmissionPhase::ReviewTerminal;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::ReplyPending => {
                require_review_phase(&record, ReviewSubmissionPhase::DenFinalized)?;
                record.phase = ReviewSubmissionPhase::ReplyPending;
                record.reply_reason_code = None;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::ReplySent {
                reply_message_id,
                reply_delivery_id,
                reply_status,
            } => {
                require_review_phase(&record, ReviewSubmissionPhase::ReplyPending)?;
                if reply_message_id.trim().is_empty()
                    || reply_delivery_id.trim().is_empty()
                    || reply_status.trim().is_empty()
                {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "reply delivery identifiers and status are required",
                    ));
                }
                record.reply_message_id = Some(reply_message_id);
                record.reply_delivery_id = Some(reply_delivery_id);
                record.reply_status = Some(reply_status);
                record.phase = ReviewSubmissionPhase::Replied;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::ReplyTerminal { reason_code } => {
                if !matches!(
                    record.phase,
                    ReviewSubmissionPhase::DenFinalized | ReviewSubmissionPhase::ReplyPending
                ) {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!(
                            "review_submission_phase_mismatch: reply terminal cannot settle {:?}",
                            record.phase
                        ),
                    ));
                }
                record.reply_reason_code = Some(reason_code.clone());
                record.terminal_reason = Some(reason_code);
                record.phase = ReviewSubmissionPhase::ReplyTerminal;
                record.last_adapter_error = None;
            }
            ReviewSubmissionTransition::GateFailureSettled { terminal_reason }
            | ReviewSubmissionTransition::ReviewTerminal { terminal_reason } => {
                if review_submission_terminal(record.phase) {
                    return Ok(record);
                }
                record.phase = ReviewSubmissionPhase::ReviewTerminal;
                record.terminal_reason = Some(terminal_reason);
                record.last_adapter_error = None;
            }
        }
        record.updated_at = request.now;
        record.revision += 1;
        save_review_submission_record(&self.store, &record)?;
        Ok(record)
    }

    pub fn list_review_submissions(
        &self,
        query: &ReviewSubmissionQuery,
    ) -> CoreResult<Vec<ReviewSubmissionRecord>> {
        let mut records = list_review_submission_records(&self.store)?;
        records.retain(|record| {
            query
                .submission_id
                .as_ref()
                .is_none_or(|id| record.submission_id == *id)
                && query
                    .task_id
                    .as_ref()
                    .is_none_or(|task_id| record.task_id == *task_id)
                && query
                    .submitter_session_id
                    .as_ref()
                    .is_none_or(|session_id| {
                        record.submitter_session_id.as_ref() == Some(session_id)
                    })
                && query.reviewer_session_id.as_ref().is_none_or(|session_id| {
                    record.reviewer_session_id.as_ref() == Some(session_id)
                })
                && (!query.pending_only
                    || matches!(
                        record.phase,
                        ReviewSubmissionPhase::Submitted
                            | ReviewSubmissionPhase::DenHandoffRecorded
                            | ReviewSubmissionPhase::GatePending
                            | ReviewSubmissionPhase::GateFailed
                            | ReviewSubmissionPhase::ReviewerDispatchPending
                            | ReviewSubmissionPhase::ReviewerDispatched
                            | ReviewSubmissionPhase::DenFinalizationPending
                            | ReviewSubmissionPhase::DenFinalized
                            | ReviewSubmissionPhase::ReplyPending
                    ))
        });
        records.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(records)
    }

    pub(crate) fn apply_review_gate_terminal(
        &self,
        event: &GitHubGateTerminalEvent,
    ) -> CoreResult<Option<ReviewSubmissionRecord>> {
        let Some(mut record) = list_review_submission_records(&self.store)?
            .into_iter()
            .find(|record| {
                record.phase == ReviewSubmissionPhase::GatePending
                    && record.gate_id == Some(event.gate_id)
                    && record.commit_sha.eq_ignore_ascii_case(&event.commit_sha)
            })
        else {
            return Ok(None);
        };
        record.gate_status = Some(event.status.clone());
        record.terminal_reason = Some(event.terminal_reason.clone());
        record.phase = if event.status == "passed" {
            ReviewSubmissionPhase::ReviewerDispatchPending
        } else {
            ReviewSubmissionPhase::GateFailed
        };
        record.updated_at = event.completed_at.clone();
        record.revision += 1;
        save_review_submission_record(&self.store, &record)?;
        Ok(Some(record))
    }

    pub(crate) fn mark_review_reply_terminal(
        &self,
        dispatch_message_id: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        let Some(mut record) = list_review_submission_records(&self.store)?
            .into_iter()
            .find(|record| {
                record.phase == ReviewSubmissionPhase::ReviewerDispatched
                    && record.dispatch_message_id.as_deref() == Some(dispatch_message_id)
            })
        else {
            return Ok(());
        };
        record.phase = ReviewSubmissionPhase::ReviewTerminal;
        record.terminal_reason = Some("reviewer_reply_received".to_string());
        record.updated_at = now.clone();
        record.revision += 1;
        save_review_submission_record(&self.store, &record)
    }

    fn schedule_review_submission_failure_wake(
        &self,
        record: &ReviewSubmissionRecord,
        reason_code: &str,
        summary: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        let Some(submitter_session_id) = record.submitter_session_id.as_ref() else {
            return Ok(());
        };
        let session = self.sessions.get_session(submitter_session_id)?;
        if session.status == SessionStatus::Archived
            || session.agent_id != record.submitter_agent_id
        {
            return Ok(());
        }
        let state = self.body_projector.project(submitter_session_id)?;
        let ttl_ms = state.delta_policy.queued_message_ttl_ms;
        let body = serde_json::to_string(&serde_json::json!({
            "type": "review_submission_failure",
            "submissionId": record.submission_id,
            "taskId": record.task_id,
            "commitSha": record.commit_sha,
            "reasonCode": reason_code,
            "summary": summary,
        }))
        .map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("encode review submission failure: {error}"),
            )
        })?;
        self.store.save_queued_message(&QueuedMessageRecord {
            message_id: format!("review-submission-failure:{}", record.submission_id),
            owner_session_id: Some(submitter_session_id.clone()),
            owner_agent_id: record.submitter_agent_id.clone(),
            message: AgentMessage {
                from: AgentId::new("rusty-crew:review-submission"),
                to: record.submitter_agent_id.clone(),
                from_session_id: None,
                to_session_id: Some(submitter_session_id.clone()),
                body,
                correlation_id: Some(record.submission_id.clone()),
                projection: None,
            },
            source_sequence: None,
            enqueued_at: now.clone(),
            expires_at: add_millis_to_iso(now, ttl_ms as u64)?,
            ttl_ms,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        })?;
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: submitter_session_id.clone(),
        })?;
        Ok(())
    }
}

fn validate_duplicate_review_submission(
    existing: &ReviewSubmissionRecord,
    request: &ReviewSubmissionRequest,
) -> CoreResult<()> {
    let same_request = existing.project_id == request.project_id
        && existing.task_id == request.task_id
        && existing.repository == request.repository
        && existing
            .commit_sha
            .eq_ignore_ascii_case(&request.commit_sha)
        && existing.git_ref == request.git_ref
        && existing.required_checks == request.required_checks
        && existing.review_summary_md == request.review_summary_md
        && existing.reviewer == request.reviewer
        && (existing.base_commit == request.base_commit || existing.base_commit.is_none());
    if !same_request {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "review_submission_duplicate_payload_mismatch",
        ));
    }
    Ok(())
}

fn validate_review_submission_request(request: &ReviewSubmissionRequest) -> CoreResult<()> {
    let valid_repository = request
        .repository
        .split_once('/')
        .is_some_and(|(owner, repo)| {
            !owner.trim().is_empty() && !repo.trim().is_empty() && !repo.contains('/')
        });
    if request.project_id.0.trim().is_empty()
        || request.task_id.0.trim().is_empty()
        || !valid_repository
        || !crate::github_gate::valid_full_github_sha(&request.commit_sha)
        || request.git_ref.trim().is_empty()
        || request.required_checks.is_empty()
        || request
            .required_checks
            .iter()
            .any(|check| check.trim().is_empty())
        || request.review_summary_md.trim().is_empty()
        || request.review_summary_md.len() > 64 * 1024
        || !request.reviewer.starts_with('@')
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "invalid review submission request",
        ));
    }
    if request
        .base_commit
        .as_ref()
        .is_some_and(|sha| !crate::github_gate::valid_full_github_sha(sha))
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "review submission base_commit must be an exact SHA",
        ));
    }
    if let AgentCoordinationCaller::ExternalCli {
        client_id,
        idempotency_key,
    } = &request.caller
    {
        if client_id.trim().is_empty()
            || idempotency_key.trim().is_empty()
            || client_id.len() > 128
            || idempotency_key.len() > 256
            || request.reviewer != "@reviewer"
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external CLI review submissions require bounded client identity, idempotency key, and fixed @reviewer recipient",
            ));
        }
    }
    Ok(())
}

fn require_review_phase(
    record: &ReviewSubmissionRecord,
    expected: ReviewSubmissionPhase,
) -> CoreResult<()> {
    if record.phase == expected {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "review_submission_phase_mismatch: expected {expected:?}, found {:?}",
                record.phase
            ),
        ))
    }
}

fn review_submission_terminal(phase: ReviewSubmissionPhase) -> bool {
    matches!(
        phase,
        ReviewSubmissionPhase::Replied
            | ReviewSubmissionPhase::ReplyTerminal
            | ReviewSubmissionPhase::ReviewTerminal
            | ReviewSubmissionPhase::Superseded
    )
}

fn review_submission_id(
    project_id: &ProjectId,
    task_id: &TaskId,
    commit_sha: &str,
    caller: &AgentCoordinationCaller,
) -> String {
    let caller_identity = match caller {
        AgentCoordinationCaller::DirectBrain { session_id, .. } => session_id.0.clone(),
        AgentCoordinationCaller::ExternalCli {
            client_id,
            idempotency_key,
        } => format!("external-cli|{client_id}|{idempotency_key}"),
        _ => "unsupported".to_string(),
    };
    let digest = Sha256::digest(
        format!(
            "{}|{}|{}|{}",
            project_id.0,
            task_id.0,
            commit_sha.to_ascii_lowercase(),
            caller_identity,
        )
        .as_bytes(),
    );
    format!("review-submission:{digest:x}")
}

fn provider_thread_id(caller: &AgentCoordinationCaller) -> Option<String> {
    match caller {
        AgentCoordinationCaller::ExternalAgent {
            native_thread_id, ..
        } => Some(native_thread_id.clone()),
        _ => None,
    }
}
