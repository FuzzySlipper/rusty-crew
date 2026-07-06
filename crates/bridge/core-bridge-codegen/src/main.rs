use anyhow::{bail, Context, Result};
use rusty_crew_core_bridge_api::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, env, fs, path::Path};

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
            check_native_surface(Path::new(&path))?;
            println!("bridge native surface inventory check passed");
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
  check-native-surface <path>     Check generated napi *Json methods have manifest entries.

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

fn check_native_surface(native_index_path: &Path) -> Result<()> {
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
    )
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
    let marker = "export declare class NativeBridgeBinding {";
    let start = source
        .find(marker)
        .context("failed to find NativeBridgeBinding class in native declaration file")?
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find("\n}")
        .context("failed to find end of NativeBridgeBinding class in native declaration file")?;
    let block = &rest[..end];
    let mut names = Vec::new();

    for line in block.lines() {
        let trimmed = line.trim();
        let Some(open_paren) = trimmed.find('(') else {
            continue;
        };
        let method_name = &trimmed[..open_paren];
        if !method_name.ends_with("Json") {
            continue;
        }
        names.push(camel_json_method_to_operation_name(method_name)?);
    }

    ensure_no_duplicate_operations("generated napi NativeBridgeBinding *Json methods", &names)?;
    Ok(names)
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

fn sample_session_id() -> SessionId {
    SessionId::new("validation-session")
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
