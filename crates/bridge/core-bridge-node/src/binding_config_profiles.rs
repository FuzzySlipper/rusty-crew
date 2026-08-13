use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn validate_tool_metadata_policy_json(&self, input_json: String) -> napi::Result<String> {
        let input: ToolMetadataPolicyValidationInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid tool metadata policy input JSON: {error}"),
                )
            })?;
        let result = validate_tool_metadata_policy(&input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn validate_local_tool_profile_policy_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input: LocalToolProfileValidationInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid local tool profile policy input JSON: {error}"),
                )
            })?;
        let result = validate_local_tool_profile_policy(&input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_tool_availability_json(&self, input_json: String) -> napi::Result<String> {
        let input: ToolAvailabilityPlanInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid tool availability plan input JSON: {error}"),
                )
            })?;
        let result = plan_tool_availability(&input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_local_code_resource_policy_json(&self, input_json: String) -> napi::Result<String> {
        let input: LocalCodeResourcePolicyInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid local code resource policy input JSON: {error}"),
                )
            })?;
        let result = plan_local_code_resource_policy(&input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_web_browser_resource_policy_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input: WebBrowserResourcePolicyInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid web/browser resource policy input JSON: {error}"),
                )
            })?;
        let result = plan_web_browser_resource_policy(&input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn validate_runtime_config_draft_json(&self, input_json: String) -> napi::Result<String> {
        let input: RuntimeConfigValidationInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime config validation input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let result = bridge.validate_runtime_config_draft(input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_create_profile_json(&self, input_json: String) -> napi::Result<String> {
        let input: CreateProfilePlanInput = serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid create-profile plan input JSON: {error}"),
            )
        })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_create_profile(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_profile_registry_mutation_json(&self, input_json: String) -> napi::Result<String> {
        let input: ProfileRegistryMutationRequest =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid profile registry mutation plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge
            .plan_profile_registry_mutation(input)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error))?;
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_new_session_control_json(&self, input_json: String) -> napi::Result<String> {
        let input: NewSessionControlPlanInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid new-session control plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_new_session_control(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_reload_mcp_control_json(&self, input_json: String) -> napi::Result<String> {
        let input: ReloadMcpControlPlanInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid reload-MCP control plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_reload_mcp_control(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_delegated_role_lifecycle_json(&self, input_json: String) -> napi::Result<String> {
        let input: DelegatedRoleLifecyclePlanInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid delegated role lifecycle plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_delegated_role_lifecycle(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_channel_ingress_route_json(&self, input_json: String) -> napi::Result<String> {
        let input: ChannelIngressRoutePlanInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid channel ingress route plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_channel_ingress_route(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_den_product_ingress_policy_json(&self, input_json: String) -> napi::Result<String> {
        let input: DenProductIngressPolicyInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid Den product ingress policy input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_den_product_ingress_policy(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_runtime_config_json(&self, input_json: String) -> napi::Result<String> {
        let input: RuntimeConfigValidationInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime config plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_runtime_config(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_runtime_graph_json(&self, input_json: String) -> napi::Result<String> {
        let input: RuntimeGraphPlanInput = serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid runtime graph plan input JSON: {error}"),
            )
        })?;
        serde_json::to_string(&plan_runtime_graph(&input))
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn create_profile_registry_record_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<ProfileRegistryWrite>(&write_json, "profile registry write")?;
        let record = bridge
            .create_profile_registry_record(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "profile registry record")
    }

    #[napi]
    pub fn update_profile_registry_record_json(&self, update_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let update = parse_json::<ProfileRegistryUpdate>(&update_json, "profile registry update")?;
        let record = bridge
            .update_profile_registry_record(&update)
            .map_err(to_napi_error)?;
        serialize_json(&record, "profile registry record")
    }

    #[napi]
    pub fn list_profile_registry_records_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<WireProfileRegistryQuery>(&query_json, "profile registry query")?;
        let records = bridge
            .list_profile_registry_records(&to_profile_registry_query(query)?)
            .map_err(to_napi_error)?;
        serialize_json(&records, "profile registry records")
    }

    #[napi]
    pub fn get_profile_registry_record_json(&self, profile_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_profile_registry_record(&rusty_crew_core_bridge_api::ProfileId::new(profile_id))
            .map_err(to_napi_error)?;
        serialize_json(&record, "profile registry record")
    }

    #[napi]
    pub fn purge_profile_json(&self, profile_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let report = bridge
            .purge_profile(&rusty_crew_core_bridge_api::ProfileId::new(profile_id))
            .map_err(to_napi_error)?;
        serialize_json(&report, "profile purge report")
    }

    #[napi]
    pub fn upsert_model_endpoint_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<ModelEndpointWrite>(&write_json, "model endpoint write")?;
        let record = bridge
            .upsert_model_endpoint(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "model endpoint record")
    }

    #[napi]
    pub fn list_model_endpoints_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ModelEndpointQuery>(&query_json, "model endpoint query")?;
        let records = bridge.list_model_endpoints(&query).map_err(to_napi_error)?;
        serialize_json(&records, "model endpoint records")
    }

    #[napi]
    pub fn get_model_endpoint_json(&self, endpoint_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_model_endpoint(&endpoint_id)
            .map_err(to_napi_error)?;
        serialize_json(&record, "model endpoint record")
    }

    #[napi]
    pub fn delete_model_endpoint_json(&self, delete_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let delete = parse_json::<ModelEndpointDelete>(&delete_json, "model endpoint delete")?;
        let record = bridge
            .delete_model_endpoint(&delete)
            .map_err(to_napi_error)?;
        serialize_json(&record, "deleted model endpoint record")
    }

    #[napi]
    pub fn upsert_model_configuration_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write =
            parse_json::<ModelConfigurationWrite>(&write_json, "model configuration write")?;
        let record = bridge
            .upsert_model_configuration(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "model configuration record")
    }

    #[napi]
    pub fn list_model_configurations_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<ModelConfigurationQuery>(&query_json, "model configuration query")?;
        let records = bridge
            .list_model_configurations(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "model configuration records")
    }

    #[napi]
    pub fn get_model_configuration_json(&self, model_config_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_model_configuration(&model_config_id)
            .map_err(to_napi_error)?;
        serialize_json(&record, "model configuration record")
    }

    #[napi]
    pub fn delete_model_configuration_json(&self, delete_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let delete =
            parse_json::<ModelConfigurationDelete>(&delete_json, "model configuration delete")?;
        let record = bridge
            .delete_model_configuration(&delete)
            .map_err(to_napi_error)?;
        serialize_json(&record, "deleted model configuration record")
    }

    #[napi]
    pub fn upsert_model_provider_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<ModelProviderWrite>(&write_json, "model provider write")?;
        let record = bridge
            .upsert_model_provider(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "model provider record")
    }

    #[napi]
    pub fn list_model_providers_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ModelProviderQuery>(&query_json, "model provider query")?;
        let records = bridge.list_model_providers(&query).map_err(to_napi_error)?;
        serialize_json(&records, "model provider records")
    }

    #[napi]
    pub fn get_model_provider_json(&self, alias: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge.get_model_provider(&alias).map_err(to_napi_error)?;
        serialize_json(&record, "model provider record")
    }

    #[napi]
    pub fn get_model_provider_secret_json(&self, alias: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let secret = bridge
            .get_model_provider_secret(&alias)
            .map_err(to_napi_error)?;
        serialize_json(&secret, "model provider secret")
    }

    #[napi]
    pub fn upsert_service_credential_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<ServiceCredentialWrite>(&write_json, "service credential write")?;
        let record = bridge
            .upsert_service_credential(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "service credential record")
    }

    #[napi]
    pub fn list_service_credentials_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ServiceCredentialQuery>(&query_json, "service credential query")?;
        let records = bridge
            .list_service_credentials(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "service credential records")
    }

    #[napi]
    pub fn get_service_credential_json(&self, credential_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_service_credential(&credential_id)
            .map_err(to_napi_error)?;
        serialize_json(&record, "service credential record")
    }

    #[napi]
    pub fn get_service_credential_secret_json(
        &self,
        credential_id: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let secret = bridge
            .get_service_credential_secret(&credential_id)
            .map_err(to_napi_error)?;
        serialize_json(&secret, "service credential secret")
    }

    #[napi]
    pub fn delete_service_credential_json(&self, delete_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let delete =
            parse_json::<ServiceCredentialDelete>(&delete_json, "service credential delete")?;
        let record = bridge
            .delete_service_credential(&delete)
            .map_err(to_napi_error)?;
        serialize_json(&record, "service credential record")
    }

    #[napi]
    pub fn link_model_provider_credential_json(&self, link_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let link = parse_json::<ModelProviderCredentialLink>(
            &link_json,
            "model provider credential link",
        )?;
        let result = bridge
            .link_model_provider_credential(&link)
            .map_err(to_napi_error)?;
        serialize_json(&result, "model provider credential link result")
    }

    #[napi]
    pub fn unlink_model_provider_credential_json(
        &self,
        unlink_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let unlink = parse_json::<ModelProviderCredentialUnlink>(
            &unlink_json,
            "model provider credential unlink",
        )?;
        let provider = bridge
            .unlink_model_provider_credential(&unlink)
            .map_err(to_napi_error)?;
        serialize_json(&provider, "model provider record")
    }

    #[napi]
    pub fn put_install_diplomat_binding_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<InstallDiplomatBindingWrite>(
            &input_json,
            "install diplomat binding write",
        )?;
        serialize_json(
            &self
                .bridge()?
                .put_install_diplomat_binding(input)
                .map_err(to_napi_error)?,
            "install diplomat binding record",
        )
    }

    #[napi]
    pub fn rebind_install_diplomat_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<InstallDiplomatRebindRequest>(
            &input_json,
            "install diplomat rebind request",
        )?;
        serialize_json(
            &self
                .bridge()?
                .rebind_install_diplomat(input)
                .map_err(to_napi_error)?,
            "install diplomat binding record",
        )
    }

    #[napi]
    pub fn set_install_diplomat_binding_status_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<InstallDiplomatBindingStatusUpdate>(
            &input_json,
            "install diplomat binding status update",
        )?;
        serialize_json(
            &self
                .bridge()?
                .set_install_diplomat_binding_status(input)
                .map_err(to_napi_error)?,
            "install diplomat binding record",
        )
    }

    #[napi]
    pub fn get_install_diplomat_binding_json(&self, binding_id: String) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .get_install_diplomat_binding(&binding_id)
                .map_err(to_napi_error)?,
            "install diplomat binding record",
        )
    }

    #[napi]
    pub fn list_install_diplomat_bindings_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<InstallDiplomatBindingQuery>(
            &input_json,
            "install diplomat binding query",
        )?;
        serialize_json(
            &self
                .bridge()?
                .list_install_diplomat_bindings(&input)
                .map_err(to_napi_error)?,
            "install diplomat binding records",
        )
    }

    #[napi]
    pub fn plan_telegram_diplomat_ingress_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<TelegramDiplomatIngressRequest>(
            &input_json,
            "telegram diplomat ingress request",
        )?;
        serialize_json(
            &self
                .bridge()?
                .plan_telegram_diplomat_ingress(input)
                .map_err(to_napi_error)?,
            "telegram diplomat ingress plan",
        )
    }

    #[napi]
    pub fn model_provider_refresh_impact_json(&self, request_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<ModelProviderRefreshImpactRequest>(
            &request_json,
            "model provider refresh impact request",
        )?;
        let impact = bridge
            .model_provider_refresh_impact(&request)
            .map_err(to_napi_error)?;
        serialize_json(&impact, "model provider refresh impact")
    }

    #[napi]
    pub fn plan_model_provider_refresh_json(&self, request_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<ModelProviderRefreshPlanRequest>(
            &request_json,
            "model provider refresh plan request",
        )?;
        let plan = bridge
            .plan_model_provider_refresh(&request)
            .map_err(to_napi_error)?;
        serialize_json(&plan, "model provider refresh plan")
    }
}
