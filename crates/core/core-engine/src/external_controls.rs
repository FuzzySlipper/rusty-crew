use super::*;
use rusty_crew_core_protocol::{ExternalControlKind, ExternalControlRequest};

pub(super) fn external_thread_command_requires_idle(payload: &serde_json::Value) -> bool {
    matches!(
        external_thread_command_name(payload),
        Some("compact" | "new" | "restart")
    )
}

pub(super) fn validate_external_control_payload(
    request: &ExternalControlRequest,
) -> CoreResult<()> {
    if request.kind != ExternalControlKind::InterruptTurn {
        return Ok(());
    }
    let payload = request.payload.as_object().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "external interrupt payload must be an object",
        )
    })?;
    if !payload.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external interrupt payload must be empty; Rust-owned binding and turn identity determine native interrupt parameters",
        ));
    }
    Ok(())
}

pub(super) fn validate_external_thread_command(payload: &serde_json::Value) -> CoreResult<()> {
    let object = payload.as_object().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "external thread command payload must be an object",
        )
    })?;
    if object
        .keys()
        .any(|key| key != "command" && key != "argument")
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external thread command payload contains unsupported fields",
        ));
    }
    let command = object
        .get("command")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "external thread command requires command",
            )
        })?;
    if !matches!(
        command,
        "help" | "commands" | "status" | "new" | "restart" | "model" | "effort" | "compact"
    ) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external thread command is not recognized",
        ));
    }
    let argument = object.get("argument").filter(|value| !value.is_null());
    if let Some(argument) = argument {
        let argument = argument.as_str().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "external thread command argument must be a string or null",
            )
        })?;
        if argument.trim().is_empty() || argument.len() > 256 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external thread command argument must contain 1 to 256 characters",
            ));
        }
        if !matches!(command, "model" | "effort") {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external thread command does not accept an argument",
            ));
        }
    }
    Ok(())
}

fn external_thread_command_name(payload: &serde_json::Value) -> Option<&str> {
    payload.get("command")?.as_str()
}
