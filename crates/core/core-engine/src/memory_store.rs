use rusty_crew_core_persistence::*;
use rusty_crew_core_protocol::{
    ContextCompactionArtifact, ContextCompactionArtifactQuery, CoreResult, IsoTimestamp,
    MemoryGovernanceDecisionInput, MemoryGovernanceDecisionRecord, MemoryProposalEnvelope,
    MemoryProposalQuery, MemoryProposalRecord, MemorySpaceDescriptor, ProfileId,
    SessionActivityDigest, SessionActivityDigestQuery,
};

pub(crate) trait CrewMemoryStore {
    fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>>;
    fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>>;
    fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord>;
    fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord>;
    fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord>;
    fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>>;
    fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext>;
    fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord>;
    fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>>;
    fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest>;
    fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>>;
    fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact>;
    fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>>;
    fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord>;
}

impl CrewMemoryStore for CoreCoordinationStore {
    fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        self.memory().list_profile_memory(query)
    }

    fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        self.memory().get_profile_memory(profile_id, target, key)
    }

    fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.memory().add_profile_memory(write, caps)
    }

    fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.memory().replace_profile_memory(replace, caps)
    }

    fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.memory().remove_profile_memory(delete)
    }

    fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        self.memory().query_session_memory_records(query)
    }

    fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        self.memory().build_session_memory_prompt_context(query)
    }

    fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        self.memory()
            .save_memory_proposal(proposal, descriptor, now)
    }

    fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        self.memory().list_memory_proposals(query)
    }

    fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        self.memory().save_session_activity_digest(digest)
    }

    fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        self.memory().list_session_activity_digests(query)
    }

    fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        self.conversation()
            .save_context_compaction_artifact(artifact)
    }

    fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        self.conversation().list_context_compaction_artifacts(query)
    }

    fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        self.memory()
            .record_memory_governance_decision(decision, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{MemorySpaceId, ProfileId, SessionId};
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeCrewMemoryStore {
        digests: Mutex<Vec<SessionActivityDigest>>,
    }

    impl CrewMemoryStore for FakeCrewMemoryStore {
        fn list_profile_memory(
            &self,
            _query: &ProfileMemoryQuery,
        ) -> CoreResult<Vec<ProfileMemoryRecord>> {
            Ok(Vec::new())
        }

        fn get_profile_memory(
            &self,
            _profile_id: &ProfileId,
            _target: &ProfileMemoryTarget,
            _key: &str,
        ) -> CoreResult<Option<ProfileMemoryRecord>> {
            Ok(None)
        }

        fn add_profile_memory(
            &self,
            _write: &ProfileMemoryWrite,
            _caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord> {
            unimplemented!("not needed for digest fake")
        }

        fn replace_profile_memory(
            &self,
            _replace: &ProfileMemoryReplace,
            _caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord> {
            unimplemented!("not needed for digest fake")
        }

        fn remove_profile_memory(
            &self,
            _delete: &ProfileMemoryDelete,
        ) -> CoreResult<ProfileMemoryRecord> {
            unimplemented!("not needed for digest fake")
        }

        fn query_session_memory_records(
            &self,
            _query: &SessionMemoryQuery,
        ) -> CoreResult<Vec<SessionMemoryRecord>> {
            Ok(Vec::new())
        }

        fn build_session_memory_prompt_context(
            &self,
            _query: &BranchAwareSessionMemoryQuery,
        ) -> CoreResult<SessionMemoryPromptContext> {
            unimplemented!("not needed for digest fake")
        }

        fn save_memory_proposal(
            &self,
            _proposal: &MemoryProposalEnvelope,
            _descriptor: &MemorySpaceDescriptor,
            _now: &IsoTimestamp,
        ) -> CoreResult<MemoryProposalRecord> {
            unimplemented!("not needed for digest fake")
        }

        fn list_memory_proposals(
            &self,
            _query: &MemoryProposalQuery,
        ) -> CoreResult<Vec<MemoryProposalRecord>> {
            Ok(Vec::new())
        }

        fn save_session_activity_digest(
            &self,
            digest: &SessionActivityDigest,
        ) -> CoreResult<SessionActivityDigest> {
            self.digests.lock().unwrap().push(digest.clone());
            Ok(digest.clone())
        }

        fn list_session_activity_digests(
            &self,
            query: &SessionActivityDigestQuery,
        ) -> CoreResult<Vec<SessionActivityDigest>> {
            let digests = self.digests.lock().unwrap();
            Ok(digests
                .iter()
                .filter(|digest| {
                    query
                        .profile_id
                        .as_ref()
                        .is_none_or(|profile_id| &digest.profile_id == profile_id)
                        && query
                            .session_id
                            .as_ref()
                            .is_none_or(|session_id| &digest.session_id == session_id)
                        && query
                            .wake_id
                            .as_ref()
                            .is_none_or(|wake_id| &digest.wake_id == wake_id)
                        && (query.include_reviewed || digest.reviewed_at.is_none())
                })
                .take(query.limit.unwrap_or(100) as usize)
                .cloned()
                .collect())
        }

        fn save_context_compaction_artifact(
            &self,
            _artifact: &ContextCompactionArtifact,
        ) -> CoreResult<ContextCompactionArtifact> {
            unimplemented!("not needed for digest fake")
        }

        fn list_context_compaction_artifacts(
            &self,
            _query: &ContextCompactionArtifactQuery,
        ) -> CoreResult<Vec<ContextCompactionArtifact>> {
            Ok(Vec::new())
        }

        fn record_memory_governance_decision(
            &self,
            _decision: &MemoryGovernanceDecisionInput,
            _now: &IsoTimestamp,
        ) -> CoreResult<MemoryGovernanceDecisionRecord> {
            unimplemented!("not needed for digest fake")
        }
    }

    #[test]
    fn session_activity_digests_use_fake_memory_store() {
        let store = FakeCrewMemoryStore::default();
        let digest = session_activity_digest("wake-1");

        CrewMemoryStore::save_session_activity_digest(&store, &digest).unwrap();
        let listed = CrewMemoryStore::list_session_activity_digests(
            &store,
            &SessionActivityDigestQuery {
                profile_id: Some(ProfileId::new("memory-profile")),
                session_id: Some(SessionId::new("memory-session")),
                wake_id: None,
                include_reviewed: false,
                limit: Some(10),
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(listed, vec![digest]);
    }

    fn session_activity_digest(wake_id: &str) -> SessionActivityDigest {
        SessionActivityDigest {
            digest_id: format!("digest-{wake_id}"),
            profile_id: ProfileId::new("memory-profile"),
            session_id: SessionId::new("memory-session"),
            wake_id: wake_id.to_string(),
            source: "post_wake_capture".to_string(),
            summary_text: "The session discussed durable Crew memory.".to_string(),
            event_counts_json: json!({"messages": 2}),
            tool_calls_json: json!([]),
            signals_json: json!({"candidate": true}),
            completion_summary: Some("done".to_string()),
            allowed_capture_spaces: vec![
                MemorySpaceId::new("profile_dense").expect("valid memory space id")
            ],
            created_at: "2026-07-09T09:40:00Z".to_string(),
            retention_until: None,
            reviewed_at: None,
        }
    }
}
