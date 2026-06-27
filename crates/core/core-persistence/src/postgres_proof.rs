//! Narrow PostgreSQL proof slice for the runtime counter repository.
//!
//! This module is intentionally not the full `CoordinationStore` backend. It
//! exists to prove connection, migration, typed API parity, and diagnostics for
//! one low-risk repository before correctness-sensitive coordination state moves
//! beyond SQLite.

use crate::{
    counter_value, from_json_text, repositories, to_json_text, validate_provider_wire_state_key,
    validate_simple_kv_identity, validate_simple_kv_query, validate_simple_kv_write,
    ActiveBranchConflict, ActiveBranchExpectation, ActiveVariantConflict, ActiveVariantExpectation,
    BranchHeadConflict, BranchHeadExpectation, ConversationBranchId, ConversationBranchQuery,
    ConversationBranchRecord, ConversationBranchStateRecord, ConversationBranchWrite,
    ConversationJumpRequest, ConversationJumpResult, ConversationJumpTarget,
    ConversationSnapshotId, ConversationSnapshotQuery, ConversationSnapshotRecord,
    ConversationSnapshotSource, ConversationSnapshotWrite, CoreError, CoreErrorKind, CoreResult,
    DurableMessageRecord, DurableMessageStatus, DurableMessageWrite, IsoTimestamp,
    MessageBlockRecord, MessageId, MessageSlotId, MessageSlotQuery, MessageSlotRecord,
    MessageSlotWrite, MessageVariantId, MessageVariantQuery, MessageVariantRecord,
    MessageVariantSource, MessageVariantStatus, MessageVariantWrite, ProviderStateAbsenceReason,
    ProviderWireStateDiagnostic, ProviderWireStateInvalidationReason, ProviderWireStateKey,
    ProviderWireStateRecord, ProviderWireStateWakeLookup, ProviderWireStateWakeResult,
    ProviderWireStateWrite, QueryPage, RuntimeCounterQuery, RuntimeCounterRecord,
    RuntimeCounterScope, RuntimeRepositoryGroupDiagnostic, RuntimeSearchFilter,
    RuntimeSearchResult, RuntimeSearchRowType, RuntimeStateSummary, RuntimeStorageCapability,
    RuntimeStorageTableCount, SelectActiveBranchRequest, SelectActiveBranchResult,
    SelectActiveVariantRequest, SelectActiveVariantResult, SessionId, SimpleKvCompareAndSwap,
    SimpleKvDelete, SimpleKvQuery, SimpleKvRecord, SimpleKvScope, SimpleKvWrite,
    UpdateBranchHeadRequest, UpdateBranchHeadResult, COUNTER_BRAIN_TURNS, COUNTER_COMPLETIONS,
    COUNTER_DELEGATIONS_CANCELLED, COUNTER_DELEGATIONS_COMPLETED, COUNTER_DELEGATIONS_CREATED,
    COUNTER_DELEGATIONS_FAILED, COUNTER_DELEGATIONS_TIMED_OUT, COUNTER_MESSAGES,
    COUNTER_QUEUE_EXPIRATIONS, COUNTER_TOOL_CALLS, COUNTER_TOOL_ERRORS, COUNTER_WAKES,
};
use postgres::{Client, GenericClient, NoTls, Row, Transaction};
use std::sync::{Mutex, MutexGuard};

const POSTGRES_PROOF_SCHEMA_VERSION: i64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeCounterProofConfig {
    pub database_url_env: String,
    pub schema: String,
}

impl Default for PostgresRuntimeCounterProofConfig {
    fn default() -> Self {
        Self {
            database_url_env: "RUSTY_CREW_DATABASE_URL".to_string(),
            schema: "rusty_crew".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeCounterProofDiagnostics {
    pub backend: String,
    pub backend_label: String,
    pub schema: String,
    pub proof_repository: String,
    pub schema_version: i64,
    pub table_counts: Vec<RuntimeStorageTableCount>,
    pub capabilities: Vec<RuntimeStorageCapability>,
    pub repository_groups: Vec<RuntimeRepositoryGroupDiagnostic>,
}

pub struct PostgresRuntimeCounterProofStore {
    schema: String,
    client: Mutex<Client>,
}

impl PostgresRuntimeCounterProofStore {
    pub fn connect_from_env(config: &PostgresRuntimeCounterProofConfig) -> CoreResult<Self> {
        let database_url = std::env::var(&config.database_url_env).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "load PostgreSQL proof database URL from env {}: {error}",
                    config.database_url_env
                ),
            )
        })?;
        Self::connect(&database_url, &config.schema)
    }

    pub fn connect(database_url: &str, schema: &str) -> CoreResult<Self> {
        validate_postgres_identifier("postgres schema", schema)?;
        let client = Client::connect(database_url, NoTls)
            .map_err(|error| postgres_error("connect PostgreSQL runtime counter proof", error))?;
        let store = Self {
            schema: schema.to_string(),
            client: Mutex::new(client),
        };
        store.migrate()?;
        Ok(store)
    }

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
        let counters = self.runtime_counters(Some(scope))?;
        Ok(RuntimeStateSummary {
            scope: scope.clone(),
            brain_turns: counter_value(&counters, COUNTER_BRAIN_TURNS),
            wakes: counter_value(&counters, COUNTER_WAKES),
            tool_calls: counter_value(&counters, COUNTER_TOOL_CALLS),
            tool_errors: counter_value(&counters, COUNTER_TOOL_ERRORS),
            delegations_created: counter_value(&counters, COUNTER_DELEGATIONS_CREATED),
            delegations_completed: counter_value(&counters, COUNTER_DELEGATIONS_COMPLETED),
            delegations_failed: counter_value(&counters, COUNTER_DELEGATIONS_FAILED),
            delegations_timed_out: counter_value(&counters, COUNTER_DELEGATIONS_TIMED_OUT),
            delegations_cancelled: counter_value(&counters, COUNTER_DELEGATIONS_CANCELLED),
            messages: counter_value(&counters, COUNTER_MESSAGES),
            completions: counter_value(&counters, COUNTER_COMPLETIONS),
            queue_expirations: counter_value(&counters, COUNTER_QUEUE_EXPIRATIONS),
        })
    }

    pub fn get_simple_kv(
        &self,
        scope: &SimpleKvScope,
        key: &str,
        now: Option<&IsoTimestamp>,
    ) -> CoreResult<Option<SimpleKvRecord>> {
        validate_simple_kv_identity(scope, key)?;
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "SELECT scope_type, scope_id, entry_key, value_json, revision,
                            created_at, updated_at, expires_at
                     FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND entry_key = $3
                       AND (expires_at IS NULL OR $4::text IS NULL OR expires_at > $4)"
                ),
                &[&scope.scope_type, &scope.scope_id, &key, &now],
            )
            .map_err(|error| postgres_error("get PostgreSQL simple_kv entry", error))?;
        row.as_ref().map(row_to_simple_kv).transpose()
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        validate_simple_kv_query(query)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let key_prefix = query
            .key_prefix
            .as_ref()
            .map(|prefix| postgres_like_prefix(prefix));
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT scope_type, scope_id, entry_key, value_json, revision,
                            created_at, updated_at, expires_at
                     FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND ($3::text IS NULL OR entry_key LIKE $3 ESCAPE '\\')
                       AND (
                            ($4 AND expires_at IS NOT NULL AND $5::text IS NOT NULL AND expires_at <= $5)
                            OR
                            (NOT $4 AND ($6 OR expires_at IS NULL OR $5::text IS NULL OR expires_at > $5))
                       )
                     ORDER BY entry_key ASC
                     LIMIT $7 OFFSET $8"
                ),
                &[
                    &query.scope.scope_type,
                    &query.scope.scope_id,
                    &key_prefix,
                    &query.expired_only,
                    &query.now,
                    &query.include_expired,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("list PostgreSQL simple_kv entries", error))?;
        rows.iter().map(row_to_simple_kv).collect()
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(write)?;
        let existing = self.get_simple_kv(&write.scope, &write.key, None)?;
        match existing {
            Some(existing) => self.update_simple_kv(write, existing.revision + 1),
            None => self.insert_simple_kv(write),
        }
    }

    pub fn compare_and_swap_simple_kv(
        &self,
        compare_and_swap: &SimpleKvCompareAndSwap,
    ) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(&compare_and_swap.write)?;
        let existing = self
            .get_simple_kv(
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
        self.update_simple_kv(&compare_and_swap.write, existing.revision + 1)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_identity(&delete.scope, &delete.key)?;
        let existing = self
            .get_simple_kv(&delete.scope, &delete.key, None)?
            .ok_or_else(|| {
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
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1 AND scope_id = $2 AND entry_key = $3"
                ),
                &[
                    &delete.scope.scope_type,
                    &delete.scope.scope_id,
                    &delete.key,
                ],
            )
            .map_err(|error| postgres_error("delete PostgreSQL simple_kv entry", error))?;
        Ok(existing)
    }

    pub fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let changed = self
            .client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_simple_kv_entries
                     WHERE expires_at IS NOT NULL AND expires_at <= $1"
                ),
                &[now],
            )
            .map_err(|error| postgres_error("expire PostgreSQL simple_kv entries", error))?;
        Ok(changed)
    }

    pub fn storage_diagnostics(&self) -> CoreResult<PostgresRuntimeCounterProofDiagnostics> {
        Ok(PostgresRuntimeCounterProofDiagnostics {
            backend: "postgres".to_string(),
            backend_label: "PostgreSQL runtime-counter proof slice".to_string(),
            schema: self.schema.clone(),
            proof_repository:
                "runtime_counters,module_simple_kv_entries,runtime_search,provider_wire_states"
                    .to_string(),
            schema_version: self.schema_version()?,
            table_counts: vec![
                RuntimeStorageTableCount {
                    table: "runtime_counters".to_string(),
                    rows: self.runtime_counter_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "module_simple_kv_entries".to_string(),
                    rows: self.simple_kv_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "runtime_search_entries".to_string(),
                    rows: self.runtime_search_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "provider_wire_states".to_string(),
                    rows: self.provider_wire_state_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "message_slots".to_string(),
                    rows: self.table_rows("message_slots")?,
                },
                RuntimeStorageTableCount {
                    table: "message_variants".to_string(),
                    rows: self.table_rows("message_variants")?,
                },
                RuntimeStorageTableCount {
                    table: "messages".to_string(),
                    rows: self.table_rows("messages")?,
                },
                RuntimeStorageTableCount {
                    table: "conversation_branches".to_string(),
                    rows: self.table_rows("conversation_branches")?,
                },
                RuntimeStorageTableCount {
                    table: "conversation_snapshots".to_string(),
                    rows: self.table_rows("conversation_snapshots")?,
                },
            ],
            capabilities: postgres_proof_capabilities(),
            repository_groups: postgres_proof_repository_groups(),
        })
    }

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        let metadata_json = to_json_text(&slot.metadata_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL message slot", error))?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.message_slots (
                    slot_id,
                    session_id,
                    primary_variant_id,
                    active_variant_id,
                    metadata_json,
                    created_at,
                    updated_at,
                    version
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, 0)
                 ON CONFLICT(slot_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    primary_variant_id = EXCLUDED.primary_variant_id,
                    active_variant_id = EXCLUDED.active_variant_id,
                    metadata_json = EXCLUDED.metadata_json,
                    updated_at = EXCLUDED.updated_at,
                    version = message_slots.version + 1"
            ),
            &[
                &slot.slot_id.0,
                &slot.session_id.0,
                &slot.primary_variant_id.0,
                &slot
                    .active_variant_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &metadata_json,
                &slot.created_at,
                &slot.updated_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL message slot", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL message slot", error))?;
        Ok(())
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL message variant", error))?;
        save_message_variant_in_tx(&mut tx, &schema, variant)?;
        let record = load_message_variant_in_tx(&mut tx, &schema, &variant.variant_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL message variant", error))?;
        Ok(record)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        let schema = self.quoted_schema();
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let mut client = self.client()?;
        let rows = client
            .query(
                &format!(
                    "SELECT slot_id
                     FROM {schema}.message_slots
                     WHERE ($1::text IS NULL OR session_id = $1)
                     ORDER BY created_at ASC, slot_id ASC
                     LIMIT $2 OFFSET $3"
                ),
                &[&session_id, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL message slots", error))?;
        rows.iter()
            .map(|row| MessageSlotId::new(row.get::<_, String>(0)))
            .map(|slot_id| {
                load_message_slot(&mut *client, &schema, &slot_id, query.include_alternates)
            })
            .collect()
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_message_variants(&mut *client, &schema, query)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start select PostgreSQL active message variant", error)
        })?;
        let current = current_active_variant_in_tx(&mut tx, &schema, &request.slot_id)?;
        let expected = match &request.expected {
            ActiveVariantExpectation::Any => current.clone(),
            ActiveVariantExpectation::Primary => None,
            ActiveVariantExpectation::Variant(variant_id) => Some(variant_id.clone()),
        };
        if request.expected != ActiveVariantExpectation::Any && current != expected {
            let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL active variant conflict", error)
            })?;
            return Ok(SelectActiveVariantResult {
                slot,
                conflict: Some(ActiveVariantConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(variant_id) = &request.active_variant_id {
            ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, &request.slot_id, variant_id)?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET active_variant_id = $2,
                     updated_at = $3,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[
                &request.slot_id.0,
                &request
                    .active_variant_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("select PostgreSQL active message variant", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
        tx.commit().map_err(|error| {
            postgres_error("commit select PostgreSQL active message variant", error)
        })?;
        Ok(SelectActiveVariantResult {
            slot,
            conflict: None,
        })
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        let metadata_json = to_json_text(&branch.metadata_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL conversation branch", error))?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.conversation_branches (
                    branch_id,
                    session_id,
                    parent_branch_id,
                    parent_message_id,
                    origin_message_id,
                    head_message_id,
                    label,
                    metadata_json,
                    created_at,
                    updated_at,
                    version
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)
                 ON CONFLICT(branch_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    parent_branch_id = EXCLUDED.parent_branch_id,
                    parent_message_id = EXCLUDED.parent_message_id,
                    origin_message_id = EXCLUDED.origin_message_id,
                    head_message_id = EXCLUDED.head_message_id,
                    label = EXCLUDED.label,
                    metadata_json = EXCLUDED.metadata_json,
                    updated_at = EXCLUDED.updated_at,
                    version = conversation_branches.version + 1"
            ),
            &[
                &branch.branch_id.0,
                &branch.session_id.0,
                &branch
                    .parent_branch_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch
                    .parent_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch
                    .origin_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch
                    .head_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch.label,
                &metadata_json,
                &branch.created_at,
                &branch.updated_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL conversation branch", error))?;
        let record = load_conversation_branch_in_tx(&mut tx, &schema, &branch.branch_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL conversation branch", error))?;
        Ok(record)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        let schema = self.quoted_schema();
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let parent_branch_id = query
            .parent_branch_id
            .as_ref()
            .map(|value| value.0.as_str());
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let mut client = self.client()?;
        let rows = client
            .query(
                &format!(
                    "SELECT branch_id
                     FROM {schema}.conversation_branches
                     WHERE ($1::text IS NULL OR session_id = $1)
                       AND ($2::text IS NULL OR parent_branch_id = $2)
                     ORDER BY created_at ASC, branch_id ASC
                     LIMIT $3 OFFSET $4"
                ),
                &[&session_id, &parent_branch_id, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL conversation branches", error))?;
        rows.iter()
            .map(|row| ConversationBranchId::new(row.get::<_, String>(0)))
            .map(|branch_id| load_conversation_branch(&mut *client, &schema, &branch_id))
            .collect()
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        load_conversation_branch_state(&mut *client, &schema, session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start select PostgreSQL active conversation branch", error)
        })?;
        let current = current_active_branch_in_tx(&mut tx, &schema, &request.session_id)?;
        let expected = match &request.expected {
            ActiveBranchExpectation::Any => current.clone(),
            ActiveBranchExpectation::None => None,
            ActiveBranchExpectation::Branch(branch_id) => Some(branch_id.clone()),
        };
        if request.expected != ActiveBranchExpectation::Any && current != expected {
            let state = load_conversation_branch_state_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                &request.updated_at,
            )?;
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL active branch conflict", error)
            })?;
            return Ok(SelectActiveBranchResult {
                state,
                conflict: Some(ActiveBranchConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(branch_id) = &request.active_branch_id {
            ensure_branch_belongs_to_session_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                branch_id,
            )?;
        }
        let changed = if current.is_none() {
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.conversation_branch_state (
                        session_id,
                        active_branch_id,
                        updated_at,
                        version
                     ) VALUES ($1, $2, $3, 0)
                     ON CONFLICT(session_id) DO NOTHING"
                ),
                &[
                    &request.session_id.0,
                    &request
                        .active_branch_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    &request.updated_at,
                ],
            )
        } else {
            tx.execute(
                &format!(
                    "UPDATE {schema}.conversation_branch_state
                     SET active_branch_id = $2,
                         updated_at = $3,
                         version = version + 1
                     WHERE session_id = $1"
                ),
                &[
                    &request.session_id.0,
                    &request
                        .active_branch_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    &request.updated_at,
                ],
            )
        }
        .map_err(|error| postgres_error("select PostgreSQL active branch", error))?;
        if changed == 0 {
            let state = load_conversation_branch_state_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                &request.updated_at,
            )?;
            let actual = state.active_branch_id.clone();
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL active branch insert conflict", error)
            })?;
            return Ok(SelectActiveBranchResult {
                state,
                conflict: Some(ActiveBranchConflict { expected, actual }),
            });
        }
        let state = load_conversation_branch_state_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.updated_at,
        )?;
        tx.commit().map_err(|error| {
            postgres_error("commit select PostgreSQL active conversation branch", error)
        })?;
        Ok(SelectActiveBranchResult {
            state,
            conflict: None,
        })
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start update PostgreSQL branch head", error))?;
        let current = current_branch_head_in_tx(&mut tx, &schema, &request.branch_id)?;
        let expected = match &request.expected {
            BranchHeadExpectation::Any => current.clone(),
            BranchHeadExpectation::None => None,
            BranchHeadExpectation::Message(message_id) => Some(message_id.clone()),
        };
        if request.expected != BranchHeadExpectation::Any && current != expected {
            let branch = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)?;
            tx.commit()
                .map_err(|error| postgres_error("commit PostgreSQL branch head conflict", error))?;
            return Ok(UpdateBranchHeadResult {
                branch,
                conflict: Some(BranchHeadConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(message_id) = &request.head_message_id {
            ensure_message_exists_in_tx(&mut tx, &schema, message_id)?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.conversation_branches
                 SET head_message_id = $2,
                     updated_at = $3,
                     version = version + 1
                 WHERE branch_id = $1"
            ),
            &[
                &request.branch_id.0,
                &request
                    .head_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL branch head", error))?;
        let branch = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit update PostgreSQL branch head", error))?;
        Ok(UpdateBranchHeadResult {
            branch,
            conflict: None,
        })
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        let metadata_json = to_json_text(&snapshot.metadata_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start save PostgreSQL conversation snapshot", error)
        })?;
        let source = conversation_snapshot_source_as_str(snapshot.source);
        tx.execute(
            &format!(
                "INSERT INTO {schema}.conversation_snapshots (
                    snapshot_id,
                    session_id,
                    branch_id,
                    message_id,
                    cursor,
                    label,
                    summary,
                    source,
                    metadata_json,
                    created_at,
                    updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT(snapshot_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    branch_id = EXCLUDED.branch_id,
                    message_id = EXCLUDED.message_id,
                    cursor = EXCLUDED.cursor,
                    label = EXCLUDED.label,
                    summary = EXCLUDED.summary,
                    source = EXCLUDED.source,
                    metadata_json = EXCLUDED.metadata_json,
                    updated_at = EXCLUDED.updated_at"
            ),
            &[
                &snapshot.snapshot_id.0,
                &snapshot.session_id.0,
                &snapshot.branch_id.as_ref().map(|value| value.0.as_str()),
                &snapshot.message_id.as_ref().map(|value| value.0.as_str()),
                &snapshot.cursor,
                &snapshot.label,
                &snapshot.summary,
                &source,
                &metadata_json,
                &snapshot.created_at,
                &snapshot.updated_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL conversation snapshot", error))?;
        let record = load_conversation_snapshot_in_tx(&mut tx, &schema, &snapshot.snapshot_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit save PostgreSQL conversation snapshot", error)
        })?;
        Ok(record)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        let schema = self.quoted_schema();
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let branch_id = query.branch_id.as_ref().map(|value| value.0.as_str());
        let message_id = query.message_id.as_ref().map(|value| value.0.as_str());
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let mut client = self.client()?;
        let rows = client
            .query(
                &format!(
                    "SELECT snapshot_id
                     FROM {schema}.conversation_snapshots
                     WHERE ($1::text IS NULL OR session_id = $1)
                       AND ($2::text IS NULL OR branch_id = $2)
                       AND ($3::text IS NULL OR message_id = $3)
                     ORDER BY created_at ASC, snapshot_id ASC
                     LIMIT $4 OFFSET $5"
                ),
                &[&session_id, &branch_id, &message_id, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL conversation snapshots", error))?;
        rows.iter()
            .map(|row| ConversationSnapshotId::new(row.get::<_, String>(0)))
            .map(|snapshot_id| load_conversation_snapshot(&mut *client, &schema, &snapshot_id))
            .collect()
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        resolve_conversation_jump(&mut *client, &schema, request)
    }

    pub fn save_provider_wire_state(
        &self,
        write: &ProviderWireStateWrite,
    ) -> CoreResult<ProviderWireStateRecord> {
        validate_provider_wire_state_key(&write.key)?;
        let payload_json = to_json_text(&write.payload_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL provider wire state", error))?;
        invalidate_current_provider_wire_state_for_key_in_tx(
            &mut tx,
            &schema,
            &write.key,
            &write.now,
            ProviderWireStateInvalidationReason::Superseded,
        )?;
        let row = tx
            .query_one(
                &format!(
                    "INSERT INTO {schema}.provider_wire_states (
                        session_id,
                        module_id,
                        strategy_id,
                        profile_fingerprint,
                        provider_fingerprint,
                        payload_version,
                        payload_json,
                        payload_encoding,
                        created_at,
                        updated_at,
                        expires_at,
                        last_wake_id,
                        invalidated_at,
                        invalidation_reason
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'json', $8, $8, $9, $10, NULL, NULL)
                     RETURNING row_id,
                        session_id,
                        module_id,
                        strategy_id,
                        profile_fingerprint,
                        provider_fingerprint,
                        payload_version,
                        payload_json,
                        payload_encoding,
                        created_at,
                        updated_at,
                        expires_at,
                        last_wake_id,
                        invalidated_at,
                        invalidation_reason"
                ),
                &[
                    &write.key.session_id.0,
                    &write.key.module_id,
                    &write.key.strategy_id,
                    &write.profile_fingerprint,
                    &write.provider_fingerprint,
                    &write.payload_version,
                    &payload_json,
                    &write.now,
                    &write.expires_at,
                    &write.last_wake_id,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL provider wire state", error))?;
        let record = row_to_provider_wire_state_record(&row)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL provider wire state", error))?;
        Ok(record)
    }

    pub fn load_provider_wire_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult> {
        validate_provider_wire_state_key(&lookup.key)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start load PostgreSQL provider wire state", error))?;
        invalidate_provider_wire_states_for_session_except_in_tx(
            &mut tx,
            &schema,
            &lookup.key,
            &lookup.now,
        )?;
        let Some(record) = load_current_provider_wire_state_by_key(&mut tx, &schema, &lookup.key)?
        else {
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit missing PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Missing),
            });
        };
        if record
            .expires_at
            .as_ref()
            .is_some_and(|expires| expires <= &lookup.now)
        {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                &lookup.now,
                ProviderWireStateInvalidationReason::Expired,
            )?;
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit expired PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Expired),
            });
        }
        if record.profile_fingerprint != lookup.profile_fingerprint {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                &lookup.now,
                ProviderWireStateInvalidationReason::ProfileChanged,
            )?;
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit profile-invalidated PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
            });
        }
        if record.provider_fingerprint != lookup.provider_fingerprint {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                &lookup.now,
                ProviderWireStateInvalidationReason::ProviderChanged,
            )?;
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit provider-invalidated PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
            });
        }
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL provider wire state lookup", error)
        })?;
        Ok(ProviderWireStateWakeResult {
            record: Some(record),
            absence_reason: None,
        })
    }

    pub fn clear_provider_wire_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<Option<ProviderWireStateRecord>> {
        validate_provider_wire_state_key(key)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start clear PostgreSQL provider wire state", error))?;
        let Some(record) = load_current_provider_wire_state_by_key(&mut tx, &schema, key)? else {
            tx.commit().map_err(|error| {
                postgres_error("commit missing PostgreSQL provider wire state clear", error)
            })?;
            return Ok(None);
        };
        invalidate_provider_wire_state_by_row_in_tx(&mut tx, &schema, record.row_id, now, reason)?;
        let cleared = load_provider_wire_state_by_row_id(&mut tx, &schema, record.row_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit clear PostgreSQL provider wire state", error)
        })?;
        Ok(Some(cleared))
    }

    pub fn expire_provider_wire_states_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ProviderWireStateRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start expire PostgreSQL provider wire states", error)
        })?;
        let expiring = load_expired_current_provider_wire_states(&mut tx, &schema, now)?;
        for record in &expiring {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                now,
                ProviderWireStateInvalidationReason::Expired,
            )?;
        }
        let expired = expiring
            .iter()
            .map(|record| load_provider_wire_state_by_row_id(&mut tx, &schema, record.row_id))
            .collect::<CoreResult<Vec<_>>>()?;
        tx.commit().map_err(|error| {
            postgres_error("commit expire PostgreSQL provider wire states", error)
        })?;
        Ok(expired)
    }

    pub fn list_provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        let schema = self.quoted_schema();
        let limit = i64::from(limit);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT
                        session_id,
                        module_id,
                        strategy_id,
                        payload_version,
                        octet_length(payload_json)::bigint,
                        created_at,
                        updated_at,
                        expires_at,
                        last_wake_id,
                        invalidated_at,
                        invalidation_reason
                     FROM {schema}.provider_wire_states
                     ORDER BY updated_at DESC, row_id DESC
                     LIMIT $1"
                ),
                &[&limit],
            )
            .map_err(|error| postgres_error("list PostgreSQL provider wire diagnostics", error))?;
        rows.iter()
            .map(row_to_provider_wire_state_diagnostic)
            .collect()
    }

    pub fn upsert_runtime_search_entry(&self, entry: &RuntimeSearchResult) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let row_type = runtime_search_row_type_as_str(entry.row_type);
        let event_kind = entry.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_search_entries (
                        row_type,
                        row_key,
                        sequence,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at,
                        title,
                        body
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                     ON CONFLICT(row_type, row_key) DO UPDATE SET
                        sequence = EXCLUDED.sequence,
                        session_id = EXCLUDED.session_id,
                        agent_id = EXCLUDED.agent_id,
                        instance_id = EXCLUDED.instance_id,
                        task_id = EXCLUDED.task_id,
                        event_kind = EXCLUDED.event_kind,
                        recorded_at = EXCLUDED.recorded_at,
                        title = EXCLUDED.title,
                        body = EXCLUDED.body"
                ),
                &[
                    &row_type,
                    &entry.row_key,
                    &entry.sequence.map(|value| value as i64),
                    &entry.session_id.as_ref().map(|value| value.0.as_str()),
                    &entry.agent_id.as_ref().map(|value| value.0.as_str()),
                    &entry.instance_id.as_ref().map(|value| value.0.as_str()),
                    &entry.task_id.as_ref().map(|value| value.0.as_str()),
                    &event_kind,
                    &entry.recorded_at,
                    &entry.title,
                    &entry.body,
                ],
            )
            .map_err(|error| postgres_error("upsert PostgreSQL runtime search entry", error))?;
        Ok(())
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        if filter.query.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "runtime search query must be non-empty",
            ));
        }
        let row_type = filter.row_type.map(runtime_search_row_type_as_str);
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let task_id = filter.task_id.as_ref().map(|value| value.0.as_str());
        let event_kind = filter.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        let recorded_after = filter.recorded_after.as_deref();
        let recorded_before = filter.recorded_before.as_deref();
        let limit = filter.limit.unwrap_or(50).clamp(1, 200) as i64;
        let query = filter.query.trim();
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT
                        row_type,
                        row_key,
                        sequence,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at,
                        title,
                        body
                     FROM {schema}.runtime_search_entries
                     WHERE search_vector @@ plainto_tsquery('simple', $1)
                       AND ($2::text IS NULL OR row_type = $2)
                       AND ($3::text IS NULL OR session_id = $3)
                       AND ($4::text IS NULL OR agent_id = $4)
                       AND ($5::text IS NULL OR instance_id = $5)
                       AND ($6::text IS NULL OR task_id = $6)
                       AND ($7::text IS NULL OR event_kind = $7)
                       AND ($8::text IS NULL OR recorded_at >= $8)
                       AND ($9::text IS NULL OR recorded_at <= $9)
                     ORDER BY
                       ts_rank(search_vector, plainto_tsquery('simple', $1)) DESC,
                       recorded_at ASC,
                       row_type ASC,
                       row_key ASC
                     LIMIT $10"
                ),
                &[
                    &query,
                    &row_type,
                    &session_id,
                    &agent_id,
                    &instance_id,
                    &task_id,
                    &event_kind,
                    &recorded_after,
                    &recorded_before,
                    &limit,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime search", error))?;
        rows.iter().map(row_to_runtime_search_result).collect()
    }

    #[cfg(test)]
    fn drop_schema_for_test(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .map_err(|error| postgres_error("drop PostgreSQL proof schema", error))
    }

    fn migrate(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {schema};
                 CREATE TABLE IF NOT EXISTS {schema}.rusty_crew_storage_metadata (
                    metadata_key TEXT PRIMARY KEY,
                    metadata_value TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 INSERT INTO {schema}.rusty_crew_storage_metadata (
                    metadata_key,
                    metadata_value,
                    updated_at
                 ) VALUES (
                    'runtime_counter_proof_schema_version',
                    '{POSTGRES_PROOF_SCHEMA_VERSION}',
                    '2026-06-26T00:00:00Z'
                 )
                 ON CONFLICT(metadata_key) DO UPDATE SET
                    metadata_value = EXCLUDED.metadata_value,
                    updated_at = EXCLUDED.updated_at;
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_counters (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    counter_name TEXT NOT NULL,
                    value BIGINT NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(scope_type, scope_id, counter_name)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_counters_scope_idx
                    ON {schema}.runtime_counters(scope_type, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.module_simple_kv_entries (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    entry_key TEXT NOT NULL,
                    value_json TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT,
                    PRIMARY KEY(scope_type, scope_id, entry_key)
                 );
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_scope_key_idx
                    ON {schema}.module_simple_kv_entries(scope_type, scope_id, entry_key);
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_expires_at_idx
                    ON {schema}.module_simple_kv_entries(expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_search_entries (
                    row_type TEXT NOT NULL,
                    row_key TEXT NOT NULL,
                    sequence BIGINT,
                    session_id TEXT,
                    agent_id TEXT,
                    instance_id TEXT,
                    task_id TEXT,
                    event_kind TEXT,
                    recorded_at TEXT NOT NULL,
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    search_vector TSVECTOR GENERATED ALWAYS AS (
                        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, ''))
                    ) STORED,
                    PRIMARY KEY(row_type, row_key)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_vector_idx
                    ON {schema}.runtime_search_entries USING GIN(search_vector);
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_metadata_idx
                    ON {schema}.runtime_search_entries(
                        row_type,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at
                    );
                 CREATE TABLE IF NOT EXISTS {schema}.provider_wire_states (
                    row_id BIGSERIAL PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    module_id TEXT NOT NULL,
                    strategy_id TEXT NOT NULL,
                    profile_fingerprint TEXT NOT NULL,
                    provider_fingerprint TEXT NOT NULL,
                    payload_version TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    payload_encoding TEXT NOT NULL DEFAULT 'json',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT,
                    last_wake_id TEXT,
                    invalidated_at TEXT,
                    invalidation_reason TEXT
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS provider_wire_states_current_idx
                    ON {schema}.provider_wire_states(session_id, module_id, strategy_id)
                    WHERE invalidated_at IS NULL;
                 CREATE INDEX IF NOT EXISTS provider_wire_states_session_current_idx
                    ON {schema}.provider_wire_states(session_id, invalidated_at);
                 CREATE INDEX IF NOT EXISTS provider_wire_states_expiry_idx
                    ON {schema}.provider_wire_states(invalidated_at, expires_at);
                 CREATE INDEX IF NOT EXISTS provider_wire_states_updated_idx
                    ON {schema}.provider_wire_states(updated_at DESC, row_id DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.message_slots (
                    slot_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    primary_variant_id TEXT NOT NULL,
                    active_variant_id TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS message_slots_session_slot_idx
                    ON {schema}.message_slots(session_id, slot_id);
                 CREATE INDEX IF NOT EXISTS message_slots_active_variant_idx
                    ON {schema}.message_slots(active_variant_id);
                 CREATE TABLE IF NOT EXISTS {schema}.messages (
                    message_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    branch_id TEXT,
                    parent_message_id TEXT,
                    previous_message_id TEXT,
                    author_id TEXT NOT NULL,
                    author_role TEXT NOT NULL,
                    status TEXT NOT NULL,
                    body TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS messages_session_created_idx
                    ON {schema}.messages(session_id, created_at, message_id);
                 CREATE INDEX IF NOT EXISTS messages_session_branch_idx
                    ON {schema}.messages(session_id, branch_id);
                 CREATE INDEX IF NOT EXISTS messages_parent_message_idx
                    ON {schema}.messages(parent_message_id);
                 CREATE TABLE IF NOT EXISTS {schema}.message_blocks (
                    block_id TEXT PRIMARY KEY,
                    message_id TEXT NOT NULL REFERENCES {schema}.messages(message_id),
                    ordinal BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    content_json TEXT NOT NULL,
                    render_policy_json TEXT,
                    metadata_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS message_blocks_message_ordinal_idx
                    ON {schema}.message_blocks(message_id, ordinal);
                 CREATE TABLE IF NOT EXISTS {schema}.message_variants (
                    variant_id TEXT PRIMARY KEY,
                    slot_id TEXT NOT NULL REFERENCES {schema}.message_slots(slot_id),
                    source TEXT NOT NULL,
                    ordinal BIGINT NOT NULL,
                    status TEXT NOT NULL,
                    message_id TEXT NOT NULL REFERENCES {schema}.messages(message_id),
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS message_variants_slot_ordinal_idx
                    ON {schema}.message_variants(slot_id, ordinal);
                 CREATE INDEX IF NOT EXISTS message_variants_slot_status_idx
                    ON {schema}.message_variants(slot_id, status, ordinal);
                 CREATE INDEX IF NOT EXISTS message_variants_message_idx
                    ON {schema}.message_variants(message_id);
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_branches (
                    branch_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    parent_branch_id TEXT,
                    parent_message_id TEXT,
                    origin_message_id TEXT,
                    head_message_id TEXT,
                    label TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS conversation_branches_session_branch_idx
                    ON {schema}.conversation_branches(session_id, branch_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_parent_branch_idx
                    ON {schema}.conversation_branches(parent_branch_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_parent_message_idx
                    ON {schema}.conversation_branches(parent_message_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_session_created_idx
                    ON {schema}.conversation_branches(session_id, created_at, branch_id);
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_branch_state (
                    session_id TEXT PRIMARY KEY,
                    active_branch_id TEXT,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_snapshots (
                    snapshot_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    branch_id TEXT,
                    message_id TEXT,
                    cursor TEXT,
                    label TEXT,
                    summary TEXT,
                    source TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_message_idx
                    ON {schema}.conversation_snapshots(session_id, message_id);
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_branch_idx
                    ON {schema}.conversation_snapshots(session_id, branch_id, created_at);
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_created_idx
                    ON {schema}.conversation_snapshots(session_id, created_at, snapshot_id);"
            ))
            .map_err(|error| postgres_error("migrate PostgreSQL runtime counter proof", error))
    }

    fn insert_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        let value_json = to_json_text(&write.value_json)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.module_simple_kv_entries (
                        scope_type,
                        scope_id,
                        entry_key,
                        value_json,
                        revision,
                        created_at,
                        updated_at,
                        expires_at
                     ) VALUES ($1, $2, $3, $4, 1, $5, $5, $6)"
                ),
                &[
                    &write.scope.scope_type,
                    &write.scope.scope_id,
                    &write.key,
                    &value_json,
                    &write.now,
                    &write.expires_at,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL simple_kv entry", error))?;
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

    fn update_simple_kv(&self, write: &SimpleKvWrite, revision: u64) -> CoreResult<SimpleKvRecord> {
        validate_counter_amount(revision)?;
        let existing = self.get_simple_kv(&write.scope, &write.key, None)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| write.now.clone());
        let value_json = to_json_text(&write.value_json)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.module_simple_kv_entries
                     SET value_json = $4,
                         revision = $5,
                         updated_at = $6,
                         expires_at = $7
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND entry_key = $3"
                ),
                &[
                    &write.scope.scope_type,
                    &write.scope.scope_id,
                    &write.key,
                    &value_json,
                    &(revision as i64),
                    &write.now,
                    &write.expires_at,
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL simple_kv entry", error))?;
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

    fn schema_version(&self) -> CoreResult<i64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!(
                    "SELECT metadata_value
                     FROM {schema}.rusty_crew_storage_metadata
                     WHERE metadata_key = 'runtime_counter_proof_schema_version'"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL proof schema version", error))?;
        let raw: String = row.get(0);
        raw.parse::<i64>().map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL proof schema version: {error}"),
            )
        })
    }

    fn runtime_counter_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.runtime_counters"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL runtime counters", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn simple_kv_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.module_simple_kv_entries"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL simple_kv entries", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn runtime_search_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.runtime_search_entries"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL runtime search entries", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn provider_wire_state_rows(&self) -> CoreResult<u64> {
        self.table_rows("provider_wire_states")
    }

    fn table_rows(&self, table: &str) -> CoreResult<u64> {
        validate_postgres_identifier("postgres table", table)?;
        let schema = self.quoted_schema();
        let table = quote_postgres_identifier(table);
        let row = self
            .client()?
            .query_one(&format!("SELECT COUNT(*) FROM {schema}.{table}"), &[])
            .map_err(|error| postgres_error("count PostgreSQL proof table rows", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn quoted_schema(&self) -> String {
        quote_postgres_identifier(&self.schema)
    }

    fn client(&self) -> CoreResult<MutexGuard<'_, Client>> {
        self.client.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "PostgreSQL proof connection mutex poisoned",
            )
        })
    }
}

fn postgres_proof_capabilities() -> Vec<RuntimeStorageCapability> {
    [
        (
            "transactions",
            true,
            "PostgreSQL transactions are available for the proof repository",
        ),
        (
            "json_metadata",
            true,
            "PostgreSQL can store JSON metadata, though runtime counters do not need it",
        ),
        (
            "concurrent_writers",
            true,
            "PostgreSQL supports concurrent writers for this proof repository",
        ),
        (
            "estimated_table_size",
            true,
            "the proof slice exposes row counts for proof-owned tables",
        ),
        (
            "row_level_claims",
            false,
            "not exercised by the runtime counter proof slice",
        ),
        (
            "runtime_full_text_search",
            true,
            "PostgreSQL runtime search proof uses tsvector behind the typed RuntimeSearchFilter API",
        ),
        (
            "logical_export_import",
            false,
            "logical cross-backend export/import remains future work",
        ),
    ]
    .into_iter()
    .map(|(name, supported, detail)| RuntimeStorageCapability {
        name: name.to_string(),
        supported,
        detail: detail.to_string(),
    })
    .collect()
}

fn postgres_proof_repository_groups() -> Vec<RuntimeRepositoryGroupDiagnostic> {
    repositories::core_repository_group_diagnostics()
        .into_iter()
        .map(|mut group| {
            if group.group_id == "storage_admin" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for proof-owned migrations and storage diagnostics only.".to_string(),
                );
            } else if group.group_id == "module_schema_registry" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented only for the simple_kv module-owned data table, not the full module registry.".to_string(),
                );
            } else if group.group_id == "runtime_counters" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for the runtime-counter proof repository.".to_string(),
                );
            } else if group.group_id == "runtime_search" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for runtime search entries through the typed search API; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "provider_state" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for provider wire-state conformance through the typed provider-state API; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "conversations_attachments" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: partially implemented for conversation branches, messages, variants, snapshots, and jumps; attachments and data-bank scopes remain unsupported.".to_string(),
                );
            } else {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: unsupported; full service boot must fail closed before using this repository group.".to_string(),
                );
            }
            group
        })
        .collect()
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

fn row_to_simple_kv(row: &Row) -> CoreResult<SimpleKvRecord> {
    let value_json: String = row.get(3);
    let revision: i64 = row.get(4);
    if revision <= 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid simple_kv revision {revision}"),
        ));
    }
    Ok(SimpleKvRecord {
        scope: SimpleKvScope {
            scope_type: row.get(0),
            scope_id: row.get(1),
        },
        key: row.get(2),
        value_json: from_json_text(&value_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL simple_kv value_json: {error}"),
            )
        })?,
        revision: revision as u64,
        created_at: row.get(5),
        updated_at: row.get(6),
        expires_at: row.get(7),
    })
}

fn row_to_runtime_search_result(row: &Row) -> CoreResult<RuntimeSearchResult> {
    let row_type: String = row.get(0);
    let sequence: Option<i64> = row.get(2);
    let event_kind: Option<String> = row.get(7);
    Ok(RuntimeSearchResult {
        row_type: runtime_search_row_type_from_str(&row_type)?,
        row_key: row.get(1),
        sequence: sequence.map(|value| value as u64),
        session_id: row.get::<_, Option<String>>(3).map(crate::SessionId),
        agent_id: row.get::<_, Option<String>>(4).map(crate::AgentId),
        instance_id: row.get::<_, Option<String>>(5).map(crate::AgentInstanceId),
        task_id: row.get::<_, Option<String>>(6).map(crate::TaskId),
        event_kind: event_kind
            .as_deref()
            .map(core_event_kind_from_debug_str)
            .transpose()?,
        recorded_at: row.get(8),
        title: row.get(9),
        body: row.get(10),
    })
}

fn save_message_variant_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    variant: &MessageVariantWrite,
) -> CoreResult<()> {
    if variant.source == MessageVariantSource::Primary && variant.ordinal != 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "primary message variant ordinal must be 0",
        ));
    }
    save_durable_message_in_tx(tx, schema, &variant.message)?;
    let metadata_json = to_json_text(&variant.metadata_json)?;
    let source = message_variant_source_as_str(variant.source);
    let status = message_variant_status_as_str(variant.status);
    tx.execute(
        &format!(
            "INSERT INTO {schema}.message_variants (
                variant_id,
                slot_id,
                source,
                ordinal,
                status,
                message_id,
                metadata_json,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT(variant_id) DO UPDATE SET
                slot_id = EXCLUDED.slot_id,
                source = EXCLUDED.source,
                ordinal = EXCLUDED.ordinal,
                status = EXCLUDED.status,
                message_id = EXCLUDED.message_id,
                metadata_json = EXCLUDED.metadata_json,
                updated_at = EXCLUDED.updated_at"
        ),
        &[
            &variant.variant_id.0,
            &variant.slot_id.0,
            &source,
            &(variant.ordinal as i64),
            &status,
            &variant.message.message_id.0,
            &metadata_json,
            &variant.created_at,
            &variant.updated_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL message variant", error))?;
    Ok(())
}

fn save_durable_message_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    message: &DurableMessageWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&message.metadata_json)?;
    let status = durable_message_status_as_str(message.status);
    tx.execute(
        &format!(
            "INSERT INTO {schema}.messages (
                message_id,
                session_id,
                branch_id,
                parent_message_id,
                previous_message_id,
                author_id,
                author_role,
                status,
                body,
                metadata_json,
                created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(message_id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                branch_id = EXCLUDED.branch_id,
                parent_message_id = EXCLUDED.parent_message_id,
                previous_message_id = EXCLUDED.previous_message_id,
                author_id = EXCLUDED.author_id,
                author_role = EXCLUDED.author_role,
                status = EXCLUDED.status,
                body = EXCLUDED.body,
                metadata_json = EXCLUDED.metadata_json"
        ),
        &[
            &message.message_id.0,
            &message.session_id.0,
            &message.branch_id.as_ref().map(|value| value.0.as_str()),
            &message
                .parent_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            &message
                .previous_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            &message.author_id,
            &message.author_role,
            &status,
            &message.body,
            &metadata_json,
            &message.created_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL durable message", error))?;
    tx.execute(
        &format!("DELETE FROM {schema}.message_blocks WHERE message_id = $1"),
        &[&message.message_id.0],
    )
    .map_err(|error| postgres_error("replace PostgreSQL message blocks", error))?;
    for block in &message.blocks {
        let content_json = to_json_text(&block.content_json)?;
        let render_policy_json = block
            .render_policy_json
            .as_ref()
            .map(to_json_text)
            .transpose()?;
        let metadata_json = to_json_text(&block.metadata_json)?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.message_blocks (
                    block_id,
                    message_id,
                    ordinal,
                    kind,
                    content_json,
                    render_policy_json,
                    metadata_json
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)"
            ),
            &[
                &block.block_id.0,
                &message.message_id.0,
                &(block.ordinal as i64),
                &block.kind,
                &content_json,
                &render_policy_json,
                &metadata_json,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL message block", error))?;
    }
    Ok(())
}

fn query_message_variants<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &MessageVariantQuery,
) -> CoreResult<Vec<MessageVariantRecord>> {
    let slot_id = query.slot_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let rows = conn
        .query(
            &format!(
                "SELECT variant_id
                 FROM {schema}.message_variants
                 WHERE ($1::text IS NULL OR slot_id = $1)
                   AND ($2 OR status <> 'deleted')
                 ORDER BY slot_id ASC, ordinal ASC, variant_id ASC
                 LIMIT $3 OFFSET $4"
            ),
            &[&slot_id, &query.include_deleted, &limit, &offset],
        )
        .map_err(|error| postgres_error("query PostgreSQL message variants", error))?;
    rows.iter()
        .map(|row| MessageVariantId::new(row.get::<_, String>(0)))
        .map(|variant_id| load_message_variant(conn, schema, &variant_id))
        .collect()
}

fn load_message_slot<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        primary_variant_id,
                        active_variant_id,
                        metadata_json,
                        created_at,
                        updated_at,
                        version
                 FROM {schema}.message_slots
                 WHERE slot_id = $1"
            ),
            &[&slot_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL message slot", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message slot {slot_id} not found"),
            )
        })?;
    row_to_message_slot(conn, schema, slot_id, include_alternates, &row)
}

fn load_message_slot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    load_message_slot(tx, schema, slot_id, include_alternates)
}

fn row_to_message_slot<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    slot_id: &MessageSlotId,
    include_alternates: bool,
    row: &Row,
) -> CoreResult<MessageSlotRecord> {
    let metadata_json: String = row.get(3);
    let version: i64 = row.get(6);
    if version < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL message slot version {version}"),
        ));
    }
    let primary_variant_id = MessageVariantId::new(row.get::<_, String>(1));
    let primary = load_message_variant(conn, schema, &primary_variant_id)?;
    let alternates = if include_alternates {
        query_message_variants(
            conn,
            schema,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?
        .into_iter()
        .filter(|variant| variant.source == MessageVariantSource::Alternate)
        .collect()
    } else {
        Vec::new()
    };
    Ok(MessageSlotRecord {
        slot_id: slot_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        primary_variant_id,
        active_variant_id: row.get::<_, Option<String>>(2).map(MessageVariantId::new),
        metadata_json: parse_postgres_json(&metadata_json, "message slot metadata_json")?,
        created_at: row.get(4),
        updated_at: row.get(5),
        version: version as u64,
        primary,
        alternates,
    })
}

fn load_message_variant<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT slot_id,
                        source,
                        ordinal,
                        status,
                        message_id,
                        metadata_json,
                        created_at,
                        updated_at
                 FROM {schema}.message_variants
                 WHERE variant_id = $1"
            ),
            &[&variant_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL message variant", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message variant {variant_id} not found"),
            )
        })?;
    row_to_message_variant(conn, schema, variant_id, &row)
}

fn load_message_variant_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    load_message_variant(tx, schema, variant_id)
}

fn row_to_message_variant<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    variant_id: &MessageVariantId,
    row: &Row,
) -> CoreResult<MessageVariantRecord> {
    let ordinal: i64 = row.get(2);
    if ordinal < 0 || ordinal > u32::MAX as i64 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL message variant ordinal {ordinal}"),
        ));
    }
    let source: String = row.get(1);
    let status: String = row.get(3);
    let message_id = MessageId::new(row.get::<_, String>(4));
    let metadata_json: String = row.get(5);
    Ok(MessageVariantRecord {
        variant_id: variant_id.clone(),
        slot_id: MessageSlotId::new(row.get::<_, String>(0)),
        source: message_variant_source_from_str(&source)?,
        ordinal: ordinal as u32,
        status: message_variant_status_from_str(&status)?,
        message: load_durable_message(conn, schema, &message_id)?,
        metadata_json: parse_postgres_json(&metadata_json, "message variant metadata_json")?,
        created_at: row.get(6),
        updated_at: row.get(7),
    })
}

fn load_durable_message<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    message_id: &MessageId,
) -> CoreResult<DurableMessageRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        branch_id,
                        parent_message_id,
                        previous_message_id,
                        author_id,
                        author_role,
                        status,
                        body,
                        metadata_json,
                        created_at
                 FROM {schema}.messages
                 WHERE message_id = $1"
            ),
            &[&message_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL durable message", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message {message_id} not found"),
            )
        })?;
    row_to_durable_message(conn, schema, message_id, &row)
}

fn row_to_durable_message<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    message_id: &MessageId,
    row: &Row,
) -> CoreResult<DurableMessageRecord> {
    let status: String = row.get(6);
    let metadata_json: String = row.get(8);
    Ok(DurableMessageRecord {
        message_id: message_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        branch_id: row
            .get::<_, Option<String>>(1)
            .map(ConversationBranchId::new),
        parent_message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        previous_message_id: row.get::<_, Option<String>>(3).map(MessageId::new),
        author_id: row.get(4),
        author_role: row.get(5),
        status: durable_message_status_from_str(&status)?,
        body: row.get(7),
        metadata_json: parse_postgres_json(&metadata_json, "durable message metadata_json")?,
        created_at: row.get(9),
        blocks: load_message_blocks(conn, schema, message_id)?,
    })
}

fn load_message_blocks<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    message_id: &MessageId,
) -> CoreResult<Vec<MessageBlockRecord>> {
    let rows = conn
        .query(
            &format!(
                "SELECT block_id,
                        ordinal,
                        kind,
                        content_json,
                        render_policy_json,
                        metadata_json
                 FROM {schema}.message_blocks
                 WHERE message_id = $1
                 ORDER BY ordinal ASC, block_id ASC"
            ),
            &[&message_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL message blocks", error))?;
    rows.iter()
        .map(|row| row_to_message_block(row, message_id))
        .collect()
}

fn row_to_message_block(row: &Row, message_id: &MessageId) -> CoreResult<MessageBlockRecord> {
    let ordinal: i64 = row.get(1);
    if ordinal < 0 || ordinal > u32::MAX as i64 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL message block ordinal {ordinal}"),
        ));
    }
    let content_json: String = row.get(3);
    let render_policy_json: Option<String> = row.get(4);
    let metadata_json: String = row.get(5);
    Ok(MessageBlockRecord {
        block_id: crate::MessageBlockId::new(row.get::<_, String>(0)),
        message_id: message_id.clone(),
        ordinal: ordinal as u32,
        kind: row.get(2),
        content_json: parse_postgres_json(&content_json, "message block content_json")?,
        render_policy_json: render_policy_json
            .as_deref()
            .map(|value| parse_postgres_json(value, "message block render_policy_json"))
            .transpose()?,
        metadata_json: parse_postgres_json(&metadata_json, "message block metadata_json")?,
    })
}

fn load_conversation_branch<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        parent_branch_id,
                        parent_message_id,
                        origin_message_id,
                        head_message_id,
                        label,
                        metadata_json,
                        created_at,
                        updated_at,
                        version
                 FROM {schema}.conversation_branches
                 WHERE branch_id = $1"
            ),
            &[&branch_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL conversation branch", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("conversation branch {branch_id} not found"),
            )
        })?;
    row_to_conversation_branch(branch_id, &row)
}

fn load_conversation_branch_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    load_conversation_branch(tx, schema, branch_id)
}

fn row_to_conversation_branch(
    branch_id: &ConversationBranchId,
    row: &Row,
) -> CoreResult<ConversationBranchRecord> {
    let metadata_json: String = row.get(6);
    let version: i64 = row.get(9);
    if version < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL conversation branch version {version}"),
        ));
    }
    Ok(ConversationBranchRecord {
        branch_id: branch_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        parent_branch_id: row
            .get::<_, Option<String>>(1)
            .map(ConversationBranchId::new),
        parent_message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        origin_message_id: row.get::<_, Option<String>>(3).map(MessageId::new),
        head_message_id: row.get::<_, Option<String>>(4).map(MessageId::new),
        label: row.get(5),
        metadata_json: parse_postgres_json(&metadata_json, "conversation branch metadata_json")?,
        created_at: row.get(7),
        updated_at: row.get(8),
        version: version as u64,
    })
}

fn current_active_branch_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
) -> CoreResult<Option<ConversationBranchId>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT active_branch_id
                 FROM {schema}.conversation_branch_state
                 WHERE session_id = $1
                 FOR UPDATE"
            ),
            &[&session_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL current active branch", error))?;
    Ok(row
        .as_ref()
        .and_then(|row| row.get::<_, Option<String>>(0))
        .map(ConversationBranchId::new))
}

fn load_conversation_branch_state<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT active_branch_id, updated_at, version
                 FROM {schema}.conversation_branch_state
                 WHERE session_id = $1"
            ),
            &[&session_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL conversation branch state", error))?;
    row.as_ref()
        .map(|row| row_to_conversation_branch_state(session_id, row))
        .transpose()
        .map(|state| {
            state.unwrap_or_else(|| ConversationBranchStateRecord {
                session_id: session_id.clone(),
                active_branch_id: None,
                updated_at: default_updated_at.clone(),
                version: 0,
            })
        })
}

fn load_conversation_branch_state_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    load_conversation_branch_state(tx, schema, session_id, default_updated_at)
}

fn row_to_conversation_branch_state(
    session_id: &SessionId,
    row: &Row,
) -> CoreResult<ConversationBranchStateRecord> {
    let version: i64 = row.get(2);
    if version < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL conversation branch state version {version}"),
        ));
    }
    Ok(ConversationBranchStateRecord {
        session_id: session_id.clone(),
        active_branch_id: row
            .get::<_, Option<String>>(0)
            .map(ConversationBranchId::new),
        updated_at: row.get(1),
        version: version as u64,
    })
}

fn current_branch_head_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    branch_id: &ConversationBranchId,
) -> CoreResult<Option<MessageId>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT head_message_id
                 FROM {schema}.conversation_branches
                 WHERE branch_id = $1
                 FOR UPDATE"
            ),
            &[&branch_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL current branch head", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("conversation branch {branch_id} not found"),
            )
        })?;
    Ok(row.get::<_, Option<String>>(0).map(MessageId::new))
}

fn ensure_branch_belongs_to_session_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.conversation_branches
                    WHERE session_id = $1 AND branch_id = $2
                )"
            ),
            &[&session_id.0, &branch_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL branch session ownership", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found for session {session_id}"),
        ))
    }
}

fn ensure_message_exists_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    message_id: &MessageId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.messages
                    WHERE message_id = $1
                )"
            ),
            &[&message_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL durable message existence", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message {message_id} not found"),
        ))
    }
}

fn load_conversation_snapshot<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        branch_id,
                        message_id,
                        cursor,
                        label,
                        summary,
                        source,
                        metadata_json,
                        created_at,
                        updated_at
                 FROM {schema}.conversation_snapshots
                 WHERE snapshot_id = $1"
            ),
            &[&snapshot_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL conversation snapshot", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("conversation snapshot {snapshot_id} not found"),
            )
        })?;
    row_to_conversation_snapshot(snapshot_id, &row)
}

fn load_conversation_snapshot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    load_conversation_snapshot(tx, schema, snapshot_id)
}

fn row_to_conversation_snapshot(
    snapshot_id: &ConversationSnapshotId,
    row: &Row,
) -> CoreResult<ConversationSnapshotRecord> {
    let source: String = row.get(6);
    let metadata_json: String = row.get(7);
    Ok(ConversationSnapshotRecord {
        snapshot_id: snapshot_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        branch_id: row
            .get::<_, Option<String>>(1)
            .map(ConversationBranchId::new),
        message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        cursor: row.get(3),
        label: row.get(4),
        summary: row.get(5),
        source: conversation_snapshot_source_from_str(&source)?,
        metadata_json: parse_postgres_json(&metadata_json, "conversation snapshot metadata_json")?,
        created_at: row.get(8),
        updated_at: row.get(9),
    })
}

fn resolve_conversation_jump<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    request: &ConversationJumpRequest,
) -> CoreResult<ConversationJumpResult> {
    match &request.target {
        ConversationJumpTarget::Message { message_id } => {
            let message = load_durable_message(conn, schema, message_id)?;
            if message.session_id != request.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "message {message_id} not found for session {}",
                        request.session_id
                    ),
                ));
            }
            Ok(ConversationJumpResult {
                session_id: request.session_id.clone(),
                target: request.target.clone(),
                branch_id: message.branch_id,
                message_id: Some(message_id.clone()),
                cursor: None,
                snapshot_id: None,
            })
        }
        ConversationJumpTarget::Branch { branch_id } => {
            let branch = load_conversation_branch(conn, schema, branch_id)?;
            if branch.session_id != request.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "branch {branch_id} not found for session {}",
                        request.session_id
                    ),
                ));
            }
            Ok(ConversationJumpResult {
                session_id: request.session_id.clone(),
                target: request.target.clone(),
                branch_id: Some(branch.branch_id),
                message_id: branch
                    .head_message_id
                    .or(branch.origin_message_id)
                    .or(branch.parent_message_id),
                cursor: None,
                snapshot_id: None,
            })
        }
        ConversationJumpTarget::Snapshot { snapshot_id } => {
            let snapshot = load_conversation_snapshot(conn, schema, snapshot_id)?;
            if snapshot.session_id != request.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "snapshot {snapshot_id} not found for session {}",
                        request.session_id
                    ),
                ));
            }
            Ok(ConversationJumpResult {
                session_id: request.session_id.clone(),
                target: request.target.clone(),
                branch_id: snapshot.branch_id,
                message_id: snapshot.message_id,
                cursor: snapshot.cursor,
                snapshot_id: Some(snapshot.snapshot_id),
            })
        }
        ConversationJumpTarget::Cursor { cursor } => Ok(ConversationJumpResult {
            session_id: request.session_id.clone(),
            target: request.target.clone(),
            branch_id: None,
            message_id: None,
            cursor: Some(cursor.clone()),
            snapshot_id: None,
        }),
    }
}

fn current_active_variant_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    slot_id: &MessageSlotId,
) -> CoreResult<Option<MessageVariantId>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT active_variant_id
                 FROM {schema}.message_slots
                 WHERE slot_id = $1
                 FOR UPDATE"
            ),
            &[&slot_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL active message variant", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message slot {slot_id} not found"),
            )
        })?;
    Ok(row.get::<_, Option<String>>(0).map(MessageVariantId::new))
}

fn ensure_variant_belongs_to_slot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    slot_id: &MessageSlotId,
    variant_id: &MessageVariantId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.message_variants
                    WHERE slot_id = $1 AND variant_id = $2 AND status <> 'deleted'
                )"
            ),
            &[&slot_id.0, &variant_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL message variant slot", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message variant {variant_id} not found in slot {slot_id}"),
        ))
    }
}

fn row_to_provider_wire_state_record(row: &Row) -> CoreResult<ProviderWireStateRecord> {
    let payload_json: String = row.get(7);
    let invalidation_reason = row
        .get::<_, Option<String>>(14)
        .as_deref()
        .map(provider_wire_state_invalidation_reason_from_str)
        .transpose()?;
    Ok(ProviderWireStateRecord {
        row_id: row.get(0),
        key: ProviderWireStateKey {
            session_id: crate::SessionId(row.get(1)),
            module_id: row.get(2),
            strategy_id: row.get(3),
        },
        profile_fingerprint: row.get(4),
        provider_fingerprint: row.get(5),
        payload_version: row.get(6),
        payload_json: from_json_text(&payload_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL provider wire payload_json: {error}"),
            )
        })?,
        payload_encoding: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
        expires_at: row.get(11),
        last_wake_id: row.get(12),
        invalidated_at: row.get(13),
        invalidation_reason,
    })
}

fn row_to_provider_wire_state_diagnostic(row: &Row) -> CoreResult<ProviderWireStateDiagnostic> {
    let payload_bytes: i64 = row.get(4);
    if payload_bytes < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL provider wire payload byte count {payload_bytes}"),
        ));
    }
    Ok(ProviderWireStateDiagnostic {
        key: ProviderWireStateKey {
            session_id: crate::SessionId(row.get(0)),
            module_id: row.get(1),
            strategy_id: row.get(2),
        },
        payload_version: row.get(3),
        payload_bytes: payload_bytes as u64,
        created_at: row.get(5),
        updated_at: row.get(6),
        expires_at: row.get(7),
        last_wake_id: row.get(8),
        invalidated_at: row.get(9),
        invalidation_reason: row.get(10),
    })
}

fn invalidate_provider_wire_states_for_session_except_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        &format!(
            "UPDATE {schema}.provider_wire_states
             SET invalidated_at = $4,
                 updated_at = $4,
                 invalidation_reason = CASE
                     WHEN module_id != $2 THEN 'module_changed'
                     ELSE 'strategy_changed'
                 END
             WHERE session_id = $1
               AND invalidated_at IS NULL
               AND (module_id != $2 OR strategy_id != $3)"
        ),
        &[&key.session_id.0, &key.module_id, &key.strategy_id, now],
    )
    .map_err(|error| postgres_error("invalidate changed PostgreSQL provider wire states", error))?;
    Ok(())
}

fn invalidate_current_provider_wire_state_for_key_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    let reason = provider_wire_state_invalidation_reason_as_str(reason);
    tx.execute(
        &format!(
            "UPDATE {schema}.provider_wire_states
             SET invalidated_at = $4,
                 updated_at = $4,
                 invalidation_reason = $5
             WHERE session_id = $1
               AND module_id = $2
               AND strategy_id = $3
               AND invalidated_at IS NULL"
        ),
        &[
            &key.session_id.0,
            &key.module_id,
            &key.strategy_id,
            now,
            &reason,
        ],
    )
    .map_err(|error| postgres_error("invalidate current PostgreSQL provider wire state", error))?;
    Ok(())
}

fn invalidate_provider_wire_state_by_row_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    row_id: i64,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    let reason = provider_wire_state_invalidation_reason_as_str(reason);
    tx.execute(
        &format!(
            "UPDATE {schema}.provider_wire_states
             SET invalidated_at = $2,
                 updated_at = $2,
                 invalidation_reason = $3
             WHERE row_id = $1
               AND invalidated_at IS NULL"
        ),
        &[&row_id, now, &reason],
    )
    .map_err(|error| postgres_error("invalidate PostgreSQL provider wire state row", error))?;
    Ok(())
}

fn load_current_provider_wire_state_by_key(
    tx: &mut Transaction<'_>,
    schema: &str,
    key: &ProviderWireStateKey,
) -> CoreResult<Option<ProviderWireStateRecord>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT
                    row_id,
                    session_id,
                    module_id,
                    strategy_id,
                    profile_fingerprint,
                    provider_fingerprint,
                    payload_version,
                    payload_json,
                    payload_encoding,
                    created_at,
                    updated_at,
                    expires_at,
                    last_wake_id,
                    invalidated_at,
                    invalidation_reason
                 FROM {schema}.provider_wire_states
                 WHERE session_id = $1
                   AND module_id = $2
                   AND strategy_id = $3
                   AND invalidated_at IS NULL
                 LIMIT 1"
            ),
            &[&key.session_id.0, &key.module_id, &key.strategy_id],
        )
        .map_err(|error| postgres_error("load current PostgreSQL provider wire state", error))?;
    row.as_ref()
        .map(row_to_provider_wire_state_record)
        .transpose()
}

fn load_provider_wire_state_by_row_id(
    tx: &mut Transaction<'_>,
    schema: &str,
    row_id: i64,
) -> CoreResult<ProviderWireStateRecord> {
    let row = tx
        .query_one(
            &format!(
                "SELECT
                    row_id,
                    session_id,
                    module_id,
                    strategy_id,
                    profile_fingerprint,
                    provider_fingerprint,
                    payload_version,
                    payload_json,
                    payload_encoding,
                    created_at,
                    updated_at,
                    expires_at,
                    last_wake_id,
                    invalidated_at,
                    invalidation_reason
                 FROM {schema}.provider_wire_states
                 WHERE row_id = $1"
            ),
            &[&row_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL provider wire state by row id", error))?;
    row_to_provider_wire_state_record(&row)
}

fn load_expired_current_provider_wire_states(
    tx: &mut Transaction<'_>,
    schema: &str,
    now: &IsoTimestamp,
) -> CoreResult<Vec<ProviderWireStateRecord>> {
    let rows = tx
        .query(
            &format!(
                "SELECT
                    row_id,
                    session_id,
                    module_id,
                    strategy_id,
                    profile_fingerprint,
                    provider_fingerprint,
                    payload_version,
                    payload_json,
                    payload_encoding,
                    created_at,
                    updated_at,
                    expires_at,
                    last_wake_id,
                    invalidated_at,
                    invalidation_reason
                 FROM {schema}.provider_wire_states
                 WHERE invalidated_at IS NULL
                   AND expires_at IS NOT NULL
                   AND expires_at <= $1
                 ORDER BY expires_at ASC, row_id ASC"
            ),
            &[now],
        )
        .map_err(|error| {
            postgres_error(
                "load expired current PostgreSQL provider wire states",
                error,
            )
        })?;
    rows.iter().map(row_to_provider_wire_state_record).collect()
}

fn provider_wire_state_invalidation_reason_as_str(
    reason: ProviderWireStateInvalidationReason,
) -> &'static str {
    match reason {
        ProviderWireStateInvalidationReason::ProfileChanged => "profile_changed",
        ProviderWireStateInvalidationReason::ProviderChanged => "provider_changed",
        ProviderWireStateInvalidationReason::ModuleChanged => "module_changed",
        ProviderWireStateInvalidationReason::StrategyChanged => "strategy_changed",
        ProviderWireStateInvalidationReason::Expired => "expired",
        ProviderWireStateInvalidationReason::BrainRequestedClear => "brain_requested_clear",
        ProviderWireStateInvalidationReason::OperatorRequestedClear => "operator_requested_clear",
        ProviderWireStateInvalidationReason::Superseded => "superseded",
    }
}

fn provider_wire_state_invalidation_reason_from_str(
    raw: &str,
) -> CoreResult<ProviderWireStateInvalidationReason> {
    match raw {
        "profile_changed" => Ok(ProviderWireStateInvalidationReason::ProfileChanged),
        "provider_changed" => Ok(ProviderWireStateInvalidationReason::ProviderChanged),
        "module_changed" => Ok(ProviderWireStateInvalidationReason::ModuleChanged),
        "strategy_changed" => Ok(ProviderWireStateInvalidationReason::StrategyChanged),
        "expired" => Ok(ProviderWireStateInvalidationReason::Expired),
        "brain_requested_clear" => Ok(ProviderWireStateInvalidationReason::BrainRequestedClear),
        "operator_requested_clear" => {
            Ok(ProviderWireStateInvalidationReason::OperatorRequestedClear)
        }
        "superseded" => Ok(ProviderWireStateInvalidationReason::Superseded),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown provider wire state invalidation reason {other}"),
        )),
    }
}

fn message_variant_source_as_str(source: MessageVariantSource) -> &'static str {
    match source {
        MessageVariantSource::Primary => "primary",
        MessageVariantSource::Alternate => "alternate",
    }
}

fn message_variant_source_from_str(raw: &str) -> CoreResult<MessageVariantSource> {
    match raw {
        "primary" => Ok(MessageVariantSource::Primary),
        "alternate" => Ok(MessageVariantSource::Alternate),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown message variant source {other}"),
        )),
    }
}

fn message_variant_status_as_str(status: MessageVariantStatus) -> &'static str {
    match status {
        MessageVariantStatus::Active => "active",
        MessageVariantStatus::Deleted => "deleted",
    }
}

fn message_variant_status_from_str(raw: &str) -> CoreResult<MessageVariantStatus> {
    match raw {
        "active" => Ok(MessageVariantStatus::Active),
        "deleted" => Ok(MessageVariantStatus::Deleted),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown message variant status {other}"),
        )),
    }
}

fn durable_message_status_as_str(status: DurableMessageStatus) -> &'static str {
    match status {
        DurableMessageStatus::Created => "created",
        DurableMessageStatus::Streaming => "streaming",
        DurableMessageStatus::Completed => "completed",
        DurableMessageStatus::Failed => "failed",
        DurableMessageStatus::Deleted => "deleted",
    }
}

fn durable_message_status_from_str(raw: &str) -> CoreResult<DurableMessageStatus> {
    match raw {
        "created" => Ok(DurableMessageStatus::Created),
        "streaming" => Ok(DurableMessageStatus::Streaming),
        "completed" => Ok(DurableMessageStatus::Completed),
        "failed" => Ok(DurableMessageStatus::Failed),
        "deleted" => Ok(DurableMessageStatus::Deleted),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown durable message status {other}"),
        )),
    }
}

fn conversation_snapshot_source_as_str(source: ConversationSnapshotSource) -> &'static str {
    match source {
        ConversationSnapshotSource::User => "user",
        ConversationSnapshotSource::System => "system",
        ConversationSnapshotSource::Import => "import",
    }
}

fn conversation_snapshot_source_from_str(raw: &str) -> CoreResult<ConversationSnapshotSource> {
    match raw {
        "user" => Ok(ConversationSnapshotSource::User),
        "system" => Ok(ConversationSnapshotSource::System),
        "import" => Ok(ConversationSnapshotSource::Import),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown conversation snapshot source {other}"),
        )),
    }
}

fn parse_postgres_json(value: &str, label: &str) -> CoreResult<serde_json::Value> {
    from_json_text(value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse PostgreSQL {label}: {error}"),
        )
    })
}

fn runtime_search_row_type_as_str(row_type: RuntimeSearchRowType) -> &'static str {
    match row_type {
        RuntimeSearchRowType::Message => "message",
        RuntimeSearchRowType::QueueMessage => "queue_message",
        RuntimeSearchRowType::Session => "session",
    }
}

fn runtime_search_row_type_from_str(raw: &str) -> CoreResult<RuntimeSearchRowType> {
    match raw {
        "message" => Ok(RuntimeSearchRowType::Message),
        "queue_message" => Ok(RuntimeSearchRowType::QueueMessage),
        "session" => Ok(RuntimeSearchRowType::Session),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime search row type {other}"),
        )),
    }
}

fn core_event_kind_from_debug_str(raw: &str) -> CoreResult<crate::CoreEventKind> {
    match raw {
        "AgentMessageRouted" => Ok(crate::CoreEventKind::AgentMessageRouted),
        "SessionCreated" => Ok(crate::CoreEventKind::SessionCreated),
        "SessionArchived" => Ok(crate::CoreEventKind::SessionArchived),
        "BrainWakeRequested" => Ok(crate::CoreEventKind::BrainWakeRequested),
        "BrainEventObserved" => Ok(crate::CoreEventKind::BrainEventObserved),
        "BrainActionsAccepted" => Ok(crate::CoreEventKind::BrainActionsAccepted),
        "DelegationLifecycleObserved" => Ok(crate::CoreEventKind::DelegationLifecycleObserved),
        "CompletionPacketDelivered" => Ok(crate::CoreEventKind::CompletionPacketDelivered),
        "ExternalEventInjected" => Ok(crate::CoreEventKind::ExternalEventInjected),
        "DenDataUpdated" => Ok(crate::CoreEventKind::DenDataUpdated),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime search event kind {other}"),
        )),
    }
}

fn postgres_like_prefix(prefix: &str) -> String {
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

fn validate_counter_amount(amount: u64) -> CoreResult<()> {
    if amount > i64::MAX as u64 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "runtime counter increment exceeds PostgreSQL BIGINT range",
        ));
    }
    Ok(())
}

fn validate_postgres_identifier(label: &str, value: &str) -> CoreResult<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not be empty"),
        ));
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must start with an ASCII letter or underscore"),
        ));
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must contain only ASCII letters, digits, or underscores"),
        ));
    }
    Ok(())
}

fn quote_postgres_identifier(identifier: &str) -> String {
    format!("\"{identifier}\"")
}

fn postgres_error(context: &str, error: postgres::Error) -> CoreError {
    CoreError::new(
        CoreErrorKind::PersistenceFailure,
        format!("{context}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoordinationStore, MessageBlockWrite, COUNTER_MESSAGES, COUNTER_WAKES};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    trait SimpleKvConformanceStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord>;
        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord>;
        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>>;
        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>>;
        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord>;
        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64>;
    }

    trait ProviderWireStateConformanceStore {
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord>;
        fn load_provider_wire_state_for_wake(
            &self,
            lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult>;
        fn clear_provider_wire_state(
            &self,
            key: &ProviderWireStateKey,
            now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<Option<ProviderWireStateRecord>>;
        fn expire_provider_wire_states_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ProviderWireStateRecord>>;
        fn list_provider_wire_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>>;
    }

    trait ConversationConformanceStore {
        fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()>;
        fn save_message_variant(
            &self,
            variant: &MessageVariantWrite,
        ) -> CoreResult<MessageVariantRecord>;
        fn query_message_slots(
            &self,
            query: &MessageSlotQuery,
        ) -> CoreResult<Vec<MessageSlotRecord>>;
        fn query_message_variants(
            &self,
            query: &MessageVariantQuery,
        ) -> CoreResult<Vec<MessageVariantRecord>>;
        fn select_active_message_variant(
            &self,
            request: &SelectActiveVariantRequest,
        ) -> CoreResult<SelectActiveVariantResult>;
        fn save_conversation_branch(
            &self,
            branch: &ConversationBranchWrite,
        ) -> CoreResult<ConversationBranchRecord>;
        fn query_conversation_branches(
            &self,
            query: &ConversationBranchQuery,
        ) -> CoreResult<Vec<ConversationBranchRecord>>;
        fn get_conversation_branch_state(
            &self,
            session_id: &SessionId,
            default_updated_at: &IsoTimestamp,
        ) -> CoreResult<ConversationBranchStateRecord>;
        fn select_active_conversation_branch(
            &self,
            request: &SelectActiveBranchRequest,
        ) -> CoreResult<SelectActiveBranchResult>;
        fn update_conversation_branch_head(
            &self,
            request: &UpdateBranchHeadRequest,
        ) -> CoreResult<UpdateBranchHeadResult>;
        fn save_conversation_snapshot(
            &self,
            snapshot: &ConversationSnapshotWrite,
        ) -> CoreResult<ConversationSnapshotRecord>;
        fn query_conversation_snapshots(
            &self,
            query: &ConversationSnapshotQuery,
        ) -> CoreResult<Vec<ConversationSnapshotRecord>>;
        fn resolve_conversation_jump(
            &self,
            request: &ConversationJumpRequest,
        ) -> CoreResult<ConversationJumpResult>;
    }

    impl SimpleKvConformanceStore for CoordinationStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::put_simple_kv(self, write)
        }

        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::compare_and_swap_simple_kv(self, compare_and_swap)
        }

        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>> {
            CoordinationStore::get_simple_kv(self, scope, key, now)
        }

        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            CoordinationStore::list_simple_kv(self, query)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::delete_simple_kv(self, delete)
        }

        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
            CoordinationStore::expire_simple_kv(self, now)
        }
    }

    impl ProviderWireStateConformanceStore for CoordinationStore {
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord> {
            CoordinationStore::save_provider_wire_state(self, write)
        }

        fn load_provider_wire_state_for_wake(
            &self,
            lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult> {
            CoordinationStore::load_provider_wire_state_for_wake(self, lookup)
        }

        fn clear_provider_wire_state(
            &self,
            key: &ProviderWireStateKey,
            now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<Option<ProviderWireStateRecord>> {
            CoordinationStore::clear_provider_wire_state(self, key, now, reason)
        }

        fn expire_provider_wire_states_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ProviderWireStateRecord>> {
            CoordinationStore::expire_provider_wire_states_at(self, now)
        }

        fn list_provider_wire_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
            CoordinationStore::list_provider_wire_state_diagnostics(self, limit)
        }
    }

    impl ConversationConformanceStore for CoordinationStore {
        fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
            CoordinationStore::save_message_slot(self, slot)
        }

        fn save_message_variant(
            &self,
            variant: &MessageVariantWrite,
        ) -> CoreResult<MessageVariantRecord> {
            CoordinationStore::save_message_variant(self, variant)
        }

        fn query_message_slots(
            &self,
            query: &MessageSlotQuery,
        ) -> CoreResult<Vec<MessageSlotRecord>> {
            CoordinationStore::query_message_slots(self, query)
        }

        fn query_message_variants(
            &self,
            query: &MessageVariantQuery,
        ) -> CoreResult<Vec<MessageVariantRecord>> {
            CoordinationStore::query_message_variants(self, query)
        }

        fn select_active_message_variant(
            &self,
            request: &SelectActiveVariantRequest,
        ) -> CoreResult<SelectActiveVariantResult> {
            CoordinationStore::select_active_message_variant(self, request)
        }

        fn save_conversation_branch(
            &self,
            branch: &ConversationBranchWrite,
        ) -> CoreResult<ConversationBranchRecord> {
            CoordinationStore::save_conversation_branch(self, branch)
        }

        fn query_conversation_branches(
            &self,
            query: &ConversationBranchQuery,
        ) -> CoreResult<Vec<ConversationBranchRecord>> {
            CoordinationStore::query_conversation_branches(self, query)
        }

        fn get_conversation_branch_state(
            &self,
            session_id: &SessionId,
            default_updated_at: &IsoTimestamp,
        ) -> CoreResult<ConversationBranchStateRecord> {
            CoordinationStore::get_conversation_branch_state(self, session_id, default_updated_at)
        }

        fn select_active_conversation_branch(
            &self,
            request: &SelectActiveBranchRequest,
        ) -> CoreResult<SelectActiveBranchResult> {
            CoordinationStore::select_active_conversation_branch(self, request)
        }

        fn update_conversation_branch_head(
            &self,
            request: &UpdateBranchHeadRequest,
        ) -> CoreResult<UpdateBranchHeadResult> {
            CoordinationStore::update_conversation_branch_head(self, request)
        }

        fn save_conversation_snapshot(
            &self,
            snapshot: &ConversationSnapshotWrite,
        ) -> CoreResult<ConversationSnapshotRecord> {
            CoordinationStore::save_conversation_snapshot(self, snapshot)
        }

        fn query_conversation_snapshots(
            &self,
            query: &ConversationSnapshotQuery,
        ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
            CoordinationStore::query_conversation_snapshots(self, query)
        }

        fn resolve_conversation_jump(
            &self,
            request: &ConversationJumpRequest,
        ) -> CoreResult<ConversationJumpResult> {
            CoordinationStore::resolve_conversation_jump(self, request)
        }
    }

    impl SimpleKvConformanceStore for PostgresRuntimeCounterProofStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::put_simple_kv(self, write)
        }

        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::compare_and_swap_simple_kv(self, compare_and_swap)
        }

        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>> {
            PostgresRuntimeCounterProofStore::get_simple_kv(self, scope, key, now)
        }

        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            PostgresRuntimeCounterProofStore::list_simple_kv(self, query)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::delete_simple_kv(self, delete)
        }

        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
            PostgresRuntimeCounterProofStore::expire_simple_kv(self, now)
        }
    }

    impl ProviderWireStateConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord> {
            PostgresRuntimeCounterProofStore::save_provider_wire_state(self, write)
        }

        fn load_provider_wire_state_for_wake(
            &self,
            lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult> {
            PostgresRuntimeCounterProofStore::load_provider_wire_state_for_wake(self, lookup)
        }

        fn clear_provider_wire_state(
            &self,
            key: &ProviderWireStateKey,
            now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<Option<ProviderWireStateRecord>> {
            PostgresRuntimeCounterProofStore::clear_provider_wire_state(self, key, now, reason)
        }

        fn expire_provider_wire_states_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ProviderWireStateRecord>> {
            PostgresRuntimeCounterProofStore::expire_provider_wire_states_at(self, now)
        }

        fn list_provider_wire_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
            PostgresRuntimeCounterProofStore::list_provider_wire_state_diagnostics(self, limit)
        }
    }

    impl ConversationConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_message_slot(self, slot)
        }

        fn save_message_variant(
            &self,
            variant: &MessageVariantWrite,
        ) -> CoreResult<MessageVariantRecord> {
            PostgresRuntimeCounterProofStore::save_message_variant(self, variant)
        }

        fn query_message_slots(
            &self,
            query: &MessageSlotQuery,
        ) -> CoreResult<Vec<MessageSlotRecord>> {
            PostgresRuntimeCounterProofStore::query_message_slots(self, query)
        }

        fn query_message_variants(
            &self,
            query: &MessageVariantQuery,
        ) -> CoreResult<Vec<MessageVariantRecord>> {
            PostgresRuntimeCounterProofStore::query_message_variants(self, query)
        }

        fn select_active_message_variant(
            &self,
            request: &SelectActiveVariantRequest,
        ) -> CoreResult<SelectActiveVariantResult> {
            PostgresRuntimeCounterProofStore::select_active_message_variant(self, request)
        }

        fn save_conversation_branch(
            &self,
            branch: &ConversationBranchWrite,
        ) -> CoreResult<ConversationBranchRecord> {
            PostgresRuntimeCounterProofStore::save_conversation_branch(self, branch)
        }

        fn query_conversation_branches(
            &self,
            query: &ConversationBranchQuery,
        ) -> CoreResult<Vec<ConversationBranchRecord>> {
            PostgresRuntimeCounterProofStore::query_conversation_branches(self, query)
        }

        fn get_conversation_branch_state(
            &self,
            session_id: &SessionId,
            default_updated_at: &IsoTimestamp,
        ) -> CoreResult<ConversationBranchStateRecord> {
            PostgresRuntimeCounterProofStore::get_conversation_branch_state(
                self,
                session_id,
                default_updated_at,
            )
        }

        fn select_active_conversation_branch(
            &self,
            request: &SelectActiveBranchRequest,
        ) -> CoreResult<SelectActiveBranchResult> {
            PostgresRuntimeCounterProofStore::select_active_conversation_branch(self, request)
        }

        fn update_conversation_branch_head(
            &self,
            request: &UpdateBranchHeadRequest,
        ) -> CoreResult<UpdateBranchHeadResult> {
            PostgresRuntimeCounterProofStore::update_conversation_branch_head(self, request)
        }

        fn save_conversation_snapshot(
            &self,
            snapshot: &ConversationSnapshotWrite,
        ) -> CoreResult<ConversationSnapshotRecord> {
            PostgresRuntimeCounterProofStore::save_conversation_snapshot(self, snapshot)
        }

        fn query_conversation_snapshots(
            &self,
            query: &ConversationSnapshotQuery,
        ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
            PostgresRuntimeCounterProofStore::query_conversation_snapshots(self, query)
        }

        fn resolve_conversation_jump(
            &self,
            request: &ConversationJumpRequest,
        ) -> CoreResult<ConversationJumpResult> {
            PostgresRuntimeCounterProofStore::resolve_conversation_jump(self, request)
        }
    }

    #[test]
    fn validates_schema_identifiers_before_connecting() {
        let error = match PostgresRuntimeCounterProofStore::connect(
            "postgres://example.invalid/db",
            "bad-schema",
        ) {
            Ok(_) => panic!("invalid schema unexpectedly connected"),
            Err(error) => error,
        };
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn postgres_proof_repository_groups_mark_unsupported_service_repositories() {
        let groups = postgres_proof_repository_groups();
        assert!(groups.iter().any(
            |group| group.group_id == "storage_admin" && group.notes[0].contains("implemented")
        ));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "runtime_counters"
                && group.notes[0].contains("implemented")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "queues_messages"
                && group.notes[0].contains("unsupported")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "scheduler_jobs"
                && group.notes[0].contains("unsupported")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "module_schema_registry"
                && group.notes[0].contains("simple_kv")));
        assert!(groups.iter().any(|group| group.group_id == "runtime_search"
            && group.notes[0].contains("runtime search entries")));
        assert!(groups.iter().any(|group| group.group_id == "provider_state"
            && group.notes[0].contains("provider wire-state conformance")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "conversations_attachments"
                && group.notes[0].contains("partially implemented")
                && group.notes[0].contains("attachments")));
    }

    #[test]
    fn sqlite_simple_kv_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-simple-kv-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        simple_kv_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_provider_wire_state_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-provider-wire-state-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        provider_wire_state_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_conversation_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-conversation-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        conversation_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_runtime_counter_proof_matches_typed_counter_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_counter_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

        store
            .increment_counter(
                &RuntimeCounterScope::Runtime,
                COUNTER_MESSAGES,
                2,
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();
        store
            .increment_counter(
                &RuntimeCounterScope::Session(crate::SessionId::new("session-alpha")),
                COUNTER_WAKES,
                1,
                &"2026-06-26T00:00:01Z".to_string(),
            )
            .unwrap();

        let runtime_summary = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        let session_summary = store
            .runtime_summary(&RuntimeCounterScope::Session(crate::SessionId::new(
                "session-alpha",
            )))
            .unwrap();
        assert_eq!(runtime_summary.messages, 2);
        assert_eq!(session_summary.wakes, 1);

        let messages = store
            .query_runtime_counters(&RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(messages[0].value, 2);
        assert_eq!(
            store
                .reset_runtime_counters(
                    &RuntimeCounterQuery {
                        scope: Some(RuntimeCounterScope::Runtime),
                        counter_name: Some(COUNTER_MESSAGES.to_string()),
                        page: None,
                    },
                    "2026-06-26T00:00:02Z".to_string(),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap()
                .messages,
            0
        );

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.backend, "postgres");
        assert_eq!(diagnostics.schema_version, POSTGRES_PROOF_SCHEMA_VERSION);
        assert_eq!(diagnostics.repository_groups[0].group_id, "storage_admin");
        assert_eq!(diagnostics.table_counts[0].rows, 2);

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_simple_kv_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL simple_kv proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_simple_kv_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        simple_kv_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "module_simple_kv_entries")
                .map(|count| count.rows),
            Some(0)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "module_schema_registry" && group.notes[0].contains("simple_kv")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_runtime_search_proof_matches_typed_search_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL runtime search proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_runtime_search_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        runtime_search_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "runtime_search_entries")
                .map(|count| count.rows),
            Some(5)
        );
        assert!(diagnostics.capabilities.iter().any(|capability| {
            capability.name == "runtime_full_text_search" && capability.supported
        }));
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "runtime_search" && group.notes[0].contains("implemented")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_provider_wire_state_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL provider wire-state proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_provider_state_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        provider_wire_state_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "provider_wire_states")
                .map(|count| count.rows),
            Some(9)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "provider_state" && group.notes[0].contains("implemented")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_conversation_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL conversation proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_conversation_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        conversation_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "conversation_branches")
                .map(|count| count.rows),
            Some(2)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "message_variants")
                .map(|count| count.rows),
            Some(3)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "conversations_attachments"
                && group.notes[0].contains("partially implemented")
                && group.notes[0].contains("attachments")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_conversation_conflicts_apply_once_across_connections() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!(
                "skipping PostgreSQL conversation conflict proof; no database URL env is set"
            );
            return;
        };
        let schema = unique_schema("rusty_crew_conversation_conflict_proof");
        let setup = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        seed_conversation_conflict_fixture(&setup);

        let first = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        let second = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

        let active_branch_first = first
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conflict"),
                active_branch_id: Some(ConversationBranchId::new("branch-conflict-a")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T03:01:00Z".to_string(),
            })
            .unwrap();
        let active_branch_second = second
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conflict"),
                active_branch_id: Some(ConversationBranchId::new("branch-conflict-b")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T03:01:01Z".to_string(),
            })
            .unwrap();
        assert!(active_branch_first.conflict.is_none());
        assert_eq!(
            active_branch_second.conflict.unwrap().actual,
            Some(ConversationBranchId::new("branch-conflict-a"))
        );

        let head_first = first
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conflict-a"),
                head_message_id: Some(MessageId::new("message-conflict-a")),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-26T03:02:00Z".to_string(),
            })
            .unwrap();
        let head_second = second
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conflict-a"),
                head_message_id: Some(MessageId::new("message-conflict-b")),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-26T03:02:01Z".to_string(),
            })
            .unwrap();
        assert!(head_first.conflict.is_none());
        assert_eq!(
            head_second.conflict.unwrap().actual,
            Some(MessageId::new("message-conflict-a"))
        );

        let variant_first = first
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conflict"),
                active_variant_id: Some(MessageVariantId::new("variant-conflict-a")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T03:03:00Z".to_string(),
            })
            .unwrap();
        let variant_second = second
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conflict"),
                active_variant_id: Some(MessageVariantId::new("variant-conflict-b")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T03:03:01Z".to_string(),
            })
            .unwrap();
        assert!(variant_first.conflict.is_none());
        assert_eq!(
            variant_second.conflict.unwrap().actual,
            Some(MessageVariantId::new("variant-conflict-a"))
        );

        setup.drop_schema_for_test().unwrap();
    }

    fn simple_kv_conformance(store: &dyn SimpleKvConformanceStore) {
        let scope = SimpleKvScope {
            scope_type: "profile".to_string(),
            scope_id: "rusty-crew-runner".to_string(),
        };
        let first = store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "steady"}),
                now: "2026-06-26T00:00:00Z".to_string(),
                expires_at: None,
            })
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.value_json, json!({"style": "steady"}));

        let fetched = store
            .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:01:00Z".to_string()))
            .unwrap()
            .unwrap();
        assert_eq!(fetched, first);

        let second = store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "direct"}),
                now: "2026-06-26T00:02:00Z".to_string(),
                expires_at: Some("2026-06-26T01:00:00Z".to_string()),
            })
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.created_at, first.created_at);

        let stale = store
            .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
                write: SimpleKvWrite {
                    scope: scope.clone(),
                    key: "tone".to_string(),
                    value_json: json!({"style": "stale"}),
                    now: "2026-06-26T00:03:00Z".to_string(),
                    expires_at: None,
                },
                expected_revision: 1,
            })
            .unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::ActionRejected);

        let third = store
            .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
                write: SimpleKvWrite {
                    scope: scope.clone(),
                    key: "tone".to_string(),
                    value_json: json!({"style": "precise"}),
                    now: "2026-06-26T00:04:00Z".to_string(),
                    expires_at: Some("2026-06-26T00:05:00Z".to_string()),
                },
                expected_revision: 2,
            })
            .unwrap();
        assert_eq!(third.revision, 3);

        store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "working_set".to_string(),
                value_json: json!(["a", "b"]),
                now: "2026-06-26T00:04:30Z".to_string(),
                expires_at: None,
            })
            .unwrap();

        let visible = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: false,
                expired_only: false,
                now: Some("2026-06-26T00:04:45Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(
            visible
                .iter()
                .map(|record| record.key.as_str())
                .collect::<Vec<_>>(),
            vec!["tone", "working_set"]
        );

        let prefixed = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: Some("work".to_string()),
                include_expired: false,
                expired_only: false,
                now: Some("2026-06-26T00:04:45Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(prefixed.len(), 1);
        assert_eq!(prefixed[0].key, "working_set");

        assert!(store
            .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:05:01Z".to_string()))
            .unwrap()
            .is_none());
        let with_expired = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: true,
                expired_only: false,
                now: Some("2026-06-26T00:05:01Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(with_expired.len(), 2);
        let expired_only = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: true,
                expired_only: true,
                now: Some("2026-06-26T00:05:01Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(expired_only.len(), 1);
        assert_eq!(expired_only[0].key, "tone");

        assert_eq!(
            store
                .delete_simple_kv(&SimpleKvDelete {
                    scope: scope.clone(),
                    key: "working_set".to_string(),
                    expected_revision: 1,
                })
                .unwrap()
                .key,
            "working_set"
        );
        assert_eq!(
            store
                .expire_simple_kv(&"2026-06-26T00:05:01Z".to_string())
                .unwrap(),
            1
        );
        assert!(store
            .list_simple_kv(&SimpleKvQuery {
                scope,
                key_prefix: None,
                include_expired: true,
                expired_only: false,
                now: None,
                page: None,
            })
            .unwrap()
            .is_empty());
    }

    fn runtime_search_conformance(store: &PostgresRuntimeCounterProofStore) {
        for entry in runtime_search_fixture() {
            store.upsert_runtime_search_entry(&entry).unwrap();
        }

        let session = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tools".to_string(),
                row_type: Some(RuntimeSearchRowType::Session),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::SessionCreated),
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(session[0].row_type, RuntimeSearchRowType::Session);

        let beta_message = store
            .search_runtime(&RuntimeSearchFilter {
                query: "needle".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_after: Some("2026-06-26T00:00:00Z".to_string()),
                recorded_before: Some("2026-06-26T00:10:00Z".to_string()),
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(beta_message.len(), 1);
        assert_eq!(beta_message[0].row_key, "message:1:agent-beta");
        assert_eq!(beta_message[0].sequence, Some(1));

        let queued = store
            .search_runtime(&RuntimeSearchFilter {
                query: "needle".to_string(),
                row_type: Some(RuntimeSearchRowType::QueueMessage),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].row_key, "queue-search-conformance");

        let stable_ties = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tie".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_after: None,
                recorded_before: None,
                limit: Some(1),
            })
            .unwrap();
        assert_eq!(stable_ties.len(), 1);
        assert_eq!(stable_ties[0].row_key, "message:2:agent-alpha");

        let empty_query = store
            .search_runtime(&RuntimeSearchFilter {
                query: "   ".to_string(),
                row_type: None,
                session_id: None,
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: None,
            })
            .unwrap_err();
        assert_eq!(empty_query.kind, CoreErrorKind::InvalidInput);
    }

    fn conversation_conformance(store: &dyn ConversationConformanceStore) {
        seed_conversation_base_fixture(store, "session-conversation", "slot-conversation");

        let lazy_slots = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("session-conversation")),
                include_alternates: false,
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: Some(0),
                }),
            })
            .unwrap();
        assert_eq!(lazy_slots.len(), 1);
        assert_eq!(lazy_slots[0].primary.message.body, "root body");
        assert!(lazy_slots[0].alternates.is_empty());

        let eager_slots = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("session-conversation")),
                include_alternates: true,
                page: None,
            })
            .unwrap();
        assert_eq!(eager_slots[0].alternates.len(), 2);
        assert_eq!(eager_slots[0].primary.message.blocks[0].kind, "text");

        let variants = store
            .query_message_variants(&MessageVariantQuery {
                slot_id: Some(MessageSlotId::new("slot-conversation")),
                include_deleted: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            variants
                .iter()
                .map(|variant| variant.variant_id.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                "variant-conversation-primary",
                "variant-conversation-a",
                "variant-conversation-b"
            ]
        );

        let selected_variant = store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conversation"),
                active_variant_id: Some(MessageVariantId::new("variant-conversation-a")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T02:02:00Z".to_string(),
            })
            .unwrap();
        assert!(selected_variant.conflict.is_none());
        assert_eq!(
            selected_variant.slot.active_variant_id,
            Some(MessageVariantId::new("variant-conversation-a"))
        );
        let variant_conflict = store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conversation"),
                active_variant_id: Some(MessageVariantId::new("variant-conversation-b")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T02:02:01Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            variant_conflict.conflict.unwrap().actual,
            Some(MessageVariantId::new("variant-conversation-a"))
        );

        let branches = store
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(SessionId::new("session-conversation")),
                parent_branch_id: None,
                page: None,
            })
            .unwrap();
        assert_eq!(
            branches
                .iter()
                .map(|branch| branch.branch_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["branch-conversation-root", "branch-conversation-child"]
        );
        let paged_branches = store
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(SessionId::new("session-conversation")),
                parent_branch_id: None,
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: Some(1),
                }),
            })
            .unwrap();
        assert_eq!(
            paged_branches[0].branch_id,
            ConversationBranchId::new("branch-conversation-child")
        );

        let default_state = store
            .get_conversation_branch_state(
                &SessionId::new("session-conversation"),
                &"2026-06-26T02:00:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(default_state.active_branch_id, None);
        assert_eq!(default_state.version, 0);

        let selected_branch = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conversation"),
                active_branch_id: Some(ConversationBranchId::new("branch-conversation-child")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T02:03:00Z".to_string(),
            })
            .unwrap();
        assert!(selected_branch.conflict.is_none());
        assert_eq!(
            selected_branch.state.active_branch_id,
            Some(ConversationBranchId::new("branch-conversation-child"))
        );
        let branch_conflict = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conversation"),
                active_branch_id: Some(ConversationBranchId::new("branch-conversation-root")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T02:03:01Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            branch_conflict.conflict.unwrap().actual,
            Some(ConversationBranchId::new("branch-conversation-child"))
        );

        let head_conflict = store
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conversation-child"),
                head_message_id: Some(MessageId::new("message-conversation-root")),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-26T02:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            head_conflict.conflict.unwrap().actual,
            Some(MessageId::new("message-conversation-child"))
        );
        let head_updated = store
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conversation-root"),
                head_message_id: Some(MessageId::new("message-conversation-a")),
                expected: BranchHeadExpectation::Message(MessageId::new(
                    "message-conversation-root",
                )),
                updated_at: "2026-06-26T02:04:01Z".to_string(),
            })
            .unwrap();
        assert!(head_updated.conflict.is_none());
        assert_eq!(
            head_updated.branch.head_message_id,
            Some(MessageId::new("message-conversation-a"))
        );

        let snapshot = store
            .save_conversation_snapshot(&ConversationSnapshotWrite {
                snapshot_id: ConversationSnapshotId::new("snapshot-conversation"),
                session_id: SessionId::new("session-conversation"),
                branch_id: Some(ConversationBranchId::new("branch-conversation-child")),
                message_id: Some(MessageId::new("message-conversation-root")),
                cursor: Some("session-conversation:42".to_string()),
                label: Some("Checkpoint".to_string()),
                summary: Some("Conversation checkpoint".to_string()),
                source: ConversationSnapshotSource::User,
                metadata_json: json!({"proof": "conversation"}),
                created_at: "2026-06-26T02:05:00Z".to_string(),
                updated_at: "2026-06-26T02:05:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            snapshot.branch_id,
            Some(ConversationBranchId::new("branch-conversation-child"))
        );
        let snapshots = store
            .query_conversation_snapshots(&ConversationSnapshotQuery {
                session_id: Some(SessionId::new("session-conversation")),
                branch_id: None,
                message_id: Some(MessageId::new("message-conversation-root")),
                page: None,
            })
            .unwrap();
        assert_eq!(snapshots.len(), 1);

        let branch_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Branch {
                    branch_id: ConversationBranchId::new("branch-conversation-child"),
                },
            })
            .unwrap();
        assert_eq!(
            branch_jump.message_id,
            Some(MessageId::new("message-conversation-child"))
        );
        let snapshot_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Snapshot {
                    snapshot_id: ConversationSnapshotId::new("snapshot-conversation"),
                },
            })
            .unwrap();
        assert_eq!(
            snapshot_jump.cursor,
            Some("session-conversation:42".to_string())
        );
        let message_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Message {
                    message_id: MessageId::new("message-conversation-root"),
                },
            })
            .unwrap();
        assert_eq!(
            message_jump.branch_id,
            Some(ConversationBranchId::new("branch-conversation-root"))
        );
        let cursor_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Cursor {
                    cursor: "manual-cursor".to_string(),
                },
            })
            .unwrap();
        assert_eq!(cursor_jump.cursor, Some("manual-cursor".to_string()));
    }

    fn seed_conversation_base_fixture(
        store: &dyn ConversationConformanceStore,
        session_id: &str,
        slot_id: &str,
    ) {
        let session = SessionId::new(session_id);
        let root_branch = ConversationBranchId::new("branch-conversation-root");
        let child_branch = ConversationBranchId::new("branch-conversation-child");
        let slot = MessageSlotId::new(slot_id);
        let primary_variant = MessageVariantId::new("variant-conversation-primary");
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: root_branch.clone(),
                session_id: session.clone(),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(MessageId::new("message-conversation-root")),
                label: Some("Root".to_string()),
                metadata_json: json!({"kind": "root"}),
                created_at: "2026-06-26T02:00:00Z".to_string(),
                updated_at: "2026-06-26T02:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot.clone(),
                session_id: session.clone(),
                primary_variant_id: primary_variant.clone(),
                active_variant_id: None,
                metadata_json: json!({"slot": "proof"}),
                created_at: "2026-06-26T02:00:00Z".to_string(),
                updated_at: "2026-06-26T02:00:00Z".to_string(),
            })
            .unwrap();
        let mut primary = conversation_variant_write(
            &slot,
            &primary_variant,
            MessageVariantSource::Primary,
            0,
            "message-conversation-root",
            "root body",
        );
        primary.message.session_id = session.clone();
        primary.message.branch_id = Some(root_branch.clone());
        store.save_message_variant(&primary).unwrap();
        let mut alternate_a = conversation_variant_write(
            &slot,
            &MessageVariantId::new("variant-conversation-a"),
            MessageVariantSource::Alternate,
            1,
            "message-conversation-a",
            "alternate a",
        );
        alternate_a.message.session_id = session.clone();
        alternate_a.message.branch_id = Some(root_branch.clone());
        alternate_a.message.parent_message_id = Some(MessageId::new("message-conversation-root"));
        store.save_message_variant(&alternate_a).unwrap();
        let mut alternate_b = conversation_variant_write(
            &slot,
            &MessageVariantId::new("variant-conversation-b"),
            MessageVariantSource::Alternate,
            2,
            "message-conversation-b",
            "alternate b",
        );
        alternate_b.message.session_id = session.clone();
        alternate_b.message.branch_id = Some(root_branch.clone());
        alternate_b.message.parent_message_id = Some(MessageId::new("message-conversation-root"));
        store.save_message_variant(&alternate_b).unwrap();
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: child_branch,
                session_id: session,
                parent_branch_id: Some(root_branch),
                parent_message_id: Some(MessageId::new("message-conversation-root")),
                origin_message_id: Some(MessageId::new("message-conversation-root")),
                head_message_id: Some(MessageId::new("message-conversation-child")),
                label: Some("Child".to_string()),
                metadata_json: json!({"kind": "child"}),
                created_at: "2026-06-26T02:01:00Z".to_string(),
                updated_at: "2026-06-26T02:01:00Z".to_string(),
            })
            .unwrap();
    }

    fn seed_conversation_conflict_fixture(store: &PostgresRuntimeCounterProofStore) {
        let session = SessionId::new("session-conflict");
        for branch_id in ["branch-conflict-a", "branch-conflict-b"] {
            store
                .save_conversation_branch(&ConversationBranchWrite {
                    branch_id: ConversationBranchId::new(branch_id),
                    session_id: session.clone(),
                    parent_branch_id: None,
                    parent_message_id: None,
                    origin_message_id: None,
                    head_message_id: None,
                    label: Some(branch_id.to_string()),
                    metadata_json: json!({"conflict": true}),
                    created_at: "2026-06-26T03:00:00Z".to_string(),
                    updated_at: "2026-06-26T03:00:00Z".to_string(),
                })
                .unwrap();
        }
        let slot = MessageSlotId::new("slot-conflict");
        let primary_variant = MessageVariantId::new("variant-conflict-primary");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot.clone(),
                session_id: session.clone(),
                primary_variant_id: primary_variant.clone(),
                active_variant_id: None,
                metadata_json: json!({"conflict": true}),
                created_at: "2026-06-26T03:00:00Z".to_string(),
                updated_at: "2026-06-26T03:00:00Z".to_string(),
            })
            .unwrap();
        let variants = [
            (
                primary_variant,
                MessageVariantSource::Primary,
                0,
                "message-conflict-primary",
                "primary conflict body",
            ),
            (
                MessageVariantId::new("variant-conflict-a"),
                MessageVariantSource::Alternate,
                1,
                "message-conflict-a",
                "alternate conflict a",
            ),
            (
                MessageVariantId::new("variant-conflict-b"),
                MessageVariantSource::Alternate,
                2,
                "message-conflict-b",
                "alternate conflict b",
            ),
        ];
        for (variant_id, source, ordinal, message_id, body) in variants {
            let mut variant =
                conversation_variant_write(&slot, &variant_id, source, ordinal, message_id, body);
            variant.message.session_id = session.clone();
            store.save_message_variant(&variant).unwrap();
        }
    }

    fn conversation_variant_write(
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        source: MessageVariantSource,
        ordinal: u32,
        message_id: &str,
        body: &str,
    ) -> MessageVariantWrite {
        MessageVariantWrite {
            variant_id: variant_id.clone(),
            slot_id: slot_id.clone(),
            source,
            ordinal,
            status: MessageVariantStatus::Active,
            message: DurableMessageWrite {
                message_id: MessageId::new(message_id),
                session_id: SessionId::new("session-conversation"),
                branch_id: None,
                parent_message_id: None,
                previous_message_id: None,
                author_id: "agent-proof".to_string(),
                author_role: "assistant".to_string(),
                status: DurableMessageStatus::Completed,
                body: body.to_string(),
                metadata_json: json!({"provider": "proof"}),
                created_at: "2026-06-26T02:00:00Z".to_string(),
                blocks: vec![MessageBlockWrite {
                    block_id: crate::MessageBlockId::new(format!("{message_id}:block-1")),
                    ordinal: 0,
                    kind: "text".to_string(),
                    content_json: json!({"text": body}),
                    render_policy_json: None,
                    metadata_json: json!({}),
                }],
            },
            metadata_json: json!({"variant": variant_id.0}),
            created_at: "2026-06-26T02:00:00Z".to_string(),
            updated_at: "2026-06-26T02:00:00Z".to_string(),
        }
    }

    fn provider_wire_state_conformance(store: &dyn ProviderWireStateConformanceStore) {
        let key = provider_wire_state_key("session-alpha", "openai-responses", "replay");
        let first = store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "resp_1"}),
                now: "2026-06-26T00:00:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: Some("wake-1"),
            }))
            .unwrap();
        assert!(first.is_current());
        assert_eq!(first.payload_encoding, "json");

        let second = store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v2",
                payload_json: json!({"response_id": "resp_2", "large": "x".repeat(128)}),
                now: "2026-06-26T00:01:00Z",
                expires_at: Some("2026-06-26T01:01:00Z"),
                last_wake_id: Some("wake-2"),
            }))
            .unwrap();
        assert!(second.row_id > first.row_id);

        let current = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: key.clone(),
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:02:00Z".to_string(),
            })
            .unwrap();
        let record = current.record.unwrap();
        assert_eq!(current.absence_reason, None);
        assert_eq!(record.payload_version, "responses:v2");
        assert_eq!(record.payload_json["response_id"], "resp_2");
        assert_eq!(record.last_wake_id.as_deref(), Some("wake-2"));

        let diagnostics = store.list_provider_wire_state_diagnostics(10).unwrap();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].key, key);
        assert_eq!(diagnostics[0].payload_version, "responses:v2");
        assert!(diagnostics[0].payload_bytes > 128);
        assert_eq!(
            diagnostics[1].invalidation_reason.as_deref(),
            Some("superseded")
        );

        let expired_key = provider_wire_state_key("session-expired", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: expired_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "expired"}),
                now: "2026-06-26T00:03:00Z",
                expires_at: Some("2026-06-26T00:04:00Z"),
                last_wake_id: Some("wake-expired"),
            }))
            .unwrap();
        let expired = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: expired_key.clone(),
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(expired.record, None);
        assert_eq!(
            expired.absence_reason,
            Some(ProviderStateAbsenceReason::Expired)
        );

        let profile_key = provider_wire_state_key("session-profile", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: profile_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "profile_stale"}),
                now: "2026-06-26T00:05:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let profile_stale = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: profile_key,
                profile_fingerprint: "profile:v2".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:06:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(profile_stale.record, None);
        assert_eq!(
            profile_stale.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        let provider_key =
            provider_wire_state_key("session-provider", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: provider_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "provider_stale"}),
                now: "2026-06-26T00:07:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let provider_stale = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: provider_key,
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v2".to_string(),
                now: "2026-06-26T00:08:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(provider_stale.record, None);
        assert_eq!(
            provider_stale.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        let clear_key = provider_wire_state_key("session-clear", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: clear_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "clear"}),
                now: "2026-06-26T00:09:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let cleared = store
            .clear_provider_wire_state(
                &clear_key,
                &"2026-06-26T00:10:00Z".to_string(),
                ProviderWireStateInvalidationReason::BrainRequestedClear,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            cleared.invalidation_reason,
            Some(ProviderWireStateInvalidationReason::BrainRequestedClear)
        );
        assert_eq!(
            store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: clear_key,
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-26T00:11:00Z".to_string(),
                })
                .unwrap()
                .absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );

        let strategy_key =
            provider_wire_state_key("session-strategy", "openai-responses", "replay-v1");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: strategy_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "strategy"}),
                now: "2026-06-26T00:12:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let changed_strategy =
            provider_wire_state_key("session-strategy", "openai-responses", "replay-v2");
        assert_eq!(
            store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: changed_strategy,
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-26T00:13:00Z".to_string(),
                })
                .unwrap()
                .absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );
        assert!(store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: strategy_key,
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:14:00Z".to_string(),
            })
            .unwrap()
            .record
            .is_none());

        let module_key = provider_wire_state_key("session-module", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: module_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "module"}),
                now: "2026-06-26T00:15:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let changed_module =
            provider_wire_state_key("session-module", "anthropic-messages", "replay");
        assert_eq!(
            store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: changed_module,
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-26T00:16:00Z".to_string(),
                })
                .unwrap()
                .absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );
        assert!(store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: module_key,
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:17:00Z".to_string(),
            })
            .unwrap()
            .record
            .is_none());

        let maintenance_key =
            provider_wire_state_key("session-maintenance", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: maintenance_key,
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "maintenance"}),
                now: "2026-06-26T00:18:00Z",
                expires_at: Some("2026-06-26T00:19:00Z"),
                last_wake_id: Some("wake-maintenance"),
            }))
            .unwrap();
        assert!(store
            .expire_provider_wire_states_at(&"2026-06-26T00:18:30Z".to_string())
            .unwrap()
            .is_empty());
        let expired = store
            .expire_provider_wire_states_at(&"2026-06-26T00:19:00Z".to_string())
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(
            expired[0].invalidation_reason,
            Some(ProviderWireStateInvalidationReason::Expired)
        );
    }

    fn provider_wire_state_key(
        session_id: &str,
        module_id: &str,
        strategy_id: &str,
    ) -> ProviderWireStateKey {
        ProviderWireStateKey {
            session_id: crate::SessionId::new(session_id),
            module_id: module_id.to_string(),
            strategy_id: strategy_id.to_string(),
        }
    }

    struct ProviderWireStateWriteFixture<'a> {
        key: ProviderWireStateKey,
        profile_fingerprint: &'a str,
        provider_fingerprint: &'a str,
        payload_version: &'a str,
        payload_json: serde_json::Value,
        now: &'a str,
        expires_at: Option<&'a str>,
        last_wake_id: Option<&'a str>,
    }

    fn provider_wire_state_write(
        input: ProviderWireStateWriteFixture<'_>,
    ) -> ProviderWireStateWrite {
        ProviderWireStateWrite {
            key: input.key,
            profile_fingerprint: input.profile_fingerprint.to_string(),
            provider_fingerprint: input.provider_fingerprint.to_string(),
            payload_version: input.payload_version.to_string(),
            payload_json: input.payload_json,
            now: input.now.to_string(),
            expires_at: input.expires_at.map(ToString::to_string),
            last_wake_id: input.last_wake_id.map(ToString::to_string),
        }
    }

    fn runtime_search_fixture() -> Vec<RuntimeSearchResult> {
        vec![
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Session,
                row_key: "session-alpha".to_string(),
                sequence: None,
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: Some(crate::AgentInstanceId::new("instance:session-alpha")),
                task_id: Some(crate::TaskId::new("task-search")),
                event_kind: Some(crate::CoreEventKind::SessionCreated),
                recorded_at: "2026-06-26T00:00:00Z".to_string(),
                title: "session session-alpha".to_string(),
                body: "agent agent-alpha profile runner kind full tools shell den".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:1:agent-beta".to_string(),
                sequence: Some(1),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:01:00Z".to_string(),
                title: "agent message".to_string(),
                body: "needle event search beta".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::QueueMessage,
                row_key: "queue-search-conformance".to_string(),
                sequence: Some(1),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: Some(crate::AgentInstanceId::new("instance:session-alpha")),
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:02:00Z".to_string(),
                title: "queued message pending".to_string(),
                body: "needle queue search".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:2:agent-alpha".to_string(),
                sequence: Some(2),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:03:00Z".to_string(),
                title: "agent message".to_string(),
                body: "tie search alpha".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:3:agent-alpha".to_string(),
                sequence: Some(3),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:04:00Z".to_string(),
                title: "agent message".to_string(),
                body: "tie search alpha".to_string(),
            },
        ]
    }

    fn postgres_test_database_url() -> Option<String> {
        for key in [
            "RUSTY_CREW_POSTGRES_PROOF_DATABASE_URL",
            "RUSTY_CREW_DATABASE_URL",
            "RUSTY_CREW_APP_DATABASE_URL",
        ] {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }

    fn unique_schema(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        format!("{prefix}_{}_{}", std::process::id(), nanos)
    }

    fn temp_sqlite_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rusty_crew_{label}_{nanos}.sqlite3"))
    }
}
