use super::*;
use rusty_crew_core_protocol::RuntimeActivityLiveEvidence;

const MAX_ACTIVITY_TEXT_BYTES: usize = 512;
const MAX_ACTIVITY_PHASE_BYTES: usize = 128;
const MAX_CENSUS_RECORDS: u32 = 5_000;

impl CoreEngine {
    pub fn begin_runtime_activity(
        &self,
        input: RuntimeActivityBegin,
    ) -> CoreResult<RuntimeActivityRecord> {
        validate_runtime_activity_begin(&input)?;
        if let Some(session_id) = input.session_id.as_ref() {
            let session = self.get_session(session_id)?;
            validate_activity_session_identity(
                &session,
                input.agent_id.as_ref(),
                input.profile_id.as_ref(),
            )?;
        }
        let now = self.now();
        self.store.insert_runtime_activity(&RuntimeActivityRecord {
            activity_id: input.activity_id,
            service_instance_id: self.service_instance_id.clone(),
            parent_activity_id: input.parent_activity_id,
            kind: input.kind,
            owner: input.owner,
            status: RuntimeActivityStatus::Active,
            agent_id: input.agent_id,
            profile_id: input.profile_id,
            session_id: input.session_id,
            wake_id: input.wake_id,
            phase: input.phase,
            summary: input.summary,
            provider_alias: input.provider_alias,
            model: input.model,
            tool_name: input.tool_name,
            process_id: input.process_id,
            debug_detail_id: input.debug_detail_id,
            reason_code: None,
            started_at: now.clone(),
            last_progress_at: now,
            terminal_at: None,
            revision: 1,
        })
    }

    pub fn progress_runtime_activity(
        &self,
        input: RuntimeActivityProgress,
    ) -> CoreResult<RuntimeActivityRecord> {
        validate_activity_text("phase", &input.phase, MAX_ACTIVITY_PHASE_BYTES)?;
        validate_optional_activity_text("summary", input.summary.as_deref())?;
        validate_optional_activity_text("debugDetailId", input.debug_detail_id.as_deref())?;
        let mut record = self
            .store
            .get_runtime_activity(&input.activity_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("runtime activity {} was not found", input.activity_id.0),
                )
            })?;
        if record.status.is_terminal() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "runtime activity {} is already terminal",
                    record.activity_id.0
                ),
            ));
        }
        let expected_revision = record.revision;
        record.phase = input.phase;
        if input.summary.is_some() {
            record.summary = input.summary;
        }
        if input.process_id.is_some() {
            record.process_id = input.process_id;
        }
        if input.debug_detail_id.is_some() {
            record.debug_detail_id = input.debug_detail_id;
        }
        record.last_progress_at = self.now();
        record.revision += 1;
        self.store
            .update_runtime_activity(&record, expected_revision)
    }

    pub fn finish_runtime_activity(
        &self,
        input: RuntimeActivityFinish,
    ) -> CoreResult<RuntimeActivityRecord> {
        if !input.status.is_terminal() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "runtime activity finish status must be terminal",
            ));
        }
        validate_activity_text("phase", &input.phase, MAX_ACTIVITY_PHASE_BYTES)?;
        validate_optional_activity_text("reasonCode", input.reason_code.as_deref())?;
        validate_optional_activity_text("summary", input.summary.as_deref())?;
        let mut record = self
            .store
            .get_runtime_activity(&input.activity_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("runtime activity {} was not found", input.activity_id.0),
                )
            })?;
        if record.status.is_terminal() {
            return Ok(record);
        }
        let expected_revision = record.revision;
        let now = self.now();
        record.status = input.status;
        record.phase = input.phase;
        record.reason_code = input.reason_code;
        if input.summary.is_some() {
            record.summary = input.summary;
        }
        record.last_progress_at = now.clone();
        record.terminal_at = Some(now);
        record.revision += 1;
        self.store
            .update_runtime_activity(&record, expected_revision)
    }

    pub fn runtime_activity_census(
        &self,
        query: RuntimeActivityCensusQuery,
    ) -> CoreResult<RuntimeActivityCensus> {
        let now = self.now();
        let mut active = self.store.list_runtime_activities(
            Some(RuntimeActivityStatus::Active),
            Some(MAX_CENSUS_RECORDS),
        )?;
        let mut findings = Vec::new();
        let projected_active_session_ids = query
            .projected_active_session_ids
            .map(|session_ids| session_ids.into_iter().collect::<HashSet<_>>());
        let live_ids = query
            .live_evidence
            .iter()
            .map(|evidence| evidence.activity_id.clone())
            .collect::<HashSet<_>>();

        for evidence in query.live_evidence {
            if let Some(record) = active.iter_mut().find(|record| {
                record.activity_id == evidence.activity_id
                    || (evidence.kind == RuntimeActivityKind::Subprocess
                        && evidence.process_id.is_some()
                        && record.process_id == evidence.process_id)
            }) {
                record.phase = evidence.phase;
                record.summary = evidence.summary;
                record.process_id = evidence.process_id.or(record.process_id);
                record.last_progress_at = evidence.last_progress_at;
                continue;
            }
            if evidence.kind == RuntimeActivityKind::Subprocess
                && evidence
                    .parent_activity_id
                    .as_ref()
                    .is_some_and(|parent| parent.0.starts_with("process:"))
            {
                continue;
            }
            let code = if evidence.kind == RuntimeActivityKind::Subprocess {
                RuntimeActivityFindingCode::UntrackedServiceProcess
            } else {
                RuntimeActivityFindingCode::UntrackedNativeRun
            };
            findings.push(RuntimeActivityFinding {
                code,
                activity_id: evidence.activity_id.clone(),
                related_activity_id: evidence.parent_activity_id.clone(),
                message: "live runtime evidence has no durable activity record".into(),
            });
            active.push(runtime_activity_from_live_evidence(
                &self.service_instance_id,
                evidence,
            ));
        }

        for turn in self.list_active_external_turns()? {
            let activity_id =
                RuntimeActivityId::new(format!("external:{}", turn.request.request_id.0));
            if active
                .iter()
                .any(|record| record.activity_id == activity_id)
            {
                continue;
            }
            let session = self.get_session(&turn.request.session_id).ok();
            active.push(RuntimeActivityRecord {
                activity_id,
                service_instance_id: self.service_instance_id.clone(),
                parent_activity_id: None,
                kind: RuntimeActivityKind::ExternalTurn,
                owner: RuntimeActivityOwner::ExternalRuntime,
                status: RuntimeActivityStatus::Active,
                agent_id: session.as_ref().map(|state| state.agent_id.clone()),
                profile_id: session.as_ref().map(|state| state.profile_id.clone()),
                session_id: Some(turn.request.session_id),
                wake_id: None,
                phase: external_turn_phase_name(turn.phase).into(),
                summary: Some(format!("external runtime {} turn", turn.runtime_id.0)),
                provider_alias: None,
                model: None,
                tool_name: None,
                process_id: None,
                debug_detail_id: None,
                reason_code: None,
                started_at: turn.request.created_at,
                last_progress_at: turn.updated_at,
                terminal_at: None,
                revision: turn.revision,
            });
        }

        let active_ids = active
            .iter()
            .map(|record| record.activity_id.clone())
            .collect::<HashSet<_>>();
        let child_parent_ids = active
            .iter()
            .filter_map(|record| record.parent_activity_id.clone())
            .collect::<HashSet<_>>();

        for record in &active {
            if let Some(session_id) = record.session_id.as_ref() {
                match self.get_session(session_id) {
                    Ok(session) => {
                        let identity_mismatch = record
                            .agent_id
                            .as_ref()
                            .is_some_and(|agent_id| agent_id != &session.agent_id)
                            || record
                                .profile_id
                                .as_ref()
                                .is_some_and(|profile_id| profile_id != &session.profile_id);
                        if identity_mismatch {
                            findings.push(activity_finding(
                                RuntimeActivityFindingCode::SessionProjectionMismatch,
                                record,
                                "runtime activity identity does not match the session projection",
                            ));
                        } else if activity_requires_active_session_projection(record.kind) {
                            let projected_active = projected_active_session_ids
                                .as_ref()
                                .map(|session_ids| session_ids.contains(session_id))
                                .unwrap_or(session.status == SessionStatus::Active);
                            if !projected_active {
                                findings.push(activity_finding(
                                    RuntimeActivityFindingCode::SessionProjectionMismatch,
                                    record,
                                    "runtime activity is active while the session projection is idle",
                                ));
                            }
                        }
                    }
                    Err(error) if error.kind == CoreErrorKind::NotFound => {
                        findings.push(activity_finding(
                            RuntimeActivityFindingCode::SessionProjectionMismatch,
                            record,
                            "runtime activity references a missing session",
                        ));
                    }
                    Err(error) => return Err(error),
                }
            }

            if let Some(parent_id) = record.parent_activity_id.as_ref() {
                if !active_ids.contains(parent_id) {
                    let code = if matches!(
                        record.kind,
                        RuntimeActivityKind::ToolCall
                            | RuntimeActivityKind::Subprocess
                            | RuntimeActivityKind::Browser
                    ) {
                        RuntimeActivityFindingCode::OrphanToolExecution
                    } else {
                        RuntimeActivityFindingCode::DetachedDispatch
                    };
                    findings.push(RuntimeActivityFinding {
                        code,
                        activity_id: record.activity_id.clone(),
                        related_activity_id: Some(parent_id.clone()),
                        message: "active runtime activity has no active parent".into(),
                    });
                }
            }

            if record.kind == RuntimeActivityKind::Dispatch
                && !child_parent_ids.contains(&record.activity_id)
                && activity_elapsed_ms(&now, &record.started_at)? >= 10_000
            {
                findings.push(activity_finding(
                    RuntimeActivityFindingCode::DetachedDispatch,
                    record,
                    "dispatch remained active without a child wake",
                ));
            }

            if record.owner == RuntimeActivityOwner::RustBrain
                && matches!(
                    record.kind,
                    RuntimeActivityKind::Wake | RuntimeActivityKind::ProviderRequest
                )
                && !live_ids.contains(&record.activity_id)
            {
                findings.push(activity_finding(
                    RuntimeActivityFindingCode::StaleLedgerEntry,
                    record,
                    "Rust brain activity has no matching live runtime evidence",
                ));
            }

            if query.stall_after_ms.is_some_and(|threshold| {
                activity_elapsed_ms(&now, &record.last_progress_at)
                    .is_ok_and(|elapsed| elapsed >= threshold)
            }) {
                findings.push(activity_finding(
                    RuntimeActivityFindingCode::Stalled,
                    record,
                    "runtime activity has not reported progress within the diagnostic threshold",
                ));
            }
        }

        active.sort_by(|left, right| {
            right
                .last_progress_at
                .cmp(&left.last_progress_at)
                .then_with(|| left.activity_id.cmp(&right.activity_id))
        });

        let abnormal_limit = query.recent_abnormal_limit.unwrap_or(100).clamp(1, 500);
        let mut recently_abnormal = Vec::new();
        for status in [
            RuntimeActivityStatus::Failed,
            RuntimeActivityStatus::Cancelled,
            RuntimeActivityStatus::Interrupted,
        ] {
            recently_abnormal.extend(
                self.store
                    .list_runtime_activities(Some(status), Some(abnormal_limit))?,
            );
        }
        recently_abnormal.sort_by(|left, right| {
            right
                .last_progress_at
                .cmp(&left.last_progress_at)
                .then_with(|| left.activity_id.cmp(&right.activity_id))
        });
        recently_abnormal.truncate(abnormal_limit as usize);
        for record in &recently_abnormal {
            if record.status == RuntimeActivityStatus::Interrupted
                && record.reason_code.as_deref() == Some("restart_interrupted")
            {
                findings.push(activity_finding(
                    RuntimeActivityFindingCode::RestartInterrupted,
                    record,
                    "unfinished runtime activity was interrupted during service restart",
                ));
            }
        }

        let untracked_processes = findings
            .iter()
            .filter(|finding| finding.code == RuntimeActivityFindingCode::UntrackedServiceProcess)
            .count() as u32;
        let summary = RuntimeActivityCensusSummary {
            active: active.len() as u32,
            recently_abnormal: recently_abnormal.len() as u32,
            findings: findings.len() as u32,
            untracked_processes,
        };
        let summary_time = now.clone();
        Ok(RuntimeActivityCensus {
            generated_at: now,
            service_instance_id: self.service_instance_id.clone(),
            active: active
                .into_iter()
                .map(|record| runtime_activity_view(&summary_time, record))
                .collect::<CoreResult<Vec<_>>>()?,
            recently_abnormal: recently_abnormal
                .into_iter()
                .map(|record| runtime_activity_view(&summary_time, record))
                .collect::<CoreResult<Vec<_>>>()?,
            findings,
            summary,
            automatic_cancellation_enabled: false,
        })
    }

    pub fn finish_runtime_activity_tree(
        &self,
        wake_id: &str,
        status: RuntimeActivityStatus,
        reason_code: Option<&str>,
        summary: &str,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        if !status.is_terminal() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "runtime activity tree finish status must be terminal",
            ));
        }
        let mut records = self
            .store
            .list_runtime_activities(
                Some(RuntimeActivityStatus::Active),
                Some(MAX_CENSUS_RECORDS),
            )?
            .into_iter()
            .filter(|record| {
                record.wake_id.as_deref() == Some(wake_id)
                    && record.kind != RuntimeActivityKind::Dispatch
            })
            .collect::<Vec<_>>();
        records.sort_by_key(|record| match record.kind {
            RuntimeActivityKind::Subprocess | RuntimeActivityKind::Browser => 0,
            RuntimeActivityKind::ToolCall => 1,
            RuntimeActivityKind::ProviderRequest => 2,
            RuntimeActivityKind::Wake => 3,
            RuntimeActivityKind::Dispatch | RuntimeActivityKind::ExternalTurn => 4,
        });
        records
            .into_iter()
            .map(|record| {
                self.finish_runtime_activity(RuntimeActivityFinish {
                    activity_id: record.activity_id,
                    status,
                    phase: if status == RuntimeActivityStatus::Completed {
                        "completed".into()
                    } else {
                        "terminated".into()
                    },
                    reason_code: reason_code.map(str::to_string),
                    summary: Some(summary.into()),
                })
            })
            .collect()
    }
}

fn activity_requires_active_session_projection(kind: RuntimeActivityKind) -> bool {
    matches!(
        kind,
        RuntimeActivityKind::Dispatch
            | RuntimeActivityKind::Wake
            | RuntimeActivityKind::ProviderRequest
            | RuntimeActivityKind::ToolCall
            | RuntimeActivityKind::Subprocess
    )
}

fn runtime_activity_view(
    now: &str,
    record: RuntimeActivityRecord,
) -> CoreResult<RuntimeActivityView> {
    Ok(RuntimeActivityView {
        elapsed_ms: activity_elapsed_ms(now, &record.started_at)?,
        since_progress_ms: activity_elapsed_ms(now, &record.last_progress_at)?,
        activity: record,
    })
}

fn validate_runtime_activity_begin(input: &RuntimeActivityBegin) -> CoreResult<()> {
    validate_activity_text("activityId", &input.activity_id.0, MAX_ACTIVITY_TEXT_BYTES)?;
    validate_activity_text("phase", &input.phase, MAX_ACTIVITY_PHASE_BYTES)?;
    validate_optional_activity_text("summary", input.summary.as_deref())?;
    validate_optional_activity_text("providerAlias", input.provider_alias.as_deref())?;
    validate_optional_activity_text("model", input.model.as_deref())?;
    validate_optional_activity_text("toolName", input.tool_name.as_deref())?;
    validate_optional_activity_text("debugDetailId", input.debug_detail_id.as_deref())
}

fn validate_activity_text(name: &str, value: &str, max_bytes: usize) -> CoreResult<()> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("runtime activity {name} must contain 1..={max_bytes} bytes"),
        ));
    }
    Ok(())
}

fn validate_optional_activity_text(name: &str, value: Option<&str>) -> CoreResult<()> {
    if let Some(value) = value {
        validate_activity_text(name, value, MAX_ACTIVITY_TEXT_BYTES)?;
    }
    Ok(())
}

fn validate_activity_session_identity(
    session: &SessionState,
    agent_id: Option<&AgentId>,
    profile_id: Option<&ProfileId>,
) -> CoreResult<()> {
    if agent_id.is_some_and(|agent_id| agent_id != &session.agent_id)
        || profile_id.is_some_and(|profile_id| profile_id != &session.profile_id)
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "runtime activity identity does not match its session",
        ));
    }
    Ok(())
}

fn runtime_activity_from_live_evidence(
    service_instance_id: &str,
    evidence: RuntimeActivityLiveEvidence,
) -> RuntimeActivityRecord {
    RuntimeActivityRecord {
        activity_id: evidence.activity_id,
        service_instance_id: service_instance_id.into(),
        parent_activity_id: evidence.parent_activity_id,
        kind: evidence.kind,
        owner: evidence.owner,
        status: RuntimeActivityStatus::Active,
        agent_id: evidence.agent_id,
        profile_id: evidence.profile_id,
        session_id: evidence.session_id,
        wake_id: evidence.wake_id,
        phase: evidence.phase,
        summary: evidence.summary,
        provider_alias: None,
        model: None,
        tool_name: None,
        process_id: evidence.process_id,
        debug_detail_id: None,
        reason_code: None,
        started_at: evidence.started_at,
        last_progress_at: evidence.last_progress_at,
        terminal_at: None,
        revision: 0,
    }
}

fn activity_finding(
    code: RuntimeActivityFindingCode,
    record: &RuntimeActivityRecord,
    message: &str,
) -> RuntimeActivityFinding {
    RuntimeActivityFinding {
        code,
        activity_id: record.activity_id.clone(),
        related_activity_id: record.parent_activity_id.clone(),
        message: message.into(),
    }
}

fn activity_elapsed_ms(now: &str, then: &str) -> CoreResult<u64> {
    let elapsed = parse_rfc3339(now)? - parse_rfc3339(then)?;
    Ok(elapsed.whole_milliseconds().max(0) as u64)
}

fn external_turn_phase_name(phase: ExternalTurnPhase) -> &'static str {
    match phase {
        ExternalTurnPhase::Accepted => "accepted",
        ExternalTurnPhase::Starting => "starting",
        ExternalTurnPhase::Active => "active",
        ExternalTurnPhase::WaitingInteraction => "waiting_interaction",
        ExternalTurnPhase::Completed => "completed",
        ExternalTurnPhase::Failed => "failed",
        ExternalTurnPhase::Interrupted => "interrupted",
        ExternalTurnPhase::OutcomeUnknown => "outcome_unknown",
    }
}
