use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn brain_catalog_json(&self) -> napi::Result<String> {
        serde_json::to_string(&brain_catalog())
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_brain_selection_json(&self, input_json: String) -> napi::Result<String> {
        let input: BrainSelectionRequest = serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid brain selection request JSON: {error}"),
            )
        })?;
        let plan = plan_brain_selection(&input)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }
}
