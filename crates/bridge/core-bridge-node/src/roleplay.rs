use super::*;

impl NativeBridge {
    pub fn put_roleplay_character(
        &self,
        write: &RoleplayCharacterWrite,
    ) -> CoreResult<RoleplayCharacterRecord> {
        self.engine()?.put_roleplay_character(write)
    }
    pub fn get_roleplay_character(&self, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>> {
        self.engine()?.get_roleplay_character(id)
    }
    pub fn list_roleplay_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        self.engine()?.list_roleplay_characters(query)
    }
    pub fn put_roleplay_player_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        self.engine()?.put_roleplay_player_persona(write)
    }
    pub fn get_roleplay_player_persona(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        self.engine()?.get_roleplay_player_persona(id)
    }
    pub fn list_roleplay_player_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        self.engine()?.list_roleplay_player_personas(query)
    }
    pub fn put_roleplay_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        self.engine()?.put_roleplay_session_metadata(write)
    }
    pub fn get_roleplay_session_metadata(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        self.engine()?.get_roleplay_session_metadata(id)
    }
    pub fn list_roleplay_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        self.engine()?.list_roleplay_session_metadata(query)
    }
    pub fn apply_roleplay_session_projection(
        &self,
        write: &RoleplaySessionProjectionWrite,
    ) -> CoreResult<RoleplaySessionProjectionRecord> {
        self.engine()?.apply_roleplay_session_projection(write)
    }
    pub fn put_roleplay_import(
        &self,
        write: &RoleplayImportWrite,
    ) -> CoreResult<RoleplayImportRecord> {
        self.engine()?.put_roleplay_import(write)
    }
    pub fn get_roleplay_import(&self, id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        self.engine()?.get_roleplay_import(id)
    }
    pub fn list_roleplay_imports(
        &self,
        query: &RoleplayImportQuery,
    ) -> CoreResult<Vec<RoleplayImportRecord>> {
        self.engine()?.list_roleplay_imports(query)
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        self.engine()?.add_roleplay_lore_record(write)
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        self.engine()?.replace_roleplay_lore_record(replace)
    }

    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        self.engine()?.supersede_roleplay_lore_record(supersede)
    }

    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        self.engine()?.tombstone_roleplay_lore_record(tombstone)
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        self.engine()?.query_roleplay_lore_records(query)
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        self.engine()?.get_roleplay_lore_record(record_id)
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        self.engine()?.roleplay_lore_provenance_events(record_id)
    }

    pub fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        self.engine()?.create_lore_layer(write)
    }

    pub fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        self.engine()?.get_lore_layer(layer_id)
    }

    pub fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        self.engine()?.list_lore_layers_by_profile(profile_id)
    }

    pub fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        self.engine()?.update_lore_layer(update)
    }

    pub fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        self.engine()?.archive_lore_layer(archive)
    }

    pub fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        self.engine()?.get_lore_layer_config(layer_id)
    }

    pub fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        self.engine()?.set_lore_layer_config(write)
    }

    pub fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        self.engine()?.add_entry_to_layer(link)
    }

    pub fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        self.engine()?.capture_lore_fact(capture)
    }

    pub fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        self.engine()?.promote_lore_entry(promotion)
    }

    pub fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        self.engine()?.remove_entry_from_layer(layer_id, record_id)
    }

    pub fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        self.engine()?
            .set_entry_constant(layer_id, record_id, is_constant)
    }

    pub fn list_entries_by_layer(
        &self,
        layer_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        self.engine()?.list_entries_by_layer(layer_id)
    }

    pub fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        self.engine()?.set_chat_layers(write)
    }

    pub fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        self.engine()?.get_chat_layers(chat_id)
    }

    pub fn toggle_chat_layer(
        &self,
        chat_id: &str,
        layer_id: &str,
        enabled: bool,
    ) -> CoreResult<()> {
        self.engine()?.toggle_chat_layer(chat_id, layer_id, enabled)
    }

    pub fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        self.engine()?.reorder_chat_layers(chat_id, layer_ids)
    }

    pub fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        self.engine()?.recall_lore(query)
    }

    pub fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        self.engine()?.list_recall_traces(query)
    }

    pub fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        self.engine()?.get_recall_trace(trace_id)
    }
}
