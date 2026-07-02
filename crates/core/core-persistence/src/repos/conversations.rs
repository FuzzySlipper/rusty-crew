use super::super::*;

impl CoordinationStore {
    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save message slot", error))?;
        save_message_slot_in_tx(&tx, slot)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save message slot", error))?;
        Ok(())
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save message variant", error))?;
        save_message_variant_in_tx(&tx, variant)?;
        let record = load_message_variant_in_tx(&tx, &variant.variant_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save message variant", error))?;
        Ok(record)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        let conn = self.conn()?;
        query_message_slots(&conn, query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let conn = self.conn()?;
        query_message_variants(&conn, query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save conversation branch", error))?;
        save_conversation_branch_in_tx(&tx, branch)?;
        let record = load_conversation_branch_in_tx(&tx, &branch.branch_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save conversation branch", error))?;
        Ok(record)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        let conn = self.conn()?;
        query_conversation_branches(&conn, query)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        let conn = self.conn()?;
        load_conversation_branch_state(&conn, session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin select active branch", error))?;
        let current = current_active_branch_in_tx(&tx, &request.session_id)?;
        let expected = match &request.expected {
            ActiveBranchExpectation::Any => current.clone(),
            ActiveBranchExpectation::None => None,
            ActiveBranchExpectation::Branch(branch_id) => Some(branch_id.clone()),
        };
        if request.expected != ActiveBranchExpectation::Any && current != expected {
            let state = load_conversation_branch_state_in_tx(
                &tx,
                &request.session_id,
                &request.updated_at,
            )?;
            tx.commit()
                .map_err(|error| persistence_error("commit active branch conflict", error))?;
            return Ok(SelectActiveBranchResult {
                state,
                conflict: Some(ActiveBranchConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(branch_id) = &request.active_branch_id {
            ensure_branch_belongs_to_session_in_tx(&tx, &request.session_id, branch_id)?;
        }
        tx.execute(
            "INSERT INTO conversation_branch_state (
                session_id, active_branch_id, updated_at, version
             ) VALUES (?1, ?2, ?3, 0)
             ON CONFLICT(session_id) DO UPDATE SET
                active_branch_id = excluded.active_branch_id,
                updated_at = excluded.updated_at,
                version = conversation_branch_state.version + 1",
            params![
                request.session_id.0,
                request
                    .active_branch_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                request.updated_at,
            ],
        )
        .map_err(|error| persistence_error("select active conversation branch", error))?;
        let state =
            load_conversation_branch_state_in_tx(&tx, &request.session_id, &request.updated_at)?;
        tx.commit()
            .map_err(|error| persistence_error("commit select active branch", error))?;
        Ok(SelectActiveBranchResult {
            state,
            conflict: None,
        })
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin update branch head", error))?;
        let current = current_branch_head_in_tx(&tx, &request.branch_id)?;
        let expected = match &request.expected {
            BranchHeadExpectation::Any => current.clone(),
            BranchHeadExpectation::None => None,
            BranchHeadExpectation::Message(message_id) => Some(message_id.clone()),
        };
        if request.expected != BranchHeadExpectation::Any && current != expected {
            let branch = load_conversation_branch_in_tx(&tx, &request.branch_id)?;
            tx.commit()
                .map_err(|error| persistence_error("commit branch head conflict", error))?;
            return Ok(UpdateBranchHeadResult {
                branch,
                conflict: Some(BranchHeadConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(message_id) = &request.head_message_id {
            ensure_message_exists_in_tx(&tx, message_id)?;
        }
        tx.execute(
            "UPDATE conversation_branches
             SET head_message_id = ?2,
                 updated_at = ?3,
                 version = version + 1
             WHERE branch_id = ?1",
            params![
                request.branch_id.0,
                request
                    .head_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                request.updated_at,
            ],
        )
        .map_err(|error| persistence_error("update conversation branch head", error))?;
        let branch = load_conversation_branch_in_tx(&tx, &request.branch_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit update branch head", error))?;
        Ok(UpdateBranchHeadResult {
            branch,
            conflict: None,
        })
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save conversation snapshot", error))?;
        save_conversation_snapshot_in_tx(&tx, snapshot)?;
        let record = load_conversation_snapshot_in_tx(&tx, &snapshot.snapshot_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save conversation snapshot", error))?;
        Ok(record)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        let conn = self.conn()?;
        query_conversation_snapshots(&conn, query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        let conn = self.conn()?;
        resolve_conversation_jump(&conn, request)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin select active message variant", error))?;
        let current = current_active_variant_in_tx(&tx, &request.slot_id)?;
        let expected = match &request.expected {
            ActiveVariantExpectation::Any => current.clone(),
            ActiveVariantExpectation::Primary => None,
            ActiveVariantExpectation::Variant(variant_id) => Some(variant_id.clone()),
        };
        if request.expected != ActiveVariantExpectation::Any && current != expected {
            let slot = load_message_slot_in_tx(&tx, &request.slot_id, true)?;
            tx.commit()
                .map_err(|error| persistence_error("commit active variant conflict", error))?;
            return Ok(SelectActiveVariantResult {
                slot,
                conflict: Some(ActiveVariantConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(variant_id) = &request.active_variant_id {
            ensure_variant_belongs_to_slot_in_tx(&tx, &request.slot_id, variant_id)?;
        }
        tx.execute(
            "UPDATE message_slots
             SET active_variant_id = ?2, updated_at = ?3, version = version + 1
             WHERE slot_id = ?1",
            params![
                request.slot_id.0,
                request
                    .active_variant_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                request.updated_at,
            ],
        )
        .map_err(|error| persistence_error("select active message variant", error))?;
        let slot = load_message_slot_in_tx(&tx, &request.slot_id, true)?;
        tx.commit()
            .map_err(|error| persistence_error("commit select active message variant", error))?;
        Ok(SelectActiveVariantResult {
            slot,
            conflict: None,
        })
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin delete message variant", error))?;
        ensure_variant_belongs_to_slot_in_tx(&tx, slot_id, variant_id)?;
        tx.execute(
            "UPDATE message_variants
             SET status = 'deleted', updated_at = ?3
             WHERE slot_id = ?1 AND variant_id = ?2 AND source <> 'primary'",
            params![slot_id.0, variant_id.0, updated_at],
        )
        .map_err(|error| persistence_error("delete message variant", error))?;
        tx.execute(
            "UPDATE message_slots
             SET active_variant_id = CASE
                    WHEN active_variant_id = ?2 THEN NULL
                    ELSE active_variant_id
                 END,
                 updated_at = ?3,
                 version = version + 1
             WHERE slot_id = ?1",
            params![slot_id.0, variant_id.0, updated_at],
        )
        .map_err(|error| persistence_error("clear deleted active variant", error))?;
        let slot = load_message_slot_in_tx(&tx, slot_id, true)?;
        tx.commit()
            .map_err(|error| persistence_error("commit delete message variant", error))?;
        Ok(slot)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin reorder message variants", error))?;
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            ensure_variant_belongs_to_slot_in_tx(&tx, slot_id, variant_id)?;
            tx.execute(
                "UPDATE message_variants
                 SET ordinal = ?3, updated_at = ?4
                 WHERE slot_id = ?1 AND variant_id = ?2 AND source <> 'primary'",
                params![slot_id.0, variant_id.0, -((index + 1) as i64), updated_at],
            )
            .map_err(|error| persistence_error("stage reorder message variant", error))?;
        }
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            tx.execute(
                "UPDATE message_variants
                 SET ordinal = ?3, updated_at = ?4
                 WHERE slot_id = ?1 AND variant_id = ?2 AND source <> 'primary'",
                params![slot_id.0, variant_id.0, (index + 1) as i64, updated_at],
            )
            .map_err(|error| persistence_error("reorder message variant", error))?;
        }
        tx.execute(
            "UPDATE message_slots
             SET updated_at = ?2, version = version + 1
             WHERE slot_id = ?1",
            params![slot_id.0, updated_at],
        )
        .map_err(|error| persistence_error("touch reordered message slot", error))?;
        let variants = query_message_variants_in_tx(
            &tx,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit reorder message variants", error))?;
        Ok(variants)
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        artifact.validate()?;
        let conn = self.conn()?;
        insert_or_replace_context_compaction_artifact(&conn, artifact)?;
        get_context_compaction_artifact_by_id(&conn, &artifact.artifact_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "saved context compaction artifact was not readable",
            )
        })
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        let conn = self.conn()?;
        list_context_compaction_artifacts(&conn, query)
    }
}

fn save_message_slot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot: &MessageSlotWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO message_slots (
            slot_id,
            session_id,
            primary_variant_id,
            active_variant_id,
            metadata_json,
            created_at,
            updated_at,
            version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
         ON CONFLICT(slot_id) DO UPDATE SET
            session_id = excluded.session_id,
            primary_variant_id = excluded.primary_variant_id,
            active_variant_id = excluded.active_variant_id,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at,
            version = message_slots.version + 1",
        params![
            slot.slot_id.0,
            slot.session_id.0,
            slot.primary_variant_id.0,
            slot.active_variant_id
                .as_ref()
                .map(|value| value.0.as_str()),
            to_json_text(&slot.metadata_json)?,
            slot.created_at,
            slot.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save message slot", error))?;
    Ok(())
}

fn save_message_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    variant: &MessageVariantWrite,
) -> CoreResult<()> {
    if variant.source == MessageVariantSource::Primary && variant.ordinal != 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "primary message variant ordinal must be 0",
        ));
    }
    save_durable_message_in_tx(tx, &variant.message)?;
    tx.execute(
        "INSERT INTO message_variants (
            variant_id,
            slot_id,
            source,
            ordinal,
            status,
            message_id,
            metadata_json,
            created_at,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(variant_id) DO UPDATE SET
            slot_id = excluded.slot_id,
            source = excluded.source,
            ordinal = excluded.ordinal,
            status = excluded.status,
            message_id = excluded.message_id,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            variant.variant_id.0,
            variant.slot_id.0,
            variant.source.as_str(),
            variant.ordinal as i64,
            variant.status.as_str(),
            variant.message.message_id.0,
            to_json_text(&variant.metadata_json)?,
            variant.created_at,
            variant.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save message variant", error))?;
    Ok(())
}

fn save_durable_message_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message: &DurableMessageWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO messages (
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
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(message_id) DO UPDATE SET
            session_id = excluded.session_id,
            branch_id = excluded.branch_id,
            parent_message_id = excluded.parent_message_id,
            previous_message_id = excluded.previous_message_id,
            author_id = excluded.author_id,
            author_role = excluded.author_role,
            status = excluded.status,
            body = excluded.body,
            metadata_json = excluded.metadata_json",
        params![
            message.message_id.0,
            message.session_id.0,
            message.branch_id.as_ref().map(|value| value.0.as_str()),
            message
                .parent_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            message
                .previous_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            message.author_id,
            message.author_role,
            message.status.as_str(),
            message.body,
            to_json_text(&message.metadata_json)?,
            message.created_at,
        ],
    )
    .map_err(|error| persistence_error("save durable message", error))?;
    tx.execute(
        "DELETE FROM message_blocks WHERE message_id = ?1",
        params![message.message_id.0],
    )
    .map_err(|error| persistence_error("replace message blocks", error))?;
    for block in &message.blocks {
        tx.execute(
            "INSERT INTO message_blocks (
                block_id,
                message_id,
                ordinal,
                kind,
                content_json,
                render_policy_json,
                metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                block.block_id.0,
                message.message_id.0,
                block.ordinal as i64,
                block.kind,
                to_json_text(&block.content_json)?,
                block
                    .render_policy_json
                    .as_ref()
                    .map(to_json_text)
                    .transpose()?,
                to_json_text(&block.metadata_json)?,
            ],
        )
        .map_err(|error| persistence_error("save message block", error))?;
    }
    Ok(())
}

fn query_message_slots(
    conn: &Connection,
    query: &MessageSlotQuery,
) -> CoreResult<Vec<MessageSlotRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT slot_id
             FROM message_slots
             WHERE (?1 IS NULL OR session_id = ?1)
             ORDER BY created_at ASC, slot_id ASC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| persistence_error("prepare query message slots", error))?;
    let slot_ids = stmt
        .query_map(params![session_id, limit, offset], |row| {
            Ok(MessageSlotId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| persistence_error("query message slots", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message slot ids", error))?;
    slot_ids
        .iter()
        .map(|slot_id| load_message_slot(conn, slot_id, query.include_alternates))
        .collect()
}

fn query_message_variants(
    conn: &Connection,
    query: &MessageVariantQuery,
) -> CoreResult<Vec<MessageVariantRecord>> {
    let slot_id = query.slot_id.as_ref().map(|value| value.0.as_str());
    let include_deleted = query.include_deleted;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT variant_id
             FROM message_variants
             WHERE (?1 IS NULL OR slot_id = ?1)
               AND (?2 OR status <> 'deleted')
             ORDER BY slot_id ASC, ordinal ASC, variant_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query message variants", error))?;
    let variant_ids = stmt
        .query_map(params![slot_id, include_deleted, limit, offset], |row| {
            Ok(MessageVariantId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| persistence_error("query message variants", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message variant ids", error))?;
    variant_ids
        .iter()
        .map(|variant_id| load_message_variant(conn, variant_id))
        .collect()
}

fn query_message_variants_in_tx(
    tx: &rusqlite::Transaction<'_>,
    query: &MessageVariantQuery,
) -> CoreResult<Vec<MessageVariantRecord>> {
    let slot_id = query.slot_id.as_ref().map(|value| value.0.as_str());
    let include_deleted = query.include_deleted;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = tx
        .prepare(
            "SELECT variant_id
             FROM message_variants
             WHERE (?1 IS NULL OR slot_id = ?1)
               AND (?2 OR status <> 'deleted')
             ORDER BY slot_id ASC, ordinal ASC, variant_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query message variants", error))?;
    let variant_ids = stmt
        .query_map(params![slot_id, include_deleted, limit, offset], |row| {
            Ok(MessageVariantId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| persistence_error("query message variants", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message variant ids", error))?;
    variant_ids
        .iter()
        .map(|variant_id| load_message_variant_in_tx(tx, variant_id))
        .collect()
}

fn save_conversation_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch: &ConversationBranchWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO conversation_branches (
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
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
         ON CONFLICT(branch_id) DO UPDATE SET
            session_id = excluded.session_id,
            parent_branch_id = excluded.parent_branch_id,
            parent_message_id = excluded.parent_message_id,
            origin_message_id = excluded.origin_message_id,
            head_message_id = excluded.head_message_id,
            label = excluded.label,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at,
            version = conversation_branches.version + 1",
        params![
            branch.branch_id.0,
            branch.session_id.0,
            branch
                .parent_branch_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch
                .parent_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch
                .origin_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch
                .head_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch.label,
            to_json_text(&branch.metadata_json)?,
            branch.created_at,
            branch.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save conversation branch", error))?;
    Ok(())
}

fn query_conversation_branches(
    conn: &Connection,
    query: &ConversationBranchQuery,
) -> CoreResult<Vec<ConversationBranchRecord>> {
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
    let mut stmt = conn
        .prepare(
            "SELECT branch_id
             FROM conversation_branches
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR parent_branch_id = ?2)
             ORDER BY created_at ASC, branch_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query conversation branches", error))?;
    let branch_ids = stmt
        .query_map(
            params![session_id, parent_branch_id, limit, offset],
            |row| Ok(ConversationBranchId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query conversation branches", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load conversation branch ids", error))?;
    branch_ids
        .iter()
        .map(|branch_id| load_conversation_branch(conn, branch_id))
        .collect()
}

pub(crate) fn load_conversation_branch(
    conn: &Connection,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    conn.query_row(
        "SELECT session_id, parent_branch_id, parent_message_id, origin_message_id,
                head_message_id, label, metadata_json, created_at, updated_at, version
         FROM conversation_branches
         WHERE branch_id = ?1",
        params![branch_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?.map(MessageId::new),
                row.get::<_, Option<String>>(4)?.map(MessageId::new),
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)? as u64,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation branch", error))?
    .map(
        |(
            session_id,
            parent_branch_id,
            parent_message_id,
            origin_message_id,
            head_message_id,
            label,
            metadata_json,
            created_at,
            updated_at,
            version,
        )| {
            Ok(ConversationBranchRecord {
                branch_id: branch_id.clone(),
                session_id,
                parent_branch_id,
                parent_message_id,
                origin_message_id,
                head_message_id,
                label,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
                version,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found"),
        )
    })
}

fn load_conversation_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    tx.query_row(
        "SELECT session_id, parent_branch_id, parent_message_id, origin_message_id,
                head_message_id, label, metadata_json, created_at, updated_at, version
         FROM conversation_branches
         WHERE branch_id = ?1",
        params![branch_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?.map(MessageId::new),
                row.get::<_, Option<String>>(4)?.map(MessageId::new),
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)? as u64,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation branch in tx", error))?
    .map(
        |(
            session_id,
            parent_branch_id,
            parent_message_id,
            origin_message_id,
            head_message_id,
            label,
            metadata_json,
            created_at,
            updated_at,
            version,
        )| {
            Ok(ConversationBranchRecord {
                branch_id: branch_id.clone(),
                session_id,
                parent_branch_id,
                parent_message_id,
                origin_message_id,
                head_message_id,
                label,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
                version,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found"),
        )
    })
}

fn current_active_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
) -> CoreResult<Option<ConversationBranchId>> {
    tx.query_row(
        "SELECT active_branch_id FROM conversation_branch_state WHERE session_id = ?1",
        params![session_id.0],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load current active branch", error))
    .map(|value| value.flatten().map(ConversationBranchId::new))
}

fn load_conversation_branch_state(
    conn: &Connection,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    Ok(conn
        .query_row(
            "SELECT active_branch_id, updated_at, version
             FROM conversation_branch_state
             WHERE session_id = ?1",
            params![session_id.0],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .map(ConversationBranchId::new),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load conversation branch state", error))?
        .map(
            |(active_branch_id, updated_at, version)| ConversationBranchStateRecord {
                session_id: session_id.clone(),
                active_branch_id,
                updated_at,
                version,
            },
        )
        .unwrap_or_else(|| ConversationBranchStateRecord {
            session_id: session_id.clone(),
            active_branch_id: None,
            updated_at: default_updated_at.clone(),
            version: 0,
        }))
}

fn load_conversation_branch_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    Ok(tx
        .query_row(
            "SELECT active_branch_id, updated_at, version
             FROM conversation_branch_state
             WHERE session_id = ?1",
            params![session_id.0],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .map(ConversationBranchId::new),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load conversation branch state", error))?
        .map(
            |(active_branch_id, updated_at, version)| ConversationBranchStateRecord {
                session_id: session_id.clone(),
                active_branch_id,
                updated_at,
                version,
            },
        )
        .unwrap_or_else(|| ConversationBranchStateRecord {
            session_id: session_id.clone(),
            active_branch_id: None,
            updated_at: default_updated_at.clone(),
            version: 0,
        }))
}

fn current_branch_head_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch_id: &ConversationBranchId,
) -> CoreResult<Option<MessageId>> {
    tx.query_row(
        "SELECT head_message_id FROM conversation_branches WHERE branch_id = ?1",
        params![branch_id.0],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load current branch head", error))?
    .map(|value| value.map(MessageId::new))
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found"),
        )
    })
}

fn ensure_branch_belongs_to_session_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM conversation_branches
                WHERE session_id = ?1 AND branch_id = ?2
             )",
            params![session_id.0, branch_id.0],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check branch session ownership", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found for session {session_id}"),
        ))
    }
}

fn ensure_message_exists_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message_id: &MessageId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE message_id = ?1)",
            params![message_id.0],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check durable message existence", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message {message_id} not found"),
        ))
    }
}

fn save_conversation_snapshot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot: &ConversationSnapshotWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO conversation_snapshots (
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
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(snapshot_id) DO UPDATE SET
            session_id = excluded.session_id,
            branch_id = excluded.branch_id,
            message_id = excluded.message_id,
            cursor = excluded.cursor,
            label = excluded.label,
            summary = excluded.summary,
            source = excluded.source,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            snapshot.snapshot_id.0,
            snapshot.session_id.0,
            snapshot.branch_id.as_ref().map(|value| value.0.as_str()),
            snapshot.message_id.as_ref().map(|value| value.0.as_str()),
            snapshot.cursor,
            snapshot.label,
            snapshot.summary,
            snapshot.source.as_str(),
            to_json_text(&snapshot.metadata_json)?,
            snapshot.created_at,
            snapshot.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save conversation snapshot", error))?;
    Ok(())
}

fn query_conversation_snapshots(
    conn: &Connection,
    query: &ConversationSnapshotQuery,
) -> CoreResult<Vec<ConversationSnapshotRecord>> {
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
    let mut stmt = conn
        .prepare(
            "SELECT snapshot_id
             FROM conversation_snapshots
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR branch_id = ?2)
               AND (?3 IS NULL OR message_id = ?3)
             ORDER BY created_at ASC, snapshot_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query conversation snapshots", error))?;
    let snapshot_ids = stmt
        .query_map(
            params![session_id, branch_id, message_id, limit, offset],
            |row| Ok(ConversationSnapshotId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query conversation snapshots", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load conversation snapshot ids", error))?;
    snapshot_ids
        .iter()
        .map(|snapshot_id| load_conversation_snapshot(conn, snapshot_id))
        .collect()
}

fn load_conversation_snapshot(
    conn: &Connection,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    conn.query_row(
        "SELECT session_id, branch_id, message_id, cursor, label, summary,
                source, metadata_json, created_at, updated_at
         FROM conversation_snapshots
         WHERE snapshot_id = ?1",
        params![snapshot_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation snapshot", error))?
    .map(
        |(
            session_id,
            branch_id,
            message_id,
            cursor,
            label,
            summary,
            source,
            metadata_json,
            created_at,
            updated_at,
        )| {
            Ok(ConversationSnapshotRecord {
                snapshot_id: snapshot_id.clone(),
                session_id,
                branch_id,
                message_id,
                cursor,
                label,
                summary,
                source: ConversationSnapshotSource::parse(&source)?,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation snapshot {snapshot_id} not found"),
        )
    })
}

fn load_conversation_snapshot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    tx.query_row(
        "SELECT session_id, branch_id, message_id, cursor, label, summary,
                source, metadata_json, created_at, updated_at
         FROM conversation_snapshots
         WHERE snapshot_id = ?1",
        params![snapshot_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation snapshot in tx", error))?
    .map(
        |(
            session_id,
            branch_id,
            message_id,
            cursor,
            label,
            summary,
            source,
            metadata_json,
            created_at,
            updated_at,
        )| {
            Ok(ConversationSnapshotRecord {
                snapshot_id: snapshot_id.clone(),
                session_id,
                branch_id,
                message_id,
                cursor,
                label,
                summary,
                source: ConversationSnapshotSource::parse(&source)?,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation snapshot {snapshot_id} not found"),
        )
    })
}

fn resolve_conversation_jump(
    conn: &Connection,
    request: &ConversationJumpRequest,
) -> CoreResult<ConversationJumpResult> {
    match &request.target {
        ConversationJumpTarget::Message { message_id } => {
            let message = load_durable_message(conn, message_id)?;
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
            let branch = load_conversation_branch(conn, branch_id)?;
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
            let snapshot = load_conversation_snapshot(conn, snapshot_id)?;
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

fn load_message_slot(
    conn: &Connection,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    let (session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version) =
        conn.query_row(
            "SELECT session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version
             FROM message_slots
             WHERE slot_id = ?1",
            params![slot_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    MessageVariantId::new(row.get::<_, String>(1)?),
                    row.get::<_, Option<String>>(2)?.map(MessageVariantId::new),
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message slot", error))?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, format!("message slot {slot_id} not found")))?;
    let primary = load_message_variant(conn, &primary_variant_id)?;
    let alternates = if include_alternates {
        query_message_variants(
            conn,
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
        session_id,
        primary_variant_id,
        active_variant_id,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
        version,
        primary,
        alternates,
    })
}

fn load_message_slot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    let (session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version) =
        tx.query_row(
            "SELECT session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version
             FROM message_slots
             WHERE slot_id = ?1",
            params![slot_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    MessageVariantId::new(row.get::<_, String>(1)?),
                    row.get::<_, Option<String>>(2)?.map(MessageVariantId::new),
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message slot in tx", error))?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, format!("message slot {slot_id} not found")))?;
    let primary = load_message_variant_in_tx(tx, &primary_variant_id)?;
    let alternates = if include_alternates {
        query_message_variants_in_tx(
            tx,
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
        session_id,
        primary_variant_id,
        active_variant_id,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
        version,
        primary,
        alternates,
    })
}

fn load_message_variant(
    conn: &Connection,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    let row = conn
        .query_row(
            "SELECT slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at
             FROM message_variants
             WHERE variant_id = ?1",
            params![variant_id.0],
            |row| {
                Ok((
                    MessageSlotId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u32,
                    row.get::<_, String>(3)?,
                    MessageId::new(row.get::<_, String>(4)?),
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message variant", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message variant {variant_id} not found"),
            )
        })?;
    hydrate_message_variant(conn, variant_id, row)
}

fn load_message_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    let row = tx
        .query_row(
            "SELECT slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at
             FROM message_variants
             WHERE variant_id = ?1",
            params![variant_id.0],
            |row| {
                Ok((
                    MessageSlotId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u32,
                    row.get::<_, String>(3)?,
                    MessageId::new(row.get::<_, String>(4)?),
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message variant in tx", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message variant {variant_id} not found"),
            )
        })?;
    hydrate_message_variant_in_tx(tx, variant_id, row)
}

fn hydrate_message_variant(
    conn: &Connection,
    variant_id: &MessageVariantId,
    row: (
        MessageSlotId,
        String,
        u32,
        String,
        MessageId,
        String,
        IsoTimestamp,
        IsoTimestamp,
    ),
) -> CoreResult<MessageVariantRecord> {
    let (slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at) = row;
    Ok(MessageVariantRecord {
        variant_id: variant_id.clone(),
        slot_id,
        source: MessageVariantSource::parse(&source)?,
        ordinal,
        status: MessageVariantStatus::parse(&status)?,
        message: load_durable_message(conn, &message_id)?,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
    })
}

fn hydrate_message_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    variant_id: &MessageVariantId,
    row: (
        MessageSlotId,
        String,
        u32,
        String,
        MessageId,
        String,
        IsoTimestamp,
        IsoTimestamp,
    ),
) -> CoreResult<MessageVariantRecord> {
    let (slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at) = row;
    Ok(MessageVariantRecord {
        variant_id: variant_id.clone(),
        slot_id,
        source: MessageVariantSource::parse(&source)?,
        ordinal,
        status: MessageVariantStatus::parse(&status)?,
        message: load_durable_message_in_tx(tx, &message_id)?,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
    })
}

fn load_durable_message(
    conn: &Connection,
    message_id: &MessageId,
) -> CoreResult<DurableMessageRecord> {
    let (
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status,
        body,
        metadata_json,
        created_at,
    ) = conn
        .query_row(
            "SELECT session_id, branch_id, parent_message_id, previous_message_id,
                    author_id, author_role, status, body, metadata_json, created_at
             FROM messages
             WHERE message_id = ?1",
            params![message_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, Option<String>>(1)?
                        .map(ConversationBranchId::new),
                    row.get::<_, Option<String>>(2)?.map(MessageId::new),
                    row.get::<_, Option<String>>(3)?.map(MessageId::new),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load durable message", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message {message_id} not found"),
            )
        })?;
    Ok(DurableMessageRecord {
        message_id: message_id.clone(),
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status: DurableMessageStatus::parse(&status)?,
        body,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        blocks: load_message_blocks(conn, message_id)?,
    })
}

fn load_durable_message_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message_id: &MessageId,
) -> CoreResult<DurableMessageRecord> {
    let (
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status,
        body,
        metadata_json,
        created_at,
    ) = tx
        .query_row(
            "SELECT session_id, branch_id, parent_message_id, previous_message_id,
                    author_id, author_role, status, body, metadata_json, created_at
             FROM messages
             WHERE message_id = ?1",
            params![message_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, Option<String>>(1)?
                        .map(ConversationBranchId::new),
                    row.get::<_, Option<String>>(2)?.map(MessageId::new),
                    row.get::<_, Option<String>>(3)?.map(MessageId::new),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load durable message in tx", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message {message_id} not found"),
            )
        })?;
    Ok(DurableMessageRecord {
        message_id: message_id.clone(),
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status: DurableMessageStatus::parse(&status)?,
        body,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        blocks: load_message_blocks_in_tx(tx, message_id)?,
    })
}

fn load_message_blocks(
    conn: &Connection,
    message_id: &MessageId,
) -> CoreResult<Vec<MessageBlockRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT block_id, ordinal, kind, content_json, render_policy_json, metadata_json
             FROM message_blocks
             WHERE message_id = ?1
             ORDER BY ordinal ASC, block_id ASC",
        )
        .map_err(|error| persistence_error("prepare load message blocks", error))?;
    let rows = stmt
        .query_map(params![message_id.0], |row| {
            row_to_message_block(row, message_id)
        })
        .map_err(|error| persistence_error("query message blocks", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message blocks", error))
}

fn load_message_blocks_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message_id: &MessageId,
) -> CoreResult<Vec<MessageBlockRecord>> {
    let mut stmt = tx
        .prepare(
            "SELECT block_id, ordinal, kind, content_json, render_policy_json, metadata_json
             FROM message_blocks
             WHERE message_id = ?1
             ORDER BY ordinal ASC, block_id ASC",
        )
        .map_err(|error| persistence_error("prepare load message blocks in tx", error))?;
    let rows = stmt
        .query_map(params![message_id.0], |row| {
            row_to_message_block(row, message_id)
        })
        .map_err(|error| persistence_error("query message blocks in tx", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message blocks in tx", error))
}

fn row_to_message_block(
    row: &rusqlite::Row<'_>,
    message_id: &MessageId,
) -> rusqlite::Result<MessageBlockRecord> {
    let content_json: String = row.get(3)?;
    let render_policy_json: Option<String> = row.get(4)?;
    let metadata_json: String = row.get(5)?;
    Ok(MessageBlockRecord {
        block_id: MessageBlockId::new(row.get::<_, String>(0)?),
        message_id: message_id.clone(),
        ordinal: row.get::<_, i64>(1)? as u32,
        kind: row.get(2)?,
        content_json: from_json_text(&content_json).map_err(to_sql_error)?,
        render_policy_json: render_policy_json
            .as_deref()
            .map(from_json_text)
            .transpose()
            .map_err(to_sql_error)?,
        metadata_json: from_json_text(&metadata_json).map_err(to_sql_error)?,
    })
}

fn current_active_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot_id: &MessageSlotId,
) -> CoreResult<Option<MessageVariantId>> {
    tx.query_row(
        "SELECT active_variant_id FROM message_slots WHERE slot_id = ?1",
        params![slot_id.0],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load active message variant", error))?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("message slot {slot_id} not found"),
        )
    })
    .map(|value| value.map(MessageVariantId::new))
}

fn ensure_variant_belongs_to_slot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot_id: &MessageSlotId,
    variant_id: &MessageVariantId,
) -> CoreResult<()> {
    let exists = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM message_variants
                WHERE slot_id = ?1 AND variant_id = ?2 AND status <> 'deleted'
            )",
            params![slot_id.0, variant_id.0],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("check message variant slot", error))?
        != 0;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message variant {variant_id} not found in slot {slot_id}"),
        ))
    }
}

fn insert_or_replace_context_compaction_artifact(
    conn: &Connection,
    artifact: &ContextCompactionArtifact,
) -> CoreResult<()> {
    artifact.validate()?;
    conn.execute(
        "INSERT INTO context_compaction_artifacts (
            artifact_id,
            session_id,
            branch_id,
            strategy_id,
            source_refs_json,
            provider_metadata_json,
            estimate_before_json,
            estimate_after_json,
            summary_text,
            enters_future_context,
            context_policy,
            metadata_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(artifact_id) DO UPDATE SET
            session_id = excluded.session_id,
            branch_id = excluded.branch_id,
            strategy_id = excluded.strategy_id,
            source_refs_json = excluded.source_refs_json,
            provider_metadata_json = excluded.provider_metadata_json,
            estimate_before_json = excluded.estimate_before_json,
            estimate_after_json = excluded.estimate_after_json,
            summary_text = excluded.summary_text,
            enters_future_context = excluded.enters_future_context,
            context_policy = excluded.context_policy,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            artifact.artifact_id.as_str(),
            artifact.session_id.0.as_str(),
            artifact.branch_id.as_ref().map(|id| id.0.as_str()),
            artifact.strategy_id.as_str(),
            to_json_text(&artifact.source_refs_json)?,
            to_json_text(&artifact.provider_metadata_json)?,
            to_json_text(&artifact.estimate_before_json)?,
            artifact
                .estimate_after_json
                .as_ref()
                .map(to_json_text)
                .transpose()?,
            artifact.summary_text.as_str(),
            if artifact.enters_future_context {
                1_i64
            } else {
                0_i64
            },
            artifact.context_policy.as_str(),
            to_json_text(&artifact.metadata_json)?,
            artifact.created_at.as_str(),
            artifact.updated_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert context compaction artifact", error))?;
    Ok(())
}

fn get_context_compaction_artifact_by_id(
    conn: &Connection,
    artifact_id: &str,
) -> CoreResult<Option<ContextCompactionArtifact>> {
    conn.query_row(
        "SELECT artifact_id,
                session_id,
                branch_id,
                strategy_id,
                source_refs_json,
                provider_metadata_json,
                estimate_before_json,
                estimate_after_json,
                summary_text,
                enters_future_context,
                context_policy,
                metadata_json,
                created_at,
                updated_at
         FROM context_compaction_artifacts
         WHERE artifact_id = ?1",
        params![artifact_id],
        row_to_context_compaction_artifact,
    )
    .optional()
    .map_err(|error| persistence_error("get context compaction artifact", error))
}

fn list_context_compaction_artifacts(
    conn: &Connection,
    query: &ContextCompactionArtifactQuery,
) -> CoreResult<Vec<ContextCompactionArtifact>> {
    let (limit, offset) = QueryPage {
        limit: if query.latest_only {
            Some(1)
        } else {
            query.limit
        },
        offset: query.offset,
    }
    .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT artifact_id,
                    session_id,
                    branch_id,
                    strategy_id,
                    source_refs_json,
                    provider_metadata_json,
                    estimate_before_json,
                    estimate_after_json,
                    summary_text,
                    enters_future_context,
                    context_policy,
                    metadata_json,
                    created_at,
                    updated_at
             FROM context_compaction_artifacts
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR branch_id = ?2)
               AND (?3 IS NULL OR strategy_id = ?3)
               AND (?4 IS NULL OR enters_future_context = ?4)
             ORDER BY created_at DESC, artifact_id ASC
             LIMIT ?5 OFFSET ?6",
        )
        .map_err(|error| persistence_error("prepare list context compaction artifacts", error))?;
    let enters_future_context = query
        .enters_future_context
        .map(|value| if value { 1_i64 } else { 0_i64 });
    let rows = stmt
        .query_map(
            params![
                query.session_id.as_ref().map(|id| id.0.as_str()),
                query.branch_id.as_ref().map(|id| id.0.as_str()),
                query.strategy_id.as_deref(),
                enters_future_context,
                limit,
                offset,
            ],
            row_to_context_compaction_artifact,
        )
        .map_err(|error| persistence_error("query context compaction artifacts", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load context compaction artifacts", error))
}

fn row_to_context_compaction_artifact(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ContextCompactionArtifact> {
    let source_refs_json: String = row.get(4)?;
    let provider_metadata_json: String = row.get(5)?;
    let estimate_before_json: String = row.get(6)?;
    let estimate_after_json: Option<String> = row.get(7)?;
    let enters_future_context: i64 = row.get(9)?;
    let artifact = ContextCompactionArtifact {
        artifact_id: row.get(0)?,
        session_id: SessionId(row.get(1)?),
        branch_id: row.get::<_, Option<String>>(2)?.map(ConversationBranchId),
        strategy_id: row.get(3)?,
        source_refs_json: from_json_text(&source_refs_json).map_err(to_sql_error)?,
        provider_metadata_json: from_json_text(&provider_metadata_json).map_err(to_sql_error)?,
        estimate_before_json: from_json_text(&estimate_before_json).map_err(to_sql_error)?,
        estimate_after_json: estimate_after_json
            .map(|text| from_json_text(&text).map_err(to_sql_error))
            .transpose()?,
        summary_text: row.get(8)?,
        enters_future_context: enters_future_context != 0,
        context_policy: row.get(10)?,
        metadata_json: from_json_text(&row.get::<_, String>(11)?).map_err(to_sql_error)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    };
    artifact.validate().map_err(to_sql_core_error)?;
    Ok(artifact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conversations_repo_persists_variant_and_branch_conflicts_across_reopen() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-conversations-repo-{}-{}.sqlite3",
            std::process::id(),
            "conflicts"
        ));
        let _ = fs::remove_file(&db_path);
        let session_id = SessionId::new("session-conversations-repo");
        let root_branch = ConversationBranchId::new("branch-conversations-root");
        let child_branch = ConversationBranchId::new("branch-conversations-child");
        let slot_id = MessageSlotId::new("slot-conversations-repo");
        let primary_variant_id = MessageVariantId::new("variant-conversations-primary");
        let alternate_variant_id = MessageVariantId::new("variant-conversations-alt");
        let root_message_id = MessageId::new("message-conversations-root");

        {
            let store = CoordinationStore::open_file(&db_path).unwrap();
            store
                .save_conversation_branch(&ConversationBranchWrite {
                    branch_id: root_branch.clone(),
                    session_id: session_id.clone(),
                    parent_branch_id: None,
                    parent_message_id: None,
                    origin_message_id: None,
                    head_message_id: Some(root_message_id.clone()),
                    label: Some("Root".to_string()),
                    metadata_json: json!({"fixture": "conversations_repo"}),
                    created_at: "2026-07-02T00:00:00Z".to_string(),
                    updated_at: "2026-07-02T00:00:00Z".to_string(),
                })
                .unwrap();
            store
                .save_message_slot(&MessageSlotWrite {
                    slot_id: slot_id.clone(),
                    session_id: session_id.clone(),
                    primary_variant_id: primary_variant_id.clone(),
                    active_variant_id: None,
                    metadata_json: json!({"fixture": "conversations_repo"}),
                    created_at: "2026-07-02T00:00:00Z".to_string(),
                    updated_at: "2026-07-02T00:00:00Z".to_string(),
                })
                .unwrap();
            let mut primary = variant_write(
                &session_id,
                &slot_id,
                &primary_variant_id,
                MessageVariantSource::Primary,
                0,
                &root_message_id.0,
                "primary transcript body",
            );
            primary.message.branch_id = Some(root_branch.clone());
            store.save_message_variant(&primary).unwrap();
            store
                .save_message_variant(&variant_write(
                    &session_id,
                    &slot_id,
                    &alternate_variant_id,
                    MessageVariantSource::Alternate,
                    1,
                    "message-conversations-alt",
                    "alternate transcript body",
                ))
                .unwrap();
            store
                .save_conversation_branch(&ConversationBranchWrite {
                    branch_id: child_branch.clone(),
                    session_id: session_id.clone(),
                    parent_branch_id: Some(root_branch.clone()),
                    parent_message_id: Some(root_message_id.clone()),
                    origin_message_id: Some(root_message_id.clone()),
                    head_message_id: None,
                    label: Some("Child".to_string()),
                    metadata_json: json!({}),
                    created_at: "2026-07-02T00:01:00Z".to_string(),
                    updated_at: "2026-07-02T00:01:00Z".to_string(),
                })
                .unwrap();
            let selected_variant = store
                .select_active_message_variant(&SelectActiveVariantRequest {
                    slot_id: slot_id.clone(),
                    active_variant_id: Some(alternate_variant_id.clone()),
                    expected: ActiveVariantExpectation::Primary,
                    updated_at: "2026-07-02T00:02:00Z".to_string(),
                })
                .unwrap();
            assert!(selected_variant.conflict.is_none());
            let selected_branch = store
                .select_active_conversation_branch(&SelectActiveBranchRequest {
                    session_id: session_id.clone(),
                    active_branch_id: Some(child_branch.clone()),
                    expected: ActiveBranchExpectation::None,
                    updated_at: "2026-07-02T00:03:00Z".to_string(),
                })
                .unwrap();
            assert!(selected_branch.conflict.is_none());
        }

        let store = CoordinationStore::open_file(&db_path).unwrap();
        let slots = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(session_id.clone()),
                include_alternates: true,
                page: None,
            })
            .unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].active_variant_id, Some(alternate_variant_id));
        assert_eq!(slots[0].alternates.len(), 1);
        let branch_state = store
            .get_conversation_branch_state(&session_id, &"2026-07-02T00:04:00Z".to_string())
            .unwrap();
        assert_eq!(branch_state.active_branch_id, Some(child_branch.clone()));
        let branch_conflict = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id,
                active_branch_id: Some(root_branch),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-07-02T00:05:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(branch_conflict.conflict.unwrap().actual, Some(child_branch));

        drop(store);
        let _ = fs::remove_file(&db_path);
    }

    fn variant_write(
        session_id: &SessionId,
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
                session_id: session_id.clone(),
                branch_id: None,
                parent_message_id: None,
                previous_message_id: None,
                author_id: "agent-alpha".to_string(),
                author_role: "assistant".to_string(),
                status: DurableMessageStatus::Completed,
                body: body.to_string(),
                metadata_json: json!({"provider": "fixture"}),
                created_at: "2026-07-02T00:00:00Z".to_string(),
                blocks: vec![MessageBlockWrite {
                    block_id: MessageBlockId::new(format!("{message_id}:block-1")),
                    ordinal: 0,
                    kind: "text".to_string(),
                    content_json: json!({"text": body}),
                    render_policy_json: None,
                    metadata_json: json!({}),
                }],
            },
            metadata_json: json!({}),
            created_at: "2026-07-02T00:00:00Z".to_string(),
            updated_at: "2026-07-02T00:00:00Z".to_string(),
        }
    }
}
