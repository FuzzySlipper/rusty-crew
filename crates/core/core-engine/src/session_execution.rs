use super::*;
use rusty_crew_core_protocol::AgentDirectoryRuntimeKind;

const MAX_EXECUTION_ACTIVITIES: u32 = 5_000;
const MAX_EXECUTION_TURNS: u32 = 500;

impl CoreEngine {
    pub(crate) fn project_agent_directory_execution(
        &self,
        session: &SessionState,
        runtime_kind: AgentDirectoryRuntimeKind,
    ) -> CoreResult<(SessionStatus, Option<SessionExecutionState>)> {
        if runtime_kind != AgentDirectoryRuntimeKind::DirectBrain {
            return Ok((session.status.clone(), None));
        }
        let execution = self.session_execution_state(&session.session_id)?;
        let status = if execution.phase.is_working() {
            SessionStatus::Active
        } else {
            SessionStatus::Idle
        };
        Ok((status, Some(execution)))
    }

    pub fn session_execution_state(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<SessionExecutionState> {
        let session = self.sessions.get_session(session_id)?;
        let lifecycle_status = if session.status == SessionStatus::Archived {
            SessionLifecycleStatus::Archived
        } else {
            SessionLifecycleStatus::Live
        };
        if lifecycle_status == SessionLifecycleStatus::Archived {
            return Ok(SessionExecutionState {
                session_id: session_id.clone(),
                lifecycle_status,
                phase: SessionExecutionPhase::Idle,
                source: SessionExecutionSource::SessionLifecycle,
                wake_id: None,
                logical_turn_id: None,
                last_outcome: None,
                reason_code: Some("session_archived".into()),
                summary: Some("session is archived".into()),
                started_at: None,
                updated_at: session.last_active_at,
            });
        }

        let activities = self
            .store
            .list_runtime_activities_for_session(session_id, Some(MAX_EXECUTION_ACTIVITIES))?;
        let turns = self.store.list_logical_turns(&LogicalTurnDiagnosticQuery {
            logical_turn_id: None,
            session_id: Some(session_id.clone()),
            include_terminal: true,
            limit: MAX_EXECUTION_TURNS,
        })?;

        if let Some(turn) = turns.iter().find(|turn| !turn.phase.is_terminal()) {
            let active_activities = activities
                .iter()
                .filter(|activity| activity.status == RuntimeActivityStatus::Active)
                .collect::<Vec<_>>();
            let phase = match turn.phase {
                LogicalTurnPhase::Admitted
                | LogicalTurnPhase::Runnable
                | LogicalTurnPhase::Yielded => SessionExecutionPhase::Queued,
                LogicalTurnPhase::Running => {
                    if active_activities
                        .iter()
                        .any(|activity| activity.kind == RuntimeActivityKind::ToolCall)
                    {
                        SessionExecutionPhase::Waiting
                    } else {
                        SessionExecutionPhase::Active
                    }
                }
                LogicalTurnPhase::AttentionRequired => SessionExecutionPhase::Paused,
                LogicalTurnPhase::CancelRequested => SessionExecutionPhase::Cancelling,
                LogicalTurnPhase::Completed
                | LogicalTurnPhase::Cancelled
                | LogicalTurnPhase::Failed => unreachable!("terminal turn was filtered"),
            };
            let active = active_activities
                .iter()
                .max_by(|left, right| left.last_progress_at.cmp(&right.last_progress_at));
            return Ok(SessionExecutionState {
                session_id: session_id.clone(),
                lifecycle_status,
                phase,
                source: SessionExecutionSource::LogicalTurn,
                wake_id: active
                    .and_then(|activity| activity.wake_id.clone())
                    .or_else(|| Some(turn.source_wake_id.clone())),
                logical_turn_id: Some(turn.logical_turn_id.clone()),
                last_outcome: None,
                reason_code: turn
                    .attention
                    .as_ref()
                    .map(|attention| attention.reason_code.clone()),
                summary: turn
                    .attention
                    .as_ref()
                    .map(|attention| attention.summary.clone()),
                started_at: Some(turn.admitted_at.clone()),
                updated_at: active
                    .map(|activity| activity.last_progress_at.clone())
                    .filter(|at| at > &turn.updated_at)
                    .unwrap_or_else(|| turn.updated_at.clone()),
            });
        }

        let active = activities
            .iter()
            .filter(|activity| activity.status == RuntimeActivityStatus::Active)
            .max_by(|left, right| {
                execution_activity_priority(left.kind)
                    .cmp(&execution_activity_priority(right.kind))
                    .then_with(|| left.last_progress_at.cmp(&right.last_progress_at))
            });
        if let Some(activity) = active {
            let phase = match activity.kind {
                RuntimeActivityKind::Dispatch => SessionExecutionPhase::Queued,
                RuntimeActivityKind::ToolCall
                | RuntimeActivityKind::Subprocess
                | RuntimeActivityKind::Browser => SessionExecutionPhase::Waiting,
                RuntimeActivityKind::Wake
                | RuntimeActivityKind::ProviderRequest
                | RuntimeActivityKind::ExternalTurn => SessionExecutionPhase::Active,
            };
            return Ok(SessionExecutionState {
                session_id: session_id.clone(),
                lifecycle_status,
                phase,
                source: SessionExecutionSource::RuntimeActivity,
                wake_id: activity.wake_id.clone(),
                logical_turn_id: None,
                last_outcome: None,
                reason_code: activity.reason_code.clone(),
                summary: activity.summary.clone(),
                started_at: Some(activity.started_at.clone()),
                updated_at: activity.last_progress_at.clone(),
            });
        }

        let terminal_turn = turns.iter().find(|turn| turn.phase.is_terminal());
        let terminal_activity = activities
            .iter()
            .filter(|activity| {
                activity.status.is_terminal()
                    && matches!(
                        activity.kind,
                        RuntimeActivityKind::Dispatch
                            | RuntimeActivityKind::Wake
                            | RuntimeActivityKind::ExternalTurn
                    )
            })
            .max_by(|left, right| left.last_progress_at.cmp(&right.last_progress_at));
        let use_turn = match (terminal_turn, terminal_activity) {
            (Some(turn), Some(activity)) => turn.updated_at >= activity.last_progress_at,
            (Some(_), None) => true,
            _ => false,
        };
        let (last_outcome, wake_id, logical_turn_id, reason_code, summary, updated_at) = if use_turn
        {
            let turn = terminal_turn.expect("terminal turn selected");
            (
                Some(outcome_for_logical_turn(turn.phase)),
                Some(turn.source_wake_id.clone()),
                Some(turn.logical_turn_id.clone()),
                Some(logical_turn_reason_code(turn.phase).into()),
                Some(logical_turn_summary(turn.phase).into()),
                turn.updated_at.clone(),
            )
        } else if let Some(activity) = terminal_activity {
            (
                Some(outcome_for_runtime_activity(activity.status)),
                activity.wake_id.clone(),
                None,
                activity.reason_code.clone(),
                activity.summary.clone(),
                activity.last_progress_at.clone(),
            )
        } else {
            (None, None, None, None, None, session.last_active_at.clone())
        };
        Ok(SessionExecutionState {
            session_id: session_id.clone(),
            lifecycle_status,
            phase: SessionExecutionPhase::Idle,
            source: if terminal_turn.is_some() || terminal_activity.is_some() {
                if use_turn {
                    SessionExecutionSource::LogicalTurn
                } else {
                    SessionExecutionSource::RuntimeActivity
                }
            } else {
                SessionExecutionSource::SessionLifecycle
            },
            wake_id,
            logical_turn_id,
            last_outcome,
            reason_code,
            summary,
            started_at: None,
            updated_at,
        })
    }

    pub fn list_session_execution_states(&self) -> CoreResult<Vec<SessionExecutionState>> {
        let mut states = self
            .sessions
            .all_sessions()?
            .into_iter()
            .map(|session| self.session_execution_state(&session.session_id))
            .collect::<CoreResult<Vec<_>>>()?;
        states.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
        Ok(states)
    }

    pub(crate) fn project_session_execution(
        &self,
        mut session: SessionState,
    ) -> CoreResult<SessionState> {
        if session.status != SessionStatus::Archived {
            session.status = if self
                .session_execution_state(&session.session_id)?
                .phase
                .is_working()
            {
                SessionStatus::Active
            } else {
                SessionStatus::Idle
            };
        }
        Ok(session)
    }

    pub(crate) fn publish_session_execution(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<SessionExecutionState> {
        let execution = self.session_execution_state(session_id)?;
        self.bus.publish(CoreEvent::SessionExecutionObserved {
            execution: execution.clone(),
        })?;
        Ok(execution)
    }

    pub(crate) fn publish_session_execution_transition(
        &self,
        session_id: &SessionId,
        previous: Option<&SessionExecutionState>,
    ) -> CoreResult<SessionExecutionState> {
        let execution = self.session_execution_state(session_id)?;
        if previous.is_none_or(|previous| !same_execution_transition(previous, &execution)) {
            self.bus.publish(CoreEvent::SessionExecutionObserved {
                execution: execution.clone(),
            })?;
        }
        Ok(execution)
    }
}

fn same_execution_transition(left: &SessionExecutionState, right: &SessionExecutionState) -> bool {
    left.session_id == right.session_id
        && left.lifecycle_status == right.lifecycle_status
        && left.phase == right.phase
        && left.source == right.source
        && left.wake_id == right.wake_id
        && left.logical_turn_id == right.logical_turn_id
        && left.last_outcome == right.last_outcome
        && left.reason_code == right.reason_code
}

fn execution_activity_priority(kind: RuntimeActivityKind) -> u8 {
    match kind {
        RuntimeActivityKind::ToolCall
        | RuntimeActivityKind::Subprocess
        | RuntimeActivityKind::Browser => 3,
        RuntimeActivityKind::Wake
        | RuntimeActivityKind::ProviderRequest
        | RuntimeActivityKind::ExternalTurn => 2,
        RuntimeActivityKind::Dispatch => 1,
    }
}

fn outcome_for_logical_turn(phase: LogicalTurnPhase) -> SessionExecutionOutcome {
    match phase {
        LogicalTurnPhase::Completed => SessionExecutionOutcome::Completed,
        LogicalTurnPhase::Cancelled => SessionExecutionOutcome::Cancelled,
        LogicalTurnPhase::Failed => SessionExecutionOutcome::Failed,
        _ => unreachable!("logical turn outcome requires a terminal phase"),
    }
}

fn logical_turn_reason_code(phase: LogicalTurnPhase) -> &'static str {
    match phase {
        LogicalTurnPhase::Completed => "logical_turn_completed",
        LogicalTurnPhase::Cancelled => "logical_turn_cancelled",
        LogicalTurnPhase::Failed => "logical_turn_failed",
        _ => unreachable!("logical turn reason requires a terminal phase"),
    }
}

fn logical_turn_summary(phase: LogicalTurnPhase) -> &'static str {
    match phase {
        LogicalTurnPhase::Completed => "logical turn completed",
        LogicalTurnPhase::Cancelled => "logical turn was cancelled",
        LogicalTurnPhase::Failed => "logical turn failed",
        _ => unreachable!("logical turn summary requires a terminal phase"),
    }
}

fn outcome_for_runtime_activity(status: RuntimeActivityStatus) -> SessionExecutionOutcome {
    match status {
        RuntimeActivityStatus::Completed => SessionExecutionOutcome::Completed,
        RuntimeActivityStatus::Failed => SessionExecutionOutcome::Failed,
        RuntimeActivityStatus::Cancelled => SessionExecutionOutcome::Cancelled,
        RuntimeActivityStatus::Interrupted => SessionExecutionOutcome::Interrupted,
        RuntimeActivityStatus::Active => {
            unreachable!("runtime activity outcome requires a terminal status")
        }
    }
}
