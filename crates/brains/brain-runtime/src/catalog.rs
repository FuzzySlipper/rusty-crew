use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

pub const BRAIN_CATALOG_REVISION: u32 = 2;
pub const CHAT_COMPLETIONS_BRAIN_ID: &str = "chat-completions";
pub const OPENAI_RESPONSES_BRAIN_ID: &str = "openai-responses";

const PREVIOUS_RESPONSE_FALLBACK_REASONS: &[&str] = &[
    "no_predecessor_state",
    "request_fingerprint_mismatch",
    "profile_fingerprint_mismatch",
    "provider_fingerprint_mismatch",
    "predecessor_rejected_by_provider",
    "provider_state_expired",
    "provider_state_load_failed",
    "input_not_append_only",
    "normal_invalidation",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainProviderProtocol {
    ChatCompletions,
    Responses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainProviderStateMode {
    Unused,
    Optional,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainProviderStateRebuildAction {
    Discard,
    Migrate,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrainProviderStateRebuildPolicy {
    pub action: BrainProviderStateRebuildAction,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrainProviderStatePolicy {
    pub mode: BrainProviderStateMode,
    pub rebuild: BrainProviderStateRebuildPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainHostCapability {
    ExecuteTool,
    ProjectDebugReference,
    ProjectEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainStrategyDiagnostics {
    pub selected_strategy_id: String,
    pub effective_strategy_id: String,
    pub replay_fallback_used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_reason_catalog: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainCatalogStrategy {
    pub strategy_id: String,
    pub provider_state: BrainProviderStatePolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_fingerprint_options: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_fingerprint_options: Option<Value>,
    pub diagnostics: BrainStrategyDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainCatalogModule {
    pub module_id: String,
    pub display_name: String,
    pub provider_protocols: Vec<BrainProviderProtocol>,
    pub default_strategy_id: String,
    pub strategies: Vec<BrainCatalogStrategy>,
    pub required_host_capabilities: Vec<BrainHostCapability>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainCatalog {
    pub revision: u32,
    pub modules: Vec<BrainCatalogModule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrainSelectionRequest {
    #[serde(default)]
    pub configured_module_id: Option<String>,
    #[serde(default)]
    pub configured_strategy_id: Option<String>,
    pub provider_protocol: BrainProviderProtocol,
    pub provider_kind: String,
    #[serde(default)]
    pub roleplay_narrator_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainSelectionPlan {
    pub catalog_revision: u32,
    pub module_id: String,
    pub selected_strategy_id: String,
    pub effective_strategy_id: String,
    pub provider_state_policy: BrainProviderStatePolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_fingerprint_options: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_fingerprint_options: Option<Value>,
    pub strategy_diagnostics: BrainStrategyDiagnostics,
    pub required_host_capabilities: Vec<BrainHostCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrainCatalogError {
    UnsupportedModule {
        module_id: String,
    },
    ProtocolMismatch {
        module_id: String,
        protocol: BrainProviderProtocol,
    },
    UnsupportedStrategy {
        module_id: String,
        strategy_id: String,
    },
    NarratorRequiresChatCompletions,
    NarratorStrategyConflict {
        strategy_id: String,
    },
}

impl fmt::Display for BrainCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedModule { module_id } => {
                write!(formatter, "unknown production brain module {module_id}")
            }
            Self::ProtocolMismatch {
                module_id,
                protocol,
            } => write!(
                formatter,
                "brain module {module_id} does not support provider protocol {protocol:?}"
            ),
            Self::UnsupportedStrategy {
                module_id,
                strategy_id,
            } => write!(
                formatter,
                "unknown strategy {strategy_id} for brain module {module_id}"
            ),
            Self::NarratorRequiresChatCompletions => {
                write!(
                    formatter,
                    "roleplay narrator requires the chat-completions brain"
                )
            }
            Self::NarratorStrategyConflict { strategy_id } => write!(
                formatter,
                "roleplay narrator requires roleplay_narrator strategy, not {strategy_id}"
            ),
        }
    }
}

impl std::error::Error for BrainCatalogError {}

pub fn brain_catalog() -> BrainCatalog {
    BrainCatalog {
        revision: BRAIN_CATALOG_REVISION,
        modules: vec![chat_completions_module(), openai_responses_module()],
    }
}

pub fn plan_brain_selection(
    request: &BrainSelectionRequest,
) -> Result<BrainSelectionPlan, BrainCatalogError> {
    let catalog = brain_catalog();
    let module_id = canonical_module_id(
        request.configured_module_id.as_deref(),
        request.provider_protocol,
    )?;
    let module = catalog
        .modules
        .iter()
        .find(|module| module.module_id == module_id)
        .expect("canonical module must exist in built-in catalog");
    if !module
        .provider_protocols
        .contains(&request.provider_protocol)
    {
        return Err(BrainCatalogError::ProtocolMismatch {
            module_id,
            protocol: request.provider_protocol,
        });
    }
    if request.roleplay_narrator_enabled && module.module_id != CHAT_COMPLETIONS_BRAIN_ID {
        return Err(BrainCatalogError::NarratorRequiresChatCompletions);
    }
    if request.roleplay_narrator_enabled {
        if let Some(strategy_id) = request.configured_strategy_id.as_deref() {
            if strategy_id != "roleplay_narrator" {
                return Err(BrainCatalogError::NarratorStrategyConflict {
                    strategy_id: strategy_id.to_string(),
                });
            }
        }
    }
    let strategy_id = if request.roleplay_narrator_enabled {
        "roleplay_narrator"
    } else {
        request
            .configured_strategy_id
            .as_deref()
            .unwrap_or(&module.default_strategy_id)
    };
    let strategy = module
        .strategies
        .iter()
        .find(|strategy| strategy.strategy_id == strategy_id)
        .ok_or_else(|| BrainCatalogError::UnsupportedStrategy {
            module_id: module.module_id.clone(),
            strategy_id: strategy_id.to_string(),
        })?;
    Ok(BrainSelectionPlan {
        catalog_revision: catalog.revision,
        module_id: module.module_id.clone(),
        selected_strategy_id: strategy.strategy_id.clone(),
        effective_strategy_id: strategy.diagnostics.effective_strategy_id.clone(),
        provider_state_policy: strategy.provider_state.clone(),
        profile_fingerprint_options: strategy.profile_fingerprint_options.clone(),
        provider_fingerprint_options: strategy.provider_fingerprint_options.clone(),
        strategy_diagnostics: strategy.diagnostics.clone(),
        required_host_capabilities: module.required_host_capabilities.clone(),
    })
}

fn canonical_module_id(
    configured: Option<&str>,
    protocol: BrainProviderProtocol,
) -> Result<String, BrainCatalogError> {
    let default = match protocol {
        BrainProviderProtocol::ChatCompletions => CHAT_COMPLETIONS_BRAIN_ID,
        BrainProviderProtocol::Responses => OPENAI_RESPONSES_BRAIN_ID,
    };
    let Some(configured) = configured else {
        return Ok(default.to_string());
    };
    match configured {
        CHAT_COMPLETIONS_BRAIN_ID | OPENAI_RESPONSES_BRAIN_ID => Ok(configured.to_string()),
        other => Err(BrainCatalogError::UnsupportedModule {
            module_id: other.to_string(),
        }),
    }
}

fn host_capabilities() -> Vec<BrainHostCapability> {
    vec![
        BrainHostCapability::ExecuteTool,
        BrainHostCapability::ProjectDebugReference,
        BrainHostCapability::ProjectEvent,
    ]
}

fn chat_completions_module() -> BrainCatalogModule {
    BrainCatalogModule {
        module_id: CHAT_COMPLETIONS_BRAIN_ID.to_string(),
        display_name: "Chat Completions".to_string(),
        provider_protocols: vec![BrainProviderProtocol::ChatCompletions],
        default_strategy_id: "default".to_string(),
        strategies: vec![
            strategy(
                "default",
                BrainProviderStateMode::Optional,
                "chat completions reasoning history is provider-scoped and is discarded on runtime brain rebuild",
                None,
                BrainStrategyDiagnostics {
                    selected_strategy_id: "default".to_string(),
                    effective_strategy_id: "default".to_string(),
                    replay_fallback_used: false,
                    fallback_reason: None,
                    fallback_reason_catalog: Vec::new(),
                },
            ),
            strategy(
                "roleplay_narrator",
                BrainProviderStateMode::Optional,
                "roleplay narrator chat completions reasoning history is provider-scoped and is discarded on runtime brain rebuild",
                None,
                BrainStrategyDiagnostics {
                    selected_strategy_id: "roleplay_narrator".to_string(),
                    effective_strategy_id: "roleplay_narrator".to_string(),
                    replay_fallback_used: false,
                    fallback_reason: None,
                    fallback_reason_catalog: Vec::new(),
                },
            ),
        ],
        required_host_capabilities: host_capabilities(),
    }
}

fn openai_responses_module() -> BrainCatalogModule {
    BrainCatalogModule {
        module_id: OPENAI_RESPONSES_BRAIN_ID.to_string(),
        display_name: "OpenAI Responses".to_string(),
        provider_protocols: vec![BrainProviderProtocol::Responses],
        default_strategy_id: "replay".to_string(),
        strategies: vec![
            strategy(
                "replay",
                BrainProviderStateMode::Optional,
                "OpenAI Responses wire state is response-chain scoped and is discarded on runtime brain rebuild unless a safe migration is explicitly implemented",
                Some(json!({"strategy": "replay"})),
                BrainStrategyDiagnostics {
                    selected_strategy_id: "replay".to_string(),
                    effective_strategy_id: "replay".to_string(),
                    replay_fallback_used: false,
                    fallback_reason: None,
                    fallback_reason_catalog: Vec::new(),
                },
            ),
            strategy(
                "previous-response-chain",
                BrainProviderStateMode::Optional,
                "OpenAI Responses previous_response_id state is provider-chain scoped and is discarded on runtime brain rebuild unless a safe migration is explicitly implemented",
                Some(json!({"strategy": "previous-response-chain"})),
                BrainStrategyDiagnostics {
                    selected_strategy_id: "previous-response-chain".to_string(),
                    effective_strategy_id: "replay".to_string(),
                    replay_fallback_used: true,
                    fallback_reason: Some("normal_invalidation".to_string()),
                    fallback_reason_catalog: PREVIOUS_RESPONSE_FALLBACK_REASONS
                        .iter()
                        .map(|reason| (*reason).to_string())
                        .collect(),
                },
            ),
        ],
        required_host_capabilities: host_capabilities(),
    }
}

fn strategy(
    strategy_id: &str,
    mode: BrainProviderStateMode,
    rebuild_reason: &str,
    provider_fingerprint_options: Option<Value>,
    diagnostics: BrainStrategyDiagnostics,
) -> BrainCatalogStrategy {
    BrainCatalogStrategy {
        strategy_id: strategy_id.to_string(),
        provider_state: BrainProviderStatePolicy {
            mode,
            rebuild: BrainProviderStateRebuildPolicy {
                action: BrainProviderStateRebuildAction::Discard,
                reason: rebuild_reason.to_string(),
                migration_id: None,
            },
        },
        profile_fingerprint_options: None,
        provider_fingerprint_options,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(protocol: BrainProviderProtocol) -> BrainSelectionRequest {
        BrainSelectionRequest {
            configured_module_id: None,
            configured_strategy_id: None,
            provider_protocol: protocol,
            provider_kind: "test".to_string(),
            roleplay_narrator_enabled: false,
        }
    }

    #[test]
    fn catalog_has_only_canonical_production_brains() {
        let catalog = brain_catalog();
        assert_eq!(catalog.revision, BRAIN_CATALOG_REVISION);
        assert_eq!(
            catalog
                .modules
                .iter()
                .map(|module| module.module_id.as_str())
                .collect::<Vec<_>>(),
            vec![CHAT_COMPLETIONS_BRAIN_ID, OPENAI_RESPONSES_BRAIN_ID]
        );
        assert!(!catalog
            .modules
            .iter()
            .any(|module| module.module_id == "local"));
    }

    #[test]
    fn protocol_selects_canonical_default() {
        assert_eq!(
            plan_brain_selection(&request(BrainProviderProtocol::ChatCompletions))
                .expect("chat plan")
                .module_id,
            CHAT_COMPLETIONS_BRAIN_ID
        );
        assert_eq!(
            plan_brain_selection(&request(BrainProviderProtocol::Responses))
                .expect("responses plan")
                .module_id,
            OPENAI_RESPONSES_BRAIN_ID
        );
    }

    #[test]
    fn retired_and_noncanonical_module_ids_are_rejected() {
        for rejected in [
            "pi-agent",
            "pi-agent-core",
            "rust-pi-agent",
            "chat-completions-core",
            "rust-chat-completions",
            "local",
        ] {
            let mut input = request(BrainProviderProtocol::ChatCompletions);
            input.configured_module_id = Some(rejected.to_string());
            assert!(matches!(
                plan_brain_selection(&input),
                Err(BrainCatalogError::UnsupportedModule { module_id }) if module_id == rejected
            ));
        }
    }

    #[test]
    fn module_protocol_mismatches_fail_closed() {
        let mut input = request(BrainProviderProtocol::Responses);
        input.configured_module_id = Some(CHAT_COMPLETIONS_BRAIN_ID.to_string());
        assert!(matches!(
            plan_brain_selection(&input),
            Err(BrainCatalogError::ProtocolMismatch { .. })
        ));
    }

    #[test]
    fn strategy_metadata_and_narrator_selection_are_rust_owned() {
        let mut responses = request(BrainProviderProtocol::Responses);
        responses.configured_strategy_id = Some("previous-response-chain".to_string());
        let responses = plan_brain_selection(&responses).expect("chain plan");
        assert_eq!(responses.selected_strategy_id, "previous-response-chain");
        assert_eq!(responses.effective_strategy_id, "replay");
        assert!(responses.strategy_diagnostics.replay_fallback_used);
        assert_eq!(
            responses.provider_fingerprint_options,
            Some(json!({"strategy": "previous-response-chain"}))
        );

        let mut narrator = request(BrainProviderProtocol::ChatCompletions);
        narrator.roleplay_narrator_enabled = true;
        let narrator = plan_brain_selection(&narrator).expect("narrator plan");
        assert_eq!(narrator.module_id, CHAT_COMPLETIONS_BRAIN_ID);
        assert_eq!(narrator.selected_strategy_id, "roleplay_narrator");
    }

    #[test]
    fn invalid_strategy_and_narrator_conflicts_are_rejected() {
        let mut invalid = request(BrainProviderProtocol::ChatCompletions);
        invalid.configured_strategy_id = Some("missing".to_string());
        assert!(matches!(
            plan_brain_selection(&invalid),
            Err(BrainCatalogError::UnsupportedStrategy { .. })
        ));

        let mut narrator = request(BrainProviderProtocol::ChatCompletions);
        narrator.roleplay_narrator_enabled = true;
        narrator.configured_strategy_id = Some("default".to_string());
        assert!(matches!(
            plan_brain_selection(&narrator),
            Err(BrainCatalogError::NarratorStrategyConflict { .. })
        ));
    }
}
