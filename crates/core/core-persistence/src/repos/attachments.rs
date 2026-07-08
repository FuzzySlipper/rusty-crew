use super::super::*;

impl CoordinationStore {
    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save attachment", error))?;
        save_attachment_in_tx(&tx, attachment)?;
        let record = load_attachment_in_tx(&tx, &attachment.attachment_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save attachment", error))?;
        Ok(record)
    }

    pub fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin create chat attachment", error))?;
        validate_chat_attachment_write(&tx, &request.attachment)?;
        let existing = attachment_session_created_at_in_tx(&tx, &request.attachment.attachment_id)?;
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
        save_attachment_in_tx(&tx, &attachment)?;
        let record = load_attachment_in_tx(&tx, &attachment.attachment_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit create chat attachment", error))?;
        Ok(CreateChatAttachmentResult {
            status,
            attachment: record,
        })
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        let conn = self.conn()?;
        query_attachments(&conn, query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin remove attachment", error))?;
        tx.execute(
            "UPDATE attachments
             SET status = 'removed', updated_at = ?2
             WHERE attachment_id = ?1",
            params![attachment_id.0.as_str(), updated_at],
        )
        .map_err(|error| persistence_error("remove attachment", error))?;
        let record = load_attachment_in_tx(&tx, attachment_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove attachment", error))?;
        Ok(record)
    }

    pub fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin remove chat attachment", error))?;
        let changed = tx
            .execute(
                "UPDATE attachments
                 SET status = 'removed', updated_at = ?3
                 WHERE attachment_id = ?1 AND session_id = ?2",
                params![
                    request.attachment_id.0.as_str(),
                    request.session_id.0.as_str(),
                    request.updated_at,
                ],
            )
            .map_err(|error| persistence_error("remove chat attachment", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "attachment {} not found for session {}",
                    request.attachment_id, request.session_id
                ),
            ));
        }
        let record = load_attachment_in_tx(&tx, &request.attachment_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove chat attachment", error))?;
        Ok(record)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save data-bank scope", error))?;
        save_data_bank_scope_in_tx(&tx, scope)?;
        let record = load_data_bank_scope_in_tx(&tx, &scope.scope_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save data-bank scope", error))?;
        Ok(record)
    }

    pub fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin create chat data-bank scope", error))?;
        let existing = data_bank_scope_session_created_at_in_tx(&tx, &request.scope.scope_id)?;
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
        save_data_bank_scope_in_tx(&tx, &scope)?;
        let record = load_data_bank_scope_in_tx(&tx, &scope.scope_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit create chat data-bank scope", error))?;
        Ok(CreateChatDataBankScopeResult {
            status,
            scope: record,
        })
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        let conn = self.conn()?;
        query_data_bank_scopes(&conn, query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin remove data-bank scope", error))?;
        tx.execute(
            "UPDATE data_bank_scopes
             SET status = 'removed', updated_at = ?2
             WHERE scope_id = ?1",
            params![scope_id.0.as_str(), updated_at],
        )
        .map_err(|error| persistence_error("remove data-bank scope", error))?;
        let record = load_data_bank_scope_in_tx(&tx, scope_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove data-bank scope", error))?;
        Ok(record)
    }

    pub fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin remove chat data-bank scope", error))?;
        let changed = tx
            .execute(
                "UPDATE data_bank_scopes
                 SET status = 'removed', updated_at = ?3
                 WHERE scope_id = ?1 AND session_id = ?2",
                params![
                    request.scope_id.0.as_str(),
                    request.session_id.0.as_str(),
                    request.updated_at,
                ],
            )
            .map_err(|error| persistence_error("remove chat data-bank scope", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "data-bank scope {} not found for session {}",
                    request.scope_id, request.session_id
                ),
            ));
        }
        let record = load_data_bank_scope_in_tx(&tx, &request.scope_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove chat data-bank scope", error))?;
        Ok(record)
    }
}

type AttachmentRow = (
    SessionId,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    String,
    String,
    String,
    Option<String>,
);

fn save_attachment_in_tx(
    tx: &rusqlite::Transaction<'_>,
    attachment: &AttachmentWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO attachments (
            attachment_id,
            session_id,
            status,
            filename,
            mime_type,
            byte_size,
            storage_url,
            download_url,
            thumbnail_url,
            extracted_text,
            extracted_text_truncated,
            metadata_json,
            created_at,
            updated_at,
            expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(attachment_id) DO UPDATE SET
            session_id = excluded.session_id,
            status = excluded.status,
            filename = excluded.filename,
            mime_type = excluded.mime_type,
            byte_size = excluded.byte_size,
            storage_url = excluded.storage_url,
            download_url = excluded.download_url,
            thumbnail_url = excluded.thumbnail_url,
            extracted_text = excluded.extracted_text,
            extracted_text_truncated = excluded.extracted_text_truncated,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at,
            expires_at = excluded.expires_at",
        params![
            attachment.attachment_id.0,
            attachment.session_id.0,
            attachment.status.as_str(),
            attachment.filename,
            attachment.mime_type,
            attachment.byte_size as i64,
            attachment.storage_url,
            attachment.download_url,
            attachment.thumbnail_url,
            attachment.extracted_text,
            attachment.extracted_text_truncated,
            to_json_text(&attachment.metadata_json)?,
            attachment.created_at,
            attachment.updated_at,
            attachment.expires_at,
        ],
    )
    .map_err(|error| persistence_error("save attachment", error))?;
    if let Some(link) = &attachment.link {
        save_attachment_link_in_tx(tx, link)?;
    }
    Ok(())
}

fn save_attachment_link_in_tx(
    tx: &rusqlite::Transaction<'_>,
    link: &AttachmentLinkWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO attachment_links (
            link_id,
            attachment_id,
            session_id,
            message_id,
            block_id,
            scope_id,
            metadata_json,
            created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(link_id) DO UPDATE SET
            attachment_id = excluded.attachment_id,
            session_id = excluded.session_id,
            message_id = excluded.message_id,
            block_id = excluded.block_id,
            scope_id = excluded.scope_id,
            metadata_json = excluded.metadata_json",
        params![
            link.link_id.0,
            link.attachment_id.0,
            link.session_id.0,
            link.message_id.as_ref().map(|value| value.0.as_str()),
            link.block_id.as_ref().map(|value| value.0.as_str()),
            link.scope_id.as_ref().map(|value| value.0.as_str()),
            to_json_text(&link.metadata_json)?,
            link.created_at,
        ],
    )
    .map_err(|error| persistence_error("save attachment link", error))?;
    Ok(())
}

fn validate_chat_attachment_write(
    tx: &rusqlite::Transaction<'_>,
    attachment: &AttachmentWrite,
) -> CoreResult<()> {
    if let Some(link) = &attachment.link {
        if link.attachment_id != attachment.attachment_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "attachment link {} targets {} but request writes {}",
                    link.link_id, link.attachment_id, attachment.attachment_id
                ),
            ));
        }
        if link.session_id != attachment.session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "attachment link {} session {} does not match attachment session {}",
                    link.link_id, link.session_id, attachment.session_id
                ),
            ));
        }
        if let Some(message_id) = &link.message_id {
            ensure_attachment_message_belongs_to_session_in_tx(
                tx,
                &attachment.session_id,
                message_id,
            )?;
        }
        if let Some(block_id) = &link.block_id {
            ensure_attachment_block_belongs_to_session_in_tx(
                tx,
                &attachment.session_id,
                link.message_id.as_ref(),
                block_id,
            )?;
        }
        if let Some(scope_id) = &link.scope_id {
            ensure_attachment_scope_belongs_to_session_in_tx(tx, &attachment.session_id, scope_id)?;
        }
    }
    Ok(())
}

fn attachment_session_created_at_in_tx(
    tx: &rusqlite::Transaction<'_>,
    attachment_id: &AttachmentId,
) -> CoreResult<Option<(SessionId, IsoTimestamp)>> {
    tx.query_row(
        "SELECT session_id, created_at
         FROM attachments
         WHERE attachment_id = ?1",
        params![attachment_id.0.as_str()],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load attachment session ownership", error))
}

fn ensure_attachment_message_belongs_to_session_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    message_id: &MessageId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM messages
                WHERE session_id = ?1 AND message_id = ?2
            )",
            params![session_id.0.as_str(), message_id.0.as_str()],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check attachment message ownership", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message {message_id} not found for session {session_id}"),
        ))
    }
}

fn ensure_attachment_block_belongs_to_session_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    message_id: Option<&MessageId>,
    block_id: &MessageBlockId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM message_blocks b
                JOIN messages m ON m.message_id = b.message_id
                WHERE m.session_id = ?1
                  AND b.block_id = ?2
                  AND (?3 IS NULL OR b.message_id = ?3)
            )",
            params![
                session_id.0.as_str(),
                block_id.0.as_str(),
                message_id.map(|value| value.0.as_str()),
            ],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check attachment block ownership", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message block {block_id} not found for session {session_id}"),
        ))
    }
}

fn ensure_attachment_scope_belongs_to_session_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    scope_id: &DataBankScopeId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM data_bank_scopes
                WHERE session_id = ?1 AND scope_id = ?2
            )",
            params![session_id.0.as_str(), scope_id.0.as_str()],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check attachment scope ownership", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("data-bank scope {scope_id} not found for session {session_id}"),
        ))
    }
}

fn query_attachments(
    conn: &Connection,
    query: &AttachmentQuery,
) -> CoreResult<Vec<AttachmentRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let message_id = query.message_id.as_ref().map(|value| value.0.as_str());
    let block_id = query.block_id.as_ref().map(|value| value.0.as_str());
    let scope_id = query.scope_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.map(AttachmentStatus::as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT a.attachment_id
             FROM attachments a
             LEFT JOIN attachment_links l ON l.attachment_id = a.attachment_id
             WHERE (?1 IS NULL OR a.session_id = ?1)
               AND (?2 OR a.status <> 'removed')
               AND (?3 IS NULL OR l.message_id = ?3)
               AND (?4 IS NULL OR l.scope_id = ?4)
               AND (?5 IS NULL OR l.block_id = ?5)
               AND (?6 IS NULL OR a.status = ?6)
               AND (
                    (?7 AND a.expires_at IS NOT NULL AND ?8 IS NOT NULL AND a.expires_at <= ?8)
                    OR
                    (NOT ?7 AND (?9 OR a.expires_at IS NULL OR ?8 IS NULL OR a.expires_at > ?8))
               )
             ORDER BY a.created_at ASC, a.attachment_id ASC
             LIMIT ?10 OFFSET ?11",
        )
        .map_err(|error| persistence_error("prepare query attachments", error))?;
    let attachment_ids = stmt
        .query_map(
            params![
                session_id,
                query.include_removed,
                message_id,
                scope_id,
                block_id,
                status,
                query.expired_only,
                query.now,
                query.include_expired,
                limit,
                offset,
            ],
            |row| Ok(AttachmentId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query attachments", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load attachment ids", error))?;
    attachment_ids
        .iter()
        .map(|attachment_id| load_attachment(conn, attachment_id))
        .collect()
}

fn load_attachment(
    conn: &Connection,
    attachment_id: &AttachmentId,
) -> CoreResult<AttachmentRecord> {
    let record = conn
        .query_row(
            "SELECT session_id, status, filename, mime_type, byte_size,
                    storage_url, download_url, thumbnail_url, extracted_text,
                    extracted_text_truncated, metadata_json, created_at, updated_at, expires_at
             FROM attachments
             WHERE attachment_id = ?1",
            params![attachment_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load attachment", error))?;
    attachment_record_from_row(conn, attachment_id, record)
}

fn load_attachment_in_tx(
    tx: &rusqlite::Transaction<'_>,
    attachment_id: &AttachmentId,
) -> CoreResult<AttachmentRecord> {
    let record = tx
        .query_row(
            "SELECT session_id, status, filename, mime_type, byte_size,
                    storage_url, download_url, thumbnail_url, extracted_text,
                    extracted_text_truncated, metadata_json, created_at, updated_at, expires_at
             FROM attachments
             WHERE attachment_id = ?1",
            params![attachment_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load attachment in tx", error))?;
    attachment_record_from_row(tx, attachment_id, record)
}

fn attachment_record_from_row(
    conn: &Connection,
    attachment_id: &AttachmentId,
    record: Option<AttachmentRow>,
) -> CoreResult<AttachmentRecord> {
    record
        .map(
            |(
                session_id,
                status,
                filename,
                mime_type,
                byte_size,
                storage_url,
                download_url,
                thumbnail_url,
                extracted_text,
                extracted_text_truncated,
                metadata_json,
                created_at,
                updated_at,
                expires_at,
            )| {
                Ok(AttachmentRecord {
                    attachment_id: attachment_id.clone(),
                    session_id,
                    status: AttachmentStatus::parse(&status)?,
                    filename,
                    mime_type,
                    byte_size: byte_size.max(0) as u64,
                    storage_url,
                    download_url,
                    thumbnail_url,
                    extracted_text,
                    extracted_text_truncated,
                    metadata_json: parse_json_record(&metadata_json)?,
                    created_at,
                    updated_at,
                    expires_at,
                    links: load_attachment_links(conn, attachment_id)?,
                })
            },
        )
        .transpose()?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("attachment {attachment_id} not found"),
            )
        })
}

fn load_attachment_links(
    conn: &Connection,
    attachment_id: &AttachmentId,
) -> CoreResult<Vec<AttachmentLinkRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT link_id, session_id, message_id, block_id, scope_id,
                    metadata_json, created_at
             FROM attachment_links
             WHERE attachment_id = ?1
             ORDER BY created_at ASC, link_id ASC",
        )
        .map_err(|error| persistence_error("prepare load attachment links", error))?;
    let links = stmt
        .query_map(params![attachment_id.0], |row| {
            Ok((
                AttachmentLinkId::new(row.get::<_, String>(0)?),
                SessionId::new(row.get::<_, String>(1)?),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?.map(MessageBlockId::new),
                row.get::<_, Option<String>>(4)?.map(DataBankScopeId::new),
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|error| persistence_error("query attachment links", error))?
        .map(|row| {
            let (link_id, session_id, message_id, block_id, scope_id, metadata_json, created_at) =
                row.map_err(|error| persistence_error("load attachment link", error))?;
            Ok(AttachmentLinkRecord {
                link_id,
                attachment_id: attachment_id.clone(),
                session_id,
                message_id,
                block_id,
                scope_id,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(links)
}

fn save_data_bank_scope_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope: &DataBankScopeWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO data_bank_scopes (
            scope_id,
            session_id,
            status,
            label,
            description,
            metadata_json,
            created_at,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(scope_id) DO UPDATE SET
            session_id = excluded.session_id,
            status = excluded.status,
            label = excluded.label,
            description = excluded.description,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            scope.scope_id.0,
            scope.session_id.0,
            scope.status.as_str(),
            scope.label,
            scope.description,
            to_json_text(&scope.metadata_json)?,
            scope.created_at,
            scope.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save data-bank scope", error))?;
    Ok(())
}

fn data_bank_scope_session_created_at_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope_id: &DataBankScopeId,
) -> CoreResult<Option<(SessionId, IsoTimestamp)>> {
    tx.query_row(
        "SELECT session_id, created_at
         FROM data_bank_scopes
         WHERE scope_id = ?1",
        params![scope_id.0.as_str()],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load data-bank scope session ownership", error))
}

fn query_data_bank_scopes(
    conn: &Connection,
    query: &DataBankScopeQuery,
) -> CoreResult<Vec<DataBankScopeRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.map(DataBankScopeStatus::as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT scope_id
             FROM data_bank_scopes
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 OR status <> 'removed')
               AND (?3 IS NULL OR status = ?3)
             ORDER BY created_at ASC, scope_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query data-bank scopes", error))?;
    let scope_ids = stmt
        .query_map(
            params![session_id, query.include_removed, status, limit, offset],
            |row| Ok(DataBankScopeId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query data-bank scopes", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load data-bank scope ids", error))?;
    scope_ids
        .iter()
        .map(|scope_id| load_data_bank_scope(conn, scope_id))
        .collect()
}

fn load_data_bank_scope(
    conn: &Connection,
    scope_id: &DataBankScopeId,
) -> CoreResult<DataBankScopeRecord> {
    conn.query_row(
        "SELECT session_id, status, label, description, metadata_json,
                created_at, updated_at
         FROM data_bank_scopes
         WHERE scope_id = ?1",
        params![scope_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load data-bank scope", error))?
    .map(
        |(session_id, status, label, description, metadata_json, created_at, updated_at)| {
            Ok(DataBankScopeRecord {
                scope_id: scope_id.clone(),
                session_id,
                status: DataBankScopeStatus::parse(&status)?,
                label,
                description,
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
            format!("data-bank scope {scope_id} not found"),
        )
    })
}

fn load_data_bank_scope_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope_id: &DataBankScopeId,
) -> CoreResult<DataBankScopeRecord> {
    tx.query_row(
        "SELECT session_id, status, label, description, metadata_json,
                created_at, updated_at
         FROM data_bank_scopes
         WHERE scope_id = ?1",
        params![scope_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load data-bank scope in tx", error))?
    .map(
        |(session_id, status, label, description, metadata_json, created_at, updated_at)| {
            Ok(DataBankScopeRecord {
                scope_id: scope_id.clone(),
                session_id,
                status: DataBankScopeStatus::parse(&status)?,
                label,
                description,
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
            format!("data-bank scope {scope_id} not found"),
        )
    })
}

#[cfg(test)]
pub(crate) mod conformance {
    use super::*;
    use serde_json::json;

    pub(crate) trait AttachmentDataBankConformanceStore {
        fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord>;
        fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>>;
        fn remove_attachment(
            &self,
            attachment_id: &AttachmentId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<AttachmentRecord>;
        fn save_data_bank_scope(
            &self,
            scope: &DataBankScopeWrite,
        ) -> CoreResult<DataBankScopeRecord>;
        fn query_data_bank_scopes(
            &self,
            query: &DataBankScopeQuery,
        ) -> CoreResult<Vec<DataBankScopeRecord>>;
        fn remove_data_bank_scope(
            &self,
            scope_id: &DataBankScopeId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<DataBankScopeRecord>;
    }

    impl AttachmentDataBankConformanceStore for CoordinationStore {
        fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
            CoordinationStore::save_attachment(self, attachment)
        }

        fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
            CoordinationStore::query_attachments(self, query)
        }

        fn remove_attachment(
            &self,
            attachment_id: &AttachmentId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<AttachmentRecord> {
            CoordinationStore::remove_attachment(self, attachment_id, updated_at)
        }

        fn save_data_bank_scope(
            &self,
            scope: &DataBankScopeWrite,
        ) -> CoreResult<DataBankScopeRecord> {
            CoordinationStore::save_data_bank_scope(self, scope)
        }

        fn query_data_bank_scopes(
            &self,
            query: &DataBankScopeQuery,
        ) -> CoreResult<Vec<DataBankScopeRecord>> {
            CoordinationStore::query_data_bank_scopes(self, query)
        }

        fn remove_data_bank_scope(
            &self,
            scope_id: &DataBankScopeId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<DataBankScopeRecord> {
            CoordinationStore::remove_data_bank_scope(self, scope_id, updated_at)
        }
    }

    pub(crate) fn run_attachment_data_bank_conformance(
        store: &dyn AttachmentDataBankConformanceStore,
    ) {
        let session = SessionId::new("session-attachments");
        let other_session = SessionId::new("session-attachments-other");
        let scope = DataBankScopeId::new("scope-reference");
        let removed_scope = DataBankScopeId::new("scope-removed");
        let message = MessageId::new("message-reference");
        let block = MessageBlockId::new("block-reference");

        store
            .save_data_bank_scope(&DataBankScopeWrite {
                scope_id: scope.clone(),
                session_id: session.clone(),
                status: DataBankScopeStatus::Active,
                label: Some("Reference".to_string()),
                description: Some("Reusable reference files".to_string()),
                metadata_json: json!({"kind": "reference"}),
                created_at: "2026-06-26T04:00:00Z".to_string(),
                updated_at: "2026-06-26T04:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_data_bank_scope(&DataBankScopeWrite {
                scope_id: removed_scope.clone(),
                session_id: session.clone(),
                status: DataBankScopeStatus::Active,
                label: Some("Removed".to_string()),
                description: None,
                metadata_json: json!({"kind": "temporary"}),
                created_at: "2026-06-26T04:00:01Z".to_string(),
                updated_at: "2026-06-26T04:00:01Z".to_string(),
            })
            .unwrap();

        let saved = store
            .save_attachment(&AttachmentWrite {
                attachment_id: AttachmentId::new("attachment-reference"),
                session_id: session.clone(),
                status: AttachmentStatus::Active,
                filename: "reference.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 42,
                storage_url: Some("file:///store/reference.txt".to_string()),
                download_url: Some("/attachments/reference".to_string()),
                thumbnail_url: None,
                extracted_text: Some("bounded reference text".to_string()),
                extracted_text_truncated: true,
                metadata_json: json!({"source": "conformance"}),
                created_at: "2026-06-26T04:01:00Z".to_string(),
                updated_at: "2026-06-26T04:01:00Z".to_string(),
                expires_at: Some("2026-06-26T05:00:00Z".to_string()),
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-reference"),
                    attachment_id: AttachmentId::new("attachment-reference"),
                    session_id: session.clone(),
                    message_id: Some(message.clone()),
                    block_id: Some(block.clone()),
                    scope_id: Some(scope.clone()),
                    metadata_json: json!({"linked_by": "conformance"}),
                    created_at: "2026-06-26T04:01:00Z".to_string(),
                }),
            })
            .unwrap();
        assert_eq!(saved.links.len(), 1);
        assert_eq!(saved.links[0].message_id, Some(message.clone()));
        assert_eq!(saved.links[0].block_id, Some(block.clone()));
        assert_eq!(saved.links[0].scope_id, Some(scope.clone()));
        assert!(saved.extracted_text_truncated);

        store
            .save_attachment(&AttachmentWrite {
                attachment_id: AttachmentId::new("attachment-expired"),
                session_id: session.clone(),
                status: AttachmentStatus::Active,
                filename: "expired.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 7,
                storage_url: None,
                download_url: None,
                thumbnail_url: None,
                extracted_text: Some("expired".to_string()),
                extracted_text_truncated: false,
                metadata_json: json!({"source": "expired"}),
                created_at: "2026-06-26T04:02:00Z".to_string(),
                updated_at: "2026-06-26T04:02:00Z".to_string(),
                expires_at: Some("2026-06-26T04:30:00Z".to_string()),
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-expired"),
                    attachment_id: AttachmentId::new("attachment-expired"),
                    session_id: session.clone(),
                    message_id: None,
                    block_id: None,
                    scope_id: Some(scope.clone()),
                    metadata_json: json!({"linked_by": "expiry"}),
                    created_at: "2026-06-26T04:02:00Z".to_string(),
                }),
            })
            .unwrap();

        store
            .save_attachment(&AttachmentWrite {
                attachment_id: AttachmentId::new("attachment-other-session"),
                session_id: other_session,
                status: AttachmentStatus::Active,
                filename: "other.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 3,
                storage_url: None,
                download_url: None,
                thumbnail_url: None,
                extracted_text: None,
                extracted_text_truncated: false,
                metadata_json: json!({}),
                created_at: "2026-06-26T04:03:00Z".to_string(),
                updated_at: "2026-06-26T04:03:00Z".to_string(),
                expires_at: None,
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-other-session"),
                    attachment_id: AttachmentId::new("attachment-other-session"),
                    session_id: SessionId::new("session-attachments-other"),
                    message_id: None,
                    block_id: None,
                    scope_id: None,
                    metadata_json: json!({}),
                    created_at: "2026-06-26T04:03:00Z".to_string(),
                }),
            })
            .unwrap();

        let by_message = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                message_id: Some(message.clone()),
                now: Some("2026-06-26T04:10:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(by_message.len(), 1);
        assert_eq!(
            by_message[0].attachment_id,
            AttachmentId::new("attachment-reference")
        );

        let by_block = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                block_id: Some(block),
                now: Some("2026-06-26T04:10:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(by_block.len(), 1);
        assert_eq!(
            by_block[0].links[0].metadata_json["linked_by"],
            "conformance"
        );

        let by_scope_before_expiry = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope.clone()),
                now: Some("2026-06-26T04:10:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            by_scope_before_expiry
                .iter()
                .map(|record| record.attachment_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["attachment-reference", "attachment-expired"]
        );

        let by_scope_after_expiry = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope.clone()),
                now: Some("2026-06-26T04:31:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            by_scope_after_expiry
                .iter()
                .map(|record| record.attachment_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["attachment-reference"]
        );

        let expired_only = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope.clone()),
                expired_only: true,
                now: Some("2026-06-26T04:31:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(expired_only.len(), 1);
        assert_eq!(
            expired_only[0].attachment_id,
            AttachmentId::new("attachment-expired")
        );

        let include_expired = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope),
                include_expired: true,
                now: Some("2026-06-26T04:31:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(include_expired.len(), 2);

        let removed_attachment = store
            .remove_attachment(
                &AttachmentId::new("attachment-reference"),
                &"2026-06-26T04:40:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(removed_attachment.status, AttachmentStatus::Removed);
        assert_eq!(removed_attachment.links.len(), 1);

        let active_after_remove = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                include_expired: true,
                now: Some("2026-06-26T04:41:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            active_after_remove
                .iter()
                .map(|record| record.attachment_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["attachment-expired"]
        );

        let with_removed = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                status: Some(AttachmentStatus::Removed),
                include_removed: true,
                include_expired: true,
                now: Some("2026-06-26T04:41:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(with_removed.len(), 1);
        assert_eq!(
            with_removed[0].attachment_id,
            AttachmentId::new("attachment-reference")
        );

        let scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session.clone()),
                status: Some(DataBankScopeStatus::Active),
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            scopes
                .iter()
                .map(|scope| scope.scope_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["scope-reference", "scope-removed"]
        );
        let removed = store
            .remove_data_bank_scope(&removed_scope, &"2026-06-26T04:45:00Z".to_string())
            .unwrap();
        assert_eq!(removed.status, DataBankScopeStatus::Removed);
        let active_scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session.clone()),
                status: Some(DataBankScopeStatus::Active),
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(active_scopes.len(), 1);
        assert_eq!(
            active_scopes[0].scope_id,
            DataBankScopeId::new("scope-reference")
        );
        let all_scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session),
                status: Some(DataBankScopeStatus::Removed),
                include_removed: true,
                page: None,
            })
            .unwrap();
        assert_eq!(all_scopes.len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn attachments_data_bank_conformance_contract_runs_on_sqlite_repo() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-attachments-data-bank-conformance-{}-{}.sqlite3",
            std::process::id(),
            "sqlite"
        ));
        let _ = fs::remove_file(&db_path);
        let store = CoordinationStore::open_file(&db_path).unwrap();
        conformance::run_attachment_data_bank_conformance(&store);
        drop(store);
        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn attachments_repo_preserves_scope_links_removed_state_and_restart() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-attachments-repo-{}-{}.sqlite3",
            std::process::id(),
            "restart"
        ));
        let _ = fs::remove_file(&db_path);
        let session_id = SessionId::new("session-attachment-repo");
        let scope_id = DataBankScopeId::new("scope-reference-repo");
        let attachment_id = AttachmentId::new("attachment-reference-repo");
        {
            let store = CoordinationStore::open_file(&db_path).unwrap();
            store
                .save_data_bank_scope(&DataBankScopeWrite {
                    scope_id: scope_id.clone(),
                    session_id: session_id.clone(),
                    status: DataBankScopeStatus::Active,
                    label: Some("Reference".to_string()),
                    description: Some("Restart fixture scope".to_string()),
                    metadata_json: json!({"kind": "reference"}),
                    created_at: "2026-07-02T03:00:00Z".to_string(),
                    updated_at: "2026-07-02T03:00:00Z".to_string(),
                })
                .unwrap();
            store
                .save_attachment(&AttachmentWrite {
                    attachment_id: attachment_id.clone(),
                    session_id: session_id.clone(),
                    status: AttachmentStatus::Active,
                    filename: "reference.md".to_string(),
                    mime_type: "text/markdown".to_string(),
                    byte_size: 42,
                    storage_url: Some("file:///tmp/reference.md".to_string()),
                    download_url: None,
                    thumbnail_url: None,
                    extracted_text: Some("durable attachment".to_string()),
                    extracted_text_truncated: false,
                    metadata_json: json!({"source": "repo-test"}),
                    created_at: "2026-07-02T03:01:00Z".to_string(),
                    updated_at: "2026-07-02T03:01:00Z".to_string(),
                    expires_at: None,
                    link: Some(AttachmentLinkWrite {
                        link_id: AttachmentLinkId::new("attachment-link-reference-repo"),
                        attachment_id: attachment_id.clone(),
                        session_id: session_id.clone(),
                        message_id: Some(MessageId::new("message-reference-repo")),
                        block_id: Some(MessageBlockId::new("block-reference-repo")),
                        scope_id: Some(scope_id.clone()),
                        metadata_json: json!({"slot": "reference"}),
                        created_at: "2026-07-02T03:01:00Z".to_string(),
                    }),
                })
                .unwrap();
        }

        let store = CoordinationStore::open_file(&db_path).unwrap();
        let by_scope = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                scope_id: Some(scope_id.clone()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(by_scope.len(), 1);
        assert_eq!(by_scope[0].attachment_id, attachment_id);
        assert_eq!(by_scope[0].links.len(), 1);
        assert_eq!(by_scope[0].links[0].scope_id.as_ref(), Some(&scope_id));

        store
            .remove_attachment(&attachment_id, &"2026-07-02T03:02:00Z".to_string())
            .unwrap();
        assert!(store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                scope_id: Some(scope_id.clone()),
                ..AttachmentQuery::default()
            })
            .unwrap()
            .is_empty());
        let removed = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                scope_id: Some(scope_id.clone()),
                include_removed: true,
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].status, AttachmentStatus::Removed);

        store
            .remove_data_bank_scope(&scope_id, &"2026-07-02T03:03:00Z".to_string())
            .unwrap();
        assert!(store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session_id),
                ..DataBankScopeQuery::default()
            })
            .unwrap()
            .is_empty());

        drop(store);
        let _ = fs::remove_file(&db_path);
    }
}
