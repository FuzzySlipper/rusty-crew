use anyhow::{bail, Context, Result};
use rusty_crew_core_bridge_api::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, fs, path::Path};

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
            let operation_count = manifest_operation_count();
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

The fixtures are an incremental drift-check scaffold. They do not replace the
bridge manifest operation inventory; they give TS validation smokes a Rust
serialization source for covered bridge families while full codegen matures."
    );
}

fn manifest_operation_count() -> usize {
    MANIFEST_TEXT.matches("[[operation]]").count()
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

fn bridge_validation_fixture_file() -> Result<BridgeValidationFixtureFile> {
    Ok(BridgeValidationFixtureFile {
        format_version: 1,
        manifest_version: MANIFEST_VERSION,
        operation_count: manifest_operation_count(),
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
        ],
    })
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
