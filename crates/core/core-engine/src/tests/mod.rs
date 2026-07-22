use super::*;
use rusty_crew_core_config::ClockConfig;
#[cfg(feature = "postgres")]
use rusty_crew_core_config::EngineStorageConfig;
use rusty_crew_core_persistence::{
    ActiveVariantConflict, ActiveVariantExpectation, AgentMessageQuery, AttachmentLinkWrite,
    AttachmentStatus, BranchHeadConflict, ChatAttachmentMutationStatus,
    ChatConversationSnapshotMutationStatus, ChatDataBankScopeMutationStatus, CompletionPacketQuery,
    ConversationJumpTarget, ConversationSnapshotSource, CoordinationStore, DataBankScopeStatus,
    DurableMessageStatus, DurableMessageWrite, MessageVariantSource, MessageVariantStatus,
    QueryPage, QueuedMessageFilter, QueuedMessageRecord, QueuedMessageState, RuntimeCounterScope,
    RuntimeMaintenancePolicy, RuntimeSearchFilter, RuntimeSearchRowType, ScheduledRunQuery,
    ScheduledRunStatus, SessionQuery, ToolCallPhase, WorkerRunQuery,
};
use rusty_crew_core_protocol::SessionHistoryWindow;
use rusty_crew_core_protocol::{
    AdapterId, AgentDirectoryRuntimeKind, AgentId, AgentMessage, AttachmentLinkId, BrainAction,
    BrainEvent, CompletionPacket, CompletionStatus, ConversationBranchId, ConversationSnapshotId,
    CoreErrorKind, CoreEventKind, DelegatedRunStatus, DelegationLifecyclePhase,
    ExternalEventPayload, MessageId, ProfileId, ProjectId, ResourceLimits, SessionKind, TaskId,
    ToolCallMetadata, ToolCallPolicyMetadata, ToolCallSource, ToolDescriptor, ToolProfile,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

mod chat_support;
mod delegation_support;
mod github_gate;
use chat_support::*;
use delegation_support::*;
mod agent_routes;
mod body;
mod bootstrap_sessions;
mod brain_runtime;
mod chat_mutations;
mod chat_read;
mod delegation_fanout;
mod delegation_lifecycle;
mod external_runtime;
mod external_runtime_certification;
mod external_runtime_controls;
mod external_serial_inbox;
mod profile_admin;
mod restart_hydration;
mod roleplay_proposals;
mod runtime_activity;
mod scheduler;

fn test_engine() -> CoreEngine {
    test_engine_with_data_dir(unique_data_dir("engine"))
}

fn test_engine_with_data_dir(data_dir: PathBuf) -> CoreEngine {
    CoreEngine::initialize(test_engine_config(data_dir)).unwrap()
}

fn test_engine_config(data_dir: PathBuf) -> EngineConfig {
    EngineConfig {
        engine_data_dir: data_dir.to_string_lossy().to_string(),
        clock: ClockConfig::Fixed {
            at: "2026-06-19T00:00:00Z".to_string(),
        },
        default_turn_budget: 3,
        default_idle_timeout_ms: 1000,
        storage: None,
    }
}

fn unique_data_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rusty-crew-{name}-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::remove_file(&path);
    path
}

fn assert_receiver_disconnects_after_buffered_events(
    receiver: std::sync::mpsc::Receiver<CoreEvent>,
) {
    for _ in 0..8 {
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("subscriber receiver remained open after shutdown")
            }
        }
    }
    panic!("subscriber receiver still had buffered events after shutdown");
}

fn session_config(
    session_id: &str,
    agent_id: &str,
    profile_id: &str,
    kind: SessionKind,
) -> SessionConfig {
    SessionConfig {
        session_id: SessionId::new(session_id),
        agent_id: AgentId::new(agent_id),
        profile_id: ProfileId::new(profile_id),
        kind,
        delegation: None,
        resource_limits: ResourceLimits {
            workdir: Some("/home/dev/rusty-crew".to_string()),
            max_duration_ms: Some(60_000),
            max_delegation_depth: Some(1),
        },
        tool_profile: ToolProfile {
            tools: vec![ToolDescriptor {
                name: "patch".to_string(),
                description: "Apply a source patch".to_string(),
                input_schema: None,
            }],
        },
        history_window: None,
    }
}

fn profile_registry_write(
    profile_id: &str,
    provider_alias: &str,
    configured_session_id: &str,
) -> ProfileRegistryWrite {
    ProfileRegistryWrite {
        profile_id: ProfileId::new(profile_id),
        lifecycle_status: rusty_crew_core_protocol::ProfileRegistryLifecycleStatus::Active,
        display_name: None,
        summary: None,
        default_session_kind: Some(SessionKind::Full),
        agent_id: Some(AgentId::new(profile_id)),
        owner_id: None,
        prompt_soul_markdown: None,
        prompt_memory_markdown: None,
        active_runtime_settings_json: serde_json::json!({
            "provider_alias": provider_alias,
        }),
        source_asset_refs: vec![],
        derived_runtime_refs: vec![rusty_crew_core_protocol::ProfileRegistryDerivedRuntimeRef {
            ref_kind: "session".to_string(),
            ref_id: configured_session_id.to_string(),
            status: "active".to_string(),
            updated_at: None,
            metadata_json: serde_json::json!({}),
        }],
        import_export: rusty_crew_core_protocol::ProfileRegistryImportExportMetadata {
            imported_from: None,
            imported_at: None,
            exported_to: None,
            exported_at: None,
            metadata_json: serde_json::json!({}),
        },
        now: "2026-06-19T00:00:00Z".to_string(),
    }
}
