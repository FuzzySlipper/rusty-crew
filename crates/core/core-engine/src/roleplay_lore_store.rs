use rusty_crew_core_persistence::*;
use rusty_crew_core_protocol::CoreResult;

pub(crate) trait RoleplayLoreStore {
    fn add_lore_record(&self, write: &RoleplayLoreWrite) -> CoreResult<RoleplayLoreRecord>;
    fn replace_lore_record(&self, replace: &RoleplayLoreReplace) -> CoreResult<RoleplayLoreRecord>;
    fn supersede_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)>;
    fn tombstone_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord>;
    fn query_lore_records(&self, query: &RoleplayLoreQuery) -> CoreResult<Vec<RoleplayLoreRecord>>;
    fn get_lore_record(&self, record_id: &str) -> CoreResult<Option<RoleplayLoreRecord>>;
    fn lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>>;
    fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord>;
    fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>>;
    fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>>;
    fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord>;
    fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord>;
    fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>>;
    fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord>;
    fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()>;
    fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin>;
    fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin>;
    fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()>;
    fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()>;
    fn list_entries_by_layer(&self, layer_id: &str) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>>;
    fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()>;
    fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>>;
    fn toggle_chat_layer(&self, chat_id: &str, layer_id: &str, enabled: bool) -> CoreResult<()>;
    fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()>;
    fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult>;
    fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>>;
    fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>>;
}

impl RoleplayLoreStore for CoreCoordinationStore {
    fn add_lore_record(&self, write: &RoleplayLoreWrite) -> CoreResult<RoleplayLoreRecord> {
        self.memory().add_roleplay_lore_record(write)
    }

    fn replace_lore_record(&self, replace: &RoleplayLoreReplace) -> CoreResult<RoleplayLoreRecord> {
        self.memory().replace_roleplay_lore_record(replace)
    }

    fn supersede_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        self.supersede_roleplay_lore_record(supersede)
    }

    fn tombstone_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        self.tombstone_roleplay_lore_record(tombstone)
    }

    fn query_lore_records(&self, query: &RoleplayLoreQuery) -> CoreResult<Vec<RoleplayLoreRecord>> {
        self.memory().query_roleplay_lore_records(query)
    }

    fn get_lore_record(&self, record_id: &str) -> CoreResult<Option<RoleplayLoreRecord>> {
        self.memory().get_roleplay_lore_record(record_id)
    }

    fn lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        self.roleplay_lore_provenance_events(record_id)
    }

    fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        self.create_lore_layer(write)
    }

    fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        self.get_lore_layer(layer_id)
    }

    fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        self.list_lore_layers_by_profile(profile_id)
    }

    fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        self.update_lore_layer(update)
    }

    fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        self.archive_lore_layer(archive)
    }

    fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        self.get_lore_layer_config(layer_id)
    }

    fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        self.set_lore_layer_config(write)
    }

    fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        self.add_entry_to_layer(link)
    }

    fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        self.capture_lore_fact(capture)
    }

    fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        self.promote_lore_entry(promotion)
    }

    fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        self.remove_entry_from_layer(layer_id, record_id)
    }

    fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        self.set_entry_constant(layer_id, record_id, is_constant)
    }

    fn list_entries_by_layer(&self, layer_id: &str) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        self.list_entries_by_layer(layer_id)
    }

    fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        self.set_chat_layers(write)
    }

    fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        self.get_chat_layers(chat_id)
    }

    fn toggle_chat_layer(&self, chat_id: &str, layer_id: &str, enabled: bool) -> CoreResult<()> {
        self.toggle_chat_layer(chat_id, layer_id, enabled)
    }

    fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        self.reorder_chat_layers(chat_id, layer_ids)
    }

    fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        self.recall_lore(query)
    }

    fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        self.list_recall_traces(query)
    }

    fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        self.get_recall_trace(trace_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRoleplayLoreStore {
        chat_layers: Mutex<HashMap<String, Vec<RoleplayChatLayerRecord>>>,
    }

    impl RoleplayLoreStore for FakeRoleplayLoreStore {
        fn add_lore_record(&self, _write: &RoleplayLoreWrite) -> CoreResult<RoleplayLoreRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn replace_lore_record(
            &self,
            _replace: &RoleplayLoreReplace,
        ) -> CoreResult<RoleplayLoreRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn supersede_lore_record(
            &self,
            _supersede: &RoleplayLoreSupersede,
        ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
            unimplemented!("not needed for chat layer fake")
        }

        fn tombstone_lore_record(
            &self,
            _tombstone: &RoleplayLoreTombstone,
        ) -> CoreResult<RoleplayLoreRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn query_lore_records(
            &self,
            _query: &RoleplayLoreQuery,
        ) -> CoreResult<Vec<RoleplayLoreRecord>> {
            Ok(Vec::new())
        }

        fn get_lore_record(&self, _record_id: &str) -> CoreResult<Option<RoleplayLoreRecord>> {
            Ok(None)
        }

        fn lore_provenance_events(
            &self,
            _record_id: &str,
        ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
            Ok(Vec::new())
        }

        fn create_lore_layer(
            &self,
            _write: &RoleplayLoreLayerWrite,
        ) -> CoreResult<RoleplayLoreLayerRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn get_lore_layer(&self, _layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
            Ok(None)
        }

        fn list_lore_layers_by_profile(
            &self,
            _profile_id: &str,
        ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
            Ok(Vec::new())
        }

        fn update_lore_layer(
            &self,
            _update: &RoleplayLoreLayerUpdate,
        ) -> CoreResult<RoleplayLoreLayerRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn archive_lore_layer(
            &self,
            _archive: &RoleplayLoreLayerArchive,
        ) -> CoreResult<RoleplayLoreLayerRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn get_lore_layer_config(
            &self,
            _layer_id: &str,
        ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
            Ok(None)
        }

        fn set_lore_layer_config(
            &self,
            _write: &RoleplayLoreLayerConfigWrite,
        ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
            unimplemented!("not needed for chat layer fake")
        }

        fn add_entry_to_layer(&self, _link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
            Ok(())
        }

        fn capture_lore_fact(
            &self,
            _capture: &RoleplayLoreFactCapture,
        ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
            unimplemented!("not needed for chat layer fake")
        }

        fn promote_lore_entry(
            &self,
            _promotion: &RoleplayLoreEntryPromotion,
        ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
            unimplemented!("not needed for chat layer fake")
        }

        fn remove_entry_from_layer(&self, _layer_id: &str, _record_id: &str) -> CoreResult<()> {
            Ok(())
        }

        fn set_entry_constant(
            &self,
            _layer_id: &str,
            _record_id: &str,
            _is_constant: bool,
        ) -> CoreResult<()> {
            Ok(())
        }

        fn list_entries_by_layer(
            &self,
            _layer_id: &str,
        ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
            Ok(Vec::new())
        }

        fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
            let records = write
                .layers
                .iter()
                .map(|link| RoleplayChatLayerRecord {
                    chat_id: write.chat_id.clone(),
                    layer_id: link.layer_id.clone(),
                    priority: link.priority,
                    enabled: link.enabled,
                    created_at: write.now.clone(),
                    layer: layer_record(&link.layer_id, &write.now),
                })
                .collect();
            self.chat_layers
                .lock()
                .unwrap()
                .insert(write.chat_id.clone(), records);
            Ok(())
        }

        fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
            Ok(self
                .chat_layers
                .lock()
                .unwrap()
                .get(chat_id)
                .cloned()
                .unwrap_or_default())
        }

        fn toggle_chat_layer(
            &self,
            chat_id: &str,
            layer_id: &str,
            enabled: bool,
        ) -> CoreResult<()> {
            if let Some(layers) = self.chat_layers.lock().unwrap().get_mut(chat_id) {
                if let Some(layer) = layers.iter_mut().find(|layer| layer.layer_id == layer_id) {
                    layer.enabled = enabled;
                }
            }
            Ok(())
        }

        fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
            if let Some(layers) = self.chat_layers.lock().unwrap().get_mut(chat_id) {
                for layer in layers {
                    if let Some(index) = layer_ids.iter().position(|id| id == &layer.layer_id) {
                        layer.priority = index as i64;
                    }
                }
            }
            Ok(())
        }

        fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
            Ok(LoreRecallResult {
                chat_id: query.chat_id.clone(),
                entries: Vec::new(),
                entries_considered: 0,
                tokens_consumed: 0,
                token_budget: query.token_budget,
                trace: None,
            })
        }

        fn list_recall_traces(
            &self,
            _query: &LoreRecallTraceQuery,
        ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
            Ok(Vec::new())
        }

        fn get_recall_trace(&self, _trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
            Ok(None)
        }
    }

    #[test]
    fn chat_layer_assignment_uses_fake_roleplay_lore_store() {
        let store = FakeRoleplayLoreStore::default();
        let now = "2026-07-09T09:20:00Z".to_string();

        RoleplayLoreStore::set_chat_layers(
            &store,
            &RoleplayChatLayersWrite {
                chat_id: "chat-1".to_string(),
                layers: vec![
                    RoleplayChatLayerLink {
                        layer_id: "characters".to_string(),
                        priority: 0,
                        enabled: true,
                    },
                    RoleplayChatLayerLink {
                        layer_id: "world".to_string(),
                        priority: 1,
                        enabled: true,
                    },
                ],
                now,
            },
        )
        .unwrap();
        RoleplayLoreStore::toggle_chat_layer(&store, "chat-1", "world", false).unwrap();
        RoleplayLoreStore::reorder_chat_layers(
            &store,
            "chat-1",
            &["world".to_string(), "characters".to_string()],
        )
        .unwrap();

        let layers = RoleplayLoreStore::get_chat_layers(&store, "chat-1").unwrap();

        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].layer_id, "characters");
        assert_eq!(layers[0].priority, 1);
        assert_eq!(layers[1].layer_id, "world");
        assert!(!layers[1].enabled);
        assert_eq!(layers[1].priority, 0);
    }

    fn layer_record(layer_id: &str, now: &str) -> RoleplayLoreLayerRecord {
        RoleplayLoreLayerRecord {
            layer_id: layer_id.to_string(),
            profile_id: "roleplay-profile".to_string(),
            name: layer_id.to_string(),
            description: None,
            purpose: RoleplayLoreLayerPurpose::Mixed,
            write_policy: RoleplayLoreLayerWritePolicy::Manual,
            is_archived: false,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        }
    }
}
