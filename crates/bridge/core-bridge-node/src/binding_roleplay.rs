use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn plan_roleplay_assistant_alternative_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<RoleplayAssistantAlternativePlanInput>(
            &input_json,
            "roleplay assistant alternative plan input",
        )?;
        let plan = plan_assistant_alternative(input).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("{}: {}", error.reason_code, error.message),
            )
        })?;
        serialize_json(&plan, "roleplay assistant alternative plan")
    }

    #[napi]
    pub fn build_roleplay_prompt_context_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            parse_json::<RoleplayPromptContextInput>(&input_json, "roleplay prompt context input")?;
        let output = build_prompt_context(input);
        serialize_json(&output, "roleplay prompt context output")
    }

    #[napi]
    pub fn roleplay_speaker_identity_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RoleplaySpeakerIdentityInput>(
            &input_json,
            "roleplay speaker identity input",
        )?;
        let snapshot = speaker_identity_snapshot(input);
        serialize_json(&snapshot, "roleplay speaker identity snapshot")
    }

    #[napi]
    pub fn write_roleplay_character_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            parse_json::<RoleplayCharacterWriteInput>(&input_json, "roleplay character write")?;
        let character = write_character(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&character, "roleplay character")
    }

    #[napi]
    pub fn merge_roleplay_character_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            parse_json::<RoleplayCharacterMergeInput>(&input_json, "roleplay character merge")?;
        let character = merge_character(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&character, "roleplay character")
    }

    #[napi]
    pub fn write_roleplay_player_persona_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RoleplayPlayerPersonaWriteInput>(
            &input_json,
            "roleplay player persona write",
        )?;
        let persona = write_player_persona(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&persona, "roleplay player persona")
    }

    #[napi]
    pub fn merge_roleplay_player_persona_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RoleplayPlayerPersonaMergeInput>(
            &input_json,
            "roleplay player persona merge",
        )?;
        let persona = merge_player_persona(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&persona, "roleplay player persona")
    }

    #[napi]
    pub fn patch_roleplay_session_metadata_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RoleplaySessionMetadataPatchInput>(
            &input_json,
            "roleplay session metadata patch",
        )?;
        let output = patch_session_metadata(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&output, "roleplay session metadata patch")
    }

    #[napi]
    pub fn normalize_roleplay_narrator_config_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<serde_json::Value>(&input_json, "roleplay narrator config")?;
        let config = normalize_narrator_config(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&config, "roleplay narrator config")
    }

    #[napi]
    pub fn roleplay_narrator_mandatory_explore_requests_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<RoleplayNarratorMandatoryExploreInput>(
            &input_json,
            "roleplay narrator mandatory explore input",
        )?;
        let requests = narrator_mandatory_explore_requests(input);
        serialize_json(&requests, "roleplay narrator mandatory explore requests")
    }

    #[napi]
    pub fn roleplay_narrator_auto_capture_request_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<RoleplayNarratorAutoCaptureInput>(
            &input_json,
            "roleplay narrator auto capture input",
        )?;
        let request = narrator_auto_capture_request(input);
        serialize_json(&request, "roleplay narrator auto capture request")
    }

    #[napi]
    pub fn start_roleplay_narrator_turn_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            parse_json::<RoleplayNarratorStartInput>(&input_json, "roleplay narrator start input")?;
        let plan = start_narrator_turn(input);
        serialize_json(&plan, "roleplay narrator phase plan")
    }

    #[napi]
    pub fn next_roleplay_narrator_phase_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            parse_json::<RoleplayNarratorNextInput>(&input_json, "roleplay narrator next input")?;
        let plan = next_narrator_phase(input).map_err(roleplay_domain_error_to_napi)?;
        serialize_json(&plan, "roleplay narrator phase plan")
    }

    #[napi]
    pub fn roleplay_narrator_review_requests_revision(&self, feedback: String) -> bool {
        narrator_review_requests_revision(&feedback)
    }

    #[napi]
    pub fn add_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayLoreWrite>(&input_json, "roleplay lore write")?;
        let record = bridge
            .add_roleplay_lore_record(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn replace_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let replace = parse_json::<RoleplayLoreReplace>(&input_json, "roleplay lore replace")?;
        let record = bridge
            .replace_roleplay_lore_record(&replace)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn supersede_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let supersede =
            parse_json::<RoleplayLoreSupersede>(&input_json, "roleplay lore supersede")?;
        let records = bridge
            .supersede_roleplay_lore_record(&supersede)
            .map_err(to_napi_error)?;
        serialize_json(&records, "roleplay lore supersede records")
    }

    #[napi]
    pub fn tombstone_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let tombstone =
            parse_json::<RoleplayLoreTombstone>(&input_json, "roleplay lore tombstone")?;
        let record = bridge
            .tombstone_roleplay_lore_record(&tombstone)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn query_lore_entries_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<RoleplayLoreQuery>(&input_json, "roleplay lore query")?;
        let records = bridge
            .query_roleplay_lore_records(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "roleplay lore records")
    }

    #[napi]
    pub fn get_lore_entry_json(&self, record_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_roleplay_lore_record(&record_id)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn lore_entry_provenance_events_json(&self, record_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let events = bridge
            .roleplay_lore_provenance_events(&record_id)
            .map_err(to_napi_error)?;
        serialize_json(&events, "roleplay lore provenance events")
    }

    #[napi]
    pub fn create_lore_layer_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayLoreLayerWrite>(&input_json, "roleplay lore layer write")?;
        let layer = bridge.create_lore_layer(&write).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn get_lore_layer_json(&self, layer_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let layer = bridge.get_lore_layer(&layer_id).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn list_lore_layers_json(&self, profile_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let layers = bridge
            .list_lore_layers_by_profile(&profile_id)
            .map_err(to_napi_error)?;
        serialize_json(&layers, "roleplay lore layers")
    }

    #[napi]
    pub fn update_lore_layer_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let update =
            parse_json::<RoleplayLoreLayerUpdate>(&input_json, "roleplay lore layer update")?;
        let layer = bridge.update_lore_layer(&update).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn archive_lore_layer_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let archive =
            parse_json::<RoleplayLoreLayerArchive>(&input_json, "roleplay lore layer archive")?;
        let layer = bridge.archive_lore_layer(&archive).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn get_lore_layer_config_json(&self, layer_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let config = bridge
            .get_lore_layer_config(&layer_id)
            .map_err(to_napi_error)?;
        serialize_json(&config, "roleplay lore layer config")
    }

    #[napi]
    pub fn set_lore_layer_config_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayLoreLayerConfigWrite>(
            &input_json,
            "roleplay lore layer config write",
        )?;
        let config = bridge
            .set_lore_layer_config(&write)
            .map_err(to_napi_error)?;
        serialize_json(&config, "roleplay lore layer config")
    }

    #[napi]
    pub fn add_entry_to_layer_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let link = parse_json::<RoleplayLoreLayerEntryLink>(
            &input_json,
            "roleplay lore layer entry link",
        )?;
        bridge.add_entry_to_layer(&link).map_err(to_napi_error)
    }

    #[napi]
    pub fn remove_entry_from_layer_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireRemoveLoreEntryFromLayerRequest>(
            &input_json,
            "remove roleplay lore layer entry request",
        )?;
        bridge
            .remove_entry_from_layer(&request.layer_id, &request.record_id)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn set_entry_constant_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireSetLoreEntryConstantRequest>(
            &input_json,
            "set roleplay lore layer entry constant request",
        )?;
        bridge
            .set_entry_constant(&request.layer_id, &request.record_id, request.is_constant)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn list_entries_by_layer_json(&self, layer_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let entries = bridge
            .list_entries_by_layer(&layer_id)
            .map_err(to_napi_error)?;
        serialize_json(&entries, "roleplay lore layer entries")
    }

    #[napi]
    pub fn capture_lore_fact_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let capture =
            parse_json::<RoleplayLoreFactCapture>(&input_json, "roleplay lore fact capture")?;
        let entry = bridge.capture_lore_fact(&capture).map_err(to_napi_error)?;
        serialize_json(&entry, "roleplay lore layer entry")
    }

    #[napi]
    pub fn promote_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let promotion =
            parse_json::<RoleplayLoreEntryPromotion>(&input_json, "roleplay lore entry promotion")?;
        let entry = bridge
            .promote_lore_entry(&promotion)
            .map_err(to_napi_error)?;
        serialize_json(&entry, "roleplay lore layer entry")
    }

    #[napi]
    pub fn set_chat_layers_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayChatLayersWrite>(&input_json, "roleplay chat layers")?;
        bridge.set_chat_layers(&write).map_err(to_napi_error)
    }

    #[napi]
    pub fn get_chat_layers_json(&self, chat_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let layers = bridge.get_chat_layers(&chat_id).map_err(to_napi_error)?;
        serialize_json(&layers, "roleplay chat layers")
    }

    #[napi]
    pub fn toggle_chat_layer_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request =
            parse_json::<WireToggleChatLayerRequest>(&input_json, "toggle roleplay chat layer")?;
        bridge
            .toggle_chat_layer(&request.chat_id, &request.layer_id, request.enabled)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn reorder_chat_layers_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireReorderChatLayersRequest>(
            &input_json,
            "reorder roleplay chat layers",
        )?;
        bridge
            .reorder_chat_layers(&request.chat_id, &request.layer_ids)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn recall_lore_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<LoreRecallQuery>(&input_json, "roleplay lore recall query")?;
        let recall = bridge.recall_lore(&query).map_err(to_napi_error)?;
        serialize_json(&recall, "roleplay lore recall")
    }

    #[napi]
    pub fn list_recall_traces_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<LoreRecallTraceQuery>(&input_json, "roleplay lore recall trace query")?;
        let traces = bridge.list_recall_traces(&query).map_err(to_napi_error)?;
        serialize_json(&traces, "roleplay lore recall traces")
    }

    #[napi]
    pub fn get_recall_trace_json(&self, trace_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let trace = bridge.get_recall_trace(&trace_id).map_err(to_napi_error)?;
        serialize_json(&trace, "roleplay lore recall trace")
    }
}

fn roleplay_domain_error_to_napi(
    error: rusty_crew_roleplay_core::RoleplayDomainError,
) -> napi::Error {
    napi::Error::new(
        napi::Status::InvalidArg,
        format!("{}: {}", error.reason_code, error.message),
    )
}
