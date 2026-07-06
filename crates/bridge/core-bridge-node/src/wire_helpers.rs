use rusty_crew_core_bridge_api::{CoreError, RuntimeBufferHandle};
use serde::{de::DeserializeOwned, Serialize};

pub(crate) fn handle_to_u32(handle: RuntimeBufferHandle) -> napi::Result<u32> {
    u32::try_from(handle.get()).map_err(|_| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("runtime buffer handle {} does not fit in u32", handle.get()),
        )
    })
}

pub(crate) fn to_napi_error(error: CoreError) -> napi::Error {
    napi::Error::new(
        napi::Status::GenericFailure,
        format!("{:?}: {}", error.kind, error.message),
    )
}

pub(crate) fn parse_json<T: DeserializeOwned>(raw: &str, label: &str) -> napi::Result<T> {
    serde_json::from_str(raw).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid {label} json: {error}"),
        )
    })
}

pub(crate) fn serialize_json<T: Serialize>(value: &T, label: &str) -> napi::Result<String> {
    serde_json::to_string(value).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize {label}: {error}"),
        )
    })
}
