use super::*;

impl CoreEngine {
    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        RuntimeServiceDataStore::list_profile_registry_records(&self.store, query)
    }

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        RuntimeServiceDataStore::create_profile_registry_record(&self.store, write)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &rusty_crew_core_protocol::ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        RuntimeServiceDataStore::update_profile_registry_record(&self.store, update)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        RuntimeServiceDataStore::get_profile_registry_record(&self.store, profile_id)
    }

    pub fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        let removed_sessions = self.sessions.remove_sessions_for_profile(profile_id)?;
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile tool profiles lock poisoned",
                )
            })?
            .remove(profile_id);
        let mut report = RuntimeServiceDataStore::purge_profile(&self.store, profile_id)?;
        for state in removed_sessions {
            if !report
                .session_ids
                .iter()
                .any(|session_id| session_id == &state.session_id)
            {
                report.session_ids.push(state.session_id);
            }
            if !report
                .agent_ids
                .iter()
                .any(|agent_id| agent_id == &state.agent_id)
            {
                report.agent_ids.push(state.agent_id);
            }
        }
        Ok(report)
    }

    pub fn upsert_model_endpoint(
        &self,
        write: &ModelEndpointWrite,
    ) -> CoreResult<ModelEndpointRecord> {
        RuntimeServiceDataStore::upsert_model_endpoint(&self.store, write)
    }

    pub fn get_model_endpoint(&self, endpoint_id: &str) -> CoreResult<Option<ModelEndpointRecord>> {
        RuntimeServiceDataStore::get_model_endpoint(&self.store, endpoint_id)
    }

    pub fn list_model_endpoints(
        &self,
        query: &ModelEndpointQuery,
    ) -> CoreResult<Vec<ModelEndpointRecord>> {
        RuntimeServiceDataStore::list_model_endpoints(&self.store, query)
    }

    pub fn delete_model_endpoint(
        &self,
        delete: &ModelEndpointDelete,
    ) -> CoreResult<ModelEndpointRecord> {
        RuntimeServiceDataStore::delete_model_endpoint(&self.store, delete)
    }

    pub fn upsert_model_configuration(
        &self,
        write: &ModelConfigurationWrite,
    ) -> CoreResult<ModelConfigurationRecord> {
        RuntimeServiceDataStore::upsert_model_configuration(&self.store, write)
    }

    pub fn get_model_configuration(
        &self,
        model_config_id: &str,
    ) -> CoreResult<Option<ModelConfigurationRecord>> {
        RuntimeServiceDataStore::get_model_configuration(&self.store, model_config_id)
    }

    pub fn list_model_configurations(
        &self,
        query: &ModelConfigurationQuery,
    ) -> CoreResult<Vec<ModelConfigurationRecord>> {
        RuntimeServiceDataStore::list_model_configurations(&self.store, query)
    }

    pub fn delete_model_configuration(
        &self,
        delete: &ModelConfigurationDelete,
    ) -> CoreResult<ModelConfigurationRecord> {
        let mut offset = 0_u32;
        let mut referencing_profiles = Vec::new();
        loop {
            let profiles = RuntimeServiceDataStore::list_profile_registry_records(
                &self.store,
                &ProfileRegistryQuery {
                    lifecycle_status: None,
                    page: Some(QueryPage {
                        limit: Some(1_000),
                        offset: Some(offset),
                    }),
                },
            )?;
            let page_len = profiles.len() as u32;
            referencing_profiles.extend(
                profiles
                    .into_iter()
                    .filter(|profile| {
                        profile_registry_model_config_id(profile).as_deref()
                            == Some(delete.model_config_id.as_str())
                    })
                    .map(|profile| profile.profile_id.0),
            );
            if page_len < 1_000 {
                break;
            }
            offset = offset.saturating_add(page_len);
        }
        if !referencing_profiles.is_empty() {
            referencing_profiles.sort();
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model configuration {} is still referenced by profile(s): {}",
                    delete.model_config_id,
                    referencing_profiles.join(", ")
                ),
            ));
        }
        RuntimeServiceDataStore::delete_model_configuration(&self.store, delete)
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        RuntimeServiceDataStore::upsert_model_provider(&self.store, write)
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        RuntimeServiceDataStore::get_model_provider(&self.store, alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        RuntimeServiceDataStore::get_model_provider_secret(&self.store, alias)
    }

    pub fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        RuntimeServiceDataStore::upsert_service_credential(&self.store, write)
    }

    pub fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        RuntimeServiceDataStore::get_service_credential(&self.store, credential_id)
    }

    pub fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        RuntimeServiceDataStore::get_service_credential_secret(&self.store, credential_id)
    }

    pub fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        RuntimeServiceDataStore::delete_service_credential(&self.store, delete)
    }

    pub fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        RuntimeServiceDataStore::list_service_credentials(&self.store, query)
    }

    pub fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        RuntimeServiceDataStore::link_model_provider_credential(&self.store, link)
    }

    pub fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        RuntimeServiceDataStore::unlink_model_provider_credential(&self.store, unlink)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        RuntimeServiceDataStore::list_model_providers(&self.store, query)
    }

    pub fn model_provider_refresh_impact(
        &self,
        request: &ModelProviderRefreshImpactRequest,
    ) -> CoreResult<ModelProviderRefreshImpact> {
        let provider_alias = request.provider_alias.trim();
        if provider_alias.is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "model provider refresh impact provider_alias is required",
            ));
        }

        let profiles = self
            .store
            .service_data()
            .list_profile_registry_records(&ProfileRegistryQuery::default())?;
        let sessions = self.sessions.all_sessions()?;
        let mut affected_profiles = Vec::new();

        for profile in profiles {
            if profile_registry_provider_alias(&profile).as_deref() != Some(provider_alias) {
                continue;
            }

            let configured_session_ids = profile
                .derived_runtime_refs
                .iter()
                .filter(|runtime_ref| {
                    runtime_ref.ref_kind == "session"
                        && runtime_ref.status != "archived"
                        && runtime_ref.status != "disabled"
                })
                .map(|runtime_ref| SessionId::new(runtime_ref.ref_id.clone()))
                .collect::<HashSet<_>>();
            let active_session_ids = sessions
                .iter()
                .filter(|session| {
                    session.profile_id == profile.profile_id
                        && session.status != SessionStatus::Archived
                })
                .map(|session| session.session_id.clone())
                .collect::<HashSet<_>>();
            let mut session_ids = configured_session_ids
                .union(&active_session_ids)
                .cloned()
                .collect::<Vec<_>>();
            session_ids.sort_by(|left, right| left.0.cmp(&right.0));
            let mut configured_session_ids = configured_session_ids.into_iter().collect::<Vec<_>>();
            configured_session_ids.sort_by(|left, right| left.0.cmp(&right.0));
            let mut active_session_ids = active_session_ids.into_iter().collect::<Vec<_>>();
            active_session_ids.sort_by(|left, right| left.0.cmp(&right.0));

            affected_profiles.push(rusty_crew_core_protocol::ModelProviderAffectedProfile {
                profile_id: profile.profile_id,
                session_ids,
                configured_session_ids,
                active_session_ids,
            });
        }

        affected_profiles.sort_by(|left, right| left.profile_id.0.cmp(&right.profile_id.0));

        Ok(ModelProviderRefreshImpact {
            provider_alias: provider_alias.to_string(),
            affected_profiles,
        })
    }

    pub fn plan_model_provider_refresh(
        &self,
        request: &ModelProviderRefreshPlanRequest,
    ) -> CoreResult<ModelProviderRefreshPlan> {
        let impact = self.model_provider_refresh_impact(&ModelProviderRefreshImpactRequest {
            provider_alias: request.provider_alias.clone(),
        })?;
        let command_name = match request.mode {
            ModelProviderRefreshMode::None => None,
            ModelProviderRefreshMode::Plan => Some("plan_runtime_rebuild"),
            ModelProviderRefreshMode::Apply => Some("apply_runtime_rebuild"),
        };
        let actions = command_name
            .map(|command_name| {
                impact
                    .affected_profiles
                    .iter()
                    .map(|affected| {
                        let profile_id = affected.profile_id.to_string();
                        ModelProviderRefreshProfileAction {
                            profile_id: affected.profile_id.clone(),
                            command_name: command_name.to_string(),
                            reason: format!("model provider {} updated", impact.provider_alias),
                            planned_summary: format!(
                                "runtime rebuild plan prepared for profile {profile_id}"
                            ),
                            applied_summary: format!(
                                "runtime rebuild applied for profile {profile_id}"
                            ),
                            blocked_summary: format!(
                                "runtime rebuild blocked for profile {profile_id}"
                            ),
                            failure_reason_code: "model_provider_refresh_failed".to_string(),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(ModelProviderRefreshPlan {
            provider_alias: impact.provider_alias,
            mode: request.mode.clone(),
            affected_profiles: impact.affected_profiles,
            actions,
        })
    }
}

fn profile_registry_provider_alias(record: &ProfileRegistryRecord) -> Option<String> {
    record
        .active_runtime_settings_json
        .get("providerAlias")
        .or_else(|| record.active_runtime_settings_json.get("provider_alias"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn profile_registry_model_config_id(record: &ProfileRegistryRecord) -> Option<String> {
    let settings = &record.active_runtime_settings_json;
    let nested = settings.get("profile").unwrap_or(&serde_json::Value::Null);
    [settings, nested].into_iter().find_map(|scope| {
        scope
            .get("modelConfigId")
            .or_else(|| scope.get("model_config_id"))
            .or_else(|| scope.get("providerAlias"))
            .or_else(|| scope.get("provider_alias"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}
