use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi(getter)]
    pub fn manifest_version(&self) -> u32 {
        MANIFEST_VERSION
    }

    #[napi(getter)]
    pub fn operation_names(&self) -> Vec<String> {
        OPERATION_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    #[napi(getter)]
    pub fn wire_shape_fingerprint(&self) -> String {
        wire_shape_fingerprint().to_string()
    }
}
