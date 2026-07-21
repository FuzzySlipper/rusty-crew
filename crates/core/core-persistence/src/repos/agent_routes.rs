//! Durable operator-managed agent routing switchboard.

use super::super::*;
use rusty_crew_core_protocol::validate_agent_route_write;

pub(crate) fn migrate_v54_add_agent_routes(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_routes (
            route_key TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL,
            target_kind TEXT NOT NULL,
            target_agent_id TEXT NOT NULL,
            target_session_id TEXT,
            target_binding_id TEXT,
            revision INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS agent_routes_enabled_idx
            ON agent_routes(enabled, route_key);
         CREATE INDEX IF NOT EXISTS agent_routes_direct_target_idx
            ON agent_routes(target_agent_id, target_session_id);
         CREATE INDEX IF NOT EXISTS agent_routes_external_target_idx
            ON agent_routes(target_binding_id)
            WHERE target_binding_id IS NOT NULL;",
    )
    .map_err(|error| persistence_error("create agent route tables", error))?;
    Ok(())
}

pub(crate) fn migrate_v55_add_agent_delivery_requested_address(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "UPDATE agent_message_delivery_receipts
            SET record_json = json_set(
                record_json,
                '$.request.requestedAddress',
                json_extract(record_json, '$.request.toAgentId')
            )
          WHERE json_extract(record_json, '$.request.requestedAddress') IS NULL;
         UPDATE event_history
            SET event_json = json_set(
                event_json,
                '$.receipt.request.requestedAddress',
                json_extract(event_json, '$.receipt.request.toAgentId')
            )
          WHERE json_type(event_json, '$.receipt.request') IS NOT NULL
            AND json_extract(event_json, '$.receipt.request.requestedAddress') IS NULL;",
    )
    .map_err(|error| persistence_error("migrate agent delivery requested addresses", error))?;
    Ok(())
}

impl CoordinationStore {
    pub fn put_agent_route(&self, write: &AgentRouteWrite) -> CoreResult<AgentRouteRecord> {
        validate_agent_route_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start agent route write", error))?;
        let current = tx
            .query_row(
                "SELECT record_json FROM agent_routes WHERE route_key = ?1",
                params![write.route_key.0],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| persistence_error("load agent route for write", error))?
            .map(|json| parse_json_record::<AgentRouteRecord>(&json))
            .transpose()?;
        let saved = match (current, write.expected_revision) {
            (None, None) => AgentRouteRecord {
                route_key: write.route_key.clone(),
                label: write.label.clone(),
                description: write.description.clone(),
                enabled: write.enabled,
                target: write.target.clone(),
                required_runtime_kind: write.required_runtime_kind,
                required_delivery_policy: write.required_delivery_policy,
                revision: 1,
                created_at: write.updated_at.clone(),
                updated_at: write.updated_at.clone(),
            },
            (Some(_), None) => {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    "agent_route_already_exists",
                ))
            }
            (None, Some(_)) => {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    "agent_route_not_found",
                ))
            }
            (Some(current), Some(expected_revision)) => {
                if current.revision != expected_revision {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "agent_route_revision_mismatch",
                    ));
                }
                AgentRouteRecord {
                    route_key: write.route_key.clone(),
                    label: write.label.clone(),
                    description: write.description.clone(),
                    enabled: write.enabled,
                    target: write.target.clone(),
                    required_runtime_kind: write.required_runtime_kind,
                    required_delivery_policy: write.required_delivery_policy,
                    revision: current.revision + 1,
                    created_at: current.created_at,
                    updated_at: write.updated_at.clone(),
                }
            }
        };
        let (target_kind, target_agent_id, target_session_id, target_binding_id) =
            route_target_columns(&saved);
        let affected = if write.expected_revision.is_none() {
            tx.execute(
                "INSERT INTO agent_routes
                (route_key, enabled, target_kind, target_agent_id, target_session_id,
                 target_binding_id, revision, updated_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(route_key) DO NOTHING",
                params![
                    saved.route_key.0,
                    saved.enabled,
                    target_kind,
                    target_agent_id,
                    target_session_id,
                    target_binding_id,
                    saved.revision,
                    saved.updated_at,
                    to_json_text(&saved)?,
                ],
            )
            .map_err(|error| persistence_error("create agent route", error))?
        } else {
            tx.execute(
                "UPDATE agent_routes SET
                    enabled = ?2,
                    target_kind = ?3,
                    target_agent_id = ?4,
                    target_session_id = ?5,
                    target_binding_id = ?6,
                    revision = ?7,
                    updated_at = ?8,
                    record_json = ?9
                  WHERE route_key = ?1 AND revision = ?10",
                params![
                    saved.route_key.0,
                    saved.enabled,
                    target_kind,
                    target_agent_id,
                    target_session_id,
                    target_binding_id,
                    saved.revision,
                    saved.updated_at,
                    to_json_text(&saved)?,
                    saved.revision - 1,
                ],
            )
            .map_err(|error| persistence_error("update agent route", error))?
        };
        if affected != 1 {
            return Err(CoreError::new(
                if write.expected_revision.is_none() {
                    CoreErrorKind::AlreadyExists
                } else {
                    CoreErrorKind::ActionRejected
                },
                if write.expected_revision.is_none() {
                    "agent_route_already_exists"
                } else {
                    "agent_route_revision_mismatch"
                },
            ));
        }
        tx.commit()
            .map_err(|error| persistence_error("commit agent route write", error))?;
        Ok(saved)
    }

    pub fn get_agent_route(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentRouteRecord>> {
        self.conn()?
            .query_row(
                "SELECT record_json FROM agent_routes WHERE route_key = ?1",
                params![route_key.0],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| persistence_error("load agent route", error))?
            .map(|json| parse_json_record::<AgentRouteRecord>(&json))
            .transpose()
    }

    pub fn list_agent_routes(&self) -> CoreResult<Vec<AgentRouteRecord>> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare("SELECT record_json FROM agent_routes ORDER BY route_key")
            .map_err(|error| persistence_error("prepare agent route list", error))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query agent route list", error))?;
        rows.map(|row| {
            let json = row.map_err(|error| persistence_error("read agent route", error))?;
            parse_json_record::<AgentRouteRecord>(&json)
        })
        .collect()
    }

    pub fn get_latest_agent_route_delivery(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        self.conn()?
            .query_row(
                "SELECT record_json FROM agent_message_delivery_receipts
                 WHERE json_extract(record_json, '$.request.routing.routeKey') = ?1
                 ORDER BY created_at DESC, delivery_id DESC LIMIT 1",
                params![route_key.0],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| persistence_error("load latest agent route delivery", error))?
            .map(|json| parse_json_record::<AgentMessageDeliveryReceipt>(&json))
            .transpose()
    }

    pub fn delete_agent_route(&self, delete: &AgentRouteDelete) -> CoreResult<AgentRouteRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start agent route delete", error))?;
        let current = tx
            .query_row(
                "SELECT record_json FROM agent_routes WHERE route_key = ?1",
                params![delete.route_key.0],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| persistence_error("load agent route for delete", error))?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "agent_route_not_found"))
            .and_then(|json| parse_json_record::<AgentRouteRecord>(&json))?;
        if current.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_route_revision_mismatch",
            ));
        }
        tx.execute(
            "DELETE FROM agent_routes WHERE route_key = ?1 AND revision = ?2",
            params![delete.route_key.0, delete.expected_revision],
        )
        .map_err(|error| persistence_error("delete agent route", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit agent route delete", error))?;
        Ok(current)
    }
}

fn route_target_columns(
    record: &AgentRouteRecord,
) -> (&'static str, &str, Option<&str>, Option<&str>) {
    match &record.target {
        rusty_crew_core_protocol::AgentRouteTarget::DirectBrain {
            agent_id,
            session_id,
        } => ("direct_brain", &agent_id.0, Some(&session_id.0), None),
        rusty_crew_core_protocol::AgentRouteTarget::ManagedExternal {
            agent_id,
            binding_id,
            ..
        } => ("managed_external", &agent_id.0, None, Some(&binding_id.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AgentDirectoryRuntimeKind, AgentId, AgentRouteTarget, SessionId,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn sqlite_agent_routes_are_revisioned_and_survive_reopen() {
        let path = temp_db_path();
        let store = CoordinationStore::open_file(&path).unwrap();
        let created = store.put_agent_route(&route_write(None)).unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(created.created_at, "2026-07-20T00:00:00Z");
        assert_eq!(store.list_agent_routes().unwrap(), vec![created.clone()]);
        assert_eq!(
            store
                .put_agent_route(&route_write(None))
                .unwrap_err()
                .message,
            "agent_route_already_exists"
        );
        assert_eq!(
            store
                .put_agent_route(&route_write(Some(99)))
                .unwrap_err()
                .message,
            "agent_route_revision_mismatch"
        );

        let mut update = route_write(Some(1));
        update.label = "Reviewer queue".into();
        update.updated_at = "2026-07-20T00:01:00Z".into();
        let updated = store.put_agent_route(&update).unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.created_at, created.created_at);
        drop(store);

        let reopened = CoordinationStore::open_file(&path).unwrap();
        assert_eq!(
            reopened
                .get_agent_route(&AgentRouteKey::new("reviewer"))
                .unwrap(),
            Some(updated.clone())
        );
        assert_eq!(
            reopened
                .delete_agent_route(&AgentRouteDelete {
                    route_key: AgentRouteKey::new("reviewer"),
                    expected_revision: 1,
                })
                .unwrap_err()
                .message,
            "agent_route_revision_mismatch"
        );
        assert_eq!(
            reopened
                .delete_agent_route(&AgentRouteDelete {
                    route_key: AgentRouteKey::new("reviewer"),
                    expected_revision: 2,
                })
                .unwrap(),
            updated
        );
        assert!(reopened.list_agent_routes().unwrap().is_empty());
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    fn route_write(expected_revision: Option<u64>) -> AgentRouteWrite {
        AgentRouteWrite {
            route_key: AgentRouteKey::new("reviewer"),
            label: "Reviewer".into(),
            description: Some("Serial review destination".into()),
            enabled: true,
            target: AgentRouteTarget::DirectBrain {
                agent_id: AgentId::new("review-agent"),
                session_id: SessionId::new("review-session"),
            },
            required_runtime_kind: Some(AgentDirectoryRuntimeKind::DirectBrain),
            required_delivery_policy: None,
            expected_revision,
            updated_at: "2026-07-20T00:00:00Z".into(),
        }
    }

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-crew-agent-routes-{}-{}.sqlite3",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ))
    }
}
