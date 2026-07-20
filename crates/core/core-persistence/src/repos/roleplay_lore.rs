use super::super::*;

impl CoordinationStore {
    pub fn roleplay_lore_memory_space_descriptor(&self) -> MemorySpaceDescriptor {
        roleplay_lore_memory_space_descriptor()
    }
    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start add roleplay lore record", error))?;
        if get_roleplay_lore_record_in_tx(&tx, &write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("roleplay lore record {} already exists", write.record_id),
            ));
        }
        insert_roleplay_lore_record_in_tx(&tx, write)?;
        insert_roleplay_lore_provenance_event_in_tx(
            &tx,
            &RoleplayLoreProvenanceEvent {
                event_id: format!("{}:created", write.record_id),
                record_id: write.record_id.clone(),
                world_id: write.world_id.clone(),
                evidence_refs: write.evidence_refs.clone(),
                source: write.source,
                actor: memory_proposal_source_as_str(write.source).to_string(),
                note: Some("created roleplay lore record".to_string()),
                created_at: write.now.clone(),
            },
        )?;
        let record = get_roleplay_lore_record_in_tx(&tx, &write.record_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "created roleplay lore record was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit add roleplay lore record", error))?;
        Ok(record)
    }
    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_write(&replace.write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start replace roleplay lore record", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &tx,
            &replace.write.record_id,
            replace.expected_revision,
        )?;
        update_roleplay_lore_record_content_in_tx(&tx, replace, existing.revision + 1)?;
        insert_roleplay_lore_provenance_event_in_tx(
            &tx,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:revision:{}",
                    replace.write.record_id,
                    existing.revision + 1
                ),
                record_id: replace.write.record_id.clone(),
                world_id: replace.write.world_id.clone(),
                evidence_refs: replace.write.evidence_refs.clone(),
                source: replace.write.source,
                actor: memory_proposal_source_as_str(replace.write.source).to_string(),
                note: Some("replaced roleplay lore record".to_string()),
                created_at: replace.write.now.clone(),
            },
        )?;
        let record =
            get_roleplay_lore_record_in_tx(&tx, &replace.write.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replaced roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit replace roleplay lore record", error))?;
        Ok(record)
    }
    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        validate_roleplay_lore_write(&supersede.replacement)?;
        if supersede.replacement.supersedes_record_id.as_deref()
            != Some(supersede.record_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore replacement must reference the superseded record",
            ));
        }
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start supersede roleplay lore record", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &tx,
            &supersede.record_id,
            supersede.expected_revision,
        )?;
        if existing.world_id != supersede.replacement.world_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore replacement must stay in the same world",
            ));
        }
        if get_roleplay_lore_record_in_tx(&tx, &supersede.replacement.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore replacement {} already exists",
                    supersede.replacement.record_id
                ),
            ));
        }
        insert_roleplay_lore_record_in_tx(&tx, &supersede.replacement)?;
        mark_roleplay_lore_superseded_in_tx(
            &tx,
            &existing.record_id,
            &supersede.replacement.record_id,
            existing.revision + 1,
            &supersede.replacement.now,
        )?;
        insert_roleplay_lore_provenance_event_in_tx(
            &tx,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:superseded_by:{}",
                    existing.record_id, supersede.replacement.record_id
                ),
                record_id: existing.record_id.clone(),
                world_id: existing.world_id.clone(),
                evidence_refs: supersede.replacement.evidence_refs.clone(),
                source: supersede.replacement.source,
                actor: memory_proposal_source_as_str(supersede.replacement.source).to_string(),
                note: Some(format!("superseded by {}", supersede.replacement.record_id)),
                created_at: supersede.replacement.now.clone(),
            },
        )?;
        let old_record =
            get_roleplay_lore_record_in_tx(&tx, &existing.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "superseded roleplay lore record was not readable",
                )
            })?;
        let new_record = get_roleplay_lore_record_in_tx(&tx, &supersede.replacement.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replacement roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit supersede roleplay lore record", error))?;
        Ok((old_record, new_record))
    }
    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_record_id(&tombstone.record_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start tombstone roleplay lore record", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &tx,
            &tombstone.record_id,
            tombstone.expected_revision,
        )?;
        tombstone_roleplay_lore_record_in_tx(&tx, tombstone, existing.revision + 1)?;
        insert_roleplay_lore_provenance_event_in_tx(
            &tx,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:tombstoned:{}",
                    tombstone.record_id,
                    existing.revision + 1
                ),
                record_id: tombstone.record_id.clone(),
                world_id: existing.world_id,
                evidence_refs: existing.evidence_refs,
                source: existing.source,
                actor: "rusty_crew_storage".to_string(),
                note: tombstone.reason.clone(),
                created_at: tombstone.now.clone(),
            },
        )?;
        let record =
            get_roleplay_lore_record_in_tx(&tx, &tombstone.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "tombstoned roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit tombstone roleplay lore record", error))?;
        Ok(record)
    }
    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        let conn = self.conn()?;
        query_roleplay_lore_records(&conn, query)
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        validate_roleplay_lore_record_id(record_id)?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT record_id,
                    world_id,
                    entity_id,
                    session_id,
                    branch_id,
                    shape_id,
                    shape_version,
                    canon_status,
                    visibility,
                    status,
                    revision,
                    title,
                    body,
                    content_json,
                    evidence_refs_json,
                    source,
                    confidence,
                    durability_rationale,
                    supersedes_record_id,
                    superseded_by_record_id,
                    tombstoned_at,
                    tombstone_reason,
                    created_at,
                    updated_at
             FROM module_roleplay_lore_records
             WHERE record_id = ?1",
            params![record_id],
            row_to_roleplay_lore_record,
        )
        .optional()
        .map_err(|error| persistence_error("get roleplay lore record", error))
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        validate_roleplay_lore_record_id(record_id)?;
        let conn = self.conn()?;
        roleplay_lore_provenance_events(&conn, record_id)
    }
    pub fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        validate_roleplay_lore_layer_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start create roleplay lore layer", error))?;
        if get_lore_layer_in_tx(&tx, &write.layer_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("roleplay lore layer {} already exists", write.layer_id),
            ));
        }
        tx.execute(
            "INSERT INTO module_roleplay_lore_layers (
                layer_id,
                profile_id,
                name,
                description,
                purpose,
                write_policy,
                is_archived,
                created_at,
                updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
            params![
                write.layer_id.as_str(),
                write.profile_id.as_str(),
                write.name.as_str(),
                normalized_optional_text(write.description.as_deref()).as_deref(),
                roleplay_lore_layer_purpose_as_str(write.purpose),
                roleplay_lore_layer_write_policy_as_str(write.write_policy),
                write.now.as_str(),
            ],
        )
        .map_err(|error| persistence_error("insert roleplay lore layer", error))?;
        let layer = get_lore_layer_in_tx(&tx, &write.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "created roleplay lore layer was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit create roleplay lore layer", error))?;
        Ok(layer)
    }
    pub fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let conn = self.conn()?;
        get_lore_layer(&conn, layer_id)
    }
    pub fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        validate_roleplay_lore_identifier("roleplay lore profile_id", profile_id)?;
        let conn = self.conn()?;
        list_lore_layers_by_profile(&conn, profile_id)
    }
    pub fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        validate_roleplay_lore_layer_update(update)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update roleplay lore layer", error))?;
        let mut existing = get_lore_layer_in_tx(&tx, &update.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", update.layer_id),
            )
        })?;
        if let Some(name) = &update.name {
            existing.name = name.trim().to_string();
        }
        if let Some(description) = &update.description {
            existing.description = normalized_optional_text(description.as_deref());
        }
        if let Some(purpose) = update.purpose {
            existing.purpose = purpose;
        }
        if let Some(write_policy) = update.write_policy {
            existing.write_policy = write_policy;
        }
        tx.execute(
            "UPDATE module_roleplay_lore_layers
             SET name = ?2,
                 description = ?3,
                 purpose = ?4,
                 write_policy = ?5,
                 updated_at = ?6
             WHERE layer_id = ?1",
            params![
                update.layer_id.as_str(),
                existing.name.as_str(),
                existing.description.as_deref(),
                roleplay_lore_layer_purpose_as_str(existing.purpose),
                roleplay_lore_layer_write_policy_as_str(existing.write_policy),
                update.now.as_str(),
            ],
        )
        .map_err(|error| persistence_error("update roleplay lore layer", error))?;
        let layer = get_lore_layer_in_tx(&tx, &update.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "updated roleplay lore layer was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit update roleplay lore layer", error))?;
        Ok(layer)
    }
    pub fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", &archive.layer_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start archive roleplay lore layer", error))?;
        if get_lore_layer_in_tx(&tx, &archive.layer_id)?.is_none() {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", archive.layer_id),
            ));
        }
        tx.execute(
            "UPDATE module_roleplay_lore_layers
             SET is_archived = 1,
                 updated_at = ?2
             WHERE layer_id = ?1",
            params![archive.layer_id.as_str(), archive.now.as_str()],
        )
        .map_err(|error| persistence_error("archive roleplay lore layer", error))?;
        let layer = get_lore_layer_in_tx(&tx, &archive.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "archived roleplay lore layer was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit archive roleplay lore layer", error))?;
        Ok(layer)
    }
    pub fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let conn = self.conn()?;
        get_lore_layer_config(&conn, layer_id)
    }
    pub fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        validate_roleplay_lore_layer_config_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start set roleplay lore layer config", error))?;
        if get_lore_layer_in_tx(&tx, &write.layer_id)?.is_none() {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", write.layer_id),
            ));
        }
        let existing = get_lore_layer_config_in_tx(&tx, &write.layer_id)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at.as_str())
            .unwrap_or_else(|| write.now.as_str());
        tx.execute(
            "INSERT INTO module_roleplay_lore_layer_config (
                config_id,
                layer_id,
                fts_weight,
                subject_weight,
                canon_weight,
                tag_boost_weight,
                recency_weight,
                default_token_budget,
                constant_token_reserve,
                min_relevance_score,
                max_constants,
                created_at,
                updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(layer_id) DO UPDATE SET
                config_id = excluded.config_id,
                fts_weight = excluded.fts_weight,
                subject_weight = excluded.subject_weight,
                canon_weight = excluded.canon_weight,
                tag_boost_weight = excluded.tag_boost_weight,
                recency_weight = excluded.recency_weight,
                default_token_budget = excluded.default_token_budget,
                constant_token_reserve = excluded.constant_token_reserve,
                min_relevance_score = excluded.min_relevance_score,
                max_constants = excluded.max_constants,
                updated_at = excluded.updated_at",
            params![
                write.config_id.as_str(),
                write.layer_id.as_str(),
                write.fts_weight as f64,
                write.subject_weight as f64,
                write.canon_weight as f64,
                write.tag_boost_weight as f64,
                write.recency_weight as f64,
                write.default_token_budget as i64,
                write.constant_token_reserve as i64,
                write.min_relevance_score as f64,
                write.max_constants as i64,
                created_at,
                write.now.as_str(),
            ],
        )
        .map_err(|error| persistence_error("upsert roleplay lore layer config", error))?;
        let config = get_lore_layer_config_in_tx(&tx, &write.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "saved roleplay lore layer config was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit set roleplay lore layer config", error))?;
        Ok(config)
    }
    pub fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        validate_roleplay_lore_layer_entry_link(link)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start add roleplay lore entry to layer", error))?;
        require_lore_layer_and_record(&tx, &link.layer_id, &link.record_id)?;
        insert_lore_layer_entry_in_tx(&tx, link)?;
        tx.commit()
            .map_err(|error| persistence_error("commit add roleplay lore entry to layer", error))
    }
    pub fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        validate_roleplay_lore_fact_capture(capture)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start capture roleplay lore fact", error))?;
        let layer = get_lore_layer_in_tx(&tx, &capture.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", capture.layer_id),
            )
        })?;
        if layer.is_archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("roleplay lore layer {} is archived", capture.layer_id),
            ));
        }
        if layer.write_policy != RoleplayLoreLayerWritePolicy::AutoCapture {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore layer {} does not allow auto capture",
                    capture.layer_id
                ),
            ));
        }
        if get_roleplay_lore_record_in_tx(&tx, &capture.write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore record {} already exists",
                    capture.write.record_id
                ),
            ));
        }
        insert_roleplay_lore_record_in_tx(&tx, &capture.write)?;
        insert_lore_layer_entry_in_tx(
            &tx,
            &RoleplayLoreLayerEntryLink {
                layer_id: capture.layer_id.clone(),
                record_id: capture.write.record_id.clone(),
                is_constant: capture.is_constant,
                priority: capture.priority,
                added_at: capture.write.now.clone(),
            },
        )?;
        insert_roleplay_lore_provenance_event_in_tx(
            &tx,
            &RoleplayLoreProvenanceEvent {
                event_id: format!("{}:captured:{}", capture.write.record_id, capture.layer_id),
                record_id: capture.write.record_id.clone(),
                world_id: capture.write.world_id.clone(),
                evidence_refs: capture.write.evidence_refs.clone(),
                source: capture.write.source,
                actor: memory_proposal_source_as_str(capture.write.source).to_string(),
                note: capture
                    .capture_reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| Some("captured roleplay lore fact".to_string())),
                created_at: capture.write.now.clone(),
            },
        )?;
        let entry =
            get_lore_layer_entry_join_in_tx(&tx, &capture.layer_id, &capture.write.record_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "captured roleplay lore layer entry was not readable",
                    )
                })?;
        tx.commit()
            .map_err(|error| persistence_error("commit capture roleplay lore fact", error))?;
        Ok(entry)
    }
    pub fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        validate_roleplay_lore_entry_promotion(promotion)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start promote roleplay lore entry", error))?;
        let source = get_lore_layer_entry_join_in_tx(
            &tx,
            &promotion.source_layer_id,
            &promotion.source_record_id,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "roleplay lore source entry {}/{} not found",
                    promotion.source_layer_id, promotion.source_record_id
                ),
            )
        })?;
        if source.record.status != RoleplayLoreRecordStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore source record {} is not active",
                    promotion.source_record_id
                ),
            ));
        }
        let target = get_lore_layer_in_tx(&tx, &promotion.target_layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "roleplay lore target layer {} not found",
                    promotion.target_layer_id
                ),
            )
        })?;
        if target.is_archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore target layer {} is archived",
                    promotion.target_layer_id
                ),
            ));
        }
        if target.write_policy == RoleplayLoreLayerWritePolicy::Readonly {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore target layer {} is readonly",
                    promotion.target_layer_id
                ),
            ));
        }
        if get_roleplay_lore_record_in_tx(&tx, &promotion.new_record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore promoted record {} already exists",
                    promotion.new_record_id
                ),
            ));
        }
        let promoted = RoleplayLoreWrite {
            record_id: promotion.new_record_id.clone(),
            world_id: source.record.world_id.clone(),
            entity_id: source.record.entity_id.clone(),
            session_id: source.record.session_id.clone(),
            branch_id: source.record.branch_id.clone(),
            shape: source.record.shape.clone(),
            canon_status: source.record.canon_status,
            visibility: source.record.visibility,
            title: source.record.title.clone(),
            body: source.record.body.clone(),
            content: source.record.content.clone(),
            evidence_refs: source.record.evidence_refs.clone(),
            source: source.record.source,
            confidence: source.record.confidence,
            durability_rationale: source.record.durability_rationale.clone(),
            supersedes_record_id: Some(promotion.source_record_id.clone()),
            now: promotion.now.clone(),
        };
        insert_roleplay_lore_record_in_tx(&tx, &promoted)?;
        mark_roleplay_lore_superseded_in_tx(
            &tx,
            &source.record.record_id,
            &promotion.new_record_id,
            source.record.revision + 1,
            &promotion.now,
        )?;
        insert_lore_layer_entry_in_tx(
            &tx,
            &RoleplayLoreLayerEntryLink {
                layer_id: promotion.target_layer_id.clone(),
                record_id: promotion.new_record_id.clone(),
                is_constant: promotion.is_constant,
                priority: promotion.priority,
                added_at: promotion.now.clone(),
            },
        )?;
        insert_roleplay_lore_provenance_event_in_tx(
            &tx,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:promoted_from:{}:{}",
                    promotion.new_record_id, promotion.source_layer_id, promotion.source_record_id
                ),
                record_id: promotion.new_record_id.clone(),
                world_id: source.record.world_id.clone(),
                evidence_refs: source.record.evidence_refs.clone(),
                source: source.record.source,
                actor: "rusty_crew_storage".to_string(),
                note: Some(format!(
                    "promoted from {}:{}",
                    promotion.source_layer_id, promotion.source_record_id
                )),
                created_at: promotion.now.clone(),
            },
        )?;
        let entry = get_lore_layer_entry_join_in_tx(
            &tx,
            &promotion.target_layer_id,
            &promotion.new_record_id,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "promoted roleplay lore layer entry was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit promote roleplay lore entry", error))?;
        Ok(entry)
    }
    pub fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        validate_roleplay_lore_record_id(record_id)?;
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM module_roleplay_lore_layer_entries
             WHERE layer_id = ?1 AND record_id = ?2",
            params![layer_id, record_id],
        )
        .map_err(|error| persistence_error("remove roleplay lore entry from layer", error))?;
        Ok(())
    }
    pub fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        validate_roleplay_lore_record_id(record_id)?;
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE module_roleplay_lore_layer_entries
                 SET is_constant = ?3
                 WHERE layer_id = ?1 AND record_id = ?2",
                params![layer_id, record_id, bool_to_sql(is_constant)],
            )
            .map_err(|error| persistence_error("set roleplay lore entry constant", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer entry {layer_id}/{record_id} not found"),
            ));
        }
        Ok(())
    }
    pub fn list_entries_by_layer(
        &self,
        layer_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let conn = self.conn()?;
        list_entries_by_layer(&conn, layer_id)
    }
    pub fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        validate_roleplay_chat_layers_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start set roleplay chat layers", error))?;
        Self::set_chat_layers_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit set roleplay chat layers", error))
    }
    pub fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
        let conn = self.conn()?;
        get_chat_layers(&conn, chat_id)
    }
    pub(crate) fn set_chat_layers_in_tx(
        tx: &rusqlite::Transaction<'_>,
        write: &RoleplayChatLayersWrite,
    ) -> CoreResult<()> {
        tx.execute(
            "DELETE FROM module_roleplay_chat_layers WHERE chat_id = ?1",
            params![write.chat_id.as_str()],
        )
        .map_err(|error| persistence_error("clear roleplay chat layers", error))?;
        for layer in &write.layers {
            if get_lore_layer_in_tx(tx, &layer.layer_id)?.is_none() {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("roleplay lore layer {} not found", layer.layer_id),
                ));
            }
            tx.execute(
                "INSERT INTO module_roleplay_chat_layers (
                    chat_id,
                    layer_id,
                    priority,
                    enabled,
                    created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    write.chat_id.as_str(),
                    layer.layer_id.as_str(),
                    layer.priority,
                    bool_to_sql(layer.enabled),
                    write.now.as_str(),
                ],
            )
            .map_err(|error| persistence_error("insert roleplay chat layer", error))?;
        }
        Ok(())
    }
    pub fn toggle_chat_layer(
        &self,
        chat_id: &str,
        layer_id: &str,
        enabled: bool,
    ) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE module_roleplay_chat_layers
                 SET enabled = ?3
                 WHERE chat_id = ?1 AND layer_id = ?2",
                params![chat_id, layer_id, bool_to_sql(enabled)],
            )
            .map_err(|error| persistence_error("toggle roleplay chat layer", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay chat layer {chat_id}/{layer_id} not found"),
            ));
        }
        Ok(())
    }
    pub fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
        validate_unique_roleplay_ids("roleplay chat layer_ids", layer_ids)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start reorder roleplay chat layers", error))?;
        let existing = get_chat_layers_in_tx(&tx, chat_id)?;
        if existing.len() != layer_ids.len() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay chat layer reorder must include exactly the existing layers",
            ));
        }
        let existing_ids = existing
            .iter()
            .map(|record| record.layer_id.as_str())
            .collect::<BTreeSet<_>>();
        for layer_id in layer_ids {
            if !existing_ids.contains(layer_id.as_str()) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!("roleplay chat layer {layer_id} is not attached to chat {chat_id}"),
                ));
            }
        }
        for (priority, layer_id) in layer_ids.iter().enumerate() {
            tx.execute(
                "UPDATE module_roleplay_chat_layers
                 SET priority = ?3
                 WHERE chat_id = ?1 AND layer_id = ?2",
                params![chat_id, layer_id.as_str(), priority as i64],
            )
            .map_err(|error| persistence_error("reorder roleplay chat layer", error))?;
        }
        tx.commit()
            .map_err(|error| persistence_error("commit reorder roleplay chat layers", error))
    }
    pub fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        validate_lore_recall_query(query)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start roleplay lore recall", error))?;
        let layers = get_chat_layers_in_tx(&tx, &query.chat_id)?
            .into_iter()
            .filter(|layer| layer.enabled && !layer.layer.is_archived)
            .collect::<Vec<_>>();
        let mut layer_configs = Vec::new();
        for layer in &layers {
            let config = get_lore_layer_config_in_tx(&tx, &layer.layer_id)?
                .unwrap_or_else(|| default_lore_layer_config(&layer.layer_id, &query.now));
            layer_configs.push((layer.clone(), config));
        }

        let token_budget = query.token_budget.unwrap_or_else(|| {
            layer_configs
                .first()
                .map(|(_, config)| config.default_token_budget)
                .unwrap_or(4_000)
        });
        let mut remaining = token_budget;
        let mut entries = Vec::new();
        let mut entry_decisions = Vec::new();
        let mut seen_records = BTreeSet::new();
        let mut entries_considered = 0_u32;

        for (layer, config) in &layer_configs {
            let constants = constant_lore_entries_for_layer(&tx, &layer.layer_id, config)?;
            let mut reserve_remaining = config.constant_token_reserve;
            for mut entry in constants {
                entries_considered += 1;
                if excluded_subject_match(&entry.record, &query.excluded_subjects) {
                    entry_decisions.push(lore_recall_decision(
                        &entry,
                        false,
                        LoreRecallTraceDecisionReason::ExcludedSubject,
                    ));
                    continue;
                }
                entry.token_estimate = estimate_lore_tokens(&entry.record);
                if entry.token_estimate > remaining {
                    entry_decisions.push(lore_recall_decision(
                        &entry,
                        false,
                        LoreRecallTraceDecisionReason::TokenBudgetExceeded,
                    ));
                    continue;
                }
                if entry.token_estimate > reserve_remaining {
                    entry_decisions.push(lore_recall_decision(
                        &entry,
                        false,
                        LoreRecallTraceDecisionReason::ConstantReserveExceeded,
                    ));
                    continue;
                }
                remaining -= entry.token_estimate;
                reserve_remaining -= entry.token_estimate;
                seen_records.insert(entry.record.record_id.clone());
                entry_decisions.push(lore_recall_decision(
                    &entry,
                    true,
                    LoreRecallTraceDecisionReason::Included,
                ));
                entries.push(entry);
            }
        }

        let mut scored = if query
            .query_text
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        {
            scored_lore_entries_for_recall(&tx, query, &layer_configs, &seen_records)?
        } else {
            Vec::new()
        };
        entries_considered += scored.len() as u32;
        scored.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.record.updated_at.cmp(&right.record.updated_at))
                .then_with(|| left.record.record_id.cmp(&right.record.record_id))
        });
        for entry in scored {
            if entry.token_estimate > remaining {
                entry_decisions.push(lore_recall_decision(
                    &entry,
                    false,
                    LoreRecallTraceDecisionReason::TokenBudgetExceeded,
                ));
                continue;
            }
            remaining -= entry.token_estimate;
            entry_decisions.push(lore_recall_decision(
                &entry,
                true,
                LoreRecallTraceDecisionReason::Included,
            ));
            entries.push(entry);
        }

        let tokens_consumed = token_budget.saturating_sub(remaining);
        let trace = if query.record_trace {
            let trace = LoreRecallTraceRecord {
                trace_id: query.trace_id.clone().unwrap_or_else(|| {
                    format!("recall:{}:{}:{}", query.chat_id, query.now, entries.len())
                }),
                session_id: query.session_id.clone(),
                layer_ids: layers.iter().map(|layer| layer.layer_id.clone()).collect(),
                query_text: query.query_text.clone(),
                active_subjects: query.active_subjects.clone(),
                excluded_subjects: query.excluded_subjects.clone(),
                config_snapshot: lore_recall_config_snapshot(&layer_configs),
                entries_considered,
                entries_returned: entries.len() as u32,
                token_budget: Some(token_budget),
                tokens_consumed,
                entry_decisions,
                created_at: query.now.clone(),
            };
            insert_lore_recall_trace_in_tx(&tx, &trace)?;
            Some(trace)
        } else {
            None
        };
        tx.commit()
            .map_err(|error| persistence_error("commit roleplay lore recall", error))?;
        Ok(LoreRecallResult {
            chat_id: query.chat_id.clone(),
            entries,
            entries_considered,
            tokens_consumed,
            token_budget: Some(token_budget),
            trace,
        })
    }
    pub fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        validate_lore_recall_trace_query(query)?;
        let conn = self.conn()?;
        list_lore_recall_traces(&conn, query)
    }
    pub fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        validate_roleplay_lore_identifier("roleplay lore recall trace_id", trace_id)?;
        let conn = self.conn()?;
        get_lore_recall_trace(&conn, trace_id)
    }
}

pub(crate) fn lore_recall_decision(
    entry: &LoreRecallEntry,
    included: bool,
    reason: LoreRecallTraceDecisionReason,
) -> LoreRecallTraceEntryDecision {
    LoreRecallTraceEntryDecision {
        record_id: entry.record.record_id.clone(),
        layer_id: entry.layer_id.clone(),
        score: entry.score,
        token_estimate: entry.token_estimate,
        is_constant: entry.is_constant,
        included,
        reason,
    }
}

pub(crate) fn roleplay_lore_memory_space_descriptor() -> MemorySpaceDescriptor {
    MemorySpaceDescriptor {
        space_id: MemorySpaceId::unchecked("roleplay_lore"),
        schema_version: 1,
        module_id: Some("roleplay_lore".to_string()),
        description: "Crew-owned roleplay lore with canon-aware governance.".to_string(),
        record_shapes: vec![
            roleplay_lore_shape(
                "world",
                "Roleplay world or campaign record.",
                &[
                    ("world_id", MemoryFieldType::String, true),
                    ("title", MemoryFieldType::String, true),
                    ("body", MemoryFieldType::Markdown, true),
                    ("visibility", MemoryFieldType::String, true),
                    ("metadata_json", MemoryFieldType::Json, false),
                ],
            ),
            roleplay_lore_shape(
                "entity",
                "Roleplay character, faction, place, object, or concept.",
                &[
                    ("world_id", MemoryFieldType::String, true),
                    ("entity_id", MemoryFieldType::String, true),
                    ("title", MemoryFieldType::String, true),
                    ("body", MemoryFieldType::Markdown, true),
                    ("entity_kind", MemoryFieldType::String, false),
                    ("metadata_json", MemoryFieldType::Json, false),
                ],
            ),
            roleplay_lore_shape(
                "lore_entry",
                "World or entity lore entry.",
                &[
                    ("world_id", MemoryFieldType::String, true),
                    ("entity_id", MemoryFieldType::String, false),
                    ("title", MemoryFieldType::String, true),
                    ("body", MemoryFieldType::Markdown, true),
                    ("canon_status", MemoryFieldType::String, true),
                    ("visibility", MemoryFieldType::String, true),
                    ("metadata_json", MemoryFieldType::Json, false),
                ],
            ),
            roleplay_lore_shape(
                "relationship",
                "Relationship between roleplay entities.",
                &[
                    ("world_id", MemoryFieldType::String, true),
                    ("entity_id", MemoryFieldType::String, true),
                    ("target_entity_id", MemoryFieldType::String, true),
                    ("relationship_kind", MemoryFieldType::String, true),
                    ("body", MemoryFieldType::Markdown, true),
                    ("metadata_json", MemoryFieldType::Json, false),
                ],
            ),
            roleplay_lore_shape(
                "timeline_event",
                "Canon or draft timeline event.",
                &[
                    ("world_id", MemoryFieldType::String, true),
                    ("event_time", MemoryFieldType::String, false),
                    ("title", MemoryFieldType::String, true),
                    ("body", MemoryFieldType::Markdown, true),
                    ("metadata_json", MemoryFieldType::Json, false),
                ],
            ),
            roleplay_lore_shape(
                "provenance_event",
                "Stored provenance event projection.",
                &[
                    ("world_id", MemoryFieldType::String, true),
                    ("record_id", MemoryFieldType::String, true),
                    ("body", MemoryFieldType::Markdown, false),
                    ("metadata_json", MemoryFieldType::Json, false),
                ],
            ),
        ],
        scope_model: MemoryScopeModel {
            allowed_scopes: vec![
                MemoryScopeType::World,
                MemoryScopeType::Entity,
                MemoryScopeType::Session,
                MemoryScopeType::ConversationBranch,
            ],
            primary_scope: MemoryScopeType::World,
        },
        visibility_model: MemoryVisibilityModel::WorldScoped,
        retrieval_strategies: vec![
            MemoryRetrievalStrategy::DirectLookup,
            MemoryRetrievalStrategy::QuerySearch,
            MemoryRetrievalStrategy::Relevance,
            MemoryRetrievalStrategy::DomainSpecific,
        ],
        indexing: MemoryIndexingPolicy {
            required_capabilities: vec![
                "world_lookup".to_string(),
                "entity_lookup".to_string(),
                "canon_visibility_filters".to_string(),
                "expected_revision_conflicts".to_string(),
            ],
            optional_capabilities: vec!["full_text_search".to_string()],
        },
        prompt_policy: MemoryPromptPolicy::ExplicitUserContext,
        write_policy: MemoryWritePolicy {
            default_mode: MemoryGovernanceMode::ManualReview,
            operation_policies: vec![
                roleplay_lore_operation_policy(MemoryOperation::Add, false),
                roleplay_lore_operation_policy(MemoryOperation::Replace, true),
                roleplay_lore_operation_policy(MemoryOperation::Supersede, true),
                roleplay_lore_operation_policy(MemoryOperation::Remove, true),
                roleplay_lore_operation_policy(MemoryOperation::Archive, true),
                roleplay_lore_operation_policy(MemoryOperation::CandidateOnly, false),
            ],
        },
        operations: vec![
            MemoryOperation::Read,
            MemoryOperation::List,
            MemoryOperation::Add,
            MemoryOperation::Replace,
            MemoryOperation::Supersede,
            MemoryOperation::Remove,
            MemoryOperation::Archive,
            MemoryOperation::CandidateOnly,
        ],
        provenance_policy: MemoryProvenancePolicy {
            required_evidence: vec![MemoryEvidenceKind::Wake],
            source_required: true,
            rationale_required: true,
        },
        retention_policy: MemoryRetentionPolicy::DomainSpecific,
        conflict_policy: MemoryConflictPolicy::ExpectedRevision,
        diagnostics: MemoryDiagnosticsPolicy {
            expose_catalog: true,
            expose_record_counts: true,
            expose_policy_decisions: true,
        },
        export_import: MemoryExportImportPolicy {
            export_supported: true,
            import_supported: true,
            import_governance_mode: MemoryGovernanceMode::ManualReview,
        },
    }
}

fn roleplay_lore_shape(
    shape_id: &str,
    description: &str,
    fields: &[(&str, MemoryFieldType, bool)],
) -> MemoryRecordShapeDescriptor {
    MemoryRecordShapeDescriptor {
        shape_id: MemoryRecordShapeId::unchecked(shape_id),
        version: 1,
        description: description.to_string(),
        fields: fields
            .iter()
            .map(
                |(field_name, field_type, required)| MemoryRecordFieldDescriptor {
                    field_name: (*field_name).to_string(),
                    field_type: *field_type,
                    required: *required,
                    description: format!("{field_name} field"),
                },
            )
            .collect(),
    }
}

fn roleplay_lore_operation_policy(
    operation: MemoryOperation,
    requires_expected_revision: bool,
) -> MemoryOperationPolicy {
    MemoryOperationPolicy {
        operation,
        governance_mode: MemoryGovernanceMode::ManualReview,
        requires_expected_revision,
        min_confidence: None,
    }
}

pub(crate) fn validate_roleplay_lore_write(write: &RoleplayLoreWrite) -> CoreResult<()> {
    validate_roleplay_lore_record_id(&write.record_id)?;
    validate_roleplay_lore_identifier("roleplay lore world_id", &write.world_id)?;
    if let Some(entity_id) = &write.entity_id {
        validate_roleplay_lore_identifier("roleplay lore entity_id", entity_id)?;
    }
    if write.title.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore title must not be empty",
        ));
    }
    if write.body.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore body must not be empty",
        ));
    }
    validate_memory_confidence(write.confidence)?;
    if write.durability_rationale.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore durability_rationale is required",
        ));
    }
    if write.evidence_refs.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore evidence_refs must not be empty",
        ));
    }
    validate_roleplay_lore_shape(&write.shape)?;
    validate_roleplay_lore_content(&write.shape, &write.content)?;
    Ok(())
}

fn validate_roleplay_lore_shape(shape: &MemoryRecordShapeRef) -> CoreResult<()> {
    let descriptor = roleplay_lore_memory_space_descriptor();
    descriptor.validate()?;
    if !descriptor.has_shape(shape) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore shape is not declared by descriptor",
        ));
    }
    Ok(())
}

fn validate_roleplay_lore_content(
    shape_ref: &MemoryRecordShapeRef,
    content: &JsonValue,
) -> CoreResult<()> {
    let descriptor = roleplay_lore_memory_space_descriptor();
    let shape = descriptor
        .record_shapes
        .iter()
        .find(|shape| shape.shape_id == shape_ref.shape_id && shape.version == shape_ref.version)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore shape is not declared by descriptor",
            )
        })?;
    let object = content.as_object().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore content must be a JSON object",
        )
    })?;
    for field in shape.fields.iter().filter(|field| field.required) {
        if !object
            .get(&field.field_name)
            .map(|value| !value.is_null())
            .unwrap_or(false)
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "roleplay lore content missing required field {}",
                    field.field_name
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_roleplay_lore_record_id(record_id: &str) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore record_id", record_id)
}

pub(crate) fn validate_roleplay_lore_identifier(label: &str, value: &str) -> CoreResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not be empty"),
        ));
    }
    if trimmed.len() > 256 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be at most 256 characters"),
        ));
    }
    if trimmed.contains('\0') {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not contain NUL"),
        ));
    }
    Ok(())
}

fn insert_roleplay_lore_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &RoleplayLoreWrite,
) -> CoreResult<()> {
    let content_json = to_json_text(&write.content)?;
    let evidence_refs_json = to_json_text(&write.evidence_refs)?;
    tx.execute(
        "INSERT INTO module_roleplay_lore_records (
            record_id,
            world_id,
            entity_id,
            session_id,
            branch_id,
            shape_id,
            shape_version,
            canon_status,
            visibility,
            status,
            revision,
            title,
            body,
            content_json,
            evidence_refs_json,
            source,
            confidence,
            durability_rationale,
            supersedes_record_id,
            superseded_by_record_id,
            tombstoned_at,
            tombstone_reason,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, NULL, NULL, NULL, ?19, ?19)",
        params![
            write.record_id.as_str(),
            write.world_id.as_str(),
            write.entity_id.as_deref(),
            write.session_id.as_ref().map(|value| value.0.as_str()),
            write.branch_id.as_ref().map(|value| value.0.as_str()),
            write.shape.shape_id.0.as_str(),
            write.shape.version as i64,
            roleplay_lore_canon_status_as_str(write.canon_status),
            roleplay_lore_visibility_as_str(write.visibility),
            roleplay_lore_record_status_as_str(RoleplayLoreRecordStatus::Active),
            write.title.as_str(),
            write.body.as_str(),
            content_json,
            evidence_refs_json,
            memory_proposal_source_as_str(write.source),
            write.confidence as f64,
            write.durability_rationale.as_str(),
            write.supersedes_record_id.as_deref(),
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert roleplay lore record", error))?;
    Ok(())
}

fn update_roleplay_lore_record_content_in_tx(
    tx: &rusqlite::Transaction<'_>,
    replace: &RoleplayLoreReplace,
    next_revision: u64,
) -> CoreResult<()> {
    let content_json = to_json_text(&replace.write.content)?;
    let evidence_refs_json = to_json_text(&replace.write.evidence_refs)?;
    tx.execute(
        "UPDATE module_roleplay_lore_records
         SET world_id = ?2,
             entity_id = ?3,
             session_id = ?4,
             branch_id = ?5,
             shape_id = ?6,
             shape_version = ?7,
             canon_status = ?8,
             visibility = ?9,
             revision = ?10,
             title = ?11,
             body = ?12,
             content_json = ?13,
             evidence_refs_json = ?14,
             source = ?15,
             confidence = ?16,
             durability_rationale = ?17,
             updated_at = ?18
         WHERE record_id = ?1",
        params![
            replace.write.record_id.as_str(),
            replace.write.world_id.as_str(),
            replace.write.entity_id.as_deref(),
            replace
                .write
                .session_id
                .as_ref()
                .map(|value| value.0.as_str()),
            replace
                .write
                .branch_id
                .as_ref()
                .map(|value| value.0.as_str()),
            replace.write.shape.shape_id.0.as_str(),
            replace.write.shape.version as i64,
            roleplay_lore_canon_status_as_str(replace.write.canon_status),
            roleplay_lore_visibility_as_str(replace.write.visibility),
            next_revision as i64,
            replace.write.title.as_str(),
            replace.write.body.as_str(),
            content_json,
            evidence_refs_json,
            memory_proposal_source_as_str(replace.write.source),
            replace.write.confidence as f64,
            replace.write.durability_rationale.as_str(),
            replace.write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update roleplay lore record", error))?;
    Ok(())
}

fn mark_roleplay_lore_superseded_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record_id: &str,
    replacement_record_id: &str,
    next_revision: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE module_roleplay_lore_records
         SET status = ?2,
             revision = ?3,
             superseded_by_record_id = ?4,
             updated_at = ?5
         WHERE record_id = ?1",
        params![
            record_id,
            roleplay_lore_record_status_as_str(RoleplayLoreRecordStatus::Superseded),
            next_revision as i64,
            replacement_record_id,
            now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("mark roleplay lore superseded", error))?;
    Ok(())
}

fn tombstone_roleplay_lore_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    tombstone: &RoleplayLoreTombstone,
    next_revision: u64,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE module_roleplay_lore_records
         SET status = ?2,
             revision = ?3,
             tombstoned_at = ?4,
             tombstone_reason = ?5,
             updated_at = ?4
         WHERE record_id = ?1",
        params![
            tombstone.record_id.as_str(),
            roleplay_lore_record_status_as_str(RoleplayLoreRecordStatus::Tombstoned),
            next_revision as i64,
            tombstone.now.as_str(),
            tombstone.reason.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("tombstone roleplay lore record", error))?;
    Ok(())
}

fn active_roleplay_lore_record_for_update(
    tx: &rusqlite::Transaction<'_>,
    record_id: &str,
    expected_revision: u64,
) -> CoreResult<RoleplayLoreRecord> {
    validate_roleplay_lore_record_id(record_id)?;
    if expected_revision == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore expected_revision must be greater than zero",
        ));
    }
    let existing = get_roleplay_lore_record_in_tx(tx, record_id)?.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("roleplay lore record {record_id} not found"),
        )
    })?;
    if existing.status != RoleplayLoreRecordStatus::Active {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("roleplay lore record {record_id} is not active"),
        ));
    }
    if existing.revision != expected_revision {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "roleplay lore revision mismatch for {record_id}: expected {}, found {}",
                expected_revision, existing.revision
            ),
        ));
    }
    Ok(existing)
}

fn get_roleplay_lore_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record_id: &str,
) -> CoreResult<Option<RoleplayLoreRecord>> {
    tx.query_row(
        "SELECT record_id,
                world_id,
                entity_id,
                session_id,
                branch_id,
                shape_id,
                shape_version,
                canon_status,
                visibility,
                status,
                revision,
                title,
                body,
                content_json,
                evidence_refs_json,
                source,
                confidence,
                durability_rationale,
                supersedes_record_id,
                superseded_by_record_id,
                tombstoned_at,
                tombstone_reason,
                created_at,
                updated_at
         FROM module_roleplay_lore_records
         WHERE record_id = ?1",
        params![record_id],
        row_to_roleplay_lore_record,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore record", error))
}

fn query_roleplay_lore_records(
    conn: &Connection,
    query: &RoleplayLoreQuery,
) -> CoreResult<Vec<RoleplayLoreRecord>> {
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let canon_status = query.canon_status.map(roleplay_lore_canon_status_as_str);
    let visibility = query.visibility.map(roleplay_lore_visibility_as_str);
    let include_superseded = query.include_superseded;
    let include_tombstoned = query.include_tombstoned;
    let query_like = query
        .query
        .as_ref()
        .map(|value| sqlite_like_contains(value));
    let mut stmt = conn
        .prepare(
            "SELECT record_id,
                    world_id,
                    entity_id,
                    session_id,
                    branch_id,
                    shape_id,
                    shape_version,
                    canon_status,
                    visibility,
                    status,
                    revision,
                    title,
                    body,
                    content_json,
                    evidence_refs_json,
                    source,
                    confidence,
                    durability_rationale,
                    supersedes_record_id,
                    superseded_by_record_id,
                    tombstoned_at,
                    tombstone_reason,
                    created_at,
                    updated_at
             FROM module_roleplay_lore_records
             WHERE (?1 IS NULL OR world_id = ?1)
               AND (?2 IS NULL OR entity_id = ?2)
               AND (?3 IS NULL OR canon_status = ?3)
               AND (?4 IS NULL OR visibility = ?4)
               AND (?5 IS NULL OR shape_id = ?5)
               AND (?6 OR status != 'superseded')
               AND (?7 OR status != 'tombstoned')
               AND (?8 IS NULL OR title LIKE ?8 ESCAPE '\\' OR body LIKE ?8 ESCAPE '\\')
               AND (
                    ?9 IS NULL OR EXISTS (
                        SELECT 1
                        FROM module_roleplay_lore_provenance_events p
                        WHERE p.record_id = module_roleplay_lore_records.record_id
                          AND p.evidence_refs_json LIKE ?10 ESCAPE '\\'
                    )
               )
             ORDER BY updated_at DESC, record_id ASC
             LIMIT ?11 OFFSET ?12",
        )
        .map_err(|error| persistence_error("prepare query roleplay lore records", error))?;
    let provenance_like = query
        .provenance_ref_id
        .as_ref()
        .map(|value| sqlite_like_contains(value));
    let rows = stmt
        .query_map(
            params![
                query.world_id.as_deref(),
                query.entity_id.as_deref(),
                canon_status,
                visibility,
                query.shape_id.as_deref(),
                include_superseded,
                include_tombstoned,
                query_like.as_deref(),
                query.provenance_ref_id.as_deref(),
                provenance_like.as_deref(),
                limit,
                offset,
            ],
            row_to_roleplay_lore_record,
        )
        .map_err(|error| persistence_error("query roleplay lore records", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay lore records", error))
}

fn insert_roleplay_lore_provenance_event_in_tx(
    tx: &rusqlite::Transaction<'_>,
    event: &RoleplayLoreProvenanceEvent,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore provenance event_id", &event.event_id)?;
    validate_roleplay_lore_record_id(&event.record_id)?;
    validate_roleplay_lore_identifier("roleplay lore provenance world_id", &event.world_id)?;
    let evidence_refs_json = to_json_text(&event.evidence_refs)?;
    tx.execute(
        "INSERT INTO module_roleplay_lore_provenance_events (
            event_id,
            record_id,
            world_id,
            evidence_refs_json,
            source,
            actor,
            note,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.event_id.as_str(),
            event.record_id.as_str(),
            event.world_id.as_str(),
            evidence_refs_json,
            memory_proposal_source_as_str(event.source),
            event.actor.as_str(),
            event.note.as_deref(),
            event.created_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert roleplay lore provenance event", error))?;
    Ok(())
}

fn roleplay_lore_provenance_events(
    conn: &Connection,
    record_id: &str,
) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
    let mut stmt = conn
        .prepare(
            "SELECT event_id,
                    record_id,
                    world_id,
                    evidence_refs_json,
                    source,
                    actor,
                    note,
                    created_at
             FROM module_roleplay_lore_provenance_events
             WHERE record_id = ?1
             ORDER BY created_at ASC, event_id ASC",
        )
        .map_err(|error| persistence_error("prepare roleplay lore provenance events", error))?;
    let rows = stmt
        .query_map(params![record_id], row_to_roleplay_lore_provenance_event)
        .map_err(|error| persistence_error("query roleplay lore provenance events", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay lore provenance events", error))
}

fn get_lore_layer(
    conn: &Connection,
    layer_id: &str,
) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
    conn.query_row(
        "SELECT layer_id,
                profile_id,
                name,
                description,
                purpose,
                write_policy,
                is_archived,
                created_at,
                updated_at
         FROM module_roleplay_lore_layers
         WHERE layer_id = ?1",
        params![layer_id],
        row_to_lore_layer,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore layer", error))
}

fn get_lore_layer_in_tx(
    tx: &rusqlite::Transaction<'_>,
    layer_id: &str,
) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
    tx.query_row(
        "SELECT layer_id,
                profile_id,
                name,
                description,
                purpose,
                write_policy,
                is_archived,
                created_at,
                updated_at
         FROM module_roleplay_lore_layers
         WHERE layer_id = ?1",
        params![layer_id],
        row_to_lore_layer,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore layer in transaction", error))
}

fn list_lore_layers_by_profile(
    conn: &Connection,
    profile_id: &str,
) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT layer_id,
                    profile_id,
                    name,
                    description,
                    purpose,
                    write_policy,
                    is_archived,
                    created_at,
                    updated_at
             FROM module_roleplay_lore_layers
             WHERE profile_id = ?1 AND is_archived = 0
             ORDER BY name ASC, layer_id ASC",
        )
        .map_err(|error| persistence_error("prepare list roleplay lore layers", error))?;
    let rows = stmt
        .query_map(params![profile_id], row_to_lore_layer)
        .map_err(|error| persistence_error("query roleplay lore layers", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay lore layers", error))
}

fn get_lore_layer_config(
    conn: &Connection,
    layer_id: &str,
) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
    conn.query_row(
        "SELECT config_id,
                layer_id,
                fts_weight,
                subject_weight,
                canon_weight,
                tag_boost_weight,
                recency_weight,
                default_token_budget,
                constant_token_reserve,
                min_relevance_score,
                max_constants,
                created_at,
                updated_at
         FROM module_roleplay_lore_layer_config
         WHERE layer_id = ?1",
        params![layer_id],
        row_to_lore_layer_config,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore layer config", error))
}

fn get_lore_layer_config_in_tx(
    tx: &rusqlite::Transaction<'_>,
    layer_id: &str,
) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
    tx.query_row(
        "SELECT config_id,
                layer_id,
                fts_weight,
                subject_weight,
                canon_weight,
                tag_boost_weight,
                recency_weight,
                default_token_budget,
                constant_token_reserve,
                min_relevance_score,
                max_constants,
                created_at,
                updated_at
         FROM module_roleplay_lore_layer_config
         WHERE layer_id = ?1",
        params![layer_id],
        row_to_lore_layer_config,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore layer config in transaction", error))
}

fn list_entries_by_layer(
    conn: &Connection,
    layer_id: &str,
) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
    let mut stmt = conn
        .prepare(
            "SELECT e.layer_id,
                    e.record_id,
                    e.is_constant,
                    e.priority,
                    e.added_at,
                    r.record_id,
                    r.world_id,
                    r.entity_id,
                    r.session_id,
                    r.branch_id,
                    r.shape_id,
                    r.shape_version,
                    r.canon_status,
                    r.visibility,
                    r.status,
                    r.revision,
                    r.title,
                    r.body,
                    r.content_json,
                    r.evidence_refs_json,
                    r.source,
                    r.confidence,
                    r.durability_rationale,
                    r.supersedes_record_id,
                    r.superseded_by_record_id,
                    r.tombstoned_at,
                    r.tombstone_reason,
                    r.created_at,
                    r.updated_at
             FROM module_roleplay_lore_layer_entries e
             JOIN module_roleplay_lore_records r ON r.record_id = e.record_id
             WHERE e.layer_id = ?1
             ORDER BY e.is_constant DESC, e.priority ASC, r.updated_at DESC, e.record_id ASC",
        )
        .map_err(|error| persistence_error("prepare list roleplay lore layer entries", error))?;
    let rows = stmt
        .query_map(params![layer_id], row_to_lore_layer_entry_join)
        .map_err(|error| persistence_error("query roleplay lore layer entries", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay lore layer entries", error))
}

fn get_lore_layer_entry_join_in_tx(
    tx: &rusqlite::Transaction<'_>,
    layer_id: &str,
    record_id: &str,
) -> CoreResult<Option<RoleplayLoreLayerEntryJoin>> {
    tx.query_row(
        "SELECT e.layer_id,
                e.record_id,
                e.is_constant,
                e.priority,
                e.added_at,
                r.record_id,
                r.world_id,
                r.entity_id,
                r.session_id,
                r.branch_id,
                r.shape_id,
                r.shape_version,
                r.canon_status,
                r.visibility,
                r.status,
                r.revision,
                r.title,
                r.body,
                r.content_json,
                r.evidence_refs_json,
                r.source,
                r.confidence,
                r.durability_rationale,
                r.supersedes_record_id,
                r.superseded_by_record_id,
                r.tombstoned_at,
                r.tombstone_reason,
                r.created_at,
                r.updated_at
         FROM module_roleplay_lore_layer_entries e
         JOIN module_roleplay_lore_records r ON r.record_id = e.record_id
         WHERE e.layer_id = ?1 AND e.record_id = ?2",
        params![layer_id, record_id],
        row_to_lore_layer_entry_join,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore layer entry", error))
}

fn insert_lore_layer_entry_in_tx(
    tx: &rusqlite::Transaction<'_>,
    link: &RoleplayLoreLayerEntryLink,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO module_roleplay_lore_layer_entries (
            layer_id,
            record_id,
            is_constant,
            priority,
            added_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(layer_id, record_id) DO UPDATE SET
            is_constant = excluded.is_constant,
            priority = excluded.priority",
        params![
            link.layer_id.as_str(),
            link.record_id.as_str(),
            bool_to_sql(link.is_constant),
            link.priority,
            link.added_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("upsert roleplay lore layer entry", error))?;
    Ok(())
}

fn get_chat_layers(conn: &Connection, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT c.chat_id,
                    c.layer_id,
                    c.priority,
                    c.enabled,
                    c.created_at,
                    l.layer_id,
                    l.profile_id,
                    l.name,
                    l.description,
                    l.purpose,
                    l.write_policy,
                    l.is_archived,
                    l.created_at,
                    l.updated_at
             FROM module_roleplay_chat_layers c
             JOIN module_roleplay_lore_layers l ON l.layer_id = c.layer_id
             WHERE c.chat_id = ?1
             ORDER BY c.priority ASC, c.layer_id ASC",
        )
        .map_err(|error| persistence_error("prepare get roleplay chat layers", error))?;
    let rows = stmt
        .query_map(params![chat_id], row_to_chat_layer_record)
        .map_err(|error| persistence_error("query roleplay chat layers", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay chat layers", error))
}

pub(crate) fn get_chat_layers_in_tx(
    tx: &rusqlite::Transaction<'_>,
    chat_id: &str,
) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
    let mut stmt = tx
        .prepare(
            "SELECT c.chat_id,
                    c.layer_id,
                    c.priority,
                    c.enabled,
                    c.created_at,
                    l.layer_id,
                    l.profile_id,
                    l.name,
                    l.description,
                    l.purpose,
                    l.write_policy,
                    l.is_archived,
                    l.created_at,
                    l.updated_at
             FROM module_roleplay_chat_layers c
             JOIN module_roleplay_lore_layers l ON l.layer_id = c.layer_id
             WHERE c.chat_id = ?1
             ORDER BY c.priority ASC, c.layer_id ASC",
        )
        .map_err(|error| persistence_error("prepare get roleplay chat layers in tx", error))?;
    let rows = stmt
        .query_map(params![chat_id], row_to_chat_layer_record)
        .map_err(|error| persistence_error("query roleplay chat layers in tx", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay chat layers in tx", error))
}

fn require_lore_layer_and_record(
    tx: &rusqlite::Transaction<'_>,
    layer_id: &str,
    record_id: &str,
) -> CoreResult<()> {
    if get_lore_layer_in_tx(tx, layer_id)?.is_none() {
        return Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("roleplay lore layer {layer_id} not found"),
        ));
    }
    if get_roleplay_lore_record_in_tx(tx, record_id)?.is_none() {
        return Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("roleplay lore record {record_id} not found"),
        ));
    }
    Ok(())
}

fn constant_lore_entries_for_layer(
    tx: &rusqlite::Transaction<'_>,
    layer_id: &str,
    config: &RoleplayLoreLayerConfigRecord,
) -> CoreResult<Vec<LoreRecallEntry>> {
    let mut stmt = tx
        .prepare(
            "SELECT e.layer_id,
                    e.record_id,
                    e.is_constant,
                    e.priority,
                    e.added_at,
                    r.record_id,
                    r.world_id,
                    r.entity_id,
                    r.session_id,
                    r.branch_id,
                    r.shape_id,
                    r.shape_version,
                    r.canon_status,
                    r.visibility,
                    r.status,
                    r.revision,
                    r.title,
                    r.body,
                    r.content_json,
                    r.evidence_refs_json,
                    r.source,
                    r.confidence,
                    r.durability_rationale,
                    r.supersedes_record_id,
                    r.superseded_by_record_id,
                    r.tombstoned_at,
                    r.tombstone_reason,
                    r.created_at,
                    r.updated_at
             FROM module_roleplay_lore_layer_entries e
             JOIN module_roleplay_lore_records r ON r.record_id = e.record_id
             WHERE e.layer_id = ?1
               AND e.is_constant = 1
               AND r.status = 'active'
             ORDER BY e.priority ASC, r.updated_at DESC, e.record_id ASC
             LIMIT ?2",
        )
        .map_err(|error| persistence_error("prepare roleplay lore constant recall", error))?;
    let rows = stmt
        .query_map(params![layer_id, config.max_constants as i64], |row| {
            let join = row_to_lore_layer_entry_join(row)?;
            let token_estimate = estimate_lore_tokens(&join.record);
            Ok(LoreRecallEntry {
                record: join.record,
                layer_id: join.layer_id,
                score: 1_000.0 - join.priority as f32,
                token_estimate,
                is_constant: true,
            })
        })
        .map_err(|error| persistence_error("query roleplay lore constant recall", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load roleplay lore constant recall", error))
}

fn scored_lore_entries_for_recall(
    tx: &rusqlite::Transaction<'_>,
    query: &LoreRecallQuery,
    layer_configs: &[(RoleplayChatLayerRecord, RoleplayLoreLayerConfigRecord)],
    seen_records: &BTreeSet<String>,
) -> CoreResult<Vec<LoreRecallEntry>> {
    let Some(query_text) = query.query_text.as_deref().map(str::trim) else {
        return Ok(Vec::new());
    };
    if query_text.is_empty() || layer_configs.is_empty() {
        return Ok(Vec::new());
    }
    let Some(fts_query) = sqlite_lore_recall_fts_query(query_text) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (layer, config) in layer_configs {
        let mut stmt = tx
            .prepare(
                "SELECT e.layer_id,
                        e.record_id,
                        e.is_constant,
                        e.priority,
                        e.added_at,
                        r.record_id,
                        r.world_id,
                        r.entity_id,
                        r.session_id,
                        r.branch_id,
                        r.shape_id,
                        r.shape_version,
                        r.canon_status,
                        r.visibility,
                        r.status,
                        r.revision,
                        r.title,
                        r.body,
                        r.content_json,
                        r.evidence_refs_json,
                        r.source,
                        r.confidence,
                        r.durability_rationale,
                        r.supersedes_record_id,
                        r.superseded_by_record_id,
                        r.tombstoned_at,
                        r.tombstone_reason,
                        r.created_at,
                        r.updated_at,
                        bm25(module_roleplay_lore_records_fts) AS fts_rank
                 FROM module_roleplay_lore_records_fts
                 JOIN module_roleplay_lore_records r
                    ON r.rowid = module_roleplay_lore_records_fts.rowid
                 JOIN module_roleplay_lore_layer_entries e
                    ON e.record_id = r.record_id
                 WHERE module_roleplay_lore_records_fts MATCH ?1
                   AND e.layer_id = ?2
                   AND r.status = 'active'
                 ORDER BY fts_rank ASC, e.priority ASC, r.updated_at DESC
                 LIMIT 100",
            )
            .map_err(|error| persistence_error("prepare roleplay lore scored recall", error))?;
        let rows = stmt
            .query_map(
                params![fts_query.as_str(), layer.layer_id.as_str()],
                |row| {
                    let join = row_to_lore_layer_entry_join(row)?;
                    let fts_rank = row.get::<_, f64>(29)? as f32;
                    let token_estimate = estimate_lore_tokens(&join.record);
                    let score = score_lore_recall_entry(
                        &join.record,
                        config,
                        layer.priority,
                        join.priority,
                        fts_rank,
                        query_text,
                        &query.active_subjects,
                    );
                    Ok(LoreRecallEntry {
                        record: join.record,
                        layer_id: join.layer_id,
                        score,
                        token_estimate,
                        is_constant: false,
                    })
                },
            )
            .map_err(|error| persistence_error("query roleplay lore scored recall", error))?;
        for entry in rows {
            let entry = entry
                .map_err(|error| persistence_error("load roleplay lore scored recall", error))?;
            if seen_records.contains(&entry.record.record_id) {
                continue;
            }
            if excluded_subject_match(&entry.record, &query.excluded_subjects) {
                continue;
            }
            // The FTS index also contains structured content metadata. Require
            // at least one title/body match so broad natural-language OR
            // queries cannot pull records solely through repeated IDs or
            // world metadata.
            if lore_query_overlap(&entry.record, query_text) == 0.0 {
                continue;
            }
            if entry.score < config.min_relevance_score {
                continue;
            }
            out.push(entry);
        }
    }
    Ok(out)
}

pub(crate) fn score_lore_recall_entry(
    record: &RoleplayLoreRecord,
    config: &RoleplayLoreLayerConfigRecord,
    chat_layer_priority: i64,
    entry_priority: i64,
    fts_rank: f32,
    query_text: &str,
    active_subjects: &[String],
) -> f32 {
    let fts_score = (1.0 / (1.0 + fts_rank.max(0.0))) * config.fts_weight;
    let subject_score = if subject_match(record, active_subjects) {
        config.subject_weight
    } else {
        0.0
    };
    let canon_score = match record.canon_status {
        RoleplayLoreCanonStatus::Canon => 1.0,
        RoleplayLoreCanonStatus::Contested => 0.5,
        RoleplayLoreCanonStatus::Draft => 0.25,
        RoleplayLoreCanonStatus::Deprecated => 0.0,
    } * config.canon_weight;
    let layer_boost = 1.0 / (1.0 + chat_layer_priority.max(0) as f32);
    let priority_boost = 1.0 / (1.0 + entry_priority.max(0) as f32);
    let tag_overlap = lore_query_overlap(record, query_text) * config.tag_boost_weight;
    let recency = config.recency_weight;
    fts_score + subject_score + canon_score + layer_boost + priority_boost + tag_overlap + recency
}

fn subject_match(record: &RoleplayLoreRecord, subjects: &[String]) -> bool {
    subjects.iter().any(|subject| {
        let normalized = subject.trim();
        !normalized.is_empty()
            && (record.world_id == normalized
                || record.entity_id.as_deref() == Some(normalized)
                || record.title.contains(normalized)
                || record.body.contains(normalized))
    })
}

pub(crate) fn excluded_subject_match(record: &RoleplayLoreRecord, subjects: &[String]) -> bool {
    subject_match(record, subjects)
}

pub(crate) fn lore_query_overlap(record: &RoleplayLoreRecord, query_text: &str) -> f32 {
    let haystack = format!(
        "{} {}",
        record.title.to_lowercase(),
        record.body.to_lowercase()
    );
    let mut total = 0_u32;
    let mut matched = 0_u32;
    for token in lore_recall_search_terms(query_text) {
        total += 1;
        if haystack.contains(&token) {
            matched += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        matched as f32 / total as f32
    }
}

const LORE_RECALL_STOP_WORDS: &[&str] = &[
    "about",
    "after",
    "again",
    "answer",
    "answering",
    "before",
    "character",
    "continue",
    "could",
    "does",
    "from",
    "have",
    "into",
    "just",
    "keep",
    "keeps",
    "more",
    "once",
    "only",
    "other",
    "roleplay",
    "scene",
    "should",
    "that",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "through",
    "turn",
    "turns",
    "voice",
    "what",
    "when",
    "where",
    "which",
    "while",
    "without",
    "would",
    "write",
    "your",
];

pub(crate) fn lore_recall_search_terms(query_text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    query_text
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 4)
        .map(str::to_lowercase)
        .filter(|token| !LORE_RECALL_STOP_WORDS.contains(&token.as_str()))
        .filter(|token| seen.insert(token.clone()))
        .take(24)
        .collect()
}

pub(crate) fn sqlite_lore_recall_fts_query(query_text: &str) -> Option<String> {
    let terms = lore_recall_search_terms(query_text);
    (!terms.is_empty()).then(|| {
        terms
            .iter()
            .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ")
    })
}

#[cfg(feature = "postgres")]
pub(crate) fn postgres_lore_recall_tsquery(query_text: &str) -> Option<String> {
    let terms = lore_recall_search_terms(query_text);
    (!terms.is_empty()).then(|| {
        terms
            .iter()
            .map(|term| format!("{term}:*"))
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

pub(crate) fn estimate_lore_tokens(record: &RoleplayLoreRecord) -> u32 {
    let words = record.title.split_whitespace().count() + record.body.split_whitespace().count();
    ((words as f32) * 1.35).ceil().max(1.0) as u32
}

pub(crate) fn default_lore_layer_config(
    layer_id: &str,
    now: &IsoTimestamp,
) -> RoleplayLoreLayerConfigRecord {
    RoleplayLoreLayerConfigRecord {
        config_id: format!("{layer_id}:default"),
        layer_id: layer_id.to_string(),
        fts_weight: 1.0,
        subject_weight: 1.0,
        canon_weight: 0.5,
        tag_boost_weight: 0.5,
        recency_weight: 0.2,
        default_token_budget: 4_000,
        constant_token_reserve: 500,
        min_relevance_score: 0.3,
        max_constants: 5,
        created_at: now.clone(),
        updated_at: now.clone(),
    }
}

pub(crate) fn lore_recall_config_snapshot(
    layer_configs: &[(RoleplayChatLayerRecord, RoleplayLoreLayerConfigRecord)],
) -> JsonValue {
    serde_json::json!({
        "layers": layer_configs
            .iter()
            .map(|(layer, config)| {
                serde_json::json!({
                    "layer_id": layer.layer_id,
                    "priority": layer.priority,
                    "config": config,
                })
            })
            .collect::<Vec<_>>()
    })
}

fn insert_lore_recall_trace_in_tx(
    tx: &rusqlite::Transaction<'_>,
    trace: &LoreRecallTraceRecord,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore recall trace_id", &trace.trace_id)?;
    let layer_ids = to_json_text(&trace.layer_ids)?;
    let active_subjects = to_json_text(&trace.active_subjects)?;
    let excluded_subjects = to_json_text(&trace.excluded_subjects)?;
    let config_snapshot = to_json_text(&trace.config_snapshot)?;
    let entry_decisions = to_json_text(&trace.entry_decisions)?;
    tx.execute(
        "INSERT INTO module_roleplay_lore_recall_traces (
            trace_id,
            session_id,
            layer_ids,
            query_text,
            active_subjects,
            excluded_subjects,
            config_snapshot,
            entries_considered,
            entries_returned,
            token_budget,
            tokens_consumed,
            entry_decisions,
            created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            trace.trace_id.as_str(),
            trace.session_id.as_ref().map(|value| value.0.as_str()),
            layer_ids,
            trace.query_text.as_deref(),
            active_subjects,
            excluded_subjects,
            config_snapshot,
            trace.entries_considered as i64,
            trace.entries_returned as i64,
            trace.token_budget.map(|value| value as i64),
            trace.tokens_consumed as i64,
            entry_decisions,
            trace.created_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert roleplay lore recall trace", error))?;
    Ok(())
}

fn list_lore_recall_traces(
    conn: &Connection,
    query: &LoreRecallTraceQuery,
) -> CoreResult<Vec<LoreRecallTraceRecord>> {
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 500);
    let mut stmt = conn
        .prepare(
            "SELECT trace_id,
                    session_id,
                    layer_ids,
                    query_text,
                    active_subjects,
                    excluded_subjects,
                    config_snapshot,
                    entries_considered,
                    entries_returned,
                    token_budget,
                    tokens_consumed,
                    entry_decisions,
                    created_at
             FROM module_roleplay_lore_recall_traces
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR trace_id LIKE ?2 || ':%' OR trace_id LIKE 'recall:' || ?2 || ':%')
             ORDER BY created_at DESC, trace_id DESC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare list roleplay lore recall traces", error))?;
    let rows = stmt
        .query_map(
            params![
                query.session_id.as_ref().map(|value| value.0.as_str()),
                query.chat_id.as_deref(),
                limit,
                offset,
            ],
            row_to_lore_recall_trace,
        )
        .map_err(|error| persistence_error("query roleplay lore recall traces", error))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| persistence_error("load roleplay lore recall traces", error))
}

fn get_lore_recall_trace(
    conn: &Connection,
    trace_id: &str,
) -> CoreResult<Option<LoreRecallTraceRecord>> {
    conn.query_row(
        "SELECT trace_id,
                session_id,
                layer_ids,
                query_text,
                active_subjects,
                excluded_subjects,
                config_snapshot,
                entries_considered,
                entries_returned,
                token_budget,
                tokens_consumed,
                entry_decisions,
                created_at
         FROM module_roleplay_lore_recall_traces
         WHERE trace_id = ?1",
        params![trace_id],
        row_to_lore_recall_trace,
    )
    .optional()
    .map_err(|error| persistence_error("get roleplay lore recall trace", error))
}

fn row_to_roleplay_lore_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoleplayLoreRecord> {
    row_to_roleplay_lore_record_at(row, 0)
}

fn row_to_roleplay_lore_record_at(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<RoleplayLoreRecord> {
    let shape_id: String = row.get(base + 5)?;
    let canon_status: String = row.get(base + 7)?;
    let visibility: String = row.get(base + 8)?;
    let status: String = row.get(base + 9)?;
    let revision: i64 = row.get(base + 10)?;
    let content_json: String = row.get(base + 13)?;
    let evidence_refs_json: String = row.get(base + 14)?;
    let source: String = row.get(base + 15)?;
    Ok(RoleplayLoreRecord {
        record_id: row.get(base)?,
        world_id: row.get(base + 1)?,
        entity_id: row.get(base + 2)?,
        session_id: row.get::<_, Option<String>>(base + 3)?.map(SessionId::new),
        branch_id: row
            .get::<_, Option<String>>(base + 4)?
            .map(ConversationBranchId::new),
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::new(shape_id).map_err(to_sql_core_error)?,
            version: row.get::<_, i64>(base + 6)? as u32,
        },
        canon_status: parse_roleplay_lore_canon_status(&canon_status).map_err(to_sql_core_error)?,
        visibility: parse_roleplay_lore_visibility(&visibility).map_err(to_sql_core_error)?,
        status: parse_roleplay_lore_record_status(&status).map_err(to_sql_core_error)?,
        revision: revision as u64,
        title: row.get(base + 11)?,
        body: row.get(base + 12)?,
        content: from_json_text(&content_json).map_err(to_sql_error)?,
        evidence_refs: from_json_text(&evidence_refs_json).map_err(to_sql_error)?,
        source: parse_memory_proposal_source(&source).map_err(to_sql_core_error)?,
        confidence: row.get::<_, f64>(base + 16)? as f32,
        durability_rationale: row.get(base + 17)?,
        supersedes_record_id: row.get(base + 18)?,
        superseded_by_record_id: row.get(base + 19)?,
        tombstoned_at: row.get(base + 20)?,
        tombstone_reason: row.get(base + 21)?,
        created_at: row.get(base + 22)?,
        updated_at: row.get(base + 23)?,
    })
}

fn row_to_lore_layer(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoleplayLoreLayerRecord> {
    row_to_lore_layer_at(row, 0)
}

fn row_to_lore_layer_at(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<RoleplayLoreLayerRecord> {
    let purpose: String = row.get(base + 4)?;
    let write_policy: String = row.get(base + 5)?;
    let is_archived: i64 = row.get(base + 6)?;
    Ok(RoleplayLoreLayerRecord {
        layer_id: row.get(base)?,
        profile_id: row.get(base + 1)?,
        name: row.get(base + 2)?,
        description: row.get(base + 3)?,
        purpose: parse_roleplay_lore_layer_purpose(&purpose).map_err(to_sql_core_error)?,
        write_policy: parse_roleplay_lore_layer_write_policy(&write_policy)
            .map_err(to_sql_core_error)?,
        is_archived: sql_bool(is_archived),
        created_at: row.get(base + 7)?,
        updated_at: row.get(base + 8)?,
    })
}

fn row_to_lore_layer_config(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RoleplayLoreLayerConfigRecord> {
    Ok(RoleplayLoreLayerConfigRecord {
        config_id: row.get(0)?,
        layer_id: row.get(1)?,
        fts_weight: row.get::<_, f64>(2)? as f32,
        subject_weight: row.get::<_, f64>(3)? as f32,
        canon_weight: row.get::<_, f64>(4)? as f32,
        tag_boost_weight: row.get::<_, f64>(5)? as f32,
        recency_weight: row.get::<_, f64>(6)? as f32,
        default_token_budget: row.get::<_, i64>(7)? as u32,
        constant_token_reserve: row.get::<_, i64>(8)? as u32,
        min_relevance_score: row.get::<_, f64>(9)? as f32,
        max_constants: row.get::<_, i64>(10)? as u32,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn row_to_lore_layer_entry_join(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RoleplayLoreLayerEntryJoin> {
    let is_constant: i64 = row.get(2)?;
    Ok(RoleplayLoreLayerEntryJoin {
        layer_id: row.get(0)?,
        record_id: row.get(1)?,
        is_constant: sql_bool(is_constant),
        priority: row.get(3)?,
        added_at: row.get(4)?,
        record: row_to_roleplay_lore_record_at(row, 5)?,
    })
}

fn row_to_chat_layer_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoleplayChatLayerRecord> {
    let enabled: i64 = row.get(3)?;
    Ok(RoleplayChatLayerRecord {
        chat_id: row.get(0)?,
        layer_id: row.get(1)?,
        priority: row.get(2)?,
        enabled: sql_bool(enabled),
        created_at: row.get(4)?,
        layer: row_to_lore_layer_at(row, 5)?,
    })
}

fn row_to_roleplay_lore_provenance_event(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RoleplayLoreProvenanceEvent> {
    let evidence_refs_json: String = row.get(3)?;
    let source: String = row.get(4)?;
    Ok(RoleplayLoreProvenanceEvent {
        event_id: row.get(0)?,
        record_id: row.get(1)?,
        world_id: row.get(2)?,
        evidence_refs: from_json_text(&evidence_refs_json).map_err(to_sql_error)?,
        source: parse_memory_proposal_source(&source).map_err(to_sql_core_error)?,
        actor: row.get(5)?,
        note: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn row_to_lore_recall_trace(row: &rusqlite::Row<'_>) -> rusqlite::Result<LoreRecallTraceRecord> {
    let layer_ids_json: String = row.get(2)?;
    let active_subjects_json: String = row.get(4)?;
    let excluded_subjects_json: String = row.get(5)?;
    let config_snapshot_json: String = row.get(6)?;
    let entry_decisions_json: String = row.get(11)?;
    Ok(LoreRecallTraceRecord {
        trace_id: row.get(0)?,
        session_id: row.get::<_, Option<String>>(1)?.map(SessionId::new),
        layer_ids: from_json_text(&layer_ids_json).map_err(to_sql_error)?,
        query_text: row.get(3)?,
        active_subjects: from_json_text(&active_subjects_json).map_err(to_sql_error)?,
        excluded_subjects: from_json_text(&excluded_subjects_json).map_err(to_sql_error)?,
        config_snapshot: from_json_text(&config_snapshot_json).map_err(to_sql_error)?,
        entries_considered: row.get::<_, i64>(7)? as u32,
        entries_returned: row.get::<_, i64>(8)? as u32,
        token_budget: row.get::<_, Option<i64>>(9)?.map(|value| value as u32),
        tokens_consumed: row.get::<_, i64>(10)? as u32,
        entry_decisions: from_json_text(&entry_decisions_json).map_err(to_sql_error)?,
        created_at: row.get(12)?,
    })
}

pub(crate) fn roleplay_lore_record_status_as_str(status: RoleplayLoreRecordStatus) -> &'static str {
    match status {
        RoleplayLoreRecordStatus::Active => "active",
        RoleplayLoreRecordStatus::Superseded => "superseded",
        RoleplayLoreRecordStatus::Tombstoned => "tombstoned",
    }
}

pub(crate) fn parse_roleplay_lore_record_status(raw: &str) -> CoreResult<RoleplayLoreRecordStatus> {
    match raw {
        "active" => Ok(RoleplayLoreRecordStatus::Active),
        "superseded" => Ok(RoleplayLoreRecordStatus::Superseded),
        "tombstoned" => Ok(RoleplayLoreRecordStatus::Tombstoned),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid roleplay lore record status {other}"),
        )),
    }
}

pub(crate) fn roleplay_lore_canon_status_as_str(status: RoleplayLoreCanonStatus) -> &'static str {
    match status {
        RoleplayLoreCanonStatus::Canon => "canon",
        RoleplayLoreCanonStatus::Draft => "draft",
        RoleplayLoreCanonStatus::Contested => "contested",
        RoleplayLoreCanonStatus::Deprecated => "deprecated",
    }
}

pub(crate) fn parse_roleplay_lore_canon_status(raw: &str) -> CoreResult<RoleplayLoreCanonStatus> {
    match raw {
        "canon" => Ok(RoleplayLoreCanonStatus::Canon),
        "draft" => Ok(RoleplayLoreCanonStatus::Draft),
        "contested" => Ok(RoleplayLoreCanonStatus::Contested),
        "deprecated" => Ok(RoleplayLoreCanonStatus::Deprecated),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid roleplay lore canon status {other}"),
        )),
    }
}

pub(crate) fn roleplay_lore_visibility_as_str(visibility: RoleplayLoreVisibility) -> &'static str {
    match visibility {
        RoleplayLoreVisibility::Public => "public",
        RoleplayLoreVisibility::Private => "private",
        RoleplayLoreVisibility::GmOnly => "gm_only",
        RoleplayLoreVisibility::ToolOnly => "tool_only",
    }
}

pub(crate) fn parse_roleplay_lore_visibility(raw: &str) -> CoreResult<RoleplayLoreVisibility> {
    match raw {
        "public" => Ok(RoleplayLoreVisibility::Public),
        "private" => Ok(RoleplayLoreVisibility::Private),
        "gm_only" => Ok(RoleplayLoreVisibility::GmOnly),
        "tool_only" => Ok(RoleplayLoreVisibility::ToolOnly),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid roleplay lore visibility {other}"),
        )),
    }
}

pub(crate) fn roleplay_lore_layer_purpose_as_str(
    purpose: RoleplayLoreLayerPurpose,
) -> &'static str {
    match purpose {
        RoleplayLoreLayerPurpose::World => "world",
        RoleplayLoreLayerPurpose::Story => "story",
        RoleplayLoreLayerPurpose::Characters => "characters",
        RoleplayLoreLayerPurpose::Factions => "factions",
        RoleplayLoreLayerPurpose::Mixed => "mixed",
    }
}

pub(crate) fn parse_roleplay_lore_layer_purpose(raw: &str) -> CoreResult<RoleplayLoreLayerPurpose> {
    match raw {
        "world" => Ok(RoleplayLoreLayerPurpose::World),
        "story" => Ok(RoleplayLoreLayerPurpose::Story),
        "characters" => Ok(RoleplayLoreLayerPurpose::Characters),
        "factions" => Ok(RoleplayLoreLayerPurpose::Factions),
        "mixed" => Ok(RoleplayLoreLayerPurpose::Mixed),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid roleplay lore layer purpose {other}"),
        )),
    }
}

pub(crate) fn roleplay_lore_layer_write_policy_as_str(
    write_policy: RoleplayLoreLayerWritePolicy,
) -> &'static str {
    match write_policy {
        RoleplayLoreLayerWritePolicy::Manual => "manual",
        RoleplayLoreLayerWritePolicy::AutoCapture => "auto_capture",
        RoleplayLoreLayerWritePolicy::Readonly => "readonly",
    }
}

pub(crate) fn parse_roleplay_lore_layer_write_policy(
    raw: &str,
) -> CoreResult<RoleplayLoreLayerWritePolicy> {
    match raw {
        "manual" => Ok(RoleplayLoreLayerWritePolicy::Manual),
        "auto_capture" => Ok(RoleplayLoreLayerWritePolicy::AutoCapture),
        "readonly" => Ok(RoleplayLoreLayerWritePolicy::Readonly),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid roleplay lore layer write policy {other}"),
        )),
    }
}

pub(crate) fn validate_roleplay_lore_layer_write(write: &RoleplayLoreLayerWrite) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore layer_id", &write.layer_id)?;
    validate_roleplay_lore_identifier("roleplay lore profile_id", &write.profile_id)?;
    validate_roleplay_lore_layer_name(&write.name)?;
    validate_optional_lore_text(
        "roleplay lore layer description",
        write.description.as_deref(),
    )?;
    Ok(())
}

pub(crate) fn validate_roleplay_lore_layer_update(
    update: &RoleplayLoreLayerUpdate,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore layer_id", &update.layer_id)?;
    if let Some(name) = &update.name {
        validate_roleplay_lore_layer_name(name)?;
    }
    if let Some(description) = &update.description {
        validate_optional_lore_text("roleplay lore layer description", description.as_deref())?;
    }
    if update.name.is_none()
        && update.description.is_none()
        && update.purpose.is_none()
        && update.write_policy.is_none()
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore layer update must include at least one field",
        ));
    }
    Ok(())
}

fn validate_roleplay_lore_layer_name(name: &str) -> CoreResult<()> {
    validate_non_empty_bounded_text("roleplay lore layer name", name, 160)
}

pub(crate) fn validate_roleplay_lore_layer_config_write(
    write: &RoleplayLoreLayerConfigWrite,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore layer config_id", &write.config_id)?;
    validate_roleplay_lore_identifier("roleplay lore layer_id", &write.layer_id)?;
    for (label, value) in [
        ("roleplay lore fts_weight", write.fts_weight),
        ("roleplay lore subject_weight", write.subject_weight),
        ("roleplay lore canon_weight", write.canon_weight),
        ("roleplay lore tag_boost_weight", write.tag_boost_weight),
        ("roleplay lore recency_weight", write.recency_weight),
        (
            "roleplay lore min_relevance_score",
            write.min_relevance_score,
        ),
    ] {
        validate_non_negative_finite(label, value)?;
    }
    validate_positive_u32(
        "roleplay lore default_token_budget",
        write.default_token_budget,
    )?;
    validate_positive_u32(
        "roleplay lore constant_token_reserve",
        write.constant_token_reserve,
    )?;
    validate_positive_u32("roleplay lore max_constants", write.max_constants)?;
    Ok(())
}

pub(crate) fn validate_roleplay_lore_layer_entry_link(
    link: &RoleplayLoreLayerEntryLink,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore layer_id", &link.layer_id)?;
    validate_roleplay_lore_record_id(&link.record_id)?;
    Ok(())
}

pub(crate) fn validate_roleplay_lore_fact_capture(
    capture: &RoleplayLoreFactCapture,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore layer_id", &capture.layer_id)?;
    validate_roleplay_lore_write(&capture.write)?;
    validate_optional_lore_text(
        "roleplay lore capture_reason",
        capture.capture_reason.as_deref(),
    )?;
    Ok(())
}

pub(crate) fn validate_roleplay_lore_entry_promotion(
    promotion: &RoleplayLoreEntryPromotion,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay lore source_layer_id", &promotion.source_layer_id)?;
    validate_roleplay_lore_record_id(&promotion.source_record_id)?;
    validate_roleplay_lore_identifier("roleplay lore target_layer_id", &promotion.target_layer_id)?;
    validate_roleplay_lore_record_id(&promotion.new_record_id)?;
    if promotion.new_record_id == promotion.source_record_id {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore promoted record_id must differ from source_record_id",
        ));
    }
    Ok(())
}

pub(crate) fn validate_roleplay_chat_layers_write(
    write: &RoleplayChatLayersWrite,
) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay chat_id", &write.chat_id)?;
    let layer_ids = write
        .layers
        .iter()
        .map(|layer| layer.layer_id.clone())
        .collect::<Vec<_>>();
    validate_unique_roleplay_ids("roleplay chat layer_ids", &layer_ids)?;
    Ok(())
}

pub(crate) fn validate_lore_recall_query(query: &LoreRecallQuery) -> CoreResult<()> {
    validate_roleplay_lore_identifier("roleplay chat_id", &query.chat_id)?;
    if let Some(trace_id) = &query.trace_id {
        validate_roleplay_lore_identifier("roleplay lore recall trace_id", trace_id)?;
    }
    if let Some(token_budget) = query.token_budget {
        validate_positive_u32("roleplay lore recall token_budget", token_budget)?;
    }
    validate_unique_roleplay_ids("roleplay lore active_subjects", &query.active_subjects)?;
    validate_unique_roleplay_ids("roleplay lore excluded_subjects", &query.excluded_subjects)?;
    if let Some(query_text) = &query.query_text {
        validate_optional_lore_text("roleplay lore recall query_text", Some(query_text))?;
    }
    Ok(())
}

pub(crate) fn validate_unique_roleplay_ids(label: &str, ids: &[String]) -> CoreResult<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_roleplay_lore_identifier(label, id)?;
        if !seen.insert(id.as_str()) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("{label} contains duplicate id {id}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_lore_recall_trace_query(query: &LoreRecallTraceQuery) -> CoreResult<()> {
    if let Some(session_id) = &query.session_id {
        validate_non_empty_bounded_text("roleplay lore recall session_id", &session_id.0, 256)?;
    }
    if let Some(chat_id) = &query.chat_id {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
    }
    Ok(())
}

fn validate_non_empty_bounded_text(label: &str, value: &str, max_len: usize) -> CoreResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not be empty"),
        ));
    }
    if trimmed.len() > max_len {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be at most {max_len} characters"),
        ));
    }
    if trimmed.contains('\0') {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not contain NUL"),
        ));
    }
    Ok(())
}

fn validate_optional_lore_text(label: &str, value: Option<&str>) -> CoreResult<()> {
    if let Some(value) = value {
        if value.trim().len() > 1_000 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("{label} must be at most 1000 characters"),
            ));
        }
        if value.contains('\0') {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("{label} must not contain NUL"),
            ));
        }
    }
    Ok(())
}

fn validate_positive_u32(label: &str, value: u32) -> CoreResult<()> {
    if value == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be greater than zero"),
        ));
    }
    Ok(())
}

pub(crate) fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bool_to_sql(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn sql_bool(value: i64) -> bool {
    value != 0
}

fn sqlite_like_contains(value: &str) -> String {
    format!("%{}%", escape_sqlite_like(value))
}

fn escape_sqlite_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roleplay_lore_repo_persists_layers_capture_and_recall_trace() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-roleplay-lore-repo-{}-{}.sqlite3",
            std::process::id(),
            "layers-recall"
        ));
        let _ = fs::remove_file(&db_path);
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .create_lore_layer(&RoleplayLoreLayerWrite {
                layer_id: "layer-story".to_string(),
                profile_id: "profile-roleplay".to_string(),
                name: "Story".to_string(),
                description: Some("Story facts".to_string()),
                purpose: RoleplayLoreLayerPurpose::Story,
                write_policy: RoleplayLoreLayerWritePolicy::AutoCapture,
                now: "2026-07-02T02:00:00Z".to_string(),
            })
            .unwrap();
        store
            .set_chat_layers(&RoleplayChatLayersWrite {
                chat_id: "chat-roleplay".to_string(),
                layers: vec![RoleplayChatLayerLink {
                    layer_id: "layer-story".to_string(),
                    priority: 0,
                    enabled: true,
                }],
                now: "2026-07-02T02:01:00Z".to_string(),
            })
            .unwrap();
        let captured = store
            .capture_lore_fact(&RoleplayLoreFactCapture {
                layer_id: "layer-story".to_string(),
                write: lore_write(
                    "lore-moon-gate",
                    "Moon Gate",
                    "The Moon Gate opens during silver tides.",
                    MemoryProposalSource::CaptureProducer,
                ),
                is_constant: false,
                priority: 3,
                capture_reason: Some("observed in chat".to_string()),
            })
            .unwrap();
        assert_eq!(captured.record.record_id, "lore-moon-gate");
        assert_eq!(store.count_rows("module_roleplay_lore_records").unwrap(), 1);

        let recall = store
            .recall_lore(&LoreRecallQuery {
                chat_id: "chat-roleplay".to_string(),
                session_id: Some(SessionId::new("session-roleplay")),
                trace_id: Some("trace-roleplay-1".to_string()),
                query_text: Some("silver tides".to_string()),
                active_subjects: vec![],
                excluded_subjects: vec![],
                token_budget: Some(512),
                record_trace: true,
                now: "2026-07-02T02:02:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(recall.entries.len(), 1);
        assert_eq!(recall.entries[0].record.record_id, "lore-moon-gate");
        assert_eq!(recall.trace.as_ref().unwrap().trace_id, "trace-roleplay-1");
        assert_eq!(
            store
                .count_rows("module_roleplay_lore_recall_traces")
                .unwrap(),
            1
        );

        let provenance = store
            .roleplay_lore_provenance_events("lore-moon-gate")
            .unwrap();
        assert_eq!(provenance.len(), 1);
        assert_eq!(provenance[0].note.as_deref(), Some("observed in chat"));

        drop(store);
        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn roleplay_lore_recall_matches_natural_language_roleplay_prompts() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-roleplay-lore-repo-{}-{}-{}.sqlite3",
            std::process::id(),
            "natural-language-recall",
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&db_path);
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .create_lore_layer(&RoleplayLoreLayerWrite {
                layer_id: "layer-philos".to_string(),
                profile_id: "profile-roleplay".to_string(),
                name: "The World of Philos".to_string(),
                description: Some("Imported setting lore".to_string()),
                purpose: RoleplayLoreLayerPurpose::World,
                write_policy: RoleplayLoreLayerWritePolicy::AutoCapture,
                now: "2026-07-20T00:00:00Z".to_string(),
            })
            .unwrap();
        store
            .set_chat_layers(&RoleplayChatLayersWrite {
                chat_id: "chat-imported".to_string(),
                layers: vec![RoleplayChatLayerLink {
                    layer_id: "layer-philos".to_string(),
                    priority: 0,
                    enabled: true,
                }],
                now: "2026-07-20T00:01:00Z".to_string(),
            })
            .unwrap();
        store
            .capture_lore_fact(&RoleplayLoreFactCapture {
                layer_id: "layer-philos".to_string(),
                write: lore_write(
                    "lore-dream-planet",
                    "The Dream Planet",
                    "Uluru is a young planet covered in flowers and free of Wanderers. Xavier promised to take his knight there if she gave up the dream.",
                    MemoryProposalSource::Import,
                ),
                is_constant: false,
                priority: 0,
                capture_reason: Some("imported from SillyTavern lorebook".to_string()),
            })
            .unwrap();

        let prompt = "The copied library key turns in the lock. If we could leave the court after the tournament, where did you once promise to take me, and why? Continue the scene in Xavier's voice without answering out of character.";
        let recall = store
            .recall_lore(&LoreRecallQuery {
                chat_id: "chat-imported".to_string(),
                session_id: Some(SessionId::new("session-imported")),
                trace_id: Some("trace-natural-language".to_string()),
                query_text: Some(prompt.to_string()),
                active_subjects: vec![],
                excluded_subjects: vec![],
                token_budget: Some(512),
                record_trace: true,
                now: "2026-07-20T00:02:00Z".to_string(),
            })
            .unwrap();

        assert!(
            recall
                .entries
                .iter()
                .any(|entry| entry.record.record_id == "lore-dream-planet"),
            "natural roleplay prose should retrieve the matching imported lore entry"
        );
        let trace = recall.trace.expect("recorded recall trace");
        assert!(trace.entries_considered > 0);
        assert!(trace.entries_returned > 0);
        let sqlite_query = sqlite_lore_recall_fts_query(prompt).expect("searchable terms");
        assert!(sqlite_query.contains("\"promise\"*"));
        assert!(sqlite_query.contains(" OR "));

        drop(store);
        let _ = fs::remove_file(&db_path);
    }

    fn lore_write(
        record_id: &str,
        title: &str,
        body: &str,
        source: MemoryProposalSource,
    ) -> RoleplayLoreWrite {
        RoleplayLoreWrite {
            record_id: record_id.to_string(),
            world_id: "world-roleplay".to_string(),
            entity_id: Some("moon-gate".to_string()),
            session_id: None,
            branch_id: None,
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("lore_entry"),
                version: 1,
            },
            canon_status: RoleplayLoreCanonStatus::Canon,
            visibility: RoleplayLoreVisibility::Public,
            title: title.to_string(),
            body: body.to_string(),
            content: json!({
                "world_id": "world-roleplay",
                "entity_id": "moon-gate",
                "title": title,
                "body": body,
                "canon_status": "canon",
                "visibility": "public",
            }),
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: "wake-roleplay-lore".to_string(),
                label: Some("wake evidence".to_string()),
            }],
            source,
            confidence: 0.91,
            durability_rationale: "Roleplay lore fixture should survive recall.".to_string(),
            supersedes_record_id: None,
            now: "2026-07-02T02:01:00Z".to_string(),
        }
    }
}
