use crate::{AdapterId, AgentId, AgentInstanceId, IsoTimestamp, SessionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const TELEGRAM_INSTALL_DIPLOMAT_BINDING_VERSION: &str = "telegram_install_diplomat.v1";
pub const TELEGRAM_DIPLOMAT_INTERACTION_VERSION: &str = "telegram_diplomat_interaction.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallDiplomatParticipationMode {
    MentionOrReply,
    TopicHumanMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallDiplomatBindingStatus {
    Active,
    Paused,
    NeedsRebind,
    Removed,
}

impl InstallDiplomatBindingStatus {
    pub fn is_routable(self) -> bool {
        self == Self::Active
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiplomatBindingRecord {
    pub schema_version: String,
    pub binding_id: String,
    pub revision: u64,
    pub installation_id: String,
    pub installation_label: String,
    pub adapter_id: AdapterId,
    pub bot_user_id: String,
    pub bot_username: String,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: SessionId,
    pub external_chat_id: String,
    pub external_thread_id: Option<String>,
    pub participation_mode: InstallDiplomatParticipationMode,
    pub status: InstallDiplomatBindingStatus,
    pub degraded_reason: Option<String>,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiplomatBindingWrite {
    pub binding_id: String,
    pub expected_revision: Option<u64>,
    pub installation_id: String,
    pub installation_label: String,
    pub adapter_id: AdapterId,
    pub bot_user_id: String,
    pub bot_username: String,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: SessionId,
    pub external_chat_id: String,
    pub external_thread_id: Option<String>,
    pub participation_mode: InstallDiplomatParticipationMode,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiplomatRebindRequest {
    pub binding_id: String,
    pub expected_revision: u64,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: SessionId,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiplomatBindingStatusUpdate {
    pub binding_id: String,
    pub expected_revision: u64,
    pub status: InstallDiplomatBindingStatus,
    pub degraded_reason: Option<String>,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiplomatBindingQuery {
    pub binding_id: Option<String>,
    pub installation_id: Option<String>,
    pub adapter_id: Option<AdapterId>,
    pub session_id: Option<SessionId>,
    pub external_chat_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub status: Option<InstallDiplomatBindingStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TelegramDiplomatSenderKind {
    Human,
    Bot,
    SenderChat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TelegramDiplomatSender {
    pub kind: TelegramDiplomatSenderKind,
    pub external_user_id: String,
    pub username: Option<String>,
    pub display_label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TelegramDiplomatInteractionTerminalReason {
    DepthExceeded,
    MessageBudgetExceeded,
    InteractionExpired,
    BotPairRateLimited,
    BindingUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TelegramDiplomatInteractionRecord {
    pub schema_version: String,
    pub interaction_id: String,
    pub binding_id: String,
    pub revision: u64,
    pub root_external_message_id: String,
    pub last_external_message_id: String,
    pub last_sender: TelegramDiplomatSender,
    pub bot_pair_key: Option<String>,
    pub bot_depth: u32,
    pub bot_message_count: u32,
    pub bot_message_timestamps: Vec<IsoTimestamp>,
    pub crew_correlation_id: String,
    pub deadline_at: IsoTimestamp,
    pub terminal_reason: Option<TelegramDiplomatInteractionTerminalReason>,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TelegramDiplomatIngressRequest {
    pub binding_id: String,
    pub interaction_id: String,
    pub external_message_id: String,
    pub reply_to_external_message_id: Option<String>,
    pub sender: TelegramDiplomatSender,
    pub addressed_to_bot: bool,
    pub correlated_interaction: bool,
    pub receiving_bot_user_id: String,
    pub received_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TelegramDiplomatIngressDecision {
    Routed,
    Ignored,
    BindingUnavailable,
    LoopTerminated,
    RateLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TelegramDiplomatIngressPlan {
    pub decision: TelegramDiplomatIngressDecision,
    pub reason_code: String,
    pub binding: InstallDiplomatBindingRecord,
    pub interaction: Option<TelegramDiplomatInteractionRecord>,
    pub target_session_id: Option<SessionId>,
    pub sender: TelegramDiplomatSender,
    pub reply_to_external_message_id: Option<String>,
    pub crew_correlation_id: Option<String>,
}
