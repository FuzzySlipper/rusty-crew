//! PostgreSQL profile memory, session memory, memory governance, and roleplay lore repositories.

use super::*;

impl PostgresBackendStore {
    pub fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor> {
        vec![
            profile_dense_memory_space_descriptor(),
            roleplay_lore_memory_space_descriptor(),
        ]
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_profile_memory(&mut *client, &schema, query)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        validate_profile_memory_key(key, ProfileMemoryCaps::default().max_key_bytes)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_profile_memory(&mut *client, &schema, profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(write, caps)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start add PostgreSQL profile memory", error))?;
        let count = count_profile_memory_for_profile(&mut tx, &schema, &write.profile_id)?;
        if count >= u64::from(caps.max_records_per_profile) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile {} already has the maximum {} dense memory records",
                    write.profile_id, caps.max_records_per_profile
                ),
            ));
        }
        if get_profile_memory(
            &mut tx,
            &schema,
            &write.profile_id,
            &write.target,
            &write.key,
        )?
        .is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile memory {} for profile {} already exists",
                    write.key, write.profile_id
                ),
            ));
        }
        let record = insert_profile_memory_in_tx(&mut tx, &schema, write)?;
        tx.commit()
            .map_err(|error| postgres_error("commit add PostgreSQL profile memory", error))?;
        Ok(record)
    }

    pub fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(&replace.write, caps)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start replace PostgreSQL profile memory", error))?;
        let existing = get_profile_memory(
            &mut tx,
            &schema,
            &replace.write.profile_id,
            &replace.write.target,
            &replace.write.key,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "profile memory {} for profile {} not found",
                    replace.write.key, replace.write.profile_id
                ),
            )
        })?;
        if existing.revision != replace.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile memory revision mismatch for {}: expected {}, found {}",
                    replace.write.key, replace.expected_revision, existing.revision
                ),
            ));
        }
        let record = update_profile_memory_in_tx(
            &mut tx,
            &schema,
            &replace.write,
            existing.revision + 1,
            &existing.created_at,
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit replace PostgreSQL profile memory", error))?;
        Ok(record)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_key(&delete.key, ProfileMemoryCaps::default().max_key_bytes)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL profile memory", error))?;
        let existing = get_profile_memory(
            &mut tx,
            &schema,
            &delete.profile_id,
            &delete.target,
            &delete.key,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "profile memory {} for profile {} not found",
                    delete.key, delete.profile_id
                ),
            )
        })?;
        if existing.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile memory revision mismatch for {}: expected {}, found {}",
                    delete.key, delete.expected_revision, existing.revision
                ),
            ));
        }
        let (target_type, target_id) =
            profile_memory_target_parts(&delete.profile_id, &delete.target);
        tx.execute(
            &format!(
                "DELETE FROM {schema}.profile_memories
                 WHERE profile_id = $1
                   AND target_type = $2
                   AND target_id = $3
                   AND memory_key = $4"
            ),
            &[&delete.profile_id.0, &target_type, &target_id, &delete.key],
        )
        .map_err(|error| postgres_error("remove PostgreSQL profile memory", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL profile memory", error))?;
        Ok(existing)
    }

    pub fn add_session_memory_record(
        &self,
        write: &SessionMemoryRecordWrite,
    ) -> CoreResult<SessionMemoryRecord> {
        crate::validate_session_memory_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start add PostgreSQL session memory record", error))?;
        validate_postgres_session_memory_scope(
            &mut tx,
            &schema,
            &write.session_id,
            &write.scope,
            &write.branch_id,
        )?;
        if get_session_memory_record_in_tx(&mut tx, &schema, &write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("session memory record {} already exists", write.record_id),
            ));
        }
        insert_session_memory_record_in_tx(&mut tx, &schema, write)?;
        let record = get_session_memory_record_in_tx(&mut tx, &schema, &write.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "created PostgreSQL session memory record was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit add PostgreSQL session memory record", error)
        })?;
        Ok(record)
    }

    pub fn replace_session_memory_record(
        &self,
        replace: &SessionMemoryReplace,
    ) -> CoreResult<SessionMemoryRecord> {
        validate_postgres_session_memory_revision_input(
            &replace.record_id,
            replace.expected_revision,
            &replace.evidence_refs,
            replace.confidence,
            &replace.durability_rationale,
        )?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start replace PostgreSQL session memory record", error)
        })?;
        let existing = active_session_memory_record_for_update(
            &mut tx,
            &schema,
            &replace.record_id,
            replace.expected_revision,
        )?;
        crate::validate_session_memory_content(&existing.shape, &replace.content)?;
        update_session_memory_record_content_in_tx(
            &mut tx,
            &schema,
            replace,
            existing.revision + 1,
        )?;
        let record = get_session_memory_record_in_tx(&mut tx, &schema, &replace.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replaced PostgreSQL session memory record was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit replace PostgreSQL session memory record", error)
        })?;
        Ok(record)
    }

    pub fn supersede_session_memory_record(
        &self,
        supersede: &SessionMemorySupersede,
    ) -> CoreResult<(SessionMemoryRecord, SessionMemoryRecord)> {
        crate::validate_session_memory_write(&supersede.replacement)?;
        if supersede.replacement.supersedes_record_id.as_deref()
            != Some(supersede.record_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory replacement must reference the superseded record",
            ));
        }
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start supersede PostgreSQL session memory record", error)
        })?;
        let existing = active_session_memory_record_for_update(
            &mut tx,
            &schema,
            &supersede.record_id,
            supersede.expected_revision,
        )?;
        validate_postgres_session_memory_scope(
            &mut tx,
            &schema,
            &supersede.replacement.session_id,
            &supersede.replacement.scope,
            &supersede.replacement.branch_id,
        )?;
        if existing.session_id != supersede.replacement.session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory replacement must stay in the same session",
            ));
        }
        if get_session_memory_record_in_tx(&mut tx, &schema, &supersede.replacement.record_id)?
            .is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "session memory replacement {} already exists",
                    supersede.replacement.record_id
                ),
            ));
        }
        insert_session_memory_record_in_tx(&mut tx, &schema, &supersede.replacement)?;
        mark_session_memory_superseded_in_tx(
            &mut tx,
            &schema,
            &existing.record_id,
            &supersede.replacement.record_id,
            existing.revision + 1,
            &supersede.replacement.now,
        )?;
        let old_record = get_session_memory_record_in_tx(&mut tx, &schema, &existing.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "superseded PostgreSQL session memory record was not readable",
                )
            })?;
        let new_record =
            get_session_memory_record_in_tx(&mut tx, &schema, &supersede.replacement.record_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "replacement PostgreSQL session memory record was not readable",
                    )
                })?;
        tx.commit().map_err(|error| {
            postgres_error("commit supersede PostgreSQL session memory record", error)
        })?;
        Ok((old_record, new_record))
    }

    pub fn archive_session_memory_record(
        &self,
        archive: &SessionMemoryArchive,
    ) -> CoreResult<SessionMemoryRecord> {
        crate::validate_session_memory_record_id(&archive.record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start archive PostgreSQL session memory record", error)
        })?;
        let existing = active_session_memory_record_for_update(
            &mut tx,
            &schema,
            &archive.record_id,
            archive.expected_revision,
        )?;
        archive_session_memory_record_in_tx(&mut tx, &schema, archive, existing.revision + 1)?;
        let record = get_session_memory_record_in_tx(&mut tx, &schema, &archive.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "archived PostgreSQL session memory record was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit archive PostgreSQL session memory record", error)
        })?;
        Ok(record)
    }

    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_session_memory_records(&mut *client, &schema, query)
    }

    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        select_branch_aware_session_memory(&mut *client, &schema, query)
    }

    pub fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        crate::validate_memory_proposal(proposal, descriptor)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL memory proposal", error))?;
        if let Some(dedupe_key) = proposal
            .dedupe_key
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            if let Some(existing) =
                get_memory_proposal_by_dedupe(&mut tx, &schema, &proposal.space_id.0, dedupe_key)?
            {
                tx.commit().map_err(|error| {
                    postgres_error("commit PostgreSQL duplicate memory proposal", error)
                })?;
                return Ok(existing);
            }
        }
        if get_memory_proposal_by_id(&mut tx, &schema, &proposal.proposal_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("memory proposal {} already exists", proposal.proposal_id),
            ));
        }
        let record = MemoryProposalRecord {
            proposal: proposal.clone(),
            status: MemoryProposalReviewStatus::PendingReview,
            selected_governance_mode: selected_governance_mode(
                proposal.governance_mode,
                proposal.source,
            ),
            created_at: now.clone(),
            updated_at: now.clone(),
            decided_at: None,
            applied_at: None,
            resulting_revision: None,
            duplicate_of: None,
        };
        insert_memory_proposal_record_in_tx(&mut tx, &schema, &record)?;
        insert_memory_governance_decision_in_tx(
            &mut tx,
            &schema,
            &MemoryGovernanceDecisionRecord {
                decision_id: format!("{}_routed", proposal.proposal_id),
                proposal_id: proposal.proposal_id.clone(),
                decision: MemoryGovernanceDecisionKind::RoutedToReview,
                actor: "rusty_crew_governance".to_string(),
                source: proposal.source,
                evidence_refs: proposal.evidence_refs.clone(),
                policy_mode: record.selected_governance_mode,
                confidence: Some(proposal.confidence),
                message: Some("typed memory proposals start in curator/manual review".to_string()),
                resulting_revision: None,
                decided_at: now.clone(),
            },
        )?;
        let saved = get_memory_proposal_by_id(&mut tx, &schema, &proposal.proposal_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "saved PostgreSQL memory proposal was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL memory proposal", error))?;
        Ok(saved)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_memory_proposals(&mut *client, &schema, query)
    }

    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        digest.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        insert_or_replace_session_activity_digest(&mut *client, &schema, digest)?;
        get_session_activity_digest_by_id(&mut *client, &schema, &digest.digest_id)?.ok_or_else(
            || {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "saved PostgreSQL session activity digest was not readable",
                )
            },
        )
    }

    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_session_activity_digests(&mut *client, &schema, query)
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        artifact.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        insert_or_replace_context_compaction_artifact(&mut *client, &schema, artifact)?;
        get_context_compaction_artifact_by_id(&mut *client, &schema, &artifact.artifact_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "saved PostgreSQL context compaction artifact was not readable",
                )
            })
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_context_compaction_artifacts(&mut *client, &schema, query)
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        validate_postgres_memory_governance_decision(decision)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL memory governance decision", error)
        })?;
        let mut proposal = get_memory_proposal_by_id(&mut tx, &schema, &decision.proposal_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("memory proposal {} not found", decision.proposal_id),
                )
            })?;
        validate_postgres_memory_governance_transition(proposal.status, decision.decision)?;
        let resulting_revision = if decision.decision == MemoryGovernanceDecisionKind::Applied
            && proposal.proposal.space_id.as_str() == "session_memory"
        {
            Some(apply_session_memory_proposal_in_tx(
                &mut tx,
                &schema,
                &proposal.proposal,
                now,
            )?)
        } else {
            decision.resulting_revision
        };
        let decided_at = decision.decided_at.clone().unwrap_or_else(|| now.clone());
        let record = MemoryGovernanceDecisionRecord {
            decision_id: decision.decision_id.clone(),
            proposal_id: decision.proposal_id.clone(),
            decision: decision.decision,
            actor: decision.actor.clone(),
            source: decision.source,
            evidence_refs: decision.evidence_refs.clone(),
            policy_mode: decision.policy_mode,
            confidence: decision.confidence,
            message: decision.message.clone(),
            resulting_revision,
            decided_at,
        };
        insert_memory_governance_decision_in_tx(&mut tx, &schema, &record)?;
        update_memory_proposal_review_state(&mut proposal, &record);
        update_memory_proposal_record_in_tx(&mut tx, &schema, &proposal)?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL memory governance decision", error)
        })?;
        Ok(record)
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start add PostgreSQL roleplay lore", error))?;
        if get_roleplay_lore_record(&mut tx, &schema, &write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("roleplay lore record {} already exists", write.record_id),
            ));
        }
        insert_roleplay_lore_record(&mut tx, &schema, write)?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!("{}:created", write.record_id),
                record_id: write.record_id.clone(),
                world_id: write.world_id.clone(),
                evidence_refs: write.evidence_refs.clone(),
                source: write.source,
                actor: crate::memory_proposal_source_as_str(write.source).to_string(),
                note: Some("created roleplay lore record".to_string()),
                created_at: write.now.clone(),
            },
        )?;
        let record =
            get_roleplay_lore_record(&mut tx, &schema, &write.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "created PostgreSQL roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit add PostgreSQL roleplay lore", error))?;
        Ok(record)
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_write(&replace.write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start replace PostgreSQL roleplay lore", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &mut tx,
            &schema,
            &replace.write.record_id,
            replace.expected_revision,
        )?;
        update_roleplay_lore_record(&mut tx, &schema, replace, existing.revision + 1)?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:revision:{}",
                    replace.write.record_id,
                    existing.revision + 1
                ),
                record_id: replace.write.record_id.clone(),
                world_id: replace.write.world_id.clone(),
                evidence_refs: replace.write.evidence_refs.clone(),
                source: replace.write.source,
                actor: crate::memory_proposal_source_as_str(replace.write.source).to_string(),
                note: Some("replaced roleplay lore record".to_string()),
                created_at: replace.write.now.clone(),
            },
        )?;
        let record = get_roleplay_lore_record(&mut tx, &schema, &replace.write.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replaced PostgreSQL roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit replace PostgreSQL roleplay lore", error))?;
        Ok(record)
    }

    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        validate_roleplay_lore_write(&supersede.replacement)?;
        if supersede.replacement.supersedes_record_id.as_deref()
            != Some(supersede.record_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore replacement must reference the superseded record",
            ));
        }
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start supersede PostgreSQL roleplay lore", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &mut tx,
            &schema,
            &supersede.record_id,
            supersede.expected_revision,
        )?;
        if existing.world_id != supersede.replacement.world_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore replacement must stay in the same world",
            ));
        }
        if get_roleplay_lore_record(&mut tx, &schema, &supersede.replacement.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore replacement {} already exists",
                    supersede.replacement.record_id
                ),
            ));
        }
        insert_roleplay_lore_record(&mut tx, &schema, &supersede.replacement)?;
        mark_roleplay_lore_superseded(
            &mut tx,
            &schema,
            &existing.record_id,
            &supersede.replacement.record_id,
            existing.revision + 1,
            &supersede.replacement.now,
        )?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:superseded_by:{}",
                    existing.record_id, supersede.replacement.record_id
                ),
                record_id: existing.record_id.clone(),
                world_id: existing.world_id.clone(),
                evidence_refs: supersede.replacement.evidence_refs.clone(),
                source: supersede.replacement.source,
                actor: crate::memory_proposal_source_as_str(supersede.replacement.source)
                    .to_string(),
                note: Some(format!("superseded by {}", supersede.replacement.record_id)),
                created_at: supersede.replacement.now.clone(),
            },
        )?;
        let old_record = get_roleplay_lore_record(&mut tx, &schema, &existing.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "superseded PostgreSQL roleplay lore record was not readable",
                )
            })?;
        let new_record =
            get_roleplay_lore_record(&mut tx, &schema, &supersede.replacement.record_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "replacement PostgreSQL roleplay lore record was not readable",
                    )
                })?;
        tx.commit()
            .map_err(|error| postgres_error("commit supersede PostgreSQL roleplay lore", error))?;
        Ok((old_record, new_record))
    }

    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_record_id(&tombstone.record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start tombstone PostgreSQL roleplay lore", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &mut tx,
            &schema,
            &tombstone.record_id,
            tombstone.expected_revision,
        )?;
        tombstone_roleplay_lore_record(&mut tx, &schema, tombstone, existing.revision + 1)?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:tombstoned:{}",
                    tombstone.record_id,
                    existing.revision + 1
                ),
                record_id: tombstone.record_id.clone(),
                world_id: existing.world_id,
                evidence_refs: existing.evidence_refs,
                source: existing.source,
                actor: "rusty_crew_storage".to_string(),
                note: tombstone.reason.clone(),
                created_at: tombstone.now.clone(),
            },
        )?;
        let record =
            get_roleplay_lore_record(&mut tx, &schema, &tombstone.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "tombstoned PostgreSQL roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit tombstone PostgreSQL roleplay lore", error))?;
        Ok(record)
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_roleplay_lore_records(&mut *client, &schema, query)
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        validate_roleplay_lore_record_id(record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_roleplay_lore_record(&mut *client, &schema, record_id)
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        validate_roleplay_lore_record_id(record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        roleplay_lore_provenance_events(&mut *client, &schema, record_id)
    }

    pub fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        validate_roleplay_lore_layer_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start create PostgreSQL roleplay lore layer", error)
        })?;
        if get_lore_layer(&mut tx, &schema, &write.layer_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("roleplay lore layer {} already exists", write.layer_id),
            ));
        }
        tx.execute(
            &format!(
                "INSERT INTO {schema}.module_roleplay_lore_layers (
                    layer_id,
                    profile_id,
                    name,
                    description,
                    purpose,
                    write_policy,
                    is_archived,
                    created_at,
                    updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7, $7)"
            ),
            &[
                &write.layer_id,
                &write.profile_id,
                &write.name,
                &normalized_optional_text(write.description.as_deref()),
                &roleplay_lore_layer_purpose_as_str(write.purpose),
                &roleplay_lore_layer_write_policy_as_str(write.write_policy),
                &write.now,
            ],
        )
        .map_err(|error| postgres_error("insert PostgreSQL roleplay lore layer", error))?;
        let layer = get_lore_layer(&mut tx, &schema, &write.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "created PostgreSQL roleplay lore layer was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit create PostgreSQL roleplay lore layer", error)
        })?;
        Ok(layer)
    }

    pub fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_lore_layer(&mut *client, &schema, layer_id)
    }

    pub fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        validate_roleplay_lore_identifier("roleplay lore profile_id", profile_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_lore_layers_by_profile(&mut *client, &schema, profile_id)
    }

    pub fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        validate_roleplay_lore_layer_update(update)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start update PostgreSQL roleplay lore layer", error)
        })?;
        let mut existing =
            get_lore_layer(&mut tx, &schema, &update.layer_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("roleplay lore layer {} not found", update.layer_id),
                )
            })?;
        if let Some(name) = &update.name {
            existing.name = name.trim().to_string();
        }
        if let Some(description) = &update.description {
            existing.description = normalized_optional_text(description.as_deref());
        }
        if let Some(purpose) = update.purpose {
            existing.purpose = purpose;
        }
        if let Some(write_policy) = update.write_policy {
            existing.write_policy = write_policy;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.module_roleplay_lore_layers
                 SET name = $2,
                     description = $3,
                     purpose = $4,
                     write_policy = $5,
                     updated_at = $6
                 WHERE layer_id = $1"
            ),
            &[
                &update.layer_id,
                &existing.name,
                &existing.description,
                &roleplay_lore_layer_purpose_as_str(existing.purpose),
                &roleplay_lore_layer_write_policy_as_str(existing.write_policy),
                &update.now,
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL roleplay lore layer", error))?;
        let layer = get_lore_layer(&mut tx, &schema, &update.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "updated PostgreSQL roleplay lore layer was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit update PostgreSQL roleplay lore layer", error)
        })?;
        Ok(layer)
    }

    pub fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", &archive.layer_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start archive PostgreSQL roleplay lore layer", error)
        })?;
        if get_lore_layer(&mut tx, &schema, &archive.layer_id)?.is_none() {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", archive.layer_id),
            ));
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.module_roleplay_lore_layers
                 SET is_archived = TRUE,
                     updated_at = $2
                 WHERE layer_id = $1"
            ),
            &[&archive.layer_id, &archive.now],
        )
        .map_err(|error| postgres_error("archive PostgreSQL roleplay lore layer", error))?;
        let layer = get_lore_layer(&mut tx, &schema, &archive.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "archived PostgreSQL roleplay lore layer was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit archive PostgreSQL roleplay lore layer", error)
        })?;
        Ok(layer)
    }

    pub fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_lore_layer_config(&mut *client, &schema, layer_id)
    }

    pub fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        validate_roleplay_lore_layer_config_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start set PostgreSQL roleplay lore layer config", error)
        })?;
        if get_lore_layer(&mut tx, &schema, &write.layer_id)?.is_none() {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", write.layer_id),
            ));
        }
        let existing = get_lore_layer_config(&mut tx, &schema, &write.layer_id)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at.as_str())
            .unwrap_or_else(|| write.now.as_str());
        tx.execute(
            &format!(
                "INSERT INTO {schema}.module_roleplay_lore_layer_config (
                    config_id,
                    layer_id,
                    fts_weight,
                    subject_weight,
                    canon_weight,
                    tag_boost_weight,
                    recency_weight,
                    default_token_budget,
                    constant_token_reserve,
                    min_relevance_score,
                    max_constants,
                    created_at,
                    updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(layer_id) DO UPDATE SET
                    config_id = excluded.config_id,
                    fts_weight = excluded.fts_weight,
                    subject_weight = excluded.subject_weight,
                    canon_weight = excluded.canon_weight,
                    tag_boost_weight = excluded.tag_boost_weight,
                    recency_weight = excluded.recency_weight,
                    default_token_budget = excluded.default_token_budget,
                    constant_token_reserve = excluded.constant_token_reserve,
                    min_relevance_score = excluded.min_relevance_score,
                    max_constants = excluded.max_constants,
                    updated_at = excluded.updated_at"
            ),
            &[
                &write.config_id,
                &write.layer_id,
                &(write.fts_weight as f64),
                &(write.subject_weight as f64),
                &(write.canon_weight as f64),
                &(write.tag_boost_weight as f64),
                &(write.recency_weight as f64),
                &(write.default_token_budget as i64),
                &(write.constant_token_reserve as i64),
                &(write.min_relevance_score as f64),
                &(write.max_constants as i64),
                &created_at,
                &write.now,
            ],
        )
        .map_err(|error| postgres_error("upsert PostgreSQL roleplay lore layer config", error))?;
        let config =
            get_lore_layer_config(&mut tx, &schema, &write.layer_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "saved PostgreSQL roleplay lore layer config was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit set PostgreSQL roleplay lore layer config", error)
        })?;
        Ok(config)
    }

    pub fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        validate_roleplay_lore_layer_entry_link(link)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start add PostgreSQL roleplay lore entry to layer", error)
        })?;
        require_lore_layer_and_record(&mut tx, &schema, &link.layer_id, &link.record_id)?;
        insert_lore_layer_entry(&mut tx, &schema, link)?;
        tx.commit().map_err(|error| {
            postgres_error("commit add PostgreSQL roleplay lore entry to layer", error)
        })
    }

    pub fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        validate_roleplay_lore_fact_capture(capture)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start capture PostgreSQL roleplay lore fact", error)
        })?;
        let layer = get_lore_layer(&mut tx, &schema, &capture.layer_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer {} not found", capture.layer_id),
            )
        })?;
        if layer.is_archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("roleplay lore layer {} is archived", capture.layer_id),
            ));
        }
        if layer.write_policy != RoleplayLoreLayerWritePolicy::AutoCapture {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore layer {} does not allow auto capture",
                    capture.layer_id
                ),
            ));
        }
        if get_roleplay_lore_record(&mut tx, &schema, &capture.write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore record {} already exists",
                    capture.write.record_id
                ),
            ));
        }
        insert_roleplay_lore_record(&mut tx, &schema, &capture.write)?;
        insert_lore_layer_entry(
            &mut tx,
            &schema,
            &RoleplayLoreLayerEntryLink {
                layer_id: capture.layer_id.clone(),
                record_id: capture.write.record_id.clone(),
                is_constant: capture.is_constant,
                priority: capture.priority,
                added_at: capture.write.now.clone(),
            },
        )?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!("{}:captured:{}", capture.write.record_id, capture.layer_id),
                record_id: capture.write.record_id.clone(),
                world_id: capture.write.world_id.clone(),
                evidence_refs: capture.write.evidence_refs.clone(),
                source: capture.write.source,
                actor: memory_proposal_source_as_str(capture.write.source).to_string(),
                note: capture
                    .capture_reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| Some("captured roleplay lore fact".to_string())),
                created_at: capture.write.now.clone(),
            },
        )?;
        let entry = get_lore_layer_entry_join(
            &mut tx,
            &schema,
            &capture.layer_id,
            &capture.write.record_id,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "captured PostgreSQL roleplay lore layer entry was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit capture PostgreSQL roleplay lore fact", error)
        })?;
        Ok(entry)
    }

    pub fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        validate_roleplay_lore_entry_promotion(promotion)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start promote PostgreSQL roleplay lore entry", error)
        })?;
        let source = get_lore_layer_entry_join(
            &mut tx,
            &schema,
            &promotion.source_layer_id,
            &promotion.source_record_id,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "roleplay lore source entry {}/{} not found",
                    promotion.source_layer_id, promotion.source_record_id
                ),
            )
        })?;
        if source.record.status != RoleplayLoreRecordStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore source record {} is not active",
                    promotion.source_record_id
                ),
            ));
        }
        let target =
            get_lore_layer(&mut tx, &schema, &promotion.target_layer_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "roleplay lore target layer {} not found",
                        promotion.target_layer_id
                    ),
                )
            })?;
        if target.is_archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore target layer {} is archived",
                    promotion.target_layer_id
                ),
            ));
        }
        if target.write_policy == RoleplayLoreLayerWritePolicy::Readonly {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay lore target layer {} is readonly",
                    promotion.target_layer_id
                ),
            ));
        }
        if get_roleplay_lore_record(&mut tx, &schema, &promotion.new_record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore promoted record {} already exists",
                    promotion.new_record_id
                ),
            ));
        }
        let promoted = RoleplayLoreWrite {
            record_id: promotion.new_record_id.clone(),
            world_id: source.record.world_id.clone(),
            entity_id: source.record.entity_id.clone(),
            session_id: source.record.session_id.clone(),
            branch_id: source.record.branch_id.clone(),
            shape: source.record.shape.clone(),
            canon_status: source.record.canon_status,
            visibility: source.record.visibility,
            title: source.record.title.clone(),
            body: source.record.body.clone(),
            content: source.record.content.clone(),
            evidence_refs: source.record.evidence_refs.clone(),
            source: source.record.source,
            confidence: source.record.confidence,
            durability_rationale: source.record.durability_rationale.clone(),
            supersedes_record_id: Some(promotion.source_record_id.clone()),
            now: promotion.now.clone(),
        };
        insert_roleplay_lore_record(&mut tx, &schema, &promoted)?;
        mark_roleplay_lore_superseded(
            &mut tx,
            &schema,
            &source.record.record_id,
            &promotion.new_record_id,
            source.record.revision + 1,
            &promotion.now,
        )?;
        insert_lore_layer_entry(
            &mut tx,
            &schema,
            &RoleplayLoreLayerEntryLink {
                layer_id: promotion.target_layer_id.clone(),
                record_id: promotion.new_record_id.clone(),
                is_constant: promotion.is_constant,
                priority: promotion.priority,
                added_at: promotion.now.clone(),
            },
        )?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:promoted_from:{}:{}",
                    promotion.new_record_id, promotion.source_layer_id, promotion.source_record_id
                ),
                record_id: promotion.new_record_id.clone(),
                world_id: source.record.world_id.clone(),
                evidence_refs: source.record.evidence_refs.clone(),
                source: source.record.source,
                actor: "rusty_crew_storage".to_string(),
                note: Some(format!(
                    "promoted from {}:{}",
                    promotion.source_layer_id, promotion.source_record_id
                )),
                created_at: promotion.now.clone(),
            },
        )?;
        let entry = get_lore_layer_entry_join(
            &mut tx,
            &schema,
            &promotion.target_layer_id,
            &promotion.new_record_id,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "promoted PostgreSQL roleplay lore layer entry was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error("commit promote PostgreSQL roleplay lore entry", error)
        })?;
        Ok(entry)
    }

    pub fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        validate_roleplay_lore_record_id(record_id)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_roleplay_lore_layer_entries
                     WHERE layer_id = $1 AND record_id = $2"
                ),
                &[&layer_id, &record_id],
            )
            .map_err(|error| {
                postgres_error("remove PostgreSQL roleplay lore entry from layer", error)
            })?;
        Ok(())
    }

    pub fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        validate_roleplay_lore_record_id(record_id)?;
        let schema = self.quoted_schema();
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.module_roleplay_lore_layer_entries
                     SET is_constant = $3
                     WHERE layer_id = $1 AND record_id = $2"
                ),
                &[&layer_id, &record_id, &is_constant],
            )
            .map_err(|error| {
                postgres_error("set PostgreSQL roleplay lore entry constant", error)
            })?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay lore layer entry {layer_id}/{record_id} not found"),
            ));
        }
        Ok(())
    }

    pub fn list_entries_by_layer(
        &self,
        layer_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_entries_by_layer(&mut *client, &schema, layer_id)
    }

    pub fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        validate_roleplay_chat_layers_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start set PostgreSQL roleplay chat layers", error))?;
        Self::set_chat_layers_in_tx(&mut tx, &schema, write)?;
        tx.commit()
            .map_err(|error| postgres_error("commit set PostgreSQL roleplay chat layers", error))
    }

    pub(super) fn set_chat_layers_in_tx(
        tx: &mut Transaction<'_>,
        schema: &str,
        write: &RoleplayChatLayersWrite,
    ) -> CoreResult<()> {
        tx.execute(
            &format!("DELETE FROM {schema}.module_roleplay_chat_layers WHERE chat_id = $1"),
            &[&write.chat_id],
        )
        .map_err(|error| postgres_error("clear PostgreSQL roleplay chat layers", error))?;
        for layer in &write.layers {
            if get_lore_layer(tx, schema, &layer.layer_id)?.is_none() {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("roleplay lore layer {} not found", layer.layer_id),
                ));
            }
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.module_roleplay_chat_layers (
                        chat_id,
                        layer_id,
                        priority,
                        enabled,
                        created_at
                     ) VALUES ($1, $2, $3, $4, $5)"
                ),
                &[
                    &write.chat_id,
                    &layer.layer_id,
                    &layer.priority,
                    &layer.enabled,
                    &write.now,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL roleplay chat layer", error))?;
        }
        Ok(())
    }

    pub fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_chat_layers(&mut *client, &schema, chat_id)
    }

    pub fn toggle_chat_layer(
        &self,
        chat_id: &str,
        layer_id: &str,
        enabled: bool,
    ) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
        validate_roleplay_lore_identifier("roleplay lore layer_id", layer_id)?;
        let schema = self.quoted_schema();
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.module_roleplay_chat_layers
                     SET enabled = $3
                     WHERE chat_id = $1 AND layer_id = $2"
                ),
                &[&chat_id, &layer_id, &enabled],
            )
            .map_err(|error| postgres_error("toggle PostgreSQL roleplay chat layer", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("roleplay chat layer {chat_id}/{layer_id} not found"),
            ));
        }
        Ok(())
    }

    pub fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        validate_roleplay_lore_identifier("roleplay chat_id", chat_id)?;
        crate::validate_unique_roleplay_ids("roleplay chat layer_ids", layer_ids)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start reorder PostgreSQL roleplay chat layers", error)
        })?;
        let existing = get_chat_layers(&mut tx, &schema, chat_id)?;
        if existing.len() != layer_ids.len() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay chat layer reorder must include exactly the existing layers",
            ));
        }
        let existing_ids = existing
            .iter()
            .map(|record| record.layer_id.as_str())
            .collect::<BTreeSet<_>>();
        for layer_id in layer_ids {
            if !existing_ids.contains(layer_id.as_str()) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!("roleplay chat layer {layer_id} is not attached to chat {chat_id}"),
                ));
            }
        }
        for (priority, layer_id) in layer_ids.iter().enumerate() {
            tx.execute(
                &format!(
                    "UPDATE {schema}.module_roleplay_chat_layers
                     SET priority = $3
                     WHERE chat_id = $1 AND layer_id = $2"
                ),
                &[&chat_id, &layer_id, &(priority as i64)],
            )
            .map_err(|error| postgres_error("reorder PostgreSQL roleplay chat layer", error))?;
        }
        tx.commit().map_err(|error| {
            postgres_error("commit reorder PostgreSQL roleplay chat layers", error)
        })
    }

    pub fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        validate_lore_recall_query(query)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL roleplay lore recall", error))?;
        let layers = get_chat_layers(&mut tx, &schema, &query.chat_id)?
            .into_iter()
            .filter(|layer| layer.enabled && !layer.layer.is_archived)
            .collect::<Vec<_>>();
        let mut layer_configs = Vec::new();
        for layer in &layers {
            let config = get_lore_layer_config(&mut tx, &schema, &layer.layer_id)?
                .unwrap_or_else(|| default_lore_layer_config(&layer.layer_id, &query.now));
            layer_configs.push((layer.clone(), config));
        }

        let token_budget = query.token_budget.unwrap_or_else(|| {
            layer_configs
                .first()
                .map(|(_, config)| config.default_token_budget)
                .unwrap_or(4_000)
        });
        let mut remaining = token_budget;
        let mut entries = Vec::new();
        let mut seen_records = BTreeSet::new();
        let mut entries_considered = 0_u32;

        for (layer, config) in &layer_configs {
            let constants =
                constant_lore_entries_for_layer(&mut tx, &schema, &layer.layer_id, config)?;
            let mut reserve_remaining = config.constant_token_reserve;
            for mut entry in constants {
                entries_considered += 1;
                if excluded_subject_match(&entry.record, &query.excluded_subjects) {
                    continue;
                }
                entry.token_estimate = estimate_lore_tokens(&entry.record);
                if entry.token_estimate > remaining || entry.token_estimate > reserve_remaining {
                    continue;
                }
                remaining -= entry.token_estimate;
                reserve_remaining -= entry.token_estimate;
                seen_records.insert(entry.record.record_id.clone());
                entries.push(entry);
            }
        }

        let mut scored =
            scored_lore_entries_for_recall(&mut tx, &schema, query, &layer_configs, &seen_records)?;
        entries_considered += scored.len() as u32;
        scored.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.record.updated_at.cmp(&right.record.updated_at))
                .then_with(|| left.record.record_id.cmp(&right.record.record_id))
        });
        for entry in scored {
            if entry.token_estimate > remaining {
                continue;
            }
            remaining -= entry.token_estimate;
            entries.push(entry);
        }

        let tokens_consumed = token_budget.saturating_sub(remaining);
        let trace = if query.record_trace {
            let trace = LoreRecallTraceRecord {
                trace_id: query.trace_id.clone().unwrap_or_else(|| {
                    format!("recall:{}:{}:{}", query.chat_id, query.now, entries.len())
                }),
                session_id: query.session_id.clone(),
                layer_ids: layers.iter().map(|layer| layer.layer_id.clone()).collect(),
                query_text: query.query_text.clone(),
                active_subjects: query.active_subjects.clone(),
                excluded_subjects: query.excluded_subjects.clone(),
                config_snapshot: lore_recall_config_snapshot(&layer_configs),
                entries_considered,
                entries_returned: entries.len() as u32,
                token_budget: Some(token_budget),
                tokens_consumed,
                created_at: query.now.clone(),
            };
            insert_lore_recall_trace(&mut tx, &schema, &trace)?;
            Some(trace)
        } else {
            None
        };
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL roleplay lore recall", error))?;
        Ok(LoreRecallResult {
            chat_id: query.chat_id.clone(),
            entries,
            entries_considered,
            tokens_consumed,
            token_budget: Some(token_budget),
            trace,
        })
    }

    pub fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        validate_lore_recall_trace_query(query)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_lore_recall_traces(&mut *client, &schema, query)
    }

    pub fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        validate_roleplay_lore_identifier("roleplay lore recall trace_id", trace_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_lore_recall_trace(&mut *client, &schema, trace_id)
    }
}
