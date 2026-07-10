//! PostgreSQL conversation, message-variant, attachment, and data-bank repositories.

use super::*;
use crate::{ApplyRoleplayAlternativeRequest, ApplyRoleplayAlternativeResult};

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

    pub fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start create PostgreSQL chat attachment", error))?;
        validate_chat_attachment_write(&mut tx, &schema, &request.attachment)?;
        let existing = attachment_session_created_at_in_tx(
            &mut tx,
            &schema,
            &request.attachment.attachment_id,
        )?;
        let mut attachment = request.attachment.clone();
        let status = match existing {
            Some((session_id, created_at)) if session_id == attachment.session_id => {
                attachment.created_at = created_at;
                ChatAttachmentMutationStatus::Updated
            }
            Some((session_id, _)) => {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "attachment {} already belongs to session {} and cannot be written by {}",
                        attachment.attachment_id, session_id, attachment.session_id
                    ),
                ));
            }
            None if attachment.link.is_some() => ChatAttachmentMutationStatus::Linked,
            None => ChatAttachmentMutationStatus::Created,
        };
        save_attachment_in_tx(&mut tx, &schema, &attachment)?;
        let record = load_attachment(&mut tx, &schema, &attachment.attachment_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit create PostgreSQL chat attachment", error))?;
        Ok(CreateChatAttachmentResult {
            status,
            attachment: record,
        })
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

    pub fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL chat attachment", error))?;
        let changed = tx
            .execute(
                &format!(
                    "UPDATE {schema}.attachments
                     SET status = 'removed',
                         updated_at = $3
                     WHERE attachment_id = $1 AND session_id = $2"
                ),
                &[
                    &request.attachment_id.0,
                    &request.session_id.0,
                    &request.updated_at,
                ],
            )
            .map_err(|error| postgres_error("remove PostgreSQL chat attachment", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "attachment {} not found for session {}",
                    request.attachment_id, request.session_id
                ),
            ));
        }
        let record = load_attachment(&mut tx, &schema, &request.attachment_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL chat attachment", error))?;
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

    pub fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start create PostgreSQL chat data-bank scope", error)
        })?;
        let existing =
            data_bank_scope_session_created_at_in_tx(&mut tx, &schema, &request.scope.scope_id)?;
        let mut scope = request.scope.clone();
        let status = match existing {
            Some((session_id, created_at)) if session_id == scope.session_id => {
                scope.created_at = created_at;
                ChatDataBankScopeMutationStatus::Updated
            }
            Some((session_id, _)) => {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "data-bank scope {} already belongs to session {} and cannot be written by {}",
                        scope.scope_id, session_id, scope.session_id
                    ),
                ));
            }
            None => ChatDataBankScopeMutationStatus::Created,
        };
        save_data_bank_scope_in_tx(&mut tx, &schema, &scope)?;
        let record = load_data_bank_scope(&mut tx, &schema, &scope.scope_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit create PostgreSQL chat data-bank scope", error)
        })?;
        Ok(CreateChatDataBankScopeResult {
            status,
            scope: record,
        })
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

    pub fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start remove PostgreSQL chat data-bank scope", error)
        })?;
        let changed = tx
            .execute(
                &format!(
                    "UPDATE {schema}.data_bank_scopes
                     SET status = 'removed',
                         updated_at = $3
                     WHERE scope_id = $1 AND session_id = $2"
                ),
                &[
                    &request.scope_id.0,
                    &request.session_id.0,
                    &request.updated_at,
                ],
            )
            .map_err(|error| postgres_error("remove PostgreSQL chat data-bank scope", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "data-bank scope {} not found for session {}",
                    request.scope_id, request.session_id
                ),
            ));
        }
        let record = load_data_bank_scope(&mut tx, &schema, &request.scope_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit remove PostgreSQL chat data-bank scope", error)
        })?;
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

    pub fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult> {
        validate_create_chat_message_variant_request(request)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start create PostgreSQL chat message variant", error)
        })?;
        ensure_slot_belongs_to_session_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.slot_id,
        )?;
        let mut variant = request.variant.clone();
        variant.ordinal = next_alternate_variant_ordinal_in_tx(&mut tx, &schema, &request.slot_id)?;
        save_message_variant_in_tx(&mut tx, &schema, &variant)?;
        let record = load_message_variant_in_tx(&mut tx, &schema, &variant.variant_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit create PostgreSQL chat message variant", error)
        })?;
        Ok(CreateChatMessageVariantResult { variant: record })
    }

    pub fn apply_roleplay_alternative(
        &self,
        request: &ApplyRoleplayAlternativeRequest,
    ) -> CoreResult<ApplyRoleplayAlternativeResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start apply PostgreSQL roleplay alternative", error)
        })?;
        ensure_slot_belongs_to_session_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.slot_id,
        )?;
        let current = current_active_variant_in_tx(&mut tx, &schema, &request.slot_id)?;
        let expected = match &request.expected {
            ActiveVariantExpectation::Any => current.clone(),
            ActiveVariantExpectation::Primary => None,
            ActiveVariantExpectation::Variant(id) => Some(id.clone()),
        };
        if request.expected != ActiveVariantExpectation::Any && current != expected {
            let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
            return Ok(ApplyRoleplayAlternativeResult {
                created_variant: None,
                slot,
                branch: None,
                conflict: Some(ActiveVariantConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        let created_variant = if let Some(write) = &request.create_variant {
            validate_create_chat_message_variant_request(&CreateChatMessageVariantRequest {
                session_id: request.session_id.clone(),
                slot_id: request.slot_id.clone(),
                variant: write.clone(),
            })?;
            let mut write = write.clone();
            write.ordinal =
                next_alternate_variant_ordinal_in_tx(&mut tx, &schema, &request.slot_id)?;
            save_message_variant_in_tx(&mut tx, &schema, &write)?;
            Some(load_message_variant_in_tx(
                &mut tx,
                &schema,
                &write.variant_id,
            )?)
        } else {
            None
        };
        if let Some(id) = &request.active_variant_id {
            ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, &request.slot_id, id)?;
        }
        tx.execute(&format!("UPDATE {schema}.message_slots SET active_variant_id = $2, updated_at = $3, version = version + 1 WHERE slot_id = $1"), &[&request.slot_id.0, &request.active_variant_id.as_ref().map(|id| id.0.as_str()), &request.updated_at]).map_err(|error| postgres_error("select PostgreSQL roleplay alternative", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
        let active = request
            .active_variant_id
            .as_ref()
            .and_then(|id| {
                slot.alternates
                    .iter()
                    .find(|variant| &variant.variant_id == id)
            })
            .unwrap_or(&slot.primary);
        let branch = if let Some(branch_id) = &active.message.branch_id {
            tx.execute(&format!("UPDATE {schema}.conversation_branches SET head_message_id = $2, updated_at = $3, version = version + 1 WHERE branch_id = $1"), &[&branch_id.0, &active.message.message_id.0, &request.updated_at]).map_err(|error| postgres_error("advance PostgreSQL roleplay alternative branch head", error))?;
            Some(load_conversation_branch_in_tx(&mut tx, &schema, branch_id)?)
        } else {
            None
        };
        tx.commit().map_err(|error| {
            postgres_error("commit apply PostgreSQL roleplay alternative", error)
        })?;
        Ok(ApplyRoleplayAlternativeResult {
            created_variant,
            slot,
            branch,
            conflict: None,
        })
    }

    pub fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start delete PostgreSQL chat message variant", error)
        })?;
        ensure_slot_belongs_to_session_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.slot_id,
        )?;
        ensure_variant_belongs_to_slot_in_tx(
            &mut tx,
            &schema,
            &request.slot_id,
            &request.variant_id,
        )?;
        let changed = tx
            .execute(
                &format!(
                    "UPDATE {schema}.message_variants
                     SET status = 'deleted',
                         updated_at = $3
                     WHERE slot_id = $1
                       AND variant_id = $2
                       AND source <> 'primary'"
                ),
                &[
                    &request.slot_id.0,
                    &request.variant_id.0,
                    &request.updated_at,
                ],
            )
            .map_err(|error| postgres_error("delete PostgreSQL chat message variant", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "message variant {} cannot be deleted because it is the primary variant for slot {}",
                    request.variant_id, request.slot_id
                ),
            ));
        }
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
            &[
                &request.slot_id.0,
                &request.variant_id.0,
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("clear PostgreSQL deleted chat active variant", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
        tx.commit().map_err(|error| {
            postgres_error("commit delete PostgreSQL chat message variant", error)
        })?;
        Ok(slot)
    }

    pub fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start reorder PostgreSQL chat message variants", error)
        })?;
        ensure_slot_belongs_to_session_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.slot_id,
        )?;
        for (index, variant_id) in request.ordered_variant_ids.iter().enumerate() {
            ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, &request.slot_id, variant_id)?;
            let changed = tx
                .execute(
                    &format!(
                        "UPDATE {schema}.message_variants
                         SET ordinal = $3,
                             updated_at = $4
                         WHERE slot_id = $1
                           AND variant_id = $2
                           AND source <> 'primary'"
                    ),
                    &[
                        &request.slot_id.0,
                        &variant_id.0,
                        &(-((index + 1) as i64)),
                        &request.updated_at,
                    ],
                )
                .map_err(|error| {
                    postgres_error("stage PostgreSQL chat message variant reorder", error)
                })?;
            if changed != 1 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!(
                        "message variant {variant_id} cannot be reordered because it is the primary variant for slot {}",
                        request.slot_id
                    ),
                ));
            }
        }
        for (index, variant_id) in request.ordered_variant_ids.iter().enumerate() {
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
                    &request.slot_id.0,
                    &variant_id.0,
                    &((index + 1) as i64),
                    &request.updated_at,
                ],
            )
            .map_err(|error| postgres_error("reorder PostgreSQL chat message variant", error))?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET updated_at = $2,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[&request.slot_id.0, &request.updated_at],
        )
        .map_err(|error| postgres_error("touch PostgreSQL reordered chat message slot", error))?;
        let variants = query_message_variants(
            &mut tx,
            &schema,
            &MessageVariantQuery {
                slot_id: Some(request.slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?;
        tx.commit().map_err(|error| {
            postgres_error("commit reorder PostgreSQL chat message variants", error)
        })?;
        Ok(variants)
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
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL conversation branch", error))?;
        save_conversation_branch_in_tx(&mut tx, &schema, branch)?;
        let record = load_conversation_branch_in_tx(&mut tx, &schema, &branch.branch_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL conversation branch", error))?;
        Ok(record)
    }

    pub fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start create PostgreSQL chat conversation branch", error)
        })?;
        validate_chat_conversation_branch_write(&mut tx, &schema, &request.branch)?;
        save_conversation_branch_in_tx(&mut tx, &schema, &request.branch)?;
        let record = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch.branch_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit create PostgreSQL chat conversation branch", error)
        })?;
        Ok(record)
    }

    pub fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start ensure PostgreSQL active chat branch", error))?;
        let fallback = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)
            .or_else(|error| {
                if error.kind != CoreErrorKind::NotFound {
                    return Err(error);
                }
                let branch = ConversationBranchWrite {
                    branch_id: request.branch_id.clone(),
                    session_id: request.session_id.clone(),
                    parent_branch_id: None,
                    parent_message_id: None,
                    origin_message_id: None,
                    head_message_id: None,
                    label: request.label.clone(),
                    metadata_json: request.metadata_json.clone(),
                    created_at: request.created_at.clone(),
                    updated_at: request.updated_at.clone(),
                };
                save_conversation_branch_in_tx(&mut tx, &schema, &branch)?;
                load_conversation_branch_in_tx(&mut tx, &schema, &branch.branch_id)
            })?;
        if fallback.session_id != request.session_id {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "conversation branch {} not found for session {}",
                    request.branch_id, request.session_id
                ),
            ));
        }
        let current = current_active_branch_in_tx(&mut tx, &schema, &request.session_id)?;
        if current.as_ref() == Some(&request.branch_id) {
            let state = load_conversation_branch_state_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                &request.updated_at,
            )?;
            tx.commit().map_err(|error| {
                postgres_error("commit ensure PostgreSQL active chat branch", error)
            })?;
            return Ok(EnsureActiveChatConversationBranchResult {
                branch: fallback,
                state,
                conflict: None,
            });
        }
        if let Some(active_branch_id) = current {
            let state = load_conversation_branch_state_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                &request.updated_at,
            )?;
            let branch = load_conversation_branch_in_tx(&mut tx, &schema, &active_branch_id)
                .unwrap_or(fallback);
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit ensure PostgreSQL active chat branch conflict",
                    error,
                )
            })?;
            return Ok(EnsureActiveChatConversationBranchResult {
                branch,
                state,
                conflict: Some(ActiveBranchConflict {
                    expected: None,
                    actual: Some(active_branch_id),
                }),
            });
        }
        tx.execute(
            &format!(
                "INSERT INTO {schema}.conversation_branch_state (
                    session_id, active_branch_id, updated_at, version
                 ) VALUES ($1, $2, $3, 0)
                 ON CONFLICT(session_id) DO UPDATE SET
                    active_branch_id = EXCLUDED.active_branch_id,
                    updated_at = EXCLUDED.updated_at,
                    version = conversation_branch_state.version + 1"
            ),
            &[
                &request.session_id.0,
                &request.branch_id.0,
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("select ensured PostgreSQL active chat branch", error))?;
        let state = load_conversation_branch_state_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.updated_at,
        )?;
        tx.commit().map_err(|error| {
            postgres_error("commit ensure PostgreSQL active chat branch", error)
        })?;
        Ok(EnsureActiveChatConversationBranchResult {
            branch: fallback,
            state,
            conflict: None,
        })
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
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start save PostgreSQL conversation snapshot", error)
        })?;
        save_conversation_snapshot_in_tx(&mut tx, &schema, snapshot)?;
        let record = load_conversation_snapshot_in_tx(&mut tx, &schema, &snapshot.snapshot_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit save PostgreSQL conversation snapshot", error)
        })?;
        Ok(record)
    }

    pub fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start create PostgreSQL chat conversation snapshot", error)
        })?;
        validate_chat_conversation_snapshot_write(&mut tx, &schema, &request.snapshot)?;
        let existing = conversation_snapshot_session_created_at_in_tx(
            &mut tx,
            &schema,
            &request.snapshot.snapshot_id,
        )?;
        let mut snapshot = request.snapshot.clone();
        let status = match existing {
            Some((session_id, created_at)) if session_id == snapshot.session_id => {
                snapshot.created_at = created_at;
                ChatConversationSnapshotMutationStatus::Updated
            }
            Some((session_id, _)) => {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "conversation snapshot {} already belongs to session {} and cannot be written by {}",
                        snapshot.snapshot_id, session_id, snapshot.session_id
                    ),
                ));
            }
            None => ChatConversationSnapshotMutationStatus::Created,
        };
        save_conversation_snapshot_in_tx(&mut tx, &schema, &snapshot)?;
        let record = load_conversation_snapshot_in_tx(&mut tx, &schema, &snapshot.snapshot_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit create PostgreSQL chat conversation snapshot", error)
        })?;
        Ok(CreateChatConversationSnapshotResult {
            status,
            snapshot: record,
        })
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

fn save_conversation_branch_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    branch: &ConversationBranchWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&branch.metadata_json)?;
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
    Ok(())
}

fn save_conversation_snapshot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    snapshot: &ConversationSnapshotWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&snapshot.metadata_json)?;
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
    Ok(())
}

fn validate_chat_conversation_branch_write(
    tx: &mut Transaction<'_>,
    schema: &str,
    branch: &ConversationBranchWrite,
) -> CoreResult<()> {
    if let Some(parent_branch_id) = &branch.parent_branch_id {
        ensure_branch_belongs_to_session_in_tx(tx, schema, &branch.session_id, parent_branch_id)?;
    }
    if let Some(parent_message_id) = &branch.parent_message_id {
        ensure_message_belongs_to_session_in_tx(tx, schema, &branch.session_id, parent_message_id)?;
    }
    if let Some(origin_message_id) = &branch.origin_message_id {
        ensure_message_belongs_to_session_in_tx(tx, schema, &branch.session_id, origin_message_id)?;
    }
    if let Some(head_message_id) = &branch.head_message_id {
        ensure_message_belongs_to_session_in_tx(tx, schema, &branch.session_id, head_message_id)?;
    }
    Ok(())
}

fn validate_chat_conversation_snapshot_write(
    tx: &mut Transaction<'_>,
    schema: &str,
    snapshot: &ConversationSnapshotWrite,
) -> CoreResult<()> {
    if let Some(branch_id) = &snapshot.branch_id {
        ensure_branch_belongs_to_session_in_tx(tx, schema, &snapshot.session_id, branch_id)?;
    }
    if let Some(message_id) = &snapshot.message_id {
        ensure_message_belongs_to_session_in_tx(tx, schema, &snapshot.session_id, message_id)?;
    }
    Ok(())
}

fn conversation_snapshot_session_created_at_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<Option<(SessionId, IsoTimestamp)>> {
    tx.query_opt(
        &format!(
            "SELECT session_id, created_at
             FROM {schema}.conversation_snapshots
             WHERE snapshot_id = $1"
        ),
        &[&snapshot_id.0],
    )
    .map_err(|error| {
        postgres_error(
            "load PostgreSQL conversation snapshot session ownership",
            error,
        )
    })
    .map(|row| {
        row.map(|row| {
            (
                SessionId::new(row.get::<_, String>(0)),
                row.get::<_, String>(1),
            )
        })
    })
}

fn ensure_message_belongs_to_session_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
    message_id: &MessageId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.messages
                    WHERE session_id = $1 AND message_id = $2
                )"
            ),
            &[&session_id.0, &message_id.0],
        )
        .map_err(|error| {
            postgres_error("check PostgreSQL durable message session ownership", error)
        })?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message {message_id} not found for session {session_id}"),
        ))
    }
}
