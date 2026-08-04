use super::*;

impl CoreEngine {
    pub fn provider_state_for_wake(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
    ) -> CoreResult<ProviderStateHydration> {
        let Some(strategy) = &registration.strategy else {
            return Ok(ProviderStateHydration {
                state: None,
                absence_reason: Some(ProviderStateAbsenceReason::NotConfigured),
            });
        };
        match strategy.provider_state.mode {
            ProviderStateMode::Unused => {
                return Ok(ProviderStateHydration {
                    state: None,
                    absence_reason: Some(ProviderStateAbsenceReason::ModuleDoesNotUseState),
                });
            }
            ProviderStateMode::Optional | ProviderStateMode::Required => {}
        }
        let Some(scope) = &registration.provider_state_scope else {
            return self.provider_state_unavailable_for_mode(
                strategy.provider_state.mode.clone(),
                ProviderStateAbsenceReason::NotConfigured,
            );
        };
        let key = provider_wire_state_key(session_id, &strategy.module_id, &strategy.strategy_id);
        let lookup = ProviderWireStateWakeLookup {
            key,
            profile_fingerprint: scope.profile_fingerprint.clone(),
            provider_fingerprint: scope.provider_fingerprint.clone(),
            now: self.now(),
        };
        let loaded = match load_provider_state_for_wake(&self.store, &lookup) {
            Ok(loaded) => loaded,
            Err(error) => {
                if strategy.provider_state.mode == ProviderStateMode::Optional {
                    return Ok(ProviderStateHydration {
                        state: None,
                        absence_reason: Some(ProviderStateAbsenceReason::LoadFailed),
                    });
                }
                return Err(error);
            }
        };
        let Some(record) = loaded.record else {
            return self.provider_state_unavailable_for_mode(
                strategy.provider_state.mode.clone(),
                loaded
                    .absence_reason
                    .unwrap_or(ProviderStateAbsenceReason::Missing),
            );
        };
        Ok(ProviderStateHydration {
            state: Some(BrainWakeProviderStateInput {
                module_id: record.key.module_id,
                strategy_id: record.key.strategy_id,
                profile_fingerprint: record.profile_fingerprint,
                provider_fingerprint: record.provider_fingerprint,
                payload_version: record.payload_version,
                payload: record.payload_json,
                expires_at: record.expires_at,
            }),
            absence_reason: None,
        })
    }

    pub fn apply_provider_state_output(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        wake_id: &str,
        output: BrainWakeProviderStateOutput,
    ) -> CoreResult<()> {
        match output {
            BrainWakeProviderStateOutput::Unchanged => Ok(()),
            BrainWakeProviderStateOutput::Replace { state } => {
                self.replace_provider_state(registration, session_id, wake_id, state)
            }
            BrainWakeProviderStateOutput::Clear { reason } => {
                self.clear_provider_state(registration, session_id, reason)
            }
        }
    }

    pub fn provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        list_provider_state_store_diagnostics(&self.store, limit)
    }

    fn provider_state_unavailable_for_mode(
        &self,
        mode: ProviderStateMode,
        absence_reason: ProviderStateAbsenceReason,
    ) -> CoreResult<ProviderStateHydration> {
        if mode == ProviderStateMode::Required {
            return Err(CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!("required provider state unavailable: {absence_reason:?}"),
            ));
        }
        Ok(ProviderStateHydration {
            state: None,
            absence_reason: Some(absence_reason),
        })
    }

    fn replace_provider_state(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        wake_id: &str,
        state: BrainWakeProviderStateUpdate,
    ) -> CoreResult<()> {
        let (module_id, strategy_id) = provider_state_registration_key(registration)?;
        if state.module_id != module_id || state.strategy_id != strategy_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "provider state update targeted {}/{}, registered brain uses {}/{}",
                    state.module_id, state.strategy_id, module_id, strategy_id
                ),
            ));
        }
        if let Some(scope) = &registration.provider_state_scope {
            validate_provider_state_update_scope(&state, scope)?;
        }
        let ttl_ms = state
            .ttl_ms
            .unwrap_or(DEFAULT_PROVIDER_WIRE_STATE_TTL_MS)
            .min(MAX_PROVIDER_WIRE_STATE_TTL_MS);
        let now = self.now();
        let expires_at = add_millis_to_iso(&now, ttl_ms)?;
        save_provider_state_store(
            &self.store,
            &ProviderWireStateWrite {
                key: provider_wire_state_key(session_id, &module_id, &strategy_id),
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload_json: state.payload,
                now,
                expires_at: Some(expires_at),
                last_wake_id: Some(wake_id.to_string()),
            },
        )?;
        Ok(())
    }

    fn clear_provider_state(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        reason: ProviderStateClearReason,
    ) -> CoreResult<()> {
        let (module_id, strategy_id) = provider_state_registration_key(registration)?;
        let invalidation_reason = match reason {
            ProviderStateClearReason::BrainRequestedClear => {
                ProviderWireStateInvalidationReason::BrainRequestedClear
            }
            ProviderStateClearReason::OperatorRequestedClear => {
                ProviderWireStateInvalidationReason::OperatorRequestedClear
            }
        };
        clear_provider_state_store(
            &self.store,
            &provider_wire_state_key(session_id, &module_id, &strategy_id),
            &self.now(),
            invalidation_reason,
        )?;
        Ok(())
    }
}

fn provider_wire_state_key(
    session_id: &SessionId,
    module_id: &str,
    strategy_id: &str,
) -> ProviderWireStateKey {
    ProviderWireStateKey {
        session_id: session_id.clone(),
        module_id: module_id.to_string(),
        strategy_id: strategy_id.to_string(),
    }
}

fn provider_state_registration_key(
    registration: &BrainImplementationRegistration,
) -> CoreResult<(String, String)> {
    let Some(strategy) = &registration.strategy else {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain registration has no provider-state strategy metadata",
        ));
    };
    if strategy.provider_state.mode == ProviderStateMode::Unused {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain registration does not use provider state",
        ));
    }
    Ok((strategy.module_id.clone(), strategy.strategy_id.clone()))
}

fn validate_provider_state_update_scope(
    state: &BrainWakeProviderStateUpdate,
    scope: &BrainProviderStateScope,
) -> CoreResult<()> {
    if state.profile_fingerprint != scope.profile_fingerprint {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "provider state update profile fingerprint does not match registered scope",
        ));
    }
    if state.provider_fingerprint != scope.provider_fingerprint {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "provider state update provider fingerprint does not match registered scope",
        ));
    }
    Ok(())
}
