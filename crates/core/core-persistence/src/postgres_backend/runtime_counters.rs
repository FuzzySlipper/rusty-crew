//! PostgreSQL runtime counter repository.

use super::*;

impl PostgresBackendStore {
    pub fn increment_counter(
        &self,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        if amount == 0 {
            return Ok(());
        }
        validate_counter_amount(amount)?;
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_counters (
                    scope_type,
                    scope_id,
                    counter_name,
                    value,
                    updated_at
                ) VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                    value = runtime_counters.value + EXCLUDED.value,
                    updated_at = EXCLUDED.updated_at"
                ),
                &[&scope_type, &scope_id, &counter_name, &(amount as i64), now],
            )
            .map_err(|error| postgres_error("increment PostgreSQL runtime counter", error))?;
        Ok(())
    }

    pub fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.query_runtime_counters(&RuntimeCounterQuery {
            scope: scope.cloned(),
            counter_name: None,
            page: None,
        })
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
        let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
        let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
        let counter_name = query.counter_name.as_deref();
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(200, 5_000);
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT scope_type, scope_id, counter_name, value, updated_at
                     FROM {schema}.runtime_counters
                     WHERE ($1::text IS NULL OR scope_type = $1)
                       AND ($2::text IS NULL OR scope_id = $2)
                       AND ($3::text IS NULL OR counter_name = $3)
                     ORDER BY scope_type ASC, scope_id ASC, counter_name ASC
                     LIMIT $4 OFFSET $5"
                ),
                &[&scope_type, &scope_id, &counter_name, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime counters", error))?;
        rows.iter().map(row_to_runtime_counter).collect()
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
        let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
        let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
        let counter_name = query.counter_name.as_deref();
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.runtime_counters
                     SET value = 0, updated_at = $4
                     WHERE ($1::text IS NULL OR scope_type = $1)
                       AND ($2::text IS NULL OR scope_id = $2)
                       AND ($3::text IS NULL OR counter_name = $3)"
                ),
                &[&scope_type, &scope_id, &counter_name, &now],
            )
            .map_err(|error| postgres_error("reset PostgreSQL runtime counters", error))
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        crate::repos::runtime_counters::RuntimeCounterRepository::runtime_summary(self, scope)
    }

    pub(super) fn increment_counter_in_tx(
        &self,
        tx: &mut impl GenericClient,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
    ) -> CoreResult<()> {
        if amount == 0 {
            return Ok(());
        }
        validate_counter_amount(amount)?;
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let schema = self.quoted_schema();
        let now = postgres_now_iso(tx)?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.runtime_counters (
                    scope_type,
                    scope_id,
                    counter_name,
                    value,
                    updated_at
                ) VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                    value = runtime_counters.value + EXCLUDED.value,
                    updated_at = EXCLUDED.updated_at"
            ),
            &[&scope_type, &scope_id, &counter_name, &(amount as i64), &now],
        )
        .map_err(|error| postgres_error("increment PostgreSQL runtime counter in tx", error))?;
        Ok(())
    }

    pub(super) fn runtime_counter_rows(&self) -> CoreResult<u64> {
        self.table_rows("runtime_counters")
    }
}

impl crate::repos::runtime_counters::RuntimeCounterRepository for PostgresBackendStore {
    #[cfg(test)]
    fn record_runtime_counter_delta(
        &self,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        PostgresBackendStore::increment_counter(self, scope, counter_name, amount, now)
    }

    fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        PostgresBackendStore::runtime_counters(self, scope)
    }

    fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        PostgresBackendStore::query_runtime_counters(self, query)
    }

    fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        PostgresBackendStore::reset_runtime_counters(self, query, now)
    }
}

fn runtime_counter_scope_parts(scope: &RuntimeCounterScope) -> (&'static str, String) {
    match scope {
        RuntimeCounterScope::Runtime => ("runtime", "_global".to_string()),
        RuntimeCounterScope::Agent(agent_id) => ("agent", agent_id.0.clone()),
        RuntimeCounterScope::Instance(instance_id) => ("instance", instance_id.0.clone()),
        RuntimeCounterScope::Session(session_id) => ("session", session_id.0.clone()),
    }
}

fn runtime_counter_scope_from_parts(
    scope_type: &str,
    scope_id: &str,
) -> CoreResult<RuntimeCounterScope> {
    match scope_type {
        "runtime" if scope_id == "_global" => Ok(RuntimeCounterScope::Runtime),
        "agent" => Ok(RuntimeCounterScope::Agent(crate::AgentId::new(scope_id))),
        "instance" => Ok(RuntimeCounterScope::Instance(crate::AgentInstanceId::new(
            scope_id,
        ))),
        "session" => Ok(RuntimeCounterScope::Session(crate::SessionId::new(
            scope_id,
        ))),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime counter scope {other}:{scope_id}"),
        )),
    }
}

fn row_to_runtime_counter(row: &Row) -> CoreResult<RuntimeCounterRecord> {
    let scope_type: String = row.get(0);
    let scope_id: String = row.get(1);
    let value: i64 = row.get(3);
    if value < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("runtime counter value must not be negative: {value}"),
        ));
    }
    Ok(RuntimeCounterRecord {
        scope: runtime_counter_scope_from_parts(&scope_type, &scope_id)?,
        counter_name: row.get(2),
        value: value as u64,
        updated_at: row.get(4),
    })
}
