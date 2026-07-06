use super::*;

impl NativeBridge {
    pub fn validate_runtime_config_draft(
        &self,
        input: RuntimeConfigValidationInput,
    ) -> rusty_crew_core_config::RuntimeConfigValidationResult {
        validate_runtime_config_input(&input)
    }

    pub fn plan_create_profile(&self, input: CreateProfilePlanInput) -> CreateProfilePlan {
        plan_create_profile(&input)
    }

    pub fn plan_profile_registry_mutation(
        &self,
        input: ProfileRegistryMutationRequest,
    ) -> Result<ProfileRegistryMutationPlan, String> {
        rusty_crew_core_config::plan_profile_registry_mutation(&input)
    }

    pub fn plan_runtime_config(&self, input: RuntimeConfigValidationInput) -> RuntimeConfigPlan {
        plan_runtime_config(&input)
    }

    pub fn register_brain_implementation(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<rusty_crew_core_bridge_api::BrainImplementationHandle> {
        let handle = self.brain_registrations.register(registration.clone())?;
        if let Some(engine) = &self.engine {
            engine.register_profile_tool_profile(
                registration.profile_id,
                registration.tool_profile,
            )?;
        }
        Ok(handle)
    }

    pub fn replace_brain_implementation(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<rusty_crew_core_bridge_api::BrainImplementationHandle> {
        let handle = self
            .brain_registrations
            .replace_for_profile(registration.clone())?;
        if let Some(engine) = &self.engine {
            engine.register_profile_tool_profile(
                registration.profile_id,
                registration.tool_profile,
            )?;
        }
        Ok(handle)
    }

    pub fn unregister_brain_implementation_for_profile(
        &mut self,
        profile_id: ProfileId,
    ) -> CoreResult<BrainImplementationHandle> {
        let handle = self
            .brain_registrations
            .unregister_for_profile(&profile_id)?;
        if let Some(engine) = &self.engine {
            engine.unregister_profile_tool_profile(&profile_id)?;
        }
        Ok(handle)
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<rusty_crew_core_bridge_api::ProfileRegistryRecord>> {
        self.engine()?.list_profile_registry_records(query)
    }

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<rusty_crew_core_bridge_api::ProfileRegistryRecord> {
        self.engine()?.create_profile_registry_record(write)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<rusty_crew_core_bridge_api::ProfileRegistryRecord> {
        self.engine()?.update_profile_registry_record(update)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &rusty_crew_core_bridge_api::ProfileId,
    ) -> CoreResult<Option<rusty_crew_core_bridge_api::ProfileRegistryRecord>> {
        self.engine()?.get_profile_registry_record(profile_id)
    }

    pub fn purge_profile(
        &self,
        profile_id: &rusty_crew_core_bridge_api::ProfileId,
    ) -> CoreResult<rusty_crew_core_bridge_api::ProfilePurgeReport> {
        self.engine()?.purge_profile(profile_id)
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<rusty_crew_core_bridge_api::ModelProviderRecord> {
        self.engine()?.upsert_model_provider(write)
    }

    pub fn get_model_provider(
        &self,
        alias: &str,
    ) -> CoreResult<Option<rusty_crew_core_bridge_api::ModelProviderRecord>> {
        self.engine()?.get_model_provider(alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        self.engine()?.get_model_provider_secret(alias)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<rusty_crew_core_bridge_api::ModelProviderRecord>> {
        self.engine()?.list_model_providers(query)
    }

    pub fn model_provider_refresh_impact(
        &self,
        request: &ModelProviderRefreshImpactRequest,
    ) -> CoreResult<rusty_crew_core_bridge_api::ModelProviderRefreshImpact> {
        self.engine()?.model_provider_refresh_impact(request)
    }
}
