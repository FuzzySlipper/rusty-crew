use super::super::*;

impl CoordinationStore {
    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        let conn = self.conn()?;
        query_profile_memory(&conn, query)
    }
    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        validate_profile_memory_key(key, ProfileMemoryCaps::default().max_key_bytes)?;
        let conn = self.conn()?;
        get_profile_memory(&conn, profile_id, target, key)
    }
    pub fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(write, caps)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start add profile memory", error))?;
        let count = count_profile_memory_for_profile(&tx, &write.profile_id)?;
        if count >= caps.max_records_per_profile as u64 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile {} already has the maximum {} dense memory records",
                    write.profile_id, caps.max_records_per_profile
                ),
            ));
        }
        if get_profile_memory(&tx, &write.profile_id, &write.target, &write.key)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile memory {} for profile {} already exists",
                    write.key, write.profile_id
                ),
            ));
        }
        let record = insert_profile_memory_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit add profile memory", error))?;
        Ok(record)
    }
    pub fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(&replace.write, caps)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start replace profile memory", error))?;
        let existing = get_profile_memory(
            &tx,
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
        let record = update_profile_memory_in_tx(&tx, &replace.write, existing.revision + 1)?;
        tx.commit()
            .map_err(|error| persistence_error("commit replace profile memory", error))?;
        Ok(record)
    }
    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_key(&delete.key, ProfileMemoryCaps::default().max_key_bytes)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start remove profile memory", error))?;
        let existing = get_profile_memory(&tx, &delete.profile_id, &delete.target, &delete.key)?
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
            "DELETE FROM profile_memories
             WHERE profile_id = ?1
               AND target_type = ?2
               AND target_id = ?3
               AND memory_key = ?4",
            params![
                delete.profile_id.0.as_str(),
                target_type,
                target_id.as_str(),
                delete.key.as_str(),
            ],
        )
        .map_err(|error| persistence_error("remove profile memory", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove profile memory", error))?;
        Ok(existing)
    }
    pub fn add_session_memory_record(
        &self,
        write: &SessionMemoryRecordWrite,
    ) -> CoreResult<SessionMemoryRecord> {
        validate_session_memory_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start add session memory record", error))?;
        validate_session_memory_scope_in_tx(
            &tx,
            &write.session_id,
            &write.scope,
            &write.branch_id,
        )?;
        if get_session_memory_record_in_tx(&tx, &write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("session memory record {} already exists", write.record_id),
            ));
        }
        insert_session_memory_record_in_tx(&tx, write)?;
        let record = get_session_memory_record_in_tx(&tx, &write.record_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "created session memory record was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit add session memory record", error))?;
        Ok(record)
    }
    pub fn replace_session_memory_record(
        &self,
        replace: &SessionMemoryReplace,
    ) -> CoreResult<SessionMemoryRecord> {
        validate_session_memory_revision_input(
            &replace.record_id,
            replace.expected_revision,
            &replace.evidence_refs,
            replace.confidence,
            &replace.durability_rationale,
        )?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start replace session memory record", error))?;
        let existing = active_session_memory_record_for_update(
            &tx,
            &replace.record_id,
            replace.expected_revision,
        )?;
        validate_session_memory_content(&existing.shape, &replace.content)?;
        update_session_memory_record_content_in_tx(&tx, replace, existing.revision + 1)?;
        let record =
            get_session_memory_record_in_tx(&tx, &replace.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replaced session memory record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit replace session memory record", error))?;
        Ok(record)
    }
    pub fn supersede_session_memory_record(
        &self,
        supersede: &SessionMemorySupersede,
    ) -> CoreResult<(SessionMemoryRecord, SessionMemoryRecord)> {
        validate_session_memory_write(&supersede.replacement)?;
        if supersede.replacement.supersedes_record_id.as_deref()
            != Some(supersede.record_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory replacement must reference the superseded record",
            ));
        }
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start supersede session memory record", error))?;
        let existing = active_session_memory_record_for_update(
            &tx,
            &supersede.record_id,
            supersede.expected_revision,
        )?;
        validate_session_memory_scope_in_tx(
            &tx,
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
        if get_session_memory_record_in_tx(&tx, &supersede.replacement.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "session memory replacement {} already exists",
                    supersede.replacement.record_id
                ),
            ));
        }
        insert_session_memory_record_in_tx(&tx, &supersede.replacement)?;
        mark_session_memory_superseded_in_tx(
            &tx,
            &existing.record_id,
            &supersede.replacement.record_id,
            existing.revision + 1,
            &supersede.replacement.now,
        )?;
        let old_record =
            get_session_memory_record_in_tx(&tx, &existing.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "superseded session memory record was not readable",
                )
            })?;
        let new_record = get_session_memory_record_in_tx(&tx, &supersede.replacement.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replacement session memory record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit supersede session memory record", error))?;
        Ok((old_record, new_record))
    }
    pub fn archive_session_memory_record(
        &self,
        archive: &SessionMemoryArchive,
    ) -> CoreResult<SessionMemoryRecord> {
        validate_session_memory_record_id(&archive.record_id)?;
        if archive.expected_revision == 0 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory expected_revision must be greater than zero",
            ));
        }
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start archive session memory record", error))?;
        let existing = active_session_memory_record_for_update(
            &tx,
            &archive.record_id,
            archive.expected_revision,
        )?;
        archive_session_memory_record_in_tx(&tx, archive, existing.revision + 1)?;
        let record =
            get_session_memory_record_in_tx(&tx, &archive.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "archived session memory record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| persistence_error("commit archive session memory record", error))?;
        Ok(record)
    }
    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        let conn = self.conn()?;
        query_session_memory_records(&conn, query)
    }
    pub fn query_branch_aware_session_memory_records(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        let conn = self.conn()?;
        Ok(select_branch_aware_session_memory(&conn, query)?.records)
    }
    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        let conn = self.conn()?;
        select_branch_aware_session_memory(&conn, query)
    }
    pub fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        validate_memory_proposal(proposal, descriptor)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save memory proposal", error))?;
        if let Some(dedupe_key) = proposal
            .dedupe_key
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            if let Some(existing) =
                get_memory_proposal_by_dedupe(&tx, &proposal.space_id.0, dedupe_key)?
            {
                return Ok(existing);
            }
        }
        if get_memory_proposal_by_id(&tx, &proposal.proposal_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("memory proposal {} already exists", proposal.proposal_id),
            ));
        }
        insert_memory_proposal_in_tx(&tx, proposal, now)?;
        insert_memory_governance_decision_in_tx(
            &tx,
            &MemoryGovernanceDecisionInput {
                decision_id: format!("{}_routed", proposal.proposal_id),
                proposal_id: proposal.proposal_id.clone(),
                decision: MemoryGovernanceDecisionKind::RoutedToReview,
                actor: "rusty_crew_governance".to_string(),
                source: proposal.source,
                evidence_refs: proposal.evidence_refs.clone(),
                policy_mode: selected_governance_mode(proposal.governance_mode, proposal.source),
                confidence: Some(proposal.confidence),
                message: Some("typed memory proposals start in curator/manual review".to_string()),
                resulting_revision: None,
                decided_at: Some(now.clone()),
            },
        )?;
        let record = get_memory_proposal_by_id(&tx, &proposal.proposal_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "saved memory proposal was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit save memory proposal", error))?;
        Ok(record)
    }
    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        let conn = self.conn()?;
        list_memory_proposals(&conn, query)
    }
    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        digest.validate()?;
        let conn = self.conn()?;
        insert_or_replace_session_activity_digest(&conn, digest)?;
        get_session_activity_digest_by_id(&conn, &digest.digest_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "saved session activity digest was not readable",
            )
        })
    }
    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        let conn = self.conn()?;
        list_session_activity_digests(&conn, query)
    }
    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        validate_memory_governance_decision(decision)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start memory governance decision", error))?;
        let proposal = get_memory_proposal_by_id(&tx, &decision.proposal_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("memory proposal {} not found", decision.proposal_id),
            )
        })?;
        validate_memory_governance_transition(proposal.status, decision.decision)?;
        let resulting_revision = if decision.decision == MemoryGovernanceDecisionKind::Applied
            && proposal.proposal.space_id.as_str() == "session_memory"
        {
            Some(apply_session_memory_proposal_in_tx(
                &tx,
                &proposal.proposal,
                now,
            )?)
        } else {
            decision.resulting_revision
        };
        let mut stored = decision.clone();
        if stored.decided_at.is_none() {
            stored.decided_at = Some(now.clone());
        }
        stored.resulting_revision = resulting_revision;
        let record = insert_memory_governance_decision_in_tx(&tx, &stored)?;
        update_memory_proposal_review_state_in_tx(&tx, &record)?;
        tx.commit()
            .map_err(|error| persistence_error("commit memory governance decision", error))?;
        Ok(record)
    }
}

fn query_profile_memory(
    conn: &Connection,
    query: &ProfileMemoryQuery,
) -> CoreResult<Vec<ProfileMemoryRecord>> {
    let target_parts = query
        .target
        .as_ref()
        .map(|target| profile_memory_target_parts(&query.profile_id, target));
    let target_type = target_parts.as_ref().map(|(target_type, _)| *target_type);
    let target_id = target_parts
        .as_ref()
        .map(|(_, target_id)| target_id.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                profile_id,
                target_type,
                target_id,
                memory_key,
                content,
                metadata_json,
                revision,
                created_at,
                updated_at
             FROM profile_memories
             WHERE profile_id = ?1
               AND (?2 IS NULL OR target_type = ?2)
               AND (?3 IS NULL OR target_id = ?3)
             ORDER BY updated_at DESC, memory_key ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query profile memory", error))?;
    let rows = stmt
        .query_map(
            params![
                query.profile_id.0.as_str(),
                target_type,
                target_id,
                limit,
                offset
            ],
            row_to_profile_memory,
        )
        .map_err(|error| persistence_error("query profile memory", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load profile memory", error))
}

fn get_profile_memory(
    conn: &Connection,
    profile_id: &ProfileId,
    target: &ProfileMemoryTarget,
    key: &str,
) -> CoreResult<Option<ProfileMemoryRecord>> {
    let (target_type, target_id) = profile_memory_target_parts(profile_id, target);
    conn.query_row(
        "SELECT
            profile_id,
            target_type,
            target_id,
            memory_key,
            content,
            metadata_json,
            revision,
            created_at,
            updated_at
         FROM profile_memories
         WHERE profile_id = ?1
           AND target_type = ?2
           AND target_id = ?3
           AND memory_key = ?4",
        params![profile_id.0.as_str(), target_type, target_id.as_str(), key,],
        row_to_profile_memory,
    )
    .optional()
    .map_err(|error| persistence_error("get profile memory", error))
}

fn insert_profile_memory_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProfileMemoryWrite,
) -> CoreResult<ProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&write.profile_id, &write.target);
    let metadata_json = to_json_text(&write.metadata)?;
    tx.execute(
        "INSERT INTO profile_memories (
            profile_id,
            target_type,
            target_id,
            memory_key,
            content,
            metadata_json,
            revision,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)",
        params![
            write.profile_id.0.as_str(),
            target_type,
            target_id.as_str(),
            write.key.as_str(),
            write.content.as_str(),
            metadata_json,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert profile memory", error))?;
    Ok(ProfileMemoryRecord {
        profile_id: write.profile_id.clone(),
        target: write.target.clone(),
        key: write.key.clone(),
        content: write.content.clone(),
        metadata: write.metadata.clone(),
        revision: 1,
        created_at: write.now.clone(),
        updated_at: write.now.clone(),
    })
}

fn update_profile_memory_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProfileMemoryWrite,
    revision: u64,
) -> CoreResult<ProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&write.profile_id, &write.target);
    let metadata_json = to_json_text(&write.metadata)?;
    tx.execute(
        "UPDATE profile_memories
         SET content = ?5,
             metadata_json = ?6,
             revision = ?7,
             updated_at = ?8
         WHERE profile_id = ?1
           AND target_type = ?2
           AND target_id = ?3
           AND memory_key = ?4",
        params![
            write.profile_id.0.as_str(),
            target_type,
            target_id.as_str(),
            write.key.as_str(),
            write.content.as_str(),
            metadata_json,
            revision as i64,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update profile memory", error))?;
    Ok(ProfileMemoryRecord {
        profile_id: write.profile_id.clone(),
        target: write.target.clone(),
        key: write.key.clone(),
        content: write.content.clone(),
        metadata: write.metadata.clone(),
        revision,
        created_at: get_profile_memory(tx, &write.profile_id, &write.target, &write.key)?
            .map(|record| record.created_at)
            .unwrap_or_else(|| write.now.clone()),
        updated_at: write.now.clone(),
    })
}

fn count_profile_memory_for_profile(conn: &Connection, profile_id: &ProfileId) -> CoreResult<u64> {
    let count = conn
        .query_row(
            "SELECT COUNT(*) FROM profile_memories WHERE profile_id = ?1",
            params![profile_id.0.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("count profile memory", error))?;
    Ok(count as u64)
}

fn row_to_profile_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProfileMemoryRecord> {
    let profile_id = ProfileId(row.get(0)?);
    let target_type: String = row.get(1)?;
    let target_id: String = row.get(2)?;
    let metadata_json: String = row.get(5)?;
    Ok(ProfileMemoryRecord {
        profile_id: profile_id.clone(),
        target: profile_memory_target_from_parts(&profile_id, &target_type, target_id)?,
        key: row.get(3)?,
        content: row.get(4)?,
        metadata: from_json_text(&metadata_json).map_err(to_sql_error)?,
        revision: row.get::<_, i64>(6)? as u64,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub(crate) fn profile_memory_target_parts(
    profile_id: &ProfileId,
    target: &ProfileMemoryTarget,
) -> (&'static str, String) {
    match target {
        ProfileMemoryTarget::Profile => ("profile", profile_id.0.clone()),
        ProfileMemoryTarget::User(user_id) => ("user", user_id.clone()),
    }
}

fn profile_memory_target_from_parts(
    profile_id: &ProfileId,
    target_type: &str,
    target_id: String,
) -> rusqlite::Result<ProfileMemoryTarget> {
    match target_type {
        "profile" if target_id == profile_id.0 => Ok(ProfileMemoryTarget::Profile),
        "user" if !target_id.is_empty() => Ok(ProfileMemoryTarget::User(target_id)),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("invalid profile memory target {other}/{target_id}"),
            )),
        )),
    }
}

pub(crate) fn validate_profile_memory_write(
    write: &ProfileMemoryWrite,
    caps: &ProfileMemoryCaps,
) -> CoreResult<()> {
    validate_profile_memory_key(&write.key, caps.max_key_bytes)?;
    if write.content.len() > caps.max_content_bytes as usize {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "profile memory content exceeds {} bytes",
                caps.max_content_bytes
            ),
        ));
    }
    if let ProfileMemoryTarget::User(user_id) = &write.target {
        if user_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile memory user target must be non-empty",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_profile_memory_key(key: &str, max_key_bytes: u32) -> CoreResult<()> {
    if key.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "profile memory key must be non-empty",
        ));
    }
    if key.len() > max_key_bytes as usize {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("profile memory key exceeds {max_key_bytes} bytes"),
        ));
    }
    Ok(())
}

fn insert_session_memory_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SessionMemoryRecordWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO session_memory_records (
            record_id,
            session_id,
            scope_type,
            scope_id,
            branch_id,
            shape_id,
            shape_version,
            status,
            revision,
            content_json,
            evidence_refs_json,
            source,
            confidence,
            durability_rationale,
            supersedes_record_id,
            superseded_by_record_id,
            archived_at,
            archive_reason,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10, ?11, ?12, ?13, ?14, NULL, NULL, NULL, ?15, ?15)",
        params![
            write.record_id.as_str(),
            write.session_id.0.as_str(),
            memory_scope_type_as_str(write.scope.scope_type),
            write.scope.scope_id.as_str(),
            write.branch_id.as_ref().map(|value| value.0.as_str()),
            write.shape.shape_id.0.as_str(),
            write.shape.version as i64,
            session_memory_status_as_str(SessionMemoryRecordStatus::Active),
            to_json_text(&write.content)?,
            to_json_text(&write.evidence_refs)?,
            memory_proposal_source_as_str(write.source),
            write.confidence,
            write.durability_rationale.as_str(),
            write.supersedes_record_id.as_deref(),
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert session memory record", error))?;
    Ok(())
}

fn update_session_memory_record_content_in_tx(
    tx: &rusqlite::Transaction<'_>,
    replace: &SessionMemoryReplace,
    revision: u64,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE session_memory_records
         SET content_json = ?2,
             evidence_refs_json = ?3,
             source = ?4,
             confidence = ?5,
             durability_rationale = ?6,
             revision = ?7,
             updated_at = ?8
         WHERE record_id = ?1",
        params![
            replace.record_id.as_str(),
            to_json_text(&replace.content)?,
            to_json_text(&replace.evidence_refs)?,
            memory_proposal_source_as_str(replace.source),
            replace.confidence,
            replace.durability_rationale.as_str(),
            revision as i64,
            replace.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("replace session memory record", error))?;
    Ok(())
}

fn mark_session_memory_superseded_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record_id: &str,
    replacement_record_id: &str,
    revision: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE session_memory_records
         SET status = ?2,
             superseded_by_record_id = ?3,
             revision = ?4,
             updated_at = ?5
         WHERE record_id = ?1",
        params![
            record_id,
            session_memory_status_as_str(SessionMemoryRecordStatus::Superseded),
            replacement_record_id,
            revision as i64,
            now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("supersede session memory record", error))?;
    Ok(())
}

fn archive_session_memory_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    archive: &SessionMemoryArchive,
    revision: u64,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE session_memory_records
         SET status = ?2,
             archived_at = ?3,
             archive_reason = ?4,
             revision = ?5,
             updated_at = ?3
         WHERE record_id = ?1",
        params![
            archive.record_id.as_str(),
            session_memory_status_as_str(SessionMemoryRecordStatus::Archived),
            archive.now.as_str(),
            archive.reason.as_deref(),
            revision as i64,
        ],
    )
    .map_err(|error| persistence_error("archive session memory record", error))?;
    Ok(())
}

#[derive(Debug, Clone)]
struct SessionMemoryCompactionScope {
    session_id: SessionId,
    scope_type: MemoryScopeType,
    scope_id: String,
    active_records: u64,
}

pub(crate) fn compact_session_memory_records_in_tx(
    tx: &rusqlite::Transaction<'_>,
    policy: &RuntimeMaintenancePolicy,
    now: &IsoTimestamp,
) -> CoreResult<SessionMemoryCompactionReport> {
    let max_active_records = policy
        .session_memory_max_active_records_per_scope
        .unwrap_or(64)
        .max(1) as u64;
    let archive_batch_size = policy
        .session_memory_archive_batch_size
        .unwrap_or(32)
        .clamp(1, 256) as u64;
    let mut report = SessionMemoryCompactionReport {
        enabled: true,
        ..SessionMemoryCompactionReport::default()
    };
    let scopes = session_memory_compaction_scopes(tx)?;
    report.scopes_inspected = scopes.len() as u64;
    for scope in scopes {
        if scope.active_records <= max_active_records {
            continue;
        }
        report.retention_pressure_scopes += 1;
        let archive_count = (scope.active_records - max_active_records).min(archive_batch_size);
        if archive_count == 0 {
            report.skipped_scopes += 1;
            continue;
        }
        let summary_shape = session_memory_summary_shape(scope.scope_type);
        let candidates =
            session_memory_compaction_candidates(tx, &scope, summary_shape, archive_count)?;
        if candidates.is_empty() {
            report.skipped_scopes += 1;
            continue;
        }
        let summary = match build_session_memory_compaction_summary(tx, &scope, &candidates, now) {
            Ok(summary) => summary,
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                report.skipped_scopes += 1;
                continue;
            }
            Err(error) => return Err(error),
        };
        validate_session_memory_write(&summary)?;
        validate_session_memory_scope_in_tx(
            tx,
            &summary.session_id,
            &summary.scope,
            &summary.branch_id,
        )?;
        insert_session_memory_record_in_tx(tx, &summary)?;
        for record in candidates {
            archive_session_memory_record_in_tx(
                tx,
                &SessionMemoryArchive {
                    record_id: record.record_id.clone(),
                    expected_revision: record.revision,
                    reason: Some(format!(
                        "Compacted into session_memory summary {}",
                        summary.record_id
                    )),
                    now: now.clone(),
                },
                record.revision + 1,
            )?;
            report.records_archived += 1;
        }
        report.scopes_compacted += 1;
        match scope.scope_type {
            MemoryScopeType::ConversationBranch => report.branch_summaries_created += 1,
            _ => report.session_summaries_created += 1,
        }
    }
    Ok(report)
}

fn session_memory_compaction_scopes(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<Vec<SessionMemoryCompactionScope>> {
    let mut stmt = tx
        .prepare(
            "SELECT session_id, scope_type, scope_id, COUNT(*)
             FROM session_memory_records
             WHERE status = 'active'
               AND (
                    (scope_type = 'session' AND shape_id != 'session_summary')
                    OR (scope_type = 'conversation_branch' AND shape_id != 'branch_summary')
               )
             GROUP BY session_id, scope_type, scope_id
             ORDER BY session_id ASC, scope_type ASC, scope_id ASC",
        )
        .map_err(|error| persistence_error("prepare session memory compaction scopes", error))?;
    let rows = stmt
        .query_map([], |row| {
            let scope_type_raw: String = row.get(1)?;
            let scope_type = parse_memory_scope_type(&scope_type_raw).map_err(to_sql_core_error)?;
            Ok(SessionMemoryCompactionScope {
                session_id: SessionId::new(row.get::<_, String>(0)?),
                scope_type,
                scope_id: row.get(2)?,
                active_records: row.get::<_, i64>(3)? as u64,
            })
        })
        .map_err(|error| persistence_error("query session memory compaction scopes", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load session memory compaction scopes", error))
}

fn session_memory_compaction_candidates(
    tx: &rusqlite::Transaction<'_>,
    scope: &SessionMemoryCompactionScope,
    summary_shape: &str,
    limit: u64,
) -> CoreResult<Vec<SessionMemoryRecord>> {
    let mut stmt = tx
        .prepare(
            "SELECT record_id, session_id, scope_type, scope_id, branch_id, shape_id,
                    shape_version, status, revision, content_json, evidence_refs_json,
                    source, confidence, durability_rationale, supersedes_record_id,
                    superseded_by_record_id, archived_at, archive_reason, created_at, updated_at
             FROM session_memory_records
             WHERE session_id = ?1
               AND scope_type = ?2
               AND scope_id = ?3
               AND status = 'active'
               AND shape_id != ?4
             ORDER BY updated_at ASC, record_id ASC
             LIMIT ?5",
        )
        .map_err(|error| {
            persistence_error("prepare session memory compaction candidates", error)
        })?;
    let rows = stmt
        .query_map(
            params![
                scope.session_id.0.as_str(),
                memory_scope_type_as_str(scope.scope_type),
                scope.scope_id.as_str(),
                summary_shape,
                limit as i64,
            ],
            row_to_session_memory_record,
        )
        .map_err(|error| persistence_error("query session memory compaction candidates", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load session memory compaction candidates", error))
}

fn build_session_memory_compaction_summary(
    tx: &rusqlite::Transaction<'_>,
    scope: &SessionMemoryCompactionScope,
    candidates: &[SessionMemoryRecord],
    now: &IsoTimestamp,
) -> CoreResult<SessionMemoryRecordWrite> {
    let record_id = unique_session_memory_summary_record_id(tx, scope, now)?;
    let source_record_ids: Vec<String> = candidates
        .iter()
        .map(|record| record.record_id.clone())
        .collect();
    let coverage_start = candidates
        .first()
        .map(|record| record.record_id.clone())
        .unwrap_or_else(|| "none".to_string());
    let coverage_end = candidates
        .last()
        .map(|record| record.record_id.clone())
        .unwrap_or_else(|| "none".to_string());
    let summary = format!(
        "Compacted {} session_memory records for {} scope {}: {}.",
        candidates.len(),
        memory_scope_type_as_str(scope.scope_type),
        scope.scope_id,
        source_record_ids.join(", ")
    );
    let evidence_refs = session_memory_compaction_evidence_refs(candidates)?;
    let (shape_id, branch_id, content) = match scope.scope_type {
        MemoryScopeType::ConversationBranch => {
            let head_message_id = branch_head_message_id_in_tx(tx, &scope.scope_id)?;
            (
                "branch_summary",
                Some(ConversationBranchId::new(scope.scope_id.clone())),
                serde_json::json!({
                    "record_id": record_id.clone(),
                    "summary": summary.clone(),
                    "branch_id": scope.scope_id.as_str(),
                    "head_message_id": head_message_id,
                    "coverage_start": coverage_start.clone(),
                    "coverage_end": coverage_end.clone(),
                    "created_at": now,
                    "updated_at": now,
                    "source_record_ids": source_record_ids.clone(),
                    "metadata_json": {
                        "generated_by": "runtime_maintenance",
                        "compaction_kind": "retention",
                        "compacted_record_count": candidates.len()
                    }
                }),
            )
        }
        _ => (
            "session_summary",
            None,
            serde_json::json!({
                "record_id": record_id.clone(),
                "summary": summary.clone(),
                "coverage_start": coverage_start.clone(),
                "coverage_end": coverage_end.clone(),
                "summary_kind": "rolling_retention",
                "created_at": now,
                "updated_at": now,
                "source_record_ids": source_record_ids.clone(),
                "metadata_json": {
                    "generated_by": "runtime_maintenance",
                    "compaction_kind": "retention",
                    "compacted_record_count": candidates.len()
                }
            }),
        ),
    };
    Ok(SessionMemoryRecordWrite {
        record_id,
        session_id: scope.session_id.clone(),
        scope: MemoryScope {
            scope_type: scope.scope_type,
            scope_id: scope.scope_id.clone(),
        },
        branch_id,
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked(shape_id),
            version: 1,
        },
        content,
        evidence_refs,
        source: MemoryProposalSource::Migration,
        confidence: 0.75,
        durability_rationale:
            "Runtime maintenance compacted older session_memory records while preserving raw transcript history."
                .to_string(),
        supersedes_record_id: None,
        now: now.clone(),
    })
}

fn unique_session_memory_summary_record_id(
    tx: &rusqlite::Transaction<'_>,
    scope: &SessionMemoryCompactionScope,
    now: &IsoTimestamp,
) -> CoreResult<String> {
    let shape = session_memory_summary_shape(scope.scope_type);
    let timestamp = sanitize_session_memory_record_id_segment(now);
    let scope_id = sanitize_session_memory_record_id_segment(&scope.scope_id);
    let base = format!("{shape}-{scope_id}-{timestamp}");
    let mut candidate = base.clone();
    let mut suffix = 1;
    while get_session_memory_record_in_tx(tx, &candidate)?.is_some() {
        suffix += 1;
        candidate = format!("{base}-{suffix}");
    }
    Ok(candidate)
}

fn sanitize_session_memory_record_id_segment(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn session_memory_summary_shape(scope_type: MemoryScopeType) -> &'static str {
    match scope_type {
        MemoryScopeType::ConversationBranch => "branch_summary",
        _ => "session_summary",
    }
}

fn session_memory_compaction_evidence_refs(
    candidates: &[SessionMemoryRecord],
) -> CoreResult<Vec<MemoryEvidenceRef>> {
    let mut evidence_refs = Vec::new();
    for record in candidates {
        for evidence in &record.evidence_refs {
            if evidence.evidence_type != MemoryEvidenceKind::Wake {
                continue;
            }
            if !evidence_refs
                .iter()
                .any(|existing: &MemoryEvidenceRef| existing.ref_id == evidence.ref_id)
            {
                evidence_refs.push(evidence.clone());
            }
            if evidence_refs.len() >= 16 {
                return Ok(evidence_refs);
            }
        }
    }
    if evidence_refs.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory compaction candidates have no wake evidence",
        ));
    }
    Ok(evidence_refs)
}

fn get_session_memory_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record_id: &str,
) -> CoreResult<Option<SessionMemoryRecord>> {
    tx.query_row(
        "SELECT record_id, session_id, scope_type, scope_id, branch_id, shape_id,
                shape_version, status, revision, content_json, evidence_refs_json,
                source, confidence, durability_rationale, supersedes_record_id,
                superseded_by_record_id, archived_at, archive_reason, created_at, updated_at
         FROM session_memory_records
         WHERE record_id = ?1",
        params![record_id],
        row_to_session_memory_record,
    )
    .optional()
    .map_err(|error| persistence_error("get session memory record", error))
}

fn query_session_memory_records(
    conn: &Connection,
    query: &SessionMemoryQuery,
) -> CoreResult<Vec<SessionMemoryRecord>> {
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT record_id, session_id, scope_type, scope_id, branch_id, shape_id,
                    shape_version, status, revision, content_json, evidence_refs_json,
                    source, confidence, durability_rationale, supersedes_record_id,
                    superseded_by_record_id, archived_at, archive_reason, created_at, updated_at
             FROM session_memory_records
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR branch_id = ?2)
               AND (?3 IS NULL OR scope_type = ?3)
               AND (?4 IS NULL OR shape_id = ?4)
               AND (?5 = 1 OR status != 'superseded')
               AND (?6 = 1 OR status != 'archived')
             ORDER BY updated_at DESC, record_id ASC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|error| persistence_error("prepare query session memory records", error))?;
    let rows = stmt
        .query_map(
            params![
                query.session_id.as_ref().map(|value| value.0.as_str()),
                query.branch_id.as_ref().map(|value| value.0.as_str()),
                query.scope_type.map(memory_scope_type_as_str),
                query.shape_id.as_deref(),
                if query.include_superseded { 1 } else { 0 },
                if query.include_archived { 1 } else { 0 },
                limit,
                offset,
            ],
            row_to_session_memory_record,
        )
        .map_err(|error| persistence_error("query session memory records", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load session memory records", error))
}

fn select_branch_aware_session_memory(
    conn: &Connection,
    query: &BranchAwareSessionMemoryQuery,
) -> CoreResult<SessionMemoryPromptContext> {
    let descriptor = session_memory_space_descriptor();
    let ancestor_distances =
        load_branch_ancestor_distances(conn, &query.session_id, &query.active_branch_id)?;
    let mut records = query_session_memory_records(
        conn,
        &SessionMemoryQuery {
            session_id: Some(query.session_id.clone()),
            shape_id: query.shape_id.clone(),
            include_superseded: true,
            include_archived: true,
            page: None,
            ..SessionMemoryQuery::default()
        },
    )?;
    records.sort_by(|left, right| {
        let left_key = session_memory_sort_key(left, query, &ancestor_distances);
        let right_key = session_memory_sort_key(right, query, &ancestor_distances);
        left_key
            .cmp(&right_key)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.record_id.cmp(&right.record_id))
    });

    let mut excluded_counts = SessionMemoryPromptExcludedCounts::default();
    let mut candidates = Vec::new();
    for record in records {
        if let Some(reason) = session_memory_exclusion_reason(&record, query, &ancestor_distances) {
            increment_session_memory_excluded_count(&mut excluded_counts, reason);
            continue;
        }
        candidates.push(record);
    }

    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let limit = limit as usize;
    let offset = offset as usize;
    let selected = candidates
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    excluded_counts.limit_exceeded = candidates.len().saturating_sub(selected.len()) as u64;
    let character_estimate = selected
        .iter()
        .map(session_memory_record_character_estimate)
        .sum::<u64>();
    let token_estimate = character_estimate.div_ceil(4);
    let selected_records = selected
        .iter()
        .map(|record| SessionMemorySelectedRecordDiagnostic {
            record_id: record.record_id.clone(),
            shape_id: record.shape.shape_id.0.clone(),
        })
        .collect();

    Ok(SessionMemoryPromptContext {
        records: selected,
        diagnostics: SessionMemoryPromptDiagnostics {
            descriptor_id: descriptor.space_id.0,
            descriptor_schema_version: descriptor.schema_version,
            session_id: query.session_id.clone(),
            active_branch_id: query.active_branch_id.clone(),
            selected_records,
            excluded_counts,
            character_estimate,
            token_estimate,
            context_policy: if query.prompt_context_only {
                SessionMemoryPromptContextPolicy::SummaryContext
            } else {
                SessionMemoryPromptContextPolicy::ToolOnly
            },
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionMemoryPromptExclusionReason {
    WrongBranch,
    SiblingBranch,
    ToolOnly,
    Archived,
    Superseded,
    PolicyDisabled,
}

fn load_branch_ancestor_distances(
    conn: &Connection,
    session_id: &SessionId,
    active_branch_id: &Option<ConversationBranchId>,
) -> CoreResult<Vec<(ConversationBranchId, u32)>> {
    let Some(active_branch_id) = active_branch_id else {
        return Ok(Vec::new());
    };
    let active_branch = load_conversation_branch(conn, active_branch_id)?;
    if active_branch.session_id != *session_id {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "active_branch_id does not belong to session_id",
        ));
    }
    let mut ancestors = Vec::new();
    let mut parent = active_branch.parent_branch_id;
    let mut distance = 1;
    while let Some(parent_branch_id) = parent {
        let branch = load_conversation_branch(conn, &parent_branch_id)?;
        if branch.session_id != *session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "conversation branch ancestry crosses session boundary",
            ));
        }
        parent = branch.parent_branch_id.clone();
        ancestors.push((branch.branch_id, distance));
        distance += 1;
    }
    Ok(ancestors)
}

fn session_memory_exclusion_reason(
    record: &SessionMemoryRecord,
    query: &BranchAwareSessionMemoryQuery,
    ancestor_distances: &[(ConversationBranchId, u32)],
) -> Option<SessionMemoryPromptExclusionReason> {
    match record.scope.scope_type {
        MemoryScopeType::Session => {}
        MemoryScopeType::ConversationBranch => {
            let Some(record_branch_id) = &record.branch_id else {
                return Some(SessionMemoryPromptExclusionReason::WrongBranch);
            };
            if query.active_branch_id.as_ref() == Some(record_branch_id) {
            } else if ancestor_distances
                .iter()
                .any(|(branch_id, _)| branch_id == record_branch_id)
            {
                if !query.include_ancestors {
                    return Some(SessionMemoryPromptExclusionReason::WrongBranch);
                }
            } else if query.include_siblings {
            } else if query.active_branch_id.is_some() {
                return Some(SessionMemoryPromptExclusionReason::SiblingBranch);
            } else {
                return Some(SessionMemoryPromptExclusionReason::WrongBranch);
            }
        }
        _ => return Some(SessionMemoryPromptExclusionReason::WrongBranch),
    }

    if query.prompt_context_only {
        match record.status {
            SessionMemoryRecordStatus::Archived => {
                return Some(SessionMemoryPromptExclusionReason::Archived);
            }
            SessionMemoryRecordStatus::Superseded => {
                return Some(SessionMemoryPromptExclusionReason::Superseded);
            }
            SessionMemoryRecordStatus::Active => {}
        }
        if session_memory_policy_disabled(record) {
            return Some(SessionMemoryPromptExclusionReason::PolicyDisabled);
        }
        if session_memory_tool_only(record) {
            return Some(SessionMemoryPromptExclusionReason::ToolOnly);
        }
    }

    None
}

fn increment_session_memory_excluded_count(
    counts: &mut SessionMemoryPromptExcludedCounts,
    reason: SessionMemoryPromptExclusionReason,
) {
    match reason {
        SessionMemoryPromptExclusionReason::WrongBranch => counts.wrong_branch += 1,
        SessionMemoryPromptExclusionReason::SiblingBranch => counts.sibling_branch += 1,
        SessionMemoryPromptExclusionReason::ToolOnly => counts.tool_only += 1,
        SessionMemoryPromptExclusionReason::Archived => counts.archived += 1,
        SessionMemoryPromptExclusionReason::Superseded => counts.superseded += 1,
        SessionMemoryPromptExclusionReason::PolicyDisabled => counts.policy_disabled += 1,
    }
}

fn session_memory_sort_key(
    record: &SessionMemoryRecord,
    query: &BranchAwareSessionMemoryQuery,
    ancestor_distances: &[(ConversationBranchId, u32)],
) -> (u8, u32, u8) {
    let shape_priority = session_memory_shape_prompt_priority(record.shape.shape_id.as_str());
    match record.scope.scope_type {
        MemoryScopeType::ConversationBranch => {
            if query.active_branch_id.as_ref() == record.branch_id.as_ref() {
                (0, 0, shape_priority)
            } else if let Some((_, distance)) =
                record.branch_id.as_ref().and_then(|record_branch| {
                    ancestor_distances
                        .iter()
                        .find(|(branch_id, _)| branch_id == record_branch)
                })
            {
                (1, *distance, shape_priority)
            } else {
                (3, u32::MAX, shape_priority)
            }
        }
        MemoryScopeType::Session => (2, 0, shape_priority),
        _ => (4, u32::MAX, shape_priority),
    }
}

fn session_memory_shape_prompt_priority(shape_id: &str) -> u8 {
    match shape_id {
        "branch_summary" | "session_summary" => 0,
        "user_choice" => 1,
        "session_fact" => 2,
        _ => 3,
    }
}

fn session_memory_tool_only(record: &SessionMemoryRecord) -> bool {
    session_memory_json_policy_flag(&record.content, "tool_only")
        || session_memory_json_policy_eq(&record.content, "prompt_policy", "tool_only")
        || record
            .content
            .get("metadata_json")
            .map(|metadata| {
                session_memory_json_policy_flag(metadata, "tool_only")
                    || session_memory_json_policy_eq(metadata, "prompt_policy", "tool_only")
            })
            .unwrap_or(false)
}

fn session_memory_policy_disabled(record: &SessionMemoryRecord) -> bool {
    session_memory_json_policy_flag(&record.content, "prompt_disabled")
        || session_memory_json_policy_eq(&record.content, "prompt_policy", "never_prompt")
        || record
            .content
            .get("metadata_json")
            .map(|metadata| {
                session_memory_json_policy_flag(metadata, "prompt_disabled")
                    || session_memory_json_policy_eq(metadata, "prompt_policy", "never_prompt")
            })
            .unwrap_or(false)
}

fn session_memory_json_policy_flag(value: &JsonValue, key: &str) -> bool {
    value.get(key).and_then(JsonValue::as_bool).unwrap_or(false)
}

fn session_memory_json_policy_eq(value: &JsonValue, key: &str, expected: &str) -> bool {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(|actual| actual == expected)
        .unwrap_or(false)
}

fn session_memory_record_character_estimate(record: &SessionMemoryRecord) -> u64 {
    to_json_text(&record.content)
        .map(|value| value.len() as u64)
        .unwrap_or(0)
}

fn row_to_session_memory_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMemoryRecord> {
    let scope_type_raw: String = row.get(2)?;
    let shape_id: String = row.get(5)?;
    let status_raw: String = row.get(7)?;
    let content_json: String = row.get(9)?;
    let evidence_refs_json: String = row.get(10)?;
    let source_raw: String = row.get(11)?;
    Ok(SessionMemoryRecord {
        record_id: row.get(0)?,
        session_id: SessionId::new(row.get::<_, String>(1)?),
        scope: MemoryScope {
            scope_type: parse_memory_scope_type(&scope_type_raw).map_err(to_sql_core_error)?,
            scope_id: row.get(3)?,
        },
        branch_id: row
            .get::<_, Option<String>>(4)?
            .map(ConversationBranchId::new),
        shape: MemoryRecordShapeRef {
            shape_id: rusty_crew_core_protocol::MemoryRecordShapeId::new(shape_id)
                .map_err(to_sql_core_error)?,
            version: row.get::<_, i64>(6)? as u32,
        },
        status: parse_session_memory_status(&status_raw).map_err(to_sql_core_error)?,
        revision: row.get::<_, i64>(8)? as u64,
        content: from_json_text(&content_json).map_err(to_sql_error)?,
        evidence_refs: from_json_text(&evidence_refs_json).map_err(to_sql_error)?,
        source: parse_memory_proposal_source(&source_raw).map_err(to_sql_core_error)?,
        confidence: row.get::<_, f64>(12)? as f32,
        durability_rationale: row.get(13)?,
        supersedes_record_id: row.get(14)?,
        superseded_by_record_id: row.get(15)?,
        archived_at: row.get(16)?,
        archive_reason: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
    })
}

fn active_session_memory_record_for_update(
    tx: &rusqlite::Transaction<'_>,
    record_id: &str,
    expected_revision: u64,
) -> CoreResult<SessionMemoryRecord> {
    validate_session_memory_record_id(record_id)?;
    if expected_revision == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory expected_revision must be greater than zero",
        ));
    }
    let existing = get_session_memory_record_in_tx(tx, record_id)?.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("session memory record {record_id} not found"),
        )
    })?;
    if existing.status != SessionMemoryRecordStatus::Active {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("session memory record {record_id} is not active"),
        ));
    }
    if existing.revision != expected_revision {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "session memory revision mismatch for {record_id}: expected {}, found {}",
                expected_revision, existing.revision
            ),
        ));
    }
    Ok(existing)
}

pub(crate) fn validate_session_memory_write(write: &SessionMemoryRecordWrite) -> CoreResult<()> {
    validate_session_memory_record_id(&write.record_id)?;
    validate_session_memory_shape(&write.shape)?;
    validate_session_memory_content(&write.shape, &write.content)?;
    validate_session_memory_provenance(
        &write.evidence_refs,
        write.confidence,
        &write.durability_rationale,
    )?;
    if let Some(content_record_id) = write.content.get("record_id").and_then(JsonValue::as_str) {
        if content_record_id != write.record_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory content.record_id must match record_id",
            ));
        }
    }
    if let Some(supersedes_record_id) = write
        .content
        .get("supersedes_record_id")
        .and_then(JsonValue::as_str)
    {
        if write.supersedes_record_id.as_deref() != Some(supersedes_record_id) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory content.supersedes_record_id must match write metadata",
            ));
        }
    }
    Ok(())
}

fn validate_session_memory_revision_input(
    record_id: &str,
    expected_revision: u64,
    evidence_refs: &[MemoryEvidenceRef],
    confidence: f32,
    durability_rationale: &str,
) -> CoreResult<()> {
    validate_session_memory_record_id(record_id)?;
    if expected_revision == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory expected_revision must be greater than zero",
        ));
    }
    validate_session_memory_provenance(evidence_refs, confidence, durability_rationale)
}

pub(crate) fn validate_session_memory_shape(shape: &MemoryRecordShapeRef) -> CoreResult<()> {
    let descriptor = session_memory_space_descriptor();
    descriptor.validate()?;
    if !descriptor.has_shape(shape) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory shape is not declared by descriptor",
        ));
    }
    Ok(())
}

pub(crate) fn validate_session_memory_content(
    shape_ref: &MemoryRecordShapeRef,
    content: &JsonValue,
) -> CoreResult<()> {
    let descriptor = session_memory_space_descriptor();
    let shape = descriptor
        .record_shapes
        .iter()
        .find(|shape| shape.shape_id == shape_ref.shape_id && shape.version == shape_ref.version)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory shape is not declared by descriptor",
            )
        })?;
    let object = content.as_object().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory content must be a JSON object",
        )
    })?;
    for field in shape.fields.iter().filter(|field| field.required) {
        if !object
            .get(&field.field_name)
            .map(|value| !value.is_null())
            .unwrap_or(false)
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "session memory content missing required field {}",
                    field.field_name
                ),
            ));
        }
    }
    if let Some(confidence) = object.get("confidence").and_then(JsonValue::as_f64) {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory content confidence must be between 0 and 1",
            ));
        }
    }
    Ok(())
}

fn validate_session_memory_provenance(
    evidence_refs: &[MemoryEvidenceRef],
    confidence: f32,
    durability_rationale: &str,
) -> CoreResult<()> {
    validate_memory_confidence(confidence)?;
    if durability_rationale.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory durability_rationale is required",
        ));
    }
    if evidence_refs.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory evidence_refs must not be empty",
        ));
    }
    for evidence in evidence_refs {
        if evidence.ref_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory evidence ref_id must not be empty",
            ));
        }
    }
    let descriptor = session_memory_space_descriptor();
    for required in &descriptor.provenance_policy.required_evidence {
        if !evidence_refs
            .iter()
            .any(|evidence| evidence.evidence_type == *required)
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session memory missing required evidence {:?}", required),
            ));
        }
    }
    Ok(())
}

fn validate_session_memory_scope_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    scope: &MemoryScope,
    branch_id: &Option<ConversationBranchId>,
) -> CoreResult<()> {
    if !session_exists_in_tx(tx, session_id)? {
        return Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("session {session_id} not found for session memory"),
        ));
    }
    match scope.scope_type {
        MemoryScopeType::Session => {
            if scope.scope_id != session_id.0 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "session memory session scope_id must match session_id",
                ));
            }
            if branch_id.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "session-scoped memory must not carry branch_id",
                ));
            }
        }
        MemoryScopeType::ConversationBranch => {
            let branch_id = branch_id.as_ref().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "branch-scoped session memory requires branch_id",
                )
            })?;
            if scope.scope_id != branch_id.0 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "branch-scoped session memory scope_id must match branch_id",
                ));
            }
            let branch_session_id = session_id_for_conversation_branch_in_tx(tx, branch_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::NotFound,
                        format!("conversation branch {branch_id} not found for session memory"),
                    )
                })?;
            if branch_session_id != *session_id {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "branch-scoped session memory branch must belong to session_id",
                ));
            }
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory supports only session and conversation_branch scopes",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_session_memory_record_id(record_id: &str) -> CoreResult<()> {
    if record_id.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory record_id must not be empty",
        ));
    }
    if record_id.len() > 256 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory record_id must be at most 256 characters",
        ));
    }
    if record_id.contains('\0') {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory record_id must not contain NUL",
        ));
    }
    Ok(())
}

pub(crate) fn validate_memory_proposal(
    proposal: &MemoryProposalEnvelope,
    descriptor: &MemorySpaceDescriptor,
) -> CoreResult<()> {
    validate_memory_proposal_policy(proposal, descriptor)?;
    if proposal.space_id.as_str() == "profile_dense" {
        validate_profile_dense_memory_proposal(proposal)?;
    }
    Ok(())
}

fn validate_profile_dense_memory_proposal(proposal: &MemoryProposalEnvelope) -> CoreResult<()> {
    let content = proposal.content.as_object().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "profile_dense proposal content must be an object",
        )
    })?;
    let key = content
        .get("key")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    validate_profile_memory_key(key, ProfileMemoryCaps::default().max_key_bytes)?;
    if matches!(
        proposal.operation,
        MemoryOperation::Add | MemoryOperation::Replace | MemoryOperation::CandidateOnly
    ) {
        let body = content
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if body.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile_dense proposal content.content must be non-empty",
            ));
        }
        if body.len() > ProfileMemoryCaps::default().max_content_bytes as usize {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile_dense proposal content exceeds {} bytes",
                    ProfileMemoryCaps::default().max_content_bytes
                ),
            ));
        }
    }
    Ok(())
}

fn insert_memory_proposal_in_tx(
    tx: &rusqlite::Transaction<'_>,
    proposal: &MemoryProposalEnvelope,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    let envelope_json = to_json_text(proposal)?;
    let status = MemoryProposalReviewStatus::PendingReview;
    let selected_governance_mode =
        selected_governance_mode(proposal.governance_mode, proposal.source);
    tx.execute(
        "INSERT INTO memory_proposals (
            proposal_id,
            space_id,
            operation,
            scope_type,
            scope_id,
            shape_id,
            shape_version,
            envelope_json,
            status,
            selected_governance_mode,
            source,
            dedupe_key,
            duplicate_of,
            resulting_revision,
            created_at,
            updated_at,
            decided_at,
            applied_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, NULL, ?13, ?13, NULL, NULL)",
        params![
            proposal.proposal_id.as_str(),
            proposal.space_id.0.as_str(),
            memory_operation_as_str(proposal.operation),
            memory_scope_type_as_str(proposal.scope.scope_type),
            proposal.scope.scope_id.as_str(),
            proposal.shape.shape_id.0.as_str(),
            proposal.shape.version as i64,
            envelope_json,
            memory_proposal_status_as_str(status),
            memory_governance_mode_as_str(selected_governance_mode),
            memory_proposal_source_as_str(proposal.source),
            proposal.dedupe_key.as_deref(),
            now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert memory proposal", error))?;
    Ok(())
}

fn get_memory_proposal_by_id(
    conn: &Connection,
    proposal_id: &str,
) -> CoreResult<Option<MemoryProposalRecord>> {
    conn.query_row(
        "SELECT envelope_json,
                status,
                selected_governance_mode,
                created_at,
                updated_at,
                decided_at,
                applied_at,
                resulting_revision,
                duplicate_of
         FROM memory_proposals
         WHERE proposal_id = ?1",
        params![proposal_id],
        row_to_memory_proposal,
    )
    .optional()
    .map_err(|error| persistence_error("get memory proposal", error))
}

fn get_memory_proposal_by_dedupe(
    conn: &Connection,
    space_id: &str,
    dedupe_key: &str,
) -> CoreResult<Option<MemoryProposalRecord>> {
    conn.query_row(
        "SELECT envelope_json,
                status,
                selected_governance_mode,
                created_at,
                updated_at,
                decided_at,
                applied_at,
                resulting_revision,
                duplicate_of
         FROM memory_proposals
         WHERE space_id = ?1 AND dedupe_key = ?2",
        params![space_id, dedupe_key],
        row_to_memory_proposal,
    )
    .optional()
    .map_err(|error| persistence_error("get memory proposal by dedupe", error))
}

fn list_memory_proposals(
    conn: &Connection,
    query: &MemoryProposalQuery,
) -> CoreResult<Vec<MemoryProposalRecord>> {
    let (limit, offset) = QueryPage {
        limit: query.limit,
        offset: query.offset,
    }
    .bounded(100, 1_000);
    let status = query.status.map(memory_proposal_status_as_str);
    let mut stmt = conn
        .prepare(
            "SELECT envelope_json,
                    status,
                    selected_governance_mode,
                    created_at,
                    updated_at,
                    decided_at,
                    applied_at,
                    resulting_revision,
                    duplicate_of
             FROM memory_proposals
             WHERE (?1 IS NULL OR space_id = ?1)
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR dedupe_key = ?3)
             ORDER BY updated_at DESC, proposal_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare list memory proposals", error))?;
    let rows = stmt
        .query_map(
            params![
                query.space_id.as_ref().map(|space_id| space_id.0.as_str()),
                status,
                query.dedupe_key.as_deref(),
                limit,
                offset,
            ],
            row_to_memory_proposal,
        )
        .map_err(|error| persistence_error("query memory proposals", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load memory proposals", error))
}

fn insert_or_replace_session_activity_digest(
    conn: &Connection,
    digest: &SessionActivityDigest,
) -> CoreResult<()> {
    digest.validate()?;
    conn.execute(
        "INSERT INTO session_activity_digests (
            digest_id,
            profile_id,
            session_id,
            wake_id,
            source,
            summary_text,
            event_counts_json,
            tool_calls_json,
            signals_json,
            completion_summary,
            allowed_capture_spaces_json,
            created_at,
            retention_until,
            reviewed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(digest_id) DO UPDATE SET
            source = excluded.source,
            summary_text = excluded.summary_text,
            event_counts_json = excluded.event_counts_json,
            tool_calls_json = excluded.tool_calls_json,
            signals_json = excluded.signals_json,
            completion_summary = excluded.completion_summary,
            allowed_capture_spaces_json = excluded.allowed_capture_spaces_json,
            created_at = excluded.created_at,
            retention_until = excluded.retention_until,
            reviewed_at = COALESCE(session_activity_digests.reviewed_at, excluded.reviewed_at)",
        params![
            digest.digest_id.as_str(),
            digest.profile_id.0.as_str(),
            digest.session_id.0.as_str(),
            digest.wake_id.as_str(),
            digest.source.as_str(),
            digest.summary_text.as_str(),
            to_json_text(&digest.event_counts_json)?,
            to_json_text(&digest.tool_calls_json)?,
            to_json_text(&digest.signals_json)?,
            digest.completion_summary.as_deref(),
            to_json_text(&digest.allowed_capture_spaces)?,
            digest.created_at.as_str(),
            digest.retention_until.as_deref(),
            digest.reviewed_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert session activity digest", error))?;
    Ok(())
}

fn get_session_activity_digest_by_id(
    conn: &Connection,
    digest_id: &str,
) -> CoreResult<Option<SessionActivityDigest>> {
    conn.query_row(
        "SELECT digest_id,
                profile_id,
                session_id,
                wake_id,
                source,
                summary_text,
                event_counts_json,
                tool_calls_json,
                signals_json,
                completion_summary,
                allowed_capture_spaces_json,
                created_at,
                retention_until,
                reviewed_at
         FROM session_activity_digests
         WHERE digest_id = ?1",
        params![digest_id],
        row_to_session_activity_digest,
    )
    .optional()
    .map_err(|error| persistence_error("get session activity digest", error))
}

fn list_session_activity_digests(
    conn: &Connection,
    query: &SessionActivityDigestQuery,
) -> CoreResult<Vec<SessionActivityDigest>> {
    let (limit, offset) = QueryPage {
        limit: query.limit,
        offset: query.offset,
    }
    .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT digest_id,
                    profile_id,
                    session_id,
                    wake_id,
                    source,
                    summary_text,
                    event_counts_json,
                    tool_calls_json,
                    signals_json,
                    completion_summary,
                    allowed_capture_spaces_json,
                    created_at,
                    retention_until,
                    reviewed_at
             FROM session_activity_digests
             WHERE (?1 IS NULL OR profile_id = ?1)
               AND (?2 IS NULL OR session_id = ?2)
               AND (?3 IS NULL OR wake_id = ?3)
               AND (?4 OR reviewed_at IS NULL)
             ORDER BY created_at DESC, digest_id ASC
             LIMIT ?5 OFFSET ?6",
        )
        .map_err(|error| persistence_error("prepare list session activity digests", error))?;
    let rows = stmt
        .query_map(
            params![
                query.profile_id.as_ref().map(|id| id.0.as_str()),
                query.session_id.as_ref().map(|id| id.0.as_str()),
                query.wake_id.as_deref(),
                query.include_reviewed,
                limit,
                offset,
            ],
            row_to_session_activity_digest,
        )
        .map_err(|error| persistence_error("query session activity digests", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load session activity digests", error))
}

fn row_to_session_activity_digest(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SessionActivityDigest> {
    let event_counts_json: String = row.get(6)?;
    let tool_calls_json: String = row.get(7)?;
    let signals_json: String = row.get(8)?;
    let allowed_capture_spaces_json: String = row.get(10)?;
    Ok(SessionActivityDigest {
        digest_id: row.get(0)?,
        profile_id: ProfileId(row.get(1)?),
        session_id: SessionId(row.get(2)?),
        wake_id: row.get(3)?,
        source: row.get(4)?,
        summary_text: row.get(5)?,
        event_counts_json: from_json_text(&event_counts_json).map_err(to_sql_error)?,
        tool_calls_json: from_json_text(&tool_calls_json).map_err(to_sql_error)?,
        signals_json: from_json_text(&signals_json).map_err(to_sql_error)?,
        completion_summary: row.get(9)?,
        allowed_capture_spaces: from_json_text(&allowed_capture_spaces_json)
            .map_err(to_sql_error)?,
        created_at: row.get(11)?,
        retention_until: row.get(12)?,
        reviewed_at: row.get(13)?,
    })
}

fn row_to_memory_proposal(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryProposalRecord> {
    let envelope_json: String = row.get(0)?;
    let status: String = row.get(1)?;
    let governance: String = row.get(2)?;
    let resulting_revision: Option<i64> = row.get(7)?;
    Ok(MemoryProposalRecord {
        proposal: from_json_text(&envelope_json).map_err(to_sql_error)?,
        status: parse_memory_proposal_status(&status).map_err(to_sql_core_error)?,
        selected_governance_mode: parse_memory_governance_mode(&governance)
            .map_err(to_sql_core_error)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        decided_at: row.get(5)?,
        applied_at: row.get(6)?,
        resulting_revision: resulting_revision.map(|value| value as u64),
        duplicate_of: row.get(8)?,
    })
}

fn validate_memory_governance_decision(decision: &MemoryGovernanceDecisionInput) -> CoreResult<()> {
    validate_memory_governance_decision_policy(decision)?;
    Ok(())
}

fn validate_memory_governance_transition(
    current: MemoryProposalReviewStatus,
    decision: MemoryGovernanceDecisionKind,
) -> CoreResult<()> {
    validate_memory_governance_transition_policy(current, decision)?;
    Ok(())
}

fn insert_memory_governance_decision_in_tx(
    tx: &rusqlite::Transaction<'_>,
    decision: &MemoryGovernanceDecisionInput,
) -> CoreResult<MemoryGovernanceDecisionRecord> {
    let decided_at = decision.decided_at.clone().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "memory governance decision requires decided_at",
        )
    })?;
    let evidence_refs_json = to_json_text(&decision.evidence_refs)?;
    tx.execute(
        "INSERT INTO memory_governance_decisions (
            decision_id,
            proposal_id,
            decision,
            actor,
            source,
            evidence_refs_json,
            policy_mode,
            confidence,
            message,
            resulting_revision,
            decided_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            decision.decision_id.as_str(),
            decision.proposal_id.as_str(),
            memory_governance_decision_as_str(decision.decision),
            decision.actor.as_str(),
            memory_proposal_source_as_str(decision.source),
            evidence_refs_json,
            memory_governance_mode_as_str(decision.policy_mode),
            decision.confidence.map(|value| value as f64),
            decision.message.as_deref(),
            decision.resulting_revision.map(|value| value as i64),
            decided_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert memory governance decision", error))?;
    Ok(MemoryGovernanceDecisionRecord {
        decision_id: decision.decision_id.clone(),
        proposal_id: decision.proposal_id.clone(),
        decision: decision.decision,
        actor: decision.actor.clone(),
        source: decision.source,
        evidence_refs: decision.evidence_refs.clone(),
        policy_mode: decision.policy_mode,
        confidence: decision.confidence,
        message: decision.message.clone(),
        resulting_revision: decision.resulting_revision,
        decided_at,
    })
}

fn update_memory_proposal_review_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    decision: &MemoryGovernanceDecisionRecord,
) -> CoreResult<()> {
    let next_status = match decision.decision {
        MemoryGovernanceDecisionKind::RoutedToReview => MemoryProposalReviewStatus::PendingReview,
        MemoryGovernanceDecisionKind::Approved => MemoryProposalReviewStatus::Approved,
        MemoryGovernanceDecisionKind::Rejected => MemoryProposalReviewStatus::Rejected,
        MemoryGovernanceDecisionKind::Applied => MemoryProposalReviewStatus::Applied,
    };
    tx.execute(
        "UPDATE memory_proposals
         SET status = ?2,
             updated_at = ?3,
             decided_at = CASE WHEN ?4 IS NULL THEN decided_at ELSE ?4 END,
             applied_at = CASE WHEN ?5 IS NULL THEN applied_at ELSE ?5 END,
             resulting_revision = CASE WHEN ?6 IS NULL THEN resulting_revision ELSE ?6 END
         WHERE proposal_id = ?1",
        params![
            decision.proposal_id.as_str(),
            memory_proposal_status_as_str(next_status),
            decision.decided_at.as_str(),
            if matches!(
                decision.decision,
                MemoryGovernanceDecisionKind::Approved | MemoryGovernanceDecisionKind::Rejected
            ) {
                Some(decision.decided_at.as_str())
            } else {
                None
            },
            if decision.decision == MemoryGovernanceDecisionKind::Applied {
                Some(decision.decided_at.as_str())
            } else {
                None
            },
            decision.resulting_revision.map(|value| value as i64),
        ],
    )
    .map_err(|error| persistence_error("update memory proposal review state", error))?;
    Ok(())
}

fn apply_session_memory_proposal_in_tx(
    tx: &rusqlite::Transaction<'_>,
    proposal: &MemoryProposalEnvelope,
    now: &IsoTimestamp,
) -> CoreResult<u64> {
    if proposal.space_id.as_str() != "session_memory" {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "only session_memory proposals can be applied to session memory records",
        ));
    }
    match proposal.operation {
        MemoryOperation::Add => {
            let write = session_memory_write_from_proposal(tx, proposal, now)?;
            validate_session_memory_write(&write)?;
            validate_session_memory_scope_in_tx(
                tx,
                &write.session_id,
                &write.scope,
                &write.branch_id,
            )?;
            if get_session_memory_record_in_tx(tx, &write.record_id)?.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!("session memory record {} already exists", write.record_id),
                ));
            }
            insert_session_memory_record_in_tx(tx, &write)?;
            Ok(1)
        }
        MemoryOperation::Replace | MemoryOperation::Merge => {
            let record_id = session_memory_proposal_record_id(proposal)?;
            let expected_revision = session_memory_proposal_expected_revision(proposal)?;
            let durability_rationale = session_memory_proposal_rationale(proposal)?;
            validate_session_memory_revision_input(
                &record_id,
                expected_revision,
                &proposal.evidence_refs,
                proposal.confidence,
                durability_rationale,
            )?;
            let existing =
                active_session_memory_record_for_update(tx, &record_id, expected_revision)?;
            validate_session_memory_shape(&proposal.shape)?;
            validate_session_memory_content(&proposal.shape, &proposal.content)?;
            validate_session_memory_scope_in_tx(
                tx,
                &existing.session_id,
                &proposal.scope,
                &existing.branch_id,
            )?;
            let next_revision = existing.revision + 1;
            update_session_memory_record_content_in_tx(
                tx,
                &SessionMemoryReplace {
                    record_id,
                    expected_revision,
                    content: proposal.content.clone(),
                    evidence_refs: proposal.evidence_refs.clone(),
                    source: proposal.source,
                    confidence: proposal.confidence,
                    durability_rationale: durability_rationale.to_string(),
                    now: now.clone(),
                },
                next_revision,
            )?;
            Ok(next_revision)
        }
        MemoryOperation::Supersede => {
            let record_id = session_memory_proposal_supersedes_record_id(proposal)?;
            let expected_revision = session_memory_proposal_expected_revision(proposal)?;
            let replacement = session_memory_write_from_proposal(tx, proposal, now)?;
            if replacement.supersedes_record_id.as_deref() != Some(record_id.as_str()) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "session memory supersede proposal must set content.supersedes_record_id",
                ));
            }
            validate_session_memory_write(&replacement)?;
            validate_session_memory_scope_in_tx(
                tx,
                &replacement.session_id,
                &replacement.scope,
                &replacement.branch_id,
            )?;
            let existing =
                active_session_memory_record_for_update(tx, &record_id, expected_revision)?;
            validate_session_memory_scope_in_tx(
                tx,
                &existing.session_id,
                &existing.scope,
                &existing.branch_id,
            )?;
            if get_session_memory_record_in_tx(tx, &replacement.record_id)?.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!(
                        "session memory replacement record {} already exists",
                        replacement.record_id
                    ),
                ));
            }
            insert_session_memory_record_in_tx(tx, &replacement)?;
            mark_session_memory_superseded_in_tx(
                tx,
                &existing.record_id,
                &replacement.record_id,
                existing.revision + 1,
                now,
            )?;
            Ok(1)
        }
        MemoryOperation::Archive => {
            let record_id = session_memory_proposal_record_id(proposal)?;
            let expected_revision = session_memory_proposal_expected_revision(proposal)?;
            let existing =
                active_session_memory_record_for_update(tx, &record_id, expected_revision)?;
            validate_session_memory_scope_in_tx(
                tx,
                &existing.session_id,
                &proposal.scope,
                &existing.branch_id,
            )?;
            let next_revision = existing.revision + 1;
            archive_session_memory_record_in_tx(
                tx,
                &SessionMemoryArchive {
                    record_id,
                    expected_revision,
                    reason: session_memory_proposal_archive_reason(proposal),
                    now: now.clone(),
                },
                next_revision,
            )?;
            Ok(next_revision)
        }
        _ => Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "session memory proposal operation {:?} cannot be applied",
                proposal.operation
            ),
        )),
    }
}

fn session_memory_write_from_proposal(
    tx: &rusqlite::Transaction<'_>,
    proposal: &MemoryProposalEnvelope,
    now: &IsoTimestamp,
) -> CoreResult<SessionMemoryRecordWrite> {
    let record_id = session_memory_proposal_record_id(proposal)?;
    let session_id = session_id_for_session_memory_proposal(tx, proposal)?;
    let branch_id = match proposal.scope.scope_type {
        MemoryScopeType::Session => None,
        MemoryScopeType::ConversationBranch => {
            Some(ConversationBranchId::new(proposal.scope.scope_id.clone()))
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal scope must be session or conversation_branch",
            ));
        }
    };
    Ok(SessionMemoryRecordWrite {
        record_id,
        session_id,
        scope: proposal.scope.clone(),
        branch_id,
        shape: proposal.shape.clone(),
        content: proposal.content.clone(),
        evidence_refs: proposal.evidence_refs.clone(),
        source: proposal.source,
        confidence: proposal.confidence,
        durability_rationale: session_memory_proposal_rationale(proposal)?.to_string(),
        supersedes_record_id: session_memory_proposal_supersedes_record_id(proposal).ok(),
        now: now.clone(),
    })
}

fn session_id_for_session_memory_proposal(
    tx: &rusqlite::Transaction<'_>,
    proposal: &MemoryProposalEnvelope,
) -> CoreResult<SessionId> {
    match proposal.scope.scope_type {
        MemoryScopeType::Session => Ok(SessionId::new(proposal.scope.scope_id.clone())),
        MemoryScopeType::ConversationBranch => {
            let branch_id = ConversationBranchId::new(proposal.scope.scope_id.clone());
            session_id_for_conversation_branch_in_tx(tx, &branch_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "conversation branch {} not found for session memory proposal",
                        branch_id
                    ),
                )
            })
        }
        _ => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory proposal scope must be session or conversation_branch",
        )),
    }
}

fn session_memory_proposal_record_id(proposal: &MemoryProposalEnvelope) -> CoreResult<String> {
    let record_id = proposal
        .content
        .get("record_id")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal content.record_id is required",
            )
        })?;
    validate_session_memory_record_id(record_id)?;
    Ok(record_id.to_string())
}

fn session_memory_proposal_expected_revision(proposal: &MemoryProposalEnvelope) -> CoreResult<u64> {
    proposal
        .content
        .get("expected_revision")
        .and_then(JsonValue::as_u64)
        .filter(|revision| *revision > 0)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal content.expected_revision must be greater than zero",
            )
        })
}

fn session_memory_proposal_supersedes_record_id(
    proposal: &MemoryProposalEnvelope,
) -> CoreResult<String> {
    let record_id = proposal
        .content
        .get("supersedes_record_id")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory supersede proposal requires content.supersedes_record_id",
            )
        })?;
    validate_session_memory_record_id(record_id)?;
    Ok(record_id.to_string())
}

fn session_memory_proposal_archive_reason(proposal: &MemoryProposalEnvelope) -> Option<String> {
    proposal
        .content
        .get("archive_reason")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn session_memory_proposal_rationale(proposal: &MemoryProposalEnvelope) -> CoreResult<&str> {
    proposal
        .durability_rationale
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal durability_rationale is required",
            )
        })
}

fn selected_governance_mode(
    requested: MemoryGovernanceMode,
    source: MemoryProposalSource,
) -> MemoryGovernanceMode {
    select_memory_governance_mode(requested, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_repo_preserves_profile_revisions_and_proposal_dedupe() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-memory-repo-{}-{}.sqlite3",
            std::process::id(),
            "profile-proposal"
        ));
        let _ = fs::remove_file(&db_path);
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let profile_id = ProfileId::new("memory-repo-profile");
        let caps = ProfileMemoryCaps::default();
        let created = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: profile_id.clone(),
                    target: ProfileMemoryTarget::Profile,
                    key: "style".to_string(),
                    content: "prefers typed repository boundaries".to_string(),
                    metadata: json!({"fixture": "memory_repo"}),
                    now: "2026-07-02T01:00:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();
        assert_eq!(created.revision, 1);
        let conflict = store.replace_profile_memory(
            &ProfileMemoryReplace {
                write: ProfileMemoryWrite {
                    profile_id: profile_id.clone(),
                    target: ProfileMemoryTarget::Profile,
                    key: "style".to_string(),
                    content: "stale write".to_string(),
                    metadata: json!({}),
                    now: "2026-07-02T01:01:00Z".to_string(),
                },
                expected_revision: 99,
            },
            &caps,
        );
        assert!(conflict.is_err());
        let replaced = store
            .replace_profile_memory(
                &ProfileMemoryReplace {
                    write: ProfileMemoryWrite {
                        profile_id: profile_id.clone(),
                        target: ProfileMemoryTarget::Profile,
                        key: "style".to_string(),
                        content: "prefers explicit memory repositories".to_string(),
                        metadata: json!({"fixture": "memory_repo_updated"}),
                        now: "2026-07-02T01:02:00Z".to_string(),
                    },
                    expected_revision: created.revision,
                },
                &caps,
            )
            .unwrap();
        assert_eq!(replaced.revision, 2);

        let proposal = profile_dense_memory_proposal("memory_repo_proposal", "memory_repo:style");
        let descriptor = profile_dense_memory_space_descriptor();
        let first = store
            .save_memory_proposal(&proposal, &descriptor, &"2026-07-02T01:03:00Z".to_string())
            .unwrap();
        let duplicate = store
            .save_memory_proposal(
                &profile_dense_memory_proposal("memory_repo_duplicate", "memory_repo:style"),
                &descriptor,
                &"2026-07-02T01:04:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(duplicate.proposal.proposal_id, first.proposal.proposal_id);
        assert_eq!(store.count_rows("memory_proposals").unwrap(), 1);
        assert_eq!(store.count_rows("memory_governance_decisions").unwrap(), 1);

        drop(store);
        let _ = fs::remove_file(&db_path);
    }

    fn profile_dense_memory_proposal(
        proposal_id: &str,
        dedupe_key: &str,
    ) -> MemoryProposalEnvelope {
        MemoryProposalEnvelope {
            proposal_id: proposal_id.to_string(),
            space_id: MemorySpaceId::unchecked("profile_dense"),
            operation: MemoryOperation::CandidateOnly,
            scope: MemoryScope {
                scope_type: MemoryScopeType::Profile,
                scope_id: "memory-repo-profile".to_string(),
            },
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
                version: 1,
            },
            content: json!({
                "key": "style",
                "content": "prefers explicit memory repositories"
            }),
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: "wake-memory-repo".to_string(),
                label: Some("wake evidence".to_string()),
            }],
            confidence: 0.82,
            durability_rationale: Some("stable profile preference".to_string()),
            governance_mode: MemoryGovernanceMode::DirectWrite,
            source: MemoryProposalSource::InWakeTool,
            dedupe_key: Some(dedupe_key.to_string()),
            created_at: None,
        }
    }

    fn profile_dense_memory_space_descriptor() -> MemorySpaceDescriptor {
        MemorySpaceDescriptor {
            space_id: MemorySpaceId::unchecked("profile_dense"),
            schema_version: 1,
            module_id: Some("runtime_memory".to_string()),
            description: "Compact stable Crew profile memory.".to_string(),
            record_shapes: vec![MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
                version: 1,
                description: "Keyed profile or user memory item.".to_string(),
                fields: vec![
                    memory_field("key", MemoryFieldType::String, true),
                    memory_field("content", MemoryFieldType::Markdown, true),
                ],
            }],
            scope_model: MemoryScopeModel {
                allowed_scopes: vec![MemoryScopeType::Profile, MemoryScopeType::User],
                primary_scope: MemoryScopeType::Profile,
            },
            visibility_model: MemoryVisibilityModel::ProfileLocal,
            retrieval_strategies: vec![MemoryRetrievalStrategy::DirectLookup],
            indexing: MemoryIndexingPolicy {
                required_capabilities: vec!["profile_target_key_lookup".to_string()],
                optional_capabilities: vec![],
            },
            prompt_policy: MemoryPromptPolicy::SummaryContext,
            write_policy: MemoryWritePolicy {
                default_mode: MemoryGovernanceMode::Candidate,
                operation_policies: vec![memory_operation_policy(
                    MemoryOperation::CandidateOnly,
                    false,
                )],
            },
            operations: vec![MemoryOperation::CandidateOnly],
            provenance_policy: MemoryProvenancePolicy {
                required_evidence: vec![MemoryEvidenceKind::Wake],
                source_required: false,
                rationale_required: false,
            },
            retention_policy: MemoryRetentionPolicy::ManualOnly,
            conflict_policy: MemoryConflictPolicy::ExpectedRevision,
            diagnostics: MemoryDiagnosticsPolicy {
                expose_catalog: true,
                expose_record_counts: true,
                expose_policy_decisions: true,
            },
            export_import: MemoryExportImportPolicy {
                export_supported: true,
                import_supported: true,
                import_governance_mode: MemoryGovernanceMode::ManualReview,
            },
        }
    }

    fn memory_field(
        field_name: &str,
        field_type: MemoryFieldType,
        required: bool,
    ) -> MemoryRecordFieldDescriptor {
        MemoryRecordFieldDescriptor {
            field_name: field_name.to_string(),
            field_type,
            required,
            description: format!("{field_name} field"),
        }
    }

    fn memory_operation_policy(
        operation: MemoryOperation,
        requires_expected_revision: bool,
    ) -> MemoryOperationPolicy {
        MemoryOperationPolicy {
            operation,
            governance_mode: MemoryGovernanceMode::Candidate,
            requires_expected_revision,
            min_confidence: None,
        }
    }
}
