//! Backend-neutral persistence records, query shapes, and facade structs.
//!
//! This module is intentionally free of SQLite/PostgreSQL SQL behavior. It
//! defines the durable storage contracts that both backends implement and that
//! callers consume through `core-persistence` re-exports.

use super::*;

#[derive(Debug, Clone, Copy)]
pub struct CoordinationRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceDataRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, Copy)]
pub struct ConversationRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, Copy)]
pub struct ChatEventRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, Copy)]
pub struct ModuleDataRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, Copy)]
pub struct StorageAdminRepositorySet<'a> {
    pub(crate) store: &'a CoreCoordinationStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaMigrationRecord {
    pub version: i64,
    pub description: String,
    pub applied_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfigRecord {
    pub session_id: SessionId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub resource_limits: ResourceLimits,
    pub tool_profile: ToolProfile,
    pub config: SessionConfig,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedEvent {
    pub sequence: u64,
    pub event: CoreEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryPage {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl QueryPage {
    pub(crate) fn bounded(self, default_limit: u32, max_limit: u32) -> (i64, i64) {
        (
            self.limit.unwrap_or(default_limit).clamp(1, max_limit) as i64,
            self.offset.unwrap_or(0) as i64,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionQuery {
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub kind: Option<SessionKind>,
    pub status: Option<SessionStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileRegistryQuery {
    pub lifecycle_status: Option<ProfileRegistryLifecycleStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentInstanceQuery {
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub status: Option<DurableIdentityStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessageRecord {
    pub sequence: u64,
    pub message: AgentMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentMessageQuery {
    pub agent_id: Option<AgentId>,
    pub correlation_id: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageVariantSource {
    Primary,
    Alternate,
}

impl MessageVariantSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Alternate => "alternate",
        }
    }

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "primary" => Ok(Self::Primary),
            "alternate" => Ok(Self::Alternate),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported message variant source {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageVariantStatus {
    Active,
    Deleted,
}

impl MessageVariantStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deleted => "deleted",
        }
    }

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "deleted" => Ok(Self::Deleted),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported message variant status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableMessageStatus {
    Created,
    Streaming,
    Completed,
    Failed,
    Deleted,
}

impl DurableMessageStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Streaming => "streaming",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "created" => Ok(Self::Created),
            "streaming" => Ok(Self::Streaming),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "deleted" => Ok(Self::Deleted),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported durable message status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBlockRecord {
    pub block_id: MessageBlockId,
    pub message_id: MessageId,
    pub ordinal: u32,
    pub kind: String,
    pub content_json: JsonValue,
    pub render_policy_json: Option<JsonValue>,
    pub metadata_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableMessageRecord {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub previous_message_id: Option<MessageId>,
    pub author_id: String,
    pub author_role: String,
    pub status: DurableMessageStatus,
    pub body: String,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub blocks: Vec<MessageBlockRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageVariantRecord {
    pub variant_id: MessageVariantId,
    pub slot_id: MessageSlotId,
    pub source: MessageVariantSource,
    pub ordinal: u32,
    pub status: MessageVariantStatus,
    pub message: DurableMessageRecord,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSlotRecord {
    pub slot_id: MessageSlotId,
    pub session_id: SessionId,
    pub primary_variant_id: MessageVariantId,
    pub active_variant_id: Option<MessageVariantId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub version: u64,
    pub primary: MessageVariantRecord,
    pub alternates: Vec<MessageVariantRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBlockWrite {
    pub block_id: MessageBlockId,
    pub ordinal: u32,
    pub kind: String,
    pub content_json: JsonValue,
    pub render_policy_json: Option<JsonValue>,
    pub metadata_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableMessageWrite {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub previous_message_id: Option<MessageId>,
    pub author_id: String,
    pub author_role: String,
    pub status: DurableMessageStatus,
    pub body: String,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub blocks: Vec<MessageBlockWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageVariantWrite {
    pub variant_id: MessageVariantId,
    pub slot_id: MessageSlotId,
    pub source: MessageVariantSource,
    pub ordinal: u32,
    pub status: MessageVariantStatus,
    pub message: DurableMessageWrite,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSlotWrite {
    pub slot_id: MessageSlotId,
    pub session_id: SessionId,
    pub primary_variant_id: MessageVariantId,
    pub active_variant_id: Option<MessageVariantId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageSlotQuery {
    pub session_id: Option<SessionId>,
    pub include_alternates: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReadModelQuery {
    pub session_id: SessionId,
    pub agent_id: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReadModelPage {
    pub items: Vec<ChatReadModelEvent>,
    pub latest_cursor: String,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReadModelEvent {
    pub event_id: String,
    pub session_id: SessionId,
    pub sequence_id: u64,
    pub created_at: IsoTimestamp,
    pub kind: ChatReadModelEventKind,
    #[serde(rename = "payload")]
    pub payload_json: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatReadModelEventKind {
    MessageCreated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEventLogAppend {
    pub session_id: SessionId,
    pub created_at: IsoTimestamp,
    pub kind: String,
    #[serde(rename = "payload")]
    pub payload_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEventLogQuery {
    pub session_id: SessionId,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEventLogPage {
    pub items: Vec<ChatEventLogEvent>,
    pub latest_cursor: String,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEventLogEvent {
    pub event_id: String,
    pub session_id: SessionId,
    pub sequence_id: u64,
    pub created_at: IsoTimestamp,
    pub kind: String,
    #[serde(rename = "payload")]
    pub payload_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageVariantQuery {
    pub slot_id: Option<MessageSlotId>,
    pub include_deleted: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "variant_id", rename_all = "snake_case")]
pub enum ActiveVariantExpectation {
    Any,
    Primary,
    Variant(MessageVariantId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveVariantRequest {
    pub slot_id: MessageSlotId,
    pub active_variant_id: Option<MessageVariantId>,
    pub expected: ActiveVariantExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveVariantResult {
    pub slot: MessageSlotRecord,
    pub conflict: Option<ActiveVariantConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveChatMessageVariantRequest {
    pub session_id: SessionId,
    pub slot_id: MessageSlotId,
    pub active_variant_id: Option<MessageVariantId>,
    pub expected: ActiveVariantExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveChatMessageVariantResult {
    pub slot: MessageSlotRecord,
    pub conflict: Option<ActiveVariantConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatMessageSlotRequest {
    pub slot: MessageSlotWrite,
    pub primary_variant: MessageVariantWrite,
    pub branch_id: ConversationBranchId,
    pub expected_branch_head: BranchHeadExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatMessageSlotResult {
    pub slot: Option<MessageSlotRecord>,
    pub branch: ConversationBranchRecord,
    pub conflict: Option<BranchHeadConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatMessageVariantRequest {
    pub session_id: SessionId,
    pub slot_id: MessageSlotId,
    pub variant: MessageVariantWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatMessageVariantResult {
    pub variant: MessageVariantRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteChatMessageVariantRequest {
    pub session_id: SessionId,
    pub slot_id: MessageSlotId,
    pub variant_id: MessageVariantId,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReorderChatMessageVariantsRequest {
    pub session_id: SessionId,
    pub slot_id: MessageSlotId,
    pub ordered_variant_ids: Vec<MessageVariantId>,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveVariantConflict {
    pub expected: Option<MessageVariantId>,
    pub actual: Option<MessageVariantId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBranchRecord {
    pub branch_id: ConversationBranchId,
    pub session_id: SessionId,
    pub parent_branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub origin_message_id: Option<MessageId>,
    pub head_message_id: Option<MessageId>,
    pub label: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBranchWrite {
    pub branch_id: ConversationBranchId,
    pub session_id: SessionId,
    pub parent_branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub origin_message_id: Option<MessageId>,
    pub head_message_id: Option<MessageId>,
    pub label: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatConversationBranchRequest {
    pub branch: ConversationBranchWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureActiveChatConversationBranchRequest {
    pub session_id: SessionId,
    pub branch_id: ConversationBranchId,
    pub label: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureActiveChatConversationBranchResult {
    pub branch: ConversationBranchRecord,
    pub state: ConversationBranchStateRecord,
    pub conflict: Option<ActiveBranchConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConversationBranchQuery {
    pub session_id: Option<SessionId>,
    pub parent_branch_id: Option<ConversationBranchId>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBranchStateRecord {
    pub session_id: SessionId,
    pub active_branch_id: Option<ConversationBranchId>,
    pub updated_at: IsoTimestamp,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "branch_id", rename_all = "snake_case")]
pub enum ActiveBranchExpectation {
    Any,
    None,
    Branch(ConversationBranchId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveBranchRequest {
    pub session_id: SessionId,
    pub active_branch_id: Option<ConversationBranchId>,
    pub expected: ActiveBranchExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveBranchResult {
    pub state: ConversationBranchStateRecord,
    pub conflict: Option<ActiveBranchConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBranchConflict {
    pub expected: Option<ConversationBranchId>,
    pub actual: Option<ConversationBranchId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "message_id", rename_all = "snake_case")]
pub enum BranchHeadExpectation {
    Any,
    None,
    Message(MessageId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateBranchHeadRequest {
    pub branch_id: ConversationBranchId,
    pub head_message_id: Option<MessageId>,
    pub expected: BranchHeadExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateBranchHeadResult {
    pub branch: ConversationBranchRecord,
    pub conflict: Option<BranchHeadConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchHeadConflict {
    pub expected: Option<MessageId>,
    pub actual: Option<MessageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSnapshotSource {
    User,
    System,
    Import,
}

impl ConversationSnapshotSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Import => "import",
        }
    }

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "import" => Ok(Self::Import),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported conversation snapshot source {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSnapshotRecord {
    pub snapshot_id: ConversationSnapshotId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub cursor: Option<String>,
    pub label: Option<String>,
    pub summary: Option<String>,
    pub source: ConversationSnapshotSource,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSnapshotWrite {
    pub snapshot_id: ConversationSnapshotId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub cursor: Option<String>,
    pub label: Option<String>,
    pub summary: Option<String>,
    pub source: ConversationSnapshotSource,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatConversationSnapshotMutationStatus {
    Created,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatConversationSnapshotRequest {
    pub snapshot: ConversationSnapshotWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatConversationSnapshotResult {
    pub status: ChatConversationSnapshotMutationStatus,
    pub snapshot: ConversationSnapshotRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConversationSnapshotQuery {
    pub session_id: Option<SessionId>,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationJumpTarget {
    Message { message_id: MessageId },
    Branch { branch_id: ConversationBranchId },
    Snapshot { snapshot_id: ConversationSnapshotId },
    Cursor { cursor: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationJumpRequest {
    pub session_id: SessionId,
    pub target: ConversationJumpTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationJumpResult {
    pub session_id: SessionId,
    pub target: ConversationJumpTarget,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub cursor: Option<String>,
    pub snapshot_id: Option<ConversationSnapshotId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentStatus {
    Active,
    Removed,
}

impl AttachmentStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Removed => "removed",
        }
    }

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "removed" => Ok(Self::Removed),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported attachment status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLinkRecord {
    pub link_id: AttachmentLinkId,
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub message_id: Option<MessageId>,
    pub block_id: Option<MessageBlockId>,
    pub scope_id: Option<DataBankScopeId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRecord {
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub status: AttachmentStatus,
    pub filename: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub storage_url: Option<String>,
    pub download_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub extracted_text: Option<String>,
    pub extracted_text_truncated: bool,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub links: Vec<AttachmentLinkRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentWrite {
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub status: AttachmentStatus,
    pub filename: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub storage_url: Option<String>,
    pub download_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub extracted_text: Option<String>,
    pub extracted_text_truncated: bool,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub link: Option<AttachmentLinkWrite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatAttachmentMutationStatus {
    Created,
    Updated,
    Linked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatAttachmentRequest {
    pub attachment: AttachmentWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatAttachmentResult {
    pub status: ChatAttachmentMutationStatus,
    pub attachment: AttachmentRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveChatAttachmentRequest {
    pub session_id: SessionId,
    pub attachment_id: AttachmentId,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLinkWrite {
    pub link_id: AttachmentLinkId,
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub message_id: Option<MessageId>,
    pub block_id: Option<MessageBlockId>,
    pub scope_id: Option<DataBankScopeId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AttachmentQuery {
    pub session_id: Option<SessionId>,
    pub message_id: Option<MessageId>,
    pub block_id: Option<MessageBlockId>,
    pub scope_id: Option<DataBankScopeId>,
    pub status: Option<AttachmentStatus>,
    pub include_removed: bool,
    pub include_expired: bool,
    pub expired_only: bool,
    pub now: Option<IsoTimestamp>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataBankScopeStatus {
    Active,
    Removed,
}

impl DataBankScopeStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Removed => "removed",
        }
    }

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "removed" => Ok(Self::Removed),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported data-bank scope status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataBankScopeRecord {
    pub scope_id: DataBankScopeId,
    pub session_id: SessionId,
    pub status: DataBankScopeStatus,
    pub label: Option<String>,
    pub description: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataBankScopeWrite {
    pub scope_id: DataBankScopeId,
    pub session_id: SessionId,
    pub status: DataBankScopeStatus,
    pub label: Option<String>,
    pub description: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatDataBankScopeMutationStatus {
    Created,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatDataBankScopeRequest {
    pub scope: DataBankScopeWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChatDataBankScopeResult {
    pub status: ChatDataBankScopeMutationStatus,
    pub scope: DataBankScopeRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveChatDataBankScopeRequest {
    pub session_id: SessionId,
    pub scope_id: DataBankScopeId,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DataBankScopeQuery {
    pub session_id: Option<SessionId>,
    pub status: Option<DataBankScopeStatus>,
    pub include_removed: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionPacketRecord {
    pub sequence: u64,
    pub packet: CompletionPacket,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionPacketQuery {
    pub session_id: Option<SessionId>,
    pub status: Option<rusty_crew_core_protocol::CompletionStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkerRunQuery {
    pub parent_session_id: Option<SessionId>,
    pub delegated_session_id: Option<SessionId>,
    pub profile_id: Option<ProfileId>,
    pub task_id: Option<TaskId>,
    pub status: Option<WorkerRunStatus>,
    pub terminal: Option<bool>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeCounterQuery {
    pub scope: Option<RuntimeCounterScope>,
    pub counter_name: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMemoryTarget {
    Profile,
    User(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryCaps {
    pub max_records_per_profile: u32,
    pub max_key_bytes: u32,
    pub max_content_bytes: u32,
}

impl Default for ProfileMemoryCaps {
    fn default() -> Self {
        Self {
            max_records_per_profile: 64,
            max_key_bytes: 128,
            max_content_bytes: 8 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryRecord {
    pub profile_id: ProfileId,
    pub target: ProfileMemoryTarget,
    pub key: String,
    pub content: String,
    pub metadata: JsonValue,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryWrite {
    pub profile_id: ProfileId,
    pub target: ProfileMemoryTarget,
    pub key: String,
    pub content: String,
    pub metadata: JsonValue,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryReplace {
    pub write: ProfileMemoryWrite,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryDelete {
    pub profile_id: ProfileId,
    pub target: ProfileMemoryTarget,
    pub key: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryQuery {
    pub profile_id: ProfileId,
    pub target: Option<ProfileMemoryTarget>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryRecordStatus {
    Active,
    Superseded,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemoryRecord {
    pub record_id: String,
    pub session_id: SessionId,
    pub scope: MemoryScope,
    pub branch_id: Option<ConversationBranchId>,
    pub shape: MemoryRecordShapeRef,
    pub status: SessionMemoryRecordStatus,
    pub revision: u64,
    pub content: JsonValue,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub source: MemoryProposalSource,
    pub confidence: f32,
    pub durability_rationale: String,
    pub supersedes_record_id: Option<String>,
    pub superseded_by_record_id: Option<String>,
    pub archived_at: Option<IsoTimestamp>,
    pub archive_reason: Option<String>,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemoryRecordWrite {
    pub record_id: String,
    pub session_id: SessionId,
    pub scope: MemoryScope,
    pub branch_id: Option<ConversationBranchId>,
    pub shape: MemoryRecordShapeRef,
    pub content: JsonValue,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub source: MemoryProposalSource,
    pub confidence: f32,
    pub durability_rationale: String,
    pub supersedes_record_id: Option<String>,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemoryReplace {
    pub record_id: String,
    pub expected_revision: u64,
    pub content: JsonValue,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub source: MemoryProposalSource,
    pub confidence: f32,
    pub durability_rationale: String,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemorySupersede {
    pub record_id: String,
    pub expected_revision: u64,
    pub replacement: SessionMemoryRecordWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMemoryArchive {
    pub record_id: String,
    pub expected_revision: u64,
    pub reason: Option<String>,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionMemoryQuery {
    pub session_id: Option<SessionId>,
    pub branch_id: Option<ConversationBranchId>,
    pub scope_type: Option<MemoryScopeType>,
    pub shape_id: Option<String>,
    pub include_superseded: bool,
    pub include_archived: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchAwareSessionMemoryQuery {
    pub session_id: SessionId,
    pub active_branch_id: Option<ConversationBranchId>,
    pub include_ancestors: bool,
    pub include_siblings: bool,
    pub shape_id: Option<String>,
    pub prompt_context_only: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemoryPromptContext {
    pub records: Vec<SessionMemoryRecord>,
    pub diagnostics: SessionMemoryPromptDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMemoryPromptDiagnostics {
    pub descriptor_id: String,
    pub descriptor_schema_version: u32,
    pub session_id: SessionId,
    pub active_branch_id: Option<ConversationBranchId>,
    pub selected_records: Vec<SessionMemorySelectedRecordDiagnostic>,
    pub excluded_counts: SessionMemoryPromptExcludedCounts,
    pub character_estimate: u64,
    pub token_estimate: u64,
    pub context_policy: SessionMemoryPromptContextPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCharacterRecord {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub scenario: String,
    #[serde(default)]
    pub first_message: String,
    #[serde(default)]
    pub alternate_greetings: Vec<String>,
    #[serde(default)]
    pub example_messages: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub status: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayCharacterWrite {
    pub record: RoleplayCharacterRecord,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleplayCharacterQuery {
    pub profile_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayPlayerPersonaRecord {
    pub id: String,
    pub profile_id: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_asset_ref: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub notes: String,
    pub status: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPlayerPersonaWrite {
    pub record: RoleplayPlayerPersonaRecord,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleplayPlayerPersonaQuery {
    pub profile_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplaySessionMetadataRecord {
    pub session_id: String,
    pub profile_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub player_persona_id: Option<String>,
    #[serde(default)]
    pub character_id: Option<String>,
    #[serde(default)]
    pub active_layer_ids: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplaySessionMetadataWrite {
    pub record: RoleplaySessionMetadataRecord,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleplaySessionMetadataQuery {
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default)]
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplaySessionProjectionWrite {
    pub metadata: RoleplaySessionMetadataWrite,
    #[serde(default)]
    pub chat_layers: Option<RoleplayChatLayersWrite>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplaySessionProjectionRecord {
    pub metadata: RoleplaySessionMetadataRecord,
    pub chat_layers: Vec<RoleplayChatLayerRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayImportCounts {
    pub characters: u64,
    pub personas: u64,
    pub lore_entries: u64,
    pub messages: u64,
    pub assistant_variant_rows: u64,
    pub assistant_multi_swipe_rows: u64,
    pub variants: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayImportRecord {
    pub import_id: String,
    pub profile_id: String,
    pub source_kind: String,
    #[serde(default)]
    pub provenance: JsonValue,
    #[serde(default)]
    pub raw_source: Option<JsonValue>,
    #[serde(default)]
    pub character_id: Option<String>,
    #[serde(default)]
    pub persona_id: Option<String>,
    #[serde(default)]
    pub lore_layer_id: Option<String>,
    pub session_id: String,
    pub counts: RoleplayImportCounts,
    pub status: String,
    #[serde(default)]
    pub failure_reason: Option<String>,
    pub revision: u64,
    pub imported_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayImportWrite {
    pub record: RoleplayImportRecord,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleplayImportQuery {
    pub profile_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMemorySelectedRecordDiagnostic {
    pub record_id: String,
    pub shape_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionMemoryPromptExcludedCounts {
    pub wrong_branch: u64,
    pub sibling_branch: u64,
    pub tool_only: u64,
    pub archived: u64,
    pub superseded: u64,
    pub limit_exceeded: u64,
    pub policy_disabled: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryPromptContextPolicy {
    SummaryContext,
    ToolOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayLoreRecordStatus {
    Active,
    Superseded,
    Tombstoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayLoreCanonStatus {
    Canon,
    Draft,
    Contested,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayLoreVisibility {
    Public,
    Private,
    GmOnly,
    ToolOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreRecord {
    pub record_id: String,
    pub world_id: String,
    pub entity_id: Option<String>,
    pub session_id: Option<SessionId>,
    pub branch_id: Option<ConversationBranchId>,
    pub shape: MemoryRecordShapeRef,
    pub canon_status: RoleplayLoreCanonStatus,
    pub visibility: RoleplayLoreVisibility,
    pub status: RoleplayLoreRecordStatus,
    pub revision: u64,
    pub title: String,
    pub body: String,
    pub content: JsonValue,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub source: MemoryProposalSource,
    pub confidence: f32,
    pub durability_rationale: String,
    pub supersedes_record_id: Option<String>,
    pub superseded_by_record_id: Option<String>,
    pub tombstoned_at: Option<IsoTimestamp>,
    pub tombstone_reason: Option<String>,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreWrite {
    pub record_id: String,
    pub world_id: String,
    pub entity_id: Option<String>,
    pub session_id: Option<SessionId>,
    pub branch_id: Option<ConversationBranchId>,
    pub shape: MemoryRecordShapeRef,
    pub canon_status: RoleplayLoreCanonStatus,
    pub visibility: RoleplayLoreVisibility,
    pub title: String,
    pub body: String,
    pub content: JsonValue,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub source: MemoryProposalSource,
    pub confidence: f32,
    pub durability_rationale: String,
    pub supersedes_record_id: Option<String>,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreReplace {
    pub write: RoleplayLoreWrite,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreSupersede {
    pub record_id: String,
    pub expected_revision: u64,
    pub replacement: RoleplayLoreWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreTombstone {
    pub record_id: String,
    pub expected_revision: u64,
    pub reason: Option<String>,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleplayLoreQuery {
    pub world_id: Option<String>,
    pub entity_id: Option<String>,
    pub canon_status: Option<RoleplayLoreCanonStatus>,
    pub visibility: Option<RoleplayLoreVisibility>,
    pub shape_id: Option<String>,
    pub provenance_ref_id: Option<String>,
    pub query: Option<String>,
    pub include_superseded: bool,
    pub include_tombstoned: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreProvenanceEvent {
    pub event_id: String,
    pub record_id: String,
    pub world_id: String,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub source: MemoryProposalSource,
    pub actor: String,
    pub note: Option<String>,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayLoreLayerPurpose {
    World,
    Story,
    Characters,
    Factions,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayLoreLayerWritePolicy {
    Manual,
    AutoCapture,
    Readonly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerRecord {
    pub layer_id: String,
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub purpose: RoleplayLoreLayerPurpose,
    pub write_policy: RoleplayLoreLayerWritePolicy,
    pub is_archived: bool,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerWrite {
    pub layer_id: String,
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub purpose: RoleplayLoreLayerPurpose,
    pub write_policy: RoleplayLoreLayerWritePolicy,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerUpdate {
    pub layer_id: String,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub purpose: Option<RoleplayLoreLayerPurpose>,
    pub write_policy: Option<RoleplayLoreLayerWritePolicy>,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerArchive {
    pub layer_id: String,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerConfigRecord {
    pub config_id: String,
    pub layer_id: String,
    pub fts_weight: f32,
    pub subject_weight: f32,
    pub canon_weight: f32,
    pub tag_boost_weight: f32,
    pub recency_weight: f32,
    pub default_token_budget: u32,
    pub constant_token_reserve: u32,
    pub min_relevance_score: f32,
    pub max_constants: u32,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerConfigWrite {
    pub config_id: String,
    pub layer_id: String,
    pub fts_weight: f32,
    pub subject_weight: f32,
    pub canon_weight: f32,
    pub tag_boost_weight: f32,
    pub recency_weight: f32,
    pub default_token_budget: u32,
    pub constant_token_reserve: u32,
    pub min_relevance_score: f32,
    pub max_constants: u32,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerEntryLink {
    pub layer_id: String,
    pub record_id: String,
    pub is_constant: bool,
    pub priority: i64,
    pub added_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreLayerEntryJoin {
    pub layer_id: String,
    pub record_id: String,
    pub is_constant: bool,
    pub priority: i64,
    pub added_at: IsoTimestamp,
    pub record: RoleplayLoreRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayLoreFactCapture {
    pub layer_id: String,
    pub write: RoleplayLoreWrite,
    pub is_constant: bool,
    pub priority: i64,
    pub capture_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayLoreEntryPromotion {
    pub source_layer_id: String,
    pub source_record_id: String,
    pub target_layer_id: String,
    pub new_record_id: String,
    pub is_constant: bool,
    pub priority: i64,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayChatLayerLink {
    pub layer_id: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayChatLayersWrite {
    pub chat_id: String,
    pub layers: Vec<RoleplayChatLayerLink>,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayChatLayerRecord {
    pub chat_id: String,
    pub layer_id: String,
    pub priority: i64,
    pub enabled: bool,
    pub created_at: IsoTimestamp,
    pub layer: RoleplayLoreLayerRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreRecallQuery {
    pub chat_id: String,
    pub session_id: Option<SessionId>,
    pub query_text: Option<String>,
    pub active_subjects: Vec<String>,
    pub excluded_subjects: Vec<String>,
    pub token_budget: Option<u32>,
    pub trace_id: Option<String>,
    pub record_trace: bool,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreRecallEntry {
    pub record: RoleplayLoreRecord,
    pub layer_id: String,
    pub score: f32,
    pub token_estimate: u32,
    pub is_constant: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreRecallTraceRecord {
    pub trace_id: String,
    pub session_id: Option<SessionId>,
    pub layer_ids: Vec<String>,
    pub query_text: Option<String>,
    pub active_subjects: Vec<String>,
    pub excluded_subjects: Vec<String>,
    pub config_snapshot: JsonValue,
    pub entries_considered: u32,
    pub entries_returned: u32,
    pub token_budget: Option<u32>,
    pub tokens_consumed: u32,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LoreRecallTraceQuery {
    pub session_id: Option<SessionId>,
    pub chat_id: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreRecallResult {
    pub chat_id: String,
    pub entries: Vec<LoreRecallEntry>,
    pub entries_considered: u32,
    pub tokens_consumed: u32,
    pub token_budget: Option<u32>,
    pub trace: Option<LoreRecallTraceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvScope {
    pub scope_type: String,
    pub scope_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvRecord {
    pub scope: SimpleKvScope,
    pub key: String,
    pub value_json: JsonValue,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvWrite {
    pub scope: SimpleKvScope,
    pub key: String,
    pub value_json: JsonValue,
    pub now: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvCompareAndSwap {
    pub write: SimpleKvWrite,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvDelete {
    pub scope: SimpleKvScope,
    pub key: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvQuery {
    pub scope: SimpleKvScope,
    pub key_prefix: Option<String>,
    pub include_expired: bool,
    pub expired_only: bool,
    pub now: Option<IsoTimestamp>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventRecord {
    pub sequence: u64,
    pub kind: CoreEventKind,
    pub recorded_at: IsoTimestamp,
    pub event: CoreEvent,
    pub session_ids: Vec<SessionId>,
    pub agent_ids: Vec<AgentId>,
    pub instance_ids: Vec<AgentInstanceId>,
    pub correlation_ids: Vec<String>,
    pub source_wake_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeEventFilter {
    pub kind: Option<CoreEventKind>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub correlation_id: Option<String>,
    pub source_wake_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSearchRowType {
    Message,
    QueueMessage,
    Session,
}

impl RuntimeSearchRowType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::QueueMessage => "queue_message",
            Self::Session => "session",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSearchFilter {
    pub query: String,
    pub row_type: Option<RuntimeSearchRowType>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub task_id: Option<TaskId>,
    pub event_kind: Option<CoreEventKind>,
    pub recorded_after: Option<IsoTimestamp>,
    pub recorded_before: Option<IsoTimestamp>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSearchResult {
    pub row_type: RuntimeSearchRowType,
    pub row_key: String,
    pub sequence: Option<u64>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub task_id: Option<TaskId>,
    pub event_kind: Option<CoreEventKind>,
    pub recorded_at: IsoTimestamp,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedMessageState {
    Pending,
    Delivered,
    Expired,
    Discarded,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedMessageRecord {
    pub message_id: String,
    pub owner_session_id: Option<SessionId>,
    pub owner_agent_id: AgentId,
    pub message: AgentMessage,
    pub source_sequence: Option<u64>,
    pub enqueued_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
    pub ttl_ms: u32,
    pub delivery_attempts: u32,
    pub state: QueuedMessageState,
    pub terminal_at: Option<IsoTimestamp>,
    pub state_reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueuedMessageFilter {
    pub state: Option<QueuedMessageState>,
    pub owner_session_id: Option<SessionId>,
    pub owner_agent_id: Option<AgentId>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledJobStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledRunStatus {
    Claimed,
    Completed,
    Skipped,
    Failed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledRunTrigger {
    Due,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledJobRecord {
    pub job_id: String,
    pub job_kind: String,
    pub target_session_id: Option<SessionId>,
    pub interval_ms: Option<u64>,
    pub next_due_at: Option<IsoTimestamp>,
    pub payload_json: JsonValue,
    pub status: ScheduledJobStatus,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub paused_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduledJobQuery {
    pub status: Option<ScheduledJobStatus>,
    pub job_kind: Option<String>,
    pub due_at_or_before: Option<IsoTimestamp>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledRunRecord {
    pub run_id: RunId,
    pub job_id: String,
    pub job_kind: String,
    pub target_session_id: Option<SessionId>,
    pub status: ScheduledRunStatus,
    pub trigger: ScheduledRunTrigger,
    pub scheduled_for: Option<IsoTimestamp>,
    pub claimed_at: IsoTimestamp,
    pub claim_deadline_at: IsoTimestamp,
    pub completed_at: Option<IsoTimestamp>,
    pub error: Option<String>,
    pub output_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduledRunQuery {
    pub job_id: Option<String>,
    pub status: Option<ScheduledRunStatus>,
    pub trigger: Option<ScheduledRunTrigger>,
    pub target_session_id: Option<SessionId>,
    pub stale_claim_deadline_before: Option<IsoTimestamp>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderWireStateInvalidationReason {
    ProfileChanged,
    ProviderChanged,
    ModuleChanged,
    StrategyChanged,
    Expired,
    BrainRequestedClear,
    OperatorRequestedClear,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateKey {
    pub session_id: SessionId,
    pub module_id: String,
    pub strategy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateRecord {
    pub row_id: i64,
    pub key: ProviderWireStateKey,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub payload_version: String,
    pub payload_json: JsonValue,
    pub payload_encoding: String,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub last_wake_id: Option<String>,
    pub invalidated_at: Option<IsoTimestamp>,
    pub invalidation_reason: Option<ProviderWireStateInvalidationReason>,
}

impl ProviderWireStateRecord {
    pub fn is_current(&self) -> bool {
        self.invalidated_at.is_none() && self.invalidation_reason.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateDiagnostic {
    pub key: ProviderWireStateKey,
    pub payload_version: String,
    pub payload_bytes: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub last_wake_id: Option<String>,
    pub invalidated_at: Option<IsoTimestamp>,
    pub invalidation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateWrite {
    pub key: ProviderWireStateKey,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub payload_version: String,
    pub payload_json: JsonValue,
    pub now: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub last_wake_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateWakeLookup {
    pub key: ProviderWireStateKey,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateWakeResult {
    pub record: Option<ProviderWireStateRecord>,
    pub absence_reason: Option<ProviderStateAbsenceReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeMaintenancePolicy {
    pub expire_queued_messages_at: Option<IsoTimestamp>,
    pub purge_terminal_queued_messages_before: Option<IsoTimestamp>,
    pub expire_provider_wire_states_at: Option<IsoTimestamp>,
    pub compact_session_memory_at: Option<IsoTimestamp>,
    pub session_memory_max_active_records_per_scope: Option<u32>,
    pub session_memory_archive_batch_size: Option<u32>,
    pub run_wal_checkpoint: bool,
    pub run_optimize: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionMemoryCompactionReport {
    pub enabled: bool,
    pub scopes_inspected: u64,
    pub retention_pressure_scopes: u64,
    pub scopes_compacted: u64,
    pub session_summaries_created: u64,
    pub branch_summaries_created: u64,
    pub records_archived: u64,
    pub records_superseded: u64,
    pub skipped_scopes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDatabaseSize {
    pub database_bytes: u64,
    pub page_count: u64,
    pub page_size_bytes: u64,
    pub freelist_pages: u64,
    pub freelist_bytes: u64,
    pub wal_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStorageCapability {
    pub name: String,
    pub supported: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStorageTableCount {
    pub table: String,
    pub rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStoragePressureSignal {
    pub name: String,
    pub active: bool,
    pub severity: String,
    pub observed_value: u64,
    pub threshold_value: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStorageConnectionHealth {
    pub backend: String,
    pub status: String,
    pub max_connections: u32,
    pub active_connections: u32,
    pub idle_connections: u32,
    pub total_opened: u64,
    pub checkout_count: u64,
    pub checkout_reuse_count: u64,
    pub reconnect_attempts: u64,
    pub reconnect_successes: u64,
    pub closed_connections_discarded: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStorageDiagnostics {
    pub backend: String,
    pub backend_label: String,
    pub schema_version: i64,
    pub supported_schema_version: i64,
    pub migrations: Vec<SchemaMigrationRecord>,
    pub size: RuntimeDatabaseSize,
    pub table_counts: Vec<RuntimeStorageTableCount>,
    pub capabilities: Vec<RuntimeStorageCapability>,
    pub repository_groups: Vec<RuntimeRepositoryGroupDiagnostic>,
    pub connection_health: RuntimeStorageConnectionHealth,
    pub module_registry: RuntimeModuleSchemaRegistryDiagnostics,
    pub index_checks: Vec<RuntimeQueryPlanCheck>,
    pub search_healthy: bool,
    pub pressure_signals: Vec<RuntimeStoragePressureSignal>,
    pub pressure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaintenanceReport {
    pub size_before: RuntimeDatabaseSize,
    pub size_after: RuntimeDatabaseSize,
    pub expired_queue_messages: u64,
    pub purged_terminal_queue_messages: u64,
    pub expired_provider_wire_states: u64,
    pub session_memory_compaction: SessionMemoryCompactionReport,
    pub wal_checkpoint_ran: bool,
    pub optimize_ran: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeQueryPlanCheck {
    pub name: &'static str,
    pub uses_index: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeImportBatchRecord {
    pub import_batch_id: String,
    pub source_system: String,
    pub source_label: String,
    pub source_snapshot_ref: Option<String>,
    pub notes: Option<String>,
    pub imported_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeObjectKind {
    Agent,
    AgentInstance,
    Session,
    Profile,
    WorkerRun,
    Message,
    CompletionPacket,
    ToolCall,
    QueueMessage,
    ExternalArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
pub struct RuntimeImportProvenance {
    pub profile_id: Option<ProfileId>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub externally_owned: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyIdMappingRecord {
    pub import_batch_id: String,
    pub source: SourceSystemReference,
    pub legacy_kind: RuntimeObjectKind,
    pub rusty_kind: RuntimeObjectKind,
    pub rusty_id: String,
    pub provenance: RuntimeImportProvenance,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LegacyIdMappingQuery {
    pub import_batch_id: Option<String>,
    pub source_system: Option<String>,
    pub legacy_kind: Option<RuntimeObjectKind>,
    pub rusty_kind: Option<RuntimeObjectKind>,
    pub rusty_id: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageExportBundle {
    pub bundle_version: u32,
    pub export_id: String,
    pub exported_at: IsoTimestamp,
    pub service_version: Option<String>,
    pub source: LogicalStorageExportSource,
    pub schema_version: i64,
    pub module_versions: Vec<LogicalStorageModuleVersion>,
    pub capability_snapshot: Vec<LogicalStorageCapabilitySnapshot>,
    pub repositories: Vec<LogicalStorageRepositoryBundle>,
    pub legacy_id_mappings: Vec<LogicalStorageLegacyIdMapping>,
    pub profile_asset_refs: Vec<LogicalStorageProfileAssetRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageExportSource {
    pub backend: String,
    pub backend_label: String,
    pub source_instance_id: Option<String>,
    pub snapshot_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageModuleVersion {
    pub module_id: String,
    pub schema_version: u32,
    pub descriptor_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageCapabilitySnapshot {
    pub name: String,
    pub supported: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageRepositoryBundle {
    pub repository_id: String,
    pub schema_version: u32,
    pub required_capabilities: Vec<String>,
    pub exported_count: u64,
    pub checksum: Option<String>,
    pub records: Vec<LogicalStorageRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageRecord {
    pub stable_id: String,
    pub record_version: u32,
    pub exported_at: IsoTimestamp,
    pub payload: LogicalStorageRecordPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum LogicalStorageRecordPayload {
    QueueMessage(Box<LogicalQueuedMessageExportRecord>),
    TypedJson {
        object_kind: String,
        payload_json: JsonValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalQueuedMessageExportRecord {
    pub message_id: String,
    pub owner_session_id: Option<SessionId>,
    pub owner_agent_id: AgentId,
    pub message: AgentMessage,
    pub source_sequence: Option<u64>,
    pub enqueued_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
    pub ttl_ms: u32,
    pub delivery_attempts: u32,
    pub state: QueuedMessageState,
    pub terminal_at: Option<IsoTimestamp>,
    pub state_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageLegacyIdMapping {
    pub source_system: String,
    pub legacy_kind: RuntimeObjectKind,
    pub legacy_id: String,
    pub rusty_kind: RuntimeObjectKind,
    pub rusty_id: String,
    pub provenance: RuntimeImportProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStorageProfileAssetRef {
    pub profile_id: ProfileId,
    pub asset_kind: String,
    pub asset_ref: String,
    pub checksum: Option<String>,
    pub bundled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalStorageImportDryRun {
    pub import_batch_id: String,
    pub target_backend: String,
    pub validation_time: IsoTimestamp,
    pub supported_capabilities: Vec<String>,
    pub supported_repositories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalStorageImportValidationReport {
    pub import_batch_id: String,
    pub dry_run: bool,
    pub source_backend: String,
    pub target_backend: String,
    pub repository_count: u64,
    pub record_count: u64,
    pub accepted_records: u64,
    pub unsupported_records: u64,
    pub refused_records: u64,
    pub already_imported: bool,
    pub issues: Vec<LogicalStorageImportIssue>,
}

impl LogicalStorageImportValidationReport {
    pub fn can_apply(&self) -> bool {
        self.dry_run
            && !self.already_imported
            && self.unsupported_records == 0
            && self.refused_records == 0
            && self
                .issues
                .iter()
                .all(|issue| issue.severity != LogicalStorageImportIssueSeverity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalStorageImportIssue {
    pub severity: LogicalStorageImportIssueSeverity,
    pub code: String,
    pub repository_id: Option<String>,
    pub record_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalStorageImportIssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalBindingStatus {
    Active,
    Degraded,
    Disconnected,
    Archived,
}

impl ExternalBindingStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Degraded => "degraded",
            Self::Disconnected => "disconnected",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExternalBindingProvenance {
    pub source_system: Option<String>,
    pub source_ref: Option<String>,
    pub externally_owned: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelBindingRecord {
    pub binding_id: String,
    pub adapter_id: AdapterId,
    pub provider: String,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: ProfileId,
    pub external_channel_id: String,
    pub external_thread_id: Option<String>,
    pub external_user_id: Option<String>,
    pub provider_subscription_id: Option<String>,
    pub cursor: Option<String>,
    pub membership_state: Option<String>,
    pub presence_state: Option<String>,
    pub status: ExternalBindingStatus,
    pub degraded_reason: Option<String>,
    pub provenance: ExternalBindingProvenance,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChannelBindingQuery {
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: Option<ProfileId>,
    pub adapter_id: Option<AdapterId>,
    pub provider: Option<String>,
    pub external_channel_id: Option<String>,
    pub status: Option<ExternalBindingStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct McpBindingDiagnostics {
    pub last_error: Option<String>,
    pub last_checked_at: Option<IsoTimestamp>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBindingRecord {
    pub binding_id: String,
    pub adapter_id: AdapterId,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: ProfileId,
    pub server_names: Vec<String>,
    pub endpoint_ref: String,
    pub transport: String,
    pub tool_profile_key: String,
    pub discovered_tool_revision: Option<String>,
    pub status: ExternalBindingStatus,
    pub degraded_reason: Option<String>,
    pub diagnostics: McpBindingDiagnostics,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpBindingQuery {
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: Option<ProfileId>,
    pub adapter_id: Option<AdapterId>,
    pub status: Option<ExternalBindingStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCounterScope {
    Runtime,
    Agent(AgentId),
    Instance(AgentInstanceId),
    Session(SessionId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCounterRecord {
    pub scope: RuntimeCounterScope,
    pub counter_name: String,
    pub value: u64,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateSummary {
    pub scope: RuntimeCounterScope,
    pub brain_turns: u64,
    pub wakes: u64,
    pub tool_calls: u64,
    pub tool_errors: u64,
    pub delegations_created: u64,
    pub delegations_completed: u64,
    pub delegations_failed: u64,
    pub delegations_timed_out: u64,
    pub delegations_cancelled: u64,
    pub messages: u64,
    pub completions: u64,
    pub queue_expirations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallRecord {
    pub sequence: u64,
    pub session_id: SessionId,
    pub wake_id: Option<String>,
    pub tool_name: String,
    pub phase: ToolCallPhase,
    pub is_error: Option<bool>,
    pub metadata: Option<ToolCallMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallPhase {
    Started,
    Finished,
}

impl ToolCallPhase {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticTable {
    Agents,
    AgentInstances,
    Sessions,
    SessionConfigs,
    SessionIdentity,
    EventHistory,
    EventAgentIndex,
    EventCorrelationIndex,
    EventInstanceIndex,
    EventSessionIndex,
    EventWakeIndex,
    RuntimeCounters,
    RuntimeSearch,
    QueuedMessages,
    RuntimeImportBatches,
    LegacyIdMappings,
    ProfileRegistry,
    ModelProviders,
    ProfileMemories,
    SessionMemoryRecords,
    SessionActivityDigests,
    ContextCompactionArtifacts,
    RoleplayLoreRecords,
    RoleplayLoreProvenanceEvents,
    RoleplayLoreLayers,
    RoleplayLoreLayerEntries,
    RoleplayChatLayers,
    RoleplayLoreRecallTraces,
    RoleplayLoreLayerConfig,
    MemoryProposals,
    MemoryGovernanceDecisions,
    ScheduledJobs,
    ScheduledJobRuns,
    ProviderWireStates,
    MessageSlots,
    MessageVariants,
    Messages,
    MessageBlocks,
    ConversationBranches,
    ConversationBranchState,
    ConversationSnapshots,
    Attachments,
    AttachmentLinks,
    DataBankScopes,
    ChannelBindings,
    McpBindings,
    AgentMessages,
    CompletionPackets,
    WorkerRuns,
    WorkerPoolMembers,
    WorkerPoolWorkItems,
    WorkerPoolLeases,
    WorkerPoolEvents,
    ToolCallHistory,
}

impl DiagnosticTable {
    pub const ALL: &'static [Self] = &[
        Self::Agents,
        Self::AgentInstances,
        Self::Sessions,
        Self::SessionConfigs,
        Self::SessionIdentity,
        Self::EventHistory,
        Self::EventAgentIndex,
        Self::EventCorrelationIndex,
        Self::EventInstanceIndex,
        Self::EventSessionIndex,
        Self::EventWakeIndex,
        Self::RuntimeCounters,
        Self::RuntimeSearch,
        Self::QueuedMessages,
        Self::RuntimeImportBatches,
        Self::LegacyIdMappings,
        Self::ProfileRegistry,
        Self::ModelProviders,
        Self::ProfileMemories,
        Self::SessionMemoryRecords,
        Self::SessionActivityDigests,
        Self::ContextCompactionArtifacts,
        Self::RoleplayLoreRecords,
        Self::RoleplayLoreProvenanceEvents,
        Self::RoleplayLoreLayers,
        Self::RoleplayLoreLayerEntries,
        Self::RoleplayChatLayers,
        Self::RoleplayLoreRecallTraces,
        Self::RoleplayLoreLayerConfig,
        Self::MemoryProposals,
        Self::MemoryGovernanceDecisions,
        Self::ScheduledJobs,
        Self::ScheduledJobRuns,
        Self::ProviderWireStates,
        Self::MessageSlots,
        Self::MessageVariants,
        Self::Messages,
        Self::MessageBlocks,
        Self::ConversationBranches,
        Self::ConversationBranchState,
        Self::ConversationSnapshots,
        Self::Attachments,
        Self::AttachmentLinks,
        Self::DataBankScopes,
        Self::ChannelBindings,
        Self::McpBindings,
        Self::AgentMessages,
        Self::CompletionPackets,
        Self::WorkerRuns,
        Self::WorkerPoolMembers,
        Self::WorkerPoolWorkItems,
        Self::WorkerPoolLeases,
        Self::WorkerPoolEvents,
        Self::ToolCallHistory,
    ];

    pub(crate) fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "agents" => Ok(Self::Agents),
            "agent_instances" => Ok(Self::AgentInstances),
            "sessions" => Ok(Self::Sessions),
            "session_configs" => Ok(Self::SessionConfigs),
            "session_identity" => Ok(Self::SessionIdentity),
            "event_history" => Ok(Self::EventHistory),
            "event_agent_index" => Ok(Self::EventAgentIndex),
            "event_correlation_index" => Ok(Self::EventCorrelationIndex),
            "event_instance_index" => Ok(Self::EventInstanceIndex),
            "event_session_index" => Ok(Self::EventSessionIndex),
            "event_wake_index" => Ok(Self::EventWakeIndex),
            "runtime_counters" => Ok(Self::RuntimeCounters),
            "runtime_search_fts" => Ok(Self::RuntimeSearch),
            "queued_messages" => Ok(Self::QueuedMessages),
            "runtime_import_batches" => Ok(Self::RuntimeImportBatches),
            "legacy_id_mappings" => Ok(Self::LegacyIdMappings),
            "profile_registry" => Ok(Self::ProfileRegistry),
            "model_providers" => Ok(Self::ModelProviders),
            "profile_memories" => Ok(Self::ProfileMemories),
            "session_memory_records" => Ok(Self::SessionMemoryRecords),
            "session_activity_digests" => Ok(Self::SessionActivityDigests),
            "context_compaction_artifacts" => Ok(Self::ContextCompactionArtifacts),
            "module_roleplay_lore_records" => Ok(Self::RoleplayLoreRecords),
            "module_roleplay_lore_provenance_events" => Ok(Self::RoleplayLoreProvenanceEvents),
            "module_roleplay_lore_layers" => Ok(Self::RoleplayLoreLayers),
            "module_roleplay_lore_layer_entries" => Ok(Self::RoleplayLoreLayerEntries),
            "module_roleplay_chat_layers" => Ok(Self::RoleplayChatLayers),
            "module_roleplay_lore_recall_traces" => Ok(Self::RoleplayLoreRecallTraces),
            "module_roleplay_lore_layer_config" => Ok(Self::RoleplayLoreLayerConfig),
            "memory_proposals" => Ok(Self::MemoryProposals),
            "memory_governance_decisions" => Ok(Self::MemoryGovernanceDecisions),
            "scheduled_jobs" => Ok(Self::ScheduledJobs),
            "scheduled_job_runs" => Ok(Self::ScheduledJobRuns),
            "provider_wire_states" => Ok(Self::ProviderWireStates),
            "message_slots" => Ok(Self::MessageSlots),
            "message_variants" => Ok(Self::MessageVariants),
            "messages" => Ok(Self::Messages),
            "message_blocks" => Ok(Self::MessageBlocks),
            "conversation_branches" => Ok(Self::ConversationBranches),
            "conversation_branch_state" => Ok(Self::ConversationBranchState),
            "conversation_snapshots" => Ok(Self::ConversationSnapshots),
            "attachments" => Ok(Self::Attachments),
            "attachment_links" => Ok(Self::AttachmentLinks),
            "data_bank_scopes" => Ok(Self::DataBankScopes),
            "channel_bindings" => Ok(Self::ChannelBindings),
            "mcp_bindings" => Ok(Self::McpBindings),
            "agent_messages" => Ok(Self::AgentMessages),
            "completion_packets" => Ok(Self::CompletionPackets),
            "worker_runs" => Ok(Self::WorkerRuns),
            "worker_pool_members" => Ok(Self::WorkerPoolMembers),
            "worker_pool_work_items" => Ok(Self::WorkerPoolWorkItems),
            "worker_pool_leases" => Ok(Self::WorkerPoolLeases),
            "worker_pool_events" => Ok(Self::WorkerPoolEvents),
            "tool_call_history" => Ok(Self::ToolCallHistory),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported persistence table {raw}"),
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::AgentInstances => "agent_instances",
            Self::Sessions => "sessions",
            Self::SessionConfigs => "session_configs",
            Self::SessionIdentity => "session_identity",
            Self::EventHistory => "event_history",
            Self::EventAgentIndex => "event_agent_index",
            Self::EventCorrelationIndex => "event_correlation_index",
            Self::EventInstanceIndex => "event_instance_index",
            Self::EventSessionIndex => "event_session_index",
            Self::EventWakeIndex => "event_wake_index",
            Self::RuntimeCounters => "runtime_counters",
            Self::RuntimeSearch => "runtime_search_fts",
            Self::QueuedMessages => "queued_messages",
            Self::RuntimeImportBatches => "runtime_import_batches",
            Self::LegacyIdMappings => "legacy_id_mappings",
            Self::ProfileRegistry => "profile_registry",
            Self::ModelProviders => "model_providers",
            Self::ProfileMemories => "profile_memories",
            Self::SessionMemoryRecords => "session_memory_records",
            Self::SessionActivityDigests => "session_activity_digests",
            Self::ContextCompactionArtifacts => "context_compaction_artifacts",
            Self::RoleplayLoreRecords => "module_roleplay_lore_records",
            Self::RoleplayLoreProvenanceEvents => "module_roleplay_lore_provenance_events",
            Self::RoleplayLoreLayers => "module_roleplay_lore_layers",
            Self::RoleplayLoreLayerEntries => "module_roleplay_lore_layer_entries",
            Self::RoleplayChatLayers => "module_roleplay_chat_layers",
            Self::RoleplayLoreRecallTraces => "module_roleplay_lore_recall_traces",
            Self::RoleplayLoreLayerConfig => "module_roleplay_lore_layer_config",
            Self::MemoryProposals => "memory_proposals",
            Self::MemoryGovernanceDecisions => "memory_governance_decisions",
            Self::ScheduledJobs => "scheduled_jobs",
            Self::ScheduledJobRuns => "scheduled_job_runs",
            Self::ProviderWireStates => "provider_wire_states",
            Self::MessageSlots => "message_slots",
            Self::MessageVariants => "message_variants",
            Self::Messages => "messages",
            Self::MessageBlocks => "message_blocks",
            Self::ConversationBranches => "conversation_branches",
            Self::ConversationBranchState => "conversation_branch_state",
            Self::ConversationSnapshots => "conversation_snapshots",
            Self::Attachments => "attachments",
            Self::AttachmentLinks => "attachment_links",
            Self::DataBankScopes => "data_bank_scopes",
            Self::ChannelBindings => "channel_bindings",
            Self::McpBindings => "mcp_bindings",
            Self::AgentMessages => "agent_messages",
            Self::CompletionPackets => "completion_packets",
            Self::WorkerRuns => "worker_runs",
            Self::WorkerPoolMembers => "worker_pool_members",
            Self::WorkerPoolWorkItems => "worker_pool_work_items",
            Self::WorkerPoolLeases => "worker_pool_leases",
            Self::WorkerPoolEvents => "worker_pool_events",
            Self::ToolCallHistory => "tool_call_history",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerRunStatus {
    Requested,
    SessionCreated,
    WakeRequested,
    Running,
    CheckpointWaiting,
    Completed,
    Failed,
    Blocked,
    Exhausted,
    Cancelled,
    Expired,
}

impl WorkerRunStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::SessionCreated => "session_created",
            Self::WakeRequested => "wake_requested",
            Self::Running => "running",
            Self::CheckpointWaiting => "checkpoint_waiting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Blocked
                | Self::Exhausted
                | Self::Cancelled
                | Self::Expired
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRunRecord {
    pub run_id: RunId,
    pub parent_session_id: SessionId,
    pub delegated_session_id: Option<SessionId>,
    pub parent_agent_id: Option<AgentId>,
    pub profile_id: ProfileId,
    pub task_id: Option<TaskId>,
    pub status: WorkerRunStatus,
    pub created_at: IsoTimestamp,
    pub last_updated_at: IsoTimestamp,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub delegation_correlation_id: Option<String>,
    pub parent_consumption: ParentConsumptionPolicy,
    pub fan_out_group_id: Option<String>,
    pub fan_out_max_concurrency: Option<u32>,
    pub fan_out_failure_policy: FanOutFailurePolicy,
    pub worker_pool_work_item_id: Option<String>,
    pub worker_pool_lease_id: Option<String>,
    pub worker_pool_member_id: Option<String>,
    pub worker_pool_claim_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerPoolMemberStatus {
    Available,
    Busy,
    Offline,
    Quarantined,
    Retired,
}

impl WorkerPoolMemberStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Busy => "busy",
            Self::Offline => "offline",
            Self::Quarantined => "quarantined",
            Self::Retired => "retired",
        }
    }

    pub const fn can_claim(&self) -> bool {
        matches!(self, Self::Available | Self::Busy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerPoolWorkStatus {
    Pending,
    Claimed,
    Running,
    Completed,
    Failed,
    Blocked,
    Exhausted,
    Cancelled,
    Expired,
}

impl WorkerPoolWorkStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Blocked
                | Self::Exhausted
                | Self::Cancelled
                | Self::Expired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerPoolLeaseStatus {
    Active,
    Completed,
    Failed,
    Blocked,
    Exhausted,
    Cancelled,
    Expired,
    Released,
}

impl WorkerPoolLeaseStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Released => "released",
        }
    }

    pub const fn is_terminal(&self) -> bool {
        !matches!(self, Self::Active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerPoolNoCapacityReason {
    NoPendingWork,
    MemberUnavailable,
    MemberHeartbeatStale,
    MemberAtCapacity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerPoolMemberRecord {
    pub member_id: String,
    pub profile_id: ProfileId,
    pub agent_id: Option<AgentId>,
    pub session_id: Option<SessionId>,
    pub status: WorkerPoolMemberStatus,
    pub concurrency_limit: u32,
    pub active_leases: u32,
    pub capabilities_json: JsonValue,
    pub registered_at: IsoTimestamp,
    pub last_heartbeat_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerPoolWorkItemRecord {
    pub work_item_id: String,
    pub requested_profile_id: Option<ProfileId>,
    pub task_id: Option<TaskId>,
    pub status: WorkerPoolWorkStatus,
    pub priority: i32,
    pub work_json: JsonValue,
    pub required_capabilities_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub claimed_by_member_id: Option<String>,
    pub lease_id: Option<String>,
    pub claim_token: Option<String>,
    pub claim_deadline_at: Option<IsoTimestamp>,
    pub terminal_at: Option<IsoTimestamp>,
    pub terminal_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerPoolLeaseRecord {
    pub lease_id: String,
    pub work_item_id: String,
    pub member_id: String,
    pub claim_token: String,
    pub status: WorkerPoolLeaseStatus,
    pub claimed_at: IsoTimestamp,
    pub claim_deadline_at: IsoTimestamp,
    pub terminal_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerPoolClaimRecord {
    pub member: WorkerPoolMemberRecord,
    pub work_item: WorkerPoolWorkItemRecord,
    pub lease: WorkerPoolLeaseRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerPoolClaimRequest {
    pub member_id: String,
    pub lease_id: String,
    pub claim_token: String,
    pub now: IsoTimestamp,
    pub claim_deadline_at: IsoTimestamp,
    pub min_heartbeat_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerPoolCompletionRequest {
    pub lease_id: String,
    pub claim_token: String,
    pub status: WorkerPoolWorkStatus,
    pub now: IsoTimestamp,
    pub summary: Option<String>,
}
