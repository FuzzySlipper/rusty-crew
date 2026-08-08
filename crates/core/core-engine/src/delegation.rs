use super::*;

#[derive(Debug, Default)]
struct FanOutValidationGroup {
    indexes: Vec<u32>,
    max_concurrency: Option<u32>,
    failure_policy: Option<FanOutFailurePolicy>,
}

impl CoreEngine {
    pub fn request_delegated_checkpoint(
        &self,
        parent_session_id: &SessionId,
        delegated_session_id: &SessionId,
        reason: impl Into<String>,
    ) -> CoreResult<EventReceipt> {
        let parent = self.sessions.get_session(parent_session_id)?;
        let delegated = self.sessions.get_session(delegated_session_id)?;
        if delegated.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("delegated session {} is archived", delegated.session_id),
            ));
        }
        let lineage = delegated.delegation.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session {} is not delegated", delegated.session_id),
            )
        })?;
        if &lineage.parent_session_id != parent_session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "delegated session {} does not belong to parent {}",
                    delegated.session_id, parent_session_id
                ),
            ));
        }

        let receipt = self.route_agent_message(AgentMessage {
            from: parent.agent_id,
            to: delegated.agent_id.clone(),
            from_session_id: Some(parent.session_id),
            to_session_id: Some(delegated.session_id.clone()),
            body: format!("Checkpoint requested: {}", reason.into()),
            correlation_id: Some(format!("checkpoint:{}", delegated.session_id)),
            projection: None,
        })?;
        update_delegated_worker_run_status_by_session(
            &self.store,
            &delegated.session_id,
            WorkerRunStatus::CheckpointWaiting,
            self.now(),
        )?;
        self.publish_delegation_lifecycle(
            &delegated,
            Some(lineage.source_wake_id.as_str()),
            lineage.source_action_index,
            DelegationLifecyclePhase::CheckpointRequested,
            None,
        )?;
        Ok(receipt)
    }

    pub fn cancel_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<SessionState> {
        let delegated = self.sessions.get_session(delegated_session_id)?;
        if delegated.kind != SessionKind::Delegated {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session {} is not delegated", delegated.session_id),
            ));
        }
        self.archive_delegated_session_if_nonterminal(&delegated, WorkerRunStatus::Cancelled)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::SessionExpired,
                    format!(
                        "delegated session {} is already terminal",
                        delegated.session_id
                    ),
                )
            })
    }

    pub fn drain_delegated_sessions(
        &self,
        parent_session_id: Option<&SessionId>,
    ) -> CoreResult<Vec<SessionId>> {
        let sessions = match parent_session_id {
            Some(parent_session_id) => self
                .sessions
                .delegated_sessions_for_parent(parent_session_id)?,
            None => self.sessions.all_sessions()?,
        };
        let mut drained = Vec::new();
        for session in sessions {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            if let Some(archived) =
                self.archive_delegated_session_if_nonterminal(&session, WorkerRunStatus::Cancelled)?
            {
                drained.push(archived.session_id);
            }
        }
        Ok(drained)
    }

    pub fn delegated_session_status(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<DelegatedSessionRuntimeStatus> {
        let session = self.sessions.get_session(delegated_session_id)?;
        if session.kind != SessionKind::Delegated {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session {} is not delegated", session.session_id),
            ));
        }
        let run = load_delegated_worker_run_by_session(&self.store, delegated_session_id)?;
        Ok(DelegatedSessionRuntimeStatus {
            parent_session_id: session
                .delegation
                .as_ref()
                .map(|lineage| lineage.parent_session_id.clone()),
            session,
            run_id: run.as_ref().map(|run| run.run_id.clone()),
            run_status: run.as_ref().map(|run| delegated_run_status(run.status)),
            terminal: run.as_ref().is_some_and(|run| run.status.is_terminal()),
        })
    }

    pub fn expire_delegated_sessions(&self) -> CoreResult<Vec<SessionId>> {
        self.expire_delegated_sessions_at(self.now())
    }

    pub fn cleanup_delegated_resources(&self) -> CoreResult<DelegatedResourceCleanupReport> {
        let cleaned_at = self.now();
        let terminal_archived = self.archive_terminal_delegated_sessions()?;
        let orphaned_archived = self.cleanup_orphaned_delegated_sessions()?;
        let expired_archived = self.expire_delegated_sessions_at(cleaned_at.clone())?;
        Ok(DelegatedResourceCleanupReport {
            cleaned_at,
            resources_released: 0,
            terminal_archived,
            orphaned_archived,
            expired_archived,
        })
    }

    pub fn expire_delegated_sessions_at(&self, now: IsoTimestamp) -> CoreResult<Vec<SessionId>> {
        let now_time = parse_rfc3339(&now)?;
        let mut expired = Vec::new();
        for session in self.sessions.all_sessions()? {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            let Some(max_duration_ms) = session.resource_limits.max_duration_ms else {
                continue;
            };
            let created_at = parse_rfc3339(&session.created_at)?;
            if now_time - created_at < Duration::milliseconds(max_duration_ms.into()) {
                continue;
            }
            if let Some(archived) =
                self.archive_delegated_session_if_nonterminal(&session, WorkerRunStatus::Expired)?
            {
                expired.push(archived.session_id);
            }
        }
        Ok(expired)
    }

    pub fn delegated_sessions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<SessionState>> {
        self.sessions
            .delegated_sessions_for_parent(parent_session_id)
    }

    pub fn delegated_session_for_run(&self, run_id: &RunId) -> CoreResult<Option<SessionState>> {
        let Some(run) = load_delegated_worker_run(&self.store, run_id)? else {
            return Ok(None);
        };
        if let Some(session_id) = run.delegated_session_id {
            return match self.sessions.get_session(&session_id) {
                Ok(session) => Ok(Some(session)),
                Err(error) if error.kind == CoreErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            };
        }

        self.sessions.delegated_session_for_source(
            &run.parent_session_id,
            &run.source_wake_id,
            run.source_action_index,
        )
    }

    pub(crate) fn spawn_delegated_workers(
        &self,
        parent: &SessionState,
        batch: &BrainActionBatch,
    ) -> CoreResult<()> {
        for (index, action) in batch.actions.iter().enumerate() {
            let BrainAction::RequestDelegation {
                profile_id,
                task_id,
                prompt,
                resource_limits,
                correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy,
                capacity_request,
                ..
            } = action
            else {
                continue;
            };

            let run_id = RunId::new(format!("{}:{index}", batch.wake_id));
            if load_delegated_worker_run(&self.store, &run_id)?.is_some() {
                continue;
            }

            let pooled_claim = match self.prepare_worker_pool_claim(WorkerPoolDelegationInput {
                request: capacity_request.as_ref(),
                run_id: &run_id,
                profile_id,
                task_id: task_id.as_ref(),
                prompt,
                wake_id: &batch.wake_id,
                action_index: index as u32,
            })? {
                WorkerPoolDelegationPlan::Direct => None,
                WorkerPoolDelegationPlan::Claimed(claim) => Some(*claim),
                WorkerPoolDelegationPlan::Rejected(reason) => {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!(
                            "worker-pool capacity unavailable for action {index}: {}",
                            worker_pool_no_capacity_reason_as_str(reason)
                        ),
                    ));
                }
            };

            let session_id = delegated_session_id(&batch.session_id, &batch.wake_id, index);
            let agent_id = delegated_agent_id(&session_id);
            let correlation_id = correlation_id
                .clone()
                .unwrap_or_else(|| format!("delegation:{}:{index}", batch.wake_id));
            let lineage = DelegationLineage {
                parent_session_id: parent.session_id.clone(),
                parent_agent_id: parent.agent_id.clone(),
                source_wake_id: batch.wake_id.clone(),
                source_action_index: index as u32,
                requested_task_id: task_id.clone(),
                correlation_id: correlation_id.clone(),
            };
            let delegated_resource_limits = resource_limits.clone().unwrap_or(ResourceLimits {
                workdir: None,
                max_duration_ms: None,
                max_delegation_depth: Some(0),
            });
            let workspace = delegated_resource_limits
                .workdir
                .as_ref()
                .map(|cwd| SessionWorkspace {
                    cwd: cwd.clone(),
                    revision: 1,
                    updated_at: self.now(),
                })
                .or_else(|| parent.workspace.clone());
            let config = SessionConfig {
                session_id: session_id.clone(),
                agent_id: agent_id.clone(),
                profile_id: profile_id.clone(),
                kind: SessionKind::Delegated,
                delegation: Some(lineage.clone()),
                workspace,
                resource_limits: delegated_resource_limits,
                tool_profile: self.tool_profile_for_profile(profile_id)?,
                history_window: parent.history_window.clone(),
            };
            let state = self.sessions.create_session(config.clone(), self.now())?;
            save_delegated_worker_run_requested(
                &self.store,
                &WorkerRunRecord {
                    run_id,
                    parent_session_id: parent.session_id.clone(),
                    delegated_session_id: Some(state.session_id.clone()),
                    parent_agent_id: Some(parent.agent_id.clone()),
                    profile_id: profile_id.clone(),
                    task_id: task_id.clone(),
                    status: WorkerRunStatus::Requested,
                    created_at: state.created_at.clone(),
                    last_updated_at: state.created_at.clone(),
                    source_wake_id: batch.wake_id.clone(),
                    source_action_index: index as u32,
                    delegation_correlation_id: Some(correlation_id.clone()),
                    parent_consumption: parent_consumption
                        .clone()
                        .unwrap_or(ParentConsumptionPolicy::AwaitCompletion),
                    fan_out_group_id: fan_out_group_id.clone(),
                    fan_out_max_concurrency: *fan_out_max_concurrency,
                    fan_out_failure_policy: fan_out_failure_policy
                        .clone()
                        .unwrap_or(FanOutFailurePolicy::FailSoft),
                    worker_pool_work_item_id: pooled_claim
                        .as_ref()
                        .map(|claim| claim.work_item.work_item_id.clone()),
                    worker_pool_lease_id: pooled_claim
                        .as_ref()
                        .map(|claim| claim.lease.lease_id.clone()),
                    worker_pool_member_id: pooled_claim
                        .as_ref()
                        .map(|claim| claim.member.member_id.clone()),
                    worker_pool_claim_token: pooled_claim
                        .as_ref()
                        .map(|claim| claim.lease.claim_token.clone()),
                },
            )?;
            save_engine_session_with_config(&self.store, &state, &config)?;
            update_delegated_worker_run_status_by_session(
                &self.store,
                &state.session_id,
                WorkerRunStatus::SessionCreated,
                self.now(),
            )?;
            self.bus.publish(CoreEvent::SessionCreated {
                state: Box::new(state.clone()),
            })?;
            self.publish_delegation_lifecycle(
                &state,
                Some(&batch.wake_id),
                index as u32,
                DelegationLifecyclePhase::Created,
                None,
            )?;
            self.bus.publish(CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: parent.agent_id.clone(),
                    to: agent_id,
                    from_session_id: Some(parent.session_id.clone()),
                    to_session_id: Some(state.session_id.clone()),
                    body: prompt.clone(),
                    correlation_id: Some(correlation_id),
                    projection: None,
                },
            })?;
            if session_kind_can_wake(&state.kind) {
                self.bus.publish(CoreEvent::BrainWakeRequested {
                    session_id: state.session_id.clone(),
                })?;
                update_delegated_worker_run_status_by_session(
                    &self.store,
                    &state.session_id,
                    WorkerRunStatus::WakeRequested,
                    self.now(),
                )?;
                self.publish_delegation_lifecycle(
                    &state,
                    Some(&batch.wake_id),
                    index as u32,
                    DelegationLifecyclePhase::WakeRequested,
                    None,
                )?;
            }
        }

        Ok(())
    }

    fn prepare_worker_pool_claim(
        &self,
        input: WorkerPoolDelegationInput<'_>,
    ) -> CoreResult<WorkerPoolDelegationPlan> {
        let Some(request) = input.request else {
            return Ok(WorkerPoolDelegationPlan::Direct);
        };
        if request.member_id.trim().is_empty() {
            return Ok(WorkerPoolDelegationPlan::Rejected(
                WorkerPoolNoCapacityReason::MemberUnavailable,
            ));
        }
        let now = self.now();
        let Some(member) = load_worker_pool_member(&self.store, &request.member_id)? else {
            return Ok(self.worker_pool_no_capacity_plan(
                request,
                WorkerPoolNoCapacityReason::MemberUnavailable,
            ));
        };
        if !matches!(
            member.status,
            WorkerPoolMemberStatus::Available | WorkerPoolMemberStatus::Busy
        ) {
            return Ok(self.worker_pool_no_capacity_plan(
                request,
                WorkerPoolNoCapacityReason::MemberUnavailable,
            ));
        }
        if member.profile_id != *input.profile_id {
            return Ok(self.worker_pool_no_capacity_plan(
                request,
                WorkerPoolNoCapacityReason::MemberUnavailable,
            ));
        }
        if member.last_heartbeat_at > now {
            return Ok(self.worker_pool_no_capacity_plan(
                request,
                WorkerPoolNoCapacityReason::MemberHeartbeatStale,
            ));
        }
        if member.active_leases >= member.concurrency_limit {
            return Ok(self.worker_pool_no_capacity_plan(
                request,
                WorkerPoolNoCapacityReason::MemberAtCapacity,
            ));
        }

        let claim_ttl_ms = request.claim_ttl_ms.unwrap_or(300_000).max(1);
        let claim_deadline_at = add_millis_to_iso(&now, u64::from(claim_ttl_ms))?;
        let work_item = WorkerPoolWorkItemRecord {
            work_item_id: input.run_id.0.clone(),
            requested_profile_id: Some(input.profile_id.clone()),
            task_id: input.task_id.cloned(),
            status: WorkerPoolWorkStatus::Pending,
            priority: 100,
            work_json: serde_json::json!({
                "kind": "delegation_request",
                "wake_id": input.wake_id,
                "action_index": input.action_index,
                "prompt": input.prompt,
            }),
            required_capabilities_json: serde_json::json!({
                "profile_id": input.profile_id.0,
            }),
            created_at: now.clone(),
            updated_at: now.clone(),
            claimed_by_member_id: None,
            lease_id: None,
            claim_token: None,
            claim_deadline_at: None,
            terminal_at: None,
            terminal_summary: None,
        };
        create_worker_pool_work_item(&self.store, &work_item)?;
        let claim = claim_next_worker_pool_work_item(
            &self.store,
            &WorkerPoolClaimRequest {
                member_id: request.member_id.clone(),
                lease_id: format!("lease:{}", input.run_id.0),
                claim_token: format!("claim:{}:{}", input.wake_id, input.action_index),
                now,
                claim_deadline_at,
                min_heartbeat_at: member.last_heartbeat_at,
            },
        )?;
        Ok(match claim {
            Ok(claim) => WorkerPoolDelegationPlan::Claimed(Box::new(claim)),
            Err(reason) => self.worker_pool_no_capacity_plan(request, reason),
        })
    }

    fn worker_pool_no_capacity_plan(
        &self,
        request: &WorkerPoolCapacityRequest,
        reason: WorkerPoolNoCapacityReason,
    ) -> WorkerPoolDelegationPlan {
        if request.fallback_policy == WorkerPoolCapacityFallbackPolicy::DirectOnNoCapacity {
            WorkerPoolDelegationPlan::Direct
        } else {
            WorkerPoolDelegationPlan::Rejected(reason)
        }
    }

    pub(crate) fn cancel_delegated_children_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<()> {
        for child in self
            .sessions
            .delegated_sessions_for_parent(parent_session_id)?
        {
            let _ =
                self.archive_delegated_session_if_nonterminal(&child, WorkerRunStatus::Cancelled)?;
        }
        Ok(())
    }

    pub(crate) fn cleanup_orphaned_delegated_sessions(&self) -> CoreResult<Vec<SessionId>> {
        let mut cleaned = Vec::new();
        for session in self.sessions.all_sessions()? {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            let Some(lineage) = &session.delegation else {
                if let Some(archived) = self
                    .archive_delegated_session_if_nonterminal(&session, WorkerRunStatus::Expired)?
                {
                    cleaned.push(archived.session_id);
                }
                continue;
            };
            let parent = self.sessions.get_session(&lineage.parent_session_id);
            match parent {
                Ok(parent) if parent.status != SessionStatus::Archived => {}
                Ok(_) => {
                    if let Some(archived) = self.archive_delegated_session_if_nonterminal(
                        &session,
                        WorkerRunStatus::Cancelled,
                    )? {
                        cleaned.push(archived.session_id);
                    }
                }
                Err(error) if error.kind == CoreErrorKind::NotFound => {
                    if let Some(archived) = self.archive_delegated_session_if_nonterminal(
                        &session,
                        WorkerRunStatus::Expired,
                    )? {
                        cleaned.push(archived.session_id);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(cleaned)
    }

    fn archive_terminal_delegated_sessions(&self) -> CoreResult<Vec<SessionId>> {
        let mut archived = Vec::new();
        for session in self.sessions.all_sessions()? {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            let Some(run) = load_delegated_worker_run_by_session(&self.store, &session.session_id)?
            else {
                continue;
            };
            if !run.status.is_terminal() {
                continue;
            }
            let archived_session = self
                .sessions
                .archive_session(&session.session_id, self.now())?;
            save_engine_session(&self.store, &archived_session)?;
            self.bus.publish(CoreEvent::SessionArchived {
                session_id: archived_session.session_id.clone(),
            })?;
            self.publish_delegation_lifecycle(
                &archived_session,
                Some(&run.source_wake_id),
                run.source_action_index,
                delegation_phase_for_worker_status(run.status),
                Some("cleanup archived terminal delegated session".to_string()),
            )?;
            archived.push(archived_session.session_id);
        }
        Ok(archived)
    }

    fn archive_delegated_session_if_nonterminal(
        &self,
        session: &SessionState,
        status: WorkerRunStatus,
    ) -> CoreResult<Option<SessionState>> {
        if session.kind != SessionKind::Delegated {
            return Ok(None);
        }
        let run = load_delegated_worker_run_by_session(&self.store, &session.session_id)?;
        if run.as_ref().is_some_and(|run| run.status.is_terminal()) {
            return Ok(None);
        }
        let archived = self
            .sessions
            .archive_session(&session.session_id, self.now())?;
        save_engine_session(&self.store, &archived)?;
        if let Some(run) = &run {
            update_delegated_worker_run_status(&self.store, &run.run_id, status, self.now())?;
        }
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: archived.session_id.clone(),
        })?;
        self.publish_delegation_lifecycle(
            &archived,
            run.as_ref().map(|run| run.source_wake_id.as_str()),
            run.as_ref().map_or(0, |run| run.source_action_index),
            delegation_phase_for_worker_status(status),
            None,
        )?;
        Ok(Some(archived))
    }

    fn tool_profile_for_profile(&self, profile_id: &ProfileId) -> CoreResult<ToolProfile> {
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile registry lock poisoned",
                )
            })?
            .get(profile_id)
            .cloned()
            .ok_or_else(|| unregistered_delegation_profile_error(profile_id))
    }

    pub(crate) fn validate_delegation_invariants(
        &self,
        session: &SessionState,
        batch: &BrainActionBatch,
    ) -> Vec<ActionRejection> {
        let mut rejections = Vec::new();
        for (index, action) in batch.actions.iter().enumerate() {
            match action {
                BrainAction::RequestDelegation { profile_id, .. } => {
                    if session.resource_limits.max_delegation_depth == Some(0) {
                        rejections.push(ActionRejection {
                            index: index as u32,
                            kind: CoreErrorKind::ActionRejected,
                            message: "request_delegation exceeds max_delegation_depth".to_string(),
                        });
                    } else {
                        match self.tool_profile_for_profile(profile_id) {
                            Ok(_) => {}
                            Err(error) => rejections.push(ActionRejection {
                                index: index as u32,
                                kind: error.kind,
                                message: error.message,
                            }),
                        }
                    }
                }
                BrainAction::DeliverCompletion { packet } => {
                    match load_delegated_worker_run_by_session(&self.store, &packet.session_id) {
                        Ok(Some(run)) if run.status.is_terminal() => {
                            rejections.push(ActionRejection {
                                index: index as u32,
                                kind: CoreErrorKind::ActionRejected,
                                message: format!(
                                    "completion packet for delegated session {} rejected because worker run {} is already terminal",
                                    packet.session_id, run.run_id
                                ),
                            });
                        }
                        Ok(_) => {}
                        Err(error) => rejections.push(ActionRejection {
                            index: index as u32,
                            kind: error.kind,
                            message: error.message,
                        }),
                    }
                }
                BrainAction::SendMessage { .. } => {}
            }
        }
        rejections
    }

    pub(crate) fn validate_fan_out_invariants(
        &self,
        batch: &BrainActionBatch,
    ) -> Vec<ActionRejection> {
        let mut groups: HashMap<String, FanOutValidationGroup> = HashMap::new();
        let mut rejections = Vec::new();
        for (index, action) in batch.actions.iter().enumerate() {
            let BrainAction::RequestDelegation {
                fan_out_group_id: Some(group_id),
                fan_out_max_concurrency,
                fan_out_failure_policy,
                ..
            } = action
            else {
                continue;
            };
            let group = groups.entry(group_id.clone()).or_default();
            group.indexes.push(index as u32);
            if let Some(max_concurrency) = fan_out_max_concurrency {
                match group.max_concurrency {
                    Some(existing) if existing != *max_concurrency => {
                        rejections.push(ActionRejection {
                            index: index as u32,
                            kind: CoreErrorKind::ActionRejected,
                            message: format!(
                                "fan-out group {group_id} has inconsistent max concurrency"
                            ),
                        });
                    }
                    None => group.max_concurrency = Some(*max_concurrency),
                    _ => {}
                }
            }
            if let Some(policy) = fan_out_failure_policy {
                match &group.failure_policy {
                    Some(existing) if existing != policy => {
                        rejections.push(ActionRejection {
                            index: index as u32,
                            kind: CoreErrorKind::ActionRejected,
                            message: format!(
                                "fan-out group {group_id} has inconsistent failure policy"
                            ),
                        });
                    }
                    None => group.failure_policy = Some(policy.clone()),
                    _ => {}
                }
            }
        }

        for (group_id, group) in groups {
            if let Some(max_concurrency) = group.max_concurrency {
                if group.indexes.len() as u32 > max_concurrency {
                    rejections.extend(group.indexes.into_iter().map(|index| ActionRejection {
                        index,
                        kind: CoreErrorKind::ActionRejected,
                        message: format!(
                            "fan-out group {group_id} exceeds max concurrency {max_concurrency}"
                        ),
                    }));
                }
            }
        }

        rejections
    }

    pub(crate) fn update_lifecycle_for_actions(&self, batch: &BrainActionBatch) -> CoreResult<()> {
        for action in &batch.actions {
            let BrainAction::DeliverCompletion { packet } = action else {
                continue;
            };
            let status = match packet.status {
                CompletionStatus::Completed => WorkerRunStatus::Completed,
                CompletionStatus::Failed => WorkerRunStatus::Failed,
                CompletionStatus::Blocked => WorkerRunStatus::Blocked,
                CompletionStatus::Exhausted => WorkerRunStatus::Exhausted,
            };
            update_delegated_worker_run_status_by_session(
                &self.store,
                &packet.session_id,
                status,
                self.now(),
            )?;
            if let Some(run) =
                load_delegated_worker_run_by_session(&self.store, &packet.session_id)?
            {
                if let (Some(lease_id), Some(claim_token)) = (
                    run.worker_pool_lease_id.as_ref(),
                    run.worker_pool_claim_token.as_ref(),
                ) {
                    let pool_status = worker_pool_status_for_completion(&packet.status);
                    let _ = complete_worker_pool_work_item(
                        &self.store,
                        &WorkerPoolCompletionRequest {
                            lease_id: lease_id.clone(),
                            claim_token: claim_token.clone(),
                            status: pool_status,
                            now: self.now(),
                            summary: Some(packet.summary.clone()),
                        },
                    )?;
                }
            }
            if let Ok(session) = self.sessions.get_session(&packet.session_id) {
                self.publish_delegation_lifecycle(
                    &session,
                    None,
                    0,
                    delegation_phase_for_completion_status(packet.status.clone()),
                    Some(packet.summary.clone()),
                )?;
            }
        }
        Ok(())
    }

    fn publish_delegation_lifecycle(
        &self,
        session: &SessionState,
        source_wake_id: Option<&str>,
        source_action_index: u32,
        phase: DelegationLifecyclePhase,
        detail: Option<String>,
    ) -> CoreResult<()> {
        let Some(lineage) = &session.delegation else {
            return Ok(());
        };
        let run_id = self
            .store
            .load_worker_run_by_delegated_session(&session.session_id)?
            .map(|run| run.run_id)
            .or_else(|| {
                source_wake_id.map(|wake_id| RunId::new(format!("{wake_id}:{source_action_index}")))
            });
        self.bus.publish(CoreEvent::DelegationLifecycleObserved {
            lifecycle: DelegationLifecycleEvent {
                parent_session_id: lineage.parent_session_id.clone(),
                delegated_session_id: session.session_id.clone(),
                run_id,
                phase,
                detail,
            },
        })?;
        Ok(())
    }

    pub(crate) fn schedule_parent_completion_wakes(
        &self,
        batch: &BrainActionBatch,
    ) -> CoreResult<()> {
        for action in &batch.actions {
            let BrainAction::DeliverCompletion { packet } = action else {
                continue;
            };
            let Some(run) = self
                .store
                .load_worker_run_by_delegated_session(&packet.session_id)?
            else {
                continue;
            };
            if run.parent_consumption != ParentConsumptionPolicy::AwaitCompletion {
                continue;
            }
            let parent = match self.sessions.get_session(&run.parent_session_id) {
                Ok(parent) => parent,
                Err(error) if error.kind == CoreErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if !session_kind_can_wake(&parent.kind) || parent.status == SessionStatus::Archived {
                continue;
            }
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: parent.session_id,
            })?;
        }
        Ok(())
    }

    pub(crate) fn apply_fan_out_failure_policy(&self, batch: &BrainActionBatch) -> CoreResult<()> {
        for action in &batch.actions {
            let BrainAction::DeliverCompletion { packet } = action else {
                continue;
            };
            if packet.status == CompletionStatus::Completed {
                continue;
            }
            let Some(run) = self
                .store
                .load_worker_run_by_delegated_session(&packet.session_id)?
            else {
                continue;
            };
            if run.fan_out_failure_policy != FanOutFailurePolicy::FailFast {
                continue;
            }
            let Some(group_id) = run.fan_out_group_id.as_deref() else {
                continue;
            };
            for sibling in self
                .store
                .worker_runs_for_fan_out_group(&run.parent_session_id, group_id)?
            {
                if sibling.run_id == run.run_id || sibling.status.is_terminal() {
                    continue;
                }
                let Some(session_id) = sibling.delegated_session_id else {
                    continue;
                };
                let sibling_session = match self.sessions.get_session(&session_id) {
                    Ok(session) => session,
                    Err(error) if error.kind == CoreErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                let _ = self.archive_delegated_session_if_nonterminal(
                    &sibling_session,
                    WorkerRunStatus::Cancelled,
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn schedule_wake_for_event(&self, event: &CoreEvent) -> CoreResult<()> {
        let CoreEvent::AgentMessageRouted { message } = event else {
            return Ok(());
        };
        let session = match message
            .to_session_id
            .as_ref()
            .map(|session_id| self.sessions.get_session(session_id))
            .unwrap_or_else(|| self.sessions.get_session_by_agent(&message.to))
        {
            Ok(session) => session,
            Err(error) if error.kind == CoreErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };

        if !session_kind_can_wake(&session.kind) || session.status == SessionStatus::Archived {
            return Ok(());
        }

        let state = self.body_projector.project(&session.session_id)?;
        if DefaultWakeThreshold.should_wake(&state, event) {
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: session.session_id,
            })?;
        }

        Ok(())
    }
}

fn unregistered_delegation_profile_error(profile_id: &ProfileId) -> CoreError {
    CoreError::new(
        CoreErrorKind::ActionRejected,
        format!(
            "delegation profile {} is not registered with a brain implementation",
            profile_id.0
        ),
    )
}

pub fn delegated_session_id(
    parent_session_id: &SessionId,
    wake_id: &str,
    index: usize,
) -> SessionId {
    SessionId::new(format!("{parent_session_id}:delegated:{wake_id}:{index}"))
}

pub fn delegated_agent_id(session_id: &SessionId) -> AgentId {
    AgentId::new(format!("agent:{session_id}"))
}

fn delegated_run_status(status: WorkerRunStatus) -> DelegatedRunStatus {
    match status {
        WorkerRunStatus::Requested => DelegatedRunStatus::Requested,
        WorkerRunStatus::SessionCreated => DelegatedRunStatus::SessionCreated,
        WorkerRunStatus::WakeRequested => DelegatedRunStatus::WakeRequested,
        WorkerRunStatus::Running => DelegatedRunStatus::Running,
        WorkerRunStatus::CheckpointWaiting => DelegatedRunStatus::CheckpointWaiting,
        WorkerRunStatus::Completed => DelegatedRunStatus::Completed,
        WorkerRunStatus::Failed => DelegatedRunStatus::Failed,
        WorkerRunStatus::Blocked => DelegatedRunStatus::Blocked,
        WorkerRunStatus::Exhausted => DelegatedRunStatus::Exhausted,
        WorkerRunStatus::Cancelled => DelegatedRunStatus::Cancelled,
        WorkerRunStatus::Expired => DelegatedRunStatus::Expired,
    }
}

fn delegation_phase_for_worker_status(status: WorkerRunStatus) -> DelegationLifecyclePhase {
    match status {
        WorkerRunStatus::Expired => DelegationLifecyclePhase::TimedOut,
        WorkerRunStatus::Cancelled => DelegationLifecyclePhase::Cancelled,
        WorkerRunStatus::Completed => DelegationLifecyclePhase::Completed,
        WorkerRunStatus::Failed => DelegationLifecyclePhase::Failed,
        WorkerRunStatus::Blocked => DelegationLifecyclePhase::Blocked,
        WorkerRunStatus::Exhausted => DelegationLifecyclePhase::Exhausted,
        WorkerRunStatus::Requested
        | WorkerRunStatus::SessionCreated
        | WorkerRunStatus::Running
        | WorkerRunStatus::CheckpointWaiting => DelegationLifecyclePhase::Created,
        WorkerRunStatus::WakeRequested => DelegationLifecyclePhase::WakeRequested,
    }
}

fn delegation_phase_for_completion_status(status: CompletionStatus) -> DelegationLifecyclePhase {
    match status {
        CompletionStatus::Completed => DelegationLifecyclePhase::Completed,
        CompletionStatus::Failed => DelegationLifecyclePhase::Failed,
        CompletionStatus::Blocked => DelegationLifecyclePhase::Blocked,
        CompletionStatus::Exhausted => DelegationLifecyclePhase::Exhausted,
    }
}

enum WorkerPoolDelegationPlan {
    Direct,
    Claimed(Box<WorkerPoolClaimRecord>),
    Rejected(WorkerPoolNoCapacityReason),
}

struct WorkerPoolDelegationInput<'a> {
    request: Option<&'a WorkerPoolCapacityRequest>,
    run_id: &'a RunId,
    profile_id: &'a ProfileId,
    task_id: Option<&'a rusty_crew_core_protocol::TaskId>,
    prompt: &'a str,
    wake_id: &'a str,
    action_index: u32,
}

fn worker_pool_status_for_completion(status: &CompletionStatus) -> WorkerPoolWorkStatus {
    match status {
        CompletionStatus::Completed => WorkerPoolWorkStatus::Completed,
        CompletionStatus::Failed => WorkerPoolWorkStatus::Failed,
        CompletionStatus::Blocked => WorkerPoolWorkStatus::Blocked,
        CompletionStatus::Exhausted => WorkerPoolWorkStatus::Exhausted,
    }
}

fn worker_pool_no_capacity_reason_as_str(reason: WorkerPoolNoCapacityReason) -> &'static str {
    match reason {
        WorkerPoolNoCapacityReason::NoPendingWork => "no_pending_work",
        WorkerPoolNoCapacityReason::MemberUnavailable => "member_unavailable",
        WorkerPoolNoCapacityReason::MemberHeartbeatStale => "member_heartbeat_stale",
        WorkerPoolNoCapacityReason::MemberAtCapacity => "member_at_capacity",
    }
}
