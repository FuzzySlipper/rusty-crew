use rusty_crew_core_persistence::*;
use rusty_crew_core_protocol::CoreResult;

pub(crate) trait RoleplayRecordsStore {
    fn put_character(&self, write: &RoleplayCharacterWrite) -> CoreResult<RoleplayCharacterRecord>;
    fn get_character(&self, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>>;
    fn list_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>>;
    fn put_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord>;
    fn get_persona(&self, id: &str) -> CoreResult<Option<RoleplayPlayerPersonaRecord>>;
    fn list_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>>;
    fn put_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord>;
    fn get_session_metadata(&self, id: &str) -> CoreResult<Option<RoleplaySessionMetadataRecord>>;
    fn list_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>>;
    fn put_import(&self, write: &RoleplayImportWrite) -> CoreResult<RoleplayImportRecord>;
    fn get_import(&self, id: &str) -> CoreResult<Option<RoleplayImportRecord>>;
    fn list_imports(&self, query: &RoleplayImportQuery) -> CoreResult<Vec<RoleplayImportRecord>>;
}

impl RoleplayRecordsStore for CoreCoordinationStore {
    fn put_character(&self, write: &RoleplayCharacterWrite) -> CoreResult<RoleplayCharacterRecord> {
        self.put_roleplay_character(write)
    }
    fn get_character(&self, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>> {
        self.get_roleplay_character(id)
    }
    fn list_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        self.list_roleplay_characters(query)
    }
    fn put_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        self.put_roleplay_player_persona(write)
    }
    fn get_persona(&self, id: &str) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        self.get_roleplay_player_persona(id)
    }
    fn list_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        self.list_roleplay_player_personas(query)
    }
    fn put_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        self.put_roleplay_session_metadata(write)
    }
    fn get_session_metadata(&self, id: &str) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        self.get_roleplay_session_metadata(id)
    }
    fn list_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        self.list_roleplay_session_metadata(query)
    }
    fn put_import(&self, write: &RoleplayImportWrite) -> CoreResult<RoleplayImportRecord> {
        self.put_roleplay_import(write)
    }
    fn get_import(&self, id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        self.get_roleplay_import(id)
    }
    fn list_imports(&self, query: &RoleplayImportQuery) -> CoreResult<Vec<RoleplayImportRecord>> {
        self.list_roleplay_imports(query)
    }
}
