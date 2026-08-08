use super::*;

impl NativeBridge {
    pub fn create_session(
        &self,
        config: rusty_crew_core_bridge_api::SessionConfig,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?.create_session(config)
    }

    pub fn ensure_configured_session(
        &self,
        config: rusty_crew_core_bridge_api::SessionConfig,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?.ensure_configured_session(config)
    }

    pub fn archive_session(
        &self,
        session_id: SessionId,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?.archive_session(&session_id)
    }

    pub fn list_sessions(&self) -> CoreResult<Vec<rusty_crew_core_bridge_api::SessionState>> {
        self.engine()?.list_sessions()
    }

    pub fn update_session_workspace(
        &self,
        update: rusty_crew_core_bridge_api::SessionWorkspaceUpdate,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionWorkspaceUpdateRecord> {
        self.engine()?.update_session_workspace(&update)
    }

    pub fn set_session_reasoning_effort(
        &self,
        session_id: SessionId,
        reasoning_effort: Option<String>,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?
            .set_session_reasoning_effort(&session_id, reasoning_effort)
    }

    pub fn project_body_state_json(
        &self,
        session_id: rusty_crew_core_bridge_api::SessionId,
    ) -> CoreResult<Vec<u8>> {
        let state = self.engine()?.project_body_state(&session_id)?;
        serde_json::to_vec(&state).map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("serialize body state: {error}"),
            )
        })
    }
}

pub(crate) fn to_js_session_state(
    state: rusty_crew_core_bridge_api::SessionState,
) -> JsSessionState {
    JsSessionState {
        handle: state.handle.get() as f64,
        session_id: state.session_id.0,
        agent_id: state.agent_id.0,
        profile_id: state.profile_id.0,
        kind: format!("{:?}", state.kind).to_ascii_lowercase(),
        status: format!("{:?}", state.status).to_ascii_lowercase(),
        workspace: state.workspace.map(|workspace| JsSessionWorkspace {
            cwd: workspace.cwd,
            revision: workspace.revision as f64,
            updated_at: workspace.updated_at,
        }),
        resource_limits: JsResourceLimits {
            max_duration_ms: state.resource_limits.max_duration_ms,
            max_delegation_depth: state.resource_limits.max_delegation_depth,
        },
        tool_profile: JsToolProfile {
            tools: state
                .tool_profile
                .tools
                .into_iter()
                .map(|tool| JsToolDescriptor {
                    name: tool.name,
                    description: tool.description,
                    input_schema: tool.input_schema.map(|handle| handle.get() as u32),
                })
                .collect(),
        },
        history_window: state.history_window.map(|window| JsSessionHistoryWindow {
            max_messages: window.max_messages,
        }),
        reasoning_effort: state.inference_overrides.reasoning_effort,
    }
}

pub(crate) fn parse_session_kind(
    raw: &str,
) -> napi::Result<rusty_crew_core_bridge_api::SessionKind> {
    match raw {
        "full" => Ok(rusty_crew_core_bridge_api::SessionKind::Full),
        "worker" => Ok(rusty_crew_core_bridge_api::SessionKind::Worker),
        "delegated" => Ok(rusty_crew_core_bridge_api::SessionKind::Delegated),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported session kind {other}"),
        )),
    }
}

pub(crate) fn js_session_config(
    config: JsSessionConfig,
) -> napi::Result<rusty_crew_core_bridge_api::SessionConfig> {
    let resource_limits = config.resource_limits;
    let tool_profile = config.tool_profile;
    let history_window = config.history_window;
    Ok(rusty_crew_core_bridge_api::SessionConfig {
        session_id: rusty_crew_core_bridge_api::SessionId::new(config.session_id),
        agent_id: rusty_crew_core_bridge_api::AgentId::new(config.agent_id),
        profile_id: rusty_crew_core_bridge_api::ProfileId::new(config.profile_id),
        kind: parse_session_kind(&config.kind)?,
        delegation: None,
        workspace: config
            .workspace
            .map(|workspace| rusty_crew_core_bridge_api::SessionWorkspace {
                cwd: workspace.cwd,
                revision: workspace.revision as u64,
                updated_at: workspace.updated_at,
            }),
        resource_limits: match resource_limits {
            Some(limits) => rusty_crew_core_bridge_api::ResourceLimits {
                max_duration_ms: limits.max_duration_ms,
                max_delegation_depth: limits.max_delegation_depth,
            },
            None => rusty_crew_core_bridge_api::ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: None,
            },
        },
        tool_profile: match tool_profile {
            Some(profile) => rusty_crew_core_bridge_api::ToolProfile {
                tools: profile
                    .tools
                    .into_iter()
                    .map(|tool| rusty_crew_core_bridge_api::ToolDescriptor {
                        name: tool.name,
                        description: tool.description,
                        input_schema: tool
                            .input_schema
                            .map(|handle| RuntimeBufferHandle::new(handle as u64)),
                    })
                    .collect(),
            },
            None => rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
        },
        history_window: history_window.map(|window| {
            rusty_crew_core_bridge_api::SessionHistoryWindow {
                max_messages: window.max_messages,
            }
        }),
    })
}
