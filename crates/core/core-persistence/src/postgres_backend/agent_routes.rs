//! PostgreSQL repository for the operator-managed agent routing switchboard.

use super::*;
use rusty_crew_core_protocol::{
    validate_agent_route_write, AgentMessageDeliveryReceipt, AgentRouteDelete, AgentRouteKey,
    AgentRouteRecord, AgentRouteTarget, AgentRouteWrite,
};

impl PostgresBackendStore {
    pub fn put_agent_route(&self, write: &AgentRouteWrite) -> CoreResult<AgentRouteRecord> {
        validate_agent_route_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL agent route write", error))?;
        let current = tx
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.agent_routes WHERE route_key = $1 FOR UPDATE"
                ),
                &[&write.route_key.0],
            )
            .map_err(|error| postgres_error("load PostgreSQL agent route for write", error))?
            .map(|row| parse_route_json(&row.get::<_, String>(0)))
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
                &format!(
                    "INSERT INTO {schema}.agent_routes
                    (route_key, enabled, target_kind, target_agent_id, target_session_id,
                     target_binding_id, revision, updated_at, record_json)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                 ON CONFLICT(route_key) DO NOTHING"
                ),
                &[
                    &saved.route_key.0,
                    &saved.enabled,
                    &target_kind,
                    &target_agent_id,
                    &target_session_id,
                    &target_binding_id,
                    &(saved.revision as i64),
                    &saved.updated_at,
                    &to_json_text(&saved)?,
                ],
            )
            .map_err(|error| postgres_error("create PostgreSQL agent route", error))?
        } else {
            tx.execute(
                &format!(
                    "UPDATE {schema}.agent_routes SET
                        enabled = $2,
                        target_kind = $3,
                        target_agent_id = $4,
                        target_session_id = $5,
                        target_binding_id = $6,
                        revision = $7,
                        updated_at = $8,
                        record_json = $9
                      WHERE route_key = $1 AND revision = $10"
                ),
                &[
                    &saved.route_key.0,
                    &saved.enabled,
                    &target_kind,
                    &target_agent_id,
                    &target_session_id,
                    &target_binding_id,
                    &(saved.revision as i64),
                    &saved.updated_at,
                    &to_json_text(&saved)?,
                    &((saved.revision - 1) as i64),
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL agent route", error))?
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
            .map_err(|error| postgres_error("commit PostgreSQL agent route write", error))?;
        Ok(saved)
    }

    pub fn get_agent_route(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentRouteRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!("SELECT record_json FROM {schema}.agent_routes WHERE route_key = $1"),
                &[&route_key.0],
            )
            .map_err(|error| postgres_error("load PostgreSQL agent route", error))?
            .map(|row| parse_route_json(&row.get::<_, String>(0)))
            .transpose()
    }

    pub fn list_agent_routes(&self) -> CoreResult<Vec<AgentRouteRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query(
                &format!("SELECT record_json FROM {schema}.agent_routes ORDER BY route_key"),
                &[],
            )
            .map_err(|error| postgres_error("list PostgreSQL agent routes", error))?
            .into_iter()
            .map(|row| parse_route_json(&row.get::<_, String>(0)))
            .collect()
    }

    pub fn get_latest_agent_route_delivery(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.agent_message_delivery_receipts
                     WHERE record_json::jsonb #>> '{{request,routing,routeKey}}' = $1
                     ORDER BY created_at DESC, delivery_id DESC LIMIT 1"
                ),
                &[&route_key.0],
            )
            .map_err(|error| postgres_error("load latest PostgreSQL agent route delivery", error))?
            .map(|row| {
                serde_json::from_str(&row.get::<_, String>(0)).map_err(|error| {
                    CoreError::new(CoreErrorKind::PersistenceFailure, error.to_string())
                })
            })
            .transpose()
    }

    pub fn delete_agent_route(&self, delete: &AgentRouteDelete) -> CoreResult<AgentRouteRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL agent route delete", error))?;
        let current = tx
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.agent_routes WHERE route_key = $1 FOR UPDATE"
                ),
                &[&delete.route_key.0],
            )
            .map_err(|error| postgres_error("load PostgreSQL agent route for delete", error))?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "agent_route_not_found"))
            .and_then(|row| parse_route_json(&row.get::<_, String>(0)))?;
        if current.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_route_revision_mismatch",
            ));
        }
        tx.execute(
            &format!("DELETE FROM {schema}.agent_routes WHERE route_key = $1 AND revision = $2"),
            &[&delete.route_key.0, &(delete.expected_revision as i64)],
        )
        .map_err(|error| postgres_error("delete PostgreSQL agent route", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL agent route delete", error))?;
        Ok(current)
    }
}

fn parse_route_json(value: &str) -> CoreResult<AgentRouteRecord> {
    serde_json::from_str(value)
        .map_err(|error| CoreError::new(CoreErrorKind::PersistenceFailure, error.to_string()))
}

fn route_target_columns(
    record: &AgentRouteRecord,
) -> (&'static str, &str, Option<&str>, Option<&str>) {
    match &record.target {
        AgentRouteTarget::DirectBrain {
            agent_id,
            session_id,
        } => ("direct_brain", &agent_id.0, Some(&session_id.0), None),
        AgentRouteTarget::ManagedExternal {
            agent_id,
            binding_id,
            ..
        } => ("managed_external", &agent_id.0, None, Some(&binding_id.0)),
    }
}
