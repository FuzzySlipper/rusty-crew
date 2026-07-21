//! Exact switchboard target validation at the activation boundary.

use super::*;
use rusty_crew_core_protocol::{
    AgentDirectoryRuntimeKind, AgentRouteResolvedTarget, ExternalAgentBinding,
    ExternalBindingPurpose, ExternalBindingStatus,
};

impl CoreEngine {
    pub(crate) fn resolve_agent_route_activation_target(
        &self,
        request_agent_id: &AgentId,
        target: &AgentRouteResolvedTarget,
    ) -> CoreResult<Option<(SessionState, Option<ExternalAgentBinding>)>> {
        let session = match self.sessions.get_session(&target.session_id) {
            Ok(session)
                if session.agent_id == *request_agent_id
                    && session.agent_id == target.agent_id
                    && session.profile_id == target.profile_id
                    && session.status != SessionStatus::Archived =>
            {
                session
            }
            Ok(_) => return Ok(None),
            Err(error) if error.kind == CoreErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        match target.runtime_kind {
            AgentDirectoryRuntimeKind::DirectBrain => {
                if target.runtime_id.is_some()
                    || target.binding_id.is_some()
                    || target.binding_revision.is_some()
                    || target.delivery_policy.is_some()
                    || self
                        .store
                        .list_external_agent_bindings()?
                        .iter()
                        .any(|binding| {
                            binding.purpose == ExternalBindingPurpose::CrewAgent
                                && binding.session_id.as_ref() == Some(&target.session_id)
                        })
                {
                    return Ok(None);
                }
                Ok(Some((session, None)))
            }
            AgentDirectoryRuntimeKind::CodexAppServer => {
                let (Some(runtime_id), Some(binding_id), Some(binding_revision)) = (
                    target.runtime_id.as_ref(),
                    target.binding_id.as_ref(),
                    target.binding_revision,
                ) else {
                    return Ok(None);
                };
                let Some(binding) = self.store.get_external_agent_binding(binding_id)? else {
                    return Ok(None);
                };
                if binding.revision != binding_revision
                    || binding.runtime_id != *runtime_id
                    || binding.agent_id.as_ref() != Some(&target.agent_id)
                    || binding.session_id.as_ref() != Some(&target.session_id)
                    || binding.purpose != ExternalBindingPurpose::CrewAgent
                    || binding.status != ExternalBindingStatus::Active
                    || !binding.is_routable()
                    || target.delivery_policy != Some(binding.message_delivery_policy)
                {
                    return Ok(None);
                }
                Ok(Some((session, Some(binding))))
            }
        }
    }
}
