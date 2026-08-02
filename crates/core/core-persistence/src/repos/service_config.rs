use super::super::*;
use rusty_crew_core_protocol::ChatCompletionsPromptCachingPolicy;

impl CoordinationStore {
    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        validate_profile_registry_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start create profile registry record", error))?;
        if get_profile_registry_record(&tx, &write.profile_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile registry record {} already exists",
                    write.profile_id
                ),
            ));
        }
        insert_profile_registry_record_in_tx(&tx, write)?;
        let record = get_profile_registry_record(&tx, &write.profile_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "created profile registry record was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit create profile registry record", error))?;
        Ok(record)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        validate_profile_registry_write(&update.write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update profile registry record", error))?;
        let existing =
            get_profile_registry_record(&tx, &update.write.profile_id)?.ok_or_else(|| {
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
        update_profile_registry_record_in_tx(&tx, update, &existing)?;
        let record =
            get_profile_registry_record(&tx, &update.write.profile_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "updated profile registry record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit update profile registry record", error))?;
        Ok(record)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        validate_profile_registry_id(profile_id)?;
        let conn = self.conn()?;
        get_profile_registry_record(&conn, profile_id)
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        let conn = self.conn()?;
        query_profile_registry_records(&conn, query)
    }

    pub fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        validate_profile_registry_id(profile_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start purge profile", error))?;
        let report = purge_profile_in_tx(&tx, profile_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit purge profile", error))?;
        Ok(report)
    }

    pub fn update_profile_registry_lifecycle(
        &self,
        update: &ProfileRegistryLifecycleUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        validate_profile_registry_id(&update.profile_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update profile registry lifecycle", error))?;
        let existing = get_profile_registry_record(&tx, &update.profile_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("profile registry record {} not found", update.profile_id),
            )
        })?;
        if existing.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile registry revision mismatch for {}: expected {}, found {}",
                    update.profile_id, update.expected_revision, existing.revision
                ),
            ));
        }
        update_profile_registry_lifecycle_in_tx(&tx, update, existing.revision + 1)?;
        let record = get_profile_registry_record(&tx, &update.profile_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "updated profile registry record was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            persistence_error("commit update profile registry lifecycle", error)
        })?;
        Ok(record)
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        validate_model_provider_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start upsert model provider", error))?;
        let existing = get_model_provider(&tx, &write.alias)?;
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
        upsert_model_provider_in_tx(&tx, write, existing.as_ref())?;
        let record = get_model_provider(&tx, &write.alias)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "upserted model provider was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit upsert model provider", error))?;
        Ok(record)
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        validate_model_provider_alias(alias)?;
        let conn = self.conn()?;
        get_model_provider(&conn, alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        validate_model_provider_alias(alias)?;
        let conn = self.conn()?;
        get_model_provider_secret(&conn, alias)
    }

    pub fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        validate_service_credential_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start upsert service credential", error))?;
        let existing = get_service_credential(&tx, &write.credential_id)?;
        validate_expected_revision(
            "service credential",
            &write.credential_id,
            write.expected_revision,
            existing.as_ref().map_or(0, |record| record.revision),
        )?;
        upsert_service_credential_in_tx(&tx, write, existing.as_ref())?;
        let record = get_service_credential(&tx, &write.credential_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "upserted service credential was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit upsert service credential", error))?;
        Ok(record)
    }

    pub fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        validate_service_credential_id(credential_id)?;
        let conn = self.conn()?;
        get_service_credential(&conn, credential_id)
    }

    pub fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        validate_service_credential_id(credential_id)?;
        let conn = self.conn()?;
        get_service_credential_secret(&conn, credential_id)
    }

    pub fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        validate_service_credential_id(&delete.credential_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start delete service credential", error))?;
        let existing = get_service_credential(&tx, &delete.credential_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {} not found", delete.credential_id),
            )
        })?;
        validate_expected_revision(
            "service credential",
            &delete.credential_id,
            delete.expected_revision,
            existing.revision,
        )?;
        reject_linked_service_credential_mutation(&existing, "delete")?;
        let changed = tx
            .execute(
                "DELETE FROM service_credentials WHERE credential_id = ?1 AND revision = ?2",
                params![delete.credential_id, existing.revision as i64],
            )
            .map_err(|error| persistence_error("delete service credential", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "service credential {} changed concurrently",
                    delete.credential_id
                ),
            ));
        }
        tx.commit()
            .map_err(|error| persistence_error("commit delete service credential", error))?;
        Ok(existing)
    }

    pub fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        let conn = self.conn()?;
        query_service_credentials(&conn, query)
    }

    pub fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        validate_model_provider_alias(&link.provider_alias)?;
        validate_service_credential_id(&link.credential_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start model provider credential link", error))?;
        link_model_provider_credential_in_tx(&tx, link)?;
        let provider = get_model_provider(&tx, &link.provider_alias)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "linked provider was not readable",
            )
        })?;
        let credential = get_service_credential(&tx, &link.credential_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "linked credential was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit model provider credential link", error))?;
        Ok(ModelProviderCredentialLinkResult {
            provider,
            credential,
        })
    }

    pub fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        validate_model_provider_alias(&unlink.provider_alias)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start model provider credential unlink", error))?;
        let existing = get_model_provider(&tx, &unlink.provider_alias)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("model provider {} not found", unlink.provider_alias),
            )
        })?;
        validate_expected_revision(
            "model provider",
            &unlink.provider_alias,
            unlink.expected_provider_revision,
            existing.revision,
        )?;
        tx.execute(
            "UPDATE model_providers
                SET credential_id = NULL, revision = revision + 1, updated_at = ?2
              WHERE alias = ?1",
            params![unlink.provider_alias, unlink.now],
        )
        .map_err(|error| persistence_error("unlink model provider credential", error))?;
        let provider = get_model_provider(&tx, &unlink.provider_alias)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "unlinked provider was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit model provider credential unlink", error))?;
        Ok(provider)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        let conn = self.conn()?;
        query_model_providers(&conn, query)
    }

    pub fn save_channel_binding(&self, record: &ChannelBindingRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_channel_binding(&conn, record)
    }

    pub fn query_channel_bindings(
        &self,
        query: &ChannelBindingQuery,
    ) -> CoreResult<Vec<ChannelBindingRecord>> {
        let conn = self.conn()?;
        query_channel_bindings(&conn, query)
    }

    pub fn save_mcp_binding(&self, record: &McpBindingRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_mcp_binding(&conn, record)
    }

    pub fn query_mcp_bindings(&self, query: &McpBindingQuery) -> CoreResult<Vec<McpBindingRecord>> {
        let conn = self.conn()?;
        query_mcp_bindings(&conn, query)
    }
}

fn purge_profile_in_tx(
    tx: &rusqlite::Transaction<'_>,
    profile_id: &ProfileId,
) -> CoreResult<ProfilePurgeReport> {
    tx.execute_batch(
        "
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_sessions;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_agents;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_events;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_messages;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_attachments;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_lore_records;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_lore_layers;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_worker_members;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_worker_items;

        CREATE TEMP TABLE __rusty_profile_purge_sessions(session_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_agents(agent_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_events(sequence INTEGER PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_messages(message_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_attachments(attachment_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_lore_records(record_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_lore_layers(layer_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_worker_members(member_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE __rusty_profile_purge_worker_items(work_item_id TEXT PRIMARY KEY);
        ",
    )
    .map_err(|error| persistence_error("prepare profile purge temp tables", error))?;

    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_sessions(session_id)
         SELECT session_id FROM sessions WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile sessions from sessions", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_sessions(session_id)
         SELECT session_id FROM session_configs WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile sessions from configs", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_sessions(session_id)
         SELECT session_id FROM session_identity WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile sessions from identity", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_sessions(session_id)
         SELECT session_id FROM channel_bindings WHERE profile_id = ?1 AND session_id IS NOT NULL",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile sessions from channel bindings", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_sessions(session_id)
         SELECT session_id FROM mcp_bindings WHERE profile_id = ?1 AND session_id IS NOT NULL",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile sessions from mcp bindings", error))?;

    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_agents(agent_id)
         SELECT agent_id FROM agents WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile agents", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_agents(agent_id)
         SELECT agent_id FROM agent_instances WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile instance agents", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_agents(agent_id)
         SELECT agent_id FROM session_identity WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile session agents", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_agents(agent_id)
         SELECT agent_id FROM sessions WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile session state agents", error))?;

    let session_ids = purge_temp_strings(tx, "__rusty_profile_purge_sessions", "session_id")?
        .into_iter()
        .map(SessionId)
        .collect::<Vec<_>>();
    let agent_ids = purge_temp_strings(tx, "__rusty_profile_purge_agents", "agent_id")?
        .into_iter()
        .map(AgentId)
        .collect::<Vec<_>>();

    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_events(sequence)
         SELECT sequence FROM event_session_index
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )
    .map_err(|error| persistence_error("collect profile event sessions", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_events(sequence)
         SELECT sequence FROM event_agent_index
         WHERE agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )
    .map_err(|error| persistence_error("collect profile event agents", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_messages(message_id)
         SELECT message_id FROM messages
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )
    .map_err(|error| persistence_error("collect profile messages", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_attachments(attachment_id)
         SELECT attachment_id FROM attachments
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )
    .map_err(|error| persistence_error("collect profile attachments", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_lore_layers(layer_id)
         SELECT layer_id FROM module_roleplay_lore_layers WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile lore layers", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_lore_records(record_id)
         SELECT record_id FROM module_roleplay_lore_records
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )
    .map_err(|error| persistence_error("collect profile lore records by session", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_lore_records(record_id)
         SELECT record_id FROM module_roleplay_lore_layer_entries
         WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)",
        [],
    )
    .map_err(|error| persistence_error("collect profile lore records by layer", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_worker_members(member_id)
         SELECT member_id FROM worker_pool_members
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile worker members", error))?;
    tx.execute(
        "INSERT OR IGNORE INTO __rusty_profile_purge_worker_items(work_item_id)
         SELECT work_item_id FROM worker_pool_work_items
         WHERE requested_profile_id = ?1
            OR claimed_by_member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)",
        params![profile_id.0.as_str()],
    )
    .map_err(|error| persistence_error("collect profile worker items", error))?;

    let mut counts = Vec::new();
    purge_delete(
        tx,
        &mut counts,
        "external_interactions",
        "DELETE FROM external_interactions
         WHERE binding_id IN (
             SELECT binding_id FROM external_agent_bindings
             WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)
         )
            OR request_id IN (
                SELECT request_id FROM external_turns
                WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            )",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "external_control_receipts",
        "DELETE FROM external_control_receipts
         WHERE binding_id IN (
             SELECT binding_id FROM external_agent_bindings
             WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)
         )",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "external_runtime_event_checkpoints",
        "DELETE FROM external_runtime_event_checkpoints
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "external_runtime_events",
        "DELETE FROM external_runtime_events
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "external_turns",
        "DELETE FROM external_turns
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR binding_id IN (
                SELECT binding_id FROM external_agent_bindings
                WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
                   OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)
            )",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "external_agent_session_creations",
        "DELETE FROM external_agent_session_creations
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "agent_correlated_rounds",
        "DELETE FROM agent_correlated_rounds
         WHERE sender_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR recipient_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "agent_message_delivery_receipts",
        "DELETE FROM agent_message_delivery_receipts
         WHERE from_agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)
            OR to_agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "external_agent_bindings",
        "DELETE FROM external_agent_bindings
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "memory_governance_decisions",
        "DELETE FROM memory_governance_decisions
         WHERE proposal_id IN (
             SELECT proposal_id FROM memory_proposals
             WHERE scope_id = ?1
                OR scope_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
         )",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "memory_proposals",
        "DELETE FROM memory_proposals
         WHERE scope_id = ?1
            OR scope_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_lore_provenance_events",
        "DELETE FROM module_roleplay_lore_provenance_events
         WHERE record_id IN (SELECT record_id FROM __rusty_profile_purge_lore_records)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_lore_layer_entries",
        "DELETE FROM module_roleplay_lore_layer_entries
         WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)
            OR record_id IN (SELECT record_id FROM __rusty_profile_purge_lore_records)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_chat_layers",
        "DELETE FROM module_roleplay_chat_layers
         WHERE chat_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_lore_layer_config",
        "DELETE FROM module_roleplay_lore_layer_config
         WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_imports",
        "DELETE FROM module_roleplay_imports WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    for (table, sql) in [
        (
            "module_curator_approvals",
            "DELETE FROM module_curator_approvals WHERE candidate_id IN (SELECT candidate_id FROM module_curator_candidates WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))",
        ),
        (
            "module_curator_snapshot_refs",
            "DELETE FROM module_curator_snapshot_refs WHERE candidate_id IN (SELECT candidate_id FROM module_curator_candidates WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))",
        ),
        (
            "module_curator_mutations",
            "DELETE FROM module_curator_mutations WHERE candidate_id IN (SELECT candidate_id FROM module_curator_candidates WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions))",
        ),
        (
            "module_curator_audit_receipts",
            "DELETE FROM module_curator_audit_receipts WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        ),
        (
            "module_curator_candidates",
            "DELETE FROM module_curator_candidates WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        ),
    ] {
        purge_delete(tx, &mut counts, table, sql, params![profile_id.0.as_str()])?;
    }
    purge_delete(tx, &mut counts, "module_roleplay_session_metadata", "DELETE FROM module_roleplay_session_metadata WHERE profile_id = ?1 OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)", params![profile_id.0.as_str()])?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_player_personas",
        "DELETE FROM module_roleplay_player_personas WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_characters",
        "DELETE FROM module_roleplay_characters WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_lore_records",
        "DELETE FROM module_roleplay_lore_records
         WHERE record_id IN (SELECT record_id FROM __rusty_profile_purge_lore_records)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_lore_recall_traces",
        "DELETE FROM module_roleplay_lore_recall_traces
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "module_roleplay_lore_layers",
        "DELETE FROM module_roleplay_lore_layers
         WHERE layer_id IN (SELECT layer_id FROM __rusty_profile_purge_lore_layers)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "message_blocks",
        "DELETE FROM message_blocks
         WHERE message_id IN (SELECT message_id FROM __rusty_profile_purge_messages)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "message_variants",
        "DELETE FROM message_variants
         WHERE message_id IN (SELECT message_id FROM __rusty_profile_purge_messages)
            OR slot_id IN (
                SELECT slot_id FROM message_slots
                WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            )",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "chat_message_ingest_receipts",
        "DELETE FROM chat_message_ingest_receipts
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "message_slots",
        "DELETE FROM message_slots
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "messages",
        "DELETE FROM messages
         WHERE message_id IN (SELECT message_id FROM __rusty_profile_purge_messages)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "conversation_snapshots",
        "DELETE FROM conversation_snapshots
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "conversation_branch_state",
        "DELETE FROM conversation_branch_state
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "conversation_branches",
        "DELETE FROM conversation_branches
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "attachment_links",
        "DELETE FROM attachment_links
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR attachment_id IN (SELECT attachment_id FROM __rusty_profile_purge_attachments)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "attachments",
        "DELETE FROM attachments
         WHERE attachment_id IN (SELECT attachment_id FROM __rusty_profile_purge_attachments)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "data_bank_scopes",
        "DELETE FROM data_bank_scopes
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "provider_wire_states",
        "DELETE FROM provider_wire_states
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "session_activity_digests",
        "DELETE FROM session_activity_digests
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "context_compaction_artifacts",
        "DELETE FROM context_compaction_artifacts
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "session_memory_records",
        "DELETE FROM session_memory_records
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "profile_memories",
        "DELETE FROM profile_memories WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "completion_packets",
        "DELETE FROM completion_packets
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "tool_call_history",
        "DELETE FROM tool_call_history
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "queued_messages",
        "DELETE FROM queued_messages
         WHERE owner_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR owner_agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)
            OR from_agent IN (SELECT agent_id FROM __rusty_profile_purge_agents)
            OR to_agent IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "scheduled_job_runs",
        "DELETE FROM scheduled_job_runs
         WHERE target_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR job_id IN (
                SELECT job_id FROM scheduled_jobs
                WHERE target_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            )",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "scheduled_jobs",
        "DELETE FROM scheduled_jobs
         WHERE target_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "worker_pool_events",
        "DELETE FROM worker_pool_events
         WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
            OR work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)
            OR lease_id IN (
                SELECT lease_id FROM worker_pool_leases
                WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
                   OR work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)
            )",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "worker_pool_leases",
        "DELETE FROM worker_pool_leases
         WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)
            OR work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "worker_pool_work_items",
        "DELETE FROM worker_pool_work_items
         WHERE work_item_id IN (SELECT work_item_id FROM __rusty_profile_purge_worker_items)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "worker_pool_members",
        "DELETE FROM worker_pool_members
         WHERE member_id IN (SELECT member_id FROM __rusty_profile_purge_worker_members)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "worker_runs",
        "DELETE FROM worker_runs
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR delegated_session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "agent_messages",
        "DELETE FROM agent_messages
         WHERE from_agent IN (SELECT agent_id FROM __rusty_profile_purge_agents)
            OR to_agent IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "runtime_search_fts",
        "DELETE FROM runtime_search_fts
         WHERE session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)
            OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "runtime_counters",
        "DELETE FROM runtime_counters
         WHERE (scope_type = 'profile' AND scope_id = ?1)
            OR (scope_type = 'session' AND scope_id IN (
                SELECT session_id FROM __rusty_profile_purge_sessions
            ))
            OR (scope_type = 'agent' AND scope_id IN (
                SELECT agent_id FROM __rusty_profile_purge_agents
            ))",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "event_wake_index",
        "DELETE FROM event_wake_index
         WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "event_correlation_index",
        "DELETE FROM event_correlation_index
         WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "event_instance_index",
        "DELETE FROM event_instance_index
         WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "event_agent_index",
        "DELETE FROM event_agent_index
         WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)
            OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "event_session_index",
        "DELETE FROM event_session_index
         WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "event_history",
        "DELETE FROM event_history
         WHERE sequence IN (SELECT sequence FROM __rusty_profile_purge_events)",
        [],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "channel_bindings",
        "DELETE FROM channel_bindings
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "mcp_bindings",
        "DELETE FROM mcp_bindings
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "session_configs",
        "DELETE FROM session_configs
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "session_identity",
        "DELETE FROM session_identity
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "agent_instances",
        "DELETE FROM agent_instances
         WHERE profile_id = ?1
            OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "agents",
        "DELETE FROM agents
         WHERE profile_id = ?1
            OR agent_id IN (SELECT agent_id FROM __rusty_profile_purge_agents)",
        params![profile_id.0.as_str()],
    )?;
    purge_delete(
        tx,
        &mut counts,
        "sessions",
        "DELETE FROM sessions
         WHERE profile_id = ?1
            OR session_id IN (SELECT session_id FROM __rusty_profile_purge_sessions)",
        params![profile_id.0.as_str()],
    )?;
    let profile_registry_deleted = purge_delete(
        tx,
        &mut counts,
        "profile_registry",
        "DELETE FROM profile_registry WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
    )? > 0;

    tx.execute_batch(
        "
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_sessions;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_agents;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_events;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_messages;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_attachments;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_lore_records;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_lore_layers;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_worker_members;
        DROP TABLE IF EXISTS temp.__rusty_profile_purge_worker_items;
        ",
    )
    .map_err(|error| persistence_error("drop profile purge temp tables", error))?;

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

fn purge_temp_strings(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
) -> CoreResult<Vec<String>> {
    let mut stmt = tx
        .prepare(&format!(
            "SELECT {column} FROM {table} ORDER BY {column} ASC"
        ))
        .map_err(|error| persistence_error("prepare profile purge temp read", error))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| persistence_error("query profile purge temp read", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load profile purge temp read", error))
}

fn purge_delete<P>(
    tx: &rusqlite::Transaction<'_>,
    counts: &mut Vec<ProfilePurgeTableCount>,
    table: &str,
    sql: &str,
    params: P,
) -> CoreResult<u64>
where
    P: rusqlite::Params,
{
    let rows = tx
        .execute(sql, params)
        .map_err(|error| persistence_error(&format!("purge profile rows from {table}"), error))?
        as u64;
    if rows > 0 {
        counts.push(ProfilePurgeTableCount {
            table: table.to_string(),
            rows_deleted: rows,
        });
    }
    Ok(rows)
}

fn query_profile_registry_records(
    conn: &Connection,
    query: &ProfileRegistryQuery,
) -> CoreResult<Vec<ProfileRegistryRecord>> {
    let lifecycle_status = query
        .lifecycle_status
        .as_ref()
        .map(profile_registry_lifecycle_status_as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                profile_id,
                lifecycle_status,
                display_name,
                summary,
                default_session_kind,
                agent_id,
                owner_id,
                prompt_soul_markdown,
                prompt_memory_markdown,
                active_runtime_settings_json,
                source_asset_refs_json,
                derived_runtime_refs_json,
                import_export_json,
                revision,
                created_at,
                updated_at
             FROM profile_registry
             WHERE (?1 IS NULL OR lifecycle_status = ?1)
             ORDER BY updated_at DESC, profile_id ASC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| persistence_error("prepare query profile registry", error))?;
    let rows = stmt
        .query_map(
            params![lifecycle_status, limit, offset],
            row_to_profile_registry_record,
        )
        .map_err(|error| persistence_error("query profile registry", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load profile registry records", error))
}

fn get_profile_registry_record(
    conn: &Connection,
    profile_id: &ProfileId,
) -> CoreResult<Option<ProfileRegistryRecord>> {
    conn.query_row(
        "SELECT
            profile_id,
            lifecycle_status,
            display_name,
            summary,
            default_session_kind,
            agent_id,
            owner_id,
            prompt_soul_markdown,
            prompt_memory_markdown,
            active_runtime_settings_json,
            source_asset_refs_json,
            derived_runtime_refs_json,
            import_export_json,
            revision,
            created_at,
            updated_at
         FROM profile_registry
         WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
        row_to_profile_registry_record,
    )
    .optional()
    .map_err(|error| persistence_error("get profile registry record", error))
}

fn insert_profile_registry_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProfileRegistryWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO profile_registry (
            profile_id,
            lifecycle_status,
            display_name,
            summary,
            default_session_kind,
            agent_id,
            owner_id,
            prompt_soul_markdown,
            prompt_memory_markdown,
            active_runtime_settings_json,
            source_asset_refs_json,
            derived_runtime_refs_json,
            import_export_json,
            revision,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?14)",
        params![
            write.profile_id.0.as_str(),
            profile_registry_lifecycle_status_as_str(&write.lifecycle_status),
            write.display_name.as_deref(),
            write.summary.as_deref(),
            write.default_session_kind.as_ref().map(session_kind_as_str),
            write.agent_id.as_ref().map(|value| value.0.as_str()),
            write.owner_id.as_deref(),
            write.prompt_soul_markdown.as_deref(),
            write.prompt_memory_markdown.as_deref(),
            to_json_text(&write.active_runtime_settings_json)?,
            to_json_text(&write.source_asset_refs)?,
            to_json_text(&write.derived_runtime_refs)?,
            to_json_text(&write.import_export)?,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert profile registry record", error))?;
    Ok(())
}

fn update_profile_registry_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    update: &ProfileRegistryUpdate,
    existing: &ProfileRegistryRecord,
) -> CoreResult<()> {
    let write = &update.write;
    let revision = existing.revision + 1;
    tx.execute(
        "UPDATE profile_registry
             SET lifecycle_status = ?2,
                 display_name = ?3,
                 summary = ?4,
                 default_session_kind = ?5,
                 agent_id = ?6,
                 owner_id = ?7,
                 prompt_soul_markdown = ?8,
                 prompt_memory_markdown = ?9,
                 active_runtime_settings_json = ?10,
                 source_asset_refs_json = ?11,
                 derived_runtime_refs_json = ?12,
                 import_export_json = ?13,
                 revision = ?14,
                 updated_at = ?15
         WHERE profile_id = ?1",
        params![
            write.profile_id.0.as_str(),
            profile_registry_lifecycle_status_as_str(&write.lifecycle_status),
            write.display_name.as_deref(),
            write.summary.as_deref(),
            write.default_session_kind.as_ref().map(session_kind_as_str),
            write.agent_id.as_ref().map(|value| value.0.as_str()),
            write.owner_id.as_deref(),
            write.prompt_soul_markdown.as_deref(),
            write.prompt_memory_markdown.as_deref(),
            to_json_text(&write.active_runtime_settings_json)?,
            to_json_text(&write.source_asset_refs)?,
            to_json_text(&write.derived_runtime_refs)?,
            to_json_text(&write.import_export)?,
            revision as i64,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update profile registry record", error))?;
    Ok(())
}

fn update_profile_registry_lifecycle_in_tx(
    tx: &rusqlite::Transaction<'_>,
    update: &ProfileRegistryLifecycleUpdate,
    revision: u64,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE profile_registry
         SET lifecycle_status = ?2,
             revision = ?3,
             updated_at = ?4
         WHERE profile_id = ?1",
        params![
            update.profile_id.0.as_str(),
            profile_registry_lifecycle_status_as_str(&update.lifecycle_status),
            revision as i64,
            update.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update profile registry lifecycle", error))?;
    Ok(())
}

fn row_to_profile_registry_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProfileRegistryRecord> {
    let lifecycle_status: String = row.get(1)?;
    let default_session_kind: Option<String> = row.get(4)?;
    let active_runtime_settings_json: String = row.get(9)?;
    let source_asset_refs_json: String = row.get(10)?;
    let derived_runtime_refs_json: String = row.get(11)?;
    let import_export_json: String = row.get(12)?;
    Ok(ProfileRegistryRecord {
        profile_id: ProfileId::new(row.get::<_, String>(0)?),
        lifecycle_status: profile_registry_lifecycle_status_from_str(&lifecycle_status)?,
        display_name: row.get(2)?,
        summary: row.get(3)?,
        default_session_kind: default_session_kind
            .as_deref()
            .map(session_kind_from_str)
            .transpose()?,
        agent_id: row.get::<_, Option<String>>(5)?.map(AgentId::new),
        owner_id: row.get(6)?,
        prompt_soul_markdown: row.get(7)?,
        prompt_memory_markdown: row.get(8)?,
        active_runtime_settings_json: from_json_text(&active_runtime_settings_json)
            .map_err(to_sql_error)?,
        source_asset_refs: from_json_text(&source_asset_refs_json).map_err(to_sql_error)?,
        derived_runtime_refs: from_json_text(&derived_runtime_refs_json).map_err(to_sql_error)?,
        import_export: from_json_text(&import_export_json).map_err(to_sql_error)?,
        revision: row.get::<_, i64>(13)? as u64,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn query_model_providers(
    conn: &Connection,
    query: &ModelProviderQuery,
) -> CoreResult<Vec<ModelProviderRecord>> {
    let status = query.status.as_ref().map(model_provider_status_as_str);
    let alias_prefix = query
        .alias_prefix
        .as_deref()
        .map(|value| format!("{value}%"));
    let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT
                mp.alias, mp.status, mp.protocol, mp.provider_kind,
                mp.display_name, mp.description, mp.base_url, mp.model_id,
                mp.context_window_tokens, mp.max_output_tokens, mp.temperature_milli,
                mp.reasoning_effort, mp.reasoning_format, mp.responses_dialect,
                mp.chat_completions_dialect, mp.thinking_mode,
                mp.reasoning_history, mp.reasoning_budget_tokens, mp.prompt_caching,
                mp.credential_id,
                sc.secret_ciphertext, sc.secret_updated_at, sc.credential_kind, sc.revision,
                mp.metadata_json, mp.revision, mp.created_at, mp.updated_at
             FROM model_providers mp
             LEFT JOIN service_credentials sc ON sc.credential_id = mp.credential_id
             WHERE (?1 IS NULL OR mp.status = ?1)
               AND (?2 IS NULL OR mp.alias LIKE ?2)
             ORDER BY mp.updated_at DESC, mp.alias ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query model providers", error))?;
    let rows = stmt
        .query_map(
            params![status, alias_prefix, limit, offset],
            row_to_model_provider,
        )
        .map_err(|error| persistence_error("query model providers", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load model provider records", error))
}

fn get_model_provider(conn: &Connection, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
    conn.query_row(
        "SELECT
            mp.alias, mp.status, mp.protocol, mp.provider_kind,
            mp.display_name, mp.description, mp.base_url, mp.model_id,
            mp.context_window_tokens, mp.max_output_tokens, mp.temperature_milli,
            mp.reasoning_effort, mp.reasoning_format, mp.responses_dialect,
            mp.chat_completions_dialect, mp.thinking_mode,
            mp.reasoning_history, mp.reasoning_budget_tokens, mp.prompt_caching,
            mp.credential_id,
            sc.secret_ciphertext, sc.secret_updated_at, sc.credential_kind, sc.revision,
            mp.metadata_json, mp.revision, mp.created_at, mp.updated_at
         FROM model_providers mp
         LEFT JOIN service_credentials sc ON sc.credential_id = mp.credential_id
         WHERE mp.alias = ?1",
        params![alias],
        row_to_model_provider,
    )
    .optional()
    .map_err(|error| persistence_error("get model provider", error))
}

fn get_model_provider_secret(conn: &Connection, alias: &str) -> CoreResult<Option<String>> {
    conn.query_row(
        "SELECT sc.secret_ciphertext
         FROM model_providers mp
         LEFT JOIN service_credentials sc ON sc.credential_id = mp.credential_id
         WHERE mp.alias = ?1",
        params![alias],
        |row| row.get(0),
    )
    .optional()
    .map(|value: Option<Option<String>>| value.flatten())
    .map_err(|error| persistence_error("get model provider secret", error))
}

fn upsert_model_provider_in_tx(
    tx: &rusqlite::Transaction<'_>,
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
        let current = get_service_credential(tx, &id)?;
        if let Some(expected) = write.expected_credential_revision {
            validate_expected_revision(
                "service credential",
                &id,
                Some(expected),
                current.as_ref().map_or(0, |record| record.revision),
            )?;
        }
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
        upsert_service_credential_in_tx(tx, &credential_write, current.as_ref())?;
        credential_id = Some(id);
    } else if let Some(id) = credential_id.as_deref() {
        let credential = get_service_credential(tx, id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {id} not found"),
            )
        })?;
        validate_provider_credential_compatibility(
            &write.provider_kind,
            write.protocol,
            &credential,
        )?;
    }
    tx.execute(
        "INSERT INTO model_providers (
            alias,
            status,
            protocol,
            provider_kind,
            display_name,
            description,
            base_url,
            model_id,
            context_window_tokens,
            max_output_tokens,
            temperature_milli,
            reasoning_effort,
            reasoning_format,
            responses_dialect,
            chat_completions_dialect,
            thinking_mode,
            reasoning_history,
            reasoning_budget_tokens,
            prompt_caching,
            secret_ciphertext,
            secret_updated_at,
            metadata_json,
            revision,
            created_at,
            updated_at,
            credential_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, NULL, NULL, ?20, ?21, ?22, ?23, ?24)
        ON CONFLICT(alias) DO UPDATE SET
            status = excluded.status,
            protocol = excluded.protocol,
            provider_kind = excluded.provider_kind,
            display_name = excluded.display_name,
            description = excluded.description,
            base_url = excluded.base_url,
            model_id = excluded.model_id,
            context_window_tokens = excluded.context_window_tokens,
            max_output_tokens = excluded.max_output_tokens,
            temperature_milli = excluded.temperature_milli,
            reasoning_effort = excluded.reasoning_effort,
            reasoning_format = excluded.reasoning_format,
            responses_dialect = excluded.responses_dialect,
            chat_completions_dialect = excluded.chat_completions_dialect,
            thinking_mode = excluded.thinking_mode,
            reasoning_history = excluded.reasoning_history,
            reasoning_budget_tokens = excluded.reasoning_budget_tokens,
            prompt_caching = excluded.prompt_caching,
            secret_ciphertext = NULL,
            secret_updated_at = NULL,
            metadata_json = excluded.metadata_json,
            revision = excluded.revision,
            updated_at = excluded.updated_at,
            credential_id = excluded.credential_id",
        params![
            write.alias.as_str(),
            model_provider_status_as_str(&write.status),
            model_provider_protocol_as_str(&write.protocol),
            write.provider_kind.as_str(),
            write.display_name.as_deref(),
            write.description.as_deref(),
            write.base_url.as_deref(),
            write.model_id.as_str(),
            write.context_window_tokens.map(|value| value as i64),
            write.max_output_tokens.map(|value| value as i64),
            write.temperature_milli.map(|value| value as i64),
            write.reasoning_effort.as_deref(),
            write.reasoning_format.as_deref(),
            write.responses_dialect.map(responses_provider_dialect_as_str),
            chat_completions_dialect_as_str(write.chat_completions_dialect),
            chat_completions_thinking_mode_as_str(write.thinking_mode),
            chat_completions_reasoning_history_as_str(write.reasoning_history),
            write.reasoning_budget_tokens.map(|value| value as i64),
            chat_completions_prompt_caching_as_str(write.prompt_caching),
            to_json_text(&write.metadata_json)?,
            revision as i64,
            created_at.as_str(),
            write.now.as_str(),
            credential_id.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("upsert model provider", error))?;
    Ok(())
}

fn row_to_model_provider(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelProviderRecord> {
    let status: String = row.get(1)?;
    let protocol: String = row.get(2)?;
    let responses_dialect: Option<String> = row.get(13)?;
    let dialect: String = row.get(14)?;
    let thinking_mode: String = row.get(15)?;
    let reasoning_history: String = row.get(16)?;
    let prompt_caching: String = row.get(18)?;
    let credential_id: Option<String> = row.get(19)?;
    let secret_ciphertext: Option<String> = row.get(20)?;
    let credential_kind: Option<String> = row.get(22)?;
    let metadata_json: String = row.get(24)?;
    Ok(ModelProviderRecord {
        alias: row.get(0)?,
        status: model_provider_status_from_str(&status)?,
        protocol: model_provider_protocol_from_str(&protocol)?,
        provider_kind: row.get(3)?,
        display_name: row.get(4)?,
        description: row.get(5)?,
        base_url: row.get(6)?,
        model_id: row.get(7)?,
        context_window_tokens: row.get::<_, Option<i64>>(8)?.map(|value| value as u32),
        max_output_tokens: row.get::<_, Option<i64>>(9)?.map(|value| value as u32),
        temperature_milli: row.get::<_, Option<i64>>(10)?.map(|value| value as u32),
        reasoning_effort: row.get(11)?,
        reasoning_format: row.get(12)?,
        responses_dialect: responses_dialect
            .as_deref()
            .map(responses_provider_dialect_from_str)
            .transpose()?,
        chat_completions_dialect: chat_completions_dialect_from_str(&dialect)?,
        thinking_mode: chat_completions_thinking_mode_from_str(&thinking_mode)?,
        reasoning_history: chat_completions_reasoning_history_from_str(&reasoning_history)?,
        reasoning_budget_tokens: row.get::<_, Option<i64>>(17)?.map(|value| value as u32),
        prompt_caching: chat_completions_prompt_caching_from_str(&prompt_caching)?,
        credential_id: credential_id.clone(),
        credential: ModelProviderCredential {
            has_secret: secret_ciphertext.is_some(),
            secret_ref: secret_ciphertext
                .as_ref()
                .and(credential_id.as_ref())
                .map(|id| format!("db://service_credentials/{id}/secret")),
            updated_at: row.get(21)?,
            kind: credential_kind
                .as_deref()
                .map(model_provider_credential_kind_from_str)
                .transpose()?,
            revision: row.get::<_, Option<i64>>(23)?.map(|value| value as u64),
        },
        metadata_json: from_json_text(&metadata_json).map_err(to_sql_error)?,
        revision: row.get::<_, i64>(25)? as u64,
        created_at: row.get(26)?,
        updated_at: row.get(27)?,
    })
}

fn upsert_service_credential_in_tx(
    tx: &rusqlite::Transaction<'_>,
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
            reject_linked_service_credential_mutation(existing, "clear")?;
        }
        for alias in &existing.linked_provider_aliases {
            let provider = get_model_provider(tx, alias)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!("linked model provider {alias} was not readable"),
                )
            })?;
            validate_provider_credential_fields(
                &provider.provider_kind,
                provider.protocol,
                &write.provider_kind,
                write.credential_kind,
                &write.credential_id,
            )?;
        }
    }
    let preserved_secret = if write.clear_secret || incoming_secret.is_some() {
        None
    } else {
        get_service_credential_secret(tx, &write.credential_id)?
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
            "UPDATE service_credentials
                SET display_name = ?2,
                    provider_kind = ?3,
                    credential_kind = ?4,
                    secret_ciphertext = ?5,
                    secret_updated_at = ?6,
                    revision = ?7,
                    updated_at = ?8
              WHERE credential_id = ?1 AND revision = ?9",
            params![
                write.credential_id,
                write.display_name,
                write.provider_kind,
                model_provider_credential_kind_as_str(write.credential_kind),
                secret,
                secret_updated_at,
                revision as i64,
                write.now,
                existing.revision as i64,
            ],
        )
    } else {
        tx.execute(
            "INSERT INTO service_credentials (
                credential_id, display_name, provider_kind, credential_kind,
                secret_ciphertext, secret_updated_at, revision, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(credential_id) DO NOTHING",
            params![
                write.credential_id,
                write.display_name,
                write.provider_kind,
                model_provider_credential_kind_as_str(write.credential_kind),
                secret,
                secret_updated_at,
                revision as i64,
                created_at,
                write.now,
            ],
        )
    }
    .map_err(|error| persistence_error("upsert service credential", error))?;
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

fn reject_linked_service_credential_mutation(
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

fn get_service_credential(
    conn: &Connection,
    credential_id: &str,
) -> CoreResult<Option<ServiceCredentialRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT sc.credential_id, sc.display_name, sc.provider_kind,
                    sc.credential_kind, sc.secret_ciphertext, sc.secret_updated_at,
                    sc.revision, sc.created_at, sc.updated_at,
                    COALESCE(group_concat(mp.alias, char(31)), '')
               FROM service_credentials sc
               LEFT JOIN model_providers mp ON mp.credential_id = sc.credential_id
              WHERE sc.credential_id = ?1
              GROUP BY sc.credential_id",
        )
        .map_err(|error| persistence_error("prepare get service credential", error))?;
    stmt.query_row(params![credential_id], row_to_service_credential)
        .optional()
        .map_err(|error| persistence_error("get service credential", error))
}

fn get_service_credential_secret(
    conn: &Connection,
    credential_id: &str,
) -> CoreResult<Option<String>> {
    conn.query_row(
        "SELECT secret_ciphertext FROM service_credentials WHERE credential_id = ?1",
        params![credential_id],
        |row| row.get(0),
    )
    .optional()
    .map(|value: Option<Option<String>>| value.flatten())
    .map_err(|error| persistence_error("get service credential secret", error))
}

fn query_service_credentials(
    conn: &Connection,
    query: &ServiceCredentialQuery,
) -> CoreResult<Vec<ServiceCredentialRecord>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT sc.credential_id, sc.display_name, sc.provider_kind,
                    sc.credential_kind, sc.secret_ciphertext, sc.secret_updated_at,
                    sc.revision, sc.created_at, sc.updated_at,
                    COALESCE(group_concat(mp.alias, char(31)), '')
               FROM service_credentials sc
               LEFT JOIN model_providers mp ON mp.credential_id = sc.credential_id
              WHERE (?1 IS NULL OR sc.provider_kind = ?1)
              GROUP BY sc.credential_id
              ORDER BY sc.updated_at DESC, sc.credential_id ASC
              LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| persistence_error("prepare list service credentials", error))?;
    let rows = stmt
        .query_map(
            params![query.provider_kind, limit, offset],
            row_to_service_credential,
        )
        .map_err(|error| persistence_error("query service credentials", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load service credentials", error))
}

fn row_to_service_credential(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServiceCredentialRecord> {
    let credential_id: String = row.get(0)?;
    let kind = model_provider_credential_kind_from_str(&row.get::<_, String>(3)?)?;
    let secret: Option<String> = row.get(4)?;
    let aliases: String = row.get(9)?;
    Ok(ServiceCredentialRecord {
        credential_id: credential_id.clone(),
        display_name: row.get(1)?,
        provider_kind: row.get(2)?,
        credential_kind: kind,
        credential: ModelProviderCredential {
            has_secret: secret.is_some(),
            secret_ref: secret
                .as_ref()
                .map(|_| format!("db://service_credentials/{credential_id}/secret")),
            updated_at: row.get(5)?,
            kind: Some(kind),
            revision: Some(row.get::<_, i64>(6)? as u64),
        },
        linked_provider_aliases: aliases
            .split('\u{1f}')
            .filter(|alias| !alias.is_empty())
            .map(str::to_string)
            .collect(),
        revision: row.get::<_, i64>(6)? as u64,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn link_model_provider_credential_in_tx(
    tx: &rusqlite::Transaction<'_>,
    link: &ModelProviderCredentialLink,
) -> CoreResult<()> {
    let provider = get_model_provider(tx, &link.provider_alias)?.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("model provider {} not found", link.provider_alias),
        )
    })?;
    let credential = get_service_credential(tx, &link.credential_id)?.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("service credential {} not found", link.credential_id),
        )
    })?;
    validate_expected_revision(
        "model provider",
        &link.provider_alias,
        link.expected_provider_revision,
        provider.revision,
    )?;
    validate_expected_revision(
        "service credential",
        &link.credential_id,
        link.expected_credential_revision,
        credential.revision,
    )?;
    validate_provider_credential_compatibility(
        &provider.provider_kind,
        provider.protocol,
        &credential,
    )?;
    tx.execute(
        "UPDATE model_providers
            SET credential_id = ?2, revision = revision + 1, updated_at = ?3
          WHERE alias = ?1",
        params![link.provider_alias, link.credential_id, link.now],
    )
    .map_err(|error| persistence_error("link model provider credential", error))?;
    Ok(())
}

fn save_channel_binding(conn: &Connection, record: &ChannelBindingRecord) -> CoreResult<()> {
    let provenance_json = to_json_text(&record.provenance)?;
    conn.execute(
        "INSERT INTO channel_bindings (
            binding_id,
            adapter_id,
            provider,
            agent_id,
            instance_id,
            session_id,
            profile_id,
            external_channel_id,
            external_thread_id,
            external_user_id,
            provider_subscription_id,
            cursor,
            membership_state,
            presence_state,
            status,
            degraded_reason,
            provenance_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
        ON CONFLICT(binding_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            provider = excluded.provider,
            agent_id = excluded.agent_id,
            instance_id = excluded.instance_id,
            session_id = excluded.session_id,
            profile_id = excluded.profile_id,
            external_channel_id = excluded.external_channel_id,
            external_thread_id = excluded.external_thread_id,
            external_user_id = excluded.external_user_id,
            provider_subscription_id = excluded.provider_subscription_id,
            cursor = excluded.cursor,
            membership_state = excluded.membership_state,
            presence_state = excluded.presence_state,
            status = excluded.status,
            degraded_reason = excluded.degraded_reason,
            provenance_json = excluded.provenance_json,
            updated_at = excluded.updated_at",
        params![
            record.binding_id,
            record.adapter_id.0,
            record.provider,
            record.agent_id.0,
            record.instance_id.as_ref().map(|value| value.0.as_str()),
            record.session_id.as_ref().map(|value| value.0.as_str()),
            record.profile_id.0,
            record.external_channel_id,
            record.external_thread_id,
            record.external_user_id,
            record.provider_subscription_id,
            record.cursor,
            record.membership_state,
            record.presence_state,
            record.status.as_str(),
            record.degraded_reason,
            provenance_json,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save channel binding", error))?;
    Ok(())
}

fn query_channel_bindings(
    conn: &Connection,
    query: &ChannelBindingQuery,
) -> CoreResult<Vec<ChannelBindingRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let instance_id = query.instance_id.as_ref().map(|value| value.0.as_str());
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let adapter_id = query.adapter_id.as_ref().map(|value| value.0.as_str());
    let provider = query.provider.as_deref();
    let external_channel_id = query.external_channel_id.as_deref();
    let status = query.status.map(ExternalBindingStatus::as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                binding_id,
                adapter_id,
                provider,
                agent_id,
                instance_id,
                session_id,
                profile_id,
                external_channel_id,
                external_thread_id,
                external_user_id,
                provider_subscription_id,
                cursor,
                membership_state,
                presence_state,
                status,
                degraded_reason,
                provenance_json,
                created_at,
                updated_at
             FROM channel_bindings
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR instance_id = ?2)
               AND (?3 IS NULL OR session_id = ?3)
               AND (?4 IS NULL OR profile_id = ?4)
               AND (?5 IS NULL OR adapter_id = ?5)
               AND (?6 IS NULL OR provider = ?6)
               AND (?7 IS NULL OR external_channel_id = ?7)
               AND (?8 IS NULL OR status = ?8)
             ORDER BY provider ASC, external_channel_id ASC, binding_id ASC
             LIMIT ?9 OFFSET ?10",
        )
        .map_err(|error| persistence_error("prepare channel binding query", error))?;
    let rows = stmt
        .query_map(
            params![
                agent_id,
                instance_id,
                session_id,
                profile_id,
                adapter_id,
                provider,
                external_channel_id,
                status,
                limit,
                offset,
            ],
            row_to_channel_binding,
        )
        .map_err(|error| persistence_error("query channel bindings", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load channel bindings", error))
}

fn row_to_channel_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelBindingRecord> {
    let status: String = row.get(14)?;
    let provenance_json: String = row.get(16)?;
    Ok(ChannelBindingRecord {
        binding_id: row.get(0)?,
        adapter_id: AdapterId(row.get(1)?),
        provider: row.get(2)?,
        agent_id: AgentId(row.get(3)?),
        instance_id: row.get::<_, Option<String>>(4)?.map(AgentInstanceId),
        session_id: row.get::<_, Option<String>>(5)?.map(SessionId),
        profile_id: ProfileId(row.get(6)?),
        external_channel_id: row.get(7)?,
        external_thread_id: row.get(8)?,
        external_user_id: row.get(9)?,
        provider_subscription_id: row.get(10)?,
        cursor: row.get(11)?,
        membership_state: row.get(12)?,
        presence_state: row.get(13)?,
        status: external_binding_status_from_str(&status)?,
        degraded_reason: row.get(15)?,
        provenance: from_json_text(&provenance_json).map_err(to_sql_error)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn save_mcp_binding(conn: &Connection, record: &McpBindingRecord) -> CoreResult<()> {
    let server_names_json = to_json_text(&record.server_names)?;
    let diagnostics_json = to_json_text(&record.diagnostics)?;
    conn.execute(
        "INSERT INTO mcp_bindings (
            binding_id,
            adapter_id,
            agent_id,
            instance_id,
            session_id,
            profile_id,
            server_names_json,
            endpoint_ref,
            transport,
            tool_profile_key,
            discovered_tool_revision,
            status,
            degraded_reason,
            diagnostics_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(binding_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            agent_id = excluded.agent_id,
            instance_id = excluded.instance_id,
            session_id = excluded.session_id,
            profile_id = excluded.profile_id,
            server_names_json = excluded.server_names_json,
            endpoint_ref = excluded.endpoint_ref,
            transport = excluded.transport,
            tool_profile_key = excluded.tool_profile_key,
            discovered_tool_revision = excluded.discovered_tool_revision,
            status = excluded.status,
            degraded_reason = excluded.degraded_reason,
            diagnostics_json = excluded.diagnostics_json,
            updated_at = excluded.updated_at",
        params![
            record.binding_id,
            record.adapter_id.0,
            record.agent_id.0,
            record.instance_id.as_ref().map(|value| value.0.as_str()),
            record.session_id.as_ref().map(|value| value.0.as_str()),
            record.profile_id.0,
            server_names_json,
            record.endpoint_ref,
            record.transport,
            record.tool_profile_key,
            record.discovered_tool_revision,
            record.status.as_str(),
            record.degraded_reason,
            diagnostics_json,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save MCP binding", error))?;
    Ok(())
}

fn query_mcp_bindings(
    conn: &Connection,
    query: &McpBindingQuery,
) -> CoreResult<Vec<McpBindingRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let instance_id = query.instance_id.as_ref().map(|value| value.0.as_str());
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let adapter_id = query.adapter_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.map(ExternalBindingStatus::as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                binding_id,
                adapter_id,
                agent_id,
                instance_id,
                session_id,
                profile_id,
                server_names_json,
                endpoint_ref,
                transport,
                tool_profile_key,
                discovered_tool_revision,
                status,
                degraded_reason,
                diagnostics_json,
                created_at,
                updated_at
             FROM mcp_bindings
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR instance_id = ?2)
               AND (?3 IS NULL OR session_id = ?3)
               AND (?4 IS NULL OR profile_id = ?4)
               AND (?5 IS NULL OR adapter_id = ?5)
               AND (?6 IS NULL OR status = ?6)
             ORDER BY agent_id ASC, profile_id ASC, binding_id ASC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|error| persistence_error("prepare MCP binding query", error))?;
    let rows = stmt
        .query_map(
            params![
                agent_id,
                instance_id,
                session_id,
                profile_id,
                adapter_id,
                status,
                limit,
                offset,
            ],
            row_to_mcp_binding,
        )
        .map_err(|error| persistence_error("query MCP bindings", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load MCP bindings", error))
}

fn row_to_mcp_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpBindingRecord> {
    let server_names_json: String = row.get(6)?;
    let status: String = row.get(11)?;
    let diagnostics_json: String = row.get(13)?;
    Ok(McpBindingRecord {
        binding_id: row.get(0)?,
        adapter_id: AdapterId(row.get(1)?),
        agent_id: AgentId(row.get(2)?),
        instance_id: row.get::<_, Option<String>>(3)?.map(AgentInstanceId),
        session_id: row.get::<_, Option<String>>(4)?.map(SessionId),
        profile_id: ProfileId(row.get(5)?),
        server_names: from_json_text(&server_names_json).map_err(to_sql_error)?,
        endpoint_ref: row.get(7)?,
        transport: row.get(8)?,
        tool_profile_key: row.get(9)?,
        discovered_tool_revision: row.get(10)?,
        status: external_binding_status_from_str(&status)?,
        degraded_reason: row.get(12)?,
        diagnostics: from_json_text(&diagnostics_json).map_err(to_sql_error)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn external_binding_status_from_str(raw: &str) -> rusqlite::Result<ExternalBindingStatus> {
    match raw {
        "active" => Ok(ExternalBindingStatus::Active),
        "degraded" => Ok(ExternalBindingStatus::Degraded),
        "disconnected" => Ok(ExternalBindingStatus::Disconnected),
        "archived" => Ok(ExternalBindingStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            14,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown external binding status {other}"),
            )),
        )),
    }
}

fn profile_registry_lifecycle_status_as_str(
    status: &ProfileRegistryLifecycleStatus,
) -> &'static str {
    match status {
        ProfileRegistryLifecycleStatus::Active => "active",
        ProfileRegistryLifecycleStatus::Paused => "paused",
        ProfileRegistryLifecycleStatus::Decommissioned => "decommissioned",
        ProfileRegistryLifecycleStatus::Archived => "archived",
    }
}

fn profile_registry_lifecycle_status_from_str(
    raw: &str,
) -> rusqlite::Result<ProfileRegistryLifecycleStatus> {
    match raw {
        "active" => Ok(ProfileRegistryLifecycleStatus::Active),
        "paused" => Ok(ProfileRegistryLifecycleStatus::Paused),
        "decommissioned" => Ok(ProfileRegistryLifecycleStatus::Decommissioned),
        "archived" => Ok(ProfileRegistryLifecycleStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown profile registry lifecycle status {other}"),
            )),
        )),
    }
}

fn model_provider_status_as_str(status: &ModelProviderStatus) -> &'static str {
    match status {
        ModelProviderStatus::Active => "active",
        ModelProviderStatus::Disabled => "disabled",
        ModelProviderStatus::Archived => "archived",
    }
}

fn model_provider_status_from_str(raw: &str) -> rusqlite::Result<ModelProviderStatus> {
    match raw {
        "active" => Ok(ModelProviderStatus::Active),
        "disabled" => Ok(ModelProviderStatus::Disabled),
        "archived" => Ok(ModelProviderStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown model provider status {other}"),
            )),
        )),
    }
}

fn model_provider_protocol_as_str(protocol: &ModelProviderProtocol) -> &'static str {
    match protocol {
        ModelProviderProtocol::Responses => "responses",
        ModelProviderProtocol::ChatCompletions => "chat_completions",
    }
}

fn model_provider_protocol_from_str(raw: &str) -> rusqlite::Result<ModelProviderProtocol> {
    match raw {
        "responses" => Ok(ModelProviderProtocol::Responses),
        "chat_completions" => Ok(ModelProviderProtocol::ChatCompletions),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown model provider protocol {other}"),
            )),
        )),
    }
}

fn responses_provider_dialect_as_str(dialect: ResponsesProviderDialect) -> &'static str {
    match dialect {
        ResponsesProviderDialect::OpenaiStateful => "openai_stateful",
        ResponsesProviderDialect::OpenaiStateless => "openai_stateless",
        ResponsesProviderDialect::GenericStateless => "generic_stateless",
        ResponsesProviderDialect::Deepseek => "deepseek",
    }
}

fn responses_provider_dialect_from_str(raw: &str) -> rusqlite::Result<ResponsesProviderDialect> {
    match raw {
        "openai_stateful" => Ok(ResponsesProviderDialect::OpenaiStateful),
        "openai_stateless" => Ok(ResponsesProviderDialect::OpenaiStateless),
        "generic_stateless" => Ok(ResponsesProviderDialect::GenericStateless),
        "deepseek" => Ok(ResponsesProviderDialect::Deepseek),
        other => Err(model_provider_text_conversion_error(
            13,
            format!("unknown Responses provider dialect {other}"),
        )),
    }
}

fn chat_completions_dialect_as_str(dialect: ChatCompletionsWireDialect) -> &'static str {
    match dialect {
        ChatCompletionsWireDialect::Standard => "standard",
        ChatCompletionsWireDialect::Kimi => "kimi",
        ChatCompletionsWireDialect::Glm => "glm",
        ChatCompletionsWireDialect::Qwen => "qwen",
        ChatCompletionsWireDialect::Deepseek => "deepseek",
    }
}

fn chat_completions_dialect_from_str(raw: &str) -> rusqlite::Result<ChatCompletionsWireDialect> {
    match raw {
        "standard" => Ok(ChatCompletionsWireDialect::Standard),
        "kimi" => Ok(ChatCompletionsWireDialect::Kimi),
        "glm" => Ok(ChatCompletionsWireDialect::Glm),
        "qwen" => Ok(ChatCompletionsWireDialect::Qwen),
        "deepseek" => Ok(ChatCompletionsWireDialect::Deepseek),
        other => Err(model_provider_text_conversion_error(
            13,
            format!("unknown chat completions dialect {other}"),
        )),
    }
}

fn chat_completions_thinking_mode_as_str(mode: ChatCompletionsThinkingMode) -> &'static str {
    match mode {
        ChatCompletionsThinkingMode::ProviderDefault => "provider_default",
        ChatCompletionsThinkingMode::Enabled => "enabled",
        ChatCompletionsThinkingMode::Disabled => "disabled",
    }
}

fn chat_completions_thinking_mode_from_str(
    raw: &str,
) -> rusqlite::Result<ChatCompletionsThinkingMode> {
    match raw {
        "provider_default" => Ok(ChatCompletionsThinkingMode::ProviderDefault),
        "enabled" => Ok(ChatCompletionsThinkingMode::Enabled),
        "disabled" => Ok(ChatCompletionsThinkingMode::Disabled),
        other => Err(model_provider_text_conversion_error(
            14,
            format!("unknown chat completions thinking mode {other}"),
        )),
    }
}

fn chat_completions_reasoning_history_as_str(
    history: ChatCompletionsReasoningHistory,
) -> &'static str {
    match history {
        ChatCompletionsReasoningHistory::ProviderDefault => "provider_default",
        ChatCompletionsReasoningHistory::Discard => "discard",
        ChatCompletionsReasoningHistory::PreserveAll => "preserve_all",
        ChatCompletionsReasoningHistory::ToolCallsOnly => "tool_calls_only",
    }
}

fn chat_completions_reasoning_history_from_str(
    raw: &str,
) -> rusqlite::Result<ChatCompletionsReasoningHistory> {
    match raw {
        "provider_default" => Ok(ChatCompletionsReasoningHistory::ProviderDefault),
        "discard" => Ok(ChatCompletionsReasoningHistory::Discard),
        "preserve_all" => Ok(ChatCompletionsReasoningHistory::PreserveAll),
        "tool_calls_only" => Ok(ChatCompletionsReasoningHistory::ToolCallsOnly),
        other => Err(model_provider_text_conversion_error(
            15,
            format!("unknown chat completions reasoning history {other}"),
        )),
    }
}

fn chat_completions_prompt_caching_as_str(
    policy: ChatCompletionsPromptCachingPolicy,
) -> &'static str {
    match policy {
        ChatCompletionsPromptCachingPolicy::Disabled => "disabled",
        ChatCompletionsPromptCachingPolicy::Automatic5m => "automatic_5m",
        ChatCompletionsPromptCachingPolicy::Automatic1h => "automatic_1h",
    }
}

fn chat_completions_prompt_caching_from_str(
    raw: &str,
) -> rusqlite::Result<ChatCompletionsPromptCachingPolicy> {
    match raw {
        "disabled" => Ok(ChatCompletionsPromptCachingPolicy::Disabled),
        "automatic_5m" => Ok(ChatCompletionsPromptCachingPolicy::Automatic5m),
        "automatic_1h" => Ok(ChatCompletionsPromptCachingPolicy::Automatic1h),
        other => Err(model_provider_text_conversion_error(
            17,
            format!("unknown chat completions prompt caching policy {other}"),
        )),
    }
}

fn model_provider_text_conversion_error(column: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(CoreError::new(CoreErrorKind::PersistenceFailure, message)),
    )
}

fn model_provider_credential_kind_as_str(kind: ModelProviderCredentialKind) -> &'static str {
    match kind {
        ModelProviderCredentialKind::ApiKey | ModelProviderCredentialKind::LegacyRawApiKey => {
            "api_key"
        }
        ModelProviderCredentialKind::OpenAiOauth => "openai_oauth",
    }
}

fn model_provider_credential_kind_from_str(
    raw: &str,
) -> rusqlite::Result<ModelProviderCredentialKind> {
    match raw {
        "api_key" => Ok(ModelProviderCredentialKind::ApiKey),
        "openai_oauth" => Ok(ModelProviderCredentialKind::OpenAiOauth),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown service credential kind {other}"),
            )),
        )),
    }
}

fn validate_expected_revision(
    label: &str,
    id: &str,
    expected: Option<u64>,
    found: u64,
) -> CoreResult<()> {
    if expected.is_none() || expected == Some(found) {
        return Ok(());
    }
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!(
            "{label} {id} revision mismatch: expected {}, found {found}",
            expected.unwrap_or_default()
        ),
    ))
}

fn validate_provider_credential_compatibility(
    provider_kind: &str,
    protocol: ModelProviderProtocol,
    credential: &ServiceCredentialRecord,
) -> CoreResult<()> {
    validate_provider_credential_fields(
        provider_kind,
        protocol,
        &credential.provider_kind,
        credential.credential_kind,
        &credential.credential_id,
    )
}

fn validate_provider_credential_fields(
    provider_kind: &str,
    protocol: ModelProviderProtocol,
    credential_provider_kind: &str,
    credential_kind: ModelProviderCredentialKind,
    credential_id: &str,
) -> CoreResult<()> {
    if provider_kind != credential_provider_kind {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "model provider kind {provider_kind} cannot use {} credential {}",
                credential_provider_kind, credential_id
            ),
        ));
    }
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

pub(crate) fn validate_profile_registry_write(write: &ProfileRegistryWrite) -> CoreResult<()> {
    validate_profile_registry_id(&write.profile_id)?;
    validate_optional_short_text(
        "profile registry display_name",
        write.display_name.as_deref(),
    )?;
    validate_optional_short_text("profile registry summary", write.summary.as_deref())?;
    if let Some(agent_id) = &write.agent_id {
        validate_registry_id_text("profile registry agent_id", &agent_id.0)?;
    }
    validate_optional_short_text("profile registry owner_id", write.owner_id.as_deref())?;
    for asset in &write.source_asset_refs {
        validate_registry_id_text("profile registry source asset kind", &asset.asset_kind)?;
        if asset.path.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile registry source asset path must be non-empty",
            ));
        }
    }
    for derived in &write.derived_runtime_refs {
        validate_registry_id_text("profile registry derived ref kind", &derived.ref_kind)?;
        if derived.ref_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile registry derived runtime ref id must be non-empty",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_model_provider_write(write: &ModelProviderWrite) -> CoreResult<()> {
    validate_model_provider_alias(&write.alias)?;
    validate_registry_id_text("model provider provider_kind", &write.provider_kind)?;
    collect_required_text("model provider model_id", &write.model_id)?;
    validate_optional_short_text("model provider display_name", write.display_name.as_deref())?;
    validate_optional_short_text("model provider description", write.description.as_deref())?;
    validate_optional_short_text(
        "model provider reasoning_effort",
        write.reasoning_effort.as_deref(),
    )?;
    validate_optional_short_text(
        "model provider reasoning_format",
        write.reasoning_format.as_deref(),
    )?;
    if let Some(base_url) = write.base_url.as_deref() {
        collect_required_text("model provider base_url", base_url)?;
    }
    validate_chat_completions_dialect_policy(write)?;
    validate_chat_completions_prompt_caching_policy(write)?;
    validate_responses_provider_dialect(write)?;
    if write.clear_secret && write.secret.is_some() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "model provider write cannot set and clear secret in one request",
        ));
    }
    Ok(())
}

fn validate_responses_provider_dialect(write: &ModelProviderWrite) -> CoreResult<()> {
    match (write.protocol, write.responses_dialect) {
        (ModelProviderProtocol::Responses, Some(_))
        | (ModelProviderProtocol::ChatCompletions, None) => Ok(()),
        (ModelProviderProtocol::Responses, None) => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "responses protocol requires an explicit responses_dialect",
        )),
        (ModelProviderProtocol::ChatCompletions, Some(_)) => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "responses_dialect requires protocol responses",
        )),
    }
}

fn validate_chat_completions_prompt_caching_policy(write: &ModelProviderWrite) -> CoreResult<()> {
    if write.prompt_caching == ChatCompletionsPromptCachingPolicy::Disabled {
        return Ok(());
    }
    if write.protocol != ModelProviderProtocol::ChatCompletions {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "prompt caching policy requires protocol chat_completions",
        ));
    }
    if write.provider_kind != "openrouter" {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "automatic prompt caching requires provider_kind openrouter",
        ));
    }
    if !write.model_id.starts_with("anthropic/") {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "automatic OpenRouter prompt caching requires an anthropic/ model id",
        ));
    }
    Ok(())
}

fn validate_chat_completions_dialect_policy(write: &ModelProviderWrite) -> CoreResult<()> {
    const KIMI_THINKING_MIN_OUTPUT_TOKENS: u32 = 16_000;
    let is_default = write.chat_completions_dialect == ChatCompletionsWireDialect::Standard
        && write.thinking_mode == ChatCompletionsThinkingMode::ProviderDefault
        && write.reasoning_history == ChatCompletionsReasoningHistory::ProviderDefault
        && write.reasoning_budget_tokens.is_none();
    if write.protocol != ModelProviderProtocol::ChatCompletions {
        if is_default {
            return Ok(());
        }
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "chat completions dialect settings require protocol chat_completions",
        ));
    }
    if write.reasoning_history == ChatCompletionsReasoningHistory::ToolCallsOnly
        && write.chat_completions_dialect != ChatCompletionsWireDialect::Deepseek
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "tool_calls_only reasoning history requires the deepseek chat completions dialect",
        ));
    }
    if write.chat_completions_dialect == ChatCompletionsWireDialect::Standard && !is_default {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "standard chat completions dialect does not accept vendor thinking settings",
        ));
    }
    if write.thinking_mode == ChatCompletionsThinkingMode::Disabled
        && write.reasoning_history != ChatCompletionsReasoningHistory::ProviderDefault
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "disabled thinking cannot configure reasoning history preservation",
        ));
    }
    if write.chat_completions_dialect == ChatCompletionsWireDialect::Kimi
        && write.thinking_mode != ChatCompletionsThinkingMode::Disabled
    {
        if write.temperature_milli.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "kimi thinking models do not accept a temperature override",
            ));
        }
        if !matches!(
            write.max_output_tokens,
            Some(tokens) if tokens >= KIMI_THINKING_MIN_OUTPUT_TOKENS
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "kimi thinking models require max_output_tokens of at least 16000",
            ));
        }
    }
    if let Some(budget) = write.reasoning_budget_tokens {
        if budget == 0 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "reasoning budget tokens must be greater than zero",
            ));
        }
        if write.chat_completions_dialect != ChatCompletionsWireDialect::Qwen {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "reasoning budget tokens are supported only by the qwen chat completions dialect",
            ));
        }
        if write.thinking_mode != ChatCompletionsThinkingMode::Enabled {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "qwen reasoning budget tokens require thinking mode enabled",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_service_credential_write(write: &ServiceCredentialWrite) -> CoreResult<()> {
    validate_service_credential_id(&write.credential_id)?;
    validate_optional_short_text(
        "service credential display_name",
        Some(write.display_name.as_str()),
    )?;
    collect_required_text("service credential display_name", &write.display_name)?;
    validate_registry_id_text("service credential provider_kind", &write.provider_kind)?;
    if write.clear_secret && write.secret.is_some() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "service credential write cannot set and clear secret in one request",
        ));
    }
    Ok(())
}

pub(crate) fn validate_service_credential_id(credential_id: &str) -> CoreResult<()> {
    if credential_id.is_empty() || credential_id.len() > 128 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "service credential id must be 1-128 characters",
        ));
    }
    if !credential_id.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '-' | '_' | ':' | '.')
    }) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "service credential id must use lowercase ASCII id characters",
        ));
    }
    Ok(())
}

pub(crate) fn validate_model_provider_alias(alias: &str) -> CoreResult<()> {
    if alias.is_empty() || alias.len() > 128 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "model provider alias must be 1-128 characters",
        ));
    }
    if !alias.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
            || character == ':'
            || character == '.'
    }) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "model provider alias must use lowercase ASCII alias characters",
        ));
    }
    Ok(())
}

pub(crate) fn validate_profile_registry_id(profile_id: &ProfileId) -> CoreResult<()> {
    validate_registry_id_text("profile registry profile_id", &profile_id.0)
}

fn validate_registry_id_text(label: &str, value: &str) -> CoreResult<()> {
    if value.is_empty() || value.len() > 128 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be 1-128 characters"),
        ));
    }
    if !value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
            || character == ':'
    }) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must use lowercase ASCII id characters"),
        ));
    }
    Ok(())
}

fn collect_required_text(context: &str, value: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{context} must be non-empty"),
        ));
    }
    Ok(())
}

fn validate_optional_short_text(label: &str, value: Option<&str>) -> CoreResult<()> {
    if let Some(value) = value {
        if value.len() > 512 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("{label} must be at most 512 bytes"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        ProfileRegistryImportExportMetadata, ProfileRegistrySourceAssetRef,
    };
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn model_provider_alias_accepts_version_periods_without_relaxing_registry_ids() {
        validate_model_provider_alias("gpt-5.4").unwrap();
        validate_model_provider_alias("openai:gpt-5.4-mini").unwrap();

        assert!(validate_model_provider_alias("GPT-5.4").is_err());
        assert!(validate_model_provider_alias("openai/gpt-5.4").is_err());
        assert!(validate_profile_registry_id(&ProfileId::new("runner.profile")).is_err());
    }

    #[test]
    fn deepseek_tool_call_reasoning_policy_validates_and_round_trips() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-deepseek-policy-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let mut write = model_provider_write("deepseek-v4-pro", None, None, "2026-07-20T00:00:00Z");
        write.model_id = "deepseek-v4-pro".to_string();
        write.temperature_milli = None;
        write.chat_completions_dialect = ChatCompletionsWireDialect::Deepseek;
        write.thinking_mode = ChatCompletionsThinkingMode::Enabled;
        write.reasoning_history = ChatCompletionsReasoningHistory::ToolCallsOnly;

        validate_model_provider_write(&write).unwrap();
        let created = store.upsert_model_provider(&write).unwrap();
        assert_eq!(
            created.chat_completions_dialect,
            ChatCompletionsWireDialect::Deepseek
        );
        assert_eq!(
            created.reasoning_history,
            ChatCompletionsReasoningHistory::ToolCallsOnly
        );
        let read = store
            .get_model_provider("deepseek-v4-pro")
            .unwrap()
            .expect("provider");
        assert_eq!(read, created);

        write.chat_completions_dialect = ChatCompletionsWireDialect::Standard;
        let error = validate_model_provider_write(&write).expect_err("invalid standard policy");
        assert!(error.message.contains("requires the deepseek"));
        drop(store);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn responses_provider_dialect_is_required_and_protocol_scoped() {
        let mut write =
            model_provider_write("deepseek-responses", None, None, "2026-08-02T00:00:00Z");
        write.protocol = ModelProviderProtocol::Responses;
        write.provider_kind = "deepseek".to_string();
        write.model_id = "deepseek-v4-flash".to_string();

        let missing = validate_model_provider_write(&write)
            .expect_err("Responses providers require an explicit dialect");
        assert_eq!(missing.kind, CoreErrorKind::InvalidInput);
        assert!(missing.message.contains("explicit responses_dialect"));

        write.responses_dialect = Some(ResponsesProviderDialect::Deepseek);
        validate_model_provider_write(&write).unwrap();

        write.protocol = ModelProviderProtocol::ChatCompletions;
        let misplaced = validate_model_provider_write(&write)
            .expect_err("Chat Completions providers reject a Responses dialect");
        assert_eq!(misplaced.kind, CoreErrorKind::InvalidInput);
        assert!(misplaced.message.contains("requires protocol responses"));
    }

    #[test]
    fn service_config_repo_preserves_profile_provider_and_binding_contracts() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-service-config-repo-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();

        let created = store
            .create_profile_registry_record(&profile_write(
                "runner-profile",
                "2026-07-02T00:00:00Z",
            ))
            .unwrap();
        let updated = store
            .update_profile_registry_record(&ProfileRegistryUpdate {
                write: ProfileRegistryWrite {
                    summary: Some("updated runner profile".to_string()),
                    now: "2026-07-02T00:01:00Z".to_string(),
                    ..profile_write("runner-profile", "2026-07-02T00:00:00Z")
                },
                expected_revision: created.revision,
            })
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.summary.as_deref(), Some("updated runner profile"));

        let first_provider = store
            .upsert_model_provider(&model_provider_write(
                "deepseek-v3.1-flash",
                Some(
                    ModelProviderSecretEnvelope::api_key("secret-one")
                        .to_storage_text()
                        .unwrap(),
                ),
                None,
                "2026-07-02T00:00:00Z",
            ))
            .unwrap();
        let preserved_secret = store
            .upsert_model_provider(&model_provider_write(
                "deepseek-v3.1-flash",
                None,
                Some(first_provider.revision),
                "2026-07-02T00:02:00Z",
            ))
            .unwrap();
        assert_eq!(preserved_secret.revision, 2);
        assert_eq!(
            store
                .get_model_provider_secret("deepseek-v3.1-flash")
                .unwrap(),
            Some(
                ModelProviderSecretEnvelope::api_key("secret-one")
                    .to_storage_text()
                    .unwrap()
            )
        );

        store.save_channel_binding(&channel_binding()).unwrap();
        store.save_mcp_binding(&mcp_binding()).unwrap();

        let channels = store
            .query_channel_bindings(&ChannelBindingQuery {
                profile_id: Some(ProfileId::new("runner-profile")),
                provider: Some("den_channels".to_string()),
                ..ChannelBindingQuery::default()
            })
            .unwrap();
        let mcps = store
            .query_mcp_bindings(&McpBindingQuery {
                profile_id: Some(ProfileId::new("runner-profile")),
                status: Some(ExternalBindingStatus::Active),
                ..McpBindingQuery::default()
            })
            .unwrap();

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].external_channel_id, "rusty-crew");
        assert_eq!(mcps.len(), 1);
        assert_eq!(mcps[0].server_names, vec!["den".to_string()]);
        assert!(!mcps[0].endpoint_ref.contains("secret"));

        store
            .create_chat_message_slot(&chat_ingest_request("session-runner", "profile-purge"))
            .unwrap();
        let purge = store
            .purge_profile(&ProfileId::new("runner-profile"))
            .unwrap();
        assert!(purge.table_counts.iter().any(|count| {
            count.table == "chat_message_ingest_receipts" && count.rows_deleted == 1
        }));
        assert_eq!(
            store
                .purge_chat_message_ingest_receipts_for_session(&SessionId::new("session-runner"))
                .unwrap(),
            0
        );

        drop(store);
        let _ = std::fs::remove_file(db_path);
    }

    fn profile_write(profile_id: &str, now: &str) -> ProfileRegistryWrite {
        ProfileRegistryWrite {
            profile_id: ProfileId::new(profile_id),
            lifecycle_status: ProfileRegistryLifecycleStatus::Active,
            display_name: Some("Runner Profile".to_string()),
            summary: Some("service config repo test profile".to_string()),
            default_session_kind: Some(SessionKind::Full),
            agent_id: Some(AgentId::new("runner-agent")),
            owner_id: Some("owner-alpha".to_string()),
            prompt_soul_markdown: Some("# Soul".to_string()),
            prompt_memory_markdown: Some("# Memory".to_string()),
            active_runtime_settings_json: json!({"provider_alias": "deepseek-flash"}),
            source_asset_refs: vec![ProfileRegistrySourceAssetRef {
                asset_kind: "profile_dir".to_string(),
                path: "/home/system/rusty-crew/config/profiles/runner-profile".to_string(),
                content_hash: Some("sha256:test-profile".to_string()),
                last_seen_at: Some(now.to_string()),
                metadata_json: json!({}),
            }],
            derived_runtime_refs: Vec::new(),
            import_export: ProfileRegistryImportExportMetadata {
                imported_from: None,
                imported_at: None,
                exported_to: None,
                exported_at: None,
                metadata_json: json!({}),
            },
            now: now.to_string(),
        }
    }

    fn chat_ingest_request(session_id: &str, key: &str) -> CreateChatMessageSlotRequest {
        let branch_id = ConversationBranchId::new(format!("branch:{session_id}:default"));
        let slot_id = MessageSlotId::new(format!("slot:{session_id}:{key}"));
        let variant_id = MessageVariantId::new(format!("variant:{session_id}:{key}"));
        let now = "2026-07-02T00:03:00Z".to_string();
        CreateChatMessageSlotRequest {
            slot: MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: SessionId::new(session_id),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({}),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            primary_variant: MessageVariantWrite {
                variant_id,
                slot_id,
                source: MessageVariantSource::Primary,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message: DurableMessageWrite {
                    message_id: MessageId::new(format!("message:{session_id}:{key}")),
                    session_id: SessionId::new(session_id),
                    branch_id: Some(branch_id.clone()),
                    parent_message_id: None,
                    previous_message_id: None,
                    author_id: "user".to_string(),
                    author_role: "user".to_string(),
                    status: DurableMessageStatus::Completed,
                    body: "profile purge receipt".to_string(),
                    metadata_json: json!({}),
                    created_at: now.clone(),
                    blocks: Vec::new(),
                },
                metadata_json: json!({}),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            branch_id: branch_id.clone(),
            expected_branch_head: BranchHeadExpectation::Any,
            updated_at: now.clone(),
            ensure_active_branch: Some(EnsureActiveChatConversationBranchRequest {
                session_id: SessionId::new(session_id),
                branch_id,
                label: Some("Default".to_string()),
                metadata_json: json!({}),
                created_at: now.clone(),
                updated_at: now,
            }),
            inherit_branch_head: true,
            idempotency_key: Some(key.to_string()),
        }
    }

    fn model_provider_write(
        alias: &str,
        secret: Option<String>,
        expected_revision: Option<u64>,
        now: &str,
    ) -> ModelProviderWrite {
        ModelProviderWrite {
            alias: alias.to_string(),
            status: ModelProviderStatus::Active,
            protocol: ModelProviderProtocol::ChatCompletions,
            provider_kind: "openai-compatible".to_string(),
            display_name: Some("DeepSeek Flash".to_string()),
            description: None,
            base_url: Some("http://127.0.0.1:18082/v1".to_string()),
            model_id: "deepseek-flash".to_string(),
            context_window_tokens: Some(65_536),
            max_output_tokens: Some(4096),
            temperature_milli: Some(500),
            reasoning_effort: None,
            reasoning_format: None,
            responses_dialect: None,
            chat_completions_dialect: Default::default(),
            thinking_mode: Default::default(),
            reasoning_history: Default::default(),
            reasoning_budget_tokens: None,
            prompt_caching: Default::default(),
            secret,
            clear_secret: false,
            expected_credential_revision: None,
            metadata_json: json!({"source": "service_config_repo_test"}),
            expected_revision,
            now: now.to_string(),
        }
    }

    fn channel_binding() -> ChannelBindingRecord {
        ChannelBindingRecord {
            binding_id: "channel-runner".to_string(),
            adapter_id: AdapterId::new("gateway"),
            provider: "den_channels".to_string(),
            agent_id: AgentId::new("runner-agent"),
            instance_id: Some(AgentInstanceId::new("instance:runner")),
            session_id: Some(SessionId::new("session-runner")),
            profile_id: ProfileId::new("runner-profile"),
            external_channel_id: "rusty-crew".to_string(),
            external_thread_id: None,
            external_user_id: None,
            provider_subscription_id: Some("sub-runner".to_string()),
            cursor: Some("cursor-1".to_string()),
            membership_state: Some("joined".to_string()),
            presence_state: Some("online".to_string()),
            status: ExternalBindingStatus::Active,
            degraded_reason: None,
            provenance: ExternalBindingProvenance {
                source_system: Some("den-web".to_string()),
                source_ref: Some("channel:rusty-crew".to_string()),
                externally_owned: true,
                notes: None,
            },
            created_at: "2026-07-02T00:00:00Z".to_string(),
            updated_at: "2026-07-02T00:00:00Z".to_string(),
        }
    }

    fn mcp_binding() -> McpBindingRecord {
        McpBindingRecord {
            binding_id: "mcp-runner".to_string(),
            adapter_id: AdapterId::new("mcp-adapter"),
            agent_id: AgentId::new("runner-agent"),
            instance_id: Some(AgentInstanceId::new("instance:runner")),
            session_id: Some(SessionId::new("session-runner")),
            profile_id: ProfileId::new("runner-profile"),
            server_names: vec!["den".to_string()],
            endpoint_ref: "config://mcp/den".to_string(),
            transport: "http".to_string(),
            tool_profile_key: "planner".to_string(),
            discovered_tool_revision: Some("rev-1".to_string()),
            status: ExternalBindingStatus::Active,
            degraded_reason: None,
            diagnostics: McpBindingDiagnostics {
                last_error: None,
                last_checked_at: Some("2026-07-02T00:00:00Z".to_string()),
                notes: Some("ready".to_string()),
            },
            created_at: "2026-07-02T00:00:00Z".to_string(),
            updated_at: "2026-07-02T00:00:00Z".to_string(),
        }
    }
}
