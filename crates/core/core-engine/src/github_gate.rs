use super::*;

impl CoreEngine {
    pub fn suspend_for_github_gate(
        &self,
        request: GitHubGateSuspendRequest,
    ) -> CoreResult<GitHubGateWaitRecord> {
        let _guard = self.github_gate_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "GitHub gate lock poisoned")
        })?;
        validate_github_gate_suspend(&request)?;
        let session = self.sessions.get_session(&request.session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("session {} is archived", request.session_id),
            ));
        }
        if let Some(existing) = load_github_gate_wait(&self.store, &request.session_id)? {
            if existing.phase == GitHubGateWaitPhase::Waiting
                && existing.gate_id == request.gate_id
                && existing.commit_sha == request.commit_sha
            {
                return Ok(existing);
            }
        }
        let wait = GitHubGateWaitRecord {
            session_id: request.session_id.clone(),
            run_id: request.run_id,
            provider_thread_id: request.provider_thread_id,
            project_id: request.project_id,
            task_id: request.task_id,
            gate_id: request.gate_id,
            commit_sha: request.commit_sha.to_ascii_lowercase(),
            phase: GitHubGateWaitPhase::Waiting,
            terminal_event_id: None,
            created_at: request.now.clone(),
            updated_at: request.now.clone(),
        };
        save_github_gate_wait(&self.store, &wait)?;
        let idle = self.sessions.mark_idle(&request.session_id, request.now)?;
        save_engine_session(&self.store, &idle)?;
        Ok(wait)
    }

    pub fn consume_github_gate_terminal_event(
        &self,
        event: GitHubGateTerminalEvent,
    ) -> CoreResult<GitHubGateTerminalReceipt> {
        let _guard = self.github_gate_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "GitHub gate lock poisoned")
        })?;
        validate_github_gate_terminal_event(&event)?;
        let cursor = load_github_gate_cursor(&self.store)?;
        if event.event_id <= cursor {
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor,
                duplicate: true,
                wake_scheduled: false,
                ignored_reason: Some("event_cursor_already_consumed".to_string()),
                wait: None,
            });
        }
        let matching = list_github_gate_waits(&self.store)?
            .into_iter()
            .find(|wait| {
                wait.phase == GitHubGateWaitPhase::Waiting
                    && wait.gate_id == event.gate_id
                    && wait.commit_sha.eq_ignore_ascii_case(&event.commit_sha)
            });
        let Some(mut wait) = matching else {
            save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor: event.event_id,
                duplicate: false,
                wake_scheduled: false,
                ignored_reason: Some("no_current_wait_for_gate_and_sha".to_string()),
                wait: None,
            });
        };
        let session = self.sessions.get_session(&wait.session_id)?;
        if session.status == SessionStatus::Archived {
            wait.phase = GitHubGateWaitPhase::Cancelled;
            wait.terminal_event_id = Some(event.event_id);
            wait.updated_at = event.completed_at.clone();
            save_github_gate_wait(&self.store, &wait)?;
            save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor: event.event_id,
                duplicate: false,
                wake_scheduled: false,
                ignored_reason: Some("session_cancelled_or_archived".to_string()),
                wait: Some(wait),
            });
        }
        let review_submission = self.apply_review_gate_terminal(&event)?;
        if review_submission.is_some() && event.status == "passed" {
            wait.phase = GitHubGateWaitPhase::Consumed;
            wait.terminal_event_id = Some(event.event_id);
            wait.updated_at = event.completed_at.clone();
            save_github_gate_wait(&self.store, &wait)?;
            save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor: event.event_id,
                duplicate: false,
                wake_scheduled: false,
                ignored_reason: Some("review_submission_dispatch_pending".to_string()),
                wait: Some(wait),
            });
        }
        let result = GitHubGateWakeResult {
            event_id: event.event_id,
            gate_id: event.gate_id,
            commit_sha: event.commit_sha,
            status: event.status,
            terminal_reason: event.terminal_reason,
            summary: event.summary,
            failure_summary: event.failure_summary,
            completed_at: event.completed_at.clone(),
        };
        let body = serde_json::to_string(&serde_json::json!({
            "type": "github_gate_terminal_result",
            "result": result,
        }))
        .map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("encode GitHub gate wake result: {error}"),
            )
        })?;
        let state = self.body_projector.project(&wait.session_id)?;
        let ttl_ms = state.delta_policy.queued_message_ttl_ms;
        let message = QueuedMessageRecord {
            message_id: format!("github-gate-event:{}", event.event_id),
            owner_session_id: Some(wait.session_id.clone()),
            owner_agent_id: session.agent_id.clone(),
            message: AgentMessage {
                from: AgentId::new("rusty-crew:review-gate"),
                to: session.agent_id,
                body,
                correlation_id: Some(format!("github-gate-event:{}", event.event_id)),
                projection: None,
            },
            source_sequence: None,
            enqueued_at: event.completed_at.clone(),
            expires_at: add_millis_to_iso(&event.completed_at, ttl_ms as u64)?,
            ttl_ms,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        };
        self.store.save_queued_message(&message)?;
        wait.phase = GitHubGateWaitPhase::WakeScheduled;
        wait.terminal_event_id = Some(event.event_id);
        wait.updated_at = event.completed_at.clone();
        save_github_gate_wait(&self.store, &wait)?;
        save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: wait.session_id.clone(),
        })?;
        Ok(GitHubGateTerminalReceipt {
            event_id: event.event_id,
            cursor: event.event_id,
            duplicate: false,
            wake_scheduled: true,
            ignored_reason: None,
            wait: Some(wait),
        })
    }

    pub fn recover_github_gate_wakes(&self) -> CoreResult<u32> {
        let _guard = self.github_gate_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "GitHub gate lock poisoned")
        })?;
        let mut recovered = 0_u32;
        for wait in list_github_gate_waits(&self.store)? {
            if wait.phase != GitHubGateWaitPhase::WakeScheduled {
                continue;
            }
            let session = self.sessions.get_session(&wait.session_id)?;
            if session.status == SessionStatus::Archived {
                continue;
            }
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: wait.session_id,
            })?;
            recovered += 1;
        }
        Ok(recovered)
    }

    pub fn github_gate_wait(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<Option<GitHubGateWaitRecord>> {
        load_github_gate_wait(&self.store, session_id)
    }

    pub fn github_gate_event_cursor(&self) -> CoreResult<u64> {
        load_github_gate_cursor(&self.store)
    }
}

fn validate_github_gate_suspend(request: &GitHubGateSuspendRequest) -> CoreResult<()> {
    if request.gate_id == 0
        || request.project_id.0.trim().is_empty()
        || request.task_id.0.trim().is_empty()
        || !valid_full_github_sha(&request.commit_sha)
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "GitHub gate suspension requires gate_id, project/task, and an exact 40-character SHA",
        ));
    }
    Ok(())
}

fn validate_github_gate_terminal_event(event: &GitHubGateTerminalEvent) -> CoreResult<()> {
    let valid_status = matches!(
        event.status.as_str(),
        "passed" | "failed" | "timed_out" | "superseded"
    );
    let valid_reason = matches!(
        event.terminal_reason.as_str(),
        "checks_passed" | "checks_failed" | "required_checks_missing" | "timeout" | "superseded"
    );
    if event.event_id == 0
        || event.gate_id == 0
        || !valid_full_github_sha(&event.commit_sha)
        || !valid_status
        || !valid_reason
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "invalid Review GitHub gate terminal event",
        ));
    }
    Ok(())
}

pub(crate) fn valid_full_github_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
