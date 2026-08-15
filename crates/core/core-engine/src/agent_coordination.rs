//! Runtime-neutral direct-agent messaging and durable correlated rounds.

use super::*;
use crate::agent_message_format::agent_message_model_text;
use rusty_crew_core_protocol::{
    parse_agent_route_address, AgentActivation, AgentCoordinationCaller, AgentCorrelatedRound,
    AgentDirectoryEntry, AgentDirectoryRuntimeKind, AgentMessageCommand,
    AgentMessageDeliveryReceipt, AgentMessageDeliveryRequest, AgentMessageDeliveryStatus,
    AgentMessageInboxItem, AgentMessageInboxQuery, AgentMessageInboxStatus, AgentMessageInputKind,
    AgentMessageReplyCommand, AgentRoundCommand, AgentRoundStartReceipt, AgentRoundStatus,
    AgentRouteDelete, AgentRouteDeliveryProvenance, AgentRouteKey, AgentRouteLastDelivery,
    AgentRouteRecord, AgentRouteResolution, AgentRouteResolvedTarget, AgentRouteTarget,
    AgentRouteWrite, ExternalAgentBinding, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalMessageDeliveryPolicy, ExternalRuntimeDesiredState, ExternalRuntimeKind,
    ExternalRuntimeObservedState, ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId,
    TurnInputProvenance, TurnInputProvenanceKind,
};
use serde_json::json;

impl CoreEngine {
    pub fn put_agent_route(&self, write: AgentRouteWrite) -> CoreResult<AgentRouteRecord> {
        let _lifecycle_guard = self.agent_route_lifecycle_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "agent route lifecycle lock poisoned",
            )
        })?;
        let address = format!("@{}", write.route_key.0);
        if self
            .sessions
            .all_sessions()?
            .iter()
            .any(|session| session.agent_id.0 == address)
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "agent_route_address_collides_with_raw_agent_id",
            ));
        }
        self.store.put_agent_route(&write)
    }

    pub fn delete_agent_route(&self, delete: AgentRouteDelete) -> CoreResult<AgentRouteRecord> {
        let _lifecycle_guard = self.agent_route_lifecycle_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "agent route lifecycle lock poisoned",
            )
        })?;
        self.store.delete_agent_route(&delete)
    }

    pub(crate) fn validate_agent_id_route_reservation(&self, agent_id: &AgentId) -> CoreResult<()> {
        let Some(route_key) = parse_agent_route_address(&agent_id.0)? else {
            return Ok(());
        };
        if self.store.get_agent_route(&route_key)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "agent_route_address_collides_with_raw_agent_id",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_agent_route_session_collisions(&self) -> CoreResult<()> {
        for session in self.sessions.all_sessions()? {
            self.validate_agent_id_route_reservation(&session.agent_id)?;
        }
        Ok(())
    }

    pub fn get_agent_route_resolution(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentRouteResolution>> {
        self.store
            .get_agent_route(route_key)?
            .map(|route| self.resolve_agent_route_record(route))
            .transpose()
    }

    pub fn list_agent_route_resolutions(&self) -> CoreResult<Vec<AgentRouteResolution>> {
        self.store
            .list_agent_routes()?
            .into_iter()
            .map(|route| self.resolve_agent_route_record(route))
            .collect()
    }

    pub fn resolve_agent_address(&self, address: &str) -> CoreResult<AgentRouteResolution> {
        let Some(route_key) = parse_agent_route_address(address)? else {
            let agent_id = AgentId::new(address);
            let session = self.sessions.get_session_by_agent(&agent_id)?;
            let entry = self
                .list_agent_directory()?
                .into_iter()
                .find(|entry| {
                    entry.agent_id == session.agent_id && entry.session_id == session.session_id
                })
                .ok_or_else(|| {
                    CoreError::new(CoreErrorKind::NotFound, "agent_directory_entry_not_found")
                })?;
            let binding = entry
                .binding_id
                .as_ref()
                .map(|binding_id| self.store.get_external_agent_binding(binding_id))
                .transpose()?
                .flatten();
            return Ok(AgentRouteResolution {
                address: address.to_string(),
                route: None,
                routable: entry.routable,
                reason_code: entry.routability_reason_code.clone(),
                resolved_target: entry.routable.then(|| {
                    resolved_target_from_directory(
                        &entry,
                        binding.as_ref(),
                        binding
                            .as_ref()
                            .map(|binding| binding.message_delivery_policy),
                    )
                }),
                last_delivery: None,
            });
        };
        let route = self
            .store
            .get_agent_route(&route_key)?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "agent_route_not_found"))?;
        self.resolve_agent_route_record(route)
    }

    pub fn list_agent_directory(&self) -> CoreResult<Vec<AgentDirectoryEntry>> {
        let profiles = self
            .list_profile_registry_records(&ProfileRegistryQuery::default())?
            .into_iter()
            .map(|profile| (profile.profile_id.clone(), profile))
            .collect::<HashMap<_, _>>();
        let bindings = self.store.list_external_agent_bindings()?;
        let runtimes = self
            .store
            .list_external_runtime_registrations()?
            .into_iter()
            .map(|runtime| (runtime.runtime_id.clone(), runtime))
            .collect::<HashMap<_, _>>();
        let mut entries = Vec::new();

        for session in self
            .sessions
            .all_sessions()?
            .into_iter()
            .filter(|session| session.status != SessionStatus::Archived)
        {
            let profile = profiles.get(&session.profile_id);
            let binding = bindings
                .iter()
                .filter(|binding| {
                    binding.purpose == ExternalBindingPurpose::CrewAgent
                        && binding.agent_id.as_ref() == Some(&session.agent_id)
                        && binding.session_id.as_ref() == Some(&session.session_id)
                })
                .max_by_key(|binding| match binding.status {
                    ExternalBindingStatus::Active => 2,
                    ExternalBindingStatus::Paused => 1,
                    ExternalBindingStatus::Archived => 0,
                });

            let workspace = session.workspace.clone();
            let workspace_cwd = workspace.as_ref().map(|w| w.cwd.clone());
            let (
                runtime_kind,
                runtime_id,
                binding_id,
                binding_status,
                task_ref,
                workdir,
                routable,
                reason_code,
            ) = if let Some(binding) = binding {
                let runtime = runtimes.get(&binding.runtime_id).ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        format!(
                            "external binding {} references missing runtime {}",
                            binding.binding_id.0, binding.runtime_id.0
                        ),
                    )
                })?;
                let runtime_kind = match runtime.kind {
                    ExternalRuntimeKind::CodexAppServer => {
                        AgentDirectoryRuntimeKind::CodexAppServer
                    }
                };
                let reason_code = if binding.native_thread_id.is_none() {
                    Some("external_binding_native_thread_missing".to_string())
                } else if binding.status != ExternalBindingStatus::Active {
                    Some("external_binding_not_active".to_string())
                } else if runtime.desired_state != ExternalRuntimeDesiredState::Enabled {
                    Some("external_runtime_disabled".to_string())
                } else if runtime.observed_state != ExternalRuntimeObservedState::Ready {
                    Some("external_runtime_not_ready".to_string())
                } else {
                    None
                };
                (
                    runtime_kind,
                    Some(binding.runtime_id.clone()),
                    Some(binding.binding_id.clone()),
                    Some(binding.status),
                    binding.task_ref.clone(),
                    workspace_cwd.clone(),
                    reason_code.is_none(),
                    reason_code,
                )
            } else {
                (
                    AgentDirectoryRuntimeKind::DirectBrain,
                    None,
                    None,
                    None,
                    None,
                    workspace_cwd,
                    true,
                    None,
                )
            };

            let (session_status, execution) =
                self.project_agent_directory_execution(&session, runtime_kind)?;

            entries.push(AgentDirectoryEntry {
                agent_id: session.agent_id,
                session_id: session.session_id,
                profile_id: session.profile_id.clone(),
                display_label: profile
                    .and_then(|profile| profile.display_name.clone())
                    .unwrap_or(session.profile_id.0),
                session_kind: session.kind,
                session_status,
                execution,
                runtime_kind,
                runtime_id,
                binding_id,
                binding_status,
                task_ref,
                workspace,
                workdir,
                routable,
                routability_reason_code: reason_code,
            });
        }
        entries.sort_by(|left, right| {
            left.display_label
                .cmp(&right.display_label)
                .then_with(|| left.agent_id.0.cmp(&right.agent_id.0))
                .then_with(|| left.session_id.0.cmp(&right.session_id.0))
        });
        Ok(entries)
    }

    fn resolve_agent_address_for_delivery(
        &self,
        address: &str,
    ) -> CoreResult<AgentRouteResolution> {
        match self.resolve_agent_address(address) {
            Ok(resolution) => Ok(resolution),
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                let route_key = parse_agent_route_address(address)?;
                let route = match route_key {
                    Some(route_key) => self.store.get_agent_route(&route_key)?,
                    None => None,
                };
                Ok(AgentRouteResolution {
                    address: address.to_string(),
                    route,
                    routable: false,
                    reason_code: Some(if address.starts_with('@') {
                        "agent_route_not_found".into()
                    } else {
                        "recipient_not_found".into()
                    }),
                    resolved_target: None,
                    last_delivery: None,
                })
            }
            Err(error) => Err(error),
        }
    }

    fn resolve_agent_route_record(
        &self,
        route: AgentRouteRecord,
    ) -> CoreResult<AgentRouteResolution> {
        let route_key = route.route_key.clone();
        let mut resolution = self.resolve_agent_route_target(route)?;
        resolution.last_delivery = self
            .store
            .get_latest_agent_route_delivery(&route_key)?
            .and_then(|receipt| {
                let route_revision = receipt.request.routing.as_ref()?.route_revision;
                Some(AgentRouteLastDelivery {
                    delivery_id: receipt.request.delivery_id,
                    route_revision,
                    status: receipt.status,
                    reason_code: receipt.reason_code,
                    created_at: receipt.request.created_at,
                    terminal_at: receipt.terminal_at,
                })
            });
        Ok(resolution)
    }

    fn resolve_agent_route_target(
        &self,
        route: AgentRouteRecord,
    ) -> CoreResult<AgentRouteResolution> {
        if !route.enabled {
            return Ok(unroutable_route(route, "agent_route_disabled"));
        }
        let directory = self.list_agent_directory()?;
        match &route.target {
            AgentRouteTarget::DirectBrain {
                agent_id,
                session_id,
            } => {
                match self.sessions.get_session(session_id) {
                    Ok(session) if session.agent_id != *agent_id => {
                        return Ok(unroutable_route(
                            route,
                            "agent_route_direct_identity_mismatch",
                        ));
                    }
                    Ok(session) if session.status == SessionStatus::Archived => {
                        return Ok(unroutable_route(route, "agent_route_target_archived"));
                    }
                    Ok(_) => {}
                    Err(error) if error.kind == CoreErrorKind::NotFound => {
                        return Ok(unroutable_route(route, "agent_route_direct_target_missing"));
                    }
                    Err(error) => return Err(error),
                }
                let Some(entry) = directory
                    .iter()
                    .find(|entry| &entry.agent_id == agent_id && &entry.session_id == session_id)
                else {
                    return Ok(unroutable_route(route, "agent_route_direct_target_missing"));
                };
                if entry.session_status == SessionStatus::Archived {
                    return Ok(unroutable_route(route, "agent_route_target_archived"));
                }
                if entry.runtime_kind != AgentDirectoryRuntimeKind::DirectBrain {
                    return Ok(unroutable_route(route, "agent_route_runtime_kind_mismatch"));
                }
                if let Some(required_kind) = route.required_runtime_kind {
                    if required_kind != entry.runtime_kind {
                        return Ok(unroutable_route(route, "agent_route_runtime_kind_mismatch"));
                    }
                }
                if !entry.routable {
                    let reason = entry
                        .routability_reason_code
                        .clone()
                        .unwrap_or_else(|| "agent_route_target_not_routable".into());
                    return Ok(unroutable_route(route, &reason));
                }
                Ok(routable_route(
                    route,
                    resolved_target_from_directory(entry, None, None),
                ))
            }
            AgentRouteTarget::ManagedExternal {
                agent_id,
                binding_id,
                binding_revision,
            } => {
                let Some(binding) = self.store.get_external_agent_binding(binding_id)? else {
                    return Ok(unroutable_route(
                        route,
                        "agent_route_external_binding_missing",
                    ));
                };
                if binding.revision != *binding_revision {
                    return Ok(unroutable_route(
                        route,
                        "agent_route_external_binding_replaced",
                    ));
                }
                if binding.purpose != ExternalBindingPurpose::CrewAgent
                    || binding.agent_id.as_ref() != Some(agent_id)
                {
                    return Ok(unroutable_route(
                        route,
                        "agent_route_external_identity_mismatch",
                    ));
                }
                if binding.status == ExternalBindingStatus::Archived {
                    return Ok(unroutable_route(route, "agent_route_target_archived"));
                }
                if binding.status != ExternalBindingStatus::Active {
                    return Ok(unroutable_route(
                        route,
                        "agent_route_external_binding_not_active",
                    ));
                }
                let Some(session_id) = binding.session_id.as_ref() else {
                    return Ok(unroutable_route(
                        route,
                        "agent_route_external_session_missing",
                    ));
                };
                let Some(entry) = directory.iter().find(|entry| {
                    &entry.agent_id == agent_id
                        && &entry.session_id == session_id
                        && entry.binding_id.as_ref() == Some(binding_id)
                }) else {
                    return Ok(unroutable_route(
                        route,
                        "agent_route_external_target_missing",
                    ));
                };
                if let Some(required_kind) = route.required_runtime_kind {
                    if required_kind != entry.runtime_kind {
                        return Ok(unroutable_route(route, "agent_route_runtime_kind_mismatch"));
                    }
                }
                if let Some(required_policy) = route.required_delivery_policy {
                    if required_policy != binding.message_delivery_policy {
                        return Ok(unroutable_route(
                            route,
                            "agent_route_delivery_policy_mismatch",
                        ));
                    }
                }
                if !entry.routable {
                    let reason = entry
                        .routability_reason_code
                        .clone()
                        .unwrap_or_else(|| "agent_route_target_not_routable".into());
                    return Ok(unroutable_route(route, &reason));
                }
                Ok(routable_route(
                    route,
                    resolved_target_from_directory(
                        entry,
                        Some(&binding),
                        Some(binding.message_delivery_policy),
                    ),
                ))
            }
        }
    }

    pub fn deliver_agent_message(
        &self,
        command: AgentMessageCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        self.deliver_agent_message_with_reply(command, None, None)
    }

    fn deliver_agent_message_with_reply(
        &self,
        command: AgentMessageCommand,
        reply_to_message_id: Option<String>,
        resolved_address: Option<AgentRouteResolution>,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        validate_agent_message_bounds(&command.body, &command.created_at, &command.expires_at)?;
        let (sender_agent_id, sender_session_id, sender_request_id) =
            self.resolve_coordination_caller(&command.caller)?;
        let address_resolution = match resolved_address {
            Some(resolution) => resolution,
            None => self.resolve_agent_address_for_delivery(&command.to_address)?,
        };
        let recipient_agent_id = address_resolution
            .resolved_target
            .as_ref()
            .map(|target| target.agent_id.clone())
            .or_else(|| route_target_agent_id(address_resolution.route.as_ref()))
            .unwrap_or_else(|| AgentId::new(command.to_address.clone()));
        let recipient = address_resolution
            .resolved_target
            .as_ref()
            .map(|target| self.sessions.get_session(&target.session_id))
            .unwrap_or_else(|| self.sessions.get_session_by_agent(&recipient_agent_id));
        let recipient_session_id = address_resolution
            .resolved_target
            .as_ref()
            .map(|target| target.session_id.clone())
            .or_else(|| route_target_session_id(address_resolution.route.as_ref()))
            .or_else(|| {
                recipient
                    .as_ref()
                    .ok()
                    .map(|session| session.session_id.clone())
            });
        let routing = address_resolution
            .route
            .as_ref()
            .zip(address_resolution.resolved_target.as_ref())
            .map(|(route, resolved_target)| {
                Box::new(AgentRouteDeliveryProvenance {
                    address: address_resolution.address.clone(),
                    route_key: route.route_key.clone(),
                    route_revision: route.revision,
                    resolved_target: resolved_target.clone(),
                })
            });
        let request = AgentMessageDeliveryRequest {
            delivery_id: command.delivery_id,
            idempotency_key: command.idempotency_key,
            message_id: command.message_id.clone(),
            from_agent_id: sender_agent_id.clone(),
            from_session_id: sender_session_id,
            requested_address: command.to_address.clone(),
            to_agent_id: recipient_agent_id.clone(),
            to_session_id: recipient_session_id,
            routing,
            reply_to_message_id,
            input_kind: command.input_kind,
            body: command.body.clone(),
            image_attachment_ids: command.image_attachment_ids.clone(),
            collaboration_mode: command.collaboration_mode,
            correlation_id: command.correlation_id.clone(),
            require_wake: command.require_wake,
            created_at: command.created_at.clone(),
            expires_at: command.expires_at.clone(),
        };
        let pending = AgentMessageDeliveryReceipt {
            request,
            status: AgentMessageDeliveryStatus::Pending,
            sequence: None,
            activation: None,
            resolved_round_id: None,
            reason_code: None,
            terminal_at: None,
            revision: 1,
        };
        let pending = self.store.create_agent_message_delivery(&pending)?;
        if pending.status.is_terminal() {
            return Ok(pending);
        }
        if pending.activation.is_some() {
            return Ok(pending);
        }
        if command.expires_at <= self.now() {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Expired,
                None,
                None,
                None,
                Some("agent_message_expired".into()),
            );
        }
        if !address_resolution.routable && command.to_address.starts_with('@') {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Rejected,
                None,
                None,
                None,
                address_resolution
                    .reason_code
                    .or(Some("agent_route_not_routable".into())),
            );
        }
        let message = AgentMessage {
            from: sender_agent_id.clone(),
            to: recipient_agent_id.clone(),
            from_session_id: pending.request.from_session_id.clone(),
            to_session_id: pending.request.to_session_id.clone(),
            body: command.body.clone(),
            correlation_id: command.correlation_id.clone(),
            projection: None,
        };
        if let Some(round) = self.matching_agent_round(&message)? {
            let sequence = self.bus.publish(CoreEvent::AgentMessageRouted {
                message: message.clone(),
            })?;
            let round = self.resolve_agent_round_reply(
                round,
                &message,
                &command.message_id,
                &command.created_at,
            )?;
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Accepted,
                Some(sequence),
                None,
                Some(round.round_id),
                None,
            );
        }
        if !address_resolution.routable {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Rejected,
                None,
                None,
                None,
                address_resolution
                    .reason_code
                    .or(Some("recipient_not_routable".into())),
            );
        }
        let session = match recipient {
            Ok(session) if session.status != SessionStatus::Archived => session,
            Ok(_) => {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    None,
                    None,
                    None,
                    Some("recipient_session_archived".into()),
                )
            }
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    None,
                    None,
                    None,
                    Some("recipient_not_found".into()),
                )
            }
            Err(error) => return Err(error),
        };
        if command.collaboration_mode.is_some()
            && self
                .store
                .list_nonterminal_external_turns()?
                .iter()
                .any(|turn| turn.request.session_id == session.session_id)
        {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Rejected,
                None,
                None,
                None,
                Some("external_collaboration_mode_turn_already_active".into()),
            );
        }
        if !command.require_wake && !pending.request.image_attachment_ids.is_empty() {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Rejected,
                None,
                None,
                None,
                Some("external_message_images_require_wake".into()),
            );
        }
        let validated_image_inputs = match self
            .external_image_inputs(&session.session_id, &pending.request.image_attachment_ids)
        {
            Ok(images) => images,
            Err(error)
                if matches!(
                    error.kind,
                    CoreErrorKind::InvalidInput
                        | CoreErrorKind::NotFound
                        | CoreErrorKind::ActionRejected
                ) =>
            {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    None,
                    None,
                    None,
                    Some(error.message),
                )
            }
            Err(error) => return Err(error),
        };

        let event = CoreEvent::AgentMessageRouted {
            message: message.clone(),
        };
        let sequence = self.bus.publish(event)?;

        if !command.require_wake {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Accepted,
                Some(sequence),
                None,
                None,
                None,
            );
        }

        let model_body = agent_message_model_text(&pending.request);
        let provenance_kind = match pending.request.input_kind {
            AgentMessageInputKind::Operator => TurnInputProvenanceKind::Operator,
            AgentMessageInputKind::RoutedAgentMessage => {
                TurnInputProvenanceKind::RoutedAgentMessage
            }
        };
        let mut activation_input = vec![ExternalTurnInputPart::Text {
            text: model_body.clone(),
        }];
        activation_input.extend(validated_image_inputs);
        let activation_request = AgentActivationRequest {
            agent_id: recipient_agent_id,
            request_id: ExternalTurnRequestId::new(format!("agent-message:{}", command.message_id)),
            idempotency_key: format!("agent-message-turn:{}", command.message_id),
            input: activation_input,
            collaboration_mode: command.collaboration_mode,
            provenance: TurnInputProvenance {
                kind: provenance_kind,
                source_id: Some(command.message_id.clone()),
                correlation_id: command.correlation_id,
            },
            run_id: None,
            capacity_lease_id: format!("agent-message-capacity:{}", command.message_id),
            direct_wake_id: format!("agent-message-wake:{}", command.message_id),
            queued_message_id: format!("agent-message-queue:{}", command.message_id),
            created_at: command.created_at,
            expires_at: Some(command.expires_at),
        };
        let activation = match address_resolution.resolved_target.as_ref() {
            Some(target) => {
                self.activate_agent_execution_for_resolved_target(activation_request, target)?
            }
            None => self.activate_agent_execution(activation_request)?,
        };
        match &activation {
            AgentActivation::DirectBrainWakeRequested { session_id, .. } => {
                self.bus.publish(CoreEvent::BrainWakeRequested {
                    session_id: session_id.clone(),
                })?;
            }
            AgentActivation::QueuedForNextTurn { session_id, .. } => {
                if let Err(error) = self.enqueue_routed_agent_message_without_wake(
                    session_id,
                    &pending.request,
                    model_body,
                ) {
                    if matches!(
                        error.kind,
                        CoreErrorKind::ActionRejected | CoreErrorKind::InvalidInput
                    ) {
                        return self.finish_agent_message_delivery(
                            pending,
                            AgentMessageDeliveryStatus::Rejected,
                            Some(sequence),
                            Some(activation.clone()),
                            None,
                            Some(error.message),
                        );
                    }
                    return Err(error);
                }
            }
            AgentActivation::ExternalTurnSteerRequested { .. } => {
                return self.observe_pending_agent_message_delivery(pending, sequence, activation);
            }
            AgentActivation::ExternalTurnRequested { .. } => {}
            AgentActivation::Rejected { reason_code } => {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    Some(sequence),
                    Some(activation.clone()),
                    None,
                    Some(reason_code.clone()),
                )
            }
        }
        let _ = sender_request_id;
        self.finish_agent_message_delivery(
            pending,
            AgentMessageDeliveryStatus::Accepted,
            Some(sequence),
            Some(activation),
            None,
            None,
        )
    }

    pub fn begin_agent_round(
        &self,
        command: AgentRoundCommand,
    ) -> CoreResult<AgentRoundStartReceipt> {
        let (sender_agent_id, sender_session_id, sender_request_id) =
            self.resolve_coordination_caller(&command.caller)?;
        let resolution = self.resolve_agent_address(&command.to_address)?;
        if !resolution.routable {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                resolution
                    .reason_code
                    .unwrap_or_else(|| "agent_route_not_routable".into()),
            ));
        }
        let resolved_target = resolution.resolved_target.clone().ok_or_else(|| {
            CoreError::new(CoreErrorKind::ActionRejected, "agent_route_not_routable")
        })?;
        let round = AgentCorrelatedRound {
            round_id: command.round_id.clone(),
            idempotency_key: command.idempotency_key.clone(),
            sender_agent_id,
            sender_session_id,
            recipient_agent_id: resolved_target.agent_id,
            recipient_session_id: resolved_target.session_id,
            sender_request_id,
            message_id: command.message_id.clone(),
            correlation_id: command.correlation_id.clone(),
            reply_message_id: None,
            status: AgentRoundStatus::Pending,
            outcome: None,
            terminal_reason_code: None,
            created_at: command.created_at.clone(),
            expires_at: command.expires_at.clone(),
            terminal_at: None,
            revision: 1,
        };
        let round = self.store.create_agent_correlated_round(&round)?;
        self.bus.publish(CoreEvent::AgentRoundObserved {
            round: round.clone(),
        })?;
        let delivery = self.deliver_agent_message_with_reply(
            AgentMessageCommand {
                caller: command.caller,
                delivery_id: rusty_crew_core_protocol::AgentMessageDeliveryId::new(format!(
                    "round-delivery:{}",
                    command.round_id.0
                )),
                idempotency_key: format!("round-delivery:{}", command.idempotency_key),
                message_id: command.message_id,
                to_address: command.to_address,
                body: command.body,
                image_attachment_ids: Vec::new(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                collaboration_mode: None,
                correlation_id: Some(command.correlation_id),
                require_wake: true,
                created_at: command.created_at,
                expires_at: command.expires_at,
            },
            None,
            Some(resolution),
        )?;
        Ok(AgentRoundStartReceipt { round, delivery })
    }

    pub fn get_agent_round(
        &self,
        round_id: &rusty_crew_core_protocol::AgentRoundId,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        let Some(round) = self.store.get_agent_correlated_round(round_id)? else {
            return Ok(None);
        };
        if round.status != AgentRoundStatus::Pending || round.expires_at > self.now() {
            return Ok(Some(round));
        }
        let mut expired = round.clone();
        expired.status = AgentRoundStatus::Expired;
        expired.terminal_reason_code = Some("agent_round_timeout".into());
        expired.terminal_at = Some(self.now());
        let expired = self
            .store
            .update_agent_correlated_round(&expired, round.revision)?;
        self.bus.publish(CoreEvent::AgentRoundObserved {
            round: expired.clone(),
        })?;
        Ok(Some(expired))
    }

    pub fn get_agent_message_delivery(
        &self,
        delivery_id: &rusty_crew_core_protocol::AgentMessageDeliveryId,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        self.store.get_agent_message_delivery(delivery_id)
    }

    pub fn get_agent_message_delivery_by_message_id(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        self.store
            .get_agent_message_delivery_by_message_id(message_id)
    }

    pub fn list_agent_message_inbox(
        &self,
        query: &AgentMessageInboxQuery,
    ) -> CoreResult<Vec<AgentMessageInboxItem>> {
        let limit = query.limit.unwrap_or(100).clamp(1, 500);
        let deliveries = self
            .store
            .list_agent_message_inbox_deliveries(query, limit)?;
        deliveries
            .into_iter()
            .map(|delivery| self.project_agent_message_inbox_item(delivery))
            .collect()
    }

    fn project_agent_message_inbox_item(
        &self,
        delivery: AgentMessageDeliveryReceipt,
    ) -> CoreResult<AgentMessageInboxItem> {
        let reply = self
            .store
            .get_agent_message_reply(&delivery.request.message_id)?;
        let queued_message_id = format!("agent-message-queue:{}", delivery.request.message_id);
        let queue = delivery
            .request
            .to_session_id
            .as_ref()
            .map(|session_id| {
                self.store.load_queued_messages(&QueuedMessageFilter {
                    state: None,
                    owner_session_id: Some(session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
            })
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .find(|queued| queued.message_id == queued_message_id);
        let direct_request_id =
            ExternalTurnRequestId::new(format!("agent-message:{}", delivery.request.message_id));
        let follow_up_request_id =
            ExternalTurnRequestId::new(format!("external-follow-up:{queued_message_id}"));
        let turn = self
            .store
            .get_external_turn(&direct_request_id)?
            .or(self.store.get_external_turn(&follow_up_request_id)?);
        let status = match delivery.status {
            AgentMessageDeliveryStatus::Rejected => AgentMessageInboxStatus::Rejected,
            AgentMessageDeliveryStatus::Expired => AgentMessageInboxStatus::Expired,
            AgentMessageDeliveryStatus::Pending => AgentMessageInboxStatus::InProgress,
            AgentMessageDeliveryStatus::Accepted => {
                if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Pending)
                ) {
                    AgentMessageInboxStatus::Queued
                } else if let Some(turn) = turn.as_ref() {
                    match turn.phase {
                        ExternalTurnPhase::Accepted
                        | ExternalTurnPhase::Starting
                        | ExternalTurnPhase::Active
                        | ExternalTurnPhase::WaitingInteraction => {
                            AgentMessageInboxStatus::InProgress
                        }
                        ExternalTurnPhase::Completed if reply.is_some() => {
                            AgentMessageInboxStatus::Replied
                        }
                        ExternalTurnPhase::Completed
                            if matches!(
                                turn.terminal_reason_code.as_deref(),
                                Some("review_no_reply" | "agent_message_no_reply")
                            ) =>
                        {
                            AgentMessageInboxStatus::NoReply
                        }
                        ExternalTurnPhase::Completed => AgentMessageInboxStatus::AwaitingReply,
                        ExternalTurnPhase::Failed
                        | ExternalTurnPhase::Interrupted
                        | ExternalTurnPhase::OutcomeUnknown => AgentMessageInboxStatus::Failed,
                    }
                } else if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Expired)
                ) {
                    AgentMessageInboxStatus::Expired
                } else if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Cancelled)
                ) {
                    AgentMessageInboxStatus::Rejected
                } else if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Discarded)
                ) {
                    AgentMessageInboxStatus::Failed
                } else if reply.is_some() {
                    AgentMessageInboxStatus::Replied
                } else {
                    AgentMessageInboxStatus::NoReply
                }
            }
        };
        let external_turn_request_id = turn
            .as_ref()
            .map(|record| record.request.request_id.clone());
        let terminal_reason_code = turn
            .as_ref()
            .and_then(|record| record.terminal_reason_code.clone())
            .or_else(|| {
                queue
                    .as_ref()
                    .and_then(|record| record.state_reason.clone())
            });
        let delivered_model_text = agent_message_model_text(&delivery.request);
        Ok(AgentMessageInboxItem {
            delivery,
            reply,
            status,
            delivered_model_text,
            queued_message_id: queue.map(|record| record.message_id),
            external_turn_request_id,
            terminal_reason_code,
        })
    }

    pub fn reply_agent_message(
        &self,
        command: AgentMessageReplyCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let (replying_agent_id, replying_session_id, _) =
            self.resolve_coordination_caller(&command.caller)?;
        let original = self
            .store
            .get_agent_message_delivery_by_message_id(&command.in_reply_to_message_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "agent_message_reply_original_not_found",
                )
            })?;
        if original.status != AgentMessageDeliveryStatus::Accepted {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_original_not_accepted",
            ));
        }
        if original.request.to_agent_id != replying_agent_id
            || original.request.to_session_id != replying_session_id
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_wrong_recipient",
            ));
        }
        let expected_reply_session = original.request.from_session_id.clone().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_sender_has_no_session",
            )
        })?;
        let current_reply_session = match self.sessions.get_session(&expected_reply_session) {
            Ok(session) => session,
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "agent_message_reply_sender_session_changed",
                ));
            }
            Err(error) => return Err(error),
        };
        if current_reply_session.agent_id != original.request.from_agent_id
            || current_reply_session.status == SessionStatus::Archived
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_sender_session_changed",
            ));
        }
        if let Some(existing) = self
            .store
            .get_agent_message_reply(&command.in_reply_to_message_id)?
        {
            if existing.request.delivery_id == command.delivery_id
                && existing.request.idempotency_key == command.idempotency_key
                && existing.request.message_id == command.message_id
                && existing.request.body == command.body
            {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "agent_message_reply_already_exists",
            ));
        }
        let replied_to = command.in_reply_to_message_id.clone();
        let replied_at = command.created_at.clone();
        let reply_entry = self
            .list_agent_directory()?
            .into_iter()
            .find(|entry| entry.session_id == expected_reply_session)
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "agent_message_reply_sender_session_changed",
                )
            })?;
        let reply_binding = reply_entry
            .binding_id
            .as_ref()
            .map(|binding_id| self.store.get_external_agent_binding(binding_id))
            .transpose()?
            .flatten();
        let reply_resolution = AgentRouteResolution {
            address: original.request.from_agent_id.0.clone(),
            route: None,
            routable: reply_entry.routable,
            reason_code: reply_entry.routability_reason_code.clone(),
            resolved_target: Some(resolved_target_from_directory(
                &reply_entry,
                reply_binding.as_ref(),
                reply_binding
                    .as_ref()
                    .map(|binding| binding.message_delivery_policy),
            )),
            last_delivery: None,
        };
        let receipt = self.deliver_agent_message_with_reply(
            AgentMessageCommand {
                caller: command.caller,
                delivery_id: command.delivery_id,
                idempotency_key: command.idempotency_key,
                message_id: command.message_id,
                to_address: original.request.from_agent_id.0,
                body: command.body,
                image_attachment_ids: Vec::new(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                collaboration_mode: None,
                correlation_id: original
                    .request
                    .correlation_id
                    .or(Some(command.in_reply_to_message_id.clone())),
                require_wake: true,
                created_at: command.created_at,
                expires_at: command.expires_at,
            },
            Some(command.in_reply_to_message_id),
            Some(reply_resolution),
        )?;
        if receipt.status == AgentMessageDeliveryStatus::Accepted {
            self.mark_review_reply_terminal(&replied_to, &replied_at)?;
        }
        Ok(receipt)
    }

    pub fn complete_agent_message_delivery(
        &self,
        completion: rusty_crew_core_protocol::AgentMessageDeliveryCompletion,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let current = self
            .store
            .get_agent_message_delivery(&completion.delivery_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "agent message delivery was not found",
                )
            })?;
        if current.status.is_terminal() {
            return Ok(current);
        }
        if current.revision != completion.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_delivery_revision_conflict",
            ));
        }
        if !matches!(
            current.activation,
            Some(AgentActivation::ExternalTurnSteerRequested { .. })
        ) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_delivery_completion_requires_pending_steer",
            ));
        }
        if !matches!(
            completion.status,
            AgentMessageDeliveryStatus::Accepted
                | AgentMessageDeliveryStatus::Rejected
                | AgentMessageDeliveryStatus::Expired
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "agent message delivery completion must be terminal",
            ));
        }
        let mut next = current;
        let expected_revision = next.revision;
        next.status = completion.status;
        next.reason_code = completion.reason_code;
        next.terminal_at = Some(completion.completed_at);
        let saved = self
            .store
            .update_agent_message_delivery(&next, expected_revision)?;
        self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
            receipt: Box::new(saved.clone()),
        })?;
        if saved.status == AgentMessageDeliveryStatus::Accepted {
            if let Some(replied_to) = saved.request.reply_to_message_id.as_deref() {
                let terminal_at = saved
                    .terminal_at
                    .as_ref()
                    .unwrap_or(&saved.request.created_at);
                self.mark_review_reply_terminal(replied_to, terminal_at)?;
            }
        }
        Ok(saved)
    }

    pub(crate) fn resolve_coordination_caller(
        &self,
        caller: &AgentCoordinationCaller,
    ) -> CoreResult<(AgentId, Option<SessionId>, Option<ExternalTurnRequestId>)> {
        match caller {
            AgentCoordinationCaller::System { sender_agent_id } => {
                Ok((sender_agent_id.clone(), None, None))
            }
            AgentCoordinationCaller::DirectBrain { session_id, .. } => {
                let session = self.sessions.get_session(session_id)?;
                if session.status == SessionStatus::Archived {
                    return Err(CoreError::new(
                        CoreErrorKind::SessionExpired,
                        "archived direct-brain session cannot send agent messages",
                    ));
                }
                Ok((session.agent_id, Some(session.session_id), None))
            }
            AgentCoordinationCaller::ExternalAgent {
                runtime_id,
                binding_id,
                controller_instance_id,
                controller_generation,
                native_thread_id,
                native_turn_id,
                ..
            } => {
                let lease = self
                    .store
                    .get_external_controller_lease(runtime_id)?
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::ActionRejected,
                            "external coordination caller has no controller lease",
                        )
                    })?;
                if lease.holder_instance_id != *controller_instance_id
                    || lease.generation != *controller_generation
                    || lease.expires_at <= self.now()
                {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external coordination caller does not hold the current controller lease",
                    ));
                }
                let binding = self
                    .store
                    .get_external_agent_binding(binding_id)?
                    .ok_or_else(|| {
                        CoreError::new(CoreErrorKind::NotFound, "external binding was not found")
                    })?;
                if !binding.is_routable()
                    || binding.runtime_id != *runtime_id
                    || binding.native_thread_id.as_ref() != Some(native_thread_id)
                {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external coordination caller does not match its durable binding",
                    ));
                }
                let turn = self
                    .store
                    .list_nonterminal_external_turns()?
                    .into_iter()
                    .find(|turn| {
                        turn.request.binding_id == *binding_id
                            && turn.native_thread_id == *native_thread_id
                            && turn.native_turn_id.as_ref() == Some(native_turn_id)
                    })
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::ActionRejected,
                            "external coordination caller is not the active native turn",
                        )
                    })?;
                Ok((
                    binding.agent_id.expect("routable binding has agent id"),
                    Some(binding.session_id.expect("routable binding has session id")),
                    Some(turn.request.request_id),
                ))
            }
            AgentCoordinationCaller::ExternalCli { .. } => Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_cli_caller_is_review_submission_only",
            )),
            AgentCoordinationCaller::ReviewSubmission { submission_id } => {
                let (agent_id, session_id) =
                    self.resolve_review_submission_caller(submission_id)?;
                Ok((agent_id, session_id, None))
            }
        }
    }

    fn matching_agent_round(
        &self,
        message: &AgentMessage,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        let Some(correlation_id) = message.correlation_id.as_ref() else {
            return Ok(None);
        };
        Ok(self
            .store
            .list_pending_agent_rounds()?
            .into_iter()
            .find(|round| {
                round.sender_agent_id == message.to
                    && round.recipient_agent_id == message.from
                    && round.correlation_id == *correlation_id
            }))
    }

    fn resolve_agent_round_reply(
        &self,
        round: AgentCorrelatedRound,
        message: &AgentMessage,
        reply_message_id: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<AgentCorrelatedRound> {
        let correlation_id = message
            .correlation_id
            .as_ref()
            .expect("matched round reply has a correlation id");
        if round.expires_at <= *now {
            let mut expired = round.clone();
            expired.status = AgentRoundStatus::Expired;
            expired.terminal_reason_code = Some("late_agent_round_reply".into());
            expired.terminal_at = Some(now.clone());
            let expired = self
                .store
                .update_agent_correlated_round(&expired, round.revision)?;
            self.bus.publish(CoreEvent::AgentRoundObserved {
                round: expired.clone(),
            })?;
            return Ok(expired);
        }
        let mut replied = round.clone();
        replied.reply_message_id = Some(reply_message_id.to_string());
        replied.status = AgentRoundStatus::Replied;
        replied.outcome = Some(json!({
            "from": message.from.0,
            "to": message.to.0,
            "body": message.body,
            "correlationId": correlation_id,
        }));
        replied.terminal_at = Some(now.clone());
        let replied = self
            .store
            .update_agent_correlated_round(&replied, round.revision)?;
        self.bus.publish(CoreEvent::AgentRoundObserved {
            round: replied.clone(),
        })?;
        Ok(replied)
    }

    fn finish_agent_message_delivery(
        &self,
        mut pending: AgentMessageDeliveryReceipt,
        status: AgentMessageDeliveryStatus,
        sequence: Option<u64>,
        activation: Option<AgentActivation>,
        resolved_round_id: Option<rusty_crew_core_protocol::AgentRoundId>,
        reason_code: Option<String>,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let expected_revision = pending.revision;
        pending.status = status;
        pending.sequence = sequence;
        pending.activation = activation;
        pending.resolved_round_id = resolved_round_id;
        pending.reason_code = reason_code;
        pending.terminal_at = Some(self.now());
        let receipt = self
            .store
            .update_agent_message_delivery(&pending, expected_revision)?;
        self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
            receipt: Box::new(receipt.clone()),
        })?;
        Ok(receipt)
    }

    fn observe_pending_agent_message_delivery(
        &self,
        mut pending: AgentMessageDeliveryReceipt,
        sequence: u64,
        activation: AgentActivation,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let expected_revision = pending.revision;
        pending.sequence = Some(sequence);
        pending.activation = Some(activation);
        let receipt = self
            .store
            .update_agent_message_delivery(&pending, expected_revision)?;
        self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
            receipt: Box::new(receipt.clone()),
        })?;
        Ok(receipt)
    }
}

fn routable_route(
    route: AgentRouteRecord,
    resolved_target: AgentRouteResolvedTarget,
) -> AgentRouteResolution {
    AgentRouteResolution {
        address: route.address(),
        route: Some(route),
        routable: true,
        reason_code: None,
        resolved_target: Some(resolved_target),
        last_delivery: None,
    }
}

fn unroutable_route(route: AgentRouteRecord, reason_code: &str) -> AgentRouteResolution {
    AgentRouteResolution {
        address: route.address(),
        route: Some(route),
        routable: false,
        reason_code: Some(reason_code.to_string()),
        resolved_target: None,
        last_delivery: None,
    }
}

fn resolved_target_from_directory(
    entry: &AgentDirectoryEntry,
    binding: Option<&ExternalAgentBinding>,
    delivery_policy: Option<ExternalMessageDeliveryPolicy>,
) -> AgentRouteResolvedTarget {
    AgentRouteResolvedTarget {
        agent_id: entry.agent_id.clone(),
        session_id: entry.session_id.clone(),
        profile_id: entry.profile_id.clone(),
        display_label: entry.display_label.clone(),
        runtime_kind: entry.runtime_kind,
        runtime_id: entry.runtime_id.clone(),
        binding_id: entry.binding_id.clone(),
        binding_revision: binding.map(|binding| binding.revision),
        delivery_policy,
    }
}

fn route_target_agent_id(route: Option<&AgentRouteRecord>) -> Option<AgentId> {
    route.map(|route| match &route.target {
        AgentRouteTarget::DirectBrain { agent_id, .. }
        | AgentRouteTarget::ManagedExternal { agent_id, .. } => agent_id.clone(),
    })
}

fn route_target_session_id(route: Option<&AgentRouteRecord>) -> Option<SessionId> {
    route.and_then(|route| match &route.target {
        AgentRouteTarget::DirectBrain { session_id, .. } => Some(session_id.clone()),
        AgentRouteTarget::ManagedExternal { .. } => None,
    })
}

fn validate_agent_message_bounds(
    body: &str,
    created_at: &IsoTimestamp,
    expires_at: &IsoTimestamp,
) -> CoreResult<()> {
    const MIN_TTL_MS: i128 = 1;
    const MAX_TTL_MS: i128 = 24 * 60 * 60 * 1_000;
    const MAX_BODY_BYTES: usize = 256 * 1024;
    if body.is_empty() || body.len() > MAX_BODY_BYTES {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "agent_message_body_size_invalid",
        ));
    }
    let ttl_ms = (parse_rfc3339(expires_at)? - parse_rfc3339(created_at)?).whole_milliseconds();
    if !(MIN_TTL_MS..=MAX_TTL_MS).contains(&ttl_ms) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "agent_message_ttl_out_of_bounds",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/agent_coordination_text.rs"]
mod routed_agent_message_text_tests;
