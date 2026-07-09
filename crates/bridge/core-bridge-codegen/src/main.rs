#![recursion_limit = "256"]

use anyhow::{bail, Context, Result};
use rusty_crew_core_bridge_api::*;
use rusty_crew_core_config::{
    BrainConfigDraft, ChannelBindingConfigDraft, ChannelWakePolicy, CreateProfileMcpBindingRequest,
    CreateProfilePlanInput, CreateProfileRequest, CreateProfileSourceRequest,
    ExternalBindingStatusDraft, McpBindingConfigDraft, ProfileBackgroundReviewConfig,
    ProfileBackgroundReviewType, ProfileBrainMetadata, ProfileChannelDefaults,
    ProfileContextPolicy, ProfileMcpConfig, ProfileModelConfigSeed, ProfileRegistryMutationKind,
    ProfileRegistryMutationMode, ProfileRegistryMutationPlan, ProfileRegistryMutationRequest,
    ProfileRegistryRuntimeMetadata, ProfileRuntimeMetadata, ProfileRuntimeOptions,
    ProfileSessionDefaults, RuntimeConfigDraft, RuntimeConfigValidationInput,
    ScheduledJobConfigDraft, ScheduledJobShape, SessionConfigDraft,
};
use rusty_crew_core_persistence as persistence;
use rusty_crew_core_protocol::{
    AdapterId, AgentId, AgentInstanceId, AttachmentId, AttachmentLinkId, BrainImplementationId,
    ConversationBranchId, ConversationSnapshotId, DataBankScopeId, MessageBlockId, MessageId,
    MessageSlotId, MessageVariantId, ProfileId, ProfileRegistryLifecycleStatus, ResourceLimits,
    SessionHistoryWindow, SessionId, SessionKind,
};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CoreConfigFacadeArtifact {
    format_version: u32,
    source_crate: String,
    generated_module: String,
    wire_field_inventory: BTreeMap<String, Vec<String>>,
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
  emit-core-config-facade         Emit the generated TypeScript core-config facade helper.
  check-core-config-facade <path> Compare <path> with the generated core-config facade helper.
  emit-native-mapping-inventory   Emit generated TS inventory for bridge mapper drift checks.
  check-native-mapping-inventory <path>
                                  Compare <path> with the generated mapper inventory.

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

fn bridge_wire_shape_fingerprint() -> Result<String> {
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

    let contracts_source = fs::read_to_string(contracts_index_path).with_context(|| {
        format!(
            "failed to read TypeScript contracts file {}",
            contracts_index_path.display()
        )
    })?;
    let ts_value = wire_shape_fingerprint_from_ts_contracts(&contracts_source)?;
    if ts_value != expected {
        bail!(
            "TypeScript bridgeWireShapeFingerprint drift detected in {}; expected {}; update the export to match {}",
            contracts_index_path.display(),
            expected,
            fingerprint_path.display()
        );
    }
    Ok(())
}

fn check_contracts(contracts_index_path: &Path) -> Result<()> {
    let manifest_operation_names = operation_names_from_manifest(MANIFEST_TEXT)?;
    let rust_operation_names = OPERATION_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let contracts_source = fs::read_to_string(contracts_index_path).with_context(|| {
        format!(
            "failed to read TypeScript contracts file {}",
            contracts_index_path.display()
        )
    })?;
    let ts_operation_names = operation_names_from_ts_contracts(&contracts_source)?;

    compare_operation_sets(
        "bridge-manifest.toml [[operation]] names",
        &manifest_operation_names,
        "core_bridge_api::OPERATION_NAMES",
        &rust_operation_names,
    )?;
    compare_operation_order(
        "ts/packages/contracts manifestOperationNames",
        &ts_operation_names,
        "core_bridge_api::OPERATION_NAMES",
        &rust_operation_names,
    )?;

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
      toSnakeCaseKeys(item),
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

    Ok(CoreConfigFacadeArtifact {
        format_version: 1,
        source_crate: "rusty-crew-core-config".to_owned(),
        generated_module: "ts/packages/native-bridge/src/generated/core-config-facade.ts"
            .to_owned(),
        wire_field_inventory,
    })
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
    let conversation_operations = vec![
        "save_message_slot",
        "save_message_variant",
        "create_chat_message_slot",
        "create_chat_message_variant",
        "chat_read_model_page",
        "append_chat_event",
        "query_chat_events",
        "query_message_slots",
        "query_message_variants",
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
        "resolve_conversation_jump",
        "save_attachment",
        "create_chat_attachment",
        "query_attachments",
        "remove_attachment",
        "remove_chat_attachment",
        "save_data_bank_scope",
        "create_chat_data_bank_scope",
        "query_data_bank_scopes",
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
        "upsert_model_provider",
        "list_model_providers",
        "get_model_provider",
        "get_model_provider_secret",
        "model_provider_refresh_impact",
        "plan_model_provider_refresh",
    ];
    ensure_family_operations_exist("conversation", &conversation_operations)?;
    ensure_family_operations_exist("profile_registry", &profile_registry_operations)?;
    ensure_family_operations_exist("model_provider", &model_provider_operations)?;

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

    Ok(json!({
        "formatVersion": 1,
        "source": "rusty-crew-core-bridge-codegen",
        "families": {
            "conversation": {
                "operationNames": conversation_operations,
                "rawMethods": conversation_operations
                    .iter()
                    .map(|operation| operation_name_to_camel_json_method(operation))
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
                    .map(|operation| operation_name_to_camel_json_method(operation))
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
                    .map(|operation| operation_name_to_camel_json_method(operation))
                    .collect::<Vec<_>>(),
                "dtoFields": {
                    "RawModelProviderRecord": object_keys(&model_provider_record)?,
                    "RawModelProviderCredential": object_keys(required_value(&model_provider_record, "credential")?)?,
                    "RawModelProviderRefreshImpact": object_keys(&refresh_impact)?,
                    "RawModelProviderAffectedProfile": object_keys(first_array_item(&refresh_impact, "affected_profiles")?)?,
                    "RawModelProviderRefreshPlan": object_keys(&refresh_plan)?,
                    "RawModelProviderRefreshProfileAction": object_keys(first_array_item(&refresh_plan, "actions")?)?,
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
                collect_json_field_paths(child, &path, paths);
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
            profile_id: "field-created-profile".to_owned(),
            display_name: Some("Field Created Profile".to_owned()),
            agent_id: Some("field-created-agent".to_owned()),
            session_id: Some("field-created-session".to_owned()),
            implementation_id: Some("field-created-brain".to_owned()),
            kind: Some(SessionKind::Full),
            provider_alias: Some("field-provider".to_owned()),
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
                module: Some("pi-agent-core".to_owned()),
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
        brains: vec![BrainConfigDraft {
            implementation_id: BrainImplementationId::new("field-sample-brain"),
            profile_id: ProfileId::new("field-sample-profile"),
        }],
        sessions: vec![SessionConfigDraft {
            session_id: SessionId::new("field-sample-session"),
            agent_id: AgentId::new("field-sample-agent"),
            profile_id: ProfileId::new("field-sample-profile"),
            kind: SessionKind::Full,
            resource_limits: Some(sample_resource_limits()),
            owner_id: Some("field-owner".to_owned()),
            history_window: Some(SessionHistoryWindow {
                max_messages: Some(128),
            }),
            max_history_messages: Some(256),
            turn_timeout_ms: Some(60_000),
        }],
        scheduled_jobs: vec![ScheduledJobConfigDraft {
            id: "field-sample-job".to_owned(),
            schedule: "*/5 * * * *".to_owned(),
            shape: ScheduledJobShape::SessionWake,
            job_kind: Some("runtime.review.memory_skills".to_owned()),
            target_session_id: Some(SessionId::new("field-sample-session")),
            script: Some("field-script".to_owned()),
            delivery_channel_id: Some("field-delivery-channel".to_owned()),
        }],
        channel_bindings: vec![ChannelBindingConfigDraft {
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
        }],
        mcp_bindings: vec![McpBindingConfigDraft {
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
        }],
    }
}

fn sample_profile_runtime_metadata() -> ProfileRuntimeMetadata {
    ProfileRuntimeMetadata {
        profile_id: ProfileId::new("field-sample-profile"),
        brain: Some(ProfileBrainMetadata {
            module: Some("pi-agent-core".to_owned()),
            strategy: Some("default".to_owned()),
        }),
        runtime: Some(ProfileRuntimeOptions {
            default_resource_limits: Some(sample_resource_limits()),
            max_turn_duration_ms: Some(120_000),
            max_tokens_per_turn: Some(4096),
        }),
        session_defaults: Some(ProfileSessionDefaults {
            owner_id: Some("field-owner".to_owned()),
            max_history_messages: Some(512),
            turn_timeout_ms: Some(60_000),
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
        workdir: Some("/home/dev/rusty-crew".to_owned()),
        max_duration_ms: Some(3_600_000),
        max_delegation_depth: Some(4),
    }
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
                name: "brain_wake_stream_result_v1".to_owned(),
                operation: "run_openai_responses_brain".to_owned(),
                direction: "rust_to_ts".to_owned(),
                rust_type: "Vec<rusty_crew_core_protocol::BrainWakeStreamItem>".to_owned(),
                value: serde_json::to_value(BrainWakeStreamResultFixture {
                    stream: sample_brain_wake_stream(),
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

fn operation_names_from_ts_contracts(source: &str) -> Result<Vec<String>> {
    let marker = "export const manifestOperationNames = [";
    let start = source
        .find(marker)
        .context("failed to find manifestOperationNames export in TypeScript contracts")?
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find("] as const")
        .context("failed to find end of manifestOperationNames export")?;
    let block = &rest[..end];
    let mut names = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('"') {
            continue;
        }
        let Some(end_quote) = trimmed[1..].find('"') else {
            bail!("malformed manifestOperationNames line `{trimmed}`");
        };
        names.push(trimmed[1..1 + end_quote].to_owned());
    }
    ensure_no_duplicate_operations("TypeScript contracts manifestOperationNames", &names)?;
    Ok(names)
}

fn wire_shape_fingerprint_from_ts_contracts(source: &str) -> Result<String> {
    let marker = "export const bridgeWireShapeFingerprint";
    let start = source
        .find(marker)
        .context("failed to find bridgeWireShapeFingerprint export in TypeScript contracts")?
        + marker.len();
    let rest = &source[start..];
    let quote = rest
        .find('"')
        .context("failed to find bridgeWireShapeFingerprint string literal")?;
    let value = &rest[quote + 1..];
    let end = value
        .find('"')
        .context("failed to find end of bridgeWireShapeFingerprint export")?;
    Ok(value[..end].to_owned())
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
    trimmed
        .split(',')
        .filter(|part| !part.trim().is_empty())
        .count()
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

fn sample_session_state() -> SessionState {
    SessionState {
        handle: SessionHandle::new(1),
        session_id: sample_session_id(),
        agent_id: sample_agent_id(),
        profile_id: sample_profile_id(),
        kind: SessionKind::Full,
        delegation: None,
        resource_limits: ResourceLimits {
            workdir: Some("/home".to_owned()),
            max_duration_ms: None,
            max_delegation_depth: Some(3),
        },
        tool_profile: ToolProfile {
            tools: vec![ToolDescriptor {
                name: "send_message".to_owned(),
                description: "Send a direct runtime message.".to_owned(),
                input_schema: Some(RuntimeBufferHandle::new(42)),
            }],
        },
        history_window: Some(SessionHistoryWindow {
            max_messages: Some(200),
        }),
        status: SessionStatus::Idle,
        brain_turn_count: 7,
        created_at: sample_timestamp(),
        last_active_at: sample_timestamp(),
    }
}

fn sample_agent_message() -> AgentMessage {
    AgentMessage {
        from: sample_agent_id(),
        to: AgentId::new("operator"),
        body: "Bridge validation fixture message.".to_owned(),
        correlation_id: Some("validation-correlation".to_owned()),
        projection: None,
    }
}

fn sample_query_page() -> persistence::QueryPage {
    persistence::QueryPage {
        limit: Some(25),
        offset: Some(5),
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
        credential: ModelProviderCredential {
            has_secret: true,
            secret_ref: Some("db://model_providers/validation-provider/secret".to_owned()),
            updated_at: Some(sample_timestamp()),
            kind: Some(ModelProviderCredentialKind::ApiKey),
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

fn sample_context_compaction_artifact() -> ContextCompactionArtifact {
    ContextCompactionArtifact {
        artifact_id: "compaction_one".to_owned(),
        session_id: sample_session_id(),
        branch_id: None,
        strategy_id: "rolling_summary".to_owned(),
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
