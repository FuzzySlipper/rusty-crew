//! Typed PostgreSQL repositories for roleplay-owned profile and session records.

use super::*;
use crate::{
    RoleplayCharacterQuery, RoleplayCharacterRecord, RoleplayCharacterWrite, RoleplayImportQuery,
    RoleplayImportRecord, RoleplayImportWrite, RoleplayPlayerPersonaQuery,
    RoleplayPlayerPersonaRecord, RoleplayPlayerPersonaWrite, RoleplaySessionMetadataQuery,
    RoleplaySessionMetadataRecord, RoleplaySessionMetadataWrite, RoleplaySessionProjectionRecord,
    RoleplaySessionProjectionWrite,
};

impl PostgresBackendStore {
    pub fn put_roleplay_character(
        &self,
        write: &RoleplayCharacterWrite,
    ) -> CoreResult<RoleplayCharacterRecord> {
        validate_id("character_id", &write.record.id)?;
        validate_id("profile_id", &write.record.profile_id)?;
        put_character(self, write)
    }

    pub fn get_roleplay_character(
        &self,
        character_id: &str,
    ) -> CoreResult<Option<RoleplayCharacterRecord>> {
        validate_id("character_id", character_id)?;
        self.get_roleplay_json("module_roleplay_characters", "character_id", character_id)
    }

    pub fn list_roleplay_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        validate_id("profile_id", &query.profile_id)?;
        self.list_roleplay_json(
            "module_roleplay_characters",
            "profile_id = $1 AND ($2::TEXT IS NULL OR status = $2)",
            &[&query.profile_id, &query.status],
            query.page,
            "updated_at DESC, character_id ASC",
        )
    }

    pub fn put_roleplay_player_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        validate_id("persona_id", &write.record.id)?;
        validate_id("profile_id", &write.record.profile_id)?;
        put_persona(self, write)
    }

    pub fn get_roleplay_player_persona(
        &self,
        persona_id: &str,
    ) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        validate_id("persona_id", persona_id)?;
        self.get_roleplay_json("module_roleplay_player_personas", "persona_id", persona_id)
    }

    pub fn list_roleplay_player_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        validate_id("profile_id", &query.profile_id)?;
        self.list_roleplay_json(
            "module_roleplay_player_personas",
            "profile_id = $1 AND ($2::TEXT IS NULL OR status = $2)",
            &[&query.profile_id, &query.status],
            query.page,
            "updated_at DESC, persona_id ASC",
        )
    }

    pub fn put_roleplay_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        validate_id("session_id", &write.record.session_id)?;
        validate_id("profile_id", &write.record.profile_id)?;
        put_session(self, write)
    }

    pub fn get_roleplay_session_metadata(
        &self,
        session_id: &str,
    ) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        validate_id("session_id", session_id)?;
        self.get_roleplay_json("module_roleplay_session_metadata", "session_id", session_id)
    }

    pub fn list_roleplay_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        self.list_roleplay_json(
            "module_roleplay_session_metadata",
            "($1::TEXT IS NULL OR profile_id = $1) AND ($2::BOOLEAN IS NULL OR archived = $2)",
            &[&query.profile_id, &query.archived],
            query.page,
            "updated_at DESC, session_id ASC",
        )
    }

    pub fn apply_roleplay_session_projection(
        &self,
        write: &RoleplaySessionProjectionWrite,
    ) -> CoreResult<RoleplaySessionProjectionRecord> {
        if let Some(layers) = &write.chat_layers {
            validate_roleplay_chat_layers_write(layers)?;
            if layers.chat_id != write.metadata.record.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "roleplay projection chat_id must match metadata session_id",
                ));
            }
        }
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL roleplay session projection", error)
        })?;
        let metadata = put_session_with_client(&mut tx, &schema, &write.metadata)?;
        if let Some(layers) = &write.chat_layers {
            Self::set_chat_layers_in_tx(&mut tx, &schema, layers)?;
        }
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL roleplay session projection", error)
        })?;
        let chat_layers = self.get_chat_layers(&metadata.session_id)?;
        Ok(RoleplaySessionProjectionRecord {
            metadata,
            chat_layers,
        })
    }

    pub fn put_roleplay_import(
        &self,
        write: &RoleplayImportWrite,
    ) -> CoreResult<RoleplayImportRecord> {
        validate_id("import_id", &write.record.import_id)?;
        validate_id("profile_id", &write.record.profile_id)?;
        validate_id("session_id", &write.record.session_id)?;
        put_import(self, write)
    }

    pub fn get_roleplay_import(&self, import_id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        validate_id("import_id", import_id)?;
        self.get_roleplay_json("module_roleplay_imports", "import_id", import_id)
    }

    pub fn list_roleplay_imports(
        &self,
        query: &RoleplayImportQuery,
    ) -> CoreResult<Vec<RoleplayImportRecord>> {
        validate_id("profile_id", &query.profile_id)?;
        self.list_roleplay_json(
            "module_roleplay_imports",
            "profile_id = $1 AND ($2::TEXT IS NULL OR status = $2)",
            &[&query.profile_id, &query.status],
            query.page,
            "imported_at DESC, import_id ASC",
        )
    }

    fn get_roleplay_json<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        id_column: &str,
        id: &str,
    ) -> CoreResult<Option<T>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let row = client
            .query_opt(
                &format!("SELECT record_json::text FROM {schema}.{table} WHERE {id_column} = $1"),
                &[&id],
            )
            .map_err(|error| postgres_error("read typed PostgreSQL roleplay record", error))?;
        row.map(|row| parse_postgres_json(row.get::<_, String>(0)))
            .transpose()
    }

    fn list_roleplay_json<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        predicate: &str,
        params: &[&(dyn postgres::types::ToSql + Sync)],
        page: Option<QueryPage>,
        ordering: &str,
    ) -> CoreResult<Vec<T>> {
        let schema = self.quoted_schema();
        let (limit, offset) = page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let mut owned_params = params.to_vec();
        owned_params.push(&limit);
        owned_params.push(&offset);
        let limit_index = params.len() + 1;
        let offset_index = params.len() + 2;
        let mut client = self.client()?;
        client
            .query(
                &format!(
                    "SELECT record_json::text FROM {schema}.{table} WHERE {predicate} \
                     ORDER BY {ordering} LIMIT ${limit_index} OFFSET ${offset_index}"
                ),
                &owned_params,
            )
            .map_err(|error| postgres_error("query typed PostgreSQL roleplay records", error))?
            .into_iter()
            .map(|row| parse_postgres_json(row.get::<_, String>(0)))
            .collect()
    }
}

fn put_character(
    store: &PostgresBackendStore,
    write: &RoleplayCharacterWrite,
) -> CoreResult<RoleplayCharacterRecord> {
    let mut record = write.record.clone();
    record.revision = next_revision(
        store.get_roleplay_character(&record.id)?,
        write.expected_revision,
        &record.id,
    )?;
    let schema = store.quoted_schema();
    let json = to_json_text(&record)?;
    let mut client = store.client()?;
    client.execute(&format!("INSERT INTO {schema}.module_roleplay_characters (character_id, profile_id, status, name, revision, record_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6::text::jsonb,$7,$8) ON CONFLICT(character_id) DO UPDATE SET profile_id=EXCLUDED.profile_id,status=EXCLUDED.status,name=EXCLUDED.name,revision=EXCLUDED.revision,record_json=EXCLUDED.record_json,updated_at=EXCLUDED.updated_at"), &[&record.id,&record.profile_id,&record.status,&record.name,&(record.revision as i64),&json,&record.created_at,&record.updated_at]).map_err(|error| postgres_error("write typed PostgreSQL roleplay character", error))?;
    Ok(record)
}

fn put_persona(
    store: &PostgresBackendStore,
    write: &RoleplayPlayerPersonaWrite,
) -> CoreResult<RoleplayPlayerPersonaRecord> {
    let mut record = write.record.clone();
    record.revision = next_revision(
        store.get_roleplay_player_persona(&record.id)?,
        write.expected_revision,
        &record.id,
    )?;
    let schema = store.quoted_schema();
    let json = to_json_text(&record)?;
    let mut client = store.client()?;
    client.execute(&format!("INSERT INTO {schema}.module_roleplay_player_personas (persona_id, profile_id, status, display_name, revision, record_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6::text::jsonb,$7,$8) ON CONFLICT(persona_id) DO UPDATE SET profile_id=EXCLUDED.profile_id,status=EXCLUDED.status,display_name=EXCLUDED.display_name,revision=EXCLUDED.revision,record_json=EXCLUDED.record_json,updated_at=EXCLUDED.updated_at"), &[&record.id,&record.profile_id,&record.status,&record.display_name,&(record.revision as i64),&json,&record.created_at,&record.updated_at]).map_err(|error| postgres_error("write typed PostgreSQL roleplay persona", error))?;
    Ok(record)
}

fn put_session(
    store: &PostgresBackendStore,
    write: &RoleplaySessionMetadataWrite,
) -> CoreResult<RoleplaySessionMetadataRecord> {
    let schema = store.quoted_schema();
    let mut client = store.client()?;
    put_session_with_client(&mut *client, &schema, write)
}

fn put_session_with_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    write: &RoleplaySessionMetadataWrite,
) -> CoreResult<RoleplaySessionMetadataRecord> {
    let current = client.query_opt(&format!("SELECT record_json::text FROM {schema}.module_roleplay_session_metadata WHERE session_id = $1 FOR UPDATE"), &[&write.record.session_id]).map_err(|error| postgres_error("read PostgreSQL roleplay session metadata for update", error))?.map(|row| parse_postgres_json::<RoleplaySessionMetadataRecord>(row.get(0))).transpose()?;
    let mut record = write.record.clone();
    record.revision = next_revision(current, write.expected_revision, &record.session_id)?;
    let json = to_json_text(&record)?;
    client.execute(&format!("INSERT INTO {schema}.module_roleplay_session_metadata (session_id, profile_id, archived, character_id, persona_id, revision, record_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7::text::jsonb,$8,$9) ON CONFLICT(session_id) DO UPDATE SET profile_id=EXCLUDED.profile_id,archived=EXCLUDED.archived,character_id=EXCLUDED.character_id,persona_id=EXCLUDED.persona_id,revision=EXCLUDED.revision,record_json=EXCLUDED.record_json,updated_at=EXCLUDED.updated_at"), &[&record.session_id,&record.profile_id,&record.archived,&record.character_id,&record.player_persona_id,&(record.revision as i64),&json,&record.created_at,&record.updated_at]).map_err(|error| postgres_error("write typed PostgreSQL roleplay session metadata", error))?;
    Ok(record)
}

fn put_import(
    store: &PostgresBackendStore,
    write: &RoleplayImportWrite,
) -> CoreResult<RoleplayImportRecord> {
    let mut record = write.record.clone();
    record.revision = next_revision(
        store.get_roleplay_import(&record.import_id)?,
        write.expected_revision,
        &record.import_id,
    )?;
    let schema = store.quoted_schema();
    let json = to_json_text(&record)?;
    let mut client = store.client()?;
    client.execute(&format!("INSERT INTO {schema}.module_roleplay_imports (import_id, profile_id, session_id, source_kind, status, revision, record_json, imported_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7::text::jsonb,$8,$9) ON CONFLICT(import_id) DO UPDATE SET profile_id=EXCLUDED.profile_id,session_id=EXCLUDED.session_id,source_kind=EXCLUDED.source_kind,status=EXCLUDED.status,revision=EXCLUDED.revision,record_json=EXCLUDED.record_json,updated_at=EXCLUDED.updated_at"), &[&record.import_id,&record.profile_id,&record.session_id,&record.source_kind,&record.status,&(record.revision as i64),&json,&record.imported_at,&record.updated_at]).map_err(|error| postgres_error("write typed PostgreSQL roleplay import", error))?;
    Ok(record)
}

trait Revisioned {
    fn revision(&self) -> u64;
}
impl Revisioned for RoleplayCharacterRecord {
    fn revision(&self) -> u64 {
        self.revision
    }
}
impl Revisioned for RoleplayPlayerPersonaRecord {
    fn revision(&self) -> u64 {
        self.revision
    }
}
impl Revisioned for RoleplaySessionMetadataRecord {
    fn revision(&self) -> u64 {
        self.revision
    }
}
impl Revisioned for RoleplayImportRecord {
    fn revision(&self) -> u64 {
        self.revision
    }
}

fn next_revision<T: Revisioned>(
    current: Option<T>,
    expected: Option<u64>,
    id: &str,
) -> CoreResult<u64> {
    match (current.as_ref().map(Revisioned::revision), expected) {
        (None, None) => Ok(1),
        (None, Some(_)) => Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("roleplay record {id} not found"),
        )),
        (Some(_), None) => Err(CoreError::new(
            CoreErrorKind::AlreadyExists,
            format!("roleplay record {id} already exists"),
        )),
        (Some(actual), Some(expected)) if actual == expected => Ok(actual + 1),
        (Some(actual), Some(expected)) => Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("roleplay record {id} revision mismatch: expected {expected}, found {actual}"),
        )),
    }
}

fn parse_postgres_json<T: serde::de::DeserializeOwned>(json: String) -> CoreResult<T> {
    from_json_text(&json).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("deserialize typed PostgreSQL roleplay record: {error}"),
        )
    })
}

fn validate_id(field: &str, value: &str) -> CoreResult<()> {
    if value.trim().is_empty() || value.len() > 255 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{field} must contain 1..=255 characters"),
        ));
    }
    Ok(())
}
