use super::*;
use rusty_crew_core_protocol::{
    InstallDiplomatBindingQuery, InstallDiplomatBindingRecord, InstallDiplomatBindingStatus,
    InstallDiplomatBindingStatusUpdate, InstallDiplomatBindingWrite,
    InstallDiplomatParticipationMode, InstallDiplomatRebindRequest,
    TelegramDiplomatIngressDecision, TelegramDiplomatIngressPlan, TelegramDiplomatIngressRequest,
    TelegramDiplomatInteractionRecord, TelegramDiplomatInteractionTerminalReason,
    TelegramDiplomatSenderKind, TELEGRAM_DIPLOMAT_INTERACTION_VERSION,
    TELEGRAM_INSTALL_DIPLOMAT_BINDING_VERSION,
};

const DEFAULT_TELEGRAM_BOT_DEPTH_LIMIT: u32 = 6;
const DEFAULT_TELEGRAM_BOT_MESSAGE_LIMIT: u32 = 8;
const DEFAULT_TELEGRAM_INTERACTION_MINUTES: i64 = 5;
const DEFAULT_TELEGRAM_PAIR_MESSAGES_PER_MINUTE: usize = 8;

impl CoreEngine {
    pub(crate) fn bound_install_diplomat_session_ids(&self) -> CoreResult<HashSet<SessionId>> {
        Ok(self
            .store
            .list_install_diplomat_bindings(&InstallDiplomatBindingQuery::default())?
            .into_iter()
            .filter(|record| {
                matches!(
                    record.status,
                    InstallDiplomatBindingStatus::Active | InstallDiplomatBindingStatus::Paused
                )
            })
            .map(|record| record.session_id)
            .collect())
    }

    pub fn put_install_diplomat_binding(
        &self,
        write: InstallDiplomatBindingWrite,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        validate_binding_write(&write)?;
        let session = self.validate_diplomat_session(&write.session_id, &write.agent_id)?;
        let existing = self.store.get_install_diplomat_binding(&write.binding_id)?;
        match (&existing, write.expected_revision) {
            (Some(_), None) => {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    "install_diplomat_binding_exists",
                ));
            }
            (None, Some(_)) => {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    "install_diplomat_binding_not_found",
                ));
            }
            _ => {}
        }
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| write.updated_at.clone());
        let record = InstallDiplomatBindingRecord {
            schema_version: TELEGRAM_INSTALL_DIPLOMAT_BINDING_VERSION.to_string(),
            binding_id: write.binding_id,
            revision: existing.as_ref().map_or(1, |record| record.revision + 1),
            installation_id: write.installation_id,
            installation_label: write.installation_label,
            adapter_id: write.adapter_id,
            bot_user_id: write.bot_user_id,
            bot_username: normalize_bot_username(&write.bot_username),
            agent_id: session.agent_id,
            instance_id: write.instance_id,
            session_id: session.session_id,
            external_chat_id: write.external_chat_id,
            external_thread_id: normalize_optional(write.external_thread_id),
            participation_mode: write.participation_mode,
            status: InstallDiplomatBindingStatus::Active,
            degraded_reason: None,
            created_at,
            updated_at: write.updated_at,
        };
        if let Some(expected_revision) = write.expected_revision {
            self.store
                .update_install_diplomat_binding(&record, expected_revision)
        } else {
            self.store.insert_install_diplomat_binding(&record)
        }
    }

    pub fn rebind_install_diplomat(
        &self,
        request: InstallDiplomatRebindRequest,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        let session = self.validate_diplomat_session(&request.session_id, &request.agent_id)?;
        let mut record = self
            .store
            .get_install_diplomat_binding(&request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "install_diplomat_binding_not_found",
                )
            })?;
        if record.revision != request.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "install_diplomat_binding_revision_conflict",
            ));
        }
        record.revision += 1;
        record.agent_id = session.agent_id;
        record.instance_id = request.instance_id;
        record.session_id = session.session_id;
        record.status = InstallDiplomatBindingStatus::Active;
        record.degraded_reason = None;
        record.updated_at = request.updated_at;
        self.store
            .update_install_diplomat_binding(&record, request.expected_revision)
    }

    pub fn set_install_diplomat_binding_status(
        &self,
        update: InstallDiplomatBindingStatusUpdate,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        let mut record = self
            .store
            .get_install_diplomat_binding(&update.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "install_diplomat_binding_not_found",
                )
            })?;
        if record.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "install_diplomat_binding_revision_conflict",
            ));
        }
        if update.status == InstallDiplomatBindingStatus::Active {
            self.validate_diplomat_session(&record.session_id, &record.agent_id)?;
        }
        record.revision += 1;
        record.status = update.status;
        record.degraded_reason = normalize_optional(update.degraded_reason);
        record.updated_at = update.updated_at;
        self.store
            .update_install_diplomat_binding(&record, update.expected_revision)
    }

    pub fn get_install_diplomat_binding(
        &self,
        binding_id: &str,
    ) -> CoreResult<Option<InstallDiplomatBindingRecord>> {
        self.store.get_install_diplomat_binding(binding_id)
    }

    pub fn list_install_diplomat_bindings(
        &self,
        query: &InstallDiplomatBindingQuery,
    ) -> CoreResult<Vec<InstallDiplomatBindingRecord>> {
        self.store.list_install_diplomat_bindings(query)
    }

    pub fn plan_telegram_diplomat_ingress(
        &self,
        request: TelegramDiplomatIngressRequest,
    ) -> CoreResult<TelegramDiplomatIngressPlan> {
        let _interaction_guard = self.install_diplomat_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "install diplomat interaction lock poisoned",
            )
        })?;
        validate_ingress_request(&request)?;
        let mut binding = self
            .store
            .get_install_diplomat_binding(&request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "install_diplomat_binding_not_found",
                )
            })?;
        if !binding.status.is_routable() {
            return Ok(ingress_plan(
                TelegramDiplomatIngressDecision::BindingUnavailable,
                "telegram_diplomat_binding_unavailable",
                binding,
                None,
                &request,
                None,
            ));
        }
        if request.receiving_bot_user_id != binding.bot_user_id {
            return Ok(ingress_plan(
                TelegramDiplomatIngressDecision::Ignored,
                "telegram_diplomat_wrong_bot",
                binding,
                None,
                &request,
                None,
            ));
        }
        let session = match self.validate_diplomat_session(&binding.session_id, &binding.agent_id) {
            Ok(session) => session,
            Err(error)
                if matches!(
                    error.kind,
                    CoreErrorKind::NotFound | CoreErrorKind::ActionRejected
                ) =>
            {
                let expected_revision = binding.revision;
                binding.revision += 1;
                binding.status = InstallDiplomatBindingStatus::NeedsRebind;
                binding.degraded_reason = Some("diplomat_session_unavailable".to_string());
                binding.updated_at = request.received_at.clone();
                binding = self
                    .store
                    .update_install_diplomat_binding(&binding, expected_revision)?;
                return Ok(ingress_plan(
                    TelegramDiplomatIngressDecision::BindingUnavailable,
                    "telegram_diplomat_session_unavailable",
                    binding,
                    None,
                    &request,
                    None,
                ));
            }
            Err(error) => return Err(error),
        };
        let existing = self
            .store
            .get_telegram_diplomat_interaction(&request.interaction_id)?;
        if existing
            .as_ref()
            .is_some_and(|interaction| interaction.binding_id != binding.binding_id)
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "telegram_diplomat_interaction_binding_conflict",
            ));
        }

        if !message_participates(&binding, &request, existing.as_ref()) {
            return Ok(ingress_plan(
                TelegramDiplomatIngressDecision::Ignored,
                "telegram_diplomat_message_not_addressed",
                binding,
                existing,
                &request,
                None,
            ));
        }

        let expected_revision = existing.as_ref().map(|record| record.revision);
        let mut interaction = match existing {
            Some(interaction) => interaction,
            None if request.sender.kind == TelegramDiplomatSenderKind::Human => {
                new_interaction(&binding, &request)?
            }
            None => {
                return Ok(ingress_plan(
                    TelegramDiplomatIngressDecision::Ignored,
                    "telegram_diplomat_human_origin_required",
                    binding,
                    None,
                    &request,
                    None,
                ));
            }
        };

        if let Some(reason) = interaction.terminal_reason {
            return Ok(ingress_plan(
                terminal_decision(reason),
                terminal_reason_code(reason),
                binding,
                Some(interaction),
                &request,
                None,
            ));
        }

        let now = parse_rfc3339(&request.received_at)?;
        if now > parse_rfc3339(&interaction.deadline_at)? {
            interaction.terminal_reason =
                Some(TelegramDiplomatInteractionTerminalReason::InteractionExpired);
        } else if request.sender.kind == TelegramDiplomatSenderKind::Bot {
            let pair_key = bot_pair_key(&request);
            let other_pair_message_count = self
                .store
                .list_telegram_diplomat_interactions(&binding.binding_id)?
                .into_iter()
                .filter(|record| record.interaction_id != interaction.interaction_id)
                .filter(|record| record.bot_pair_key.as_deref() == Some(pair_key.as_str()))
                .flat_map(|record| record.bot_message_timestamps)
                .filter(|timestamp| {
                    parse_rfc3339(timestamp)
                        .is_ok_and(|observed| observed > now - Duration::minutes(1))
                })
                .count();
            apply_bot_budget(&mut interaction, &request, now, other_pair_message_count)?;
        }

        interaction.revision = expected_revision.map_or(1, |revision| revision + 1);
        interaction.last_external_message_id = request.external_message_id.clone();
        interaction.last_sender = request.sender.clone();
        interaction.updated_at = request.received_at.clone();
        let interaction = self
            .store
            .put_telegram_diplomat_interaction(&interaction, expected_revision)?;
        let (decision, reason_code) = interaction
            .terminal_reason
            .map(|reason| (terminal_decision(reason), terminal_reason_code(reason)))
            .unwrap_or((
                TelegramDiplomatIngressDecision::Routed,
                "telegram_diplomat_routed",
            ));
        Ok(TelegramDiplomatIngressPlan {
            decision,
            reason_code: reason_code.to_string(),
            target_session_id: (decision == TelegramDiplomatIngressDecision::Routed)
                .then_some(session.session_id),
            crew_correlation_id: Some(interaction.crew_correlation_id.clone()),
            interaction: Some(interaction),
            binding,
            sender: request.sender,
            reply_to_external_message_id: request.reply_to_external_message_id,
        })
    }

    pub(crate) fn degrade_install_diplomat_bindings_for_session(
        &self,
        session_id: &SessionId,
        now: &str,
    ) -> CoreResult<()> {
        let records = self
            .store
            .list_install_diplomat_bindings(&InstallDiplomatBindingQuery {
                session_id: Some(session_id.clone()),
                ..InstallDiplomatBindingQuery::default()
            })?;
        for mut record in records
            .into_iter()
            .filter(|record| record.status != InstallDiplomatBindingStatus::Removed)
        {
            let expected_revision = record.revision;
            record.revision += 1;
            record.status = InstallDiplomatBindingStatus::NeedsRebind;
            record.degraded_reason = Some("diplomat_session_archived".to_string());
            record.updated_at = now.to_string();
            self.store
                .update_install_diplomat_binding(&record, expected_revision)?;
        }
        Ok(())
    }

    pub(crate) fn reconcile_install_diplomat_bindings_for_session(
        &self,
        session: &SessionState,
        now: &str,
    ) -> CoreResult<()> {
        if session.kind != SessionKind::Full || session.status == SessionStatus::Archived {
            return Ok(());
        }
        let records = self
            .store
            .list_install_diplomat_bindings(&InstallDiplomatBindingQuery {
                session_id: Some(session.session_id.clone()),
                ..InstallDiplomatBindingQuery::default()
            })?;
        for mut record in records.into_iter().filter(|record| {
            record.status == InstallDiplomatBindingStatus::NeedsRebind
                && record.degraded_reason.as_deref() == Some("diplomat_session_archived")
                && record.agent_id == session.agent_id
        }) {
            let expected_revision = record.revision;
            record.revision += 1;
            record.status = InstallDiplomatBindingStatus::Active;
            record.degraded_reason = None;
            record.updated_at = now.to_string();
            self.store
                .update_install_diplomat_binding(&record, expected_revision)?;
        }
        Ok(())
    }

    fn validate_diplomat_session(
        &self,
        session_id: &SessionId,
        agent_id: &AgentId,
    ) -> CoreResult<SessionState> {
        let session = self.sessions.get_session(session_id)?;
        if &session.agent_id != agent_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "install_diplomat_session_agent_mismatch",
            ));
        }
        if session.kind != SessionKind::Full {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "install_diplomat_full_session_required",
            ));
        }
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "install_diplomat_session_archived",
            ));
        }
        Ok(session)
    }
}

fn validate_binding_write(write: &InstallDiplomatBindingWrite) -> CoreResult<()> {
    for (field, value) in [
        ("binding_id", write.binding_id.as_str()),
        ("installation_id", write.installation_id.as_str()),
        ("installation_label", write.installation_label.as_str()),
        ("bot_user_id", write.bot_user_id.as_str()),
        ("bot_username", write.bot_username.as_str()),
        ("external_chat_id", write.external_chat_id.as_str()),
        ("updated_at", write.updated_at.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("install_diplomat_{field}_required"),
            ));
        }
    }
    parse_rfc3339(&write.updated_at)?;
    Ok(())
}

fn validate_ingress_request(request: &TelegramDiplomatIngressRequest) -> CoreResult<()> {
    for (field, value) in [
        ("binding_id", request.binding_id.as_str()),
        ("interaction_id", request.interaction_id.as_str()),
        ("external_message_id", request.external_message_id.as_str()),
        (
            "sender_external_user_id",
            request.sender.external_user_id.as_str(),
        ),
        (
            "receiving_bot_user_id",
            request.receiving_bot_user_id.as_str(),
        ),
        ("received_at", request.received_at.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("telegram_diplomat_{field}_required"),
            ));
        }
    }
    parse_rfc3339(&request.received_at)?;
    Ok(())
}

fn message_participates(
    binding: &InstallDiplomatBindingRecord,
    request: &TelegramDiplomatIngressRequest,
    interaction: Option<&TelegramDiplomatInteractionRecord>,
) -> bool {
    match request.sender.kind {
        TelegramDiplomatSenderKind::Human => {
            binding.participation_mode == InstallDiplomatParticipationMode::TopicHumanMessages
                || request.addressed_to_bot
                || request.correlated_interaction
        }
        TelegramDiplomatSenderKind::Bot => {
            interaction.is_some() && (request.addressed_to_bot || request.correlated_interaction)
        }
        TelegramDiplomatSenderKind::SenderChat => false,
    }
}

fn new_interaction(
    binding: &InstallDiplomatBindingRecord,
    request: &TelegramDiplomatIngressRequest,
) -> CoreResult<TelegramDiplomatInteractionRecord> {
    let deadline = parse_rfc3339(&request.received_at)?
        + Duration::minutes(DEFAULT_TELEGRAM_INTERACTION_MINUTES);
    let deadline_at = deadline.format(&Rfc3339).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InternalError,
            format!("format Telegram interaction deadline: {error}"),
        )
    })?;
    Ok(TelegramDiplomatInteractionRecord {
        schema_version: TELEGRAM_DIPLOMAT_INTERACTION_VERSION.to_string(),
        interaction_id: request.interaction_id.clone(),
        binding_id: binding.binding_id.clone(),
        revision: 1,
        root_external_message_id: request.external_message_id.clone(),
        last_external_message_id: request.external_message_id.clone(),
        last_sender: request.sender.clone(),
        bot_pair_key: None,
        bot_depth: 0,
        bot_message_count: 0,
        bot_message_timestamps: Vec::new(),
        crew_correlation_id: format!("telegram:{}:{}", binding.binding_id, request.interaction_id),
        deadline_at,
        terminal_reason: None,
        created_at: request.received_at.clone(),
        updated_at: request.received_at.clone(),
    })
}

fn apply_bot_budget(
    interaction: &mut TelegramDiplomatInteractionRecord,
    request: &TelegramDiplomatIngressRequest,
    now: OffsetDateTime,
    other_pair_message_count: usize,
) -> CoreResult<()> {
    interaction.bot_depth += 1;
    interaction.bot_message_count += 1;
    let pair_key = bot_pair_key(request);
    if interaction.bot_pair_key.as_deref() != Some(pair_key.as_str()) {
        interaction.bot_message_timestamps.clear();
    }
    interaction.bot_pair_key = Some(pair_key);
    let cutoff = now - Duration::minutes(1);
    interaction
        .bot_message_timestamps
        .retain(|timestamp| parse_rfc3339(timestamp).is_ok_and(|observed| observed > cutoff));
    interaction
        .bot_message_timestamps
        .push(request.received_at.clone());
    interaction.terminal_reason = if interaction.bot_depth > DEFAULT_TELEGRAM_BOT_DEPTH_LIMIT {
        Some(TelegramDiplomatInteractionTerminalReason::DepthExceeded)
    } else if interaction.bot_message_count > DEFAULT_TELEGRAM_BOT_MESSAGE_LIMIT {
        Some(TelegramDiplomatInteractionTerminalReason::MessageBudgetExceeded)
    } else if other_pair_message_count + interaction.bot_message_timestamps.len()
        > DEFAULT_TELEGRAM_PAIR_MESSAGES_PER_MINUTE
    {
        Some(TelegramDiplomatInteractionTerminalReason::BotPairRateLimited)
    } else {
        None
    };
    Ok(())
}

fn bot_pair_key(request: &TelegramDiplomatIngressRequest) -> String {
    format!(
        "{}>{}",
        request.sender.external_user_id, request.receiving_bot_user_id
    )
}

fn ingress_plan(
    decision: TelegramDiplomatIngressDecision,
    reason_code: &str,
    binding: InstallDiplomatBindingRecord,
    interaction: Option<TelegramDiplomatInteractionRecord>,
    request: &TelegramDiplomatIngressRequest,
    target_session_id: Option<SessionId>,
) -> TelegramDiplomatIngressPlan {
    TelegramDiplomatIngressPlan {
        decision,
        reason_code: reason_code.to_string(),
        target_session_id,
        crew_correlation_id: interaction
            .as_ref()
            .map(|record| record.crew_correlation_id.clone()),
        interaction,
        binding,
        sender: request.sender.clone(),
        reply_to_external_message_id: request.reply_to_external_message_id.clone(),
    }
}

fn terminal_decision(
    reason: TelegramDiplomatInteractionTerminalReason,
) -> TelegramDiplomatIngressDecision {
    match reason {
        TelegramDiplomatInteractionTerminalReason::BotPairRateLimited => {
            TelegramDiplomatIngressDecision::RateLimited
        }
        TelegramDiplomatInteractionTerminalReason::BindingUnavailable => {
            TelegramDiplomatIngressDecision::BindingUnavailable
        }
        _ => TelegramDiplomatIngressDecision::LoopTerminated,
    }
}

fn terminal_reason_code(reason: TelegramDiplomatInteractionTerminalReason) -> &'static str {
    match reason {
        TelegramDiplomatInteractionTerminalReason::DepthExceeded => {
            "telegram_bot_loop_depth_exceeded"
        }
        TelegramDiplomatInteractionTerminalReason::MessageBudgetExceeded => {
            "telegram_bot_message_budget_exceeded"
        }
        TelegramDiplomatInteractionTerminalReason::InteractionExpired => {
            "telegram_bot_interaction_expired"
        }
        TelegramDiplomatInteractionTerminalReason::BotPairRateLimited => {
            "telegram_bot_pair_rate_limited"
        }
        TelegramDiplomatInteractionTerminalReason::BindingUnavailable => {
            "telegram_diplomat_binding_unavailable"
        }
    }
}

fn normalize_bot_username(value: &str) -> String {
    value.trim().trim_start_matches('@').to_ascii_lowercase()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}
