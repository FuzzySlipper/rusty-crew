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

    pub fn plan_new_session_control(
        &self,
        input: NewSessionControlPlanInput,
    ) -> NewSessionControlPlan {
        rusty_crew_core_config::plan_new_session_control(&input)
    }

    pub fn plan_reload_mcp_control(
        &self,
        input: ReloadMcpControlPlanInput,
    ) -> ReloadMcpControlPlan {
        rusty_crew_core_config::plan_reload_mcp_control(&input)
    }

    pub fn plan_delegated_role_lifecycle(
        &self,
        input: DelegatedRoleLifecyclePlanInput,
    ) -> DelegatedRoleLifecyclePlan {
        plan_delegated_role_lifecycle(&input)
    }

    pub fn plan_channel_ingress_route(
        &self,
        input: ChannelIngressRoutePlanInput,
    ) -> ChannelIngressRoutePlan {
        plan_channel_ingress_route(&input)
    }

    pub fn plan_den_product_ingress_policy(
        &self,
        input: DenProductIngressPolicyInput,
    ) -> DenProductIngressPolicyPlan {
        plan_den_product_ingress_policy(&input)
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

    pub fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        self.engine()?.upsert_service_credential(write)
    }

    pub fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        self.engine()?.get_service_credential(credential_id)
    }

    pub fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        self.engine()?.get_service_credential_secret(credential_id)
    }

    pub fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        self.engine()?.delete_service_credential(delete)
    }

    pub fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        self.engine()?.list_service_credentials(query)
    }

    pub fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        self.engine()?.link_model_provider_credential(link)
    }

    pub fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        self.engine()?.unlink_model_provider_credential(unlink)
    }

    pub fn put_install_diplomat_binding(
        &self,
        write: InstallDiplomatBindingWrite,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        self.engine()?.put_install_diplomat_binding(write)
    }

    pub fn rebind_install_diplomat(
        &self,
        request: InstallDiplomatRebindRequest,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        self.engine()?.rebind_install_diplomat(request)
    }

    pub fn set_install_diplomat_binding_status(
        &self,
        update: InstallDiplomatBindingStatusUpdate,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        self.engine()?.set_install_diplomat_binding_status(update)
    }

    pub fn get_install_diplomat_binding(
        &self,
        binding_id: &str,
    ) -> CoreResult<Option<InstallDiplomatBindingRecord>> {
        self.engine()?.get_install_diplomat_binding(binding_id)
    }

    pub fn list_install_diplomat_bindings(
        &self,
        query: &InstallDiplomatBindingQuery,
    ) -> CoreResult<Vec<InstallDiplomatBindingRecord>> {
        self.engine()?.list_install_diplomat_bindings(query)
    }

    pub fn plan_telegram_diplomat_ingress(
        &self,
        request: TelegramDiplomatIngressRequest,
    ) -> CoreResult<TelegramDiplomatIngressPlan> {
        self.engine()?.plan_telegram_diplomat_ingress(request)
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

    pub fn plan_model_provider_refresh(
        &self,
        request: &ModelProviderRefreshPlanRequest,
    ) -> CoreResult<rusty_crew_core_bridge_api::ModelProviderRefreshPlan> {
        self.engine()?.plan_model_provider_refresh(request)
    }
}

pub(crate) fn to_profile_registry_query(
    query: WireProfileRegistryQuery,
) -> napi::Result<ProfileRegistryQuery> {
    Ok(ProfileRegistryQuery {
        lifecycle_status: query
            .lifecycle_status
            .as_deref()
            .map(profile_registry_lifecycle_status_from_str)
            .transpose()?,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    })
}

pub(crate) fn profile_registry_lifecycle_status_from_str(
    raw: &str,
) -> napi::Result<ProfileRegistryLifecycleStatus> {
    match raw {
        "active" => Ok(ProfileRegistryLifecycleStatus::Active),
        "paused" => Ok(ProfileRegistryLifecycleStatus::Paused),
        "decommissioned" => Ok(ProfileRegistryLifecycleStatus::Decommissioned),
        "archived" => Ok(ProfileRegistryLifecycleStatus::Archived),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported profile registry lifecycle status {other}"),
        )),
    }
}

pub(crate) fn to_brain_registration(
    registration: JsBrainImplementationRegistration,
) -> napi::Result<BrainImplementationRegistration> {
    Ok(BrainImplementationRegistration {
        implementation_id: rusty_crew_core_bridge_api::BrainImplementationId::new(
            registration.implementation_id,
        ),
        profile_id: rusty_crew_core_bridge_api::ProfileId::new(registration.profile_id),
        tool_profile: rusty_crew_core_bridge_api::ToolProfile {
            tools: registration
                .tool_profile
                .tools
                .into_iter()
                .map(|tool| rusty_crew_core_bridge_api::ToolDescriptor {
                    name: tool.name,
                    description: tool.description,
                    input_schema: tool
                        .input_schema
                        .map(|handle| RuntimeBufferHandle::new(handle as u64)),
                })
                .collect(),
        },
        model_config: rusty_crew_core_bridge_api::BrainModelConfig {
            provider: registration.model_config.provider,
            model_name: registration.model_config.model_name,
            temperature_milli: registration.model_config.temperature_milli,
            max_output_tokens: registration.model_config.max_output_tokens,
        },
        strategy: registration
            .strategy
            .map(to_brain_strategy_metadata)
            .transpose()?,
        provider_state_scope: registration.provider_state_scope.map(|scope| {
            rusty_crew_core_bridge_api::BrainProviderStateScope {
                profile_fingerprint: scope.profile_fingerprint,
                provider_fingerprint: scope.provider_fingerprint,
                compatibility: scope.compatibility.map(|facts| {
                    rusty_crew_core_bridge_api::ProviderStateCompatibilityFacts {
                        version: facts.version,
                        profile_identity: facts.profile_identity,
                        display_metadata: facts.display_metadata,
                        prompt: facts.prompt,
                        skills: facts.skills,
                        tool_catalog: facts.tool_catalog,
                        provider_endpoint: facts.provider_endpoint,
                        model: facts.model,
                        protocol: facts.protocol,
                        dialect: facts.dialect,
                        reasoning_semantics: facts.reasoning_semantics,
                        brain_module: facts.brain_module,
                        brain_strategy: facts.brain_strategy,
                        provider_state_schema: facts.provider_state_schema,
                    }
                }),
            }
        }),
    })
}

pub(crate) fn to_brain_strategy_metadata(
    strategy: JsBrainStrategyMetadata,
) -> napi::Result<rusty_crew_core_bridge_api::BrainStrategyMetadata> {
    Ok(rusty_crew_core_bridge_api::BrainStrategyMetadata {
        module_id: strategy.module_id,
        strategy_id: strategy.strategy_id,
        provider_state: rusty_crew_core_bridge_api::BrainProviderStateStrategyMetadata {
            mode: parse_provider_state_mode(&strategy.provider_state.mode)?,
        },
    })
}

pub(crate) fn parse_provider_state_mode(
    mode: &str,
) -> napi::Result<rusty_crew_core_bridge_api::ProviderStateMode> {
    match mode {
        "unused" => Ok(rusty_crew_core_bridge_api::ProviderStateMode::Unused),
        "optional" => Ok(rusty_crew_core_bridge_api::ProviderStateMode::Optional),
        "required" => Ok(rusty_crew_core_bridge_api::ProviderStateMode::Required),
        _ => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unknown provider state mode {mode}"),
        )),
    }
}
