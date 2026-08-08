use super::*;

pub(crate) const PROVIDER_STATE_COMPATIBILITY_PLAN_VERSION: &str = "1";

pub(crate) fn plan_provider_state_compatibility(
    prior: &ProviderStateCompatibilitySnapshot,
    current: &ProviderStateCompatibilitySnapshot,
) -> ProviderStateCompatibilityPlan {
    let mut changes = Vec::new();
    let mut incompatible = false;

    compare(
        &mut changes,
        &mut incompatible,
        "facts_version",
        &prior.facts.version,
        &current.facts.version,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "profile_identity",
        &prior.facts.profile_identity,
        &current.facts.profile_identity,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "display_metadata",
        &prior.facts.display_metadata,
        &current.facts.display_metadata,
        false,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "prompt",
        &prior.facts.prompt,
        &current.facts.prompt,
        false,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "skills",
        &prior.facts.skills,
        &current.facts.skills,
        false,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "tool_catalog",
        &prior.facts.tool_catalog,
        &current.facts.tool_catalog,
        false,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "session_effort",
        &prior.session_effort,
        &current.session_effort,
        false,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "session_workspace",
        &prior.session_workspace,
        &current.session_workspace,
        false,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "provider_endpoint",
        &prior.facts.provider_endpoint,
        &current.facts.provider_endpoint,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "model",
        &prior.facts.model,
        &current.facts.model,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "protocol",
        &prior.facts.protocol,
        &current.facts.protocol,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "dialect",
        &prior.facts.dialect,
        &current.facts.dialect,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "reasoning_semantics",
        &prior.facts.reasoning_semantics,
        &current.facts.reasoning_semantics,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "brain_module",
        &prior.facts.brain_module,
        &current.facts.brain_module,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "brain_strategy",
        &prior.facts.brain_strategy,
        &current.facts.brain_strategy,
        true,
    );
    compare(
        &mut changes,
        &mut incompatible,
        "provider_state_schema",
        &prior.facts.provider_state_schema,
        &current.facts.provider_state_schema,
        true,
    );

    let class = if changes.is_empty() {
        ProviderStateCompatibilityClass::Identical
    } else if incompatible {
        ProviderStateCompatibilityClass::Incompatible
    } else {
        ProviderStateCompatibilityClass::Compatible
    };
    let (action, outcome) = if incompatible {
        (
            ProviderStateCompatibilityAction::ReconstructFromDurableProjection,
            ProviderStateCompatibilityOutcome::ReconstructionRequired,
        )
    } else {
        (
            ProviderStateCompatibilityAction::PreserveLineage,
            ProviderStateCompatibilityOutcome::Preserved,
        )
    };
    ProviderStateCompatibilityPlan {
        version: PROVIDER_STATE_COMPATIBILITY_PLAN_VERSION.to_string(),
        class,
        changes,
        action,
        outcome,
    }
}

fn compare(
    changes: &mut Vec<ProviderStateCompatibilityChange>,
    incompatible: &mut bool,
    dimension: &str,
    prior: &str,
    current: &str,
    is_incompatible: bool,
) {
    if prior == current {
        return;
    }
    changes.push(ProviderStateCompatibilityChange {
        dimension: dimension.to_string(),
        prior_fingerprint: prior.to_string(),
        current_fingerprint: current.to_string(),
    });
    *incompatible |= is_incompatible;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::ProviderStateCompatibilityFacts;

    fn snapshot() -> ProviderStateCompatibilitySnapshot {
        ProviderStateCompatibilitySnapshot {
            facts: ProviderStateCompatibilityFacts {
                version: "1".into(),
                profile_identity: "profile".into(),
                display_metadata: "display".into(),
                prompt: "prompt".into(),
                skills: "skills".into(),
                tool_catalog: "tools".into(),
                provider_endpoint: "endpoint".into(),
                model: "model".into(),
                protocol: "responses".into(),
                dialect: "stateful".into(),
                reasoning_semantics: "reasoning".into(),
                brain_module: "module".into(),
                brain_strategy: "strategy".into(),
                provider_state_schema: "schema".into(),
            },
            session_effort: "medium".into(),
            session_workspace: "/workspace/one".into(),
        }
    }

    #[test]
    fn benign_matrix_preserves_provider_lineage() {
        for protocol in ["responses", "chat_completions"] {
            for dimension in [
                "display_metadata",
                "prompt",
                "skills",
                "tool_catalog",
                "session_effort",
                "session_workspace",
            ] {
                let mut prior = snapshot();
                prior.facts.protocol = protocol.into();
                let mut current = prior.clone();
                match dimension {
                    "display_metadata" => current.facts.display_metadata = "changed".into(),
                    "prompt" => current.facts.prompt = "changed".into(),
                    "skills" => current.facts.skills = "changed".into(),
                    "tool_catalog" => current.facts.tool_catalog = "changed".into(),
                    "session_effort" => current.session_effort = "high".into(),
                    "session_workspace" => current.session_workspace = "/workspace/two".into(),
                    _ => unreachable!(),
                }
                let plan = plan_provider_state_compatibility(&prior, &current);
                assert_eq!(
                    plan.class,
                    ProviderStateCompatibilityClass::Compatible,
                    "{protocol}:{dimension}"
                );
                assert_eq!(
                    plan.action,
                    ProviderStateCompatibilityAction::PreserveLineage,
                    "{protocol}:{dimension}"
                );
            }
        }
    }

    #[test]
    fn incompatible_matrix_requires_durable_reconstruction() {
        for protocol in ["responses", "chat_completions"] {
            for dimension in [
                "profile_identity",
                "provider_endpoint",
                "model",
                "protocol",
                "dialect",
                "reasoning_semantics",
                "brain_module",
                "brain_strategy",
                "provider_state_schema",
            ] {
                let mut prior = snapshot();
                prior.facts.protocol = protocol.into();
                let mut current = prior.clone();
                match dimension {
                    "profile_identity" => current.facts.profile_identity = "changed".into(),
                    "provider_endpoint" => current.facts.provider_endpoint = "changed".into(),
                    "model" => current.facts.model = "changed".into(),
                    "protocol" => {
                        current.facts.protocol = if protocol == "responses" {
                            "chat_completions"
                        } else {
                            "responses"
                        }
                        .into()
                    }
                    "dialect" => current.facts.dialect = "changed".into(),
                    "reasoning_semantics" => current.facts.reasoning_semantics = "changed".into(),
                    "brain_module" => current.facts.brain_module = "changed".into(),
                    "brain_strategy" => current.facts.brain_strategy = "changed".into(),
                    "provider_state_schema" => {
                        current.facts.provider_state_schema = "changed".into()
                    }
                    _ => unreachable!(),
                }
                let plan = plan_provider_state_compatibility(&prior, &current);
                assert_eq!(
                    plan.class,
                    ProviderStateCompatibilityClass::Incompatible,
                    "{protocol}:{dimension}"
                );
                assert_eq!(
                    plan.action,
                    ProviderStateCompatibilityAction::ReconstructFromDurableProjection,
                    "{protocol}:{dimension}"
                );
            }
        }
    }
}
