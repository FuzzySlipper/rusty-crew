use super::*;

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
