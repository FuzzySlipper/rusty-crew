use super::*;
use crate::repos::install_diplomat::{
    binding_matches_query, install_diplomat_binding_status_as_str, operator_consult_matches_query,
    telegram_diplomat_terminal_reason_as_str, telegram_operator_consult_status_as_str,
};

pub(super) fn apply_postgres_install_diplomat_state(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE {schema}.telegram_install_diplomat_bindings (
            binding_id TEXT PRIMARY KEY,
            installation_id TEXT NOT NULL,
            adapter_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            external_chat_id TEXT NOT NULL,
            external_thread_id TEXT,
            status TEXT NOT NULL,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE UNIQUE INDEX telegram_diplomat_surface_idx
            ON {schema}.telegram_install_diplomat_bindings(
                adapter_id, external_chat_id, COALESCE(external_thread_id, '')
            );
         CREATE INDEX telegram_diplomat_session_idx
            ON {schema}.telegram_install_diplomat_bindings(session_id, status);
         CREATE TABLE {schema}.telegram_diplomat_interactions (
            interaction_id TEXT PRIMARY KEY,
            binding_id TEXT NOT NULL REFERENCES {schema}.telegram_install_diplomat_bindings(binding_id),
            terminal_reason TEXT,
            deadline_at TEXT NOT NULL,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX telegram_diplomat_interaction_binding_idx
            ON {schema}.telegram_diplomat_interactions(binding_id, updated_at DESC);
         CREATE INDEX telegram_diplomat_interaction_deadline_idx
            ON {schema}.telegram_diplomat_interactions(deadline_at, terminal_reason);"
    ))
    .map_err(|error| postgres_error("create PostgreSQL install diplomat state", error))
}

pub(super) fn apply_postgres_telegram_operator_consults(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.telegram_operator_consults (
            consult_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            binding_id TEXT NOT NULL REFERENCES {schema}.telegram_install_diplomat_bindings(binding_id),
            session_id TEXT NOT NULL,
            status TEXT NOT NULL,
            revision BIGINT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS telegram_operator_consult_delivery_idx
            ON {schema}.telegram_operator_consults(status, updated_at);
         CREATE INDEX IF NOT EXISTS telegram_operator_consult_session_idx
            ON {schema}.telegram_operator_consults(session_id, updated_at DESC);"
    ))
    .map_err(|error| postgres_error("create PostgreSQL Telegram operator consult state", error))
}

impl PostgresBackendStore {
    pub fn get_install_diplomat_binding(
        &self,
        binding_id: &str,
    ) -> CoreResult<Option<InstallDiplomatBindingRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.telegram_install_diplomat_bindings
                     WHERE binding_id = $1"
                ),
                &[&binding_id],
            )
            .map_err(|error| postgres_error("get PostgreSQL install diplomat binding", error))?
            .map(|row| decode_binding(row.get(0)))
            .transpose()
    }

    pub fn list_install_diplomat_bindings(
        &self,
        query: &InstallDiplomatBindingQuery,
    ) -> CoreResult<Vec<InstallDiplomatBindingRecord>> {
        let schema = self.quoted_schema();
        let records = self
            .client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.telegram_install_diplomat_bindings
                     ORDER BY installation_id, binding_id"
                ),
                &[],
            )
            .map_err(|error| postgres_error("list PostgreSQL install diplomat bindings", error))?
            .into_iter()
            .map(|row| decode_binding(row.get(0)))
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
        let schema = self.quoted_schema();
        let revision = record.revision as i64;
        let status = install_diplomat_binding_status_as_str(record.status);
        let record_json = to_json_text(record)?;
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.telegram_install_diplomat_bindings (
                        binding_id, installation_id, adapter_id, session_id,
                        external_chat_id, external_thread_id, status, revision,
                        updated_at, record_json
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
                ),
                &[
                    &record.binding_id,
                    &record.installation_id,
                    &record.adapter_id.0,
                    &record.session_id.0,
                    &record.external_chat_id,
                    &record.external_thread_id,
                    &status,
                    &revision,
                    &record.updated_at,
                    &record_json,
                ],
            )
            .map_err(|error| {
                map_binding_write_error("insert PostgreSQL install diplomat binding", error)
            })?;
        Ok(record.clone())
    }

    pub fn update_install_diplomat_binding(
        &self,
        record: &InstallDiplomatBindingRecord,
        expected_revision: u64,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        let schema = self.quoted_schema();
        let revision = record.revision as i64;
        let expected_revision_i64 = expected_revision as i64;
        let status = install_diplomat_binding_status_as_str(record.status);
        let record_json = to_json_text(record)?;
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.telegram_install_diplomat_bindings
                        SET installation_id = $1, adapter_id = $2, session_id = $3,
                            external_chat_id = $4, external_thread_id = $5, status = $6,
                            revision = $7, updated_at = $8, record_json = $9
                      WHERE binding_id = $10 AND revision = $11"
                ),
                &[
                    &record.installation_id,
                    &record.adapter_id.0,
                    &record.session_id.0,
                    &record.external_chat_id,
                    &record.external_thread_id,
                    &status,
                    &revision,
                    &record.updated_at,
                    &record_json,
                    &record.binding_id,
                    &expected_revision_i64,
                ],
            )
            .map_err(|error| {
                map_binding_write_error("update PostgreSQL install diplomat binding", error)
            })?;
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
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.telegram_diplomat_interactions
                     WHERE interaction_id = $1"
                ),
                &[&interaction_id],
            )
            .map_err(|error| postgres_error("get PostgreSQL Telegram diplomat interaction", error))?
            .map(|row| decode_interaction(row.get(0)))
            .transpose()
    }

    pub fn list_telegram_diplomat_interactions(
        &self,
        binding_id: &str,
    ) -> CoreResult<Vec<TelegramDiplomatInteractionRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.telegram_diplomat_interactions
                     WHERE binding_id = $1 ORDER BY updated_at, interaction_id"
                ),
                &[&binding_id],
            )
            .map_err(|error| {
                postgres_error("list PostgreSQL Telegram diplomat interactions", error)
            })?
            .into_iter()
            .map(|row| decode_interaction(row.get(0)))
            .collect()
    }

    pub fn put_telegram_diplomat_interaction(
        &self,
        record: &TelegramDiplomatInteractionRecord,
        expected_revision: Option<u64>,
    ) -> CoreResult<TelegramDiplomatInteractionRecord> {
        let schema = self.quoted_schema();
        let revision = record.revision as i64;
        let terminal_reason = record
            .terminal_reason
            .map(telegram_diplomat_terminal_reason_as_str);
        let record_json = to_json_text(record)?;
        let changed = if let Some(expected_revision) = expected_revision {
            let expected_revision = expected_revision as i64;
            self.client()?
                .execute(
                    &format!(
                        "UPDATE {schema}.telegram_diplomat_interactions
                        SET binding_id = $1, terminal_reason = $2, deadline_at = $3,
                            revision = $4, updated_at = $5, record_json = $6
                      WHERE interaction_id = $7 AND revision = $8"
                    ),
                    &[
                        &record.binding_id,
                        &terminal_reason,
                        &record.deadline_at,
                        &revision,
                        &record.updated_at,
                        &record_json,
                        &record.interaction_id,
                        &expected_revision,
                    ],
                )
                .map_err(|error| {
                    postgres_error("update PostgreSQL Telegram diplomat interaction", error)
                })?
        } else {
            self.client()?
                .execute(
                    &format!(
                        "INSERT INTO {schema}.telegram_diplomat_interactions (
                        interaction_id, binding_id, terminal_reason, deadline_at,
                        revision, updated_at, record_json
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7)"
                    ),
                    &[
                        &record.interaction_id,
                        &record.binding_id,
                        &terminal_reason,
                        &record.deadline_at,
                        &revision,
                        &record.updated_at,
                        &record_json,
                    ],
                )
                .map_err(|error| {
                    if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                        CoreError::new(
                            CoreErrorKind::AlreadyExists,
                            "telegram_diplomat_interaction_exists",
                        )
                    } else {
                        postgres_error("insert PostgreSQL Telegram diplomat interaction", error)
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
        let column = match column {
            "consult_id" => "consult_id",
            "idempotency_key" => "idempotency_key",
            _ => unreachable!("whitelisted Telegram consult lookup column"),
        };
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.telegram_operator_consults WHERE {column} = $1"
                ),
                &[&value],
            )
            .map_err(|error| postgres_error("get PostgreSQL Telegram operator consult", error))?
            .map(|row| decode_operator_consult(row.get(0)))
            .transpose()
    }

    pub fn list_telegram_operator_consults(
        &self,
        query: &TelegramOperatorConsultQuery,
    ) -> CoreResult<Vec<TelegramOperatorConsultRecord>> {
        let schema = self.quoted_schema();
        let mut records = self
            .client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.telegram_operator_consults
                     ORDER BY updated_at DESC, consult_id DESC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("list PostgreSQL Telegram operator consults", error))?
            .into_iter()
            .map(|row| decode_operator_consult(row.get(0)))
            .collect::<CoreResult<Vec<_>>>()?;
        records.retain(|record| operator_consult_matches_query(record, query));
        records.truncate(query.limit.unwrap_or(100).min(1_000) as usize);
        Ok(records)
    }

    pub fn insert_telegram_operator_consult(
        &self,
        record: &TelegramOperatorConsultRecord,
    ) -> CoreResult<TelegramOperatorConsultRecord> {
        let schema = self.quoted_schema();
        let revision = record.revision as i64;
        let status = telegram_operator_consult_status_as_str(record.status);
        let record_json = to_json_text(record)?;
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.telegram_operator_consults (
                        consult_id, idempotency_key, binding_id, session_id, status,
                        revision, updated_at, record_json
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
                ),
                &[
                    &record.consult_id,
                    &record.idempotency_key,
                    &record.binding_id,
                    &record.session_id.0,
                    &status,
                    &revision,
                    &record.updated_at,
                    &record_json,
                ],
            )
            .map_err(|error| {
                if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                    CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        "telegram_operator_consult_exists",
                    )
                } else {
                    postgres_error("insert PostgreSQL Telegram operator consult", error)
                }
            })?;
        Ok(record.clone())
    }

    pub fn update_telegram_operator_consult(
        &self,
        record: &TelegramOperatorConsultRecord,
        expected_revision: u64,
    ) -> CoreResult<TelegramOperatorConsultRecord> {
        let schema = self.quoted_schema();
        let revision = record.revision as i64;
        let expected_revision = expected_revision as i64;
        let status = telegram_operator_consult_status_as_str(record.status);
        let record_json = to_json_text(record)?;
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.telegram_operator_consults
                        SET status = $1, revision = $2, updated_at = $3, record_json = $4
                      WHERE consult_id = $5 AND revision = $6"
                ),
                &[
                    &status,
                    &revision,
                    &record.updated_at,
                    &record_json,
                    &record.consult_id,
                    &expected_revision,
                ],
            )
            .map_err(|error| {
                postgres_error("update PostgreSQL Telegram operator consult", error)
            })?;
        if changed != 1 {
            return revision_conflict(
                "Telegram operator consult",
                &record.consult_id,
                expected_revision as u64,
            );
        }
        Ok(record.clone())
    }
}

fn decode_binding(raw: &str) -> CoreResult<InstallDiplomatBindingRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode PostgreSQL install diplomat binding: {error}"),
        )
    })
}

fn decode_interaction(raw: &str) -> CoreResult<TelegramDiplomatInteractionRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode PostgreSQL Telegram diplomat interaction: {error}"),
        )
    })
}

fn decode_operator_consult(raw: &str) -> CoreResult<TelegramOperatorConsultRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode PostgreSQL Telegram operator consult: {error}"),
        )
    })
}

fn map_binding_write_error(context: &str, error: postgres::Error) -> CoreError {
    if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
        CoreError::new(
            CoreErrorKind::AlreadyExists,
            "install_diplomat_surface_conflict",
        )
    } else {
        postgres_error(context, error)
    }
}

fn revision_conflict<T>(kind: &str, id: &str, expected_revision: u64) -> CoreResult<T> {
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!("{kind} {id} revision mismatch: expected {expected_revision}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AgentId, InstallDiplomatBindingStatus, InstallDiplomatParticipationMode, ProfileId,
        TelegramDiplomatSender, TelegramDiplomatSenderKind, TelegramOperatorConsultCategory,
        TelegramOperatorConsultStatus, TELEGRAM_DIPLOMAT_INTERACTION_VERSION,
        TELEGRAM_INSTALL_DIPLOMAT_BINDING_VERSION, TELEGRAM_OPERATOR_CONSULT_VERSION,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_install_diplomat_state_is_revisioned_and_restart_readable() {
        let database_url = std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_TEST_DATABASE_URL"))
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_install_diplomat_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let store = PostgresBackendStore::connect(&database_url, &schema).unwrap();
        let mut binding = InstallDiplomatBindingRecord {
            schema_version: TELEGRAM_INSTALL_DIPLOMAT_BINDING_VERSION.to_string(),
            binding_id: "diplomat-binding".to_string(),
            revision: 1,
            installation_id: "install-alpha".to_string(),
            installation_label: "Install Alpha".to_string(),
            adapter_id: AdapterId::new("telegram-alpha"),
            bot_user_id: "9001".to_string(),
            bot_username: "install_alpha_bot".to_string(),
            agent_id: AgentId::new("install-diplomat"),
            instance_id: None,
            session_id: SessionId::new("diplomat-session"),
            external_chat_id: "-100500".to_string(),
            external_thread_id: Some("42".to_string()),
            participation_mode: InstallDiplomatParticipationMode::MentionOrReply,
            status: InstallDiplomatBindingStatus::Active,
            degraded_reason: None,
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        };
        store.insert_install_diplomat_binding(&binding).unwrap();
        binding.revision = 2;
        binding.installation_label = "Install Alpha Renamed".to_string();
        binding.updated_at = "2026-06-19T00:01:00Z".to_string();
        store.update_install_diplomat_binding(&binding, 1).unwrap();

        let interaction = TelegramDiplomatInteractionRecord {
            schema_version: TELEGRAM_DIPLOMAT_INTERACTION_VERSION.to_string(),
            interaction_id: "interaction-1".to_string(),
            binding_id: binding.binding_id.clone(),
            revision: 1,
            root_external_message_id: "message-1".to_string(),
            last_external_message_id: "message-1".to_string(),
            last_sender: TelegramDiplomatSender {
                kind: TelegramDiplomatSenderKind::Human,
                external_user_id: "7001".to_string(),
                username: Some("operator".to_string()),
                display_label: Some("Operator".to_string()),
            },
            bot_pair_key: None,
            bot_depth: 0,
            bot_message_count: 0,
            bot_message_timestamps: Vec::new(),
            crew_correlation_id: "telegram:diplomat-binding:interaction-1".to_string(),
            deadline_at: "2026-06-19T00:05:00Z".to_string(),
            terminal_reason: None,
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        };
        store
            .put_telegram_diplomat_interaction(&interaction, None)
            .unwrap();

        let mut consult = TelegramOperatorConsultRecord {
            schema_version: TELEGRAM_OPERATOR_CONSULT_VERSION.to_string(),
            consult_id: "telegram-consult-1".to_string(),
            idempotency_key: "diplomat-session:wake-1:call-1".to_string(),
            revision: 1,
            binding_id: binding.binding_id.clone(),
            adapter_id: binding.adapter_id.clone(),
            agent_id: binding.agent_id.clone(),
            session_id: binding.session_id.clone(),
            profile_id: ProfileId::new("diplomat-profile"),
            wake_id: "wake-1".to_string(),
            tool_call_id: "call-1".to_string(),
            originating_wake_kind: Some("operator".to_string()),
            category: Some(TelegramOperatorConsultCategory::NetworkTrouble),
            body: "Should I inspect the router?".to_string(),
            external_chat_id: binding.external_chat_id.clone(),
            external_thread_id: binding.external_thread_id.clone(),
            status: TelegramOperatorConsultStatus::Pending,
            delivery_attempts: 0,
            external_message_ids: Vec::new(),
            reason_code: None,
            last_error: None,
            requested_at: "2026-06-19T00:02:00Z".to_string(),
            updated_at: "2026-06-19T00:02:00Z".to_string(),
            sent_at: None,
        };
        store.insert_telegram_operator_consult(&consult).unwrap();
        consult.revision = 2;
        consult.status = TelegramOperatorConsultStatus::Sent;
        consult.delivery_attempts = 2;
        consult.external_message_ids = vec!["telegram-message-123".to_string()];
        consult.reason_code = Some("telegram_operator_consult_sent".to_string());
        consult.updated_at = "2026-06-19T00:02:01Z".to_string();
        consult.sent_at = Some(consult.updated_at.clone());
        store.update_telegram_operator_consult(&consult, 1).unwrap();

        drop(store);
        let reopened = PostgresBackendStore::connect(&database_url, &schema).unwrap();
        assert_eq!(
            reopened
                .get_install_diplomat_binding("diplomat-binding")
                .unwrap()
                .unwrap(),
            binding
        );
        assert_eq!(
            reopened
                .list_telegram_diplomat_interactions("diplomat-binding")
                .unwrap(),
            vec![interaction]
        );
        assert_eq!(
            reopened
                .get_telegram_operator_consult_by_idempotency_key("diplomat-session:wake-1:call-1")
                .unwrap()
                .unwrap(),
            consult
        );
        reopened.drop_schema_for_test().unwrap();
    }
}
