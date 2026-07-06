//! SQLite `simple_kv` repository domain.
//!
//! The simple key/value store is a Crew module-data concern backed by the
//! module schema registry. This module owns its SQLite facade methods, row
//! mapping, validation, and expiry behavior so the crate entrypoint does not
//! accumulate repository-domain SQL.

use super::*;

impl CoordinationStore {
    pub fn get_simple_kv(
        &self,
        scope: &SimpleKvScope,
        key: &str,
        now: Option<&IsoTimestamp>,
    ) -> CoreResult<Option<SimpleKvRecord>> {
        validate_simple_kv_identity(scope, key)?;
        let conn = self.conn()?;
        get_simple_kv(&conn, scope, key, now)
    }
    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        validate_simple_kv_query(query)?;
        let conn = self.conn()?;
        list_simple_kv(&conn, query)
    }
    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start put simple kv", error))?;
        let record = put_simple_kv_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit put simple kv", error))?;
        Ok(record)
    }
    pub fn compare_and_swap_simple_kv(
        &self,
        compare_and_swap: &SimpleKvCompareAndSwap,
    ) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(&compare_and_swap.write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start compare-and-swap simple kv", error))?;
        let existing = get_simple_kv(
            &tx,
            &compare_and_swap.write.scope,
            &compare_and_swap.write.key,
            None,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "simple_kv entry {}/{} not found",
                    compare_and_swap.write.scope.scope_id, compare_and_swap.write.key
                ),
            )
        })?;
        if existing.revision != compare_and_swap.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "simple_kv revision mismatch for {}/{}: expected {}, found {}",
                    compare_and_swap.write.scope.scope_id,
                    compare_and_swap.write.key,
                    compare_and_swap.expected_revision,
                    existing.revision
                ),
            ));
        }
        let record = update_simple_kv_in_tx(&tx, &compare_and_swap.write, existing.revision + 1)?;
        tx.commit()
            .map_err(|error| persistence_error("commit compare-and-swap simple kv", error))?;
        Ok(record)
    }
    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_identity(&delete.scope, &delete.key)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start delete simple kv", error))?;
        let existing = get_simple_kv(&tx, &delete.scope, &delete.key, None)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "simple_kv entry {}/{} not found",
                    delete.scope.scope_id, delete.key
                ),
            )
        })?;
        if existing.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "simple_kv revision mismatch for {}/{}: expected {}, found {}",
                    delete.scope.scope_id, delete.key, delete.expected_revision, existing.revision
                ),
            ));
        }
        tx.execute(
            "DELETE FROM module_simple_kv_entries
             WHERE scope_type = ?1 AND scope_id = ?2 AND entry_key = ?3",
            params![
                delete.scope.scope_type.as_str(),
                delete.scope.scope_id.as_str(),
                delete.key.as_str()
            ],
        )
        .map_err(|error| persistence_error("delete simple kv", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit delete simple kv", error))?;
        Ok(existing)
    }
    pub fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
        let conn = self.conn()?;
        expire_simple_kv(&conn, now)
    }
}

fn list_simple_kv(conn: &Connection, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
    validate_simple_kv_query(query)?;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let now = query.now.as_deref();
    let key_prefix = query
        .key_prefix
        .as_ref()
        .map(|prefix| sqlite_like_prefix(prefix));
    let mut stmt = conn
        .prepare(
            "SELECT
                scope_type,
                scope_id,
                entry_key,
                value_json,
                revision,
                created_at,
                updated_at,
                expires_at
             FROM module_simple_kv_entries
             WHERE scope_type = ?1
               AND scope_id = ?2
               AND (?3 IS NULL OR entry_key LIKE ?3 ESCAPE '\\')
               AND (
                    (?4 AND expires_at IS NOT NULL AND ?5 IS NOT NULL AND expires_at <= ?5)
                    OR
                    (NOT ?4 AND (?6 OR expires_at IS NULL OR ?5 IS NULL OR expires_at > ?5))
               )
             ORDER BY entry_key ASC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|error| persistence_error("prepare list simple kv", error))?;
    let rows = stmt
        .query_map(
            params![
                query.scope.scope_type.as_str(),
                query.scope.scope_id.as_str(),
                key_prefix.as_deref(),
                query.expired_only,
                now,
                query.include_expired,
                limit,
                offset
            ],
            row_to_simple_kv,
        )
        .map_err(|error| persistence_error("query simple kv", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load simple kv", error))
}

fn get_simple_kv(
    conn: &Connection,
    scope: &SimpleKvScope,
    key: &str,
    now: Option<&IsoTimestamp>,
) -> CoreResult<Option<SimpleKvRecord>> {
    validate_simple_kv_identity(scope, key)?;
    conn.query_row(
        "SELECT
            scope_type,
            scope_id,
            entry_key,
            value_json,
            revision,
            created_at,
            updated_at,
            expires_at
         FROM module_simple_kv_entries
         WHERE scope_type = ?1
           AND scope_id = ?2
           AND entry_key = ?3
           AND (expires_at IS NULL OR ?4 IS NULL OR expires_at > ?4)",
        params![scope.scope_type.as_str(), scope.scope_id.as_str(), key, now],
        row_to_simple_kv,
    )
    .optional()
    .map_err(|error| persistence_error("get simple kv", error))
}

fn put_simple_kv_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SimpleKvWrite,
) -> CoreResult<SimpleKvRecord> {
    let existing = get_simple_kv(tx, &write.scope, &write.key, None)?;
    match existing {
        Some(record) => update_simple_kv_in_tx(tx, write, record.revision + 1),
        None => insert_simple_kv_in_tx(tx, write),
    }
}

fn insert_simple_kv_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SimpleKvWrite,
) -> CoreResult<SimpleKvRecord> {
    let value_json = to_json_text(&write.value_json)?;
    tx.execute(
        "INSERT INTO module_simple_kv_entries (
            scope_type,
            scope_id,
            entry_key,
            value_json,
            revision,
            created_at,
            updated_at,
            expires_at
        ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, ?6)",
        params![
            write.scope.scope_type.as_str(),
            write.scope.scope_id.as_str(),
            write.key.as_str(),
            value_json,
            write.now.as_str(),
            write.expires_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert simple kv", error))?;
    Ok(SimpleKvRecord {
        scope: write.scope.clone(),
        key: write.key.clone(),
        value_json: write.value_json.clone(),
        revision: 1,
        created_at: write.now.clone(),
        updated_at: write.now.clone(),
        expires_at: write.expires_at.clone(),
    })
}

fn update_simple_kv_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SimpleKvWrite,
    revision: u64,
) -> CoreResult<SimpleKvRecord> {
    let value_json = to_json_text(&write.value_json)?;
    let created_at = get_simple_kv(tx, &write.scope, &write.key, None)?
        .map(|record| record.created_at)
        .unwrap_or_else(|| write.now.clone());
    tx.execute(
        "UPDATE module_simple_kv_entries
         SET value_json = ?4,
             revision = ?5,
             updated_at = ?6,
             expires_at = ?7
         WHERE scope_type = ?1
           AND scope_id = ?2
           AND entry_key = ?3",
        params![
            write.scope.scope_type.as_str(),
            write.scope.scope_id.as_str(),
            write.key.as_str(),
            value_json,
            revision as i64,
            write.now.as_str(),
            write.expires_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("update simple kv", error))?;
    Ok(SimpleKvRecord {
        scope: write.scope.clone(),
        key: write.key.clone(),
        value_json: write.value_json.clone(),
        revision,
        created_at,
        updated_at: write.now.clone(),
        expires_at: write.expires_at.clone(),
    })
}

fn expire_simple_kv(conn: &Connection, now: &IsoTimestamp) -> CoreResult<u64> {
    let changed = conn
        .execute(
            "DELETE FROM module_simple_kv_entries
             WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![now.as_str()],
        )
        .map_err(|error| persistence_error("expire simple kv", error))?;
    Ok(changed as u64)
}

fn row_to_simple_kv(row: &rusqlite::Row<'_>) -> rusqlite::Result<SimpleKvRecord> {
    let value_json: String = row.get(3)?;
    let revision: i64 = row.get(4)?;
    if revision <= 0 {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Integer,
            Box::new(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("invalid simple_kv revision {revision}"),
            )),
        ));
    }
    Ok(SimpleKvRecord {
        scope: SimpleKvScope {
            scope_type: row.get(0)?,
            scope_id: row.get(1)?,
        },
        key: row.get(2)?,
        value_json: from_json_text(&value_json).map_err(to_sql_error)?,
        revision: revision as u64,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        expires_at: row.get(7)?,
    })
}

pub(crate) fn validate_simple_kv_write(write: &SimpleKvWrite) -> CoreResult<()> {
    validate_simple_kv_identity(&write.scope, &write.key)
}

pub(crate) fn validate_simple_kv_query(query: &SimpleKvQuery) -> CoreResult<()> {
    validate_simple_kv_scope(&query.scope)?;
    if let Some(prefix) = &query.key_prefix {
        validate_simple_kv_part("key_prefix", prefix, 256)?;
    }
    if query.expired_only && query.now.is_none() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "simple_kv expired-only queries require now",
        ));
    }
    Ok(())
}

pub(crate) fn validate_simple_kv_identity(scope: &SimpleKvScope, key: &str) -> CoreResult<()> {
    validate_simple_kv_scope(scope)?;
    validate_simple_kv_part("key", key, 256)
}

fn validate_simple_kv_scope(scope: &SimpleKvScope) -> CoreResult<()> {
    validate_simple_kv_part("scope_type", &scope.scope_type, 64)?;
    validate_simple_kv_part("scope_id", &scope.scope_id, 256)
}

fn validate_simple_kv_part(label: &str, value: &str, max_bytes: usize) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("simple_kv {label} must be non-empty"),
        ));
    }
    if value.len() > max_bytes {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("simple_kv {label} exceeds {max_bytes} bytes"),
        ));
    }
    if value.contains('\0') {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("simple_kv {label} must not contain NUL bytes"),
        ));
    }
    Ok(())
}

fn sqlite_like_prefix(prefix: &str) -> String {
    let mut escaped = String::new();
    for character in prefix.chars() {
        match character {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped.push('%');
    escaped
}
