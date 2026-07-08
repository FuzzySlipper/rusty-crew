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
