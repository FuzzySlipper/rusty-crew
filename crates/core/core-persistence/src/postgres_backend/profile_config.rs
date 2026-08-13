//! PostgreSQL profile, provider, and service config repositories.

use super::*;

impl PostgresBackendStore {
    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        crate::validate_profile_registry_write(write)?;
        let schema = self.quoted_schema();
        let record = ProfileRegistryRecord {
            profile_id: write.profile_id.clone(),
            lifecycle_status: write.lifecycle_status,
            display_name: write.display_name.clone(),
            summary: write.summary.clone(),
            default_session_kind: write.default_session_kind.clone(),
            agent_id: write.agent_id.clone(),
            owner_id: write.owner_id.clone(),
            prompt_soul_markdown: write.prompt_soul_markdown.clone(),
            prompt_memory_markdown: write.prompt_memory_markdown.clone(),
            active_runtime_settings_json: write.active_runtime_settings_json.clone(),
            source_asset_refs: write.source_asset_refs.clone(),
            derived_runtime_refs: write.derived_runtime_refs.clone(),
            import_export: write.import_export.clone(),
            revision: 1,
            created_at: write.now.clone(),
            updated_at: write.now.clone(),
        };
        let record_json = to_json_text(&record)?;
        let lifecycle_status =
            profile_registry_lifecycle_status_as_str(record.lifecycle_status).to_string();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL profile registry record create", error)
        })?;
        lock_profile_model_configuration_boundary(&mut tx, &schema)?;
        let duplicate = tx
            .query_opt(
                &format!("SELECT 1 FROM {schema}.profile_registry WHERE profile_id = $1"),
                &[&write.profile_id.0],
            )
            .map_err(|error| postgres_error("check PostgreSQL profile registry duplicate", error))?
            .is_some();
        if duplicate {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile registry record {} already exists",
                    write.profile_id
                ),
            ));
        }
        validate_profile_model_configuration(&mut tx, &schema, write)?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.profile_registry (
                        profile_id,
                        lifecycle_status,
                        record_json,
                        created_at,
                        updated_at
                     ) VALUES ($1, $2, $3, $4, $5)"
            ),
            &[
                &record.profile_id.0,
                &lifecycle_status,
                &record_json,
                &record.created_at,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("create PostgreSQL profile registry record", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL profile registry record create", error)
        })?;
        Ok(record)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        crate::validate_profile_registry_write(&update.write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL profile registry record update", error)
        })?;
        lock_profile_model_configuration_boundary(&mut tx, &schema)?;
        let existing = tx
            .query_opt(
                &format!(
                    "SELECT record_json
                     FROM {schema}.profile_registry
                     WHERE profile_id = $1"
                ),
                &[&update.write.profile_id.0],
            )
            .map_err(|error| postgres_error("load PostgreSQL profile registry record", error))?
            .map(|row| {
                let record_json: String = row.get(0);
                parse_postgres_json::<ProfileRegistryRecord>(
                    &record_json,
                    "profile registry record_json",
                )
            })
            .transpose()?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "profile registry record {} not found",
                        update.write.profile_id
                    ),
                )
            })?;
        if existing.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile registry record {} revision mismatch: expected {}, found {}",
                    update.write.profile_id, update.expected_revision, existing.revision
                ),
            ));
        }
        validate_profile_model_configuration(&mut tx, &schema, &update.write)?;
        let record = ProfileRegistryRecord {
            profile_id: update.write.profile_id.clone(),
            lifecycle_status: update.write.lifecycle_status,
            display_name: update.write.display_name.clone(),
            summary: update.write.summary.clone(),
            default_session_kind: update.write.default_session_kind.clone(),
            agent_id: update.write.agent_id.clone(),
            owner_id: update.write.owner_id.clone(),
            prompt_soul_markdown: update.write.prompt_soul_markdown.clone(),
            prompt_memory_markdown: update.write.prompt_memory_markdown.clone(),
            active_runtime_settings_json: update.write.active_runtime_settings_json.clone(),
            source_asset_refs: update.write.source_asset_refs.clone(),
            derived_runtime_refs: update.write.derived_runtime_refs.clone(),
            import_export: update.write.import_export.clone(),
            revision: existing.revision + 1,
            created_at: existing.created_at,
            updated_at: update.write.now.clone(),
        };
        let record_json = to_json_text(&record)?;
        let lifecycle_status =
            profile_registry_lifecycle_status_as_str(record.lifecycle_status).to_string();
        tx.execute(
            &format!(
                "UPDATE {schema}.profile_registry
                 SET lifecycle_status = $2,
                     record_json = $3,
                     updated_at = $4
                 WHERE profile_id = $1"
            ),
            &[
                &record.profile_id.0,
                &lifecycle_status,
                &record_json,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL profile registry record", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL profile registry record update", error)
        })?;
        Ok(record)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        crate::validate_profile_registry_id(profile_id)?;
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "SELECT record_json
                     FROM {schema}.profile_registry
                     WHERE profile_id = $1"
                ),
                &[&profile_id.0],
            )
            .map_err(|error| postgres_error("get PostgreSQL profile registry record", error))?;
        row.map(|row| {
            let record_json: String = row.get(0);
            parse_postgres_json(&record_json, "profile registry record_json")
        })
        .transpose()
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        let schema = self.quoted_schema();
        let lifecycle_status = query
            .lifecycle_status
            .map(profile_registry_lifecycle_status_as_str)
            .map(str::to_string);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT record_json
                     FROM {schema}.profile_registry
                     WHERE ($1::TEXT IS NULL OR lifecycle_status = $1)
                     ORDER BY profile_id ASC
                     LIMIT $2 OFFSET $3"
                ),
                &[&lifecycle_status, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL profile registry records", error))?;
        rows.iter()
            .map(|row| {
                let record_json: String = row.get(0);
                parse_postgres_json(&record_json, "profile registry record_json")
            })
            .collect()
    }

    pub fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        crate::validate_profile_registry_id(profile_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL profile purge", error))?;
        tx.batch_execute(
            "
            DROP TABLE IF EXISTS __rusty_profile_purge_sessions;
            DROP TABLE IF EXISTS __rusty_profile_purge_agents;
            DROP TABLE IF EXISTS __rusty_profile_purge_events;
            DROP TABLE IF EXISTS __rusty_profile_purge_messages;
            DROP TABLE IF EXISTS __rusty_profile_purge_attachments;
            DROP TABLE IF EXISTS __rusty_profile_purge_lore_records;
            DROP TABLE IF EXISTS __rusty_profile_purge_lore_layers;
            DROP TABLE IF EXISTS __rusty_profile_purge_worker_members;
            DROP TABLE IF EXISTS __rusty_profile_purge_worker_items;

            CREATE TEMP TABLE __rusty_profile_purge_sessions(session_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_agents(agent_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_events(sequence BIGINT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_messages(message_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_attachments(attachment_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_lore_records(record_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_lore_layers(layer_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_worker_members(member_id TEXT PRIMARY KEY) ON COMMIT DROP;
            CREATE TEMP TABLE __rusty_profile_purge_worker_items(work_item_id TEXT PRIMARY KEY) ON COMMIT DROP;
            ",
        )
        .map_err(|error| postgres_error("prepare PostgreSQL profile purge temp tables", error))?;

        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_sessions(session_id)
                 SELECT session_id FROM {schema}.sessions WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile sessions", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_sessions(session_id)
                 SELECT session_id FROM {schema}.session_configs WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile session configs", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_sessions(session_id)
                 SELECT session_id FROM {schema}.session_identities WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile session identities", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_sessions(session_id)
                 SELECT session_id FROM {schema}.channel_bindings
                 WHERE profile_id = $1 AND session_id IS NOT NULL
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL channel sessions", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_sessions(session_id)
                 SELECT session_id FROM {schema}.mcp_bindings
                 WHERE profile_id = $1 AND session_id IS NOT NULL
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL mcp sessions", error))?;

        for sql in [
            format!(
                "INSERT INTO __rusty_profile_purge_agents(agent_id)
                 SELECT agent_id FROM {schema}.agent_identities WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            format!(
                "INSERT INTO __rusty_profile_purge_agents(agent_id)
                 SELECT agent_id FROM {schema}.agent_instances WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            format!(
                "INSERT INTO __rusty_profile_purge_agents(agent_id)
                 SELECT agent_id FROM {schema}.session_identities WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            format!(
                "INSERT INTO __rusty_profile_purge_agents(agent_id)
                 SELECT agent_id FROM {schema}.sessions WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
        ] {
            tx.execute(&sql, &[&profile_id.0])
                .map_err(|error| postgres_error("collect PostgreSQL profile agents", error))?;
        }

        let session_ids =
            postgres_purge_temp_strings(&mut tx, "__rusty_profile_purge_sessions", "session_id")?
                .into_iter()
                .map(SessionId)
                .collect::<Vec<_>>();
        let agent_ids =
            postgres_purge_temp_strings(&mut tx, "__rusty_profile_purge_agents", "agent_id")?
                .into_iter()
                .map(AgentId)
                .collect::<Vec<_>>();

        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_events(sequence)
                 SELECT sequence FROM {schema}.event_index
                 WHERE projection = 'session'
                   AND value IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile session events", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_events(sequence)
                 SELECT sequence FROM {schema}.event_index
                 WHERE projection = 'agent'
                   AND value IN (SELECT agent_id FROM __rusty_profile_purge_agents)
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile agent events", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_messages(message_id)
                 SELECT message_id FROM {schema}.messages
                 WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile messages", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_attachments(attachment_id)
                 SELECT attachment_id FROM {schema}.attachments
                 WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .map_err(|error| postgres_error("collect PostgreSQL profile attachments", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_lore_layers(layer_id)
                 SELECT layer_id FROM {schema}.module_roleplay_lore_layers WHERE profile_id = $1
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL lore layers", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_lore_records(record_id)
                 SELECT record_id FROM {schema}.module_roleplay_lore_records
                 WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .map_err(|error| postgres_error("collect PostgreSQL lore records by session", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_lore_records(record_id)
                 SELECT record_id FROM {schema}.module_roleplay_lore_layer_entries
                 WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .map_err(|error| postgres_error("collect PostgreSQL lore records by layer", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_worker_members(member_id)
                 SELECT member_id FROM {schema}.worker_pool_members
                 WHERE profile_id = $1
                    OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL worker members", error))?;
        tx.execute(
            &format!(
                "INSERT INTO __rusty_profile_purge_worker_items(work_item_id)
                 SELECT work_item_id FROM {schema}.worker_pool_work_items
                 WHERE requested_profile_id = $1
                    OR claimed_by_member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
                 ON CONFLICT DO NOTHING"
            ),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("collect PostgreSQL worker items", error))?;

        let mut counts = Vec::new();
        postgres_purge_delete(&mut tx, &mut counts, "external_interactions", &format!("DELETE FROM {schema}.external_interactions WHERE binding_id IN (SELECT binding_id FROM {schema}.external_agent_bindings WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions) OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)) OR request_id IN (SELECT request_id FROM {schema}.external_turns WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "external_control_receipts", &format!("DELETE FROM {schema}.external_control_receipts WHERE binding_id IN (SELECT binding_id FROM {schema}.external_agent_bindings WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions) OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents))"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "external_runtime_event_checkpoints", &format!("DELETE FROM {schema}.external_runtime_event_checkpoints WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "external_runtime_events", &format!("DELETE FROM {schema}.external_runtime_events WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "external_turns", &format!("DELETE FROM {schema}.external_turns WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions) OR binding_id IN (SELECT binding_id FROM {schema}.external_agent_bindings WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions) OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents))"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "external_agent_session_creations", &format!("DELETE FROM {schema}.external_agent_session_creations WHERE profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"), &[&profile_id.0])?;
        postgres_purge_delete(&mut tx, &mut counts, "agent_correlated_rounds", &format!("DELETE FROM {schema}.agent_correlated_rounds WHERE sender_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions) OR recipient_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "agent_message_delivery_receipts", &format!("DELETE FROM {schema}.agent_message_delivery_receipts WHERE from_agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents) OR to_agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)"), &[])?;
        postgres_purge_delete(&mut tx, &mut counts, "external_agent_bindings", &format!("DELETE FROM {schema}.external_agent_bindings WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions) OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)"), &[])?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_lore_provenance_events",
            &format!(
                "DELETE FROM {schema}.module_roleplay_lore_provenance_events
                 WHERE record_id IN (SELECT record_id FROM __rusty_profile_purge_lore_records)"
            ),
            &[],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_lore_layer_entries",
            &format!(
                "DELETE FROM {schema}.module_roleplay_lore_layer_entries
                 WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)
                    OR record_id IN (SELECT record_id FROM __rusty_profile_purge_lore_records)"
            ),
            &[],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_chat_layers",
            &format!(
                "DELETE FROM {schema}.module_roleplay_chat_layers
                 WHERE chat_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                    OR layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)"
            ),
            &[],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_lore_layer_config",
            &format!(
                "DELETE FROM {schema}.module_roleplay_lore_layer_config
                 WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)"
            ),
            &[],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_imports",
            &format!("DELETE FROM {schema}.module_roleplay_imports WHERE profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"),
            &[&profile_id.0],
        )?;
        for (table, predicate) in [
            (
                "module_curator_approvals",
                "candidate_id IN (SELECT candidate_id FROM {schema}.module_curator_candidates WHERE profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))",
            ),
            (
                "module_curator_snapshot_refs",
                "candidate_id IN (SELECT candidate_id FROM {schema}.module_curator_candidates WHERE profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))",
            ),
            (
                "module_curator_mutations",
                "candidate_id IN (SELECT candidate_id FROM {schema}.module_curator_candidates WHERE profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))",
            ),
            (
                "module_curator_audit_receipts",
                "profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
            ),
            (
                "module_curator_candidates",
                "profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
            ),
        ] {
            postgres_purge_delete(
                &mut tx,
                &mut counts,
                table,
                &format!("DELETE FROM {schema}.{table} WHERE {}", predicate.replace("{schema}", &schema)),
                &[&profile_id.0],
            )?;
        }
        postgres_purge_delete(&mut tx, &mut counts, "module_roleplay_session_metadata", &format!("DELETE FROM {schema}.module_roleplay_session_metadata WHERE profile_id = $1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"), &[&profile_id.0])?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_player_personas",
            &format!("DELETE FROM {schema}.module_roleplay_player_personas WHERE profile_id = $1"),
            &[&profile_id.0],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_characters",
            &format!("DELETE FROM {schema}.module_roleplay_characters WHERE profile_id = $1"),
            &[&profile_id.0],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_lore_records",
            &format!(
                "DELETE FROM {schema}.module_roleplay_lore_records
                 WHERE record_id IN (SELECT record_id FROM __rusty_profile_purge_lore_records)"
            ),
            &[],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_lore_recall_traces",
            &format!(
                "DELETE FROM {schema}.module_roleplay_lore_recall_traces
                 WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
            ),
            &[],
        )?;
        postgres_purge_delete(
            &mut tx,
            &mut counts,
            "module_roleplay_lore_layers",
            &format!(
                "DELETE FROM {schema}.module_roleplay_lore_layers
                 WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)"
            ),
            &[],
        )?;

        for (table, sql) in [
            (
                "message_blocks",
                format!(
                    "DELETE FROM {schema}.message_blocks
                     WHERE message_id IN (SELECT message_id FROM __rusty_profile_purge_messages)"
                ),
            ),
            (
                "message_variants",
                format!(
                    "DELETE FROM {schema}.message_variants
                     WHERE message_id IN (SELECT message_id FROM __rusty_profile_purge_messages)
                        OR slot_id IN (
                            SELECT slot_id FROM {schema}.message_slots
                            WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        )"
                ),
            ),
            (
                "chat_message_ingest_receipts",
                format!(
                    "DELETE FROM {schema}.chat_message_ingest_receipts
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "message_slots",
                format!(
                    "DELETE FROM {schema}.message_slots
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "messages",
                format!(
                    "DELETE FROM {schema}.messages
                     WHERE message_id IN (SELECT message_id FROM __rusty_profile_purge_messages)"
                ),
            ),
            (
                "conversation_snapshots",
                format!(
                    "DELETE FROM {schema}.conversation_snapshots
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "conversation_branch_state",
                format!(
                    "DELETE FROM {schema}.conversation_branch_state
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "conversation_branches",
                format!(
                    "DELETE FROM {schema}.conversation_branches
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "attachment_links",
                format!(
                    "DELETE FROM {schema}.attachment_links
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        OR attachment_id IN (SELECT attachment_id FROM __rusty_profile_purge_attachments)"
                ),
            ),
            (
                "attachments",
                format!(
                    "DELETE FROM {schema}.attachments
                     WHERE attachment_id IN (SELECT attachment_id FROM __rusty_profile_purge_attachments)"
                ),
            ),
            (
                "data_bank_scopes",
                format!(
                    "DELETE FROM {schema}.data_bank_scopes
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "provider_wire_states",
                format!(
                    "DELETE FROM {schema}.provider_wire_states
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "context_compaction_artifacts",
                format!(
                    "DELETE FROM {schema}.context_compaction_artifacts
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "session_memory_records",
                format!(
                    "DELETE FROM {schema}.session_memory_records
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "completion_packets",
                format!(
                    "DELETE FROM {schema}.completion_packets
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "tool_call_history",
                format!(
                    "DELETE FROM {schema}.tool_call_history
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
            ),
            (
                "worker_pool_events",
                format!(
                    "DELETE FROM {schema}.worker_pool_events
                     WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
                        OR work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)
                        OR lease_id IN (
                            SELECT lease_id FROM {schema}.worker_pool_leases
                            WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
                               OR work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)
                        )"
                ),
            ),
            (
                "worker_pool_leases",
                format!(
                    "DELETE FROM {schema}.worker_pool_leases
                     WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
                        OR work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)"
                ),
            ),
            (
                "worker_pool_work_items",
                format!(
                    "DELETE FROM {schema}.worker_pool_work_items
                     WHERE work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)"
                ),
            ),
            (
                "worker_pool_members",
                format!(
                    "DELETE FROM {schema}.worker_pool_members
                     WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)"
                ),
            ),
            (
                "runtime_search_entries",
                format!(
                    "DELETE FROM {schema}.runtime_search_entries
                     WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)"
                ),
            ),
        ] {
            postgres_purge_delete(&mut tx, &mut counts, table, &sql, &[])?;
        }

        for (table, sql, params) in [
            (
                "session_activity_digests",
                format!(
                    "DELETE FROM {schema}.session_activity_digests
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "profile_memories",
                format!("DELETE FROM {schema}.profile_memories WHERE profile_id = $1"),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "queued_messages",
                format!(
                    "DELETE FROM {schema}.queued_messages
                     WHERE owner_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        OR owner_agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)
                        OR from_agent IN (SELECT agent_id FROM __rusty_profile_purge_agents)
                        OR to_agent IN (SELECT agent_id FROM __rusty_profile_purge_agents)"
                ),
                Vec::new(),
            ),
            (
                "scheduled_job_runs",
                format!(
                    "DELETE FROM {schema}.scheduled_job_runs
                     WHERE target_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        OR job_id IN (
                            SELECT job_id FROM {schema}.scheduled_jobs
                            WHERE target_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        )"
                ),
                Vec::new(),
            ),
            (
                "scheduled_jobs",
                format!(
                    "DELETE FROM {schema}.scheduled_jobs
                     WHERE target_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                Vec::new(),
            ),
            (
                "worker_runs",
                format!(
                    "DELETE FROM {schema}.worker_runs
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                        OR delegated_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "runtime_counters",
                format!(
                    "DELETE FROM {schema}.runtime_counters
                     WHERE (scope_type = 'profile' AND scope_id = $1)
                        OR (scope_type = 'session' AND scope_id IN (
                            SELECT session_id FROM __rusty_profile_purge_sessions
                        ))
                        OR (scope_type = 'agent' AND scope_id IN (
                            SELECT agent_id FROM __rusty_profile_purge_agents
                        ))"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "event_index",
                format!(
                    "DELETE FROM {schema}.event_index
                     WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)
                        OR (projection = 'session' AND value IN (
                            SELECT session_id FROM __rusty_profile_purge_sessions
                        ))
                        OR (projection = 'agent' AND value IN (
                            SELECT agent_id FROM __rusty_profile_purge_agents
                        ))"
                ),
                Vec::new(),
            ),
            (
                "event_history",
                format!(
                    "DELETE FROM {schema}.event_history
                     WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)"
                ),
                Vec::new(),
            ),
            (
                "channel_bindings",
                format!(
                    "DELETE FROM {schema}.channel_bindings
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "mcp_bindings",
                format!(
                    "DELETE FROM {schema}.mcp_bindings
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "session_configs",
                format!(
                    "DELETE FROM {schema}.session_configs
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "session_identities",
                format!(
                    "DELETE FROM {schema}.session_identities
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "agent_instances",
                format!(
                    "DELETE FROM {schema}.agent_instances
                     WHERE profile_id = $1
                        OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "agent_identities",
                format!(
                    "DELETE FROM {schema}.agent_identities
                     WHERE profile_id = $1
                        OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
            (
                "sessions",
                format!(
                    "DELETE FROM {schema}.sessions
                     WHERE profile_id = $1
                        OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)"
                ),
                vec![&profile_id.0 as &(dyn ToSql + Sync)],
            ),
        ] {
            postgres_purge_delete(&mut tx, &mut counts, table, &sql, &params)?;
        }

        let profile_registry_deleted = postgres_purge_delete(
            &mut tx,
            &mut counts,
            "profile_registry",
            &format!("DELETE FROM {schema}.profile_registry WHERE profile_id = $1"),
            &[&profile_id.0],
        )? > 0;

        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL profile purge", error))?;
        let rows_deleted = counts.iter().map(|count| count.rows_deleted).sum();
        Ok(ProfilePurgeReport {
            profile_id: profile_id.clone(),
            profile_registry_deleted,
            session_ids,
            agent_ids,
            table_counts: counts,
            rows_deleted,
        })
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        crate::validate_model_provider_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL model provider upsert", error))?;
        let existing = get_model_provider_in_tx(&mut tx, &schema, &write.alias)?;
        if let (Some(expected), Some(record)) = (write.expected_revision, existing.as_ref()) {
            if record.revision != expected {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "model provider {} revision mismatch: expected {}, found {}",
                        write.alias, expected, record.revision
                    ),
                ));
            }
        }
        upsert_model_provider_in_tx(&mut tx, &schema, write, existing.as_ref())?;
        let record =
            get_model_provider_in_tx(&mut tx, &schema, &write.alias)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "upserted PostgreSQL model provider was not readable",
                )
            })?;
        super::model_registry::sync_legacy_provider_to_normalized_in_tx(&mut tx, &schema, &record)?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL model provider upsert", error))?;
        Ok(record)
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        crate::validate_model_provider_alias(alias)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_model_provider_in_client(&mut *client, &schema, alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        crate::validate_model_provider_alias(alias)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_model_provider_secret_in_client(&mut *client, &schema, alias)
    }

    pub fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        crate::validate_service_credential_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL service credential upsert", error))?;
        let existing = get_service_credential_in_client(&mut tx, &schema, &write.credential_id)?;
        validate_postgres_expected_revision(
            "service credential",
            &write.credential_id,
            write.expected_revision,
            existing.as_ref().map(|record| record.revision),
        )?;
        upsert_service_credential_in_tx(&mut tx, &schema, write, existing.as_ref())?;
        let record = get_service_credential_in_client(&mut tx, &schema, &write.credential_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "upserted PostgreSQL service credential was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL service credential upsert", error)
        })?;
        Ok(record)
    }

    pub fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        crate::validate_service_credential_id(credential_id)?;
        let schema = self.quoted_schema();
        get_service_credential_in_client(&mut *self.client()?, &schema, credential_id)
    }

    pub fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        crate::validate_service_credential_id(credential_id)?;
        let schema = self.quoted_schema();
        get_service_credential_secret_in_client(&mut *self.client()?, &schema, credential_id)
    }

    pub fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        crate::validate_service_credential_id(&delete.credential_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL service credential delete", error))?;
        let existing = get_service_credential_in_client(&mut tx, &schema, &delete.credential_id)?
            .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {} not found", delete.credential_id),
            )
        })?;
        validate_postgres_expected_revision(
            "service credential",
            &delete.credential_id,
            delete.expected_revision,
            Some(existing.revision),
        )?;
        reject_linked_postgres_service_credential_mutation(&existing, "delete")?;
        reject_postgres_model_endpoint_credential_mutation(
            &mut tx,
            &schema,
            &delete.credential_id,
            "delete",
        )?;
        let changed = tx
            .execute(
                &format!(
                    "DELETE FROM {schema}.service_credentials
                      WHERE credential_id = $1 AND revision = $2"
                ),
                &[&delete.credential_id, &(existing.revision as i64)],
            )
            .map_err(|error| postgres_error("delete PostgreSQL service credential", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "service credential {} changed concurrently",
                    delete.credential_id
                ),
            ));
        }
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL service credential delete", error)
        })?;
        Ok(existing)
    }

    pub fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        let schema = self.quoted_schema();
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let rows = self
            .client()?
            .query(
                &service_credential_select_sql(
                    &schema,
                    "WHERE ($1::TEXT IS NULL OR sc.provider_kind = $1)
                     GROUP BY sc.credential_id
                     ORDER BY sc.updated_at DESC, sc.credential_id ASC
                     LIMIT $2 OFFSET $3",
                ),
                &[&query.provider_kind, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL service credentials", error))?;
        rows.iter().map(row_to_service_credential).collect()
    }

    pub fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        crate::validate_model_provider_alias(&link.provider_alias)?;
        crate::validate_service_credential_id(&link.credential_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL model provider credential link", error)
        })?;
        link_model_provider_credential_in_tx(&mut tx, &schema, link)?;
        let provider = get_model_provider_in_client(&mut tx, &schema, &link.provider_alias)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "linked PostgreSQL provider was not readable",
                )
            })?;
        let credential = get_service_credential_in_client(&mut tx, &schema, &link.credential_id)?
            .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "linked PostgreSQL credential was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL model provider credential link", error)
        })?;
        Ok(ModelProviderCredentialLinkResult {
            provider,
            credential,
        })
    }

    pub fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        crate::validate_model_provider_alias(&unlink.provider_alias)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL model provider credential unlink", error)
        })?;
        let mut provider = get_model_provider_in_client(&mut tx, &schema, &unlink.provider_alias)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("model provider {} not found", unlink.provider_alias),
                )
            })?;
        validate_postgres_expected_revision(
            "model provider",
            &unlink.provider_alias,
            unlink.expected_provider_revision,
            Some(provider.revision),
        )?;
        provider.credential_id = None;
        provider.credential = empty_model_provider_credential();
        let previous_revision = provider.revision;
        provider.revision += 1;
        provider.updated_at = unlink.now.clone();
        let changed = tx
            .execute(
                &format!(
                    "UPDATE {schema}.model_providers
                    SET credential_id = NULL, provider_json = $2,
                        revision = $3, updated_at = $4
                  WHERE alias = $1 AND revision = $5"
                ),
                &[
                    &unlink.provider_alias,
                    &to_json_text(&provider)?,
                    &(provider.revision as i64),
                    &unlink.now,
                    &(previous_revision as i64),
                ],
            )
            .map_err(|error| {
                postgres_error("unlink PostgreSQL model provider credential", error)
            })?;
        verify_postgres_model_provider_cas(
            &mut tx,
            &schema,
            &unlink.provider_alias,
            previous_revision,
            changed,
        )?;
        let result = get_model_provider_in_client(&mut tx, &schema, &unlink.provider_alias)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "unlinked PostgreSQL provider was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL model provider credential unlink", error)
        })?;
        Ok(result)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        let schema = self.quoted_schema();
        let status = query
            .status
            .map(model_provider_status_as_str)
            .map(str::to_string);
        let alias_prefix = query
            .alias_prefix
            .as_deref()
            .map(|value| format!("{value}%"));
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT mp.provider_json, mp.credential_id, sc.secret_ciphertext,
                            sc.secret_updated_at, sc.credential_kind, sc.revision
                     FROM {schema}.model_providers mp
                     LEFT JOIN {schema}.service_credentials sc
                       ON sc.credential_id = mp.credential_id
                     WHERE ($1::TEXT IS NULL OR mp.status = $1)
                       AND ($2::TEXT IS NULL OR mp.alias LIKE $2)
                     ORDER BY mp.updated_at DESC, mp.alias ASC
                     LIMIT $3 OFFSET $4"
                ),
                &[&status, &alias_prefix, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL model providers", error))?;
        rows.iter().map(row_to_model_provider).collect()
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
}

pub(super) fn lock_profile_model_configuration_boundary(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "LOCK TABLE {schema}.profile_registry, {schema}.model_configurations IN SHARE ROW EXCLUSIVE MODE"
    ))
    .map_err(|error| postgres_error("lock profile model configuration boundary", error))
}

fn validate_profile_model_configuration(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &ProfileRegistryWrite,
) -> CoreResult<()> {
    let Some(model_config_id) =
        crate::normalized_profile_model_config_id(&write.active_runtime_settings_json)
    else {
        return Ok(());
    };
    let exists = tx
        .query_opt(
            &format!("SELECT 1 FROM {schema}.model_configurations WHERE model_config_id = $1"),
            &[&model_config_id],
        )
        .map_err(|error| postgres_error("validate PostgreSQL profile model configuration", error))?
        .is_some();
    if !exists {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "profile registry record {} references missing model configuration {}",
                write.profile_id, model_config_id
            ),
        ));
    }
    Ok(())
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

fn profile_registry_lifecycle_status_as_str(
    status: ProfileRegistryLifecycleStatus,
) -> &'static str {
    match status {
        ProfileRegistryLifecycleStatus::Active => "active",
        ProfileRegistryLifecycleStatus::Paused => "paused",
        ProfileRegistryLifecycleStatus::Decommissioned => "decommissioned",
        ProfileRegistryLifecycleStatus::Archived => "archived",
    }
}

fn get_model_provider_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    alias: &str,
) -> CoreResult<Option<ModelProviderRecord>> {
    get_model_provider_in_client(tx, schema, alias)
}

fn get_model_provider_in_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    alias: &str,
) -> CoreResult<Option<ModelProviderRecord>> {
    let row = client
        .query_opt(
            &format!(
                "SELECT mp.provider_json, mp.credential_id, sc.secret_ciphertext,
                        sc.secret_updated_at, sc.credential_kind, sc.revision
                 FROM {schema}.model_providers mp
                 LEFT JOIN {schema}.service_credentials sc
                   ON sc.credential_id = mp.credential_id
                 WHERE mp.alias = $1"
            ),
            &[&alias],
        )
        .map_err(|error| postgres_error("get PostgreSQL model provider", error))?;
    row.as_ref().map(row_to_model_provider).transpose()
}

fn get_model_provider_secret_in_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    alias: &str,
) -> CoreResult<Option<String>> {
    client
        .query_opt(
            &format!(
                "SELECT sc.secret_ciphertext
                 FROM {schema}.model_providers mp
                 LEFT JOIN {schema}.service_credentials sc
                   ON sc.credential_id = mp.credential_id
                 WHERE mp.alias = $1"
            ),
            &[&alias],
        )
        .map_err(|error| postgres_error("get PostgreSQL model provider secret", error))
        .map(|row| row.and_then(|row| row.get(0)))
}

fn upsert_model_provider_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &ModelProviderWrite,
    existing: Option<&ModelProviderRecord>,
) -> CoreResult<()> {
    let incoming_secret = write
        .secret
        .as_deref()
        .map(ModelProviderSecretEnvelope::normalize_storage_text)
        .transpose()?;
    let revision = existing.map_or(1, |record| record.revision + 1);
    let created_at = existing
        .map(|record| record.created_at.clone())
        .unwrap_or_else(|| write.now.clone());
    let mut credential_id = if write.clear_secret {
        None
    } else {
        existing.and_then(|record| record.credential_id.clone())
    };
    if let Some(secret) = incoming_secret {
        let envelope = ModelProviderSecretEnvelope::from_storage_text(&secret)?;
        let id = credential_id
            .clone()
            .unwrap_or_else(|| format!("provider:{}", write.alias));
        let current = get_service_credential_in_client(tx, schema, &id)?;
        validate_postgres_expected_revision(
            "service credential",
            &id,
            write.expected_credential_revision,
            current.as_ref().map(|record| record.revision),
        )?;
        let credential_write = ServiceCredentialWrite {
            credential_id: id.clone(),
            display_name: current.as_ref().map_or_else(
                || {
                    write
                        .display_name
                        .clone()
                        .unwrap_or_else(|| write.alias.clone())
                },
                |record| record.display_name.clone(),
            ),
            provider_kind: current.as_ref().map_or_else(
                || write.provider_kind.clone(),
                |record| record.provider_kind.clone(),
            ),
            credential_kind: envelope.kind(),
            secret: Some(secret),
            clear_secret: false,
            expected_revision: current.as_ref().map(|record| record.revision),
            now: write.now.clone(),
        };
        upsert_service_credential_in_tx(tx, schema, &credential_write, current.as_ref())?;
        credential_id = Some(id);
    } else if let Some(id) = credential_id.as_deref() {
        let credential = get_service_credential_in_client(tx, schema, id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {id} not found"),
            )
        })?;
        validate_postgres_provider_credential_compatibility(write.protocol, &credential)?;
    }
    let credential = credential_id
        .as_deref()
        .map(|id| get_service_credential_in_client(tx, schema, id))
        .transpose()?
        .flatten();
    let record = ModelProviderRecord {
        alias: write.alias.clone(),
        status: write.status,
        protocol: write.protocol,
        provider_kind: write.provider_kind.clone(),
        display_name: write.display_name.clone(),
        description: write.description.clone(),
        base_url: write.base_url.clone(),
        model_id: write.model_id.clone(),
        context_window_tokens: write.context_window_tokens,
        max_output_tokens: write.max_output_tokens,
        temperature_milli: write.temperature_milli,
        reasoning_effort: write.reasoning_effort.clone(),
        reasoning_format: write.reasoning_format.clone(),
        responses_dialect: write.responses_dialect,
        chat_completions_dialect: write.chat_completions_dialect,
        thinking_mode: write.thinking_mode,
        reasoning_history: write.reasoning_history,
        reasoning_budget_tokens: write.reasoning_budget_tokens,
        prompt_caching: write.prompt_caching,
        credential_id: credential_id.clone(),
        credential: credential
            .as_ref()
            .map_or_else(empty_model_provider_credential, |record| {
                record.credential.clone()
            }),
        metadata_json: write.metadata_json.clone(),
        revision,
        created_at,
        updated_at: write.now.clone(),
    };
    let provider_json = to_json_text(&record)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.model_providers (
                alias,
                status,
                protocol,
                provider_json,
                secret_ciphertext,
                secret_updated_at,
                revision,
                created_at,
                updated_at,
                credential_id
             ) VALUES ($1, $2, $3, $4, NULL, NULL, $5, $6, $7, $8)
             ON CONFLICT(alias) DO UPDATE SET
                status = excluded.status,
                protocol = excluded.protocol,
                provider_json = excluded.provider_json,
                secret_ciphertext = NULL,
                secret_updated_at = NULL,
                revision = excluded.revision,
                updated_at = excluded.updated_at,
                credential_id = excluded.credential_id"
        ),
        &[
            &record.alias,
            &model_provider_status_as_str(record.status).to_string(),
            &model_provider_protocol_as_str(record.protocol).to_string(),
            &provider_json,
            &(record.revision as i64),
            &record.created_at,
            &record.updated_at,
            &credential_id,
        ],
    )
    .map_err(|error| postgres_error("upsert PostgreSQL model provider", error))?;
    Ok(())
}

fn row_to_model_provider(row: &Row) -> CoreResult<ModelProviderRecord> {
    let provider_json: String = row.get(0);
    let credential_id: Option<String> = row.get(1);
    let secret_ciphertext: Option<String> = row.get(2);
    let secret_updated_at: Option<String> = row.get(3);
    let credential_kind: Option<String> = row.get(4);
    let credential_revision: Option<i64> = row.get(5);
    let mut record: ModelProviderRecord =
        parse_postgres_json(&provider_json, "model provider provider_json")?;
    record.credential_id = credential_id.clone();
    record.credential = ModelProviderCredential {
        has_secret: secret_ciphertext.is_some(),
        secret_ref: secret_ciphertext
            .as_ref()
            .and(credential_id.as_ref())
            .map(|id| format!("db://service_credentials/{id}/secret")),
        updated_at: secret_updated_at,
        kind: credential_kind
            .as_deref()
            .map(model_provider_credential_kind_from_storage)
            .transpose()?,
        revision: credential_revision.map(|value| value as u64),
    };
    Ok(record)
}

fn empty_model_provider_credential() -> ModelProviderCredential {
    ModelProviderCredential {
        has_secret: false,
        secret_ref: None,
        updated_at: None,
        kind: None,
        revision: None,
    }
}

fn service_credential_select_sql(schema: &str, suffix: &str) -> String {
    format!(
        "SELECT sc.credential_id, sc.display_name, sc.provider_kind,
                sc.credential_kind, sc.secret_ciphertext, sc.secret_updated_at,
                sc.revision, sc.created_at, sc.updated_at,
                COALESCE(string_agg(mp.alias, chr(31) ORDER BY mp.alias), '')
           FROM {schema}.service_credentials sc
           LEFT JOIN {schema}.model_providers mp ON mp.credential_id = sc.credential_id
           {suffix}"
    )
}

fn get_service_credential_in_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    credential_id: &str,
) -> CoreResult<Option<ServiceCredentialRecord>> {
    client
        .query_opt(
            &service_credential_select_sql(
                schema,
                "WHERE sc.credential_id = $1 GROUP BY sc.credential_id",
            ),
            &[&credential_id],
        )
        .map_err(|error| postgres_error("get PostgreSQL service credential", error))?
        .as_ref()
        .map(row_to_service_credential)
        .transpose()
}

fn get_service_credential_secret_in_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    credential_id: &str,
) -> CoreResult<Option<String>> {
    client
        .query_opt(
            &format!(
                "SELECT secret_ciphertext FROM {schema}.service_credentials WHERE credential_id = $1"
            ),
            &[&credential_id],
        )
        .map_err(|error| postgres_error("get PostgreSQL service credential secret", error))
        .map(|row| row.and_then(|row| row.get(0)))
}

fn row_to_service_credential(row: &Row) -> CoreResult<ServiceCredentialRecord> {
    let credential_id: String = row.get(0);
    let credential_kind = model_provider_credential_kind_from_storage(&row.get::<_, String>(3))?;
    let secret: Option<String> = row.get(4);
    let revision = row.get::<_, i64>(6) as u64;
    let aliases: String = row.get(9);
    Ok(ServiceCredentialRecord {
        credential_id: credential_id.clone(),
        display_name: row.get(1),
        provider_kind: row.get(2),
        credential_kind,
        credential: ModelProviderCredential {
            has_secret: secret.is_some(),
            secret_ref: secret
                .as_ref()
                .map(|_| format!("db://service_credentials/{credential_id}/secret")),
            updated_at: row.get(5),
            kind: Some(credential_kind),
            revision: Some(revision),
        },
        linked_provider_aliases: aliases
            .split('\u{1f}')
            .filter(|alias| !alias.is_empty())
            .map(str::to_string)
            .collect(),
        revision,
        created_at: row.get(7),
        updated_at: row.get(8),
    })
}

fn upsert_service_credential_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &ServiceCredentialWrite,
    existing: Option<&ServiceCredentialRecord>,
) -> CoreResult<()> {
    let incoming_secret = write
        .secret
        .as_deref()
        .map(ModelProviderSecretEnvelope::normalize_storage_text)
        .transpose()?;
    if let Some(secret) = incoming_secret.as_deref() {
        let actual = ModelProviderSecretEnvelope::from_storage_text(secret)?.kind();
        if actual != write.credential_kind {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "service credential kind does not match secret envelope",
            ));
        }
    }
    if write.credential_kind == ModelProviderCredentialKind::OpenAiOauth
        && write.provider_kind != "openai"
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "OpenAI OAuth credentials require provider_kind openai",
        ));
    }
    if let Some(existing) = existing {
        if write.clear_secret {
            reject_linked_postgres_service_credential_mutation(existing, "clear")?;
            reject_postgres_model_endpoint_credential_mutation(
                tx,
                schema,
                &write.credential_id,
                "clear",
            )?;
        }
        for alias in &existing.linked_provider_aliases {
            let provider = get_model_provider_in_client(tx, schema, alias)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!("linked PostgreSQL model provider {alias} was not readable"),
                )
            })?;
            validate_postgres_provider_credential_fields(provider.protocol, write.credential_kind)?;
        }
    }
    let preserved_secret = if write.clear_secret || incoming_secret.is_some() {
        None
    } else {
        get_service_credential_secret_in_client(tx, schema, &write.credential_id)?
    };
    let secret = if write.clear_secret {
        None
    } else {
        incoming_secret.or(preserved_secret)
    };
    if let Some(secret) = secret.as_deref() {
        let actual = ModelProviderSecretEnvelope::from_storage_text(secret)?.kind();
        if actual != write.credential_kind {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "service credential kind does not match stored secret envelope",
            ));
        }
    }
    let secret_updated_at = if write.clear_secret {
        None
    } else if write.secret.is_some() {
        Some(write.now.clone())
    } else {
        existing.and_then(|record| record.credential.updated_at.clone())
    };
    let revision = existing.map_or(1, |record| record.revision + 1);
    let created_at = existing
        .map(|record| record.created_at.clone())
        .unwrap_or_else(|| write.now.clone());
    let changed = if let Some(existing) = existing {
        tx.execute(
            &format!(
                "UPDATE {schema}.service_credentials
                    SET display_name = $2,
                        provider_kind = $3,
                        credential_kind = $4,
                        secret_ciphertext = $5,
                        secret_updated_at = $6,
                        revision = $7,
                        updated_at = $8
                  WHERE credential_id = $1 AND revision = $9"
            ),
            &[
                &write.credential_id,
                &write.display_name,
                &write.provider_kind,
                &model_provider_credential_kind_as_storage(write.credential_kind),
                &secret,
                &secret_updated_at,
                &(revision as i64),
                &write.now,
                &(existing.revision as i64),
            ],
        )
    } else {
        tx.execute(
            &format!(
                "INSERT INTO {schema}.service_credentials (
                    credential_id, display_name, provider_kind, credential_kind,
                    secret_ciphertext, secret_updated_at, revision, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(credential_id) DO NOTHING"
            ),
            &[
                &write.credential_id,
                &write.display_name,
                &write.provider_kind,
                &model_provider_credential_kind_as_storage(write.credential_kind),
                &secret,
                &secret_updated_at,
                &(revision as i64),
                &created_at,
                &write.now,
            ],
        )
    }
    .map_err(|error| postgres_error("upsert PostgreSQL service credential", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "service credential {} changed concurrently",
                write.credential_id
            ),
        ));
    }
    Ok(())
}

fn reject_linked_postgres_service_credential_mutation(
    credential: &ServiceCredentialRecord,
    action: &str,
) -> CoreResult<()> {
    if credential.linked_provider_aliases.is_empty() {
        return Ok(());
    }
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!(
            "cannot {action} service credential {} while linked to model providers: {}",
            credential.credential_id,
            credential.linked_provider_aliases.join(", ")
        ),
    ))
}

fn reject_postgres_model_endpoint_credential_mutation<C: GenericClient>(
    client: &mut C,
    schema: &str,
    credential_id: &str,
    action: &str,
) -> CoreResult<()> {
    let endpoint_ids = client
        .query(
            &format!(
                "SELECT endpoint_id FROM {schema}.model_endpoints
                  WHERE credential_id = $1 ORDER BY endpoint_id"
            ),
            &[&credential_id],
        )
        .map_err(|error| {
            postgres_error(
                "list PostgreSQL model endpoints linked to service credential",
                error,
            )
        })?
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();
    if endpoint_ids.is_empty() {
        return Ok(());
    }
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!(
            "cannot {action} service credential {credential_id} while linked to model endpoints: {}",
            endpoint_ids.join(", ")
        ),
    ))
}

fn link_model_provider_credential_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    link: &ModelProviderCredentialLink,
) -> CoreResult<()> {
    let mut provider =
        get_model_provider_in_client(tx, schema, &link.provider_alias)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("model provider {} not found", link.provider_alias),
            )
        })?;
    let credential = get_service_credential_in_client(tx, schema, &link.credential_id)?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {} not found", link.credential_id),
            )
        })?;
    validate_postgres_expected_revision(
        "model provider",
        &link.provider_alias,
        link.expected_provider_revision,
        Some(provider.revision),
    )?;
    validate_postgres_expected_revision(
        "service credential",
        &link.credential_id,
        link.expected_credential_revision,
        Some(credential.revision),
    )?;
    validate_postgres_provider_credential_compatibility(provider.protocol, &credential)?;
    provider.credential_id = Some(link.credential_id.clone());
    provider.credential = credential.credential.clone();
    let previous_revision = provider.revision;
    provider.revision += 1;
    provider.updated_at = link.now.clone();
    let changed = tx
        .execute(
            &format!(
                "UPDATE {schema}.model_providers
                SET credential_id = $2, provider_json = $3,
                    revision = $4, updated_at = $5
              WHERE alias = $1 AND revision = $6"
            ),
            &[
                &link.provider_alias,
                &link.credential_id,
                &to_json_text(&provider)?,
                &(provider.revision as i64),
                &link.now,
                &(previous_revision as i64),
            ],
        )
        .map_err(|error| postgres_error("link PostgreSQL model provider credential", error))?;
    verify_postgres_model_provider_cas(
        tx,
        schema,
        &link.provider_alias,
        previous_revision,
        changed,
    )?;
    Ok(())
}

fn verify_postgres_model_provider_cas(
    tx: &mut Transaction<'_>,
    schema: &str,
    provider_alias: &str,
    expected_revision: u64,
    changed: u64,
) -> CoreResult<()> {
    if changed == 1 {
        return Ok(());
    }
    let found = get_model_provider_in_client(tx, schema, provider_alias)?.map(|row| row.revision);
    validate_postgres_expected_revision(
        "model provider",
        provider_alias,
        Some(expected_revision),
        found,
    )?;
    Err(CoreError::new(
        CoreErrorKind::PersistenceFailure,
        format!(
            "model provider {provider_alias} compare-and-swap updated {changed} rows at revision {expected_revision}"
        ),
    ))
}

fn validate_postgres_expected_revision(
    label: &str,
    id: &str,
    expected: Option<u64>,
    found: Option<u64>,
) -> CoreResult<()> {
    if expected.is_none() || expected == found {
        return Ok(());
    }
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!(
            "{label} {id} revision mismatch: expected {}, found {}",
            expected.unwrap_or_default(),
            found.unwrap_or_default()
        ),
    ))
}

fn validate_postgres_provider_credential_compatibility(
    protocol: ModelProviderProtocol,
    credential: &ServiceCredentialRecord,
) -> CoreResult<()> {
    validate_postgres_provider_credential_fields(protocol, credential.credential_kind)
}

fn validate_postgres_provider_credential_fields(
    protocol: ModelProviderProtocol,
    credential_kind: ModelProviderCredentialKind,
) -> CoreResult<()> {
    if credential_kind == ModelProviderCredentialKind::OpenAiOauth
        && protocol != ModelProviderProtocol::Responses
    {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "OpenAI OAuth credentials require the responses protocol",
        ));
    }
    Ok(())
}

fn model_provider_credential_kind_as_storage(kind: ModelProviderCredentialKind) -> &'static str {
    match kind {
        ModelProviderCredentialKind::ApiKey | ModelProviderCredentialKind::LegacyRawApiKey => {
            "api_key"
        }
        ModelProviderCredentialKind::OpenAiOauth => "openai_oauth",
    }
}

fn model_provider_credential_kind_from_storage(
    raw: &str,
) -> CoreResult<ModelProviderCredentialKind> {
    match raw {
        "api_key" => Ok(ModelProviderCredentialKind::ApiKey),
        "openai_oauth" => Ok(ModelProviderCredentialKind::OpenAiOauth),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL service credential kind {other}"),
        )),
    }
}

fn model_provider_status_as_str(status: ModelProviderStatus) -> &'static str {
    match status {
        ModelProviderStatus::Active => "active",
        ModelProviderStatus::Disabled => "disabled",
        ModelProviderStatus::Archived => "archived",
    }
}

fn model_provider_protocol_as_str(protocol: ModelProviderProtocol) -> &'static str {
    match protocol {
        ModelProviderProtocol::Responses => "responses",
        ModelProviderProtocol::ChatCompletions => "chat_completions",
    }
}

fn postgres_purge_temp_strings(
    tx: &mut Transaction<'_>,
    table: &str,
    column: &str,
) -> CoreResult<Vec<String>> {
    let rows = tx
        .query(
            &format!("SELECT {column} FROM {table} ORDER BY {column} ASC"),
            &[],
        )
        .map_err(|error| postgres_error("query PostgreSQL profile purge temp table", error))?;
    rows.iter().map(|row| Ok(row.get::<_, String>(0))).collect()
}

fn postgres_purge_delete(
    tx: &mut Transaction<'_>,
    counts: &mut Vec<ProfilePurgeTableCount>,
    table: &str,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
) -> CoreResult<u64> {
    let rows = tx.execute(sql, params).map_err(|error| {
        postgres_error(
            &format!("purge PostgreSQL profile rows from {table}"),
            error,
        )
    })?;
    if rows > 0 {
        counts.push(ProfilePurgeTableCount {
            table: table.to_string(),
            rows_deleted: rows,
        });
    }
    Ok(rows)
}
