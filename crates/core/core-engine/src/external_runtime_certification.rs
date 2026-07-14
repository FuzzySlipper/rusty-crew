//! Rust-owned certification workflow for externally managed agent runtimes.

use super::*;
use rusty_crew_core_protocol::{
    validate_external_runtime_certification_invalidation,
    validate_external_runtime_certification_request, ExternalRuntimeCertificationInvalidation,
    ExternalRuntimeCertificationRecord, ExternalRuntimeCertificationRequest,
    ExternalRuntimeCertificationStatus, ExternalRuntimeCompatibilityProbeOutcome,
    ExternalRuntimeCompatibilityState, ExternalRuntimeHandshakeObservation, ExternalRuntimeKind,
    ExternalRuntimeObservedState, ExternalRuntimeProbeEvidenceRecord,
};

impl CoreEngine {
    pub(crate) fn record_external_runtime_probe_evidence(
        &self,
        observation: &ExternalRuntimeHandshakeObservation,
        runtime_kind: ExternalRuntimeKind,
    ) -> CoreResult<()> {
        if observation.probe_report.outcome != ExternalRuntimeCompatibilityProbeOutcome::Passed {
            return Ok(());
        }
        self.store
            .put_external_runtime_probe_evidence(&ExternalRuntimeProbeEvidenceRecord {
                runtime_id: observation.runtime_id.clone(),
                runtime_kind,
                observed_cli_version: observation.cli_version.clone(),
                consumed_contract_revision: observation.consumed_contract_revision.clone(),
                probe_report: observation.probe_report.clone(),
                observed_at: observation.observed_at.clone(),
            })
    }

    pub fn list_external_runtime_certifications(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeCertificationRecord>> {
        self.store.list_external_runtime_certifications()
    }

    pub fn get_external_runtime_certification(
        &self,
        certification_id: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        self.store
            .get_external_runtime_certification(certification_id)
    }

    pub fn certify_external_runtime(
        &self,
        request: &ExternalRuntimeCertificationRequest,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        validate_external_runtime_certification_request(request)?;
        let runtime = self
            .store
            .get_external_runtime_registration(&request.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external runtime was not found")
            })?;
        let cli_version = runtime.observed_cli_version.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::ActionRejected,
                "external runtime has no observed compatibility identity",
            )
        })?;
        let contract_revision = runtime.consumed_contract_revision.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::ActionRejected,
                "external runtime has no consumed contract revision",
            )
        })?;
        let report = runtime.last_compatibility_probe.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::ActionRejected,
                "external runtime has no compatibility probe evidence",
            )
        })?;
        let evidence = self
            .store
            .get_external_runtime_probe_evidence(&runtime.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external runtime has no Rust-recorded handshake evidence",
                )
            })?;
        let evidence_matches = evidence.runtime_kind == runtime.kind
            && evidence.observed_cli_version == *cli_version
            && evidence.consumed_contract_revision == *contract_revision
            && evidence.probe_report == *report;
        if report.outcome != ExternalRuntimeCompatibilityProbeOutcome::Passed
            || !evidence_matches
            || !matches!(
                runtime.compatibility_state,
                ExternalRuntimeCompatibilityState::CompatibleUncertified
                    | ExternalRuntimeCompatibilityState::Certified
            )
            || runtime.observed_state != ExternalRuntimeObservedState::Ready
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "only a ready runtime with a passing compatibility probe can be certified",
            ));
        }
        let record = ExternalRuntimeCertificationRecord {
            certification_id: request.certification_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
            certified_runtime_id: runtime.runtime_id.clone(),
            runtime_kind: runtime.kind,
            observed_cli_version: cli_version.clone(),
            consumed_contract_revision: contract_revision.clone(),
            probe_suite_revision: report.suite_revision.clone(),
            evidence_summary: request.evidence_summary.clone(),
            status: ExternalRuntimeCertificationStatus::Active,
            superseded_by_certification_id: None,
            invalidated_at: None,
            invalidation_reason: None,
            revision: 0,
            created_at: request.requested_at.clone(),
            updated_at: request.requested_at.clone(),
        };
        let saved = self.store.record_external_runtime_certification(&record)?;
        if runtime.compatibility_state != ExternalRuntimeCompatibilityState::Certified {
            let mut next = runtime.clone();
            next.compatibility_state = ExternalRuntimeCompatibilityState::Certified;
            next.updated_at = request.requested_at.clone();
            self.store
                .put_external_runtime_registration(&next, Some(runtime.revision))?;
        }
        Ok(saved)
    }

    pub fn invalidate_external_runtime_certification(
        &self,
        invalidation: &ExternalRuntimeCertificationInvalidation,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        validate_external_runtime_certification_invalidation(invalidation)?;
        let invalidated = self
            .store
            .invalidate_external_runtime_certification(invalidation)?;
        for runtime in self.store.list_external_runtime_registrations()? {
            let exact_identity = runtime.kind == invalidated.runtime_kind
                && runtime.observed_cli_version.as_deref()
                    == Some(invalidated.observed_cli_version.as_str())
                && runtime.consumed_contract_revision.as_deref()
                    == Some(invalidated.consumed_contract_revision.as_str())
                && runtime
                    .last_compatibility_probe
                    .as_ref()
                    .map(|report| report.suite_revision.as_str())
                    == Some(invalidated.probe_suite_revision.as_str());
            if exact_identity
                && runtime.compatibility_state == ExternalRuntimeCompatibilityState::Certified
            {
                let mut next = runtime.clone();
                next.compatibility_state = ExternalRuntimeCompatibilityState::CompatibleUncertified;
                next.updated_at = invalidation.invalidated_at.clone();
                self.store
                    .put_external_runtime_registration(&next, Some(runtime.revision))?;
            }
        }
        Ok(invalidated)
    }
}
