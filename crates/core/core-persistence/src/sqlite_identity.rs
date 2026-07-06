//! SQLite identity, session, and coordination enum conversions.
//!
//! These helpers are shared by session identity rows, worker-run rows, service
//! config defaults, and tool-call history. They stay narrow: no SQL builders and
//! no dynamic identifier validation live here.

use super::*;

pub(crate) fn durable_agent_kind_from_session_kind(kind: &SessionKind) -> DurableAgentKind {
    match kind {
        SessionKind::Full => DurableAgentKind::Full,
        SessionKind::Worker => DurableAgentKind::WorkerPoolWorker,
        SessionKind::Delegated => DurableAgentKind::Delegated,
    }
}

pub(crate) fn durable_status_from_session_status(status: &SessionStatus) -> DurableIdentityStatus {
    match status {
        SessionStatus::Active | SessionStatus::Idle => DurableIdentityStatus::Active,
        SessionStatus::Archived => DurableIdentityStatus::Archived,
    }
}

pub(crate) fn durable_agent_kind_as_str(kind: &DurableAgentKind) -> &'static str {
    match kind {
        DurableAgentKind::Prime => "prime",
        DurableAgentKind::Full => "full",
        DurableAgentKind::Delegated => "delegated",
        DurableAgentKind::WorkerPoolWorker => "worker_pool_worker",
    }
}

pub(crate) fn durable_agent_kind_from_str(raw: &str) -> rusqlite::Result<DurableAgentKind> {
    match raw {
        "prime" => Ok(DurableAgentKind::Prime),
        "full" => Ok(DurableAgentKind::Full),
        "delegated" => Ok(DurableAgentKind::Delegated),
        "worker_pool_worker" => Ok(DurableAgentKind::WorkerPoolWorker),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown durable agent kind {other}"),
            )),
        )),
    }
}

pub(crate) fn durable_identity_status_as_str(status: &DurableIdentityStatus) -> &'static str {
    match status {
        DurableIdentityStatus::Active => "active",
        DurableIdentityStatus::Archived => "archived",
    }
}

pub(crate) fn durable_identity_status_from_str(
    raw: &str,
) -> rusqlite::Result<DurableIdentityStatus> {
    match raw {
        "active" => Ok(DurableIdentityStatus::Active),
        "archived" => Ok(DurableIdentityStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown durable identity status {other}"),
            )),
        )),
    }
}

pub(crate) fn session_kind_as_str(kind: &SessionKind) -> &'static str {
    match kind {
        SessionKind::Full => "full",
        SessionKind::Worker => "worker",
        SessionKind::Delegated => "delegated",
    }
}

pub(crate) fn session_kind_from_str(raw: &str) -> rusqlite::Result<SessionKind> {
    match raw {
        "full" => Ok(SessionKind::Full),
        "worker" => Ok(SessionKind::Worker),
        "delegated" => Ok(SessionKind::Delegated),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown session kind {other}"),
            )),
        )),
    }
}

pub(crate) fn session_status_as_str(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Idle => "idle",
        SessionStatus::Archived => "archived",
    }
}

pub(crate) fn session_status_from_str(raw: &str) -> rusqlite::Result<SessionStatus> {
    match raw {
        "active" => Ok(SessionStatus::Active),
        "idle" => Ok(SessionStatus::Idle),
        "archived" => Ok(SessionStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown session status {other}"),
            )),
        )),
    }
}

pub(crate) fn tool_call_phase_from_str(raw: &str) -> rusqlite::Result<ToolCallPhase> {
    match raw {
        "started" => Ok(ToolCallPhase::Started),
        "finished" => Ok(ToolCallPhase::Finished),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unsupported tool call phase {other}"),
            )),
        )),
    }
}

pub(crate) fn parent_consumption_policy_as_str(policy: &ParentConsumptionPolicy) -> &'static str {
    match policy {
        ParentConsumptionPolicy::AwaitCompletion => "await_completion",
        ParentConsumptionPolicy::ObserveOnly => "observe_only",
    }
}

pub(crate) fn parent_consumption_policy_from_str(
    raw: &str,
) -> rusqlite::Result<ParentConsumptionPolicy> {
    match raw {
        "await_completion" => Ok(ParentConsumptionPolicy::AwaitCompletion),
        "observe_only" => Ok(ParentConsumptionPolicy::ObserveOnly),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            12,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown parent consumption policy {other}"),
            )),
        )),
    }
}
