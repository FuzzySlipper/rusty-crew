use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn list_profile_memory(
        &self,
        query: JsProfileMemoryQuery,
    ) -> napi::Result<Vec<JsProfileMemoryRecord>> {
        let bridge = self.bridge()?;
        let records = bridge
            .list_profile_memory(&to_profile_memory_query(query)?)
            .map_err(to_napi_error)?;
        records
            .into_iter()
            .map(to_js_profile_memory_record)
            .collect()
    }

    #[napi]
    pub fn list_memory_space_descriptors_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let descriptors = bridge
            .list_memory_space_descriptors()
            .map_err(to_napi_error)?;
        serialize_json(&descriptors, "memory space descriptors")
    }

    #[napi]
    pub fn query_session_memory_records_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<SessionMemoryQuery>(&input_json, "session memory query")?;
        let records = bridge
            .query_session_memory_records(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "session memory records")
    }

    #[napi]
    pub fn build_session_memory_prompt_context_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<BranchAwareSessionMemoryQuery>(
            &input_json,
            "session memory prompt context query",
        )?;
        let context = bridge
            .build_session_memory_prompt_context(&query)
            .map_err(to_napi_error)?;
        serialize_json(&context, "session memory prompt context")
    }

    #[napi]
    pub fn save_memory_proposal_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let proposal = parse_json::<MemoryProposalEnvelope>(&input_json, "memory proposal")?;
        let record = bridge
            .save_memory_proposal(proposal)
            .map_err(to_napi_error)?;
        serialize_json(&record, "memory proposal record")
    }

    #[napi]
    pub fn plan_capture_memory_proposals_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            parse_json::<CaptureMemoryProposalPlanInput>(&input_json, "capture proposal plan")?;
        let plan = plan_capture_memory_proposals(input);
        serialize_json(&plan, "capture proposal plan")
    }

    #[napi]
    pub fn list_memory_proposals_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<MemoryProposalQuery>(&input_json, "memory proposal query")?;
        let records = bridge
            .list_memory_proposals(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "memory proposal records")
    }

    #[napi]
    pub fn save_session_activity_digest_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let digest = parse_json::<SessionActivityDigest>(&input_json, "session activity digest")?;
        let record = bridge
            .save_session_activity_digest(&digest)
            .map_err(to_napi_error)?;
        serialize_json(&record, "session activity digest")
    }

    #[napi]
    pub fn list_session_activity_digests_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<SessionActivityDigestQuery>(&input_json, "session activity digest query")?;
        let records = bridge
            .list_session_activity_digests(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "session activity digests")
    }

    #[napi]
    pub fn save_context_compaction_artifact_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let artifact =
            parse_json::<ContextCompactionArtifact>(&input_json, "context compaction artifact")?;
        let record = bridge
            .save_context_compaction_artifact(&artifact)
            .map_err(to_napi_error)?;
        serialize_json(&record, "context compaction artifact")
    }

    #[napi]
    pub fn list_context_compaction_artifacts_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ContextCompactionArtifactQuery>(
            &input_json,
            "context compaction artifact query",
        )?;
        let records = bridge
            .list_context_compaction_artifacts(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "context compaction artifacts")
    }

    #[napi]
    pub fn record_memory_governance_decision_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let decision =
            parse_json::<MemoryGovernanceDecisionInput>(&input_json, "memory governance decision")?;
        let record = bridge
            .record_memory_governance_decision(&decision)
            .map_err(to_napi_error)?;
        serialize_json(&record, "memory governance decision record")
    }

    #[napi]
    pub fn get_profile_memory(
        &self,
        profile_id: String,
        target_type: String,
        target_id: Option<String>,
        key: String,
    ) -> napi::Result<Option<JsProfileMemoryRecord>> {
        let bridge = self.bridge()?;
        let profile_id = rusty_crew_core_bridge_api::ProfileId::new(profile_id);
        let target = to_profile_memory_target(&profile_id, &target_type, target_id)?;
        bridge
            .get_profile_memory(&profile_id, &target, &key)
            .map_err(to_napi_error)?
            .map(to_js_profile_memory_record)
            .transpose()
    }

    #[napi]
    pub fn add_profile_memory(
        &self,
        write: JsProfileMemoryWrite,
    ) -> napi::Result<JsProfileMemoryRecord> {
        let caps = to_profile_memory_caps(write.caps.as_ref());
        let bridge = self.bridge()?;
        let record = bridge
            .add_profile_memory(to_profile_memory_write(write)?, &caps)
            .map_err(to_napi_error)?;
        to_js_profile_memory_record(record)
    }

    #[napi]
    pub fn replace_profile_memory(
        &self,
        replace: JsProfileMemoryReplace,
    ) -> napi::Result<JsProfileMemoryRecord> {
        let caps = to_profile_memory_caps(replace.write.caps.as_ref());
        let bridge = self.bridge()?;
        let record = bridge
            .replace_profile_memory(
                ProfileMemoryReplace {
                    write: to_profile_memory_write(replace.write)?,
                    expected_revision: replace.expected_revision as u64,
                },
                &caps,
            )
            .map_err(to_napi_error)?;
        to_js_profile_memory_record(record)
    }

    #[napi]
    pub fn remove_profile_memory(
        &self,
        delete: JsProfileMemoryDelete,
    ) -> napi::Result<JsProfileMemoryRecord> {
        let bridge = self.bridge()?;
        let profile_id = rusty_crew_core_bridge_api::ProfileId::new(delete.profile_id);
        let record = bridge
            .remove_profile_memory(&ProfileMemoryDelete {
                target: to_profile_memory_target(
                    &profile_id,
                    &delete.target_type,
                    delete.target_id,
                )?,
                profile_id,
                key: delete.key,
                expected_revision: delete.expected_revision as u64,
            })
            .map_err(to_napi_error)?;
        to_js_profile_memory_record(record)
    }
}
