use super::*;

impl NativeBridge {
    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        self.engine()?.list_profile_memory(query)
    }

    pub fn list_memory_space_descriptors(&self) -> CoreResult<Vec<MemorySpaceDescriptor>> {
        self.engine()?.list_memory_space_descriptors()
    }

    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        self.engine()?.query_session_memory_records(query)
    }

    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        self.engine()?.build_session_memory_prompt_context(query)
    }

    pub fn save_memory_proposal(
        &self,
        proposal: MemoryProposalEnvelope,
    ) -> CoreResult<MemoryProposalRecord> {
        self.engine()?.save_memory_proposal(proposal)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        self.engine()?.list_memory_proposals(query)
    }

    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        self.engine()?.save_session_activity_digest(digest)
    }

    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        self.engine()?.list_session_activity_digests(query)
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        self.engine()?.save_context_compaction_artifact(artifact)
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        self.engine()?.list_context_compaction_artifacts(query)
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        self.engine()?.record_memory_governance_decision(decision)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &rusty_crew_core_bridge_api::ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        self.engine()?.get_profile_memory(profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        write: ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.engine()?.add_profile_memory(write, caps)
    }

    pub fn replace_profile_memory(
        &self,
        replace: ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.engine()?.replace_profile_memory(replace, caps)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.engine()?.remove_profile_memory(delete)
    }
}

pub(crate) fn to_js_profile_memory_record(
    record: ProfileMemoryRecord,
) -> napi::Result<JsProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&record.profile_id, &record.target);
    Ok(JsProfileMemoryRecord {
        profile_id: record.profile_id.0,
        target_type: target_type.to_string(),
        target_id,
        key: record.key,
        content: record.content,
        metadata_json: serde_json::to_string(&record.metadata)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?,
        revision: record.revision as f64,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

pub(crate) fn to_profile_memory_query(
    query: JsProfileMemoryQuery,
) -> napi::Result<ProfileMemoryQuery> {
    let profile_id = rusty_crew_core_bridge_api::ProfileId::new(query.profile_id);
    let target = match query.target_type {
        Some(target_type) => Some(to_profile_memory_target(
            &profile_id,
            &target_type,
            query.target_id,
        )?),
        None => None,
    };
    Ok(ProfileMemoryQuery {
        profile_id,
        target,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    })
}

pub(crate) fn to_profile_memory_write(
    write: JsProfileMemoryWrite,
) -> napi::Result<ProfileMemoryWrite> {
    let profile_id = rusty_crew_core_bridge_api::ProfileId::new(write.profile_id);
    let target = to_profile_memory_target(&profile_id, &write.target_type, write.target_id)?;
    let metadata = write
        .metadata_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(ProfileMemoryWrite {
        profile_id,
        target,
        key: write.key,
        content: write.content,
        metadata,
        now: String::new(),
    })
}

pub(crate) fn to_profile_memory_target(
    profile_id: &rusty_crew_core_bridge_api::ProfileId,
    target_type: &str,
    target_id: Option<String>,
) -> napi::Result<ProfileMemoryTarget> {
    match target_type {
        "profile" => Ok(ProfileMemoryTarget::Profile),
        "user" => target_id
            .filter(|value| !value.trim().is_empty())
            .map(ProfileMemoryTarget::User)
            .ok_or_else(|| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    "user profile memory target requires targetId".to_string(),
                )
            }),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!(
                "unsupported profile memory target type {other} for profile {}",
                profile_id.0
            ),
        )),
    }
}

pub(crate) fn to_profile_memory_caps(caps: Option<&JsProfileMemoryCaps>) -> ProfileMemoryCaps {
    let defaults = ProfileMemoryCaps::default();
    ProfileMemoryCaps {
        max_records_per_profile: caps
            .and_then(|caps| caps.max_records_per_profile)
            .unwrap_or(defaults.max_records_per_profile),
        max_key_bytes: caps
            .and_then(|caps| caps.max_key_bytes)
            .unwrap_or(defaults.max_key_bytes),
        max_content_bytes: caps
            .and_then(|caps| caps.max_content_bytes)
            .unwrap_or(defaults.max_content_bytes),
    }
}

pub(crate) fn profile_memory_target_parts(
    profile_id: &rusty_crew_core_bridge_api::ProfileId,
    target: &ProfileMemoryTarget,
) -> (&'static str, String) {
    match target {
        ProfileMemoryTarget::Profile => ("profile", profile_id.0.clone()),
        ProfileMemoryTarget::User(user_id) => ("user", user_id.clone()),
    }
}
