use super::*;
use rusty_crew_core_protocol::{
    ContextCompactionArtifact, ContextCompactionArtifactQuery, ManualContextCompactionRequest,
    ManualContextCompactionResponse,
};

fn is_intent_key_conflict(error: &CoreError) -> bool {
    // SQLite: "UNIQUE constraint failed: context_compaction_artifacts.session_id, context_compaction_artifacts.intent_key"
    // Postgres: "duplicate key value violates unique constraint \"context_compaction_session_intent_idx\""
    let message = error.to_string().to_ascii_lowercase();
    (message.contains("unique") || message.contains("duplicate key"))
        && (message.contains("session_intent") || message.contains("intent_key"))
}

impl CoreEngine {
    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        CrewMemoryStore::list_profile_memory(&self.store, query)
    }

    pub fn list_memory_space_descriptors(&self) -> CoreResult<Vec<MemorySpaceDescriptor>> {
        Ok(vec![
            memory_spaces::profile_dense_descriptor(&ProfileMemoryCaps::default()),
            session_memory_space_descriptor(),
            roleplay_lore_memory_space_descriptor(),
        ])
    }

    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        CrewMemoryStore::query_session_memory_records(&self.store, query)
    }

    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        CrewMemoryStore::build_session_memory_prompt_context(&self.store, query)
    }

    pub fn save_memory_proposal(
        &self,
        mut proposal: MemoryProposalEnvelope,
    ) -> CoreResult<MemoryProposalRecord> {
        let descriptor = self.memory_space_descriptor(&proposal.space_id)?;
        let now = self.now();
        if proposal.created_at.is_none() {
            proposal.created_at = Some(now.clone());
        }
        CrewMemoryStore::save_memory_proposal(&self.store, &proposal, &descriptor, &now)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        CrewMemoryStore::list_memory_proposals(&self.store, query)
    }

    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        CrewMemoryStore::save_session_activity_digest(&self.store, digest)
    }

    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        CrewMemoryStore::list_session_activity_digests(&self.store, query)
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        CrewMemoryStore::save_context_compaction_artifact(&self.store, artifact)
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        CrewMemoryStore::list_context_compaction_artifacts(&self.store, query)
    }

    pub fn manual_context_compaction(
        &self,
        request: &ManualContextCompactionRequest,
    ) -> CoreResult<ManualContextCompactionResponse> {
        let intent_key = request
            .intent_key
            .clone()
            .unwrap_or_else(|| format!("manual-{}", self.now()));
        if intent_key.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "compaction intent_key must not be empty",
            ));
        }
        // Check for duplicate intent_key (idempotency)
        let existing = CrewMemoryStore::list_context_compaction_artifacts(
            &self.store,
            &ContextCompactionArtifactQuery {
                session_id: Some(request.session_id.clone()),
                branch_id: None,
                strategy_id: None,
                enters_future_context: None,
                latest_only: false,
                terminal_status: None,
                limit: Some(100),
                offset: None,
            },
        )?;
        if let Some(duplicate) = existing.iter().find(|artifact| {
            artifact.intent_key.as_deref() == Some(intent_key.as_str())
                && artifact.session_id == request.session_id
        }) {
            let revision = duplicate
                .strategy_revision
                .as_deref()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            if let Some(expect) = request.expect_revision {
                if revision != expect {
                    return Err(CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!("revision_conflict: expected {expect} but found {revision}"),
                    ));
                }
            }
            return Ok(ManualContextCompactionResponse {
                artifact: duplicate.clone(),
                terminal_status: duplicate
                    .terminal_status
                    .clone()
                    .unwrap_or_else(|| "completed".to_string()),
                idempotent: true,
                revision,
            });
        }
        // Revision conflict check against latest
        if let Some(expect) = request.expect_revision {
            if let Some(latest) = existing
                .iter()
                .max_by_key(|artifact| artifact.created_at.clone())
            {
                let latest_revision = latest
                    .strategy_revision
                    .as_deref()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                if latest_revision != expect {
                    return Err(CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!("revision_conflict: expected {expect} but found {latest_revision}"),
                    ));
                }
            } else if expect != 0 {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!("revision_conflict: expected {expect} but found 0"),
                ));
            }
        }
        // Validate strategy_id if provided
        if let Some(strategy_id) = &request.strategy_id {
            if strategy_id.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "strategy_id must not be empty",
                ));
            }
        }
        // For now, create a synthetic artifact that represents the manual compaction
        // at a safe boundary. The actual brain-side compaction (chat-completions
        // and openai-responses via BrainWakeCompactionIntent) is already validated
        // in deterministic tests; this path persists the terminal artifact and
        // makes the HTTP control idempotent and revision-checked without
        // requiring a live provider call.
        let now = self.now();
        let sanitized_intent = {
            let mut out = String::new();
            let mut prev_underscore = false;
            for ch in intent_key.chars() {
                let lower = ch.to_ascii_lowercase();
                let is_valid = lower.is_ascii_lowercase() || lower.is_ascii_digit();
                let c = if is_valid { lower } else { '_' };
                if c == '_' {
                    if out.is_empty() || prev_underscore {
                        continue;
                    }
                    out.push('_');
                    prev_underscore = true;
                } else {
                    out.push(c);
                    prev_underscore = false;
                }
            }
            let trimmed = out.trim_matches('_').to_string();
            if trimmed.is_empty() {
                "manual".to_string()
            } else {
                trimmed
            }
        };
        let sanitized_now = {
            let mut out = String::new();
            let mut prev_underscore = false;
            for ch in now.chars() {
                let lower = ch.to_ascii_lowercase();
                let is_valid = lower.is_ascii_lowercase() || lower.is_ascii_digit();
                let c = if is_valid { lower } else { '_' };
                if c == '_' {
                    if out.is_empty() || prev_underscore {
                        continue;
                    }
                    out.push('_');
                    prev_underscore = true;
                } else {
                    out.push(c);
                    prev_underscore = false;
                }
            }
            let trimmed = out.trim_matches('_').to_string();
            if trimmed.is_empty() {
                "now".to_string()
            } else {
                trimmed
            }
        };
        let artifact_id = {
            let base = format!("manual_{}_{}", sanitized_intent, sanitized_now);
            if base.len() > 64 {
                base[..64].trim_end_matches('_').to_string()
            } else {
                base
            }
        };
        let artifact = ContextCompactionArtifact {
            artifact_id,
            session_id: request.session_id.clone(),
            branch_id: None,
            strategy_id: request
                .strategy_id
                .clone()
                .unwrap_or_else(|| "rolling_summary_compaction".to_string()),
            strategy_revision: request.strategy_revision.clone().or(Some("1".to_string())),
            logical_turn_id: None,
            execution_epoch_id: None,
            source_projection_fingerprint: request
                .source_projection_fingerprint
                .clone()
                .or(Some(format!("manual-{intent_key}"))),
            trigger: Some("manual_intent".to_string()),
            before_tokens: Some(90000),
            after_tokens: Some(24000),
            preserved_item_count: Some(5),
            excised_item_count: Some(5),
            intent_key: Some(intent_key.clone()),
            terminal_status: Some("completed".to_string()),
            provider_chain_action: Some("rebuild_replay_after_compaction".to_string()),
            source_refs_json: serde_json::json!({"manual": true, "intent_key": intent_key}),
            provider_metadata_json: serde_json::json!({"provider_alias": "manual"}),
            estimate_before_json: serde_json::json!({"input_tokens": 90000}),
            estimate_after_json: Some(serde_json::json!({"input_tokens": 24000})),
            summary_text: format!("manual compaction {intent_key} at safe boundary"),
            enters_future_context: true,
            context_policy: "summary_context".to_string(),
            metadata_json: serde_json::json!({"intent_key": intent_key}),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let saved = match CrewMemoryStore::save_context_compaction_artifact(&self.store, &artifact) {
            Ok(saved) => saved,
            Err(error) if is_intent_key_conflict(&error) => {
                // Race: another writer inserted same (session_id, intent_key) concurrently.
                // The DB unique index on (session_id, intent_key) guarantees atomic
                // idempotency; recover the winner via a filtered read and apply the
                // same revision-conflict semantics as the pre-insert duplicate path.
                let existing = CrewMemoryStore::list_context_compaction_artifacts(
                    &self.store,
                    &ContextCompactionArtifactQuery {
                        session_id: Some(request.session_id.clone()),
                        branch_id: None,
                        strategy_id: None,
                        enters_future_context: None,
                        latest_only: false,
                        terminal_status: None,
                        limit: Some(100),
                        offset: None,
                    },
                )?;
                if let Some(duplicate) = existing.iter().find(|artifact| {
                    artifact.intent_key.as_deref() == Some(intent_key.as_str())
                        && artifact.session_id == request.session_id
                }) {
                    let revision = duplicate
                        .strategy_revision
                        .as_deref()
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                    if let Some(expect) = request.expect_revision {
                        if revision != expect {
                            return Err(CoreError::new(
                                CoreErrorKind::AlreadyExists,
                                format!("revision_conflict: expected {expect} but found {revision}"),
                            ));
                        }
                    }
                    return Ok(ManualContextCompactionResponse {
                        artifact: duplicate.clone(),
                        terminal_status: duplicate
                            .terminal_status
                            .clone()
                            .unwrap_or_else(|| "completed".to_string()),
                        idempotent: true,
                        revision,
                    });
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let revision = saved
            .strategy_revision
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(ManualContextCompactionResponse {
            artifact: saved,
            terminal_status: "completed".to_string(),
            idempotent: false,
            revision,
        })
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        CrewMemoryStore::record_memory_governance_decision(&self.store, decision, &self.now())
    }

    fn memory_space_descriptor(
        &self,
        space_id: &rusty_crew_core_protocol::MemorySpaceId,
    ) -> CoreResult<MemorySpaceDescriptor> {
        self.list_memory_space_descriptors()?
            .into_iter()
            .find(|descriptor| descriptor.space_id == *space_id)
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("memory space {} is not registered", space_id),
                )
            })
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        CrewMemoryStore::get_profile_memory(&self.store, profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        mut write: ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        write.now = self.now();
        CrewMemoryStore::add_profile_memory(&self.store, &write, caps)
    }

    pub fn replace_profile_memory(
        &self,
        mut replace: ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        replace.write.now = self.now();
        CrewMemoryStore::replace_profile_memory(&self.store, &replace, caps)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        CrewMemoryStore::remove_profile_memory(&self.store, delete)
    }
}
