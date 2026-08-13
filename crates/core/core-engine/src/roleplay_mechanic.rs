use super::*;
use rusty_crew_core_persistence::{
    RoleplayMechanicDiagnosticCreate, RoleplayMechanicDiagnosticOutcomeUpdate,
    RoleplayMechanicDiagnosticQuery, RoleplayMechanicDiagnosticRecord,
    RoleplayMechanicProposalStatus, RoleplayMechanicSessionAssociationCreate,
    RoleplayMechanicSessionAssociationQuery, RoleplayMechanicSessionAssociationRecord,
    RoleplayMechanicSessionAssociationWrite, RoleplayMechanicSessionAttachmentUpdate,
};

impl CoreEngine {
    pub fn create_roleplay_mechanic_session_association(
        &self,
        create: &RoleplayMechanicSessionAssociationCreate,
    ) -> CoreResult<RoleplayMechanicSessionAssociationRecord> {
        let mechanic = self.require_mechanic_session(&create.mechanic_session_id)?;
        let (roleplay_session_id, roleplay_profile_id) =
            self.resolve_roleplay_mechanic_target(create.roleplay_session_id.as_deref())?;
        self.store.put_roleplay_mechanic_session_association(
            &RoleplayMechanicSessionAssociationWrite {
                record: RoleplayMechanicSessionAssociationRecord {
                    mechanic_session_id: mechanic.session_id,
                    mechanic_profile_id: mechanic.profile_id,
                    roleplay_session_id,
                    roleplay_profile_id,
                    revision: 1,
                    created_at: create.now.clone(),
                    updated_at: create.now.clone(),
                },
                expected_revision: None,
            },
        )
    }

    pub fn get_roleplay_mechanic_session_association(
        &self,
        mechanic_session_id: &SessionId,
    ) -> CoreResult<Option<RoleplayMechanicSessionAssociationRecord>> {
        self.store
            .get_roleplay_mechanic_session_association(mechanic_session_id)
    }

    pub fn list_roleplay_mechanic_session_associations(
        &self,
        query: &RoleplayMechanicSessionAssociationQuery,
    ) -> CoreResult<Vec<RoleplayMechanicSessionAssociationRecord>> {
        self.store
            .list_roleplay_mechanic_session_associations(query)
    }

    pub fn update_roleplay_mechanic_session_attachment(
        &self,
        update: &RoleplayMechanicSessionAttachmentUpdate,
    ) -> CoreResult<RoleplayMechanicSessionAssociationRecord> {
        let mechanic = self.require_mechanic_session(&update.mechanic_session_id)?;
        let current = self
            .store
            .get_roleplay_mechanic_session_association(&update.mechanic_session_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "roleplay mechanic session association {} not found",
                        update.mechanic_session_id
                    ),
                )
            })?;
        if current.mechanic_profile_id != mechanic.profile_id {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "mechanic session profile no longer matches its roleplay association",
            ));
        }
        let (roleplay_session_id, roleplay_profile_id) =
            self.resolve_roleplay_mechanic_target(update.roleplay_session_id.as_deref())?;
        self.store.put_roleplay_mechanic_session_association(
            &RoleplayMechanicSessionAssociationWrite {
                record: RoleplayMechanicSessionAssociationRecord {
                    mechanic_session_id: current.mechanic_session_id,
                    mechanic_profile_id: current.mechanic_profile_id,
                    roleplay_session_id,
                    roleplay_profile_id,
                    revision: current.revision + 1,
                    created_at: current.created_at,
                    updated_at: update.now.clone(),
                },
                expected_revision: Some(update.expected_revision),
            },
        )
    }

    pub fn create_roleplay_mechanic_diagnostic(
        &self,
        create: &RoleplayMechanicDiagnosticCreate,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        let association = self
            .store
            .get_roleplay_mechanic_session_association(&create.mechanic_session_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "mechanic session {} has no roleplay association",
                        create.mechanic_session_id
                    ),
                )
            })?;
        if association.roleplay_session_id.as_deref() != Some(create.roleplay_session_id.as_str()) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "mechanic session {} is not attached to roleplay session {}",
                    create.mechanic_session_id, create.roleplay_session_id
                ),
            ));
        }
        let roleplay_profile_id = association.roleplay_profile_id.clone().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "attached mechanic association is missing roleplay profile",
            )
        })?;
        self.validate_roleplay_mechanic_diagnostic_proposals(
            &association,
            &create.proposal_ids,
            &create.applied_proposal_ids,
        )?;
        self.store
            .create_roleplay_mechanic_diagnostic(&RoleplayMechanicDiagnosticRecord {
                diagnostic_id: create.diagnostic_id.clone(),
                mechanic_session_id: association.mechanic_session_id,
                mechanic_profile_id: association.mechanic_profile_id,
                roleplay_session_id: create.roleplay_session_id.clone(),
                roleplay_profile_id,
                model_config_id: create.model_config_id.clone(),
                model_config_revision: create.model_config_revision,
                endpoint_id: create.endpoint_id.clone(),
                endpoint_revision: create.endpoint_revision,
                credential_id: create.credential_id.clone(),
                credential_revision: create.credential_revision,
                symptom: create.symptom.clone(),
                hypothesis: create.hypothesis.clone(),
                proposal_ids: create.proposal_ids.clone(),
                applied_proposal_ids: create.applied_proposal_ids.clone(),
                outcome: rusty_crew_core_persistence::RoleplayMechanicDiagnosticOutcome::Pending,
                notes: create.notes.clone(),
                revision: 1,
                created_at: create.now.clone(),
                updated_at: create.now.clone(),
            })
    }

    pub fn get_roleplay_mechanic_diagnostic(
        &self,
        diagnostic_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicDiagnosticRecord>> {
        self.store.get_roleplay_mechanic_diagnostic(diagnostic_id)
    }

    pub fn list_roleplay_mechanic_diagnostics(
        &self,
        query: &RoleplayMechanicDiagnosticQuery,
    ) -> CoreResult<Vec<RoleplayMechanicDiagnosticRecord>> {
        self.store.list_roleplay_mechanic_diagnostics(query)
    }

    pub fn update_roleplay_mechanic_diagnostic_outcome(
        &self,
        update: &RoleplayMechanicDiagnosticOutcomeUpdate,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        self.store
            .update_roleplay_mechanic_diagnostic_outcome(update)
    }

    fn resolve_roleplay_mechanic_target(
        &self,
        roleplay_session_id: Option<&str>,
    ) -> CoreResult<(Option<String>, Option<ProfileId>)> {
        let Some(roleplay_session_id) = roleplay_session_id else {
            return Ok((None, None));
        };
        let metadata = self
            .get_roleplay_session_metadata(roleplay_session_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("roleplay session {roleplay_session_id} not found"),
                )
            })?;
        if metadata.archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "mechanic sessions cannot attach to archived roleplay sessions",
            ));
        }
        let runtime = self.get_session(&SessionId::new(roleplay_session_id))?;
        if runtime.profile_id.0 != metadata.profile_id {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay session {roleplay_session_id} profile mismatch: runtime {}, metadata {}",
                    runtime.profile_id.0, metadata.profile_id
                ),
            ));
        }
        Ok((
            Some(roleplay_session_id.to_string()),
            Some(runtime.profile_id),
        ))
    }

    fn validate_roleplay_mechanic_diagnostic_proposals(
        &self,
        association: &RoleplayMechanicSessionAssociationRecord,
        proposal_ids: &[String],
        applied_proposal_ids: &[String],
    ) -> CoreResult<()> {
        for applied_proposal_id in applied_proposal_ids {
            if !proposal_ids.contains(applied_proposal_id) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!(
                        "applied roleplay mechanic proposal {applied_proposal_id} must also appear in proposal_ids"
                    ),
                ));
            }
        }
        for proposal_id in proposal_ids {
            let proposal = self
                .store
                .get_roleplay_mechanic_proposal(proposal_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::NotFound,
                        format!("roleplay mechanic proposal {proposal_id} not found"),
                    )
                })?;
            if proposal.mechanic_session_id != association.mechanic_session_id
                || Some(proposal.roleplay_session_id.as_str())
                    != association.roleplay_session_id.as_deref()
                || Some(&proposal.profile_id) != association.roleplay_profile_id.as_ref()
            {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "roleplay mechanic proposal {proposal_id} does not belong to this mechanic association"
                    ),
                ));
            }
            if applied_proposal_ids.contains(proposal_id)
                && proposal.status != RoleplayMechanicProposalStatus::Applied
            {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!("roleplay mechanic proposal {proposal_id} has not been applied"),
                ));
            }
        }
        Ok(())
    }
}
