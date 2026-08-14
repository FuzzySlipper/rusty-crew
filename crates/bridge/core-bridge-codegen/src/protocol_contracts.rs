use anyhow::{bail, Context, Result};
use rusty_crew_core_protocol::*;
use schemars::{schema_for, JsonSchema};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, fs, path::Path};

#[allow(dead_code)]
#[derive(JsonSchema)]
struct ProtocolContractRoots {
    engine_handle: EngineHandle,
    session_handle: SessionHandle,
    brain_implementation_handle: BrainImplementationHandle,
    platform_adapter_handle: PlatformAdapterHandle,
    subscription_handle: SubscriptionHandle,
    runtime_buffer_handle: RuntimeBufferHandle,
    agent_id: AgentId,
    agent_instance_id: AgentInstanceId,
    session_id: SessionId,
    profile_id: ProfileId,
    project_id: ProjectId,
    task_id: TaskId,
    run_id: RunId,
    logical_turn_id: LogicalTurnId,
    continuation_id: ContinuationId,
    execution_epoch_id: ExecutionEpochId,
    brain_operation_id: BrainOperationId,
    turn_projection_id: TurnProjectionId,
    adapter_id: AdapterId,
    brain_implementation_id: BrainImplementationId,
    resource_limits: ResourceLimits,
    session_config: SessionConfig,
    session_state: SessionState,
    session_workspace_update: SessionWorkspaceUpdate,
    session_workspace_update_record: SessionWorkspaceUpdateRecord,
    delegated_session_runtime_status: DelegatedSessionRuntimeStatus,
    delegated_resource_cleanup_report: DelegatedResourceCleanupReport,
    event_subscription: EventSubscription,
    external_event: ExternalEvent,
    body_state: BodyState,
    brain_wake_request: BrainWakeRequest,
    brain_wake_accepted: BrainWakeAccepted,
    brain_wake_attention: BrainWakeAttention,
    brain_wake_settlement_request: BrainWakeSettlementRequest,
    brain_event_envelope: BrainEventEnvelope,
    brain_action_batch: BrainActionBatch,
    brain_wake_stream_item: BrainWakeStreamItem,
    brain_wake_failure: BrainWakeFailure,
    brain_strategy_metadata: BrainStrategyMetadata,
    brain_provider_state_scope: BrainProviderStateScope,
    provider_state_compatibility_snapshot: ProviderStateCompatibilitySnapshot,
    provider_state_compatibility_plan: ProviderStateCompatibilityPlan,
    brain_wake_provider_state_output: BrainWakeProviderStateOutput,
    action_batch_receipt: ActionBatchReceipt,
    event_receipt: EventReceipt,
    core_error: CoreError,
    platform_adapter_registration: PlatformAdapterRegistration,
    memory_space_descriptor: MemorySpaceDescriptor,
    memory_proposal_envelope: MemoryProposalEnvelope,
    memory_proposal_record: MemoryProposalRecord,
    memory_proposal_query: MemoryProposalQuery,
    session_activity_digest: SessionActivityDigest,
    session_activity_digest_query: SessionActivityDigestQuery,
    context_compaction_artifact: ContextCompactionArtifact,
    context_compaction_artifact_query: ContextCompactionArtifactQuery,
    model_endpoint_record: ModelEndpointRecord,
    model_endpoint_write: ModelEndpointWrite,
    model_endpoint_query: ModelEndpointQuery,
    model_configuration_record: ModelConfigurationRecord,
    model_configuration_write: ModelConfigurationWrite,
    model_configuration_query: ModelConfigurationQuery,
    model_endpoint_backfill_report: ModelEndpointBackfillReport,
    model_endpoint_parity_report: ModelEndpointParityReport,
    manual_context_compaction_request: ManualContextCompactionRequest,
    manual_context_compaction_response: ManualContextCompactionResponse,
    runtime_activity_begin: RuntimeActivityBegin,
    runtime_activity_progress: RuntimeActivityProgress,
    runtime_activity_finish: RuntimeActivityFinish,
    runtime_activity_wake_settlement: RuntimeActivityWakeSettlement,
    runtime_activity_record: RuntimeActivityRecord,
    runtime_activity_live_evidence: RuntimeActivityLiveEvidence,
    runtime_activity_census_query: RuntimeActivityCensusQuery,
    runtime_activity_census: RuntimeActivityCensus,
    logical_turn_record: LogicalTurnRecord,
    logical_turn_checkpoint: LogicalTurnCheckpoint,
    logical_turn_operation_record: LogicalTurnOperationRecord,
    logical_turn_lifecycle_event: LogicalTurnLifecycleEvent,
    logical_turn_admission: LogicalTurnAdmission,
    logical_turn_claim_request: LogicalTurnClaimRequest,
    logical_turn_continuation_claim: LogicalTurnContinuationClaim,
    logical_turn_yield_request: LogicalTurnYieldRequest,
    logical_turn_yield_receipt: LogicalTurnYieldReceipt,
    logical_turn_attention_resolution_request: LogicalTurnAttentionResolutionRequest,
    logical_turn_attention_resolution_receipt: LogicalTurnAttentionResolutionReceipt,
    logical_turn_cancel_request: LogicalTurnCancelRequest,
    logical_turn_cancellation_receipt: LogicalTurnCancellationReceipt,
    logical_turn_hydration_report: LogicalTurnHydrationReport,
    logical_turn_diagnostic_query: LogicalTurnDiagnosticQuery,
    logical_turn_diagnostic_page: LogicalTurnDiagnosticPage,
    memory_governance_decision_input: MemoryGovernanceDecisionInput,
    memory_governance_decision_record: MemoryGovernanceDecisionRecord,
    agent_directory_entry: AgentDirectoryEntry,
    agent_route_record: AgentRouteRecord,
    agent_route_write: AgentRouteWrite,
    agent_route_delete: AgentRouteDelete,
    agent_route_resolution: AgentRouteResolution,
    agent_message_command: AgentMessageCommand,
    agent_message_delivery_completion: AgentMessageDeliveryCompletion,
    agent_message_reply_command: AgentMessageReplyCommand,
    agent_message_inbox_query: AgentMessageInboxQuery,
    agent_message_inbox_item: AgentMessageInboxItem,
    agent_message_traffic_item: AgentMessageTrafficItem,
    agent_message_delivery_receipt: AgentMessageDeliveryReceipt,
    agent_round_command: AgentRoundCommand,
    agent_round_start_receipt: AgentRoundStartReceipt,
    agent_correlated_round: AgentCorrelatedRound,
    review_submission_request: ReviewSubmissionRequest,
    review_submission_record: ReviewSubmissionRecord,
    review_submission_transition_request: ReviewSubmissionTransitionRequest,
    review_submission_query: ReviewSubmissionQuery,
    install_diplomat_binding_record: InstallDiplomatBindingRecord,
    install_diplomat_binding_write: InstallDiplomatBindingWrite,
    install_diplomat_rebind_request: InstallDiplomatRebindRequest,
    install_diplomat_binding_status_update: InstallDiplomatBindingStatusUpdate,
    install_diplomat_binding_query: InstallDiplomatBindingQuery,
    telegram_diplomat_ingress_request: TelegramDiplomatIngressRequest,
    telegram_diplomat_ingress_plan: TelegramDiplomatIngressPlan,
    telegram_operator_consult_request: TelegramOperatorConsultRequest,
    telegram_operator_consult_record: TelegramOperatorConsultRecord,
    telegram_operator_consult_settlement: TelegramOperatorConsultSettlement,
    telegram_operator_consult_query: TelegramOperatorConsultQuery,
    crew_agent_session_creation_request: CrewAgentSessionCreationRequest,
    crew_agent_session_creation_record: CrewAgentSessionCreationRecord,
    external_runtime_registration: ExternalRuntimeRegistration,
    external_controller_context: ExternalControllerContext,
    external_runtime_handshake_observation: ExternalRuntimeHandshakeObservation,
    external_runtime_handshake_decision: ExternalRuntimeHandshakeDecision,
    external_runtime_certification_record: ExternalRuntimeCertificationRecord,
    external_runtime_certification_request: ExternalRuntimeCertificationRequest,
    external_runtime_certification_invalidation: ExternalRuntimeCertificationInvalidation,
    external_runtime_state_observation: ExternalRuntimeStateObservation,
    external_controller_lease: ExternalControllerLease,
    external_agent_binding: ExternalAgentBinding,
    external_agent_binding_metadata_write: ExternalAgentBindingMetadataWrite,
    external_agent_binding_restore_request: ExternalAgentBindingRestoreRequest,
    external_agent_binding_restore_receipt: ExternalAgentBindingRestoreReceipt,
    external_agent_session_creation_request: ExternalAgentSessionCreationRequest,
    external_agent_session_creation_record: ExternalAgentSessionCreationRecord,
    external_turn_correlation: ExternalTurnCorrelation,
    external_turn_page_cursor: ExternalTurnPageCursor,
    external_turn_page_query: ExternalTurnPageQuery,
    external_turn_page: ExternalTurnPage,
    external_control_request: ExternalControlRequest,
    external_control_receipt: ExternalControlReceipt,
    external_interaction_record: ExternalInteractionRecord,
    external_runtime_event_input: ExternalRuntimeEventInput,
    normalized_external_runtime_event: NormalizedExternalRuntimeEvent,
}

const STRING_BRANDS: &[&str] = &[
    "AdapterId",
    "AgentId",
    "AgentInstanceId",
    "AgentMessageDeliveryId",
    "AgentRouteKey",
    "AgentRoundId",
    "BrainImplementationId",
    "ConversationBranchId",
    "ContinuationId",
    "ExecutionEpochId",
    "BrainOperationId",
    "LogicalTurnId",
    "MemoryRecordShapeId",
    "MemorySpaceId",
    "ProfileId",
    "ProjectId",
    "RunId",
    "SessionId",
    "TaskId",
    "TurnProjectionId",
    "ExternalBindingId",
    "ExternalAgentSessionCreationId",
    "ExternalRuntimeId",
    "ExternalTurnRequestId",
];

const NUMBER_BRANDS: &[&str] = &[
    "BrainImplementationHandle",
    "EngineHandle",
    "PlatformAdapterHandle",
    "RuntimeBufferHandle",
    "SessionHandle",
    "SubscriptionHandle",
];

pub fn protocol_contracts_ts() -> Result<String> {
    let root = protocol_contract_schema_value()?;
    let definitions = root
        .get("$defs")
        .and_then(Value::as_object)
        .context("protocol contract root schema did not contain $defs")?;

    let mut output =
        String::from("// @generated by `npm run codegen:protocol-contracts`; do not edit.\n\n");
    output.push_str("import type { Brand } from \"../brands.js\";\n\n");

    for name in STRING_BRANDS {
        output.push_str(&format!(
            "export type {name} = Brand<string, \"{name}\">;\n\n"
        ));
    }
    for name in NUMBER_BRANDS {
        output.push_str(&format!(
            "export type {name} = Brand<number, \"{name}\">;\n\n"
        ));
    }

    if !definitions.contains_key("BrainProviderStateStrategyMetadata") {
        let schema = serde_json::to_value(schema_for!(BrainProviderStateStrategyMetadata))?;
        let rendered = render_schema(&schema, "$defs.BrainProviderStateStrategyMetadata", false)?;
        output.push_str(&format!(
            "export type BrainProviderStateStrategyMetadata = {rendered};\n\n"
        ));
    }

    for (name, schema) in definitions {
        if STRING_BRANDS.contains(&name.as_str()) {
            continue;
        }
        if NUMBER_BRANDS.contains(&name.as_str()) {
            continue;
        }

        let rendered = render_schema(schema, &format!("$defs.{name}"), false)?;
        output.push_str(&format!("export type {name} = {rendered};\n\n"));
    }

    Ok(output)
}

pub fn protocol_contract_schema_json() -> Result<String> {
    let mut output = serde_json::to_string_pretty(&protocol_contract_schema_value()?)?;
    output.push('\n');
    Ok(output)
}

pub fn check_protocol_contracts(path: &Path) -> Result<()> {
    let expected = protocol_contracts_ts()?;
    let actual = fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read generated protocol contracts {}",
            path.display()
        )
    })?;
    if actual != expected {
        bail!(
            "generated protocol contract drift detected for {}; run `npm run codegen:protocol-contracts`",
            path.display()
        );
    }
    Ok(())
}

pub fn check_protocol_contract_schema(path: &Path) -> Result<()> {
    let expected = protocol_contract_schema_json()?;
    let actual = fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read generated protocol schema {}",
            path.display()
        )
    })?;
    if actual != expected {
        bail!(
            "generated protocol schema drift detected for {}; run `npm run codegen:protocol-contracts`",
            path.display()
        );
    }
    Ok(())
}

fn protocol_contract_schema_value() -> Result<Value> {
    Ok(serde_json::to_value(schema_for!(ProtocolContractRoots))?)
}

fn render_schema(schema: &Value, path: &str, omit_null: bool) -> Result<String> {
    if let Some(accepts_anything) = schema.as_bool() {
        return Ok(if accepts_anything { "unknown" } else { "never" }.to_owned());
    }

    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference
            .strip_prefix("#/$defs/")
            .map(decode_json_pointer)
            .transpose()?
            .context("only local $defs references are supported")
            .with_context(|| format!("unsupported schema reference `{reference}` at {path}"));
    }

    if let Some(value) = schema.get("const") {
        return literal_type(value, path);
    }

    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let variants = values
            .iter()
            .filter(|value| !(omit_null && value.is_null()))
            .map(|value| literal_type(value, path))
            .collect::<Result<Vec<_>>>()?;
        return Ok(join_union(variants));
    }

    for union_key in ["oneOf", "anyOf"] {
        if let Some(values) = schema.get(union_key).and_then(Value::as_array) {
            let variants = values
                .iter()
                .filter(|value| !(omit_null && is_null_schema(value)))
                .map(|value| render_schema(value, path, omit_null))
                .collect::<Result<Vec<_>>>()?;
            return Ok(join_union(variants));
        }
    }

    if let Some(values) = schema.get("allOf").and_then(Value::as_array) {
        let variants = values
            .iter()
            .map(|value| render_schema(value, path, omit_null))
            .collect::<Result<Vec<_>>>()?;
        return Ok(variants.join(" & "));
    }

    match schema.get("type") {
        Some(Value::String(kind)) => render_schema_type(kind, schema, path),
        Some(Value::Array(kinds)) => {
            let variants = kinds
                .iter()
                .filter_map(Value::as_str)
                .filter(|kind| !(omit_null && *kind == "null"))
                .map(|kind| render_schema_type(kind, schema, path))
                .collect::<Result<Vec<_>>>()?;
            Ok(join_union(variants))
        }
        None if schema
            .as_object()
            .is_some_and(|object| object.keys().all(|key| key == "default")) =>
        {
            Ok("unknown".to_owned())
        }
        None if schema.as_object().is_some_and(Map::is_empty) => Ok("unknown".to_owned()),
        _ => bail!("unsupported JSON Schema construct at {path}: {schema}"),
    }
}

fn render_schema_type(kind: &str, schema: &Value, path: &str) -> Result<String> {
    match kind {
        "string" => Ok("string".to_owned()),
        "integer" | "number" => Ok("number".to_owned()),
        "boolean" => Ok("boolean".to_owned()),
        "null" => Ok("null".to_owned()),
        "array" => {
            let item = schema
                .get("items")
                .context("array schema is missing items")?;
            let rendered = render_schema(item, &format!("{path}.items"), false)?;
            Ok(format!("Array<{rendered}>"))
        }
        "object" => render_object(schema, path),
        other => bail!("unsupported JSON Schema type `{other}` at {path}"),
    }
}

fn render_object(schema: &Value, path: &str) -> Result<String> {
    let properties = schema.get("properties").and_then(Value::as_object);
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    if properties.is_none_or(Map::is_empty) {
        return match schema.get("additionalProperties") {
            Some(Value::Bool(false)) | None => Ok("Record<string, never>".to_owned()),
            Some(Value::Bool(true)) => Ok("Record<string, unknown>".to_owned()),
            Some(value) => Ok(format!(
                "Record<string, {}>",
                render_schema(value, &format!("{path}.additionalProperties"), false)?
            )),
        };
    }

    let mut output = String::from("{\n");
    for (wire_name, property) in properties.expect("checked above") {
        let is_required = required.contains(wire_name.as_str());
        let property_type = ergonomic_type_override(path, wire_name).map_or_else(
            || render_schema(property, &format!("{path}.properties.{wire_name}"), false),
            |value| Ok(value.to_owned()),
        )?;
        let property_name = if preserves_rust_wire_names(path) {
            wire_name.to_owned()
        } else {
            snake_to_camel(wire_name)
        };
        let optional = if is_required { "" } else { "?" };
        output.push_str(&format!("  {property_name}{optional}: {property_type};\n"));
    }
    output.push('}');
    Ok(output)
}

fn literal_type(value: &Value, path: &str) -> Result<String> {
    match value {
        Value::String(value) => Ok(serde_json::to_string(value)?),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Null => Ok("null".to_owned()),
        _ => bail!("unsupported literal type at {path}: {value}"),
    }
}

fn is_null_schema(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("null")
        || value.get("const").is_some_and(Value::is_null)
}

fn join_union(mut variants: Vec<String>) -> String {
    variants.dedup();
    match variants.len() {
        0 => "never".to_owned(),
        1 => variants.remove(0),
        _ => variants.join(" | "),
    }
}

fn decode_json_pointer(value: &str) -> Result<String> {
    let decoded = value.replace("~1", "/").replace("~0", "~");
    if decoded.contains('/') {
        bail!("nested JSON pointer definitions are unsupported: {value}");
    }
    Ok(decoded)
}

fn snake_to_camel(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut uppercase_next = false;
    for character in value.chars() {
        if character == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            output.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn preserves_rust_wire_names(path: &str) -> bool {
    let definition = path
        .strip_prefix("$defs.")
        .and_then(|rest| rest.split('.').next())
        .unwrap_or_default();
    definition.starts_with("Memory")
        || definition.starts_with("SessionActivityDigest")
        || definition.starts_with("ContextCompactionArtifact")
}

fn ergonomic_type_override(definition_path: &str, wire_name: &str) -> Option<&'static str> {
    let definition = definition_path
        .strip_prefix("$defs.")
        .and_then(|rest| rest.split('.').next())
        .unwrap_or_default();

    match wire_name {
        "agent_id" | "parent_agent_id" => Some("AgentId"),
        "from" | "to" if definition == "AgentMessage" => Some("AgentId"),
        "instance_id" => Some("AgentInstanceId"),
        "session_id"
        | "parent_session_id"
        | "child_session_id"
        | "delegated_session_id"
        | "target_session_id" => Some("SessionId"),
        "terminal_archived" | "orphaned_archived" | "expired_archived" => Some("Array<SessionId>"),
        "profile_id" => Some("ProfileId"),
        "project_id" => Some("ProjectId"),
        "task_id" | "requested_task_id" => Some("TaskId"),
        "run_id" => Some("RunId"),
        "adapter_id" => Some("AdapterId"),
        "implementation_id" => Some("BrainImplementationId"),
        "branch_id" => Some("ConversationBranchId"),
        "space_id" => Some("MemorySpaceId"),
        "shape_id" => Some("MemoryRecordShapeId"),
        "allowed_capture_spaces" => Some("Array<MemorySpaceId>"),
        "brain" => Some("BrainImplementationHandle"),
        "body_state" | "system_prompt" | "role_assembly" => Some("RuntimeBufferHandle"),
        "handle" if definition == "SessionState" => Some("SessionHandle"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_protocol_contracts_have_no_untyped_escape_hatch() {
        let generated = protocol_contracts_ts().expect("generate protocol contracts");
        assert!(!generated.contains(": any"));
        assert!(generated.contains("export type BrainWakeRequest"));
        assert!(generated.contains("export type MemorySpaceDescriptor"));
        assert!(generated.contains("sessionId: SessionId"));
    }

    #[test]
    fn snake_case_wire_fields_are_projected_as_camel_case() {
        assert_eq!(
            snake_to_camel("provider_state_absence"),
            "providerStateAbsence"
        );
        assert!(preserves_rust_wire_names(
            "$defs.MemorySpaceDescriptor.properties.space_id"
        ));
    }

    #[test]
    fn optional_rust_values_remain_nullable_in_typescript() {
        let generated = protocol_contracts_ts().expect("generate protocol contracts");
        assert!(generated.contains("nativeThreadId?: string | null;"));
        assert!(generated.contains("nativeTurnId?: string | null;"));

        let schema = protocol_contract_schema_value().expect("generate protocol schema");
        let binding = &schema["$defs"]["ExternalAgentBinding"];
        assert!(!binding["required"]
            .as_array()
            .expect("binding required fields")
            .iter()
            .any(|field| field.as_str() == Some("nativeThreadId")));
        assert!(binding["properties"]["nativeThreadId"]["type"]
            .as_array()
            .expect("nativeThreadId type union")
            .iter()
            .any(|kind| kind.as_str() == Some("null")));
    }
}
