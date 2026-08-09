use rusty_crew_core_persistence::{
    CoreCoordinationStore, QueryPage, SimpleKvQuery, SimpleKvRecord, SimpleKvScope, SimpleKvWrite,
};
use rusty_crew_core_protocol::{
    CoreError, CoreErrorKind, CoreResult, GitHubGateWaitRecord, IsoTimestamp, ProjectId, SessionId,
};

const WAIT_SCOPE_TYPE: &str = "coordination.github_gate_wait";
const WAIT_SCOPE_ID: &str = "active";
const CURSOR_SCOPE_TYPE: &str = "coordination.github_gate_events";
const CURSOR_SCOPE_ID: &str = "review";
const CURSOR_KEY: &str = "terminal_cursor";
const PROJECT_CURSOR_SCOPE_TYPE: &str = "coordination.github_gate_project_events";
const PROJECT_CURSOR_KEY: &str = "terminal_cursor";

fn wait_scope() -> SimpleKvScope {
    SimpleKvScope {
        scope_type: WAIT_SCOPE_TYPE.to_string(),
        scope_id: WAIT_SCOPE_ID.to_string(),
    }
}

fn cursor_scope() -> SimpleKvScope {
    SimpleKvScope {
        scope_type: CURSOR_SCOPE_TYPE.to_string(),
        scope_id: CURSOR_SCOPE_ID.to_string(),
    }
}

fn project_cursor_scope(project_id: &ProjectId) -> SimpleKvScope {
    SimpleKvScope {
        scope_type: PROJECT_CURSOR_SCOPE_TYPE.to_string(),
        scope_id: project_id.0.clone(),
    }
}

pub(crate) fn load_wait(
    store: &CoreCoordinationStore,
    session_id: &SessionId,
) -> CoreResult<Option<GitHubGateWaitRecord>> {
    get_record(store, wait_scope(), &session_id.0)?
        .map(decode_wait)
        .transpose()
}

pub(crate) fn list_waits(store: &CoreCoordinationStore) -> CoreResult<Vec<GitHubGateWaitRecord>> {
    store
        .list_simple_kv(&SimpleKvQuery {
            scope: wait_scope(),
            key_prefix: None,
            include_expired: false,
            expired_only: false,
            now: None,
            page: Some(QueryPage {
                limit: Some(1_000),
                offset: None,
            }),
        })?
        .into_iter()
        .map(decode_wait)
        .collect()
}

pub(crate) fn save_wait(
    store: &CoreCoordinationStore,
    wait: &GitHubGateWaitRecord,
) -> CoreResult<()> {
    let value_json = serde_json::to_value(wait).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("encode GitHub gate wait: {error}"),
        )
    })?;
    store.put_simple_kv(&SimpleKvWrite {
        scope: wait_scope(),
        key: wait.session_id.0.clone(),
        value_json,
        now: wait.updated_at.clone(),
        expires_at: None,
    })?;
    Ok(())
}

pub(crate) fn load_cursor(store: &CoreCoordinationStore) -> CoreResult<u64> {
    let Some(record) = get_record(store, cursor_scope(), CURSOR_KEY)? else {
        return Ok(0);
    };
    record
        .value_json
        .get("cursor")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "GitHub gate event cursor is invalid",
            )
        })
}

pub(crate) fn load_project_cursor(
    store: &CoreCoordinationStore,
    project_id: &ProjectId,
) -> CoreResult<u64> {
    let Some(record) = get_record(store, project_cursor_scope(project_id), PROJECT_CURSOR_KEY)?
    else {
        return Ok(0);
    };
    decode_cursor(&record)
}

fn decode_cursor(record: &SimpleKvRecord) -> CoreResult<u64> {
    record
        .value_json
        .get("cursor")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "GitHub gate event cursor is invalid",
            )
        })
}

fn get_record(
    store: &CoreCoordinationStore,
    scope: SimpleKvScope,
    key: &str,
) -> CoreResult<Option<SimpleKvRecord>> {
    Ok(store
        .list_simple_kv(&SimpleKvQuery {
            scope,
            key_prefix: Some(key.to_string()),
            include_expired: false,
            expired_only: false,
            now: None,
            page: Some(QueryPage {
                limit: Some(2),
                offset: None,
            }),
        })?
        .into_iter()
        .find(|record| record.key == key))
}

pub(crate) fn save_cursor(
    store: &CoreCoordinationStore,
    cursor: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    let cursor = cursor.max(load_cursor(store)?);
    store.put_simple_kv(&SimpleKvWrite {
        scope: cursor_scope(),
        key: CURSOR_KEY.to_string(),
        value_json: serde_json::json!({ "cursor": cursor }),
        now: now.clone(),
        expires_at: None,
    })?;
    Ok(())
}

pub(crate) fn save_project_cursor(
    store: &CoreCoordinationStore,
    project_id: &ProjectId,
    cursor: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    let cursor = cursor.max(load_project_cursor(store, project_id)?);
    store.put_simple_kv(&SimpleKvWrite {
        scope: project_cursor_scope(project_id),
        key: PROJECT_CURSOR_KEY.to_string(),
        value_json: serde_json::json!({ "cursor": cursor }),
        now: now.clone(),
        expires_at: None,
    })?;
    Ok(())
}

fn decode_wait(record: SimpleKvRecord) -> CoreResult<GitHubGateWaitRecord> {
    serde_json::from_value(record.value_json).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode GitHub gate wait: {error}"),
        )
    })
}
