use super::super::*;

pub(crate) fn migrate_v33_add_roleplay_records(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS module_roleplay_characters (
            character_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL,
            status TEXT NOT NULL,
            name TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_roleplay_characters_profile_status
            ON module_roleplay_characters(profile_id, status, updated_at DESC, character_id);
        CREATE INDEX IF NOT EXISTS idx_roleplay_characters_profile_name
            ON module_roleplay_characters(profile_id, name, character_id);

        CREATE TABLE IF NOT EXISTS module_roleplay_player_personas (
            persona_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL,
            status TEXT NOT NULL,
            display_name TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_roleplay_personas_profile_status
            ON module_roleplay_player_personas(profile_id, status, updated_at DESC, persona_id);
        CREATE INDEX IF NOT EXISTS idx_roleplay_personas_profile_name
            ON module_roleplay_player_personas(profile_id, display_name, persona_id);

        CREATE TABLE IF NOT EXISTS module_roleplay_session_metadata (
            session_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL,
            archived INTEGER NOT NULL,
            character_id TEXT,
            persona_id TEXT,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_roleplay_sessions_profile_archived
            ON module_roleplay_session_metadata(profile_id, archived, updated_at DESC, session_id);

        CREATE TABLE IF NOT EXISTS module_roleplay_imports (
            import_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            source_kind TEXT NOT NULL,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            imported_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_roleplay_imports_profile_status
            ON module_roleplay_imports(profile_id, status, imported_at DESC, import_id);
        CREATE INDEX IF NOT EXISTS idx_roleplay_imports_session
            ON module_roleplay_imports(session_id, imported_at DESC, import_id);
        ",
    )
    .map_err(|error| persistence_error("create typed roleplay record tables", error))?;
    Ok(())
}

impl CoordinationStore {
    pub fn put_roleplay_character(
        &self,
        write: &RoleplayCharacterWrite,
    ) -> CoreResult<RoleplayCharacterRecord> {
        validate_character_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start typed roleplay character write", error))?;
        let current = get_character_in_tx(&tx, &write.record.id)?;
        let revision = next_revision(
            "roleplay character",
            &write.record.id,
            current.as_ref().map(|record| record.revision),
            write.expected_revision,
        )?;
        let mut record = write.record.clone();
        record.revision = revision;
        tx.execute(
            "INSERT INTO module_roleplay_characters (
                character_id, profile_id, status, name, revision, record_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(character_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                status = excluded.status,
                name = excluded.name,
                revision = excluded.revision,
                record_json = excluded.record_json,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.profile_id,
                record.status,
                record.name,
                record.revision as i64,
                to_json_text(&record)?,
                record.created_at,
                record.updated_at,
            ],
        )
        .map_err(|error| persistence_error("write typed roleplay character", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit typed roleplay character write", error))?;
        Ok(record)
    }

    pub fn get_roleplay_character(
        &self,
        character_id: &str,
    ) -> CoreResult<Option<RoleplayCharacterRecord>> {
        validate_roleplay_record_id("character_id", character_id)?;
        let conn = self.conn()?;
        get_character_in_tx(&conn, character_id)
    }

    pub fn list_roleplay_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        validate_roleplay_record_id("profile_id", &query.profile_id)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_characters
                 WHERE profile_id = ?1 AND (?2 IS NULL OR status = ?2)
                 ORDER BY updated_at DESC, character_id ASC LIMIT ?3 OFFSET ?4",
            )
            .map_err(|error| persistence_error("prepare typed roleplay character query", error))?;
        let records = collect_json_rows(
            stmt.query_map(
                params![query.profile_id, query.status, limit, offset],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query typed roleplay characters", error))?,
            "typed roleplay character",
        );
        records
    }

    pub fn put_roleplay_player_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        validate_persona_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start typed roleplay persona write", error))?;
        let current = get_persona_in_tx(&tx, &write.record.id)?;
        let revision = next_revision(
            "roleplay player persona",
            &write.record.id,
            current.as_ref().map(|record| record.revision),
            write.expected_revision,
        )?;
        let mut record = write.record.clone();
        record.revision = revision;
        tx.execute(
            "INSERT INTO module_roleplay_player_personas (
                persona_id, profile_id, status, display_name, revision, record_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(persona_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                status = excluded.status,
                display_name = excluded.display_name,
                revision = excluded.revision,
                record_json = excluded.record_json,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.profile_id,
                record.status,
                record.display_name,
                record.revision as i64,
                to_json_text(&record)?,
                record.created_at,
                record.updated_at,
            ],
        )
        .map_err(|error| persistence_error("write typed roleplay persona", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit typed roleplay persona write", error))?;
        Ok(record)
    }

    pub fn get_roleplay_player_persona(
        &self,
        persona_id: &str,
    ) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        validate_roleplay_record_id("persona_id", persona_id)?;
        let conn = self.conn()?;
        get_persona_in_tx(&conn, persona_id)
    }

    pub fn list_roleplay_player_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        validate_roleplay_record_id("profile_id", &query.profile_id)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_player_personas
                 WHERE profile_id = ?1 AND (?2 IS NULL OR status = ?2)
                 ORDER BY updated_at DESC, persona_id ASC LIMIT ?3 OFFSET ?4",
            )
            .map_err(|error| persistence_error("prepare typed roleplay persona query", error))?;
        let records = collect_json_rows(
            stmt.query_map(
                params![query.profile_id, query.status, limit, offset],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query typed roleplay personas", error))?,
            "typed roleplay persona",
        );
        records
    }

    pub fn put_roleplay_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        validate_session_metadata_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start typed roleplay session metadata write", error)
        })?;
        let current = get_session_metadata_in_tx(&tx, &write.record.session_id)?;
        let revision = next_revision(
            "roleplay session metadata",
            &write.record.session_id,
            current.as_ref().map(|record| record.revision),
            write.expected_revision,
        )?;
        let mut record = write.record.clone();
        record.revision = revision;
        tx.execute(
            "INSERT INTO module_roleplay_session_metadata (
                session_id, profile_id, archived, character_id, persona_id, revision,
                record_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(session_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                archived = excluded.archived,
                character_id = excluded.character_id,
                persona_id = excluded.persona_id,
                revision = excluded.revision,
                record_json = excluded.record_json,
                updated_at = excluded.updated_at",
            params![
                record.session_id,
                record.profile_id,
                i64::from(record.archived),
                record.character_id,
                record.player_persona_id,
                record.revision as i64,
                to_json_text(&record)?,
                record.created_at,
                record.updated_at,
            ],
        )
        .map_err(|error| persistence_error("write typed roleplay session metadata", error))?;
        tx.commit().map_err(|error| {
            persistence_error("commit typed roleplay session metadata write", error)
        })?;
        Ok(record)
    }

    pub fn get_roleplay_session_metadata(
        &self,
        session_id: &str,
    ) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        validate_roleplay_record_id("session_id", session_id)?;
        let conn = self.conn()?;
        get_session_metadata_in_tx(&conn, session_id)
    }

    pub fn list_roleplay_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let archived = query.archived.map(i64::from);
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_session_metadata
                 WHERE (?1 IS NULL OR profile_id = ?1) AND (?2 IS NULL OR archived = ?2)
                 ORDER BY updated_at DESC, session_id ASC LIMIT ?3 OFFSET ?4",
            )
            .map_err(|error| persistence_error("prepare typed roleplay session query", error))?;
        let records = collect_json_rows(
            stmt.query_map(params![query.profile_id, archived, limit, offset], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| persistence_error("query typed roleplay sessions", error))?,
            "typed roleplay session metadata",
        );
        records
    }

    pub fn put_roleplay_import(
        &self,
        write: &RoleplayImportWrite,
    ) -> CoreResult<RoleplayImportRecord> {
        validate_import_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start typed roleplay import write", error))?;
        let current = get_import_in_tx(&tx, &write.record.import_id)?;
        let revision = next_revision(
            "roleplay import",
            &write.record.import_id,
            current.as_ref().map(|record| record.revision),
            write.expected_revision,
        )?;
        let mut record = write.record.clone();
        record.revision = revision;
        tx.execute(
            "INSERT INTO module_roleplay_imports (
                import_id, profile_id, session_id, source_kind, status, revision,
                record_json, imported_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(import_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                session_id = excluded.session_id,
                source_kind = excluded.source_kind,
                status = excluded.status,
                revision = excluded.revision,
                record_json = excluded.record_json,
                updated_at = excluded.updated_at",
            params![
                record.import_id,
                record.profile_id,
                record.session_id,
                record.source_kind,
                record.status,
                record.revision as i64,
                to_json_text(&record)?,
                record.imported_at,
                record.updated_at,
            ],
        )
        .map_err(|error| persistence_error("write typed roleplay import", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit typed roleplay import write", error))?;
        Ok(record)
    }

    pub fn get_roleplay_import(&self, import_id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        validate_roleplay_record_id("import_id", import_id)?;
        let conn = self.conn()?;
        get_import_in_tx(&conn, import_id)
    }

    pub fn list_roleplay_imports(
        &self,
        query: &RoleplayImportQuery,
    ) -> CoreResult<Vec<RoleplayImportRecord>> {
        validate_roleplay_record_id("profile_id", &query.profile_id)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_imports
                 WHERE profile_id = ?1 AND (?2 IS NULL OR status = ?2)
                 ORDER BY imported_at DESC, import_id ASC LIMIT ?3 OFFSET ?4",
            )
            .map_err(|error| persistence_error("prepare typed roleplay import query", error))?;
        let records = collect_json_rows(
            stmt.query_map(
                params![query.profile_id, query.status, limit, offset],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query typed roleplay imports", error))?,
            "typed roleplay import",
        );
        records
    }
}

fn get_character_in_tx(conn: &Connection, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>> {
    load_json_record(conn, "module_roleplay_characters", "character_id", id)
}

fn get_persona_in_tx(
    conn: &Connection,
    id: &str,
) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
    load_json_record(conn, "module_roleplay_player_personas", "persona_id", id)
}

fn get_session_metadata_in_tx(
    conn: &Connection,
    id: &str,
) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
    load_json_record(conn, "module_roleplay_session_metadata", "session_id", id)
}

fn get_import_in_tx(conn: &Connection, id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
    load_json_record(conn, "module_roleplay_imports", "import_id", id)
}

fn load_json_record<T: DeserializeOwned>(
    conn: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> CoreResult<Option<T>> {
    let sql = format!("SELECT record_json FROM {table} WHERE {id_column} = ?1");
    let value = conn
        .query_row(&sql, params![id], |row| row.get::<_, String>(0))
        .optional()
        .map_err(|error| persistence_error("read typed roleplay record", error))?;
    value.map(|json| parse_json_record(&json)).transpose()
}

fn collect_json_rows<T: DeserializeOwned>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<String>>,
    label: &str,
) -> CoreResult<Vec<T>> {
    rows.map(|row| {
        row.map_err(|error| persistence_error("read typed roleplay row", error))
            .and_then(|json| {
                parse_json_record(&json).map_err(|error| {
                    CoreError::new(
                        error.kind,
                        format!("deserialize {label}: {}", error.message),
                    )
                })
            })
    })
    .collect()
}

fn next_revision(
    label: &str,
    id: &str,
    current: Option<u64>,
    expected: Option<u64>,
) -> CoreResult<u64> {
    match (current, expected) {
        (None, None) => Ok(1),
        (None, Some(_)) => Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("{label} {id} was not found"),
        )),
        (Some(_), None) => Err(CoreError::new(
            CoreErrorKind::AlreadyExists,
            format!("{label} {id} already exists"),
        )),
        (Some(actual), Some(expected)) if actual == expected => Ok(actual + 1),
        (Some(actual), Some(expected)) => Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("{label} {id} revision mismatch: expected {expected}, found {actual}"),
        )),
    }
}

fn validate_character_write(write: &RoleplayCharacterWrite) -> CoreResult<()> {
    validate_roleplay_record_id("character_id", &write.record.id)?;
    validate_roleplay_record_id("profile_id", &write.record.profile_id)?;
    require_text("character name", &write.record.name)?;
    validate_status(&write.record.status)
}

fn validate_persona_write(write: &RoleplayPlayerPersonaWrite) -> CoreResult<()> {
    validate_roleplay_record_id("persona_id", &write.record.id)?;
    validate_roleplay_record_id("profile_id", &write.record.profile_id)?;
    require_text("persona display name", &write.record.display_name)?;
    validate_status(&write.record.status)
}

fn validate_session_metadata_write(write: &RoleplaySessionMetadataWrite) -> CoreResult<()> {
    validate_roleplay_record_id("session_id", &write.record.session_id)?;
    validate_roleplay_record_id("profile_id", &write.record.profile_id)?;
    let mut layers = BTreeSet::new();
    for layer_id in &write.record.active_layer_ids {
        validate_roleplay_record_id("layer_id", layer_id)?;
        if !layers.insert(layer_id) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("duplicate active roleplay layer {layer_id}"),
            ));
        }
    }
    Ok(())
}

fn validate_import_write(write: &RoleplayImportWrite) -> CoreResult<()> {
    validate_roleplay_record_id("import_id", &write.record.import_id)?;
    validate_roleplay_record_id("profile_id", &write.record.profile_id)?;
    validate_roleplay_record_id("session_id", &write.record.session_id)?;
    require_text("roleplay import source kind", &write.record.source_kind)?;
    match write.record.status.as_str() {
        "pending" | "completed" | "failed" => Ok(()),
        status => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unsupported roleplay import status {status}"),
        )),
    }
}

fn validate_roleplay_record_id(field: &str, value: &str) -> CoreResult<()> {
    if value.trim().is_empty() || value.len() > 255 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{field} must be non-empty and at most 255 bytes"),
        ));
    }
    Ok(())
}

fn validate_status(status: &str) -> CoreResult<()> {
    match status {
        "active" | "archived" => Ok(()),
        _ => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unsupported roleplay record status {status}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_roleplay_records_enforce_revisions_and_queries() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-roleplay-records-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let character = RoleplayCharacterRecord {
            id: "character-one".into(),
            profile_id: "profile-one".into(),
            name: "One".into(),
            description: "desc".into(),
            personality: String::new(),
            scenario: String::new(),
            first_message: "hello".into(),
            alternate_greetings: Vec::new(),
            example_messages: Vec::new(),
            tags: vec!["test".into()],
            avatar_url: None,
            status: "active".into(),
            revision: 0,
            created_at: "2026-07-10T00:00:00Z".into(),
            updated_at: "2026-07-10T00:00:00Z".into(),
        };
        let created = store
            .put_roleplay_character(&RoleplayCharacterWrite {
                record: character,
                expected_revision: None,
            })
            .unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            store
                .list_roleplay_characters(&RoleplayCharacterQuery {
                    profile_id: "profile-one".into(),
                    status: Some("active".into()),
                    page: None
                })
                .unwrap(),
            vec![created.clone()]
        );
        let mut changed = created.clone();
        changed.name = "Changed".into();
        let replaced = store
            .put_roleplay_character(&RoleplayCharacterWrite {
                record: changed,
                expected_revision: Some(1),
            })
            .unwrap();
        assert_eq!(replaced.revision, 2);
        assert_eq!(
            store
                .put_roleplay_character(&RoleplayCharacterWrite {
                    record: replaced.clone(),
                    expected_revision: Some(1)
                })
                .unwrap_err()
                .kind,
            CoreErrorKind::ActionRejected
        );

        let persona = store
            .put_roleplay_player_persona(&RoleplayPlayerPersonaWrite {
                record: RoleplayPlayerPersonaRecord {
                    id: "persona-one".into(),
                    profile_id: "profile-one".into(),
                    display_name: "Player".into(),
                    avatar_url: None,
                    avatar_asset_ref: None,
                    description: String::new(),
                    notes: String::new(),
                    status: "active".into(),
                    revision: 0,
                    created_at: "2026-07-10T00:00:00Z".into(),
                    updated_at: "2026-07-10T00:00:00Z".into(),
                },
                expected_revision: None,
            })
            .unwrap();
        assert_eq!(persona.revision, 1);
        let session = store
            .put_roleplay_session_metadata(&RoleplaySessionMetadataWrite {
                record: RoleplaySessionMetadataRecord {
                    session_id: "session-one".into(),
                    profile_id: "profile-one".into(),
                    display_name: Some("Session".into()),
                    player_persona_id: Some(persona.id.clone()),
                    character_id: Some(replaced.id.clone()),
                    active_layer_ids: vec!["layer-one".into()],
                    archived: false,
                    revision: 0,
                    created_at: "2026-07-10T00:00:00Z".into(),
                    updated_at: "2026-07-10T00:00:00Z".into(),
                },
                expected_revision: None,
            })
            .unwrap();
        assert_eq!(session.revision, 1);
        let import = store
            .put_roleplay_import(&RoleplayImportWrite {
                record: RoleplayImportRecord {
                    import_id: "import-one".into(),
                    profile_id: "profile-one".into(),
                    source_kind: "test".into(),
                    provenance: serde_json::json!({"source":"test"}),
                    raw_source: None,
                    character_id: Some(replaced.id),
                    persona_id: Some(persona.id),
                    lore_layer_id: None,
                    session_id: session.session_id,
                    counts: RoleplayImportCounts::default(),
                    status: "completed".into(),
                    failure_reason: None,
                    revision: 0,
                    imported_at: "2026-07-10T00:00:00Z".into(),
                    updated_at: "2026-07-10T00:00:00Z".into(),
                },
                expected_revision: None,
            })
            .unwrap();
        assert_eq!(
            store.get_roleplay_import(&import.import_id).unwrap(),
            Some(import.clone())
        );
        store.purge_profile(&ProfileId::new("profile-one")).unwrap();
        assert!(store
            .get_roleplay_character("character-one")
            .unwrap()
            .is_none());
        assert!(store
            .get_roleplay_player_persona("persona-one")
            .unwrap()
            .is_none());
        assert!(store
            .get_roleplay_session_metadata("session-one")
            .unwrap()
            .is_none());
        assert!(store
            .get_roleplay_import(&import.import_id)
            .unwrap()
            .is_none());
        drop(store);
        let _ = std::fs::remove_file(db_path);
    }
}

fn require_text(field: &str, value: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(())
    }
}
