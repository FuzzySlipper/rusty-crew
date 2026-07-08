//! PostgreSQL conversation, message-variant, attachment, and data-bank repositories.

use super::*;

impl PostgresBackendStore {
    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL attachment", error))?;
        save_attachment_in_tx(&mut tx, &schema, attachment)?;
        let record = load_attachment(&mut tx, &schema, &attachment.attachment_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL attachment", error))?;
        Ok(record)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_attachments(&mut *client, &schema, query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL attachment", error))?;
        tx.execute(
            &format!(
                "UPDATE {schema}.attachments
                 SET status = 'removed',
                     updated_at = $2
                 WHERE attachment_id = $1"
            ),
            &[&attachment_id.0, updated_at],
        )
        .map_err(|error| postgres_error("remove PostgreSQL attachment", error))?;
        let record = load_attachment(&mut tx, &schema, attachment_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL attachment", error))?;
        Ok(record)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL data-bank scope", error))?;
        save_data_bank_scope_in_tx(&mut tx, &schema, scope)?;
        let record = load_data_bank_scope(&mut tx, &schema, &scope.scope_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL data-bank scope", error))?;
        Ok(record)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_data_bank_scopes(&mut *client, &schema, query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL data-bank scope", error))?;
        tx.execute(
            &format!(
                "UPDATE {schema}.data_bank_scopes
                 SET status = 'removed',
                     updated_at = $2
                 WHERE scope_id = $1"
            ),
            &[&scope_id.0, updated_at],
        )
        .map_err(|error| postgres_error("remove PostgreSQL data-bank scope", error))?;
        let record = load_data_bank_scope(&mut tx, &schema, scope_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL data-bank scope", error))?;
        Ok(record)
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

    pub fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult> {
        validate_create_chat_message_slot_request(request)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start create PostgreSQL chat message slot", error))?;
        ensure_branch_belongs_to_session_in_tx(
            &mut tx,
            &schema,
            &request.slot.session_id,
            &request.branch_id,
        )?;
        let current = current_branch_head_in_tx(&mut tx, &schema, &request.branch_id)?;
        let expected = match &request.expected_branch_head {
            BranchHeadExpectation::Any => current.clone(),
            BranchHeadExpectation::None => None,
            BranchHeadExpectation::Message(message_id) => Some(message_id.clone()),
        };
        if request.expected_branch_head != BranchHeadExpectation::Any && current != expected {
            let branch = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)?;
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL create chat slot conflict", error)
            })?;
            return Ok(CreateChatMessageSlotResult {
                slot: None,
                branch,
                conflict: Some(BranchHeadConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        save_message_slot_in_tx(&mut tx, &schema, &request.slot)?;
        save_message_variant_in_tx(&mut tx, &schema, &request.primary_variant)?;
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
                &request.primary_variant.message.message_id.0,
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL create chat slot branch head", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot.slot_id, true)?;
        let branch = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit create PostgreSQL chat message slot", error))?;
        Ok(CreateChatMessageSlotResult {
            slot: Some(slot),
            branch,
            conflict: None,
        })
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

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start delete PostgreSQL message variant", error))?;
        ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, slot_id, variant_id)?;
        tx.execute(
            &format!(
                "UPDATE {schema}.message_variants
                 SET status = 'deleted',
                     updated_at = $3
                 WHERE slot_id = $1
                   AND variant_id = $2
                   AND source <> 'primary'"
            ),
            &[&slot_id.0, &variant_id.0, updated_at],
        )
        .map_err(|error| postgres_error("delete PostgreSQL message variant", error))?;
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET active_variant_id = CASE
                        WHEN active_variant_id = $2 THEN NULL
                        ELSE active_variant_id
                     END,
                     updated_at = $3,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[&slot_id.0, &variant_id.0, updated_at],
        )
        .map_err(|error| postgres_error("clear PostgreSQL deleted active variant", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, slot_id, true)?;
        tx.commit()
            .map_err(|error| postgres_error("commit delete PostgreSQL message variant", error))?;
        Ok(slot)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start reorder PostgreSQL message variants", error))?;
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, slot_id, variant_id)?;
            tx.execute(
                &format!(
                    "UPDATE {schema}.message_variants
                     SET ordinal = $3,
                         updated_at = $4
                     WHERE slot_id = $1
                       AND variant_id = $2
                       AND source <> 'primary'"
                ),
                &[
                    &slot_id.0,
                    &variant_id.0,
                    &(-((index + 1) as i64)),
                    updated_at,
                ],
            )
            .map_err(|error| postgres_error("stage PostgreSQL message variant reorder", error))?;
        }
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            tx.execute(
                &format!(
                    "UPDATE {schema}.message_variants
                     SET ordinal = $3,
                         updated_at = $4
                     WHERE slot_id = $1
                       AND variant_id = $2
                       AND source <> 'primary'"
                ),
                &[&slot_id.0, &variant_id.0, &((index + 1) as i64), updated_at],
            )
            .map_err(|error| postgres_error("reorder PostgreSQL message variant", error))?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET updated_at = $2,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[&slot_id.0, updated_at],
        )
        .map_err(|error| postgres_error("touch PostgreSQL reordered message slot", error))?;
        let variants = query_message_variants(
            &mut tx,
            &schema,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit reorder PostgreSQL message variants", error))?;
        Ok(variants)
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
}
