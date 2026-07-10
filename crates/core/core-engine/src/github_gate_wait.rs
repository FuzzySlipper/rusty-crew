use rusty_crew_core_persistence::{
    CoreCoordinationStore, QueryPage, SimpleKvQuery, SimpleKvRecord, SimpleKvScope, SimpleKvWrite,
};
use rusty_crew_core_protocol::{
    CoreError, CoreErrorKind, CoreResult, GitHubGateWaitRecord, IsoTimestamp, SessionId,
};

const WAIT_SCOPE_TYPE: &str = "coordination.github_gate_wait";
const WAIT_SCOPE_ID: &str = "active";
const CURSOR_SCOPE_TYPE: &str = "coordination.github_gate_events";
const CURSOR_SCOPE_ID: &str = "review";
const CURSOR_KEY: &str = "terminal_cursor";

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
    store.put_simple_kv(&SimpleKvWrite {
        scope: cursor_scope(),
        key: CURSOR_KEY.to_string(),
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
