use crate::{IsoTimestamp, ProfileId, SessionId, SessionState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CrewAgentSessionCreationRequest {
    pub idempotency_key: String,
    pub profile_id: ProfileId,
    pub expected_profile_revision: u64,
    pub requested_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrewAgentSessionCreationOutcome {
    Created,
    Replayed,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CrewAgentSessionCreationRecord {
    pub request_fingerprint: String,
    pub profile_revision: u64,
    pub template_session_id: Option<SessionId>,
    pub outcome: CrewAgentSessionCreationOutcome,
    pub session: SessionState,
}
