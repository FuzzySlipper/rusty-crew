#![recursion_limit = "256"]

mod bridge_contracts;
mod protocol_contracts;

use anyhow::{bail, Context, Result};
use rusty_crew_brain_runtime::{self as brain_runtime, BufferedBrainRunDrain};
use rusty_crew_core_bridge_api::*;
use rusty_crew_core_config::{
    self as config, BrainConfigDraft, ChannelBindingConfigDraft, ChannelWakePolicy,
    CreateProfileMcpBindingRequest, CreateProfilePlanInput, CreateProfileRequest,
    CreateProfileSourceRequest, ExternalBindingStatusDraft, McpBindingConfigDraft,
    ProfileBackgroundReviewConfig, ProfileBackgroundReviewType, ProfileBrainMetadata,
    ProfileChannelDefaults, ProfileContextPolicy, ProfileMcpConfig, ProfileModelConfigSeed,
    ProfileRegistryMutationKind, ProfileRegistryMutationMode, ProfileRegistryMutationPlan,
    ProfileRegistryMutationRequest, ProfileRegistryRuntimeMetadata, ProfileRuntimeMetadata,
    ProfileRuntimeOptions, ProfileSessionDefaults, RuntimeConfigDiagnostic,
    RuntimeConfigDiagnosticSeverity, RuntimeConfigDraft, RuntimeConfigPlan,
    RuntimeConfigValidationInput, RuntimeConfigValidationResult, RuntimeGraphDefaultSource,
    RuntimeGraphDerivedKind, RuntimeGraphPlanInput, RuntimeGraphPostgresBootMode,
    RuntimeGraphStorageBackend, RuntimeGraphStorageImplementationStatus, ScheduledJobConfigDraft,
    ScheduledJobShape, SessionConfigDraft,
};
use rusty_crew_core_persistence as persistence;
use rusty_crew_core_protocol::{
    self as protocol, AdapterId, AgentId, AgentInstanceId, AttachmentId, AttachmentLinkId,
    BrainImplementationId, ConversationBranchId, ConversationSnapshotId, DataBankScopeId,
    MessageBlockId, MessageId, MessageSlotId, MessageVariantId, ProfileId,
    ProfileRegistryLifecycleStatus, ResourceLimits, SessionHistoryWindow, SessionId, SessionKind,
};
use rusty_crew_core_tool_registry as tool_registry;
use rusty_crew_roleplay_core as roleplay;
use schemars::{generate::SchemaSettings, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BridgeValidationFixtureFile {
    format_version: u32,
    manifest_version: u32,
    operation_count: usize,
    schema_source: String,
    fixtures: Vec<BridgeValidationFixture>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BridgeValidationFixture {
    name: String,
    operation: String,
    direction: String,
    rust_type: String,
    value: Value,
}

#[derive(Debug, Clone, Serialize)]
struct BrainWakeStreamResultFixture {
    stream: Vec<BrainWakeStreamItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeJsonMethodSignature {
    operation_name: String,
    method_name: String,
    parameter_count: usize,
    return_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeBindingMethodMetadata {
    name: String,
    parameter_source: String,
    parameter_count: usize,
    return_type: String,
    return_kind: String,
    operation_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeBindingPropertyMetadata {
    name: String,
    return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeBindingSurfaceArtifact {
    format_version: u32,
    source: String,
    manifest_operation_count: usize,
    methods: Vec<NativeBindingMethodMetadata>,
    properties: Vec<NativeBindingPropertyMetadata>,
    direct_helper_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeWireSchemaArtifact {
    format_version: u32,
    source: String,
    schemas: BTreeMap<String, Value>,
    operation_schema_keys: BTreeMap<String, String>,
    sample_outputs: BTreeMap<String, Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CoreConfigFacadeArtifact {
    format_version: u32,
    source_crate: String,
    generated_module: String,
    wire_field_inventory: BTreeMap<String, Vec<String>>,
    enum_value_inventory: BTreeMap<String, Vec<String>>,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("summary") => {
            let operation_count = manifest_operation_count()?;
            println!("core bridge codegen scaffold: found {operation_count} manifest operations");
        }
        Some("emit-fixtures") => {
            let fixtures = bridge_validation_fixture_file()?;
            println!("{}", serde_json::to_string_pretty(&fixtures)?);
        }
        Some("check-fixtures") => {
            let path = args
                .next()
                .context("check-fixtures requires a fixture JSON path")?;
            check_fixtures(Path::new(&path))?;
            println!("bridge validation Rust fixture drift check passed");
        }
        Some("emit-fingerprint") => {
            println!("{}", bridge_wire_shape_fingerprint()?);
        }
        Some("check-fingerprint") => {
            let fingerprint_path = args
                .next()
                .context("check-fingerprint requires a fingerprint file path")?;
            let contracts_path = args
                .next()
                .context("check-fingerprint requires a TypeScript contracts index path")?;
            check_fingerprint(Path::new(&fingerprint_path), Path::new(&contracts_path))?;
            println!("bridge wire-shape fingerprint drift check passed");
        }
        Some("check-contracts") => {
            let path = args
                .next()
                .context("check-contracts requires a TypeScript contracts index path")?;
            check_contracts(Path::new(&path))?;
            println!("bridge contract operation parity check passed");
        }
        Some("check-native-surface") => {
            let path = args
                .next()
                .context("check-native-surface requires a native index.d.ts path")?;
            let ts_binding_path = args.next();
            check_native_surface(Path::new(&path), ts_binding_path.as_deref().map(Path::new))?;
            println!("bridge native surface inventory check passed");
        }
        Some("emit-native-binding-surface") => {
            let path = args
                .next()
                .context("emit-native-binding-surface requires a native index.d.ts path")?;
            print!("{}", native_binding_surface_ts(Path::new(&path))?);
        }
        Some("check-native-binding-surface") => {
            let native_path = args
                .next()
                .context("check-native-binding-surface requires a native index.d.ts path")?;
            let generated_path = args.next().context(
                "check-native-binding-surface requires a generated TypeScript artifact path",
            )?;
            check_native_binding_surface(Path::new(&native_path), Path::new(&generated_path))?;
            println!("native binding generated signature surface drift check passed");
        }
        Some("emit-bridge-wire-schemas") => {
            print!("{}", bridge_wire_schemas_ts()?);
        }
        Some("check-bridge-wire-schemas") => {
            let path = args
                .next()
                .context("check-bridge-wire-schemas requires a generated TypeScript path")?;
            check_bridge_wire_schemas(Path::new(&path))?;
            println!("Rust-derived bridge wire schema drift check passed");
        }
        Some("emit-core-config-facade") => {
            println!("{}", core_config_facade_ts()?);
        }
        Some("check-core-config-facade") => {
            let path = args
                .next()
                .context("check-core-config-facade requires a generated TypeScript path")?;
            check_core_config_facade(Path::new(&path))?;
            println!("core-config facade generated artifact drift check passed");
        }
        Some("emit-native-mapping-inventory") => {
            println!("{}", native_mapping_inventory_ts()?);
        }
        Some("check-native-mapping-inventory") => {
            let path = args
                .next()
                .context("check-native-mapping-inventory requires a generated TypeScript path")?;
            check_native_mapping_inventory(Path::new(&path))?;
            println!("native bridge mapping inventory generated artifact drift check passed");
        }
        Some("emit-protocol-contracts") => {
            print!("{}", protocol_contracts::protocol_contracts_ts()?);
        }
        Some("check-protocol-contracts") => {
            let path = args
                .next()
                .context("check-protocol-contracts requires a generated TypeScript path")?;
            protocol_contracts::check_protocol_contracts(Path::new(&path))?;
            println!("core protocol generated artifact drift check passed");
        }
        Some("emit-protocol-schema") => {
            print!("{}", protocol_contracts::protocol_contract_schema_json()?);
        }
        Some("check-protocol-schema") => {
            let path = args
                .next()
                .context("check-protocol-schema requires a generated JSON Schema path")?;
            protocol_contracts::check_protocol_contract_schema(Path::new(&path))?;
            println!("core protocol generated schema drift check passed");
        }
        Some("emit-bridge-contracts") => {
            print!("{}", bridge_contracts::bridge_contracts_ts()?);
        }
        Some("check-bridge-contracts") => {
            let path = args
                .next()
                .context("check-bridge-contracts requires a generated TypeScript path")?;
            bridge_contracts::check_bridge_contracts(Path::new(&path))?;
            println!("bridge manifest generated artifact drift check passed");
        }
        Some("--help" | "-h") => {
            print_help();
        }
        Some(other) => {
            bail!("unknown core-bridge-codegen command `{other}`; run with --help");
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "\
rusty-crew-core-bridge-codegen

Commands:
  summary                         Print manifest operation count.
  emit-fixtures                   Emit Rust-authored bridge validation fixtures as JSON.
  check-fixtures <path>           Compare <path> with freshly emitted fixtures.
  emit-fingerprint                Emit SHA-256 wire-shape fingerprint for fixture-backed bridge shapes.
  check-fingerprint <path> <ts>   Compare <path> and TypeScript export with fresh fingerprint.
  check-contracts <path>          Check manifest/Rust/TS operation inventory parity.
  check-native-surface <path> [ts] Check generated napi *Json methods have manifest entries
                                  and optionally compare the TS raw binding interface.
  emit-native-binding-surface <path>
                                  Emit the generated raw binding type and signature inventory.
  check-native-binding-surface <native> <generated>
                                  Compare generated raw binding signatures with napi declarations.
  emit-bridge-wire-schemas        Emit Rust-derived active bridge output JSON Schemas.
  check-bridge-wire-schemas <path>
                                  Compare <path> with Rust-derived bridge output schemas.
  emit-core-config-facade         Emit the generated TypeScript core-config facade helper.
  check-core-config-facade <path> Compare <path> with the generated core-config facade helper.
  emit-native-mapping-inventory   Emit generated TS inventory for bridge mapper drift checks.
  check-native-mapping-inventory <path>
                                  Compare <path> with the generated mapper inventory.
  emit-protocol-contracts         Emit generated TypeScript neutral protocol contracts.
  check-protocol-contracts <path> Compare <path> with generated protocol contracts.
  emit-protocol-schema            Emit the selected neutral protocol JSON Schema.
  check-protocol-schema <path>    Compare <path> with the generated JSON Schema.
  emit-bridge-contracts           Emit generated bridge manifest TypeScript metadata.
  check-bridge-contracts <path>   Compare <path> with generated bridge metadata.

The fixtures are an incremental drift-check scaffold. They do not replace the
bridge manifest operation inventory; they give TS validation smokes a Rust
serialization source for covered bridge families while full codegen matures."
    );
}

fn manifest_operation_count() -> Result<usize> {
    Ok(operation_names_from_manifest(MANIFEST_TEXT)?.len())
}

fn check_fixtures(path: &Path) -> Result<()> {
    let expected = bridge_validation_fixture_file()?;
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture file {}", path.display()))?;
    let actual: BridgeValidationFixtureFile = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse fixture file {}", path.display()))?;
    if actual != expected {
        bail!(
            "bridge validation fixture drift detected for {}; run `cargo run -p rusty-crew-core-bridge-codegen -- emit-fixtures > {}`",
            path.display(),
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn bridge_wire_shape_fingerprint() -> Result<String> {
    let payload = json!({
        "format": "rusty-crew-bridge-wire-shape-fingerprint-v1",
        "manifest_operation_names": operation_names_from_manifest(MANIFEST_TEXT)?,
        "fixtures": bridge_validation_fixture_file()?,
    });
    let bytes = serde_json::to_vec(&payload)?;
    let digest = Sha256::digest(bytes);
    Ok(hex_digest(&digest))
}

fn check_fingerprint(fingerprint_path: &Path, contracts_index_path: &Path) -> Result<()> {
    let expected = bridge_wire_shape_fingerprint()?;
    let file_value = fs::read_to_string(fingerprint_path)
        .with_context(|| {
            format!(
                "failed to read bridge wire-shape fingerprint file {}",
                fingerprint_path.display()
            )
        })?
        .trim()
        .to_owned();
    if file_value != expected {
        bail!(
            "bridge wire-shape fingerprint drift detected for {}; expected {}; run `npm run codegen:bridge-fingerprint` and update the TypeScript bridgeWireShapeFingerprint export",
            fingerprint_path.display(),
            expected
        );
    }

    bridge_contracts::check_bridge_contracts(contracts_index_path)?;
    Ok(())
}

fn check_contracts(contracts_index_path: &Path) -> Result<()> {
    let manifest_operation_names = operation_names_from_manifest(MANIFEST_TEXT)?;
    let rust_operation_names = OPERATION_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    compare_operation_order(
        "bridge-manifest.toml [[operation]] names",
        &manifest_operation_names,
        "core_bridge_api::OPERATION_NAMES",
        &rust_operation_names,
    )?;
    bridge_contracts::check_bridge_contracts(contracts_index_path)?;

    Ok(())
}

fn check_native_surface(native_index_path: &Path, ts_binding_path: Option<&Path>) -> Result<()> {
    let manifest_operation_names = operation_names_from_manifest(MANIFEST_TEXT)?;
    let native_source = fs::read_to_string(native_index_path).with_context(|| {
        format!(
            "failed to read native bridge declaration file {}",
            native_index_path.display()
        )
    })?;
    let native_json_operations = operation_names_from_native_json_methods(&native_source)?;

    ensure_operations_cover_native_json_methods(
        "bridge-manifest.toml [[operation]] names",
        &manifest_operation_names,
        "generated napi NativeBridgeBinding *Json methods",
        &native_json_operations,
    )?;

    if let Some(ts_binding_path) = ts_binding_path {
        let ts_source = fs::read_to_string(ts_binding_path).with_context(|| {
            format!(
                "failed to read TypeScript native bridge source {}",
                ts_binding_path.display()
            )
        })?;
        let native_signatures = native_json_method_signatures(
            &native_source,
            "export declare class NativeBridgeBinding {",
            "generated napi NativeBridgeBinding declaration",
        )?;
        let ts_signatures = native_json_method_signatures(
            &ts_source,
            "interface NativeBridgeBinding {",
            "TypeScript NativeBridgeBinding raw interface",
        )?;
        compare_native_json_method_signatures(
            &native_signatures,
            &ts_signatures,
            native_index_path,
            ts_binding_path,
        )?;
    }

    Ok(())
}

fn check_native_binding_surface(native_path: &Path, generated_path: &Path) -> Result<()> {
    let expected = native_binding_surface_ts(native_path)?;
    let actual = fs::read_to_string(generated_path).with_context(|| {
        format!(
            "failed to read generated native binding surface {}",
            generated_path.display()
        )
    })?;
    if actual != expected {
        bail!(
            "generated native binding surface drift detected for {}; run `npm run codegen:native-binding-surface`",
            generated_path.display()
        );
    }
    Ok(())
}

fn native_binding_surface_ts(native_path: &Path) -> Result<String> {
    let source = fs::read_to_string(native_path).with_context(|| {
        format!(
            "failed to read native bridge declaration file {}",
            native_path.display()
        )
    })?;
    let artifact = native_binding_surface_artifact(&source)?;
    let artifact_json = serde_json::to_string_pretty(&artifact)?;
    let declarations = native_binding_typescript_declarations(&source)?;
    Ok(format!(
        r#"// @generated by `npm run codegen:native-binding-surface`; do not edit manually.

{declarations}

export const nativeBridgeBindingSurface = {artifact_json} as const;
"#
    ))
}

fn native_binding_typescript_declarations(source: &str) -> Result<String> {
    let marker = "export declare class NativeBridgeBinding {";
    let start = source
        .find(marker)
        .context("failed to find napi NativeBridgeBinding declaration")?
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find("\n}")
        .context("failed to find end of napi NativeBridgeBinding declaration")?;
    let body = &rest[..end];
    let support_types = &rest[end + 2..];
    let mut output = String::from("export interface NativeBridgeBinding {\n");
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "constructor()" {
            continue;
        }
        if let Some(getter) = trimmed.strip_prefix("get ") {
            let (name, return_type) = getter
                .split_once("():")
                .with_context(|| format!("invalid native binding getter `{trimmed}`"))?;
            output.push_str(&format!(
                "  readonly {}: {}\n",
                name.trim(),
                sanitize_native_typescript(return_type.trim())
            ));
            continue;
        }
        output.push_str("  ");
        output.push_str(&sanitize_native_typescript(trimmed));
        output.push('\n');
    }
    output.push_str("}\n\n");
    output.push_str(sanitize_native_typescript(support_types).trim());
    output.push('\n');
    Ok(output)
}

fn sanitize_native_typescript(source: &str) -> String {
    source
        .replace(": Buffer", ": Uint8Array")
        .replace("(delete:", "(input:")
}

fn native_binding_surface_artifact(source: &str) -> Result<NativeBindingSurfaceArtifact> {
    let manifest_operations = operation_names_from_manifest(MANIFEST_TEXT)?;
    let manifest = manifest_operations
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let block = native_bridge_binding_block(
        source,
        "export declare class NativeBridgeBinding {",
        "generated napi NativeBridgeBinding declaration",
    )?;
    let mut methods = Vec::new();
    let mut properties = Vec::new();

    for line in block.lines() {
        let declaration = line.trim();
        if declaration.is_empty() || declaration == "constructor()" {
            continue;
        }
        if let Some(property) = declaration.strip_prefix("get ") {
            let (name, return_type) = property
                .split_once("():")
                .with_context(|| format!("invalid native binding getter `{declaration}`"))?;
            properties.push(NativeBindingPropertyMetadata {
                name: name.trim().to_owned(),
                return_type: normalize_signature_source(return_type),
            });
            continue;
        }

        let open_paren = declaration
            .find('(')
            .with_context(|| format!("invalid native binding declaration `{declaration}`"))?;
        let close_params = declaration.rfind("):").with_context(|| {
            format!("native binding method is missing return type `{declaration}`")
        })?;
        let name = declaration[..open_paren].trim().to_owned();
        let parameter_source =
            normalize_signature_source(&declaration[open_paren + 1..close_params]);
        let return_type = normalize_signature_source(&declaration[close_params + 2..]);
        let candidate_operation = camel_method_to_operation_name(&name)?;
        let operation_name = manifest
            .contains(candidate_operation.as_str())
            .then_some(candidate_operation);
        methods.push(NativeBindingMethodMetadata {
            name,
            parameter_count: count_signature_parameters(&parameter_source),
            parameter_source,
            return_kind: return_kind(&return_type),
            return_type,
            operation_name,
        });
    }

    let names = methods
        .iter()
        .map(|method| method.name.clone())
        .collect::<Vec<_>>();
    ensure_no_duplicate_operations("native binding method declarations", &names)?;
    let direct_helper_names = methods
        .iter()
        .filter(|method| method.operation_name.is_none())
        .map(|method| method.name.clone())
        .collect();

    Ok(NativeBindingSurfaceArtifact {
        format_version: 1,
        source: "napi-rs NativeBridgeBinding declaration plus bridge manifest".to_owned(),
        manifest_operation_count: manifest_operations.len(),
        methods,
        properties,
        direct_helper_names,
    })
}

fn check_bridge_wire_schemas(path: &Path) -> Result<()> {
    let expected = bridge_wire_schemas_ts()?;
    let actual = fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read generated bridge wire schemas {}",
            path.display()
        )
    })?;
    if actual != expected {
        bail!(
            "generated bridge wire schema drift detected for {}; run `npm run codegen:bridge-wire-schemas`",
            path.display()
        );
    }
    Ok(())
}

fn bridge_wire_schemas_ts() -> Result<String> {
    let artifact = bridge_wire_schema_artifact()?;
    let artifact_json = serde_json::to_string_pretty(&artifact)?;
    Ok(format!(
        r#"// @generated by `npm run codegen:bridge-wire-schemas`; do not edit manually.

import type {{ TSchema }} from "typebox";

export const bridgeWireSchemaArtifact = {artifact_json} as const;

export type GeneratedBridgeOutputOperation = keyof typeof bridgeWireSchemaArtifact.operationSchemaKeys;

export const generatedBridgeOutputSchemas = Object.fromEntries(
  Object.entries(bridgeWireSchemaArtifact.operationSchemaKeys).map(
    ([operation, schemaKey]) => [operation, bridgeWireSchemaArtifact.schemas[schemaKey]],
  ),
) as unknown as {{
  readonly [Operation in GeneratedBridgeOutputOperation]: TSchema;
}};
"#
    ))
}

fn bridge_wire_schema_artifact() -> Result<BridgeWireSchemaArtifact> {
    let mut schemas = BTreeMap::new();
    let mut operation_schema_keys = BTreeMap::new();
    macro_rules! schema {
        ($operation:literal, $type:ty) => {
            insert_bridge_output_schema::<$type>(
                &mut schemas,
                &mut operation_schema_keys,
                $operation,
            )?;
        };
    }

    schema!("brain_catalog", brain_runtime::BrainCatalog);
    schema!("begin_runtime_activity", protocol::RuntimeActivityRecord);
    schema!("progress_runtime_activity", protocol::RuntimeActivityRecord);
    schema!("finish_runtime_activity", protocol::RuntimeActivityRecord);
    schema!(
        "settle_runtime_activity_wake",
        Vec<protocol::RuntimeActivityRecord>
    );
    schema!("runtime_activity_census", protocol::RuntimeActivityCensus);
    schema!("begin_review_submission", protocol::ReviewSubmissionRecord);
    schema!(
        "transition_review_submission",
        protocol::ReviewSubmissionRecord
    );
    schema!(
        "list_review_submissions",
        Vec<protocol::ReviewSubmissionRecord>
    );
    schema!(
        "put_install_diplomat_binding",
        protocol::InstallDiplomatBindingRecord
    );
    schema!(
        "rebind_install_diplomat",
        protocol::InstallDiplomatBindingRecord
    );
    schema!(
        "set_install_diplomat_binding_status",
        protocol::InstallDiplomatBindingRecord
    );
    schema!(
        "get_install_diplomat_binding",
        Option<protocol::InstallDiplomatBindingRecord>
    );
    schema!(
        "list_install_diplomat_bindings",
        Vec<protocol::InstallDiplomatBindingRecord>
    );
    schema!(
        "plan_telegram_diplomat_ingress",
        protocol::TelegramDiplomatIngressPlan
    );
    schema!("plan_brain_selection", brain_runtime::BrainSelectionPlan);
    schema!("list_agent_directory", Vec<protocol::AgentDirectoryEntry>);
    schema!(
        "list_agent_route_resolutions",
        Vec<protocol::AgentRouteResolution>
    );
    schema!(
        "get_agent_route_resolution",
        Option<protocol::AgentRouteResolution>
    );
    schema!("resolve_agent_address", protocol::AgentRouteResolution);
    schema!("put_agent_route", protocol::AgentRouteRecord);
    schema!("delete_agent_route", protocol::AgentRouteRecord);
    schema!(
        "deliver_agent_message",
        protocol::AgentMessageDeliveryReceipt
    );
    schema!("reply_agent_message", protocol::AgentMessageDeliveryReceipt);
    schema!(
        "list_agent_message_inbox",
        Vec<protocol::AgentMessageInboxItem>
    );
    schema!(
        "list_agent_message_traffic",
        Vec<protocol::AgentMessageTrafficItem>
    );
    schema!("begin_agent_round", protocol::AgentRoundStartReceipt);
    schema!("get_agent_round", Option<protocol::AgentCorrelatedRound>);
    schema!(
        "get_agent_message_delivery",
        Option<protocol::AgentMessageDeliveryReceipt>
    );
    schema!(
        "complete_agent_message_delivery",
        protocol::AgentMessageDeliveryReceipt
    );
    schema!(
        "register_external_runtime",
        protocol::ExternalRuntimeRegistration
    );
    schema!(
        "authorize_external_runtime_handshake",
        protocol::ExternalRuntimeHandshakeDecision
    );
    schema!(
        "record_external_runtime_state",
        protocol::ExternalRuntimeRegistration
    );
    schema!(
        "certify_external_runtime",
        protocol::ExternalRuntimeCertificationRecord
    );
    schema!(
        "invalidate_external_runtime_certification",
        protocol::ExternalRuntimeCertificationRecord
    );
    schema!(
        "list_external_runtime_certifications",
        Vec<protocol::ExternalRuntimeCertificationRecord>
    );
    schema!(
        "get_external_runtime_certification",
        Option<protocol::ExternalRuntimeCertificationRecord>
    );
    schema!(
        "list_external_runtimes",
        Vec<protocol::ExternalRuntimeRegistration>
    );
    schema!(
        "get_external_runtime",
        Option<protocol::ExternalRuntimeRegistration>
    );
    schema!(
        "acquire_external_controller",
        protocol::ExternalControllerLease
    );
    schema!(
        "release_external_controller",
        protocol::ExternalControllerLease
    );
    schema!("bind_external_agent", protocol::ExternalAgentBinding);
    schema!(
        "create_crew_agent_session",
        protocol::CrewAgentSessionCreationRecord
    );
    schema!(
        "update_session_workspace",
        protocol::SessionWorkspaceUpdateRecord
    );
    schema!(
        "restore_external_agent_binding",
        protocol::ExternalAgentBindingRestoreReceipt
    );
    schema!(
        "list_external_bindings",
        Vec<protocol::ExternalAgentBinding>
    );
    schema!(
        "update_external_binding_metadata",
        protocol::ExternalAgentBinding
    );
    schema!(
        "get_external_binding",
        Option<protocol::ExternalAgentBinding>
    );
    schema!(
        "prepare_external_agent_session_creation",
        protocol::ExternalAgentSessionCreationRecord
    );
    schema!(
        "mark_external_agent_session_native_starting",
        protocol::ExternalAgentSessionCreationRecord
    );
    schema!(
        "complete_external_agent_session_creation",
        protocol::ExternalAgentSessionCreationRecord
    );
    schema!(
        "record_external_agent_session_creation_failure",
        protocol::ExternalAgentSessionCreationRecord
    );
    schema!(
        "get_external_turn",
        Option<protocol::ExternalTurnCorrelation>
    );
    schema!("query_external_turn_page", protocol::ExternalTurnPage);
    schema!(
        "list_active_external_turns",
        Vec<protocol::ExternalTurnCorrelation>
    );
    schema!(
        "expire_external_turn_dispatches",
        Vec<protocol::ExternalTurnCorrelation>
    );
    schema!(
        "transition_external_turn",
        protocol::ExternalTurnCorrelation
    );
    schema!("submit_external_control", protocol::ExternalControlReceipt);
    schema!(
        "complete_external_control",
        protocol::ExternalControlReceipt
    );
    schema!(
        "record_external_interaction",
        protocol::ExternalInteractionRecord
    );
    schema!(
        "resolve_external_interaction",
        protocol::ExternalInteractionRecord
    );
    schema!(
        "list_pending_external_interactions",
        Vec<protocol::ExternalInteractionRecord>
    );
    schema!(
        "terminalize_external_interaction",
        protocol::ExternalInteractionRecord
    );
    schema!(
        "record_external_runtime_event",
        protocol::NormalizedExternalRuntimeEvent
    );
    schema!(
        "query_external_runtime_events",
        Vec<protocol::NormalizedExternalRuntimeEvent>
    );

    schema!(
        "plan_tool_availability",
        tool_registry::ToolAvailabilityPlan
    );
    schema!(
        "plan_local_code_resource_policy",
        tool_registry::LocalCodeResourcePolicyPlan
    );
    schema!(
        "plan_web_browser_resource_policy",
        tool_registry::WebBrowserResourcePolicyPlan
    );
    schema!(
        "validate_runtime_config_draft",
        config::RuntimeConfigValidationResult
    );
    schema!("plan_runtime_config", config::RuntimeConfigPlan);
    schema!("plan_runtime_graph", config::RuntimeGraphPlan);
    schema!("plan_create_profile", config::CreateProfilePlan);
    schema!(
        "plan_profile_registry_mutation",
        config::ProfileRegistryMutationPlan
    );
    schema!("plan_new_session_control", config::NewSessionControlPlan);
    schema!("plan_reload_mcp_control", config::ReloadMcpControlPlan);
    schema!(
        "plan_delegated_role_lifecycle",
        config::DelegatedRoleLifecyclePlan
    );

    schema!("register_scheduled_wake_job", ScheduledJobWireOutput);
    schema!("register_scheduled_host_job", ScheduledJobWireOutput);
    schema!("list_scheduled_jobs", Vec<ScheduledJobWireOutput>);
    schema!("list_scheduled_runs", Vec<ScheduledRunWireOutput>);
    schema!("claim_scheduled_host_runs", Vec<ScheduledRunWireOutput>);
    schema!(
        "request_scheduled_host_job_run",
        Option<ScheduledRunWireOutput>
    );
    schema!("run_scheduler_tick", SchedulerTickWireOutput);
    schema!("request_scheduled_job_run", Option<ScheduledRunWireOutput>);

    schema!(
        "query_session_memory_records",
        Vec<persistence::SessionMemoryRecord>
    );
    schema!(
        "build_session_memory_prompt_context",
        persistence::SessionMemoryPromptContext
    );
    schema!("save_memory_proposal", protocol::MemoryProposalRecord);
    schema!(
        "plan_capture_memory_proposals",
        protocol::CaptureMemoryProposalPlan
    );
    schema!(
        "plan_curator_governance_transition",
        protocol::CuratorGovernancePlan
    );
    schema!(
        "plan_curator_lifecycle_transition",
        protocol::CuratorLifecyclePlan
    );
    schema!(
        "plan_background_memory_auto_mutations",
        protocol::BackgroundMemoryAutoMutationPlan
    );
    schema!(
        "manual_context_compaction",
        protocol::ManualContextCompactionResponse
    );

    schema!("save_message_variant", persistence::MessageVariantRecord);
    schema!(
        "create_chat_message_slot",
        persistence::CreateChatMessageSlotResult
    );
    schema!(
        "create_chat_message_variant",
        persistence::CreateChatMessageVariantResult
    );
    schema!(
        "apply_roleplay_alternative",
        persistence::ApplyRoleplayAlternativeResult
    );
    schema!("chat_read_model_page", persistence::ChatReadModelPage);
    schema!("read_chat_session", persistence::ChatSessionReadResult);
    schema!(
        "query_chat_session_summaries",
        persistence::ChatSessionSummaryPage
    );
    schema!("append_chat_event", persistence::ChatEventLogEvent);
    schema!("query_chat_events", persistence::ChatEventLogPage);
    schema!("query_message_slots", Vec<persistence::MessageSlotRecord>);
    schema!(
        "query_message_slots_page",
        persistence::ExactPage<persistence::MessageSlotRecord>
    );
    schema!(
        "query_message_variants",
        Vec<persistence::MessageVariantRecord>
    );
    schema!(
        "query_message_variants_page",
        persistence::ExactPage<persistence::MessageVariantRecord>
    );
    schema!(
        "select_active_message_variant",
        persistence::SelectActiveVariantResult
    );
    schema!(
        "select_active_chat_message_variant",
        persistence::SelectActiveChatMessageVariantResult
    );
    schema!(
        "delete_chat_message_variant",
        persistence::MessageSlotRecord
    );
    schema!(
        "reorder_chat_message_variants",
        Vec<persistence::MessageVariantRecord>
    );
    schema!("delete_message_variant", persistence::MessageSlotRecord);
    schema!(
        "reorder_message_variants",
        Vec<persistence::MessageVariantRecord>
    );
    schema!(
        "save_conversation_branch",
        persistence::ConversationBranchRecord
    );
    schema!(
        "create_chat_conversation_branch",
        persistence::ConversationBranchRecord
    );
    schema!(
        "ensure_active_chat_conversation_branch",
        persistence::EnsureActiveChatConversationBranchResult
    );
    schema!(
        "query_conversation_branches",
        Vec<persistence::ConversationBranchRecord>
    );
    schema!(
        "get_conversation_branch_state",
        persistence::ConversationBranchStateRecord
    );
    schema!(
        "select_active_conversation_branch",
        persistence::SelectActiveBranchResult
    );
    schema!(
        "update_conversation_branch_head",
        persistence::UpdateBranchHeadResult
    );
    schema!(
        "save_conversation_snapshot",
        persistence::ConversationSnapshotRecord
    );
    schema!(
        "create_chat_conversation_snapshot",
        persistence::CreateChatConversationSnapshotResult
    );
    schema!(
        "query_conversation_snapshots",
        Vec<persistence::ConversationSnapshotRecord>
    );
    schema!(
        "read_conversation_tree",
        persistence::ConversationTreeReadResult
    );
    schema!(
        "search_chat_transcript",
        persistence::ChatTranscriptSearchPage
    );
    schema!(
        "resolve_conversation_jump",
        persistence::ConversationJumpResult
    );
    schema!("save_attachment", persistence::AttachmentRecord);
    schema!(
        "create_chat_attachment",
        persistence::CreateChatAttachmentResult
    );
    schema!("query_attachments", Vec<persistence::AttachmentRecord>);
    schema!(
        "query_attachments_page",
        persistence::ExactPage<persistence::AttachmentRecord>
    );
    schema!("remove_attachment", persistence::AttachmentRecord);
    schema!("remove_chat_attachment", persistence::AttachmentRecord);
    schema!("save_data_bank_scope", persistence::DataBankScopeRecord);
    schema!(
        "create_chat_data_bank_scope",
        persistence::CreateChatDataBankScopeResult
    );
    schema!(
        "query_data_bank_scopes",
        Vec<persistence::DataBankScopeRecord>
    );
    schema!(
        "query_data_bank_scopes_page",
        persistence::ExactPage<persistence::DataBankScopeRecord>
    );
    schema!("remove_data_bank_scope", persistence::DataBankScopeRecord);
    schema!(
        "remove_chat_data_bank_scope",
        persistence::DataBankScopeRecord
    );

    schema!(
        "put_roleplay_character",
        persistence::RoleplayCharacterRecord
    );
    schema!(
        "get_roleplay_character",
        Option<persistence::RoleplayCharacterRecord>
    );
    schema!(
        "list_roleplay_characters",
        Vec<persistence::RoleplayCharacterRecord>
    );
    schema!(
        "put_roleplay_player_persona",
        persistence::RoleplayPlayerPersonaRecord
    );
    schema!(
        "get_roleplay_player_persona",
        Option<persistence::RoleplayPlayerPersonaRecord>
    );
    schema!(
        "list_roleplay_player_personas",
        Vec<persistence::RoleplayPlayerPersonaRecord>
    );
    schema!(
        "put_roleplay_session_metadata",
        persistence::RoleplaySessionMetadataRecord
    );
    schema!(
        "get_roleplay_session_metadata",
        Option<persistence::RoleplaySessionMetadataRecord>
    );
    schema!(
        "list_roleplay_session_metadata",
        Vec<persistence::RoleplaySessionMetadataRecord>
    );
    schema!(
        "apply_roleplay_session_projection",
        persistence::RoleplaySessionProjectionRecord
    );
    schema!("put_roleplay_import", persistence::RoleplayImportRecord);
    schema!(
        "get_roleplay_import",
        Option<persistence::RoleplayImportRecord>
    );
    schema!(
        "list_roleplay_imports",
        Vec<persistence::RoleplayImportRecord>
    );
    schema!("create_lore_layer", persistence::RoleplayLoreLayerRecord);
    schema!(
        "get_lore_layer",
        Option<persistence::RoleplayLoreLayerRecord>
    );
    schema!(
        "list_lore_layers",
        Vec<persistence::RoleplayLoreLayerRecord>
    );
    schema!("update_lore_layer", persistence::RoleplayLoreLayerRecord);
    schema!("archive_lore_layer", persistence::RoleplayLoreLayerRecord);
    schema!("get_chat_layers", Vec<persistence::RoleplayChatLayerRecord>);
    schema!("add_lore_entry", persistence::RoleplayLoreRecord);
    schema!("replace_lore_entry", persistence::RoleplayLoreRecord);
    schema!(
        "supersede_lore_entry",
        (
            persistence::RoleplayLoreRecord,
            persistence::RoleplayLoreRecord
        )
    );
    schema!("tombstone_lore_entry", persistence::RoleplayLoreRecord);
    schema!("query_lore_entries", Vec<persistence::RoleplayLoreRecord>);
    schema!("get_lore_entry", Option<persistence::RoleplayLoreRecord>);
    schema!(
        "lore_entry_provenance_events",
        Vec<persistence::RoleplayLoreProvenanceEvent>
    );
    schema!(
        "list_entries_by_layer",
        Vec<persistence::RoleplayLoreLayerEntryJoin>
    );
    schema!("recall_lore", persistence::LoreRecallResult);
    schema!("capture_lore_fact", persistence::RoleplayLoreLayerEntryJoin);
    schema!(
        "promote_lore_entry",
        persistence::RoleplayLoreLayerEntryJoin
    );
    schema!(
        "get_lore_layer_config",
        Option<persistence::RoleplayLoreLayerConfigRecord>
    );
    schema!(
        "set_lore_layer_config",
        persistence::RoleplayLoreLayerConfigRecord
    );
    schema!(
        "list_recall_traces",
        Vec<persistence::LoreRecallTraceRecord>
    );
    schema!(
        "get_recall_trace",
        Option<persistence::LoreRecallTraceRecord>
    );
    schema!(
        "create_roleplay_mechanic_proposal",
        persistence::RoleplayMechanicProposalRecord
    );
    schema!(
        "get_roleplay_mechanic_proposal",
        Option<persistence::RoleplayMechanicProposalRecord>
    );
    schema!(
        "list_roleplay_mechanic_proposals",
        Vec<persistence::RoleplayMechanicProposalRecord>
    );
    schema!(
        "decide_roleplay_mechanic_proposal",
        persistence::RoleplayMechanicProposalRecord
    );
    schema!(
        "apply_roleplay_mechanic_proposal",
        persistence::RoleplayMechanicProposalRecord
    );
    schema!(
        "create_roleplay_mechanic_session_association",
        persistence::RoleplayMechanicSessionAssociationRecord
    );
    schema!(
        "get_roleplay_mechanic_session_association",
        Option<persistence::RoleplayMechanicSessionAssociationRecord>
    );
    schema!(
        "list_roleplay_mechanic_session_associations",
        Vec<persistence::RoleplayMechanicSessionAssociationRecord>
    );
    schema!(
        "update_roleplay_mechanic_session_attachment",
        persistence::RoleplayMechanicSessionAssociationRecord
    );
    schema!(
        "create_roleplay_mechanic_diagnostic",
        persistence::RoleplayMechanicDiagnosticRecord
    );
    schema!(
        "get_roleplay_mechanic_diagnostic",
        Option<persistence::RoleplayMechanicDiagnosticRecord>
    );
    schema!(
        "list_roleplay_mechanic_diagnostics",
        Vec<persistence::RoleplayMechanicDiagnosticRecord>
    );
    schema!(
        "update_roleplay_mechanic_diagnostic_outcome",
        persistence::RoleplayMechanicDiagnosticRecord
    );

    schema!(
        "plan_roleplay_assistant_alternative",
        roleplay::RoleplayAssistantAlternativePlan
    );
    schema!(
        "plan_roleplay_session_lifecycle",
        roleplay::RoleplaySessionLifecyclePlan
    );
    schema!(
        "plan_roleplay_chat_layer_binding",
        roleplay::RoleplayChatLayerBindingPlan
    );
    schema!(
        "normalize_roleplay_lore_search_controls",
        roleplay::RoleplayLoreSearchControls
    );
    schema!(
        "read_roleplay_scene_state",
        roleplay::RoleplaySceneStateReadOutput
    );
    schema!(
        "plan_roleplay_scene_state_update",
        roleplay::RoleplaySceneStateUpdatePlan
    );
    schema!(
        "build_roleplay_prompt_context",
        roleplay::RoleplayPromptContextOutput
    );
    schema!(
        "roleplay_speaker_identity",
        roleplay::RoleplaySpeakerIdentitySnapshot
    );
    schema!("write_roleplay_character", roleplay::RoleplayCharacter);
    schema!("merge_roleplay_character", roleplay::RoleplayCharacter);
    schema!(
        "write_roleplay_player_persona",
        roleplay::RoleplayPlayerPersona
    );
    schema!(
        "merge_roleplay_player_persona",
        roleplay::RoleplayPlayerPersona
    );
    schema!(
        "patch_roleplay_session_metadata",
        roleplay::RoleplaySessionMetadataPatchOutput
    );
    schema!(
        "normalize_roleplay_narrator_config",
        roleplay::RoleplayNarratorConfig
    );
    schema!(
        "plan_roleplay_mechanic_profile",
        roleplay::RoleplayMechanicProfilePlan
    );
    schema!(
        "start_roleplay_narrator_turn",
        roleplay::RoleplayNarratorTurnReceipt
    );
    schema!(
        "advance_roleplay_narrator_turn",
        roleplay::RoleplayNarratorTurnReceipt
    );

    let manifest = operation_names_from_manifest(MANIFEST_TEXT)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let missing = operation_schema_keys
        .keys()
        .filter(|operation| !manifest.contains(*operation))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!("bridge wire schemas reference operations missing from manifest: {missing:?}");
    }

    let mut sample_outputs = BTreeMap::new();
    macro_rules! samples {
        ($operation:literal, [$($value:expr),+ $(,)?]) => {
            sample_outputs.insert(
                $operation.to_owned(),
                vec![$(serde_json::to_value($value)?),+],
            );
        };
    }
    samples!(
        "create_chat_message_slot",
        [sample_create_chat_message_slot_result()]
    );
    samples!("chat_read_model_page", [sample_chat_read_model_page()]);
    samples!(
        "resolve_conversation_jump",
        [sample_conversation_jump_result()]
    );
    samples!("query_attachments", [vec![sample_attachment_record()]]);
    samples!("save_data_bank_scope", [sample_data_bank_scope_record()]);
    samples!(
        "get_lore_layer",
        [
            Some(sample_roleplay_lore_layer_record()),
            None::<persistence::RoleplayLoreLayerRecord>,
        ]
    );
    samples!("recall_lore", [sample_lore_recall_result()]);
    samples!(
        "plan_roleplay_assistant_alternative",
        [sample_roleplay_assistant_alternative_plan()]
    );
    samples!(
        "start_roleplay_narrator_turn",
        [sample_roleplay_narrator_turn_receipt()]
    );

    Ok(BridgeWireSchemaArtifact {
        format_version: 1,
        source: "schemars schemas derived from Rust bridge, brain, config, tool, protocol, persistence, and roleplay DTOs".to_owned(),
        schemas,
        operation_schema_keys,
        sample_outputs,
    })
}

fn insert_bridge_output_schema<T: JsonSchema>(
    schemas: &mut BTreeMap<String, Value>,
    operation_schema_keys: &mut BTreeMap<String, String>,
    operation: &str,
) -> Result<()> {
    let schema_key = std::any::type_name::<T>().to_owned();
    if !schemas.contains_key(&schema_key) {
        schemas.insert(schema_key.clone(), inline_json_schema::<T>()?);
    }
    if operation_schema_keys
        .insert(operation.to_owned(), schema_key)
        .is_some()
    {
        bail!("duplicate generated bridge output schema operation `{operation}`");
    }
    Ok(())
}

fn inline_json_schema<T: JsonSchema>() -> Result<Value> {
    let settings = SchemaSettings::draft07().with(|settings| {
        settings.meta_schema = None;
        settings.inline_subschemas = true;
    });
    serde_json::to_value(settings.into_generator().into_root_schema_for::<T>())
        .context("failed to serialize generated bridge wire schema")
}

fn check_core_config_facade(path: &Path) -> Result<()> {
    let expected = format!("{}\n", core_config_facade_ts()?);
    let actual = fs::read_to_string(path)
        .with_context(|| format!("failed to read core-config facade {}", path.display()))?;
    if actual != expected {
        bail!(
            "core-config facade drift detected for {}; run `npm run codegen:core-config-facade`",
            path.display()
        );
    }
    Ok(())
}

fn core_config_facade_ts() -> Result<String> {
    let artifact = core_config_facade_artifact()?;
    let artifact_json = serde_json::to_string_pretty(&artifact)?;
    Ok(format!(
        r#"// @generated by `cargo run -p rusty-crew-core-bridge-codegen -- emit-core-config-facade` -- do not edit manually.

export const coreConfigFacadeArtifact = {artifact_json} as const;

export type CoreConfigFacadeFamily = keyof typeof coreConfigFacadeArtifact.wire_field_inventory;

export function toCoreConfigWireRuntimeConfigValidationInput(
  input: unknown,
): unknown {{
  return toSnakeCaseKeys(input);
}}

export function toCoreConfigWireCreateProfilePlanInput(input: unknown): unknown {{
  return toSnakeCaseKeys(input);
}}

export function toCoreConfigWireRuntimeGraphPlanInput(input: unknown): unknown {{
  return toSnakeCaseKeys(input);
}}

export function fromCoreConfigWireRuntimeGraphPlan(input: unknown): unknown {{
  return toCamelCaseKeys(input);
}}

function toSnakeCaseKeys(value: unknown): unknown {{
  if (Array.isArray(value)) {{
    return value.map(toSnakeCaseKeys);
  }}
  if (!isPlainObject(value)) {{
    return value;
  }}
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      camelToSnakeCase(key),
      isOpaqueJsonKey(key) ? item : toSnakeCaseKeys(item),
    ]),
  );
}}

function isPlainObject(value: unknown): value is Record<string, unknown> {{
  return (
    typeof value === "object" &&
    value !== null &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}}

function camelToSnakeCase(value: string): string {{
  return value.replace(/[A-Z]/g, (letter) => `_${{letter.toLowerCase()}}`);
}}

function isOpaqueJsonKey(value: string): boolean {{
  return value === "payload" || value === "strategyConfig" || value === "strategy_config";
}}

function toCamelCaseKeys(value: unknown): unknown {{
  if (Array.isArray(value)) {{
    return value.map(toCamelCaseKeys);
  }}
  if (!isPlainObject(value)) {{
    return value;
  }}
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, item]) => item !== null)
      .map(([key, item]) => [
        snakeToCamelCase(key),
        isOpaqueJsonKey(key) ? item : toCamelCaseKeys(item),
      ]),
  );
}}

function snakeToCamelCase(value: string): string {{
  return value.replace(/_([a-z])/g, (_match, letter: string) =>
    letter.toUpperCase(),
  );
}}
"#
    ))
}

fn core_config_facade_artifact() -> Result<CoreConfigFacadeArtifact> {
    let mut wire_field_inventory = BTreeMap::new();
    wire_field_inventory.insert(
        "RuntimeConfigValidationInput".to_owned(),
        json_field_paths(&serde_json::to_value(
            sample_runtime_config_validation_input(),
        )?),
    );
    wire_field_inventory.insert(
        "CreateProfilePlanInput".to_owned(),
        json_field_paths(&serde_json::to_value(sample_create_profile_plan_input())?),
    );
    let runtime_graph_input = sample_runtime_graph_plan_input()?;
    wire_field_inventory.insert(
        "RuntimeGraphPlanInput".to_owned(),
        json_field_paths(&serde_json::to_value(&runtime_graph_input)?),
    );
    wire_field_inventory.insert(
        "RuntimeGraphPlan".to_owned(),
        json_field_paths(&serde_json::to_value(
            rusty_crew_core_config::plan_runtime_graph(&runtime_graph_input),
        )?),
    );

    let mut enum_value_inventory = BTreeMap::new();
    enum_value_inventory.insert(
        "RuntimeGraphStorageBackend".to_owned(),
        serialized_enum_values(&[
            RuntimeGraphStorageBackend::Sqlite,
            RuntimeGraphStorageBackend::Postgres,
        ])?,
    );
    enum_value_inventory.insert(
        "RuntimeGraphPostgresBootMode".to_owned(),
        serialized_enum_values(&[
            RuntimeGraphPostgresBootMode::Blocked,
            RuntimeGraphPostgresBootMode::ProofAdmin,
            RuntimeGraphPostgresBootMode::Active,
        ])?,
    );
    enum_value_inventory.insert(
        "RuntimeGraphDerivedKind".to_owned(),
        serialized_enum_values(&[
            RuntimeGraphDerivedKind::ScheduledJob,
            RuntimeGraphDerivedKind::McpBinding,
        ])?,
    );
    enum_value_inventory.insert(
        "RuntimeGraphDefaultSource".to_owned(),
        serialized_enum_values(&[
            RuntimeGraphDefaultSource::CanonicalProfileDefault,
            RuntimeGraphDefaultSource::ServiceDefault,
            RuntimeGraphDefaultSource::ProfileRuntimeDefault,
            RuntimeGraphDefaultSource::ProfileSessionDefault,
        ])?,
    );
    enum_value_inventory.insert(
        "RuntimeGraphStorageImplementationStatus".to_owned(),
        serialized_enum_values(&[
            RuntimeGraphStorageImplementationStatus::Active,
            RuntimeGraphStorageImplementationStatus::ProofAdminOnly,
            RuntimeGraphStorageImplementationStatus::BlockedUnimplemented,
        ])?,
    );

    Ok(CoreConfigFacadeArtifact {
        format_version: 2,
        source_crate: "rusty-crew-core-config".to_owned(),
        generated_module: "ts/packages/native-bridge/src/generated/core-config-facade.ts"
            .to_owned(),
        wire_field_inventory,
        enum_value_inventory,
    })
}

fn serialized_enum_values<T: Serialize>(values: &[T]) -> Result<Vec<String>> {
    values
        .iter()
        .map(|value| {
            serde_json::to_value(value)?
                .as_str()
                .map(ToOwned::to_owned)
                .context("expected string-serialized core-config enum")
        })
        .collect()
}

fn sample_runtime_graph_plan_input() -> Result<RuntimeGraphPlanInput> {
    let source = include_str!(concat!(
        "../../../../fixtures/runtime-config-parity/target/complete-source.camel.json"
    ))
    .replace("__FIXTURE_ROOT__", "/tmp/rusty-crew-runtime-graph-fixture");
    let camel_value: Value = serde_json::from_str(&source)?;
    Ok(serde_json::from_value(to_snake_case_json_keys(
        camel_value,
    ))?)
}

fn to_snake_case_json_keys(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(to_snake_case_json_keys).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    (
                        key.chars().fold(String::new(), |mut output, character| {
                            if character.is_ascii_uppercase() {
                                output.push('_');
                                output.push(character.to_ascii_lowercase());
                            } else {
                                output.push(character);
                            }
                            output
                        }),
                        if key == "payload" || key == "strategyConfig" {
                            value
                        } else {
                            to_snake_case_json_keys(value)
                        },
                    )
                })
                .collect(),
        ),
        value => value,
    }
}

fn check_native_mapping_inventory(path: &Path) -> Result<()> {
    let expected = format!("{}\n", native_mapping_inventory_ts()?);
    let actual = fs::read_to_string(path)
        .with_context(|| format!("failed to read native mapping inventory {}", path.display()))?;
    if actual != expected {
        bail!(
            "native bridge mapping inventory drift detected for {}; run `npm run codegen:native-mapping-inventory`",
            path.display()
        );
    }
    Ok(())
}

fn native_mapping_inventory_ts() -> Result<String> {
    let artifact = native_mapping_inventory_artifact()?;
    let artifact_json = serde_json::to_string_pretty(&artifact)?;
    Ok(format!(
        r#"// @generated by `cargo run -p rusty-crew-core-bridge-codegen -- emit-native-mapping-inventory` -- do not edit manually.

export const nativeMappingInventory = {artifact_json} as const;
"#
    ))
}

fn native_mapping_inventory_artifact() -> Result<Value> {
    let memory_operations = vec![
        "list_memory_space_descriptors",
        "query_session_memory_records",
        "build_session_memory_prompt_context",
        "save_memory_proposal",
        "plan_capture_memory_proposals",
        "plan_curator_governance_transition",
        "plan_curator_lifecycle_transition",
        "plan_background_memory_auto_mutations",
        "list_memory_proposals",
        "save_session_activity_digest",
        "list_session_activity_digests",
        "save_context_compaction_artifact",
        "list_context_compaction_artifacts",
        "manual_context_compaction",
        "record_memory_governance_decision",
    ];
    let memory_json_input_operations = vec![
        "query_session_memory_records",
        "build_session_memory_prompt_context",
        "save_memory_proposal",
        "plan_capture_memory_proposals",
        "plan_curator_governance_transition",
        "plan_curator_lifecycle_transition",
        "plan_background_memory_auto_mutations",
        "list_memory_proposals",
        "save_session_activity_digest",
        "list_session_activity_digests",
        "save_context_compaction_artifact",
        "list_context_compaction_artifacts",
        "manual_context_compaction",
        "record_memory_governance_decision",
    ];
    let profile_memory_direct_methods = vec![
        "listProfileMemory",
        "getProfileMemory",
        "addProfileMemory",
        "replaceProfileMemory",
        "removeProfileMemory",
    ];
    let brain_provider_operations = vec![
        "wake_brain",
        "submit_brain_event",
        "submit_brain_actions",
        "apply_brain_provider_state_output",
        "start_brain_run",
        "drain_brain_run",
        "submit_brain_host_result",
        "cancel_brain_run",
        "buffered_brain_run_diagnostics",
        "cleanup_buffered_brain_runs",
        "provider_state_diagnostics",
    ];
    let brain_provider_raw_methods = vec![
        "applyBrainProviderStateOutputJson",
        "startBrainRunJson",
        "drainBrainRunJson",
        "submitBrainHostResultJson",
        "cancelBrainRunJson",
        "providerStateDiagnostics",
        "bufferedBrainRunDiagnosticsJson",
        "cleanupBufferedBrainRunsJson",
        "submitBrainActionsJson",
    ];
    let brain_provider_wrappers = vec![
        "clearBrainProviderState",
        "startBrainRun",
        "drainBrainRun",
        "submitBrainHostResult",
        "cancelBrainRun",
        "providerStateDiagnostics",
        "bufferedBrainRunDiagnostics",
        "cleanupBufferedBrainRuns",
        "diagnosticSubmitBrainActionsJson",
    ];
    let brain_provider_direct_methods = vec![
        "registerBrainImplementation",
        "replaceBrainImplementation",
        "unregisterBrainImplementationForProfile",
        "submitBrainEvent",
        "buildBrainWakeRequest",
        "buildBrainWakeRequestForSession",
        "getBuffer",
        "releaseBuffer",
    ];
    let roleplay_operations = vec![
        "plan_roleplay_assistant_alternative",
        "plan_roleplay_session_lifecycle",
        "plan_roleplay_chat_layer_binding",
        "normalize_roleplay_lore_search_controls",
        "read_roleplay_scene_state",
        "plan_roleplay_scene_state_update",
        "build_roleplay_prompt_context",
        "roleplay_speaker_identity",
        "write_roleplay_character",
        "merge_roleplay_character",
        "write_roleplay_player_persona",
        "merge_roleplay_player_persona",
        "patch_roleplay_session_metadata",
        "normalize_roleplay_narrator_config",
        "plan_roleplay_mechanic_profile",
        "start_roleplay_narrator_turn",
        "advance_roleplay_narrator_turn",
        "create_lore_layer",
        "get_lore_layer",
        "list_lore_layers",
        "update_lore_layer",
        "archive_lore_layer",
        "set_chat_layers",
        "get_chat_layers",
        "toggle_chat_layer",
        "reorder_chat_layers",
        "add_lore_entry",
        "replace_lore_entry",
        "supersede_lore_entry",
        "tombstone_lore_entry",
        "query_lore_entries",
        "get_lore_entry",
        "lore_entry_provenance_events",
        "add_entry_to_layer",
        "remove_entry_from_layer",
        "set_entry_constant",
        "list_entries_by_layer",
        "recall_lore",
        "capture_lore_fact",
        "promote_lore_entry",
        "get_lore_layer_config",
        "set_lore_layer_config",
        "list_recall_traces",
        "get_recall_trace",
    ];
    let roleplay_json_input_operations = vec![
        "plan_roleplay_assistant_alternative",
        "plan_roleplay_session_lifecycle",
        "plan_roleplay_chat_layer_binding",
        "normalize_roleplay_lore_search_controls",
        "read_roleplay_scene_state",
        "plan_roleplay_scene_state_update",
        "build_roleplay_prompt_context",
        "roleplay_speaker_identity",
        "write_roleplay_character",
        "merge_roleplay_character",
        "write_roleplay_player_persona",
        "merge_roleplay_player_persona",
        "patch_roleplay_session_metadata",
        "normalize_roleplay_narrator_config",
        "plan_roleplay_mechanic_profile",
        "start_roleplay_narrator_turn",
        "advance_roleplay_narrator_turn",
        "create_lore_layer",
        "update_lore_layer",
        "archive_lore_layer",
        "set_chat_layers",
        "toggle_chat_layer",
        "reorder_chat_layers",
        "add_lore_entry",
        "replace_lore_entry",
        "supersede_lore_entry",
        "tombstone_lore_entry",
        "query_lore_entries",
        "add_entry_to_layer",
        "remove_entry_from_layer",
        "set_entry_constant",
        "recall_lore",
        "capture_lore_fact",
        "promote_lore_entry",
        "set_lore_layer_config",
        "list_recall_traces",
    ];
    let conversation_operations = vec![
        "save_message_slot",
        "save_message_variant",
        "create_chat_message_slot",
        "create_chat_message_variant",
        "chat_read_model_page",
        "read_chat_session",
        "query_chat_session_summaries",
        "append_chat_event",
        "query_chat_events",
        "query_message_slots",
        "query_message_slots_page",
        "query_message_variants",
        "query_message_variants_page",
        "select_active_message_variant",
        "select_active_chat_message_variant",
        "delete_chat_message_variant",
        "reorder_chat_message_variants",
        "delete_message_variant",
        "reorder_message_variants",
        "save_conversation_branch",
        "create_chat_conversation_branch",
        "ensure_active_chat_conversation_branch",
        "query_conversation_branches",
        "get_conversation_branch_state",
        "select_active_conversation_branch",
        "update_conversation_branch_head",
        "save_conversation_snapshot",
        "create_chat_conversation_snapshot",
        "query_conversation_snapshots",
        "read_conversation_tree",
        "search_chat_transcript",
        "resolve_conversation_jump",
        "save_attachment",
        "create_chat_attachment",
        "query_attachments",
        "query_attachments_page",
        "remove_attachment",
        "remove_chat_attachment",
        "save_data_bank_scope",
        "create_chat_data_bank_scope",
        "query_data_bank_scopes",
        "query_data_bank_scopes_page",
        "remove_data_bank_scope",
        "remove_chat_data_bank_scope",
    ];
    let profile_registry_operations = vec![
        "plan_profile_registry_mutation",
        "create_profile_registry_record",
        "update_profile_registry_record",
        "list_profile_registry_records",
        "get_profile_registry_record",
        "purge_profile",
    ];
    let model_provider_operations = vec![
        "upsert_model_endpoint",
        "list_model_endpoints",
        "get_model_endpoint",
        "delete_model_endpoint",
        "upsert_model_configuration",
        "list_model_configurations",
        "get_model_configuration",
        "delete_model_configuration",
        "upsert_model_provider",
        "list_model_providers",
        "get_model_provider",
        "get_model_provider_secret",
        "model_provider_refresh_impact",
        "plan_model_provider_refresh",
    ];
    let runtime_scheduler_operations = vec![
        "validate_runtime_config_draft",
        "plan_runtime_config",
        "register_scheduled_wake_job",
        "register_scheduled_host_job",
        "list_scheduled_jobs",
        "list_scheduled_runs",
        "claim_scheduled_host_runs",
        "request_scheduled_host_job_run",
        "complete_scheduled_host_run",
        "run_scheduler_tick",
        "request_scheduled_job_run",
        "pause_scheduled_job",
        "resume_scheduled_job",
        "database_size",
        "storage_diagnostics",
        "storage_schema",
        "run_maintenance",
    ];
    let runtime_scheduler_raw_methods = vec![
        "validateRuntimeConfigDraftJson",
        "planRuntimeConfigJson",
        "registerScheduledWakeJobJson",
        "registerScheduledHostJobJson",
        "listScheduledJobsJson",
        "listScheduledRunsJson",
        "claimScheduledHostRunsJson",
        "requestScheduledHostJobRunJson",
        "completeScheduledHostRun",
        "runSchedulerTickJson",
        "requestScheduledJobRunJson",
        "pauseScheduledJob",
        "resumeScheduledJob",
        "databaseSize",
        "storageDiagnostics",
        "storageSchema",
        "runMaintenance",
    ];
    let runtime_scheduler_wrappers = vec![
        "validateRuntimeConfigDraft",
        "planRuntimeConfig",
        "registerScheduledWakeJob",
        "registerScheduledHostJob",
        "listScheduledJobs",
        "listScheduledRuns",
        "claimScheduledHostRuns",
        "requestScheduledHostJobRun",
        "completeScheduledHostRun",
        "runSchedulerTick",
        "requestScheduledJobRun",
        "pauseScheduledJob",
        "resumeScheduledJob",
        "databaseSize",
        "storageDiagnostics",
        "storageSchema",
        "runMaintenance",
    ];
    ensure_family_operations_exist("memory", &memory_operations)?;
    ensure_family_operations_exist("brain_provider", &brain_provider_operations)?;
    ensure_family_operations_exist("roleplay", &roleplay_operations)?;
    ensure_family_operations_exist("conversation", &conversation_operations)?;
    ensure_family_operations_exist("profile_registry", &profile_registry_operations)?;
    ensure_family_operations_exist("model_provider", &model_provider_operations)?;
    ensure_family_operations_exist("runtime_scheduler", &runtime_scheduler_operations)?;

    let message_slot_record = serde_json::to_value(sample_message_slot_record())?;
    let message_slot_write = serde_json::to_value(sample_message_slot_write())?;
    let message_variant_record = serde_json::to_value(sample_message_variant_record())?;
    let message_variant_write = serde_json::to_value(sample_message_variant_write())?;
    let durable_message_record = serde_json::to_value(sample_durable_message_record())?;
    let durable_message_write = serde_json::to_value(sample_durable_message_write())?;
    let message_block_record = serde_json::to_value(sample_message_block_record())?;
    let message_block_write = serde_json::to_value(sample_message_block_write())?;
    let message_slot_query = serde_json::to_value(sample_message_slot_query())?;
    let message_variant_query = serde_json::to_value(sample_message_variant_query())?;
    let create_chat_message_slot_request =
        serde_json::to_value(sample_create_chat_message_slot_request())?;
    let create_chat_message_slot_result =
        serde_json::to_value(sample_create_chat_message_slot_result())?;
    let create_chat_message_variant_request =
        serde_json::to_value(sample_create_chat_message_variant_request())?;
    let create_chat_message_variant_result =
        serde_json::to_value(sample_create_chat_message_variant_result())?;
    let chat_read_model_query = serde_json::to_value(sample_chat_read_model_query())?;
    let chat_read_model_page = serde_json::to_value(sample_chat_read_model_page())?;
    let chat_event_log_append = serde_json::to_value(sample_chat_event_log_append())?;
    let chat_event_log_query = serde_json::to_value(sample_chat_event_log_query())?;
    let chat_event_log_page = serde_json::to_value(sample_chat_event_log_page())?;
    let conversation_branch_record = serde_json::to_value(sample_conversation_branch_record())?;
    let conversation_branch_write = serde_json::to_value(sample_conversation_branch_write())?;
    let conversation_branch_query = serde_json::to_value(sample_conversation_branch_query())?;
    let conversation_branch_state =
        serde_json::to_value(sample_conversation_branch_state_record())?;
    let conversation_snapshot_record = serde_json::to_value(sample_conversation_snapshot_record())?;
    let conversation_snapshot_write = serde_json::to_value(sample_conversation_snapshot_write())?;
    let conversation_snapshot_query = serde_json::to_value(sample_conversation_snapshot_query())?;
    let conversation_jump_request = serde_json::to_value(sample_conversation_jump_request())?;
    let conversation_jump_result = serde_json::to_value(sample_conversation_jump_result())?;
    let attachment_record = serde_json::to_value(sample_attachment_record())?;
    let attachment_write = serde_json::to_value(sample_attachment_write())?;
    let attachment_link_record = serde_json::to_value(sample_attachment_link_record())?;
    let attachment_link_write = serde_json::to_value(sample_attachment_link_write())?;
    let attachment_query = serde_json::to_value(sample_attachment_query())?;
    let data_bank_scope_record = serde_json::to_value(sample_data_bank_scope_record())?;
    let data_bank_scope_write = serde_json::to_value(sample_data_bank_scope_write())?;
    let data_bank_scope_query = serde_json::to_value(sample_data_bank_scope_query())?;
    let roleplay_lore_record = serde_json::to_value(sample_roleplay_lore_record())?;
    let roleplay_lore_write = serde_json::to_value(sample_roleplay_lore_write())?;
    let roleplay_lore_replace = serde_json::to_value(sample_roleplay_lore_replace())?;
    let roleplay_lore_supersede = serde_json::to_value(sample_roleplay_lore_supersede())?;
    let roleplay_lore_tombstone = serde_json::to_value(sample_roleplay_lore_tombstone())?;
    let roleplay_lore_query = serde_json::to_value(sample_roleplay_lore_query())?;
    let roleplay_lore_provenance = serde_json::to_value(sample_roleplay_lore_provenance_event())?;
    let roleplay_lore_layer_record = serde_json::to_value(sample_roleplay_lore_layer_record())?;
    let roleplay_lore_layer_write = serde_json::to_value(sample_roleplay_lore_layer_write())?;
    let roleplay_lore_layer_update = serde_json::to_value(sample_roleplay_lore_layer_update())?;
    let roleplay_lore_layer_archive = serde_json::to_value(sample_roleplay_lore_layer_archive())?;
    let roleplay_lore_layer_config_record =
        serde_json::to_value(sample_roleplay_lore_layer_config_record())?;
    let roleplay_lore_layer_config_write =
        serde_json::to_value(sample_roleplay_lore_layer_config_write())?;
    let roleplay_lore_layer_entry_link =
        serde_json::to_value(sample_roleplay_lore_layer_entry_link())?;
    let roleplay_lore_layer_entry_join =
        serde_json::to_value(sample_roleplay_lore_layer_entry_join())?;
    let roleplay_lore_fact_capture = serde_json::to_value(sample_roleplay_lore_fact_capture())?;
    let roleplay_lore_entry_promotion =
        serde_json::to_value(sample_roleplay_lore_entry_promotion())?;
    let roleplay_chat_layers_write = serde_json::to_value(sample_roleplay_chat_layers_write())?;
    let roleplay_chat_layer_record = serde_json::to_value(sample_roleplay_chat_layer_record())?;
    let lore_recall_query = serde_json::to_value(sample_lore_recall_query())?;
    let lore_recall_result = serde_json::to_value(sample_lore_recall_result())?;
    let lore_recall_trace_record = serde_json::to_value(sample_lore_recall_trace_record())?;
    let lore_recall_trace_query = serde_json::to_value(sample_lore_recall_trace_query())?;
    let roleplay_prompt_context_input =
        serde_json::to_value(sample_roleplay_prompt_context_input())?;
    let roleplay_prompt_context_output =
        serde_json::to_value(sample_roleplay_prompt_context_output())?;
    let roleplay_session_lifecycle_input =
        serde_json::to_value(sample_roleplay_session_lifecycle_plan_input())?;
    let roleplay_session_lifecycle_plan =
        serde_json::to_value(sample_roleplay_session_lifecycle_plan())?;
    let roleplay_chat_layer_binding_input =
        serde_json::to_value(sample_roleplay_chat_layer_binding_plan_input())?;
    let roleplay_chat_layer_binding_plan =
        serde_json::to_value(sample_roleplay_chat_layer_binding_plan())?;
    let roleplay_lore_search_controls_input =
        serde_json::to_value(sample_roleplay_lore_search_controls_input())?;
    let roleplay_lore_search_controls =
        serde_json::to_value(sample_roleplay_lore_search_controls())?;
    let roleplay_speaker_identity_input =
        serde_json::to_value(sample_roleplay_speaker_identity_input())?;
    let roleplay_speaker_identity_snapshot =
        serde_json::to_value(sample_roleplay_speaker_identity_snapshot())?;
    let roleplay_session_metadata = serde_json::to_value(sample_roleplay_session_metadata())?;
    let roleplay_player_persona = serde_json::to_value(sample_roleplay_player_persona())?;
    let roleplay_character = serde_json::to_value(sample_roleplay_character())?;
    let roleplay_character_write_input =
        serde_json::to_value(sample_roleplay_character_write_input())?;
    let roleplay_character_merge_input =
        serde_json::to_value(sample_roleplay_character_merge_input())?;
    let roleplay_persona_write_input =
        serde_json::to_value(sample_roleplay_player_persona_write_input())?;
    let roleplay_persona_merge_input =
        serde_json::to_value(sample_roleplay_player_persona_merge_input())?;
    let roleplay_session_metadata_patch_input =
        serde_json::to_value(sample_roleplay_session_metadata_patch_input())?;
    let roleplay_session_metadata_patch_output =
        serde_json::to_value(sample_roleplay_session_metadata_patch_output())?;
    let roleplay_scene_state = serde_json::to_value(sample_roleplay_scene_state())?;
    let roleplay_scene_state_read_input =
        serde_json::to_value(sample_roleplay_scene_state_read_input())?;
    let roleplay_scene_state_read_output =
        serde_json::to_value(sample_roleplay_scene_state_read_output())?;
    let roleplay_scene_state_update_input =
        serde_json::to_value(sample_roleplay_scene_state_update_input())?;
    let roleplay_scene_state_update_plan =
        serde_json::to_value(sample_roleplay_scene_state_update_plan())?;
    let roleplay_narrator_config = serde_json::to_value(sample_roleplay_narrator_config())?;
    let roleplay_mechanic_profile_plan =
        serde_json::to_value(sample_roleplay_mechanic_profile_plan())?;
    let roleplay_narrator_tool_request =
        serde_json::to_value(sample_roleplay_narrator_tool_request())?;
    let roleplay_narrator_tool_observation =
        serde_json::to_value(sample_roleplay_narrator_tool_observation())?;
    let roleplay_narrator_start_input =
        serde_json::to_value(sample_roleplay_narrator_start_input())?;
    let roleplay_narrator_advance_input =
        serde_json::to_value(sample_roleplay_narrator_advance_input())?;
    let roleplay_narrator_turn_receipt =
        serde_json::to_value(sample_roleplay_narrator_turn_receipt())?;
    let roleplay_assistant_alternative_input =
        serde_json::to_value(sample_roleplay_assistant_alternative_plan_input())?;
    let roleplay_assistant_alternative_plan =
        serde_json::to_value(sample_roleplay_assistant_alternative_plan())?;
    let profile_memory_record = sample_profile_memory_record_value();
    let profile_memory_write = sample_profile_memory_write_value();
    let profile_memory_replace = sample_profile_memory_replace_value();
    let profile_memory_delete = sample_profile_memory_delete_value();
    let profile_memory_query = sample_profile_memory_query_value();
    let session_memory_record = serde_json::to_value(sample_session_memory_record())?;
    let session_memory_query = serde_json::to_value(sample_session_memory_query())?;
    let branch_aware_session_memory_query =
        serde_json::to_value(sample_branch_aware_session_memory_query())?;
    let session_memory_prompt_context =
        serde_json::to_value(sample_session_memory_prompt_context())?;
    let memory_proposal = serde_json::to_value(sample_memory_proposal())?;
    let memory_proposal_record = serde_json::to_value(sample_memory_proposal_record())?;
    let memory_proposal_query = serde_json::to_value(sample_memory_proposal_query())?;
    let memory_governance_decision_input =
        serde_json::to_value(sample_memory_governance_decision_input())?;
    let memory_governance_decision_record =
        serde_json::to_value(sample_memory_governance_decision_record())?;
    let session_activity_digest = serde_json::to_value(sample_session_activity_digest())?;
    let session_activity_digest_query =
        serde_json::to_value(sample_session_activity_digest_query())?;
    let context_compaction_artifact = serde_json::to_value(sample_context_compaction_artifact())?;
    let context_compaction_artifact_query =
        serde_json::to_value(sample_context_compaction_artifact_query())?;
    let brain_registration = serde_json::to_value(sample_brain_implementation_registration())?;
    let brain_model_config = serde_json::to_value(sample_brain_model_config())?;
    let brain_strategy = serde_json::to_value(sample_brain_strategy_metadata())?;
    let brain_provider_state_scope = serde_json::to_value(sample_brain_provider_state_scope())?;
    let brain_wake_request = serde_json::to_value(sample_brain_wake_request())?;
    let brain_wake_accepted = serde_json::to_value(sample_brain_wake_accepted())?;
    let brain_wake_stream_result = serde_json::to_value(BrainWakeStreamResultFixture {
        stream: sample_brain_wake_stream(),
    })?;
    let brain_event_envelope = serde_json::to_value(sample_brain_event_envelope())?;
    let brain_action_batch = serde_json::to_value(sample_brain_action_batch())?;
    let brain_action = serde_json::to_value(sample_brain_action())?;
    let brain_wake_failure = serde_json::to_value(sample_brain_wake_failure())?;
    let brain_provider_state_input = serde_json::to_value(sample_brain_provider_state_input())?;
    let brain_provider_state_update = serde_json::to_value(sample_brain_provider_state_update())?;
    let brain_provider_state_output = serde_json::to_value(sample_brain_provider_state_output())?;
    let brain_tool_call_metadata = serde_json::to_value(sample_tool_call_metadata())?;
    let brain_completion_packet = serde_json::to_value(sample_completion_packet())?;
    let runtime_buffer_view = sample_runtime_buffer_view_value();
    let native_provider_state_diagnostic = sample_native_provider_state_diagnostic_value();
    let buffered_brain_run_diagnostics = sample_buffered_brain_run_diagnostics_value();
    let buffered_brain_run_cleanup_summary = sample_buffered_brain_run_cleanup_summary_value();
    let profile_registry_record = serde_json::to_value(sample_profile_registry_record())?;
    let profile_registry_write = serde_json::to_value(sample_profile_registry_write())?;
    let profile_registry_update = serde_json::to_value(sample_profile_registry_update())?;
    let profile_registry_mutation_request =
        serde_json::to_value(sample_profile_registry_mutation_request())?;
    let profile_registry_mutation_plan =
        serde_json::to_value(sample_profile_registry_mutation_plan())?;
    let profile_purge_report = serde_json::to_value(sample_profile_purge_report())?;
    let model_provider_record = serde_json::to_value(sample_model_provider_record())?;
    let refresh_impact = serde_json::to_value(sample_model_provider_refresh_impact())?;
    let refresh_plan = serde_json::to_value(sample_model_provider_refresh_plan())?;
    let runtime_config_validation_input =
        serde_json::to_value(sample_runtime_config_validation_input())?;
    let runtime_config_validation_result =
        serde_json::to_value(sample_runtime_config_validation_result())?;
    let runtime_config_diagnostic = serde_json::to_value(sample_runtime_config_diagnostic())?;
    let runtime_config_plan = serde_json::to_value(sample_runtime_config_plan())?;
    let runtime_config_draft = serde_json::to_value(sample_runtime_config_draft())?;
    let runtime_brain_config_draft = serde_json::to_value(sample_brain_config_draft())?;
    let runtime_session_config_draft = serde_json::to_value(sample_session_config_draft())?;
    let runtime_scheduled_job_config_draft =
        serde_json::to_value(sample_scheduled_job_config_draft())?;
    let runtime_channel_binding_config_draft =
        serde_json::to_value(sample_channel_binding_config_draft())?;
    let runtime_mcp_binding_config_draft = serde_json::to_value(sample_mcp_binding_config_draft())?;
    let runtime_profile_metadata = serde_json::to_value(sample_profile_runtime_metadata())?;
    let runtime_resource_limits = serde_json::to_value(sample_resource_limits())?;
    let scheduled_job_summary = sample_scheduled_job_summary_value();
    let scheduled_run_summary = sample_scheduled_run_summary_value();
    let scheduler_tick_report = sample_scheduler_tick_report_value();
    let runtime_database_size = sample_runtime_database_size_value();
    let runtime_storage_diagnostics = sample_runtime_storage_diagnostics_value();
    let runtime_maintenance_policy = sample_runtime_maintenance_policy_value();
    let runtime_maintenance_report = sample_runtime_maintenance_report_value();

    Ok(json!({
        "formatVersion": 1,
        "source": "rusty-crew-core-bridge-codegen",
        "families": {
            "memory": {
                "operationNames": memory_operations,
                "rawMethods": memory_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "passthroughWrappers": memory_operations
                    .iter()
                    .map(|operation| operation_name_to_camel_wrapper(operation))
                    .collect::<Vec<_>>(),
                "jsonInputWrappers": memory_json_input_operations
                    .iter()
                    .map(|operation| operation_name_to_camel_wrapper(operation))
                    .collect::<Vec<_>>(),
                "jsonInputRawMethods": memory_json_input_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "directNativeMethods": profile_memory_direct_methods,
                "dtoFields": {
                    "ProfileMemoryRecord": object_keys(&profile_memory_record)?,
                    "ProfileMemoryWrite": object_keys(&profile_memory_write)?,
                    "ProfileMemoryReplace": object_keys(&profile_memory_replace)?,
                    "ProfileMemoryDelete": object_keys(&profile_memory_delete)?,
                    "ProfileMemoryQuery": object_keys(&profile_memory_query)?,
                    "SessionMemoryRecord": object_keys(&session_memory_record)?,
                    "SessionMemoryQuery": object_keys(&session_memory_query)?,
                    "BranchAwareSessionMemoryQuery": object_keys(&branch_aware_session_memory_query)?,
                    "SessionMemoryPromptContext": object_keys(&session_memory_prompt_context)?,
                    "SessionMemoryPromptDiagnostics": object_keys(required_value(&session_memory_prompt_context, "diagnostics")?)?,
                    "MemoryProposalEnvelope": object_keys(&memory_proposal)?,
                    "MemoryProposalRecord": object_keys(&memory_proposal_record)?,
                    "MemoryProposalQuery": object_keys(&memory_proposal_query)?,
                    "MemoryGovernanceDecisionInput": object_keys(&memory_governance_decision_input)?,
                    "MemoryGovernanceDecisionRecord": object_keys(&memory_governance_decision_record)?,
                    "SessionActivityDigest": object_keys(&session_activity_digest)?,
                    "SessionActivityDigestQuery": object_keys(&session_activity_digest_query)?,
                    "ContextCompactionArtifact": object_keys(&context_compaction_artifact)?,
                    "ContextCompactionArtifactQuery": object_keys(&context_compaction_artifact_query)?,
                }
            },
            "brainProvider": {
                "operationNames": brain_provider_operations,
                "rawMethods": brain_provider_raw_methods,
                "passthroughWrappers": brain_provider_wrappers,
                "directNativeMethods": brain_provider_direct_methods,
                "namedTypeScriptInterfaces": [
                    "NativeProviderStateDiagnostic",
                    "NativeBufferedBrainRunModuleDiagnostics",
                    "NativeBufferedBrainRunDiagnostic",
                    "NativeBufferedBrainRunDiagnostics",
                    "NativeBufferedBrainRunCleanupModuleReport",
                    "NativeBufferedBrainRunCleanupSummary",
                    "RawBrainWakeStreamItem",
                    "RawBrainAction",
                    "RawBrainEvent",
                    "RawToolCallMetadata",
                    "RawToolCallPolicyMetadata",
                    "RawBrainWakeProviderStateOutput"
                ],
                "dtoFields": {
                    "BrainImplementationRegistration": object_keys(&brain_registration)?,
                    "BrainModelConfig": object_keys(&brain_model_config)?,
                    "BrainStrategyMetadata": object_keys(&brain_strategy)?,
                    "BrainProviderStateScope": object_keys(&brain_provider_state_scope)?,
                    "BrainWakeRequest": object_keys(&brain_wake_request)?,
                    "BrainWakeAccepted": object_keys(&brain_wake_accepted)?,
                    "BrainWakeStreamResult": object_keys(&brain_wake_stream_result)?,
                    "BrainWakeStreamEventItem": object_keys(first_array_item(&brain_wake_stream_result, "stream")?)?,
                    "BrainEventEnvelope": object_keys(&brain_event_envelope)?,
                    "BrainActionBatch": object_keys(&brain_action_batch)?,
                    "BrainAction": object_keys(&brain_action)?,
                    "BrainWakeFailure": object_keys(&brain_wake_failure)?,
                    "BrainWakeProviderStateInput": object_keys(&brain_provider_state_input)?,
                    "BrainWakeProviderStateUpdate": object_keys(&brain_provider_state_update)?,
                    "BrainWakeProviderStateOutput": object_keys(&brain_provider_state_output)?,
                    "ToolCallMetadata": object_keys(&brain_tool_call_metadata)?,
                    "ToolCallPolicyMetadata": object_keys(required_value(&brain_tool_call_metadata, "policy")?)?,
                    "CompletionPacket": object_keys(&brain_completion_packet)?,
                    "RuntimeBufferView": object_keys(&runtime_buffer_view)?,
                    "NativeProviderStateDiagnostic": object_keys(&native_provider_state_diagnostic)?,
                    "NativeBufferedBrainRunDiagnostics": object_keys(&buffered_brain_run_diagnostics)?,
                    "NativeBufferedBrainRunModuleDiagnostics": object_keys(first_array_item(&buffered_brain_run_diagnostics, "modules")?)?,
                    "NativeBufferedBrainRunDiagnostic": object_keys(first_array_item(&buffered_brain_run_diagnostics, "runs")?)?,
                    "NativeBufferedBrainRunCleanupSummary": object_keys(&buffered_brain_run_cleanup_summary)?,
                    "NativeBufferedBrainRunCleanupModuleReport": object_keys(first_array_item(&buffered_brain_run_cleanup_summary, "modules")?)?,
                }
            },
            "roleplay": {
                "operationNames": roleplay_operations,
                "rawMethods": roleplay_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "passthroughWrappers": roleplay_operations
                    .iter()
                    .map(|operation| operation_name_to_camel_wrapper(operation))
                    .collect::<Vec<_>>(),
                "jsonInputWrappers": roleplay_json_input_operations
                    .iter()
                    .map(|operation| operation_name_to_camel_wrapper(operation))
                    .collect::<Vec<_>>(),
                "jsonInputRawMethods": roleplay_json_input_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "dtoFields": {
                    "RoleplayLoreRecord": object_keys(&roleplay_lore_record)?,
                    "RoleplayLoreWrite": object_keys(&roleplay_lore_write)?,
                    "RoleplayLoreReplace": object_keys(&roleplay_lore_replace)?,
                    "RoleplayLoreSupersede": object_keys(&roleplay_lore_supersede)?,
                    "RoleplayLoreTombstone": object_keys(&roleplay_lore_tombstone)?,
                    "RoleplayLoreQuery": object_keys(&roleplay_lore_query)?,
                    "RoleplayLoreProvenanceEvent": object_keys(&roleplay_lore_provenance)?,
                    "RoleplayLoreLayerRecord": object_keys(&roleplay_lore_layer_record)?,
                    "RoleplayLoreLayerWrite": object_keys(&roleplay_lore_layer_write)?,
                    "RoleplayLoreLayerUpdate": object_keys(&roleplay_lore_layer_update)?,
                    "RoleplayLoreLayerArchive": object_keys(&roleplay_lore_layer_archive)?,
                    "RoleplayLoreLayerConfigRecord": object_keys(&roleplay_lore_layer_config_record)?,
                    "RoleplayLoreLayerConfigWrite": object_keys(&roleplay_lore_layer_config_write)?,
                    "RoleplayLoreLayerEntryLink": object_keys(&roleplay_lore_layer_entry_link)?,
                    "RoleplayLoreLayerEntryJoin": object_keys(&roleplay_lore_layer_entry_join)?,
                    "RoleplayLoreFactCapture": object_keys(&roleplay_lore_fact_capture)?,
                    "RoleplayLoreEntryPromotion": object_keys(&roleplay_lore_entry_promotion)?,
                    "RoleplayChatLayersWrite": object_keys(&roleplay_chat_layers_write)?,
                    "RoleplayChatLayerRecord": object_keys(&roleplay_chat_layer_record)?,
                    "LoreRecallQuery": object_keys(&lore_recall_query)?,
                    "LoreRecallResult": object_keys(&lore_recall_result)?,
                    "LoreRecallEntry": object_keys(first_array_item(&lore_recall_result, "entries")?)?,
                    "LoreRecallTraceRecord": object_keys(&lore_recall_trace_record)?,
                    "LoreRecallTraceQuery": object_keys(&lore_recall_trace_query)?,
                    "RoleplayPromptContextInput": object_keys(&roleplay_prompt_context_input)?,
                    "RoleplayPromptContextOutput": object_keys(&roleplay_prompt_context_output)?,
                    "RoleplaySessionLifecyclePlanInput": object_keys(&roleplay_session_lifecycle_input)?,
                    "RoleplaySessionLifecyclePlan": object_keys(&roleplay_session_lifecycle_plan)?,
                    "RoleplayChatLayerBindingPlanInput": object_keys(&roleplay_chat_layer_binding_input)?,
                    "RoleplayChatLayerBindingPlan": object_keys(&roleplay_chat_layer_binding_plan)?,
                    "RoleplayLoreSearchControlsInput": object_keys(&roleplay_lore_search_controls_input)?,
                    "RoleplayLoreSearchControls": object_keys(&roleplay_lore_search_controls)?,
                    "RoleplaySpeakerIdentityInput": object_keys(&roleplay_speaker_identity_input)?,
                    "RoleplaySpeakerIdentitySnapshot": object_keys(&roleplay_speaker_identity_snapshot)?,
                    "RoleplaySessionMetadata": object_keys(&roleplay_session_metadata)?,
                    "RoleplayPlayerPersona": object_keys(&roleplay_player_persona)?,
                    "RoleplayCharacter": object_keys(&roleplay_character)?,
                    "RoleplayCharacterWriteInput": object_keys(&roleplay_character_write_input)?,
                    "RoleplayCharacterMergeInput": object_keys(&roleplay_character_merge_input)?,
                    "RoleplayPlayerPersonaWriteInput": object_keys(&roleplay_persona_write_input)?,
                    "RoleplayPlayerPersonaMergeInput": object_keys(&roleplay_persona_merge_input)?,
                    "RoleplaySessionMetadataPatchInput": object_keys(&roleplay_session_metadata_patch_input)?,
                    "RoleplaySessionMetadataPatchOutput": object_keys(&roleplay_session_metadata_patch_output)?,
                    "RoleplaySceneState": object_keys(&roleplay_scene_state)?,
                    "RoleplaySceneStateReadInput": object_keys(&roleplay_scene_state_read_input)?,
                    "RoleplaySceneStateReadOutput": object_keys(&roleplay_scene_state_read_output)?,
                    "RoleplaySceneStateUpdateInput": object_keys(&roleplay_scene_state_update_input)?,
                    "RoleplaySceneStateUpdatePlan": object_keys(&roleplay_scene_state_update_plan)?,
                    "RoleplayNarratorConfig": object_keys(&roleplay_narrator_config)?,
                    "RoleplayMechanicProfilePlan": object_keys(&roleplay_mechanic_profile_plan)?,
                    "RoleplayMechanicConfig": object_keys(
                        required_value(&roleplay_mechanic_profile_plan, "config")?
                    )?,
                    "RoleplayNarratorToolRequest": object_keys(&roleplay_narrator_tool_request)?,
                    "RoleplayNarratorToolObservation": object_keys(&roleplay_narrator_tool_observation)?,
                    "RoleplayNarratorStartInput": object_keys(&roleplay_narrator_start_input)?,
                    "RoleplayNarratorAdvanceInput": object_keys(&roleplay_narrator_advance_input)?,
                    "RoleplayNarratorTurnReceipt": object_keys(&roleplay_narrator_turn_receipt)?,
                    "RoleplayAssistantAlternativePlanInput": object_keys(&roleplay_assistant_alternative_input)?,
                    "RoleplayAssistantAlternativePlan": object_keys(&roleplay_assistant_alternative_plan)?,
                }
            },
            "conversation": {
                "operationNames": conversation_operations,
                "rawMethods": conversation_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "passthroughWrappers": conversation_operations
                    .iter()
                    .map(|operation| operation_name_to_camel_wrapper(operation))
                    .collect::<Vec<_>>(),
                "namedTypeScriptInterfaces": [
                    "NativeChatReadModelEvent",
                    "NativeChatReadModelPage",
                    "NativeChatEventLogEvent",
                    "NativeChatEventLogPage"
                ],
                "dtoFields": {
                    "MessageSlotRecord": object_keys(&message_slot_record)?,
                    "MessageSlotWrite": object_keys(&message_slot_write)?,
                    "MessageVariantRecord": object_keys(&message_variant_record)?,
                    "MessageVariantWrite": object_keys(&message_variant_write)?,
                    "DurableMessageRecord": object_keys(&durable_message_record)?,
                    "DurableMessageWrite": object_keys(&durable_message_write)?,
                    "MessageBlockRecord": object_keys(&message_block_record)?,
                    "MessageBlockWrite": object_keys(&message_block_write)?,
                    "MessageSlotQuery": object_keys(&message_slot_query)?,
                    "MessageVariantQuery": object_keys(&message_variant_query)?,
                    "CreateChatMessageSlotRequest": object_keys(&create_chat_message_slot_request)?,
                    "CreateChatMessageSlotResult": object_keys(&create_chat_message_slot_result)?,
                    "CreateChatMessageVariantRequest": object_keys(&create_chat_message_variant_request)?,
                    "CreateChatMessageVariantResult": object_keys(&create_chat_message_variant_result)?,
                    "ChatReadModelQuery": object_keys(&chat_read_model_query)?,
                    "ChatReadModelPage": object_keys(&chat_read_model_page)?,
                    "ChatReadModelEvent": object_keys(first_array_item(&chat_read_model_page, "items")?)?,
                    "ChatEventLogAppend": object_keys(&chat_event_log_append)?,
                    "ChatEventLogQuery": object_keys(&chat_event_log_query)?,
                    "ChatEventLogPage": object_keys(&chat_event_log_page)?,
                    "ChatEventLogEvent": object_keys(first_array_item(&chat_event_log_page, "items")?)?,
                    "ConversationBranchRecord": object_keys(&conversation_branch_record)?,
                    "ConversationBranchWrite": object_keys(&conversation_branch_write)?,
                    "ConversationBranchQuery": object_keys(&conversation_branch_query)?,
                    "ConversationBranchStateRecord": object_keys(&conversation_branch_state)?,
                    "ConversationSnapshotRecord": object_keys(&conversation_snapshot_record)?,
                    "ConversationSnapshotWrite": object_keys(&conversation_snapshot_write)?,
                    "ConversationSnapshotQuery": object_keys(&conversation_snapshot_query)?,
                    "ConversationJumpRequest": object_keys(&conversation_jump_request)?,
                    "ConversationJumpResult": object_keys(&conversation_jump_result)?,
                    "AttachmentRecord": object_keys(&attachment_record)?,
                    "AttachmentWrite": object_keys(&attachment_write)?,
                    "AttachmentLinkRecord": object_keys(&attachment_link_record)?,
                    "AttachmentLinkWrite": object_keys(&attachment_link_write)?,
                    "AttachmentQuery": object_keys(&attachment_query)?,
                    "DataBankScopeRecord": object_keys(&data_bank_scope_record)?,
                    "DataBankScopeWrite": object_keys(&data_bank_scope_write)?,
                    "DataBankScopeQuery": object_keys(&data_bank_scope_query)?,
                }
            },
            "profileRegistry": {
                "operationNames": profile_registry_operations,
                "rawMethods": profile_registry_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "dtoFields": {
                    "RawProfileRegistryRecord": object_keys(&profile_registry_record)?,
                    "RawProfileRegistryWrite": object_keys(&profile_registry_write)?,
                    "RawProfileRegistryUpdate": object_keys(&profile_registry_update)?,
                    "RawProfileRegistryMutationRequest": object_keys(&profile_registry_mutation_request)?,
                    "RawProfileRegistryMutationPlan": object_keys(&profile_registry_mutation_plan)?,
                    "RawProfileRegistryMutationImplications": object_keys(required_value(&profile_registry_mutation_plan, "implications")?)?,
                    "RawProfileRegistrySourceAssetRef": object_keys(first_array_item(&profile_registry_record, "source_asset_refs")?)?,
                    "RawProfileRegistryDerivedRuntimeRef": object_keys(first_array_item(&profile_registry_record, "derived_runtime_refs")?)?,
                    "RawProfileRegistryImportExportMetadata": object_keys(required_value(&profile_registry_record, "import_export")?)?,
                    "RawProfilePurgeReport": object_keys(&profile_purge_report)?,
                    "RawProfilePurgeTableCount": object_keys(first_array_item(&profile_purge_report, "table_counts")?)?,
                }
            },
            "modelProvider": {
                "operationNames": model_provider_operations,
                "rawMethods": model_provider_operations
                    .iter()
                    .map(|operation| operation_name_to_native_method(operation))
                    .collect::<Vec<_>>(),
                "dtoFields": {
                    "RawModelProviderRecord": object_keys(&model_provider_record)?,
                    "RawModelProviderCredential": object_keys(required_value(&model_provider_record, "credential")?)?,
                    "RawModelProviderRefreshImpact": object_keys(&refresh_impact)?,
                    "RawModelProviderAffectedProfile": object_keys(first_array_item(&refresh_impact, "affected_profiles")?)?,
                    "RawModelProviderRefreshPlan": object_keys(&refresh_plan)?,
                    "RawModelProviderRefreshProfileAction": object_keys(first_array_item(&refresh_plan, "actions")?)?,
                }
            },
            "runtimeScheduler": {
                "operationNames": runtime_scheduler_operations,
                "rawMethods": runtime_scheduler_raw_methods,
                "passthroughWrappers": runtime_scheduler_wrappers,
                "namedTypeScriptInterfaces": [
                    "RawRuntimeConfigDraft",
                    "RawSessionConfigDraft",
                    "RawScheduledJobConfigDraft",
                    "RawChannelBindingConfigDraft",
                    "RawMcpBindingConfigDraft",
                    "RawResourceLimits",
                    "RawScheduledJobSummary",
                    "RawScheduledRunSummary",
                    "RawSchedulerTickReport",
                    "NativeRuntimeConfigValidationInput",
                    "NativeRuntimeConfigValidationResult",
                    "NativeRuntimeConfigPlan",
                    "NativeRuntimeConfigDraft",
                    "NativeRuntimeStorageDiagnostics",
                    "NativeRuntimeMaintenancePolicy",
                    "NativeRuntimeMaintenanceReport"
                ],
                "dtoFields": {
                    "RuntimeConfigValidationInput": object_keys(&runtime_config_validation_input)?,
                    "RuntimeConfigValidationResult": object_keys(&runtime_config_validation_result)?,
                    "RuntimeConfigDiagnostic": object_keys(&runtime_config_diagnostic)?,
                    "RuntimeConfigPlan": object_keys(&runtime_config_plan)?,
                    "RuntimeConfigDraft": object_keys(&runtime_config_draft)?,
                    "BrainConfigDraft": object_keys(&runtime_brain_config_draft)?,
                    "SessionConfigDraft": object_keys(&runtime_session_config_draft)?,
                    "ScheduledJobConfigDraft": object_keys(&runtime_scheduled_job_config_draft)?,
                    "ChannelBindingConfigDraft": object_keys(&runtime_channel_binding_config_draft)?,
                    "McpBindingConfigDraft": object_keys(&runtime_mcp_binding_config_draft)?,
                    "ProfileRuntimeMetadata": object_keys(&runtime_profile_metadata)?,
                    "ResourceLimits": object_keys(&runtime_resource_limits)?,
                    "RawScheduledJobSummary": object_keys(&scheduled_job_summary)?,
                    "RawScheduledRunSummary": object_keys(&scheduled_run_summary)?,
                    "RawSchedulerTickReport": object_keys(&scheduler_tick_report)?,
                    "NativeRuntimeDatabaseSize": object_keys(&runtime_database_size)?,
                    "NativeRuntimeStorageDiagnostics": object_keys(&runtime_storage_diagnostics)?,
                    "NativeRuntimeStorageCapability": object_keys(first_array_item(&runtime_storage_diagnostics, "capabilities")?)?,
                    "NativeRuntimeStorageTableCount": object_keys(first_array_item(&runtime_storage_diagnostics, "tableCounts")?)?,
                    "NativeRuntimeMaintenancePolicy": object_keys(&runtime_maintenance_policy)?,
                    "NativeRuntimeMaintenanceReport": object_keys(&runtime_maintenance_report)?,
                    "NativeSessionMemoryCompactionReport": object_keys(required_value(&runtime_maintenance_report, "sessionMemoryCompaction")?)?,
                }
            }
        }
    }))
}

fn object_keys(value: &Value) -> Result<Vec<String>> {
    let object = value
        .as_object()
        .context("expected generated inventory value to be a JSON object")?;
    Ok(object.keys().cloned().collect())
}

fn required_value<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .with_context(|| format!("expected generated inventory object key `{key}`"))
}

fn first_array_item<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    let array = required_value(value, key)?.as_array().with_context(|| {
        format!("expected generated inventory object key `{key}` to be an array")
    })?;
    array.first().with_context(|| {
        format!("expected generated inventory array `{key}` to contain a sample item")
    })
}

fn json_field_paths(value: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    collect_json_field_paths(value, "", &mut paths);
    paths
}

fn collect_json_field_paths(value: &Value, prefix: &str, paths: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                paths.push(path.clone());
                if key != "payload" && key != "strategy_config" {
                    collect_json_field_paths(child, &path, paths);
                }
            }
        }
        Value::Array(items) => {
            if let Some(first) = items.first() {
                let path = format!("{prefix}[]");
                collect_json_field_paths(first, &path, paths);
            }
        }
        _ => {}
    }
}

fn sample_runtime_config_validation_input() -> RuntimeConfigValidationInput {
    RuntimeConfigValidationInput {
        runtime_config: sample_runtime_config_draft(),
        profiles: vec![sample_profile_runtime_metadata()],
    }
}

fn sample_create_profile_plan_input() -> CreateProfilePlanInput {
    CreateProfilePlanInput {
        runtime_config: sample_runtime_config_draft(),
        profiles: vec![sample_profile_runtime_metadata()],
        profile_registry: vec![ProfileRegistryRuntimeMetadata {
            profile_id: ProfileId::new("field-sample-profile"),
            lifecycle_status: Some(ProfileRegistryLifecycleStatus::Active),
            revision: Some(7),
        }],
        request: CreateProfileRequest {
            model_config_id: None,
            profile_id: "field-created-profile".to_owned(),
            display_name: Some("Field Created Profile".to_owned()),
            soul_markdown: Some("# Field soul\n\nKeep exact spacing.\n".to_owned()),
            memory_markdown: Some("# Field memory\n".to_owned()),
            agent_id: Some("field-created-agent".to_owned()),
            session_id: Some("field-created-session".to_owned()),
            implementation_id: Some("field-created-brain".to_owned()),
            kind: Some(SessionKind::Full),
            workspace_cwd: Some("/tmp/field-created-workspace".to_owned()),
            provider_alias: Some("field-provider".to_owned()),
            external_message_delivery_policy: None,
            model_config: Some(ProfileModelConfigSeed {
                provider: "local".to_owned(),
                model_name: "deterministic".to_owned(),
                base_url: Some("http://127.0.0.1:18082".to_owned()),
                api: Some("chat_completions".to_owned()),
                api_key_env: Some("FIELD_API_KEY".to_owned()),
                temperature_milli: Some(500),
                max_output_tokens: Some(1024),
            }),
            brain: Some(ProfileBrainMetadata {
                module: Some("chat-completions".to_owned()),
                strategy: Some("default".to_owned()),
            }),
            mcp_bindings: vec![CreateProfileMcpBindingRequest {
                server_id: "den".to_owned(),
                binding_id: Some("field-created-mcp".to_owned()),
                adapter_id: Some("mcp".to_owned()),
                server_names: Some(vec!["den".to_owned(), "project".to_owned()]),
                transport: Some("streamable_http".to_owned()),
                tool_profile_key: Some("planner".to_owned()),
            }],
            mcp_tool_profile: Some("planner".to_owned()),
            source: Some(CreateProfileSourceRequest {
                template_id: Some("default".to_owned()),
                source_profile_id: Some(ProfileId::new("source-profile")),
                source_bundle_path: Some("/tmp/source-profile.bundle".to_owned()),
            }),
            now: Some("2026-07-07T00:00:00Z".to_owned()),
            profile_file_exists: false,
        },
    }
}

fn sample_runtime_config_draft() -> RuntimeConfigDraft {
    RuntimeConfigDraft {
        profiles_dir: "/home/system/rusty-crew/config/profiles".to_owned(),
        skills_dir: Some("/home/system/rusty-crew/config/skills".to_owned()),
        brains: vec![sample_brain_config_draft()],
        sessions: vec![sample_session_config_draft()],
        scheduled_jobs: vec![sample_scheduled_job_config_draft()],
        channel_bindings: vec![sample_channel_binding_config_draft()],
        mcp_bindings: vec![sample_mcp_binding_config_draft()],
    }
}

fn sample_runtime_config_validation_result() -> RuntimeConfigValidationResult {
    RuntimeConfigValidationResult {
        diagnostics: vec![sample_runtime_config_diagnostic()],
    }
}

fn sample_runtime_config_diagnostic() -> RuntimeConfigDiagnostic {
    RuntimeConfigDiagnostic {
        severity: RuntimeConfigDiagnosticSeverity::Info,
        code: "validation_fixture".to_owned(),
        path: Some("runtimeConfig.sessions[0]".to_owned()),
        message: "Validation runtime config diagnostic.".to_owned(),
    }
}

fn sample_runtime_config_plan() -> RuntimeConfigPlan {
    rusty_crew_core_config::plan_runtime_config(&sample_runtime_config_validation_input())
}

fn sample_brain_config_draft() -> BrainConfigDraft {
    BrainConfigDraft {
        implementation_id: BrainImplementationId::new("field-sample-brain"),
        profile_id: ProfileId::new("field-sample-profile"),
    }
}

fn sample_session_config_draft() -> SessionConfigDraft {
    SessionConfigDraft {
        session_id: SessionId::new("field-sample-session"),
        agent_id: AgentId::new("field-sample-agent"),
        profile_id: ProfileId::new("field-sample-profile"),
        kind: SessionKind::Full,
        workspace_cwd: Some("/tmp/field-sample-workspace".to_owned()),
        resource_limits: Some(sample_resource_limits()),
        owner_id: Some("field-owner".to_owned()),
        history_window: Some(SessionHistoryWindow {
            max_messages: Some(128),
        }),
        max_history_messages: Some(256),
    }
}

fn sample_scheduled_job_config_draft() -> ScheduledJobConfigDraft {
    ScheduledJobConfigDraft {
        id: "field-sample-job".to_owned(),
        schedule: "*/5 * * * *".to_owned(),
        shape: ScheduledJobShape::SessionWake,
        job_kind: Some("runtime.review.memory_skills".to_owned()),
        target_session_id: Some(SessionId::new("field-sample-session")),
        script: Some("field-script".to_owned()),
        delivery_channel_id: Some("field-delivery-channel".to_owned()),
    }
}

fn sample_channel_binding_config_draft() -> ChannelBindingConfigDraft {
    ChannelBindingConfigDraft {
        binding_id: "field-channel-binding".to_owned(),
        adapter_id: AdapterId::new("den"),
        provider: "den".to_owned(),
        agent_id: AgentId::new("field-sample-agent"),
        instance_id: Some(AgentInstanceId::new("field-instance")),
        session_id: Some(SessionId::new("field-sample-session")),
        profile_id: ProfileId::new("field-sample-profile"),
        external_channel_id: "40".to_owned(),
        external_thread_id: Some("field-thread".to_owned()),
        external_user_id: Some("field-user".to_owned()),
        conversation_project_id: Some("asha".to_owned()),
        conversation_channel_id: Some(40),
        provider_subscription_id: Some("field-subscription".to_owned()),
        status: ExternalBindingStatusDraft::Active,
    }
}

fn sample_mcp_binding_config_draft() -> McpBindingConfigDraft {
    McpBindingConfigDraft {
        binding_id: "field-mcp-binding".to_owned(),
        adapter_id: AdapterId::new("mcp"),
        agent_id: AgentId::new("field-sample-agent"),
        instance_id: Some(AgentInstanceId::new("field-instance")),
        session_id: Some(SessionId::new("field-sample-session")),
        profile_id: ProfileId::new("field-sample-profile"),
        server_names: vec!["den".to_owned(), "project".to_owned()],
        endpoint_ref: "config://mcp/den".to_owned(),
        transport: "streamable_http".to_owned(),
        tool_profile_key: "planner".to_owned(),
        status: ExternalBindingStatusDraft::Active,
    }
}

fn sample_scheduled_job_summary_value() -> Value {
    json!({
        "job_id": "scheduled-validation",
        "job_kind": "wake",
        "target_session_id": sample_session_id().to_string(),
        "interval_ms": 300000,
        "next_due_at": "2026-07-09T12:00:00.000Z",
        "status": "active",
        "created_at": sample_timestamp(),
        "updated_at": sample_timestamp(),
        "paused_at": null
    })
}

fn sample_scheduled_run_summary_value() -> Value {
    json!({
        "run_id": "scheduled-validation:1",
        "job_id": "scheduled-validation",
        "job_kind": "wake",
        "target_session_id": sample_session_id().to_string(),
        "status": "completed",
        "trigger": "manual",
        "scheduled_for": "2026-07-09T12:00:00.000Z",
        "claimed_at": sample_timestamp(),
        "claim_deadline_at": "2026-07-09T12:05:00.000Z",
        "completed_at": sample_timestamp(),
        "error": null,
        "output": {"requestedWake": true},
        "created_at": sample_timestamp(),
        "updated_at": sample_timestamp()
    })
}

fn sample_scheduler_tick_report_value() -> Value {
    json!({
        "stale_runs_expired": 1,
        "due_runs_claimed": 2,
        "wakes_requested": 1,
        "runs_completed": 1,
        "runs_skipped": 0,
        "runs_failed": 0
    })
}

fn sample_profile_runtime_metadata() -> ProfileRuntimeMetadata {
    ProfileRuntimeMetadata {
        profile_id: ProfileId::new("field-sample-profile"),
        brain: Some(ProfileBrainMetadata {
            module: Some("chat-completions".to_owned()),
            strategy: Some("default".to_owned()),
        }),
        runtime: Some(ProfileRuntimeOptions {
            default_resource_limits: Some(sample_resource_limits()),
            max_tokens_per_turn: Some(4096),
        }),
        session_defaults: Some(ProfileSessionDefaults {
            owner_id: Some("field-owner".to_owned()),
            max_history_messages: Some(512),
        }),
        mcp_config: Some(ProfileMcpConfig {
            binding_id: Some("field-mcp-binding".to_owned()),
            endpoint_ref: Some("config://mcp/den".to_owned()),
            server_names: vec!["den".to_owned(), "project".to_owned()],
            transport: Some("streamable_http".to_owned()),
            tool_profile: Some("planner".to_owned()),
        }),
        background_review: Some(ProfileBackgroundReviewConfig {
            enabled: true,
            review_type: Some(ProfileBackgroundReviewType::Combined),
            schedule: Some("0 * * * *".to_owned()),
            ..ProfileBackgroundReviewConfig::default()
        }),
        channel_defaults: Some(ProfileChannelDefaults {
            wake_policy: Some(ChannelWakePolicy::Subscription),
        }),
        context_policy: Some(ProfileContextPolicy {
            enabled: true,
            strategy_id: "recent_window".to_owned(),
            auto_compaction_enabled: false,
            compact_at_percent: 80,
            target_percent_after_compaction: 55,
            max_context_percent_for_wake: 95,
            debug_visibility: "status".to_owned(),
            include_debug_events_in_model_context: false,
            strategy_config: json!({}),
        }),
    }
}

fn sample_resource_limits() -> ResourceLimits {
    ResourceLimits {
        max_duration_ms: Some(3_600_000),
        max_delegation_depth: Some(4),
    }
}

fn sample_runtime_database_size_value() -> Value {
    json!({
        "databaseBytes": 4096,
        "pageCount": 4,
        "pageSizeBytes": 1024,
        "freelistPages": 0,
        "freelistBytes": 0,
        "walBytes": 0
    })
}

fn sample_runtime_storage_diagnostics_value() -> Value {
    json!({
        "backend": "sqlite",
        "backendLabel": "SQLite",
        "schemaVersion": 12,
        "supportedSchemaVersion": 12,
        "migrations": [
            {
                "version": 12,
                "description": "validation migration",
                "appliedAt": sample_timestamp()
            }
        ],
        "size": sample_runtime_database_size_value(),
        "tableCounts": [
            {
                "table": "sessions",
                "rows": 3
            }
        ],
        "capabilities": [
            {
                "name": "json",
                "supported": true,
                "detail": "JSON functions available."
            }
        ],
        "repositoryGroups": [
            {
                "groupId": "scheduler",
                "label": "Scheduler",
                "correctnessSensitive": true,
                "backendRequirements": [
                    {
                        "capability": "transactions",
                        "required": true,
                        "detail": "Scheduler claims require transactions."
                    }
                ],
                "notes": ["validation fixture"]
            }
        ],
        "connectionHealth": {
            "backend": "sqlite",
            "status": "ready",
            "maxConnections": 1,
            "activeConnections": 0,
            "idleConnections": 1,
            "totalOpened": 1,
            "checkoutCount": 3,
            "checkoutReuseCount": 2,
            "reconnectAttempts": 0,
            "reconnectSuccesses": 0,
            "closedConnectionsDiscarded": 0,
            "lastError": null
        },
        "moduleRegistry": {
            "source": "validation",
            "backendCapabilities": ["sqlite"],
            "modules": [],
            "orphanInstalledModules": []
        },
        "indexChecks": [
            {
                "name": "scheduler_due_runs",
                "usesIndex": true,
                "detail": "validation query plan"
            }
        ],
        "searchHealthy": true,
        "pressureSignals": [
            {
                "name": "database_size",
                "active": false,
                "severity": "info",
                "observedValue": 4096,
                "thresholdValue": 1048576,
                "detail": "below threshold"
            }
        ],
        "pressure": false
    })
}

fn sample_runtime_maintenance_policy_value() -> Value {
    json!({
        "expireQueuedMessagesAt": sample_timestamp(),
        "purgeTerminalQueuedMessagesBefore": sample_timestamp(),
        "expireProviderWireStatesAt": sample_timestamp(),
        "compactSessionMemoryAt": sample_timestamp(),
        "sessionMemoryMaxActiveRecordsPerScope": 64,
        "sessionMemoryArchiveBatchSize": 16,
        "runWalCheckpoint": true,
        "runOptimize": true
    })
}

fn sample_runtime_maintenance_report_value() -> Value {
    json!({
        "sizeBefore": sample_runtime_database_size_value(),
        "sizeAfter": sample_runtime_database_size_value(),
        "expiredQueueMessages": 1,
        "purgedTerminalQueueMessages": 2,
        "expiredProviderWireStates": 3,
        "sessionMemoryCompaction": {
            "enabled": true,
            "scopesInspected": 4,
            "retentionPressureScopes": 1,
            "scopesCompacted": 1,
            "sessionSummariesCreated": 1,
            "branchSummariesCreated": 0,
            "recordsArchived": 2,
            "recordsSuperseded": 1,
            "skippedScopes": 0
        },
        "walCheckpointRan": true,
        "optimizeRan": true
    })
}

fn bridge_validation_fixture_file() -> Result<BridgeValidationFixtureFile> {
    Ok(BridgeValidationFixtureFile {
        format_version: 1,
        manifest_version: MANIFEST_VERSION,
        operation_count: manifest_operation_count()?,
        schema_source: "rusty-crew-core-protocol serde wire fixtures".to_owned(),
        fixtures: vec![
            BridgeValidationFixture {
                name: "body_state_v1".to_owned(),
                operation: "project_body_state_json".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::BodyState".to_owned(),
                value: serde_json::to_value(sample_body_state())?,
            },
            BridgeValidationFixture {
                name: "list_sessions_v1".to_owned(),
                operation: "list_sessions".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "Vec<rusty_crew_core_protocol::SessionState>".to_owned(),
                value: serde_json::to_value(vec![sample_session_state()])?,
            },
            BridgeValidationFixture {
                name: "buffered_brain_run_drain_v1".to_owned(),
                operation: "drain_brain_run".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_brain_runtime::BufferedBrainRunDrain".to_owned(),
                value: serde_json::to_value(BufferedBrainRunDrain {
                    module_id: "openai-responses".to_owned(),
                    wake_id: "validation-wake".to_owned(),
                    items: sample_brain_wake_stream(),
                    tool_requests: vec![],
                    stream_retention_metrics: brain_runtime::BufferedBrainStreamRetentionMetrics {
                        raw_stream_item_count: 3,
                        raw_delta_item_count: 0,
                        retained_stream_item_count: 3,
                        coalesced_delta_item_count: 0,
                        dropped_stream_item_count: 0,
                        retained_delta_bytes: 0,
                        queued_delta_bytes: 0,
                        max_stream_items: 4_096,
                        max_stream_delta_bytes: 8 * 1_024 * 1_024,
                    },
                    terminal: true,
                    attention: None,
                    provider_state: None,
                    transport_metrics: Some(json!({
                        "effectiveTransport": "responses",
                        "selectedStrategyId": "responses-replay-v1",
                        "effectiveStrategyId": "responses-replay-v1",
                        "providerRequestCount": 1,
                        "continuationRoundCount": 0,
                        "providerRequestPayloadBytes": 512,
                        "providerEventCounts": {"response.completed": 1},
                        "totalTurnDurationMs": 25
                    })),
                    credential_secret_update: None,
                    cancellation: None,
                    error: None,
                })?,
            },
            BridgeValidationFixture {
                name: "profile_registry_record_v1".to_owned(),
                operation: "list_profile_registry_records".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::ProfileRegistryRecord".to_owned(),
                value: serde_json::to_value(sample_profile_registry_record())?,
            },
            BridgeValidationFixture {
                name: "model_provider_record_v1".to_owned(),
                operation: "list_model_providers".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::ModelProviderRecord".to_owned(),
                value: serde_json::to_value(sample_model_provider_record())?,
            },
            BridgeValidationFixture {
                name: "model_provider_refresh_impact_v1".to_owned(),
                operation: "model_provider_refresh_impact".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::ModelProviderRefreshImpact".to_owned(),
                value: serde_json::to_value(sample_model_provider_refresh_impact())?,
            },
            BridgeValidationFixture {
                name: "memory_space_descriptor_v1".to_owned(),
                operation: "list_memory_space_descriptors".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::MemorySpaceDescriptor".to_owned(),
                value: serde_json::to_value(sample_memory_space_descriptor())?,
            },
            BridgeValidationFixture {
                name: "memory_proposal_record_v1".to_owned(),
                operation: "list_memory_proposals".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::MemoryProposalRecord".to_owned(),
                value: serde_json::to_value(sample_memory_proposal_record())?,
            },
            BridgeValidationFixture {
                name: "memory_governance_decision_record_v1".to_owned(),
                operation: "record_memory_governance_decision".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::MemoryGovernanceDecisionRecord".to_owned(),
                value: serde_json::to_value(sample_memory_governance_decision_record())?,
            },
            BridgeValidationFixture {
                name: "session_activity_digest_v1".to_owned(),
                operation: "list_session_activity_digests".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::SessionActivityDigest".to_owned(),
                value: serde_json::to_value(sample_session_activity_digest())?,
            },
            BridgeValidationFixture {
                name: "context_compaction_artifact_v1".to_owned(),
                operation: "list_context_compaction_artifacts".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "rusty_crew_core_protocol::ContextCompactionArtifact".to_owned(),
                value: serde_json::to_value(sample_context_compaction_artifact())?,
            },
        ],
    })
}

fn operation_names_from_manifest(manifest_text: &str) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let mut in_operation = false;

    for line in manifest_text.lines() {
        let trimmed = line.trim();
        if trimmed == "[[operation]]" {
            in_operation = true;
            continue;
        }
        if !in_operation || !trimmed.starts_with("name = ") {
            continue;
        }
        names.push(parse_quoted_assignment_value(trimmed, "name")?);
        in_operation = false;
    }

    ensure_no_duplicate_operations("bridge manifest", &names)?;
    Ok(names)
}

fn operation_names_from_native_json_methods(source: &str) -> Result<Vec<String>> {
    let names = native_json_method_signatures(
        source,
        "export declare class NativeBridgeBinding {",
        "generated napi NativeBridgeBinding declaration",
    )?
    .into_iter()
    .map(|signature| signature.operation_name)
    .collect::<Vec<_>>();
    ensure_no_duplicate_operations("generated napi NativeBridgeBinding *Json methods", &names)?;
    Ok(names)
}

fn native_json_method_signatures(
    source: &str,
    marker: &str,
    label: &str,
) -> Result<Vec<NativeJsonMethodSignature>> {
    let block = native_bridge_binding_block(source, marker, label)?;
    let lines = block.lines().collect::<Vec<_>>();
    let mut signatures = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        let Some(open_paren) = trimmed.find('(') else {
            index += 1;
            continue;
        };
        let method_name = trimmed[..open_paren].trim();
        if !method_name.ends_with("Json") || !is_camel_method_name(method_name) {
            index += 1;
            continue;
        }

        let mut declaration = trimmed.to_owned();
        while !declaration.contains("):") && index + 1 < lines.len() {
            index += 1;
            declaration.push(' ');
            declaration.push_str(lines[index].trim());
        }
        let Some(close_params) = declaration.find("):") else {
            bail!("{label} method `{method_name}` is missing a return type marker");
        };
        let params = &declaration[open_paren + 1..close_params];
        let return_source = declaration[close_params + 2..].trim();
        let signature = NativeJsonMethodSignature {
            operation_name: camel_json_method_to_operation_name(method_name)?,
            method_name: method_name.to_owned(),
            parameter_count: count_signature_parameters(params),
            return_kind: return_kind(return_source),
        };
        signatures.push(signature);
        index += 1;
    }

    let names = signatures
        .iter()
        .map(|signature| signature.operation_name.clone())
        .collect::<Vec<_>>();
    ensure_no_duplicate_operations(label, &names)?;
    Ok(signatures)
}

fn native_bridge_binding_block<'a>(source: &'a str, marker: &str, label: &str) -> Result<&'a str> {
    let start = source
        .find(marker)
        .with_context(|| format!("failed to find {marker:?} in {label}"))?
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find("\n}")
        .with_context(|| format!("failed to find end of NativeBridgeBinding in {label}"))?;
    Ok(&rest[..end])
}

fn is_camel_method_name(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn count_signature_parameters(params: &str) -> usize {
    let trimmed = params.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let mut depth = 0_u32;
    let mut count = 1_usize;
    for ch in trimmed.chars() {
        match ch {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

fn normalize_signature_source(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn return_kind(return_source: &str) -> String {
    let trimmed = return_source.trim_start();
    if trimmed.starts_with('{') {
        return "object".to_owned();
    }
    let kind = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '[' || *ch == ']')
        .collect::<String>();
    match kind.as_str() {
        "Buffer" => "Uint8Array".to_owned(),
        value if value.starts_with("Js") => "object".to_owned(),
        _ => kind,
    }
}

fn compare_native_json_method_signatures(
    native: &[NativeJsonMethodSignature],
    ts: &[NativeJsonMethodSignature],
    native_index_path: &Path,
    ts_binding_path: &Path,
) -> Result<()> {
    let native_ops = native
        .iter()
        .map(|signature| signature.operation_name.clone())
        .collect::<Vec<_>>();
    let ts_ops = ts
        .iter()
        .map(|signature| signature.operation_name.clone())
        .collect::<Vec<_>>();
    compare_operation_sets(
        &format!("generated napi declaration {}", native_index_path.display()),
        &native_ops,
        &format!(
            "TypeScript raw binding interface {}",
            ts_binding_path.display()
        ),
        &ts_ops,
    )?;

    let ts_by_operation = ts
        .iter()
        .map(|signature| (signature.operation_name.as_str(), signature))
        .collect::<BTreeMap<_, _>>();
    for native_signature in native {
        let Some(ts_signature) = ts_by_operation.get(native_signature.operation_name.as_str())
        else {
            continue;
        };
        if native_signature.method_name != ts_signature.method_name
            || native_signature.parameter_count != ts_signature.parameter_count
            || native_signature.return_kind != ts_signature.return_kind
        {
            bail!(
                "native raw binding signature drift for operation `{}`; generated declaration has {}({} params) -> {}, TypeScript interface has {}({} params) -> {}",
                native_signature.operation_name,
                native_signature.method_name,
                native_signature.parameter_count,
                native_signature.return_kind,
                ts_signature.method_name,
                ts_signature.parameter_count,
                ts_signature.return_kind
            );
        }
    }

    Ok(())
}

fn ensure_family_operations_exist(label: &str, operations: &[&str]) -> Result<()> {
    let manifest = operation_names_from_manifest(MANIFEST_TEXT)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let missing = operations
        .iter()
        .filter(|operation| !manifest.contains(**operation))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!("native mapping inventory family `{label}` references operations missing from bridge manifest: {missing:?}");
    }
    Ok(())
}

fn operation_name_to_camel_json_method(operation_name: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = false;
    for ch in operation_name.chars() {
        if ch == '_' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            output.push(ch.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            output.push(ch);
        }
    }
    output.push_str("Json");
    output
}

fn operation_name_to_native_method(operation_name: &str) -> String {
    operation_name_to_camel_json_method(operation_name)
}

fn operation_name_to_camel_wrapper(operation_name: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = false;
    for ch in operation_name.chars() {
        if ch == '_' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            output.push(ch.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            output.push(ch);
        }
    }
    output
}

fn camel_json_method_to_operation_name(method_name: &str) -> Result<String> {
    let stem = method_name
        .strip_suffix("Json")
        .with_context(|| format!("native method `{method_name}` does not end with Json"))?;
    let mut output = String::new();
    for (index, ch) in stem.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            output.push(ch);
        } else {
            bail!("native method `{method_name}` contains unsupported character `{ch}`");
        }
    }
    Ok(output)
}

fn camel_method_to_operation_name(method_name: &str) -> Result<String> {
    let stem = method_name.strip_suffix("Json").unwrap_or(method_name);
    if stem.is_empty() || !is_camel_method_name(stem) {
        bail!("invalid native binding method name `{method_name}`");
    }
    let mut output = String::new();
    for (index, ch) in stem.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            output.push(ch);
        } else {
            bail!("invalid native binding method character `{ch}` in `{method_name}`");
        }
    }
    Ok(output)
}

fn parse_quoted_assignment_value(line: &str, key: &str) -> Result<String> {
    let prefix = format!("{key} = \"");
    let value = line
        .strip_prefix(&prefix)
        .with_context(|| format!("expected `{key} = \"...\"` assignment, got `{line}`"))?;
    let end_quote = value
        .find('"')
        .with_context(|| format!("missing closing quote in assignment `{line}`"))?;
    Ok(value[..end_quote].to_owned())
}

fn ensure_no_duplicate_operations(label: &str, names: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let duplicates = names
        .iter()
        .filter(|name| !seen.insert((*name).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if !duplicates.is_empty() {
        bail!("{label} has duplicate bridge operations: {duplicates:?}");
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn compare_operation_sets(
    left_label: &str,
    left: &[String],
    right_label: &str,
    right: &[String],
) -> Result<()> {
    let left_set = left.iter().cloned().collect::<BTreeSet<_>>();
    let right_set = right.iter().cloned().collect::<BTreeSet<_>>();
    if left_set == right_set {
        return Ok(());
    }

    let missing_from_left = right_set.difference(&left_set).cloned().collect::<Vec<_>>();
    let extra_in_left = left_set.difference(&right_set).cloned().collect::<Vec<_>>();
    bail!(
        "bridge operation inventory drift between {left_label} and {right_label}; missing from {left_label}: {missing_from_left:?}; extra in {left_label}: {extra_in_left:?}"
    );
}

fn ensure_operations_cover_native_json_methods(
    manifest_label: &str,
    manifest_names: &[String],
    native_label: &str,
    native_names: &[String],
) -> Result<()> {
    let manifest_set = manifest_names.iter().cloned().collect::<BTreeSet<_>>();
    let missing_from_manifest = native_names
        .iter()
        .filter(|name| !manifest_set.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if missing_from_manifest.is_empty() {
        return Ok(());
    }

    bail!(
        "bridge native surface inventory drift between {manifest_label} and {native_label}; missing from {manifest_label}: {missing_from_manifest:?}"
    );
}

fn compare_operation_order(
    left_label: &str,
    left: &[String],
    right_label: &str,
    right: &[String],
) -> Result<()> {
    if left == right {
        return Ok(());
    }

    let first_diff = left
        .iter()
        .zip(right.iter())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| left.len().min(right.len()));
    let left_value = left.get(first_diff).map_or("<missing>", String::as_str);
    let right_value = right.get(first_diff).map_or("<missing>", String::as_str);
    let left_set = left.iter().cloned().collect::<BTreeSet<_>>();
    let right_set = right.iter().cloned().collect::<BTreeSet<_>>();
    let missing_from_left = right_set.difference(&left_set).cloned().collect::<Vec<_>>();
    let extra_in_left = left_set.difference(&right_set).cloned().collect::<Vec<_>>();
    bail!(
        "bridge operation inventory drift between {left_label} and {right_label}; first difference at index {first_diff}: {left_label} has `{left_value}`, {right_label} has `{right_value}`; missing from {left_label}: {missing_from_left:?}; extra in {left_label}: {extra_in_left:?}"
    );
}

fn sample_body_state() -> BodyState {
    BodyState {
        session: sample_session_state(),
        pending_messages: vec![sample_agent_message()],
        recent_events: vec![CoreEvent::BrainWakeRequested {
            session_id: sample_session_id(),
        }],
        child_completions: vec![],
        fan_out_groups: vec![],
        delta_policy: BodyDeltaPolicy {
            mode: MidTurnDeltaMode::FrozenSnapshotNextWake,
            queue_owner: DeltaQueueOwner::Body,
            queued_message_ttl_ms: 30_000,
            max_queued_messages: 20,
        },
    }
}

fn sample_brain_wake_stream() -> Vec<BrainWakeStreamItem> {
    vec![
        BrainWakeStreamItem::event(BrainEventEnvelope {
            wake_id: "validation-wake".to_owned(),
            session_id: sample_session_id(),
            event: BrainEvent::Started,
        }),
        BrainWakeStreamItem::actions(BrainActionBatch {
            wake_id: "validation-wake".to_owned(),
            session_id: sample_session_id(),
            actions: vec![BrainAction::SendMessage {
                message: sample_agent_message(),
            }],
        }),
    ]
}

fn sample_brain_implementation_registration() -> BrainImplementationRegistration {
    BrainImplementationRegistration {
        implementation_id: BrainImplementationId::new("validation-brain"),
        profile_id: sample_profile_id(),
        tool_profile: sample_tool_profile(),
        model_config: sample_brain_model_config(),
        strategy: Some(sample_brain_strategy_metadata()),
        provider_state_scope: Some(sample_brain_provider_state_scope()),
    }
}

fn sample_brain_model_config() -> BrainModelConfig {
    BrainModelConfig {
        model_config_id: None,
        model_config_revision: None,
        endpoint_id: None,
        endpoint_revision: None,
        credential_id: None,
        credential_revision: None,
        credential_kind: None,
        protocol: None,
        dialect: None,
        auth_scheme: None,
        prompt_cache_transport: None,
        provider: "openai-compatible".to_owned(),
        model_name: "gpt-fixture".to_owned(),
        temperature_milli: Some(500),
        max_output_tokens: Some(2048),
    }
}

fn sample_brain_strategy_metadata() -> BrainStrategyMetadata {
    BrainStrategyMetadata {
        module_id: "openai-responses".to_owned(),
        strategy_id: "responses-replay-v1".to_owned(),
        provider_state: BrainProviderStateStrategyMetadata {
            mode: ProviderStateMode::Optional,
        },
    }
}

fn sample_brain_provider_state_scope() -> BrainProviderStateScope {
    BrainProviderStateScope {
        profile_fingerprint: "profile-fingerprint-v1".to_owned(),
        provider_fingerprint: "provider-fingerprint-v1".to_owned(),
        compatibility: None,
    }
}

fn sample_brain_wake_request() -> BrainWakeRequest {
    BrainWakeRequest {
        brain: BrainImplementationHandle::new(9),
        session_id: sample_session_id(),
        body_state: RuntimeBufferHandle::new(42),
        system_prompt: RuntimeBufferHandle::new(43),
        role_assembly: RuntimeBufferHandle::new(44),
        wake_id: "validation-wake".to_owned(),
        continuation_state: None,
        provider_state: Some(sample_brain_provider_state_input()),
        provider_state_absence: None,
        compaction_intent: None,
    }
}

fn sample_brain_wake_accepted() -> BrainWakeAccepted {
    BrainWakeAccepted {
        wake_id: "validation-wake".to_owned(),
        accepted: true,
        outcome: BrainWakeOutcome::Completed,
    }
}

fn sample_brain_event_envelope() -> BrainEventEnvelope {
    BrainEventEnvelope {
        wake_id: "validation-wake".to_owned(),
        session_id: sample_session_id(),
        event: BrainEvent::ToolCallStarted {
            tool_name: "read_file".to_owned(),
            metadata: Some(sample_tool_call_metadata()),
        },
    }
}

fn sample_brain_action_batch() -> BrainActionBatch {
    BrainActionBatch {
        wake_id: "validation-wake".to_owned(),
        session_id: sample_session_id(),
        actions: vec![sample_brain_action()],
    }
}

fn sample_brain_action() -> BrainAction {
    BrainAction::RequestDelegation {
        profile_id: sample_profile_id(),
        task_id: Some(TaskId::new("task-validation")),
        prompt: "Review bridge mapping inventory coverage.".to_owned(),
        expected_output: Some("Findings and validation evidence.".to_owned()),
        resource_limits: Some(sample_resource_limits()),
        workspace_constraint: Some(DelegatedWorkspaceConstraint {
            cwd: "/home/dev/rusty-crew".to_owned(),
        }),
        timeout_ms: Some(300_000),
        priority: Some(DelegationPriority::Normal),
        fan_out_group_id: Some("bridge-validation-group".to_owned()),
        fan_out_max_concurrency: Some(2),
        fan_out_failure_policy: Some(FanOutFailurePolicy::FailSoft),
        correlation_id: Some("validation-correlation".to_owned()),
        parent_consumption: Some(ParentConsumptionPolicy::AwaitCompletion),
        capacity_request: Some(WorkerPoolCapacityRequest {
            member_id: "bridge-worker".to_owned(),
            claim_ttl_ms: Some(60_000),
            fallback_policy: WorkerPoolCapacityFallbackPolicy::RejectOnNoCapacity,
        }),
    }
}

fn sample_brain_wake_failure() -> BrainWakeFailure {
    BrainWakeFailure {
        wake_id: "validation-wake".to_owned(),
        session_id: sample_session_id(),
        kind: CoreErrorKind::InternalError,
        reason_code: Some("validation_failure".to_owned()),
        message: "Validation fixture wake failed.".to_owned(),
    }
}

fn sample_brain_provider_state_input() -> BrainWakeProviderStateInput {
    BrainWakeProviderStateInput {
        module_id: "openai-responses".to_owned(),
        strategy_id: "responses-replay-v1".to_owned(),
        profile_fingerprint: "profile-fingerprint-v1".to_owned(),
        provider_fingerprint: "provider-fingerprint-v1".to_owned(),
        payload_version: "responses-state-v1".to_owned(),
        payload: json!({
            "previous_response_id": "resp_validation",
            "conversation": "validation"
        }),
        expires_at: Some("2026-08-01T00:00:00.000Z".to_owned()),
    }
}

fn sample_brain_provider_state_update() -> BrainWakeProviderStateUpdate {
    BrainWakeProviderStateUpdate {
        module_id: "openai-responses".to_owned(),
        strategy_id: "responses-replay-v1".to_owned(),
        profile_fingerprint: "profile-fingerprint-v1".to_owned(),
        provider_fingerprint: "provider-fingerprint-v1".to_owned(),
        payload_version: "responses-state-v2".to_owned(),
        payload: json!({
            "previous_response_id": "resp_validation_next",
            "conversation": "validation"
        }),
        ttl_ms: Some(86_400_000),
    }
}

fn sample_brain_provider_state_output() -> BrainWakeProviderStateOutput {
    BrainWakeProviderStateOutput::Replace {
        state: sample_brain_provider_state_update(),
    }
}

fn sample_session_state() -> SessionState {
    SessionState {
        handle: SessionHandle::new(1),
        session_id: sample_session_id(),
        agent_id: sample_agent_id(),
        profile_id: sample_profile_id(),
        kind: SessionKind::Full,
        delegation: None,
        workspace: Some(rusty_crew_core_protocol::SessionWorkspace {
            cwd: "/home".to_owned(),
            revision: 1,
            updated_at: sample_timestamp(),
        }),
        resource_limits: ResourceLimits {
            max_duration_ms: None,
            max_delegation_depth: Some(3),
        },
        tool_profile: sample_tool_profile(),
        history_window: Some(SessionHistoryWindow {
            max_messages: Some(200),
        }),
        inference_overrides: Default::default(),
        status: SessionStatus::Idle,
        brain_turn_count: 7,
        created_at: sample_timestamp(),
        last_active_at: sample_timestamp(),
    }
}

fn sample_tool_profile() -> ToolProfile {
    ToolProfile {
        tools: vec![ToolDescriptor {
            name: "send_message".to_owned(),
            description: "Send a direct runtime message.".to_owned(),
            input_schema: Some(RuntimeBufferHandle::new(42)),
        }],
    }
}

fn sample_agent_message() -> AgentMessage {
    AgentMessage {
        from: sample_agent_id(),
        to: AgentId::new("operator"),
        from_session_id: Some(sample_session_id()),
        to_session_id: None,
        body: "Bridge validation fixture message.".to_owned(),
        correlation_id: Some("validation-correlation".to_owned()),
        projection: None,
    }
}

fn sample_tool_call_metadata() -> ToolCallMetadata {
    ToolCallMetadata {
        source: ToolCallSource::Mcp,
        adapter_id: Some(AdapterId::new("den")),
        binding_id: Some("den-mcp".to_owned()),
        server_names: vec!["den".to_owned(), "project".to_owned()],
        profile_id: Some(sample_profile_id()),
        tool_profile_key: Some("planner".to_owned()),
        source_tool_name: Some("den.get_task".to_owned()),
        catalog_revision: Some("catalog-revision-validation".to_owned()),
        debug_detail_id: Some("debug-detail-validation".to_owned()),
        policy: Some(ToolCallPolicyMetadata {
            allowed: Some(true),
            denial_reason: None,
            timeout_ms: Some(30_000),
            cancelled: Some(false),
            archive_cleanup: Some(false),
        }),
    }
}

fn sample_completion_packet() -> CompletionPacket {
    CompletionPacket {
        session_id: sample_session_id(),
        status: CompletionStatus::Completed,
        summary: "Delegated validation completed.".to_owned(),
    }
}

fn sample_runtime_buffer_view_value() -> Value {
    json!({
        "handle": 42,
        "media_type": APPLICATION_JSON,
        "byte_len": 64,
        "bytes": [123, 34, 111, 107, 34, 58, 116, 114, 117, 101, 125]
    })
}

fn sample_native_provider_state_diagnostic_value() -> Value {
    json!({
        "sessionId": sample_session_id().to_string(),
        "moduleId": "openai-responses",
        "strategyId": "responses-replay-v1",
        "status": "valid",
        "payloadVersion": "responses-state-v1",
        "payloadBytes": 128,
        "createdAt": sample_timestamp(),
        "updatedAt": sample_timestamp(),
        "expiresAt": "2026-08-01T00:00:00.000Z",
        "lastWakeId": "validation-wake",
        "invalidatedAt": null,
        "invalidationReason": null
    })
}

fn sample_buffered_brain_run_diagnostics_value() -> Value {
    json!({
        "active_run_count": 1,
        "modules": [
            {
                "module_label": "OpenAI Responses",
                "active_run_count": 1
            }
        ],
        "runs": [
            {
                "module_label": "OpenAI Responses",
                "wake_id": "validation-wake",
                "queued_stream_item_count": 2,
                "stream_retention_metrics": {
                    "raw_stream_item_count": 5,
                    "raw_delta_item_count": 3,
                    "retained_stream_item_count": 4,
                    "coalesced_delta_item_count": 1,
                    "dropped_stream_item_count": 0,
                    "retained_delta_bytes": 128,
                    "queued_delta_bytes": 64,
                    "max_stream_items": 4096,
                    "max_stream_delta_bytes": 8388608
                },
                "pending_tool_request_count": 1,
                "submitted_tool_output_count": 0,
                "age_ms": 250,
                "terminal": false,
                "cancelled": false,
                "has_error": false,
                "started_at": sample_timestamp(),
                "last_transition_at": sample_timestamp()
            }
        ]
    })
}

fn sample_buffered_brain_run_cleanup_summary_value() -> Value {
    json!({
        "active_runs": 1,
        "terminal_runs": 0,
        "cancelled_nonterminal_runs": 1,
        "removed_runs": 1,
        "modules": [
            {
                "module_label": "OpenAI Responses",
                "active_runs": 1,
                "terminal_runs": 0,
                "cancelled_nonterminal_runs": 1,
                "removed_runs": 1
            }
        ]
    })
}

fn sample_query_page() -> persistence::QueryPage {
    persistence::QueryPage {
        limit: Some(25),
        offset: Some(5),
    }
}

fn sample_profile_memory_record_value() -> Value {
    json!({
        "profileId": sample_profile_id().to_string(),
        "targetType": "user",
        "targetId": "validation-user",
        "key": "validation-memory",
        "content": "Validation profile memory content.",
        "metadataJson": "{\"fixture\":true}",
        "revision": 2,
        "createdAt": sample_timestamp(),
        "updatedAt": sample_timestamp()
    })
}

fn sample_profile_memory_write_value() -> Value {
    json!({
        "profileId": sample_profile_id().to_string(),
        "targetType": "user",
        "targetId": "validation-user",
        "key": "validation-memory",
        "content": "Validation profile memory content.",
        "metadataJson": "{\"fixture\":true}",
        "now": sample_timestamp(),
        "caps": {
            "maxRecordsPerProfile": 64,
            "maxKeyBytes": 128,
            "maxContentBytes": 8192
        }
    })
}

fn sample_profile_memory_replace_value() -> Value {
    json!({
        "write": sample_profile_memory_write_value(),
        "expectedRevision": 2
    })
}

fn sample_profile_memory_delete_value() -> Value {
    json!({
        "profileId": sample_profile_id().to_string(),
        "targetType": "user",
        "targetId": "validation-user",
        "key": "validation-memory",
        "expectedRevision": 2
    })
}

fn sample_profile_memory_query_value() -> Value {
    json!({
        "profileId": sample_profile_id().to_string(),
        "targetType": "user",
        "targetId": "validation-user",
        "limit": 25,
        "offset": 5
    })
}

fn sample_session_memory_record() -> persistence::SessionMemoryRecord {
    persistence::SessionMemoryRecord {
        record_id: "validation-session-memory".to_owned(),
        session_id: sample_session_id(),
        scope: MemoryScope {
            scope_type: MemoryScopeType::Session,
            scope_id: sample_session_id().to_string(),
        },
        branch_id: Some(sample_conversation_branch_id()),
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("session_fact"),
            version: 1,
        },
        status: persistence::SessionMemoryRecordStatus::Active,
        revision: 3,
        content: json!({"content": "Validation session memory."}),
        evidence_refs: vec![MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::Wake,
            ref_id: "wake-validation".to_owned(),
            label: Some("Validation wake".to_owned()),
        }],
        source: MemoryProposalSource::CaptureProducer,
        confidence: 0.85,
        durability_rationale: "Validation session memory rationale.".to_owned(),
        supersedes_record_id: Some("validation-old-session-memory".to_owned()),
        superseded_by_record_id: None,
        archived_at: None,
        archive_reason: None,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_session_memory_query() -> persistence::SessionMemoryQuery {
    persistence::SessionMemoryQuery {
        session_id: Some(sample_session_id()),
        branch_id: Some(sample_conversation_branch_id()),
        scope_type: Some(MemoryScopeType::Session),
        shape_id: Some("session_fact".to_owned()),
        include_superseded: true,
        include_archived: false,
        page: Some(sample_query_page()),
    }
}

fn sample_branch_aware_session_memory_query() -> persistence::BranchAwareSessionMemoryQuery {
    persistence::BranchAwareSessionMemoryQuery {
        session_id: sample_session_id(),
        active_branch_id: Some(sample_conversation_branch_id()),
        include_ancestors: true,
        include_siblings: false,
        shape_id: Some("session_fact".to_owned()),
        prompt_context_only: true,
        page: Some(sample_query_page()),
    }
}

fn sample_session_memory_prompt_context() -> persistence::SessionMemoryPromptContext {
    persistence::SessionMemoryPromptContext {
        records: vec![sample_session_memory_record()],
        diagnostics: persistence::SessionMemoryPromptDiagnostics {
            descriptor_id: "session_memory".to_owned(),
            descriptor_schema_version: 1,
            session_id: sample_session_id(),
            active_branch_id: Some(sample_conversation_branch_id()),
            selected_records: vec![persistence::SessionMemorySelectedRecordDiagnostic {
                record_id: "validation-session-memory".to_owned(),
                shape_id: "session_fact".to_owned(),
            }],
            excluded_counts: persistence::SessionMemoryPromptExcludedCounts {
                wrong_branch: 1,
                sibling_branch: 0,
                tool_only: 0,
                archived: 0,
                superseded: 1,
                limit_exceeded: 0,
                policy_disabled: 0,
            },
            character_estimate: 120,
            token_estimate: 32,
            context_policy: persistence::SessionMemoryPromptContextPolicy::SummaryContext,
        },
    }
}

fn sample_message_block_record() -> persistence::MessageBlockRecord {
    persistence::MessageBlockRecord {
        block_id: sample_message_block_id(),
        message_id: sample_message_id(),
        ordinal: 0,
        kind: "text".to_owned(),
        content_json: json!({"text": "Bridge validation block."}),
        render_policy_json: Some(json!({"mode": "plain"})),
        metadata_json: json!({"fixture": true}),
    }
}

fn sample_message_block_write() -> persistence::MessageBlockWrite {
    persistence::MessageBlockWrite {
        block_id: sample_message_block_id(),
        ordinal: 0,
        kind: "text".to_owned(),
        content_json: json!({"text": "Bridge validation block."}),
        render_policy_json: Some(json!({"mode": "plain"})),
        metadata_json: json!({"fixture": true}),
    }
}

fn sample_durable_message_record() -> persistence::DurableMessageRecord {
    persistence::DurableMessageRecord {
        message_id: sample_message_id(),
        session_id: sample_session_id(),
        branch_id: Some(sample_conversation_branch_id()),
        parent_message_id: Some(MessageId::new("validation-parent-message")),
        previous_message_id: Some(MessageId::new("validation-previous-message")),
        author_id: "validation-author".to_owned(),
        author_role: "assistant".to_owned(),
        status: persistence::DurableMessageStatus::Completed,
        body: "Bridge validation durable message.".to_owned(),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        blocks: vec![sample_message_block_record()],
    }
}

fn sample_durable_message_write() -> persistence::DurableMessageWrite {
    persistence::DurableMessageWrite {
        message_id: sample_message_id(),
        session_id: sample_session_id(),
        branch_id: Some(sample_conversation_branch_id()),
        parent_message_id: Some(MessageId::new("validation-parent-message")),
        previous_message_id: Some(MessageId::new("validation-previous-message")),
        author_id: "validation-author".to_owned(),
        author_role: "assistant".to_owned(),
        status: persistence::DurableMessageStatus::Completed,
        body: "Bridge validation durable message.".to_owned(),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        blocks: vec![sample_message_block_write()],
    }
}

fn sample_message_variant_record() -> persistence::MessageVariantRecord {
    persistence::MessageVariantRecord {
        variant_id: sample_message_variant_id(),
        slot_id: sample_message_slot_id(),
        source: persistence::MessageVariantSource::Primary,
        ordinal: 0,
        status: persistence::MessageVariantStatus::Active,
        message: sample_durable_message_record(),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_message_variant_write() -> persistence::MessageVariantWrite {
    persistence::MessageVariantWrite {
        variant_id: sample_message_variant_id(),
        slot_id: sample_message_slot_id(),
        source: persistence::MessageVariantSource::Primary,
        ordinal: 0,
        status: persistence::MessageVariantStatus::Active,
        message: sample_durable_message_write(),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_message_slot_record() -> persistence::MessageSlotRecord {
    persistence::MessageSlotRecord {
        slot_id: sample_message_slot_id(),
        session_id: sample_session_id(),
        primary_variant_id: sample_message_variant_id(),
        active_variant_id: Some(sample_message_variant_id()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        version: 7,
        primary: sample_message_variant_record(),
        alternates: vec![persistence::MessageVariantRecord {
            variant_id: MessageVariantId::new("validation-alternate-variant"),
            source: persistence::MessageVariantSource::Alternate,
            ordinal: 1,
            ..sample_message_variant_record()
        }],
    }
}

fn sample_message_slot_write() -> persistence::MessageSlotWrite {
    persistence::MessageSlotWrite {
        slot_id: sample_message_slot_id(),
        session_id: sample_session_id(),
        primary_variant_id: sample_message_variant_id(),
        active_variant_id: Some(sample_message_variant_id()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_message_slot_query() -> persistence::MessageSlotQuery {
    persistence::MessageSlotQuery {
        session_id: Some(sample_session_id()),
        include_alternates: true,
        page: Some(sample_query_page()),
    }
}

fn sample_message_variant_query() -> persistence::MessageVariantQuery {
    persistence::MessageVariantQuery {
        slot_id: Some(sample_message_slot_id()),
        include_deleted: false,
        page: Some(sample_query_page()),
    }
}

fn sample_create_chat_message_slot_request() -> persistence::CreateChatMessageSlotRequest {
    persistence::CreateChatMessageSlotRequest {
        slot: sample_message_slot_write(),
        primary_variant: sample_message_variant_write(),
        branch_id: sample_conversation_branch_id(),
        expected_branch_head: persistence::BranchHeadExpectation::Message(sample_message_id()),
        updated_at: sample_timestamp(),
        ensure_active_branch: None,
        inherit_branch_head: false,
        idempotency_key: Some("chat-request-alpha".to_string()),
    }
}

fn sample_create_chat_message_slot_result() -> persistence::CreateChatMessageSlotResult {
    persistence::CreateChatMessageSlotResult {
        slot: Some(sample_message_slot_record()),
        branch: sample_conversation_branch_record(),
        conflict: Some(persistence::BranchHeadConflict {
            expected: Some(sample_message_id()),
            actual: Some(MessageId::new("validation-actual-head-message")),
        }),
        duplicate: false,
    }
}

fn sample_create_chat_message_variant_request() -> persistence::CreateChatMessageVariantRequest {
    persistence::CreateChatMessageVariantRequest {
        session_id: sample_session_id(),
        slot_id: sample_message_slot_id(),
        variant: sample_message_variant_write(),
    }
}

fn sample_create_chat_message_variant_result() -> persistence::CreateChatMessageVariantResult {
    persistence::CreateChatMessageVariantResult {
        variant: sample_message_variant_record(),
    }
}

fn sample_chat_read_model_query() -> persistence::ChatReadModelQuery {
    persistence::ChatReadModelQuery {
        session_id: sample_session_id(),
        agent_id: sample_agent_id().to_string(),
        cursor: Some("validation-cursor".to_owned()),
        limit: Some(25),
    }
}

fn sample_chat_read_model_page() -> persistence::ChatReadModelPage {
    persistence::ChatReadModelPage {
        items: vec![persistence::ChatReadModelEvent {
            event_id: "validation-read-event".to_owned(),
            session_id: sample_session_id(),
            sequence_id: 1,
            created_at: sample_timestamp(),
            kind: persistence::ChatReadModelEventKind::MessageCreated,
            payload_json: json!({"slot_id": sample_message_slot_id()}),
        }],
        latest_cursor: "validation-read-cursor".to_owned(),
        has_more: true,
        total: 2,
        source: persistence::ChatReadModelSource::MessageSlots,
    }
}

fn sample_chat_event_log_append() -> persistence::ChatEventLogAppend {
    persistence::ChatEventLogAppend {
        session_id: sample_session_id(),
        created_at: sample_timestamp(),
        kind: "message_created".to_owned(),
        payload_json: json!({"slot_id": sample_message_slot_id()}),
    }
}

fn sample_chat_event_log_query() -> persistence::ChatEventLogQuery {
    persistence::ChatEventLogQuery {
        session_id: sample_session_id(),
        cursor: Some("validation-event-cursor".to_owned()),
        limit: Some(25),
    }
}

fn sample_chat_event_log_page() -> persistence::ChatEventLogPage {
    persistence::ChatEventLogPage {
        items: vec![persistence::ChatEventLogEvent {
            event_id: "validation-log-event".to_owned(),
            session_id: sample_session_id(),
            sequence_id: 2,
            created_at: sample_timestamp(),
            kind: "message_created".to_owned(),
            payload_json: json!({"slot_id": sample_message_slot_id()}),
        }],
        latest_cursor: "validation-log-cursor".to_owned(),
        has_more: true,
        total: 3,
        message_count: 2,
        has_more_before: true,
    }
}

fn sample_conversation_branch_record() -> persistence::ConversationBranchRecord {
    persistence::ConversationBranchRecord {
        branch_id: sample_conversation_branch_id(),
        session_id: sample_session_id(),
        parent_branch_id: Some(ConversationBranchId::new("validation-parent-branch")),
        parent_message_id: Some(MessageId::new("validation-parent-message")),
        origin_message_id: Some(MessageId::new("validation-origin-message")),
        head_message_id: Some(sample_message_id()),
        label: Some("Validation Branch".to_owned()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        version: 5,
    }
}

fn sample_conversation_branch_write() -> persistence::ConversationBranchWrite {
    persistence::ConversationBranchWrite {
        branch_id: sample_conversation_branch_id(),
        session_id: sample_session_id(),
        parent_branch_id: Some(ConversationBranchId::new("validation-parent-branch")),
        parent_message_id: Some(MessageId::new("validation-parent-message")),
        origin_message_id: Some(MessageId::new("validation-origin-message")),
        head_message_id: Some(sample_message_id()),
        label: Some("Validation Branch".to_owned()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_conversation_branch_query() -> persistence::ConversationBranchQuery {
    persistence::ConversationBranchQuery {
        session_id: Some(sample_session_id()),
        parent_branch_id: Some(ConversationBranchId::new("validation-parent-branch")),
        page: Some(sample_query_page()),
    }
}

fn sample_conversation_branch_state_record() -> persistence::ConversationBranchStateRecord {
    persistence::ConversationBranchStateRecord {
        session_id: sample_session_id(),
        active_branch_id: Some(sample_conversation_branch_id()),
        updated_at: sample_timestamp(),
        version: 6,
    }
}

fn sample_conversation_snapshot_record() -> persistence::ConversationSnapshotRecord {
    persistence::ConversationSnapshotRecord {
        snapshot_id: sample_conversation_snapshot_id(),
        session_id: sample_session_id(),
        branch_id: Some(sample_conversation_branch_id()),
        message_id: Some(sample_message_id()),
        cursor: Some("validation-snapshot-cursor".to_owned()),
        label: Some("Validation Snapshot".to_owned()),
        summary: Some("Validation snapshot summary.".to_owned()),
        source: persistence::ConversationSnapshotSource::User,
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_conversation_snapshot_write() -> persistence::ConversationSnapshotWrite {
    persistence::ConversationSnapshotWrite {
        snapshot_id: sample_conversation_snapshot_id(),
        session_id: sample_session_id(),
        branch_id: Some(sample_conversation_branch_id()),
        message_id: Some(sample_message_id()),
        cursor: Some("validation-snapshot-cursor".to_owned()),
        label: Some("Validation Snapshot".to_owned()),
        summary: Some("Validation snapshot summary.".to_owned()),
        source: persistence::ConversationSnapshotSource::User,
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_conversation_snapshot_query() -> persistence::ConversationSnapshotQuery {
    persistence::ConversationSnapshotQuery {
        session_id: Some(sample_session_id()),
        branch_id: Some(sample_conversation_branch_id()),
        message_id: Some(sample_message_id()),
        page: Some(sample_query_page()),
    }
}

fn sample_conversation_jump_target() -> persistence::ConversationJumpTarget {
    persistence::ConversationJumpTarget::Snapshot {
        snapshot_id: sample_conversation_snapshot_id(),
    }
}

fn sample_conversation_jump_request() -> persistence::ConversationJumpRequest {
    persistence::ConversationJumpRequest {
        session_id: sample_session_id(),
        target: sample_conversation_jump_target(),
    }
}

fn sample_conversation_jump_result() -> persistence::ConversationJumpResult {
    persistence::ConversationJumpResult {
        session_id: sample_session_id(),
        target: sample_conversation_jump_target(),
        branch_id: Some(sample_conversation_branch_id()),
        message_id: Some(sample_message_id()),
        cursor: Some("validation-jump-cursor".to_owned()),
        snapshot_id: Some(sample_conversation_snapshot_id()),
    }
}

fn sample_attachment_link_record() -> persistence::AttachmentLinkRecord {
    persistence::AttachmentLinkRecord {
        link_id: sample_attachment_link_id(),
        attachment_id: sample_attachment_id(),
        session_id: sample_session_id(),
        message_id: Some(sample_message_id()),
        block_id: Some(sample_message_block_id()),
        scope_id: Some(sample_data_bank_scope_id()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
    }
}

fn sample_attachment_link_write() -> persistence::AttachmentLinkWrite {
    persistence::AttachmentLinkWrite {
        link_id: sample_attachment_link_id(),
        attachment_id: sample_attachment_id(),
        session_id: sample_session_id(),
        message_id: Some(sample_message_id()),
        block_id: Some(sample_message_block_id()),
        scope_id: Some(sample_data_bank_scope_id()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
    }
}

fn sample_attachment_record() -> persistence::AttachmentRecord {
    persistence::AttachmentRecord {
        attachment_id: sample_attachment_id(),
        session_id: sample_session_id(),
        status: persistence::AttachmentStatus::Active,
        filename: "validation.txt".to_owned(),
        mime_type: "text/plain".to_owned(),
        byte_size: 42,
        storage_url: Some("file:///validation.txt".to_owned()),
        download_url: Some("http://example.invalid/validation.txt".to_owned()),
        thumbnail_url: Some("http://example.invalid/validation-thumb.png".to_owned()),
        extracted_text: Some("Validation attachment text.".to_owned()),
        extracted_text_truncated: false,
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        expires_at: Some(sample_timestamp()),
        links: vec![sample_attachment_link_record()],
    }
}

fn sample_attachment_write() -> persistence::AttachmentWrite {
    persistence::AttachmentWrite {
        attachment_id: sample_attachment_id(),
        session_id: sample_session_id(),
        status: persistence::AttachmentStatus::Active,
        filename: "validation.txt".to_owned(),
        mime_type: "text/plain".to_owned(),
        byte_size: 42,
        storage_url: Some("file:///validation.txt".to_owned()),
        download_url: Some("http://example.invalid/validation.txt".to_owned()),
        thumbnail_url: Some("http://example.invalid/validation-thumb.png".to_owned()),
        extracted_text: Some("Validation attachment text.".to_owned()),
        extracted_text_truncated: false,
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        expires_at: Some(sample_timestamp()),
        link: Some(sample_attachment_link_write()),
    }
}

fn sample_attachment_query() -> persistence::AttachmentQuery {
    persistence::AttachmentQuery {
        session_id: Some(sample_session_id()),
        message_id: Some(sample_message_id()),
        block_id: Some(sample_message_block_id()),
        scope_id: Some(sample_data_bank_scope_id()),
        status: Some(persistence::AttachmentStatus::Active),
        include_removed: false,
        include_expired: true,
        expired_only: false,
        now: Some(sample_timestamp()),
        page: Some(sample_query_page()),
    }
}

fn sample_data_bank_scope_record() -> persistence::DataBankScopeRecord {
    persistence::DataBankScopeRecord {
        scope_id: sample_data_bank_scope_id(),
        session_id: sample_session_id(),
        status: persistence::DataBankScopeStatus::Active,
        label: Some("Validation Scope".to_owned()),
        description: Some("Validation data-bank scope.".to_owned()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_data_bank_scope_write() -> persistence::DataBankScopeWrite {
    persistence::DataBankScopeWrite {
        scope_id: sample_data_bank_scope_id(),
        session_id: sample_session_id(),
        status: persistence::DataBankScopeStatus::Active,
        label: Some("Validation Scope".to_owned()),
        description: Some("Validation data-bank scope.".to_owned()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_data_bank_scope_query() -> persistence::DataBankScopeQuery {
    persistence::DataBankScopeQuery {
        session_id: Some(sample_session_id()),
        status: Some(persistence::DataBankScopeStatus::Active),
        include_removed: false,
        page: Some(sample_query_page()),
    }
}

fn sample_memory_shape_ref() -> rusty_crew_core_protocol::MemoryRecordShapeRef {
    rusty_crew_core_protocol::MemoryRecordShapeRef {
        shape_id: rusty_crew_core_protocol::MemoryRecordShapeId::unchecked("world_detail"),
        version: 1,
    }
}

fn sample_memory_evidence_ref() -> rusty_crew_core_protocol::MemoryEvidenceRef {
    rusty_crew_core_protocol::MemoryEvidenceRef {
        evidence_type: rusty_crew_core_protocol::MemoryEvidenceKind::Transcript,
        ref_id: "validation-evidence".to_owned(),
        label: Some("Validation transcript".to_owned()),
    }
}

fn sample_roleplay_lore_record() -> persistence::RoleplayLoreRecord {
    persistence::RoleplayLoreRecord {
        record_id: "validation-lore-record".to_owned(),
        world_id: "validation-world".to_owned(),
        entity_id: Some("validation-entity".to_owned()),
        session_id: Some(sample_session_id()),
        branch_id: Some(sample_conversation_branch_id()),
        shape: sample_memory_shape_ref(),
        canon_status: persistence::RoleplayLoreCanonStatus::Canon,
        visibility: persistence::RoleplayLoreVisibility::Public,
        status: persistence::RoleplayLoreRecordStatus::Active,
        revision: 4,
        title: "Validation Lore".to_owned(),
        body: "Validation lore body.".to_owned(),
        content: json!({"summary": "Validation lore body."}),
        evidence_refs: vec![sample_memory_evidence_ref()],
        source: rusty_crew_core_protocol::MemoryProposalSource::Ui,
        confidence: 0.9,
        durability_rationale: "Useful for bridge validation.".to_owned(),
        supersedes_record_id: Some("validation-old-lore".to_owned()),
        superseded_by_record_id: None,
        tombstoned_at: None,
        tombstone_reason: None,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_roleplay_lore_write() -> persistence::RoleplayLoreWrite {
    persistence::RoleplayLoreWrite {
        record_id: "validation-lore-record".to_owned(),
        world_id: "validation-world".to_owned(),
        entity_id: Some("validation-entity".to_owned()),
        session_id: Some(sample_session_id()),
        branch_id: Some(sample_conversation_branch_id()),
        shape: sample_memory_shape_ref(),
        canon_status: persistence::RoleplayLoreCanonStatus::Canon,
        visibility: persistence::RoleplayLoreVisibility::Public,
        title: "Validation Lore".to_owned(),
        body: "Validation lore body.".to_owned(),
        content: json!({"summary": "Validation lore body."}),
        evidence_refs: vec![sample_memory_evidence_ref()],
        source: rusty_crew_core_protocol::MemoryProposalSource::Ui,
        confidence: 0.9,
        durability_rationale: "Useful for bridge validation.".to_owned(),
        supersedes_record_id: Some("validation-old-lore".to_owned()),
        now: sample_timestamp(),
    }
}

fn sample_roleplay_lore_replace() -> persistence::RoleplayLoreReplace {
    persistence::RoleplayLoreReplace {
        write: sample_roleplay_lore_write(),
        expected_revision: 4,
    }
}

fn sample_roleplay_lore_supersede() -> persistence::RoleplayLoreSupersede {
    persistence::RoleplayLoreSupersede {
        record_id: "validation-lore-record".to_owned(),
        expected_revision: 4,
        replacement: persistence::RoleplayLoreWrite {
            record_id: "validation-lore-replacement".to_owned(),
            supersedes_record_id: Some("validation-lore-record".to_owned()),
            ..sample_roleplay_lore_write()
        },
    }
}

fn sample_roleplay_lore_tombstone() -> persistence::RoleplayLoreTombstone {
    persistence::RoleplayLoreTombstone {
        record_id: "validation-lore-record".to_owned(),
        expected_revision: 4,
        reason: Some("Validation tombstone.".to_owned()),
        now: sample_timestamp(),
    }
}

fn sample_roleplay_lore_query() -> persistence::RoleplayLoreQuery {
    persistence::RoleplayLoreQuery {
        world_id: Some("validation-world".to_owned()),
        entity_id: Some("validation-entity".to_owned()),
        canon_status: Some(persistence::RoleplayLoreCanonStatus::Canon),
        visibility: Some(persistence::RoleplayLoreVisibility::Public),
        shape_id: Some("world_detail".to_owned()),
        provenance_ref_id: Some("validation-evidence".to_owned()),
        query: Some("validation".to_owned()),
        include_superseded: true,
        include_tombstoned: false,
        page: Some(sample_query_page()),
    }
}

fn sample_roleplay_lore_provenance_event() -> persistence::RoleplayLoreProvenanceEvent {
    persistence::RoleplayLoreProvenanceEvent {
        event_id: "validation-lore-event".to_owned(),
        record_id: "validation-lore-record".to_owned(),
        world_id: "validation-world".to_owned(),
        evidence_refs: vec![sample_memory_evidence_ref()],
        source: rusty_crew_core_protocol::MemoryProposalSource::Ui,
        actor: "validation-actor".to_owned(),
        note: Some("Validation provenance.".to_owned()),
        created_at: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_record() -> persistence::RoleplayLoreLayerRecord {
    persistence::RoleplayLoreLayerRecord {
        layer_id: "validation-layer".to_owned(),
        profile_id: sample_profile_id().to_string(),
        name: "Validation Layer".to_owned(),
        description: Some("Validation lore layer.".to_owned()),
        purpose: persistence::RoleplayLoreLayerPurpose::World,
        write_policy: persistence::RoleplayLoreLayerWritePolicy::Manual,
        is_archived: false,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_write() -> persistence::RoleplayLoreLayerWrite {
    persistence::RoleplayLoreLayerWrite {
        layer_id: "validation-layer".to_owned(),
        profile_id: sample_profile_id().to_string(),
        name: "Validation Layer".to_owned(),
        description: Some("Validation lore layer.".to_owned()),
        purpose: persistence::RoleplayLoreLayerPurpose::World,
        write_policy: persistence::RoleplayLoreLayerWritePolicy::Manual,
        now: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_update() -> persistence::RoleplayLoreLayerUpdate {
    persistence::RoleplayLoreLayerUpdate {
        layer_id: "validation-layer".to_owned(),
        name: Some("Updated Validation Layer".to_owned()),
        description: Some(Some("Updated validation lore layer.".to_owned())),
        purpose: Some(persistence::RoleplayLoreLayerPurpose::Story),
        write_policy: Some(persistence::RoleplayLoreLayerWritePolicy::AutoCapture),
        now: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_archive() -> persistence::RoleplayLoreLayerArchive {
    persistence::RoleplayLoreLayerArchive {
        layer_id: "validation-layer".to_owned(),
        now: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_config_record() -> persistence::RoleplayLoreLayerConfigRecord {
    persistence::RoleplayLoreLayerConfigRecord {
        config_id: "validation-layer-config".to_owned(),
        layer_id: "validation-layer".to_owned(),
        fts_weight: 1.0,
        subject_weight: 1.2,
        canon_weight: 1.1,
        tag_boost_weight: 0.5,
        recency_weight: 0.25,
        default_token_budget: 1600,
        constant_token_reserve: 300,
        min_relevance_score: 0.2,
        max_constants: 8,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_config_write() -> persistence::RoleplayLoreLayerConfigWrite {
    persistence::RoleplayLoreLayerConfigWrite {
        config_id: "validation-layer-config".to_owned(),
        layer_id: "validation-layer".to_owned(),
        fts_weight: 1.0,
        subject_weight: 1.2,
        canon_weight: 1.1,
        tag_boost_weight: 0.5,
        recency_weight: 0.25,
        default_token_budget: 1600,
        constant_token_reserve: 300,
        min_relevance_score: 0.2,
        max_constants: 8,
        now: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_entry_link() -> persistence::RoleplayLoreLayerEntryLink {
    persistence::RoleplayLoreLayerEntryLink {
        layer_id: "validation-layer".to_owned(),
        record_id: "validation-lore-record".to_owned(),
        is_constant: true,
        priority: 10,
        added_at: sample_timestamp(),
    }
}

fn sample_roleplay_lore_layer_entry_join() -> persistence::RoleplayLoreLayerEntryJoin {
    persistence::RoleplayLoreLayerEntryJoin {
        layer_id: "validation-layer".to_owned(),
        record_id: "validation-lore-record".to_owned(),
        is_constant: true,
        priority: 10,
        added_at: sample_timestamp(),
        record: sample_roleplay_lore_record(),
    }
}

fn sample_roleplay_lore_fact_capture() -> persistence::RoleplayLoreFactCapture {
    persistence::RoleplayLoreFactCapture {
        layer_id: "validation-layer".to_owned(),
        write: sample_roleplay_lore_write(),
        is_constant: false,
        priority: 3,
        capture_reason: Some("Validation capture.".to_owned()),
    }
}

fn sample_roleplay_lore_entry_promotion() -> persistence::RoleplayLoreEntryPromotion {
    persistence::RoleplayLoreEntryPromotion {
        source_layer_id: "validation-source-layer".to_owned(),
        source_record_id: "validation-lore-record".to_owned(),
        target_layer_id: "validation-layer".to_owned(),
        new_record_id: "validation-promoted-lore".to_owned(),
        is_constant: true,
        priority: 12,
        now: sample_timestamp(),
    }
}

fn sample_roleplay_chat_layers_write() -> persistence::RoleplayChatLayersWrite {
    persistence::RoleplayChatLayersWrite {
        chat_id: "validation-chat".to_owned(),
        layers: vec![persistence::RoleplayChatLayerLink {
            layer_id: "validation-layer".to_owned(),
            priority: 10,
            enabled: true,
        }],
        now: sample_timestamp(),
    }
}

fn sample_roleplay_chat_layer_record() -> persistence::RoleplayChatLayerRecord {
    persistence::RoleplayChatLayerRecord {
        chat_id: "validation-chat".to_owned(),
        layer_id: "validation-layer".to_owned(),
        priority: 10,
        enabled: true,
        created_at: sample_timestamp(),
        layer: sample_roleplay_lore_layer_record(),
    }
}

fn sample_lore_recall_query() -> persistence::LoreRecallQuery {
    persistence::LoreRecallQuery {
        chat_id: "validation-chat".to_owned(),
        session_id: Some(sample_session_id()),
        query_text: Some("validation lore".to_owned()),
        active_subjects: vec!["validation-entity".to_owned()],
        excluded_subjects: vec!["validation-excluded".to_owned()],
        token_budget: Some(1600),
        trace_id: Some("validation-trace".to_owned()),
        record_trace: true,
        now: sample_timestamp(),
    }
}

fn sample_lore_recall_trace_record() -> persistence::LoreRecallTraceRecord {
    persistence::LoreRecallTraceRecord {
        trace_id: "validation-trace".to_owned(),
        session_id: Some(sample_session_id()),
        layer_ids: vec!["validation-layer".to_owned()],
        query_text: Some("validation lore".to_owned()),
        active_subjects: vec!["validation-entity".to_owned()],
        excluded_subjects: vec!["validation-excluded".to_owned()],
        config_snapshot: json!({"fixture": true}),
        entries_considered: 3,
        entries_returned: 1,
        token_budget: Some(1600),
        tokens_consumed: 240,
        entry_decisions: vec![persistence::LoreRecallTraceEntryDecision {
            record_id: "validation-lore".to_owned(),
            layer_id: "validation-layer".to_owned(),
            score: 0.95,
            token_estimate: 240,
            is_constant: true,
            included: true,
            reason: persistence::LoreRecallTraceDecisionReason::Included,
        }],
        created_at: sample_timestamp(),
    }
}

fn sample_lore_recall_trace_query() -> persistence::LoreRecallTraceQuery {
    persistence::LoreRecallTraceQuery {
        session_id: Some(sample_session_id()),
        chat_id: Some("validation-chat".to_owned()),
        page: Some(sample_query_page()),
    }
}

fn sample_lore_recall_result() -> persistence::LoreRecallResult {
    persistence::LoreRecallResult {
        chat_id: "validation-chat".to_owned(),
        entries: vec![persistence::LoreRecallEntry {
            record: sample_roleplay_lore_record(),
            layer_id: "validation-layer".to_owned(),
            score: 0.95,
            token_estimate: 240,
            is_constant: true,
        }],
        entries_considered: 3,
        tokens_consumed: 240,
        token_budget: Some(1600),
        trace: Some(sample_lore_recall_trace_record()),
    }
}

fn sample_roleplay_session_metadata() -> roleplay::RoleplaySessionMetadata {
    roleplay::RoleplaySessionMetadata {
        session_id: sample_session_id().to_string(),
        profile_id: sample_profile_id().to_string(),
        display_name: Some("Validation Roleplay".to_owned()),
        player_persona_id: Some("validation-persona".to_owned()),
        character_id: Some("validation-character".to_owned()),
        active_layer_ids: vec!["validation-layer".to_owned()],
        archived: false,
        narrator_diagnostic: Some(roleplay::RoleplayNarratorDiagnostic {
            wake_id: "validation-wake".to_owned(),
            model_config_id: Some("validation-model-config".to_owned()),
            model_config_revision: Some(3),
            endpoint_id: Some("validation-endpoint".to_owned()),
            endpoint_revision: Some(2),
            credential_id: Some("validation-credential".to_owned()),
            credential_revision: Some(1),
            scene_brief: "Validation scene brief.".to_owned(),
            relevant_lore_record_ids: vec!["validation-lore".to_owned()],
            updated_at: sample_timestamp(),
        }),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_roleplay_player_persona() -> roleplay::RoleplayPlayerPersona {
    roleplay::RoleplayPlayerPersona {
        id: "validation-persona".to_owned(),
        profile_id: sample_profile_id().to_string(),
        display_name: "Validation Player".to_owned(),
        avatar_url: Some("http://example.invalid/player.png".to_owned()),
        avatar_asset_ref: Some("asset://player".to_owned()),
        description: "Validation player persona.".to_owned(),
        notes: "Validation notes.".to_owned(),
        status: "active".to_owned(),
        created_at: sample_timestamp(),
        updated_at: Some(sample_timestamp()),
    }
}

fn sample_roleplay_character() -> roleplay::RoleplayCharacter {
    roleplay::RoleplayCharacter {
        id: "validation-character".to_owned(),
        profile_id: sample_profile_id().to_string(),
        name: "Validation Character".to_owned(),
        description: "Validation character description.".to_owned(),
        personality: "Careful and explicit.".to_owned(),
        scenario: "Bridge validation scenario.".to_owned(),
        first_message: "Hello from validation.".to_owned(),
        alternate_greetings: vec!["Alternate validation greeting.".to_owned()],
        example_messages: vec!["Example validation message.".to_owned()],
        tags: vec!["validation".to_owned()],
        avatar_url: Some("http://example.invalid/character.png".to_owned()),
        status: "active".to_owned(),
        created_at: sample_timestamp(),
        updated_at: Some(sample_timestamp()),
    }
}

fn sample_roleplay_prompt_source_text() -> roleplay::RoleplayPromptStackSourceText {
    roleplay::RoleplayPromptStackSourceText {
        source_kind: "lore".to_owned(),
        source_id: "validation-lore-record".to_owned(),
        title: "Validation Lore".to_owned(),
        body: "Validation lore body.".to_owned(),
        editable: true,
        derived: false,
    }
}

fn sample_roleplay_prompt_raw_block() -> roleplay::RoleplayPromptStackRawBlock {
    roleplay::RoleplayPromptStackRawBlock {
        source_kind: "import".to_owned(),
        source_id: "validation-import".to_owned(),
        title: "Validation Imported Block".to_owned(),
        body: "Validation imported prompt block.".to_owned(),
        metadata_json: json!({"fixture": true}),
    }
}

fn sample_roleplay_prompt_context_input() -> roleplay::RoleplayPromptContextInput {
    roleplay::RoleplayPromptContextInput {
        metadata: sample_roleplay_session_metadata(),
        player_persona: Some(sample_roleplay_player_persona()),
        character: Some(sample_roleplay_character()),
        scene_setup: Some("Validation scene setup.".to_owned()),
        relevant_lore: vec![sample_roleplay_prompt_source_text()],
        recent_history: vec![roleplay::RoleplayPromptStackSourceText {
            source_kind: "history".to_owned(),
            ..sample_roleplay_prompt_source_text()
        }],
        response_guidance: Some("Respond in validation style.".to_owned()),
        imported_prompt_blocks: vec![sample_roleplay_prompt_raw_block()],
    }
}

fn sample_roleplay_prompt_stack_output() -> roleplay::RoleplayPromptStackOutput {
    roleplay::RoleplayPromptStackOutput {
        version: 1,
        compiled_text: "Compiled validation prompt.".to_owned(),
        messages: vec![roleplay::RoleplayPromptStackMessage {
            role: "system".to_owned(),
            content: "Validation prompt message.".to_owned(),
            section_ids: vec!["validation-section".to_owned()],
        }],
        sections: vec![roleplay::RoleplayPromptStackSection {
            id: "validation-section".to_owned(),
            title: "Validation Section".to_owned(),
            body: "Validation section body.".to_owned(),
            source_kind: "lore".to_owned(),
            source_id: "validation-lore-record".to_owned(),
            inclusion_reason: "Relevant validation lore.".to_owned(),
            token_estimate: 42,
            editable: true,
            derived: false,
        }],
        trace: vec![roleplay::RoleplayPromptStackTraceEntry {
            section_id: "validation-section".to_owned(),
            source_kind: "lore".to_owned(),
            source_id: "validation-lore-record".to_owned(),
            inclusion_reason: "Relevant validation lore.".to_owned(),
            token_estimate: 42,
            editable: true,
            derived: false,
        }],
        macro_resolutions: vec![roleplay::RoleplayPromptMacroResolution {
            macro_name: "character".to_owned(),
            replacement: "Validation Character".to_owned(),
            occurrences: 1,
        }],
        imported_prompt_blocks: vec![sample_roleplay_prompt_raw_block()],
    }
}

fn sample_roleplay_prompt_context_output() -> roleplay::RoleplayPromptContextOutput {
    roleplay::RoleplayPromptContextOutput {
        prompt_context: Some("Compiled validation prompt.".to_owned()),
        stack: Some(sample_roleplay_prompt_stack_output()),
    }
}

fn sample_roleplay_session_lifecycle_session() -> roleplay::RoleplaySessionLifecycleSession {
    roleplay::RoleplaySessionLifecycleSession {
        session_id: sample_session_id().to_string(),
        agent_id: sample_agent_id().to_string(),
        profile_id: sample_profile_id().to_string(),
        kind: "full".to_owned(),
        status: "active".to_owned(),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_roleplay_chat_layer_binding() -> roleplay::RoleplayChatLayerBinding {
    roleplay::RoleplayChatLayerBinding {
        layer_id: "validation-layer".to_owned(),
        priority: 10,
        enabled: true,
    }
}

fn sample_roleplay_session_lifecycle_plan_input() -> roleplay::RoleplaySessionLifecyclePlanInput {
    roleplay::RoleplaySessionLifecyclePlanInput {
        action: "fork".to_owned(),
        now: sample_timestamp(),
        body: json!({
            "sessionId": sample_session_id().to_string(),
            "messageId": sample_message_id().to_string()
        }),
        fallback_session_id: Some("validation-fallback-session".to_owned()),
        registry_agent_id: Some(sample_agent_id().to_string()),
        source_session: Some(sample_roleplay_session_lifecycle_session()),
        current_metadata: Some(sample_roleplay_session_metadata()),
        player_persona: Some(sample_roleplay_player_persona()),
        character: Some(sample_roleplay_character()),
        available_layer_ids: Some(vec!["validation-layer".to_owned()]),
        source_chat_layers: vec![sample_roleplay_chat_layer_binding()],
    }
}

fn sample_roleplay_session_lifecycle_plan() -> roleplay::RoleplaySessionLifecyclePlan {
    roleplay::RoleplaySessionLifecyclePlan {
        action: "fork".to_owned(),
        session_id: sample_session_id().to_string(),
        agent_id: sample_agent_id().to_string(),
        profile_id: sample_profile_id().to_string(),
        kind: "full".to_owned(),
        metadata: sample_roleplay_session_metadata(),
        runtime: roleplay::RoleplayRuntimeSessionPlan {
            create_session: true,
            archive_session: false,
            ensure_configured_session: true,
        },
        chat_layer_update: Some(roleplay::RoleplayChatLayerUpdatePlan {
            chat_id: sample_session_id().to_string(),
            layers: vec![sample_roleplay_chat_layer_binding()],
        }),
        fork: Some(roleplay::RoleplaySessionForkPlan {
            source_session_id: "validation-source-session".to_owned(),
            source_message_id: sample_message_id().to_string(),
            target_session_id: sample_session_id().to_string(),
            branch_id: sample_conversation_branch_id().to_string(),
            branch_label: "Validation Fork".to_owned(),
            branch_metadata_json: json!({"fixture": true}),
        }),
    }
}

fn sample_roleplay_chat_layer_binding_plan_input() -> roleplay::RoleplayChatLayerBindingPlanInput {
    roleplay::RoleplayChatLayerBindingPlanInput {
        now: sample_timestamp(),
        body: json!({"activeLayerIds": ["validation-layer"]}),
        current_metadata: Some(sample_roleplay_session_metadata()),
        current_chat_layers: vec![sample_roleplay_chat_layer_binding()],
        available_layer_ids: Some(vec!["validation-layer".to_owned()]),
    }
}

fn sample_roleplay_chat_layer_binding_plan() -> roleplay::RoleplayChatLayerBindingPlan {
    roleplay::RoleplayChatLayerBindingPlan {
        chat_layers_write: roleplay::RoleplayChatLayersWritePlan {
            chat_id: sample_session_id().to_string(),
            layers: vec![sample_roleplay_chat_layer_binding()],
            now: sample_timestamp(),
        },
        metadata_patch: Some(roleplay::RoleplaySessionActiveLayerPatch {
            active_layer_ids: vec!["validation-layer".to_owned()],
        }),
        active_layer_ids: vec!["validation-layer".to_owned()],
        chat_layers_changed: true,
        active_layer_ids_changed: true,
        no_op: false,
    }
}

fn sample_roleplay_lore_search_controls_input() -> roleplay::RoleplayLoreSearchControlsInput {
    roleplay::RoleplayLoreSearchControlsInput {
        params: json!({"layerId": "validation-layer", "limit": 20, "offset": 5}),
    }
}

fn sample_roleplay_lore_search_controls() -> roleplay::RoleplayLoreSearchControls {
    roleplay::RoleplayLoreSearchControls {
        explicit_layer_ids: vec!["validation-layer".to_owned()],
        page: roleplay::RoleplayLoreSearchPagePlan {
            limit: 20,
            offset: 5,
        },
    }
}

fn sample_roleplay_speaker_identity_input() -> roleplay::RoleplaySpeakerIdentityInput {
    roleplay::RoleplaySpeakerIdentityInput {
        actor: roleplay::RoleplayChatActor {
            id: "validation-character".to_owned(),
            kind: "character".to_owned(),
            display_name: Some("Validation Character".to_owned()),
        },
        now: sample_timestamp(),
        metadata: Some(sample_roleplay_session_metadata()),
        player_persona: Some(sample_roleplay_player_persona()),
        character: Some(sample_roleplay_character()),
    }
}

fn sample_roleplay_speaker_identity_snapshot() -> roleplay::RoleplaySpeakerIdentitySnapshot {
    roleplay::RoleplaySpeakerIdentitySnapshot {
        speaker_kind: "character".to_owned(),
        role: "assistant".to_owned(),
        source_id: "validation-character".to_owned(),
        display_name: "Validation Character".to_owned(),
        avatar_url: Some("http://example.invalid/character.png".to_owned()),
        avatar_asset_ref: Some("asset://character".to_owned()),
        snapshot_at: sample_timestamp(),
    }
}

fn sample_roleplay_scene_state() -> roleplay::RoleplaySceneState {
    roleplay::RoleplaySceneState {
        session_id: sample_session_id().to_string(),
        location: Some("Validation room".to_owned()),
        characters_present: vec!["Validation Character".to_owned()],
        active_threads: vec!["Bridge validation".to_owned()],
        notes: Some("Validation scene notes.".to_owned()),
        updated_at: Some(sample_timestamp()),
    }
}

fn sample_roleplay_character_write_input() -> roleplay::RoleplayCharacterWriteInput {
    roleplay::RoleplayCharacterWriteInput {
        profile_id: sample_profile_id().to_string(),
        now: sample_timestamp(),
        fallback_id: "validation-character".to_owned(),
        body: json!({"name": "Validation Character"}),
    }
}

fn sample_roleplay_character_merge_input() -> roleplay::RoleplayCharacterMergeInput {
    roleplay::RoleplayCharacterMergeInput {
        current: sample_roleplay_character(),
        now: sample_timestamp(),
        body: json!({"description": "Updated validation character."}),
    }
}

fn sample_roleplay_player_persona_write_input() -> roleplay::RoleplayPlayerPersonaWriteInput {
    roleplay::RoleplayPlayerPersonaWriteInput {
        profile_id: sample_profile_id().to_string(),
        now: sample_timestamp(),
        fallback_id: "validation-persona".to_owned(),
        body: json!({"displayName": "Validation Player"}),
    }
}

fn sample_roleplay_player_persona_merge_input() -> roleplay::RoleplayPlayerPersonaMergeInput {
    roleplay::RoleplayPlayerPersonaMergeInput {
        current: sample_roleplay_player_persona(),
        now: sample_timestamp(),
        body: json!({"notes": "Updated validation notes."}),
    }
}

fn sample_roleplay_session_metadata_patch_input() -> roleplay::RoleplaySessionMetadataPatchInput {
    roleplay::RoleplaySessionMetadataPatchInput {
        current: sample_roleplay_session_metadata(),
        session_id: sample_session_id().to_string(),
        profile_id: sample_profile_id().to_string(),
        now: sample_timestamp(),
        body: json!({"displayName": "Updated Validation Roleplay"}),
        player_persona: Some(sample_roleplay_player_persona()),
        character: Some(sample_roleplay_character()),
        available_layer_ids: Some(vec!["validation-layer".to_owned()]),
    }
}

fn sample_roleplay_session_metadata_patch_output() -> roleplay::RoleplaySessionMetadataPatchOutput {
    roleplay::RoleplaySessionMetadataPatchOutput {
        metadata: sample_roleplay_session_metadata(),
        active_layer_ids_changed: true,
    }
}

fn sample_roleplay_scene_state_read_input() -> roleplay::RoleplaySceneStateReadInput {
    roleplay::RoleplaySceneStateReadInput {
        session_id: sample_session_id().to_string(),
        record_value_json: Some(serde_json::to_string(&sample_roleplay_scene_state()).unwrap()),
        record_updated_at: Some(sample_timestamp()),
        revision: Some(3),
    }
}

fn sample_roleplay_scene_state_read_output() -> roleplay::RoleplaySceneStateReadOutput {
    roleplay::RoleplaySceneStateReadOutput {
        state: sample_roleplay_scene_state(),
        revision: Some(3),
    }
}

fn sample_roleplay_scene_state_update_input() -> roleplay::RoleplaySceneStateUpdateInput {
    roleplay::RoleplaySceneStateUpdateInput {
        session_id: sample_session_id().to_string(),
        current: Some(sample_roleplay_scene_state()),
        now: sample_timestamp(),
        body: json!({"location": "Updated validation room"}),
    }
}

fn sample_roleplay_scene_state_update_plan() -> roleplay::RoleplaySceneStateUpdatePlan {
    roleplay::RoleplaySceneStateUpdatePlan {
        state: sample_roleplay_scene_state(),
        value_json: serde_json::to_string(&sample_roleplay_scene_state()).unwrap(),
        now: sample_timestamp(),
    }
}

fn sample_roleplay_narrator_config() -> roleplay::RoleplayNarratorConfig {
    roleplay::RoleplayNarratorConfig {
        tone: "dramatic".to_owned(),
        pacing: "steady".to_owned(),
        explicitness: "safe".to_owned(),
        memory_depth: "full".to_owned(),
        style_prompt: Some("Use validation style.".to_owned()),
        exemplar: Some("Validation exemplar.".to_owned()),
        review: roleplay::RoleplayNarratorReviewConfig {
            enabled: true,
            max_review_cycles: 2,
        },
    }
}

fn sample_roleplay_mechanic_profile_plan() -> roleplay::RoleplayMechanicProfilePlan {
    roleplay::RoleplayMechanicProfilePlan {
        config: roleplay::RoleplayMechanicConfig {
            model_config_id: Some("validation-model".to_owned()),
            name: "Maren".to_owned(),
            provider_alias: Some("validation-provider".to_owned()),
            auto_monitor: roleplay::RoleplayMechanicAutoMonitorConfig {
                enabled: false,
                available: false,
                status: roleplay::RoleplayMechanicAutoMonitorStatus::InactiveFuture,
            },
        },
        system_prompt: "You are Maren, the roleplay mechanic.".to_owned(),
        local_tool_profile_id: "roleplay_mechanic".to_owned(),
    }
}

fn sample_roleplay_narrator_tool_request() -> roleplay::RoleplayNarratorToolRequest {
    roleplay::RoleplayNarratorToolRequest {
        tool_name: "recall_lore".to_owned(),
        params_json: json!({"query": "validation"}),
    }
}

fn sample_roleplay_narrator_tool_observation() -> roleplay::RoleplayNarratorToolObservation {
    roleplay::RoleplayNarratorToolObservation {
        tool_name: "recall_lore".to_owned(),
        ok: true,
        summary: "Validation lore recalled.".to_owned(),
        details_json: Some(json!({"entries": 1})),
    }
}

fn sample_roleplay_narrator_turn_state() -> roleplay::RoleplayNarratorTurnState {
    roleplay::RoleplayNarratorTurnState {
        profile_id: sample_profile_id().to_string(),
        session_id: sample_session_id().to_string(),
        pending_text: "Validation pending text.".to_owned(),
        narrator_config: Some(sample_roleplay_narrator_config()),
        review_enabled: true,
        max_review_cycles: 2,
        review_cycle: 1,
        prelude_observations: vec![sample_roleplay_narrator_tool_observation()],
        relevant_lore: vec![sample_roleplay_prompt_source_text()],
        scene_brief: Some("Validation scene brief.".to_owned()),
        review_feedback: Some("Revise validation pacing.".to_owned()),
        completed_phases: vec![roleplay::RoleplayNarratorPhaseKind::PreludeExplore],
    }
}

fn sample_roleplay_narrator_start_input() -> roleplay::RoleplayNarratorStartInput {
    roleplay::RoleplayNarratorStartInput {
        wake_id: "validation-wake".to_owned(),
        session_id: sample_session_id().to_string(),
        profile_id: sample_profile_id().to_string(),
        pending_text: "Validation pending text.".to_owned(),
        narrator_config: Some(sample_roleplay_narrator_config()),
        review_enabled: true,
        max_review_cycles: Some(2),
    }
}

fn sample_roleplay_narrator_advance_input() -> roleplay::RoleplayNarratorAdvanceInput {
    roleplay::RoleplayNarratorAdvanceInput {
        receipt: sample_roleplay_narrator_turn_receipt(),
        outcome: roleplay::RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted {
            output_text: "Validation explore output.".to_owned(),
        },
    }
}

fn sample_roleplay_narrator_turn_receipt() -> roleplay::RoleplayNarratorTurnReceipt {
    roleplay::RoleplayNarratorTurnReceipt {
        receipt_id: "narrator-validation".to_owned(),
        wake_id: "validation-wake".to_owned(),
        session_id: sample_session_id().to_string(),
        sequence: 3,
        phase: roleplay::RoleplayNarratorPhaseKind::Compose,
        activity: Some(roleplay::RoleplayNarratorActivity {
            phase: roleplay::RoleplayNarratorActivityPhase::Composing,
            message: "Writing final narrative response.".to_owned(),
        }),
        directive: roleplay::RoleplayNarratorDirective::ProviderPhase {
            phase: roleplay::RoleplayNarratorPhaseKind::Compose,
            instructions: "Compose validation response.".to_owned(),
            allowed_tools: vec!["recall_lore".to_owned()],
            output_mode: roleplay::RoleplayNarratorOutputMode::Final,
        },
        state: sample_roleplay_narrator_turn_state(),
        terminal: false,
    }
}

fn sample_roleplay_durable_message() -> roleplay::RoleplayDurableMessage {
    roleplay::RoleplayDurableMessage {
        message_id: sample_message_id().to_string(),
        session_id: sample_session_id().to_string(),
        branch_id: Some(sample_conversation_branch_id().to_string()),
        parent_message_id: Some("validation-parent-message".to_owned()),
        previous_message_id: Some("validation-previous-message".to_owned()),
        author_id: "validation-character".to_owned(),
        author_role: "assistant".to_owned(),
        status: "completed".to_owned(),
        body: "Validation assistant message.".to_owned(),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        blocks: vec![json!({"kind": "text", "text": "Validation assistant message."})],
    }
}

fn sample_roleplay_message_variant() -> roleplay::RoleplayMessageVariant {
    roleplay::RoleplayMessageVariant {
        variant_id: sample_message_variant_id().to_string(),
        slot_id: sample_message_slot_id().to_string(),
        source: "primary".to_owned(),
        ordinal: 0,
        status: "active".to_owned(),
        message: sample_roleplay_durable_message(),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_roleplay_message_slot() -> roleplay::RoleplayMessageSlot {
    roleplay::RoleplayMessageSlot {
        slot_id: sample_message_slot_id().to_string(),
        session_id: sample_session_id().to_string(),
        primary_variant_id: sample_message_variant_id().to_string(),
        active_variant_id: Some(sample_message_variant_id().to_string()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        version: 4,
        primary: sample_roleplay_message_variant(),
        alternates: vec![roleplay::RoleplayMessageVariant {
            variant_id: "validation-alternate-variant".to_owned(),
            source: "alternate".to_owned(),
            ordinal: 1,
            ..sample_roleplay_message_variant()
        }],
    }
}

fn sample_roleplay_conversation_branch() -> roleplay::RoleplayConversationBranch {
    roleplay::RoleplayConversationBranch {
        branch_id: sample_conversation_branch_id().to_string(),
        session_id: sample_session_id().to_string(),
        parent_branch_id: Some("validation-parent-branch".to_owned()),
        parent_message_id: Some("validation-parent-message".to_owned()),
        origin_message_id: Some("validation-origin-message".to_owned()),
        head_message_id: Some(sample_message_id().to_string()),
        label: Some("Validation Branch".to_owned()),
        metadata_json: json!({"fixture": true}),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        version: 5,
    }
}

fn sample_roleplay_assistant_alternative_plan_input(
) -> roleplay::RoleplayAssistantAlternativePlanInput {
    roleplay::RoleplayAssistantAlternativePlanInput {
        session_id: sample_session_id().to_string(),
        requested_slot_id: Some(sample_message_slot_id().to_string()),
        request_id: Some("validation-request".to_owned()),
        body: json!({"message": "validation"}),
        slots: vec![sample_roleplay_message_slot()],
        active_branch_id: Some(sample_conversation_branch_id().to_string()),
        branches: vec![sample_roleplay_conversation_branch()],
    }
}

fn sample_roleplay_assistant_alternative_plan() -> roleplay::RoleplayAssistantAlternativePlan {
    roleplay::RoleplayAssistantAlternativePlan {
        session_id: sample_session_id().to_string(),
        terminal_slot: sample_roleplay_message_slot(),
        active_variant: sample_roleplay_message_variant(),
        variant_projection: roleplay::RoleplayAlternativeSlotProjection {
            slot_id: sample_message_slot_id().to_string(),
            active_variant_id: Some(sample_message_variant_id().to_string()),
            primary_variant_id: sample_message_variant_id().to_string(),
            alternate_count: 1,
            variant_count: 2,
            active_variant: sample_roleplay_message_variant(),
            variants: vec![sample_roleplay_message_variant()],
        },
        next_alternate_ordinal: 2,
        branch_id_for_variant: Some(sample_conversation_branch_id().to_string()),
        parent_message_id: Some("validation-parent-message".to_owned()),
        previous_message_id: Some("validation-previous-message".to_owned()),
        branch_head_update: Some(roleplay::RoleplayBranchHeadUpdatePlan {
            branch_id: sample_conversation_branch_id().to_string(),
            head_message_id: sample_message_id().to_string(),
        }),
        append_chat_message: false,
        variant_write: Some(roleplay::RoleplayAssistantAlternativeVariantWritePlan {
            slot_id: sample_message_slot_id().to_string(),
            variant_id: "validation-alternate-variant".to_owned(),
            message_id: "validation-alternate-message".to_owned(),
            source: "alternate".to_owned(),
            ordinal: 2,
            branch_id: Some(sample_conversation_branch_id().to_string()),
            parent_message_id: Some("validation-parent-message".to_owned()),
            previous_message_id: Some("validation-previous-message".to_owned()),
        }),
    }
}

fn sample_profile_registry_record() -> ProfileRegistryRecord {
    ProfileRegistryRecord {
        profile_id: sample_profile_id(),
        lifecycle_status: ProfileRegistryLifecycleStatus::Active,
        display_name: Some("Validation Profile".to_owned()),
        summary: Some("Fixture profile record.".to_owned()),
        default_session_kind: Some(SessionKind::Full),
        agent_id: Some(sample_agent_id()),
        owner_id: Some("operator".to_owned()),
        prompt_soul_markdown: Some("You are a validation fixture.".to_owned()),
        prompt_memory_markdown: Some("Remember bridge drift checks.".to_owned()),
        active_runtime_settings_json: json!({"providerAlias": "validation-provider"}),
        source_asset_refs: vec![ProfileRegistrySourceAssetRef {
            asset_kind: "profile_config".to_owned(),
            path: "profiles/validation-profile/profile.json".to_owned(),
            content_hash: Some("sha256:validation".to_owned()),
            last_seen_at: Some(sample_timestamp()),
            metadata_json: json!({"fixture": true}),
        }],
        derived_runtime_refs: vec![ProfileRegistryDerivedRuntimeRef {
            ref_kind: "session".to_owned(),
            ref_id: "validation-session".to_owned(),
            status: "active".to_owned(),
            updated_at: Some(sample_timestamp()),
            metadata_json: json!({"fixture": true}),
        }],
        import_export: ProfileRegistryImportExportMetadata {
            imported_from: None,
            imported_at: None,
            exported_to: None,
            exported_at: None,
            metadata_json: json!({"fixture": true}),
        },
        revision: 3,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_profile_registry_write() -> ProfileRegistryWrite {
    let record = sample_profile_registry_record();
    ProfileRegistryWrite {
        profile_id: record.profile_id,
        lifecycle_status: record.lifecycle_status,
        display_name: record.display_name,
        summary: record.summary,
        default_session_kind: record.default_session_kind,
        agent_id: record.agent_id,
        owner_id: record.owner_id,
        prompt_soul_markdown: record.prompt_soul_markdown,
        prompt_memory_markdown: record.prompt_memory_markdown,
        active_runtime_settings_json: record.active_runtime_settings_json,
        source_asset_refs: record.source_asset_refs,
        derived_runtime_refs: record.derived_runtime_refs,
        import_export: record.import_export,
        now: sample_timestamp(),
    }
}

fn sample_profile_registry_update() -> ProfileRegistryUpdate {
    ProfileRegistryUpdate {
        write: sample_profile_registry_write(),
        expected_revision: 3,
    }
}

fn sample_profile_registry_mutation_request() -> ProfileRegistryMutationRequest {
    ProfileRegistryMutationRequest {
        profile_id: sample_profile_id(),
        kind: ProfileRegistryMutationKind::Prompt,
        mode: ProfileRegistryMutationMode::Plan,
        current: sample_profile_registry_record(),
        body_json: json!({
            "expectedRevision": 3,
            "promptSoulMarkdown": "Updated validation fixture soul.",
            "promptMemoryMarkdown": "Updated validation fixture memory."
        }),
        now: sample_timestamp(),
    }
}

fn sample_profile_registry_mutation_plan() -> ProfileRegistryMutationPlan {
    ProfileRegistryMutationPlan {
        ok: true,
        profile_id: sample_profile_id(),
        kind: ProfileRegistryMutationKind::Prompt,
        mode: ProfileRegistryMutationMode::Plan,
        expected_revision: 3,
        current: sample_profile_registry_record(),
        next: sample_profile_registry_record(),
        next_write: sample_profile_registry_write(),
        diagnostics: vec![],
        implications: rusty_crew_core_config::ProfileRegistryMutationImplications {
            registry_revision_will_increment: true,
            profile_files_unchanged: true,
            service_config_unchanged: true,
            runtime_rebuild_recommended: true,
            lifecycle_effects: "none".to_owned(),
        },
    }
}

fn sample_profile_purge_report() -> ProfilePurgeReport {
    ProfilePurgeReport {
        profile_id: sample_profile_id(),
        profile_registry_deleted: true,
        session_ids: vec![sample_session_id()],
        agent_ids: vec![sample_agent_id()],
        table_counts: vec![ProfilePurgeTableCount {
            table: "profile_registry".to_owned(),
            rows_deleted: 1,
        }],
        rows_deleted: 3,
    }
}

fn sample_model_provider_record() -> ModelProviderRecord {
    ModelProviderRecord {
        alias: "validation-provider".to_owned(),
        status: ModelProviderStatus::Active,
        protocol: ModelProviderProtocol::ChatCompletions,
        provider_kind: "openai-compatible".to_owned(),
        display_name: Some("Validation Provider".to_owned()),
        description: Some("Fixture model provider record.".to_owned()),
        base_url: Some("http://127.0.0.1:18082/v1".to_owned()),
        model_id: "gpt-fixture".to_owned(),
        context_window_tokens: Some(128_000),
        max_output_tokens: Some(4096),
        temperature_milli: Some(500),
        reasoning_effort: Some("medium".to_owned()),
        reasoning_format: Some("summary".to_owned()),
        responses_dialect: None,
        chat_completions_dialect: Default::default(),
        thinking_mode: Default::default(),
        reasoning_history: Default::default(),
        reasoning_budget_tokens: None,
        prompt_caching: Default::default(),
        credential_id: Some("provider:validation-provider".to_owned()),
        credential: ModelProviderCredential {
            has_secret: true,
            secret_ref: Some(
                "db://service_credentials/provider:validation-provider/secret".to_owned(),
            ),
            updated_at: Some(sample_timestamp()),
            kind: Some(ModelProviderCredentialKind::ApiKey),
            revision: Some(1),
        },
        metadata_json: json!({"fixture": true}),
        revision: 5,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_model_provider_refresh_impact() -> ModelProviderRefreshImpact {
    ModelProviderRefreshImpact {
        provider_alias: "validation-provider".to_owned(),
        affected_profiles: vec![ModelProviderAffectedProfile {
            profile_id: ProfileId::new("validation-profile"),
            session_ids: vec![
                SessionId::new("validation-active-session"),
                SessionId::new("validation-configured-session"),
            ],
            configured_session_ids: vec![SessionId::new("validation-configured-session")],
            active_session_ids: vec![SessionId::new("validation-active-session")],
        }],
    }
}

fn sample_model_provider_refresh_plan() -> ModelProviderRefreshPlan {
    ModelProviderRefreshPlan {
        provider_alias: "validation-provider".to_owned(),
        mode: ModelProviderRefreshMode::Plan,
        affected_profiles: sample_model_provider_refresh_impact().affected_profiles,
        actions: vec![ModelProviderRefreshProfileAction {
            profile_id: ProfileId::new("validation-profile"),
            command_name: "runtime.rebuild.profile.plan".to_owned(),
            reason: "validation fixture".to_owned(),
            planned_summary: "Would rebuild validation-profile runtime.".to_owned(),
            applied_summary: "Rebuilt validation-profile runtime.".to_owned(),
            blocked_summary: "Validation profile rebuild blocked.".to_owned(),
            failure_reason_code: "validation_rebuild_failed".to_owned(),
        }],
    }
}

fn sample_memory_space_descriptor() -> MemorySpaceDescriptor {
    session_memory_space_descriptor()
}

fn sample_memory_proposal() -> MemoryProposalEnvelope {
    MemoryProposalEnvelope {
        proposal_id: "proposal_one".to_owned(),
        space_id: MemorySpaceId::unchecked("session_memory"),
        operation: MemoryOperation::Add,
        scope: MemoryScope {
            scope_type: MemoryScopeType::Session,
            scope_id: sample_session_id().to_string(),
        },
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("session_fact"),
            version: 1,
        },
        content: json!({
            "record_id": "session-fact-one",
            "content": "The user prefers compact bridge validation notes.",
            "fact_kind": "preference",
            "confidence": 0.8,
            "source_summary": "Captured from validation fixture.",
            "created_at": sample_timestamp(),
            "updated_at": sample_timestamp(),
            "tags": ["bridge", "validation"],
            "metadata_json": {"fixture": true}
        }),
        evidence_refs: vec![MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::Wake,
            ref_id: "wake-validation".to_owned(),
            label: Some("Validation wake".to_owned()),
        }],
        confidence: 0.8,
        durability_rationale: Some("Fixture verifies memory proposal wire shape.".to_owned()),
        governance_mode: MemoryGovernanceMode::Candidate,
        source: MemoryProposalSource::InWakeTool,
        dedupe_key: Some("session_fact:validation".to_owned()),
        created_at: Some(sample_timestamp()),
    }
}

fn sample_memory_proposal_record() -> MemoryProposalRecord {
    MemoryProposalRecord {
        proposal: sample_memory_proposal(),
        status: MemoryProposalReviewStatus::Approved,
        selected_governance_mode: MemoryGovernanceMode::ManualReview,
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
        decided_at: Some(sample_timestamp()),
        applied_at: None,
        resulting_revision: Some(7),
        duplicate_of: None,
    }
}

fn sample_memory_proposal_query() -> MemoryProposalQuery {
    MemoryProposalQuery {
        space_id: Some(MemorySpaceId::unchecked("session_memory")),
        status: Some(MemoryProposalReviewStatus::Approved),
        dedupe_key: Some("session_fact:validation".to_owned()),
        limit: Some(25),
        offset: Some(5),
    }
}

fn sample_memory_governance_decision_input() -> MemoryGovernanceDecisionInput {
    MemoryGovernanceDecisionInput {
        decision_id: "decision_one".to_owned(),
        proposal_id: "proposal_one".to_owned(),
        decision: MemoryGovernanceDecisionKind::Approved,
        actor: "validation-reviewer".to_owned(),
        source: MemoryProposalSource::Human,
        evidence_refs: vec![MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::Ui,
            ref_id: "review-validation".to_owned(),
            label: Some("Validation review".to_owned()),
        }],
        policy_mode: MemoryGovernanceMode::ManualReview,
        confidence: Some(0.9),
        message: Some("Approved by bridge validation fixture.".to_owned()),
        resulting_revision: Some(7),
        decided_at: Some(sample_timestamp()),
    }
}

fn sample_memory_governance_decision_record() -> MemoryGovernanceDecisionRecord {
    MemoryGovernanceDecisionRecord {
        decision_id: "decision_one".to_owned(),
        proposal_id: "proposal_one".to_owned(),
        decision: MemoryGovernanceDecisionKind::Approved,
        actor: "validation-reviewer".to_owned(),
        source: MemoryProposalSource::Human,
        evidence_refs: vec![MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::Ui,
            ref_id: "review-validation".to_owned(),
            label: Some("Validation review".to_owned()),
        }],
        policy_mode: MemoryGovernanceMode::ManualReview,
        confidence: Some(0.9),
        message: Some("Approved by bridge validation fixture.".to_owned()),
        resulting_revision: Some(7),
        decided_at: sample_timestamp(),
    }
}

fn sample_session_activity_digest_query() -> SessionActivityDigestQuery {
    SessionActivityDigestQuery {
        profile_id: Some(sample_profile_id()),
        session_id: Some(sample_session_id()),
        wake_id: Some("wake-validation".to_owned()),
        include_reviewed: true,
        limit: Some(25),
        offset: Some(5),
    }
}

fn sample_session_activity_digest() -> SessionActivityDigest {
    SessionActivityDigest {
        digest_id: "digest_one".to_owned(),
        profile_id: sample_profile_id(),
        session_id: sample_session_id(),
        wake_id: "wake-validation".to_owned(),
        source: "post_turn_capture".to_owned(),
        summary_text: "The agent inspected bridge fixture coverage.".to_owned(),
        event_counts_json: json!({
            "assistant_text_delta": 4,
            "tool_call_started": 1,
            "tool_call_completed": 1
        }),
        tool_calls_json: json!([
            {
                "tool": "storage_query_catalog",
                "status": "completed",
                "duration_ms": 12
            }
        ]),
        signals_json: json!({
            "candidate_memory": true,
            "context_pressure": "moderate"
        }),
        completion_summary: Some("Bridge fixture expansion completed.".to_owned()),
        allowed_capture_spaces: vec![MemorySpaceId::unchecked("profile_dense")],
        created_at: sample_timestamp(),
        retention_until: Some("2026-08-02T00:00:00.000Z".to_owned()),
        reviewed_at: None,
    }
}

fn sample_context_compaction_artifact_query() -> ContextCompactionArtifactQuery {
    ContextCompactionArtifactQuery {
        session_id: Some(sample_session_id()),
        branch_id: Some(sample_conversation_branch_id()),
        strategy_id: Some("rolling_summary".to_owned()),
        enters_future_context: Some(true),
        latest_only: true,
        terminal_status: Some("completed".to_owned()),
        limit: Some(25),
        offset: Some(5),
    }
}

fn sample_context_compaction_artifact() -> ContextCompactionArtifact {
    ContextCompactionArtifact {
        artifact_id: "compaction_one".to_owned(),
        session_id: sample_session_id(),
        branch_id: None,
        strategy_id: "rolling_summary".to_owned(),
        strategy_revision: Some("1".to_owned()),
        logical_turn_id: Some("turn-1".to_owned()),
        execution_epoch_id: Some("epoch-1".to_owned()),
        source_projection_fingerprint: Some("fp-fixture".to_owned()),
        trigger: Some("auto_threshold".to_owned()),
        before_tokens: Some(78000),
        after_tokens: Some(18000),
        preserved_item_count: Some(10),
        excised_item_count: Some(8),
        intent_key: Some("intent-fixture-1".to_owned()),
        terminal_status: Some("completed".to_owned()),
        provider_chain_action: Some("rebuild_replay_after_compaction".to_owned()),
        source_refs_json: json!([
            {
                "kind": "message_range",
                "from": "slot-1",
                "to": "slot-8"
            }
        ]),
        provider_metadata_json: json!({
            "provider_alias": "validation-provider",
            "model_id": "gpt-fixture"
        }),
        estimate_before_json: json!({
            "estimated_prompt_tokens": 78000,
            "context_window_tokens": 128000
        }),
        estimate_after_json: Some(json!({
            "estimated_prompt_tokens": 18000,
            "context_window_tokens": 128000
        })),
        summary_text: "Condensed prior bridge validation discussion into a short durable summary."
            .to_owned(),
        enters_future_context: true,
        context_policy: "default_context_policy".to_owned(),
        metadata_json: json!({
            "fixture": true,
            "strategy_revision": 1
        }),
        created_at: sample_timestamp(),
        updated_at: sample_timestamp(),
    }
}

fn sample_session_id() -> SessionId {
    SessionId::new("validation-session")
}

fn sample_message_slot_id() -> MessageSlotId {
    MessageSlotId::new("validation-message-slot")
}

fn sample_message_variant_id() -> MessageVariantId {
    MessageVariantId::new("validation-message-variant")
}

fn sample_message_id() -> MessageId {
    MessageId::new("validation-message")
}

fn sample_message_block_id() -> MessageBlockId {
    MessageBlockId::new("validation-message-block")
}

fn sample_conversation_branch_id() -> ConversationBranchId {
    ConversationBranchId::new("validation-branch")
}

fn sample_conversation_snapshot_id() -> ConversationSnapshotId {
    ConversationSnapshotId::new("validation-snapshot")
}

fn sample_attachment_id() -> AttachmentId {
    AttachmentId::new("validation-attachment")
}

fn sample_attachment_link_id() -> AttachmentLinkId {
    AttachmentLinkId::new("validation-attachment-link")
}

fn sample_data_bank_scope_id() -> DataBankScopeId {
    DataBankScopeId::new("validation-data-bank-scope")
}

fn sample_agent_id() -> AgentId {
    AgentId::new("validation-agent")
}

fn sample_profile_id() -> ProfileId {
    ProfileId::new("validation-profile")
}

fn sample_timestamp() -> IsoTimestamp {
    "2026-07-02T00:00:00.000Z".to_owned()
}
