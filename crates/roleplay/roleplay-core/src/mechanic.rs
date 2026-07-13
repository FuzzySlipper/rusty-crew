use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{RoleplayDomainError, RoleplayDomainResult};

pub const ROLEPLAY_MECHANIC_TOOL_PROFILE_ID: &str = "roleplay_mechanic";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayMechanicAutoMonitorConfig {
    pub enabled: bool,
    pub available: bool,
    pub status: RoleplayMechanicAutoMonitorStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayMechanicAutoMonitorStatus {
    InactiveFuture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayMechanicConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_alias: Option<String>,
    pub auto_monitor: RoleplayMechanicAutoMonitorConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayMechanicProfilePlan {
    pub config: RoleplayMechanicConfig,
    pub system_prompt: String,
    pub local_tool_profile_id: String,
}

pub fn plan_mechanic_profile(
    input: JsonValue,
) -> RoleplayDomainResult<RoleplayMechanicProfilePlan> {
    let raw = input.as_object().ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_mechanic_config_invalid",
            "roleplay mechanic config must be an object",
        )
    })?;
    let name = optional_trimmed_string(raw, &["name", "displayName", "display_name"])
        .unwrap_or_else(|| "Mechanic".to_string());
    validate_name(&name)?;

    let provider_alias = optional_trimmed_string(raw, &["providerAlias", "provider_alias"]);
    if let Some(provider_alias) = provider_alias.as_deref() {
        validate_provider_alias(provider_alias)?;
    }

    let auto_monitor = optional_bool(raw, &["autoMonitor", "auto_monitor"])?
        .or_else(|| {
            raw.get("autoMonitor")
                .or_else(|| raw.get("auto_monitor"))
                .and_then(JsonValue::as_object)
                .and_then(|value| value.get("enabled"))
                .and_then(JsonValue::as_bool)
        })
        .unwrap_or(false);
    if auto_monitor {
        return Err(RoleplayDomainError::invalid(
            "roleplay_mechanic_auto_monitor_unavailable",
            "roleplay mechanic auto-monitoring is not implemented; autoMonitor must remain false",
        ));
    }

    let config = RoleplayMechanicConfig {
        name,
        provider_alias,
        auto_monitor: RoleplayMechanicAutoMonitorConfig {
            enabled: false,
            available: false,
            status: RoleplayMechanicAutoMonitorStatus::InactiveFuture,
        },
    };
    let system_prompt = mechanic_system_prompt(&config.name);
    Ok(RoleplayMechanicProfilePlan {
        config,
        system_prompt,
        local_tool_profile_id: ROLEPLAY_MECHANIC_TOOL_PROFILE_ID.to_string(),
    })
}

fn mechanic_system_prompt(name: &str) -> String {
    format!(
        "You are {name}, the mechanic for a collaborative fiction roleplay system. You are a warm, direct writing partner and environmental diagnostician. You do not narrate, roleplay, continue scenes, or claim to introspect a model's hidden intentions.\n\nDiagnose observable conditions: recent roleplay history, persisted scene briefs and state, lore retrieval traces, active layers, narrator configuration, style exemplars, provider patterns, and prior proposal outcomes. Ground claims in those records. Distinguish permanent tensions that should be preserved, scene-level tensions that may resolve, and cheap fake tensions caused by drift.\n\nYou may inspect authoritative state with read-only mechanic tools. Any durable change must be expressed as a proposal for user review. Never directly rewrite narrator configuration, exemplars, lore, retrieval settings, or provider patterns. The user decides what is approved and applied.\n\nUse short readable paragraphs by default. Match the user's energy, be candid about weak evidence, and move from symptom to hypothesis to a testable proposal. If the user only wants to vent or brainstorm, meet them there without pretending a configuration diagnosis has been established."
    )
}

fn optional_trimmed_string(
    raw: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| raw.get(*key))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_bool(
    raw: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
) -> RoleplayDomainResult<Option<bool>> {
    let Some(value) = keys.iter().find_map(|key| raw.get(*key)) else {
        return Ok(None);
    };
    if value.is_object() {
        return Ok(None);
    }
    value.as_bool().map(Some).ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_mechanic_config_invalid",
            "autoMonitor must be false or an object with enabled=false",
        )
    })
}

fn validate_name(value: &str) -> RoleplayDomainResult<()> {
    if value.chars().count() > 120 {
        return Err(RoleplayDomainError::invalid(
            "roleplay_mechanic_name_too_long",
            "mechanic name must be at most 120 characters",
        ));
    }
    if value.contains('\0') {
        return Err(RoleplayDomainError::invalid(
            "roleplay_mechanic_name_invalid",
            "mechanic name must not contain NUL",
        ));
    }
    Ok(())
}

fn validate_provider_alias(value: &str) -> RoleplayDomainResult<()> {
    if value.len() > 80
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        return Err(RoleplayDomainError::invalid(
            "roleplay_mechanic_provider_alias_invalid",
            "providerAlias must contain only letters, numbers, underscore, dot, or hyphen and be at most 80 characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_mechanic_profile_with_explicit_inactive_auto_monitor() {
        let plan = plan_mechanic_profile(serde_json::json!({
            "name": "Maren",
            "providerAlias": "deepseek-flash",
            "autoMonitor": false
        }))
        .expect("mechanic profile plan");

        assert_eq!(plan.config.name, "Maren");
        assert_eq!(
            plan.config.provider_alias.as_deref(),
            Some("deepseek-flash")
        );
        assert!(!plan.config.auto_monitor.enabled);
        assert!(!plan.config.auto_monitor.available);
        assert_eq!(
            plan.config.auto_monitor.status,
            RoleplayMechanicAutoMonitorStatus::InactiveFuture
        );
        assert_eq!(plan.local_tool_profile_id, "roleplay_mechanic");
        assert!(plan.system_prompt.contains("You do not narrate"));
        assert!(plan.system_prompt.contains("proposal for user review"));
    }

    #[test]
    fn rejects_auto_monitor_until_behavior_exists() {
        let error = plan_mechanic_profile(serde_json::json!({
            "autoMonitor": { "enabled": true }
        }))
        .expect_err("auto monitor must be rejected");
        assert_eq!(
            error.reason_code,
            "roleplay_mechanic_auto_monitor_unavailable"
        );
    }

    #[test]
    fn rejects_invalid_provider_alias() {
        let error = plan_mechanic_profile(serde_json::json!({
            "providerAlias": "https://provider.invalid"
        }))
        .expect_err("provider alias must be validated");
        assert_eq!(
            error.reason_code,
            "roleplay_mechanic_provider_alias_invalid"
        );
    }
}
