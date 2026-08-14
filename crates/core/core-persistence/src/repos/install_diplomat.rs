use super::super::*;

pub(crate) fn migrate_v72_add_install_diplomat_state(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE telegram_install_diplomat_bindings (
            binding_id TEXT PRIMARY KEY,
            installation_id TEXT NOT NULL,
            adapter_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            external_chat_id TEXT NOT NULL,
            external_thread_id TEXT,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE UNIQUE INDEX idx_telegram_diplomat_surface
            ON telegram_install_diplomat_bindings(
                adapter_id, external_chat_id, COALESCE(external_thread_id, '')
            );
         CREATE INDEX idx_telegram_diplomat_session
            ON telegram_install_diplomat_bindings(session_id, status);
         CREATE TABLE telegram_diplomat_interactions (
            interaction_id TEXT PRIMARY KEY,
            binding_id TEXT NOT NULL,
            terminal_reason TEXT,
            deadline_at TEXT NOT NULL,
            revision INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(binding_id) REFERENCES telegram_install_diplomat_bindings(binding_id)
         );
         CREATE INDEX idx_telegram_diplomat_interaction_binding
            ON telegram_diplomat_interactions(binding_id, updated_at DESC);
         CREATE INDEX idx_telegram_diplomat_interaction_deadline
            ON telegram_diplomat_interactions(deadline_at, terminal_reason);",
    )
    .map_err(|error| persistence_error("apply schema migration 72", error))
}

pub(crate) fn migrate_v76_add_telegram_operator_consults(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE telegram_operator_consults (
            consult_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            binding_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            FOREIGN KEY(binding_id) REFERENCES telegram_install_diplomat_bindings(binding_id)
         );
         CREATE INDEX idx_telegram_operator_consult_delivery
            ON telegram_operator_consults(status, updated_at);
         CREATE INDEX idx_telegram_operator_consult_session
            ON telegram_operator_consults(session_id, updated_at DESC);",
    )
    .map_err(|error| persistence_error("apply schema migration 76", error))
}

impl CoordinationStore {
    pub fn get_install_diplomat_binding(
        &self,
        binding_id: &str,
    ) -> CoreResult<Option<InstallDiplomatBindingRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT record_json FROM telegram_install_diplomat_bindings WHERE binding_id = ?1",
            params![binding_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| persistence_error("get install diplomat binding", error))?
        .map(|raw| decode_binding(&raw))
        .transpose()
    }

    pub fn list_install_diplomat_bindings(
        &self,
        query: &InstallDiplomatBindingQuery,
    ) -> CoreResult<Vec<InstallDiplomatBindingRecord>> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM telegram_install_diplomat_bindings
                 ORDER BY installation_id, binding_id",
            )
            .map_err(|error| persistence_error("prepare install diplomat binding list", error))?;
        let records = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query install diplomat bindings", error))?
            .map(|raw| {
                let raw = raw.map_err(|error| {
                    persistence_error("read install diplomat binding row", error)
                })?;
                decode_binding(&raw)
            })
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(records
            .into_iter()
            .filter(|record| binding_matches_query(record, query))
            .collect())
    }

    pub fn insert_install_diplomat_binding(
        &self,
        record: &InstallDiplomatBindingRecord,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        let conn = self.conn()?;
        let status = install_diplomat_binding_status_as_str(record.status);
        conn.execute(
            "INSERT INTO telegram_install_diplomat_bindings (
                binding_id, installation_id, adapter_id, session_id,
                external_chat_id, external_thread_id, status, revision,
                updated_at, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.binding_id,
                record.installation_id,
                record.adapter_id.0,
                record.session_id.0,
                record.external_chat_id,
                record.external_thread_id,
                status,
                record.revision as i64,
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| map_binding_write_error("insert install diplomat binding", error))?;
        Ok(record.clone())
    }

    pub fn update_install_diplomat_binding(
        &self,
        record: &InstallDiplomatBindingRecord,
        expected_revision: u64,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE telegram_install_diplomat_bindings
                    SET installation_id = ?1, adapter_id = ?2, session_id = ?3,
                        external_chat_id = ?4, external_thread_id = ?5, status = ?6,
                        revision = ?7, updated_at = ?8, record_json = ?9
                  WHERE binding_id = ?10 AND revision = ?11",
                params![
                    record.installation_id,
                    record.adapter_id.0,
                    record.session_id.0,
                    record.external_chat_id,
                    record.external_thread_id,
                    install_diplomat_binding_status_as_str(record.status),
                    record.revision as i64,
                    record.updated_at,
                    to_json_text(record)?,
                    record.binding_id,
                    expected_revision as i64,
                ],
            )
            .map_err(|error| map_binding_write_error("update install diplomat binding", error))?;
        if changed != 1 {
            return revision_conflict(
                "install diplomat binding",
                &record.binding_id,
                expected_revision,
            );
        }
        Ok(record.clone())
    }

    pub fn get_telegram_diplomat_interaction(
        &self,
        interaction_id: &str,
    ) -> CoreResult<Option<TelegramDiplomatInteractionRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT record_json FROM telegram_diplomat_interactions WHERE interaction_id = ?1",
            params![interaction_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| persistence_error("get Telegram diplomat interaction", error))?
        .map(|raw| decode_interaction(&raw))
        .transpose()
    }

    pub fn list_telegram_diplomat_interactions(
        &self,
        binding_id: &str,
    ) -> CoreResult<Vec<TelegramDiplomatInteractionRecord>> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM telegram_diplomat_interactions
                 WHERE binding_id = ?1 ORDER BY updated_at, interaction_id",
            )
            .map_err(|error| {
                persistence_error("prepare Telegram diplomat interaction list", error)
            })?;
        let records = statement
            .query_map(params![binding_id], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query Telegram diplomat interactions", error))?
            .map(|raw| {
                let raw = raw.map_err(|error| {
                    persistence_error("read Telegram diplomat interaction row", error)
                })?;
                decode_interaction(&raw)
            })
            .collect();
        records
    }

    pub fn put_telegram_diplomat_interaction(
        &self,
        record: &TelegramDiplomatInteractionRecord,
        expected_revision: Option<u64>,
    ) -> CoreResult<TelegramDiplomatInteractionRecord> {
        let conn = self.conn()?;
        let terminal_reason = record
            .terminal_reason
            .map(telegram_diplomat_terminal_reason_as_str);
        let changed = if let Some(expected_revision) = expected_revision {
            conn.execute(
                "UPDATE telegram_diplomat_interactions
                    SET binding_id = ?1, terminal_reason = ?2, deadline_at = ?3,
                        revision = ?4, updated_at = ?5, record_json = ?6
                  WHERE interaction_id = ?7 AND revision = ?8",
                params![
                    record.binding_id,
                    terminal_reason,
                    record.deadline_at,
                    record.revision as i64,
                    record.updated_at,
                    to_json_text(record)?,
                    record.interaction_id,
                    expected_revision as i64,
                ],
            )
            .map_err(|error| persistence_error("update Telegram diplomat interaction", error))?
        } else {
            conn.execute(
                "INSERT INTO telegram_diplomat_interactions (
                    interaction_id, binding_id, terminal_reason, deadline_at,
                    revision, updated_at, record_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.interaction_id,
                    record.binding_id,
                    terminal_reason,
                    record.deadline_at,
                    record.revision as i64,
                    record.updated_at,
                    to_json_text(record)?,
                ],
            )
            .map_err(|error| {
                if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                    CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        "telegram_diplomat_interaction_exists",
                    )
                } else {
                    persistence_error("insert Telegram diplomat interaction", error)
                }
            })?
        };
        if changed != 1 {
            return revision_conflict(
                "Telegram diplomat interaction",
                &record.interaction_id,
                expected_revision.unwrap_or(0),
            );
        }
        Ok(record.clone())
    }

    pub fn get_telegram_operator_consult(
        &self,
        consult_id: &str,
    ) -> CoreResult<Option<TelegramOperatorConsultRecord>> {
        self.read_telegram_operator_consult("consult_id", consult_id)
    }

    pub fn get_telegram_operator_consult_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> CoreResult<Option<TelegramOperatorConsultRecord>> {
        self.read_telegram_operator_consult("idempotency_key", idempotency_key)
    }

    fn read_telegram_operator_consult(
        &self,
        column: &str,
        value: &str,
    ) -> CoreResult<Option<TelegramOperatorConsultRecord>> {
        let sql = match column {
            "consult_id" => {
                "SELECT record_json FROM telegram_operator_consults WHERE consult_id = ?1"
            }
            "idempotency_key" => {
                "SELECT record_json FROM telegram_operator_consults WHERE idempotency_key = ?1"
            }
            _ => unreachable!("whitelisted Telegram consult lookup column"),
        };
        let conn = self.conn()?;
        conn.query_row(sql, params![value], |row| row.get::<_, String>(0))
            .optional()
            .map_err(|error| persistence_error("get Telegram operator consult", error))?
            .map(|raw| decode_operator_consult(&raw))
            .transpose()
    }

    pub fn list_telegram_operator_consults(
        &self,
        query: &TelegramOperatorConsultQuery,
    ) -> CoreResult<Vec<TelegramOperatorConsultRecord>> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM telegram_operator_consults
                 ORDER BY updated_at DESC, consult_id DESC",
            )
            .map_err(|error| persistence_error("prepare Telegram operator consult list", error))?;
        let mut records = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query Telegram operator consults", error))?
            .map(|raw| {
                let raw = raw.map_err(|error| {
                    persistence_error("read Telegram operator consult row", error)
                })?;
                decode_operator_consult(&raw)
            })
            .collect::<CoreResult<Vec<_>>>()?;
        records.retain(|record| operator_consult_matches_query(record, query));
        records.truncate(query.limit.unwrap_or(100).min(1_000) as usize);
        Ok(records)
    }

    pub fn insert_telegram_operator_consult(
        &self,
        record: &TelegramOperatorConsultRecord,
    ) -> CoreResult<TelegramOperatorConsultRecord> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO telegram_operator_consults (
                consult_id, idempotency_key, binding_id, session_id, status,
                revision, updated_at, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.consult_id,
                record.idempotency_key,
                record.binding_id,
                record.session_id.0,
                telegram_operator_consult_status_as_str(record.status),
                record.revision as i64,
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| {
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    "telegram_operator_consult_exists",
                )
            } else {
                persistence_error("insert Telegram operator consult", error)
            }
        })?;
        Ok(record.clone())
    }

    pub fn update_telegram_operator_consult(
        &self,
        record: &TelegramOperatorConsultRecord,
        expected_revision: u64,
    ) -> CoreResult<TelegramOperatorConsultRecord> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE telegram_operator_consults
                    SET status = ?1, revision = ?2, updated_at = ?3, record_json = ?4
                  WHERE consult_id = ?5 AND revision = ?6",
                params![
                    telegram_operator_consult_status_as_str(record.status),
                    record.revision as i64,
                    record.updated_at,
                    to_json_text(record)?,
                    record.consult_id,
                    expected_revision as i64,
                ],
            )
            .map_err(|error| persistence_error("update Telegram operator consult", error))?;
        if changed != 1 {
            return revision_conflict(
                "Telegram operator consult",
                &record.consult_id,
                expected_revision,
            );
        }
        Ok(record.clone())
    }
}

pub(crate) fn operator_consult_matches_query(
    record: &TelegramOperatorConsultRecord,
    query: &TelegramOperatorConsultQuery,
) -> bool {
    query
        .consult_id
        .as_ref()
        .is_none_or(|value| value == &record.consult_id)
        && query
            .binding_id
            .as_ref()
            .is_none_or(|value| value == &record.binding_id)
        && query
            .session_id
            .as_ref()
            .is_none_or(|value| value == &record.session_id)
        && query.status.is_none_or(|value| value == record.status)
}

pub(crate) fn binding_matches_query(
    record: &InstallDiplomatBindingRecord,
    query: &InstallDiplomatBindingQuery,
) -> bool {
    query
        .binding_id
        .as_ref()
        .is_none_or(|value| value == &record.binding_id)
        && query
            .installation_id
            .as_ref()
            .is_none_or(|value| value == &record.installation_id)
        && query
            .adapter_id
            .as_ref()
            .is_none_or(|value| value == &record.adapter_id)
        && query
            .session_id
            .as_ref()
            .is_none_or(|value| value == &record.session_id)
        && query
            .external_chat_id
            .as_ref()
            .is_none_or(|value| value == &record.external_chat_id)
        && query
            .external_thread_id
            .as_ref()
            .is_none_or(|value| record.external_thread_id.as_ref() == Some(value))
        && query.status.is_none_or(|value| value == record.status)
}

fn decode_binding(raw: &str) -> CoreResult<InstallDiplomatBindingRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode install diplomat binding: {error}"),
        )
    })
}

fn decode_interaction(raw: &str) -> CoreResult<TelegramDiplomatInteractionRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode Telegram diplomat interaction: {error}"),
        )
    })
}

fn decode_operator_consult(raw: &str) -> CoreResult<TelegramOperatorConsultRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode Telegram operator consult: {error}"),
        )
    })
}

fn map_binding_write_error(context: &str, error: rusqlite::Error) -> CoreError {
    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        CoreError::new(
            CoreErrorKind::AlreadyExists,
            "install_diplomat_surface_conflict",
        )
    } else {
        persistence_error(context, error)
    }
}

fn revision_conflict<T>(kind: &str, id: &str, expected_revision: u64) -> CoreResult<T> {
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!("{kind} {id} revision mismatch: expected {expected_revision}"),
    ))
}

pub(crate) fn install_diplomat_binding_status_as_str(
    status: InstallDiplomatBindingStatus,
) -> &'static str {
    match status {
        InstallDiplomatBindingStatus::Active => "active",
        InstallDiplomatBindingStatus::Paused => "paused",
        InstallDiplomatBindingStatus::NeedsRebind => "needs_rebind",
        InstallDiplomatBindingStatus::Removed => "removed",
    }
}

pub(crate) fn telegram_diplomat_terminal_reason_as_str(
    reason: TelegramDiplomatInteractionTerminalReason,
) -> &'static str {
    match reason {
        TelegramDiplomatInteractionTerminalReason::DepthExceeded => "depth_exceeded",
        TelegramDiplomatInteractionTerminalReason::MessageBudgetExceeded => {
            "message_budget_exceeded"
        }
        TelegramDiplomatInteractionTerminalReason::InteractionExpired => "interaction_expired",
        TelegramDiplomatInteractionTerminalReason::BotPairRateLimited => "bot_pair_rate_limited",
        TelegramDiplomatInteractionTerminalReason::BindingUnavailable => "binding_unavailable",
    }
}

pub(crate) fn telegram_operator_consult_status_as_str(
    status: TelegramOperatorConsultStatus,
) -> &'static str {
    match status {
        TelegramOperatorConsultStatus::Pending => "pending",
        TelegramOperatorConsultStatus::Sent => "sent",
        TelegramOperatorConsultStatus::Failed => "failed",
    }
}
