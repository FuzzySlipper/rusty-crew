use super::*;

impl CoreEngine {
    pub fn put_roleplay_character(
        &self,
        write: &RoleplayCharacterWrite,
    ) -> CoreResult<RoleplayCharacterRecord> {
        RoleplayRecordsStore::put_character(&self.store, write)
    }
    pub fn get_roleplay_character(&self, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>> {
        RoleplayRecordsStore::get_character(&self.store, id)
    }
    pub fn list_roleplay_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        RoleplayRecordsStore::list_characters(&self.store, query)
    }
    pub fn put_roleplay_player_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        RoleplayRecordsStore::put_persona(&self.store, write)
    }
    pub fn get_roleplay_player_persona(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        RoleplayRecordsStore::get_persona(&self.store, id)
    }
    pub fn list_roleplay_player_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        RoleplayRecordsStore::list_personas(&self.store, query)
    }
    pub fn put_roleplay_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        RoleplayRecordsStore::put_session_metadata(&self.store, write)
    }
    pub fn get_roleplay_session_metadata(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        RoleplayRecordsStore::get_session_metadata(&self.store, id)
    }
    pub fn list_roleplay_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        RoleplayRecordsStore::list_session_metadata(&self.store, query)
    }
    pub fn apply_roleplay_session_projection(
        &self,
        write: &RoleplaySessionProjectionWrite,
    ) -> CoreResult<RoleplaySessionProjectionRecord> {
        RoleplayRecordsStore::apply_session_projection(&self.store, write)
    }
    pub fn put_roleplay_import(
        &self,
        write: &RoleplayImportWrite,
    ) -> CoreResult<RoleplayImportRecord> {
        RoleplayRecordsStore::put_import(&self.store, write)
    }
    pub fn get_roleplay_import(&self, id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        RoleplayRecordsStore::get_import(&self.store, id)
    }
    pub fn list_roleplay_imports(
        &self,
        query: &RoleplayImportQuery,
    ) -> CoreResult<Vec<RoleplayImportRecord>> {
        RoleplayRecordsStore::list_imports(&self.store, query)
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        RoleplayLoreStore::add_lore_record(&self.store, write)
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        RoleplayLoreStore::replace_lore_record(&self.store, replace)
    }

    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        RoleplayLoreStore::supersede_lore_record(&self.store, supersede)
    }

    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        RoleplayLoreStore::tombstone_lore_record(&self.store, tombstone)
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        RoleplayLoreStore::query_lore_records(&self.store, query)
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        RoleplayLoreStore::get_lore_record(&self.store, record_id)
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        RoleplayLoreStore::lore_provenance_events(&self.store, record_id)
    }

    pub fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        RoleplayLoreStore::create_lore_layer(&self.store, write)
    }

    pub fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        RoleplayLoreStore::get_lore_layer(&self.store, layer_id)
    }

    pub fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        RoleplayLoreStore::list_lore_layers_by_profile(&self.store, profile_id)
    }

    pub fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        RoleplayLoreStore::update_lore_layer(&self.store, update)
    }

    pub fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        RoleplayLoreStore::archive_lore_layer(&self.store, archive)
    }

    pub fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        RoleplayLoreStore::get_lore_layer_config(&self.store, layer_id)
    }

    pub fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        RoleplayLoreStore::set_lore_layer_config(&self.store, write)
    }

    pub fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        RoleplayLoreStore::add_entry_to_layer(&self.store, link)
    }

    pub fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        RoleplayLoreStore::capture_lore_fact(&self.store, capture)
    }

    pub fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        RoleplayLoreStore::promote_lore_entry(&self.store, promotion)
    }

    pub fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        RoleplayLoreStore::remove_entry_from_layer(&self.store, layer_id, record_id)
    }

    pub fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        RoleplayLoreStore::set_entry_constant(&self.store, layer_id, record_id, is_constant)
    }

    pub fn list_entries_by_layer(
        &self,
        layer_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        RoleplayLoreStore::list_entries_by_layer(&self.store, layer_id)
    }

    pub fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        RoleplayLoreStore::set_chat_layers(&self.store, write)
    }

    pub fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        RoleplayLoreStore::get_chat_layers(&self.store, chat_id)
    }

    pub fn toggle_chat_layer(
        &self,
        chat_id: &str,
        layer_id: &str,
        enabled: bool,
    ) -> CoreResult<()> {
        RoleplayLoreStore::toggle_chat_layer(&self.store, chat_id, layer_id, enabled)
    }

    pub fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        RoleplayLoreStore::reorder_chat_layers(&self.store, chat_id, layer_ids)
    }

    pub fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        RoleplayLoreStore::recall_lore(&self.store, query)
    }

    pub fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        RoleplayLoreStore::list_recall_traces(&self.store, query)
    }

    pub fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        RoleplayLoreStore::get_recall_trace(&self.store, trace_id)
    }
}
