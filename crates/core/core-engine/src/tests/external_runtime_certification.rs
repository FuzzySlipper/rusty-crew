use super::external_runtime::{probe_report, runtime};
use super::*;
use rusty_crew_core_protocol::{
    ExternalControllerContext, ExternalControllerLease, ExternalRuntimeCertificationInvalidation,
    ExternalRuntimeCertificationRequest, ExternalRuntimeCertificationStatus,
    ExternalRuntimeCompatibilityProbeOutcome, ExternalRuntimeCompatibilityState,
    ExternalRuntimeHandshakeObservation, ExternalRuntimeId,
};

#[test]
fn certification_requires_handshake_evidence_and_survives_restart() {
    let data_dir = unique_data_dir("external-runtime-certification");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine.register_external_runtime(&runtime(), None).unwrap();
    let request = ExternalRuntimeCertificationRequest {
        certification_id: "cert-1".into(),
        idempotency_key: "cert-key-1".into(),
        runtime_id: ExternalRuntimeId::new("codex-local"),
        evidence_summary: "focused and live compatibility gates passed".into(),
        requested_at: "2026-06-19T00:00:02Z".into(),
    };
    assert!(engine.certify_external_runtime(&request).is_err());

    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "controller-cert".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T00:10:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    engine
        .authorize_external_runtime_handshake(&ExternalRuntimeHandshakeObservation {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            controller: ExternalControllerContext {
                holder_instance_id: "controller-cert".into(),
                generation: lease.generation,
            },
            cli_version: "0.144.3".into(),
            consumed_contract_revision: "contract-v1".into(),
            probe_report: probe_report(ExternalRuntimeCompatibilityProbeOutcome::Passed, None),
            observed_at: "2026-06-19T00:00:01Z".into(),
        })
        .unwrap();

    let first = engine.certify_external_runtime(&request).unwrap();
    assert_eq!(first.status, ExternalRuntimeCertificationStatus::Active);
    assert_eq!(first.revision, 1);
    assert_eq!(engine.certify_external_runtime(&request).unwrap(), first);
    assert_eq!(
        engine
            .get_external_runtime(&ExternalRuntimeId::new("codex-local"))
            .unwrap()
            .unwrap()
            .compatibility_state,
        ExternalRuntimeCompatibilityState::Certified
    );

    let second = engine
        .certify_external_runtime(&ExternalRuntimeCertificationRequest {
            certification_id: "cert-2".into(),
            idempotency_key: "cert-key-2".into(),
            runtime_id: ExternalRuntimeId::new("codex-local"),
            evidence_summary: "repeat certification passed".into(),
            requested_at: "2026-06-19T00:00:03Z".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .get_external_runtime_certification("cert-1")
            .unwrap()
            .unwrap()
            .status,
        ExternalRuntimeCertificationStatus::Superseded
    );
    engine
        .invalidate_external_runtime_certification(&ExternalRuntimeCertificationInvalidation {
            certification_id: second.certification_id,
            expected_revision: second.revision,
            reason: "superseded by a newer Crew contract".into(),
            invalidated_at: "2026-06-19T00:00:04Z".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .get_external_runtime(&ExternalRuntimeId::new("codex-local"))
            .unwrap()
            .unwrap()
            .compatibility_state,
        ExternalRuntimeCompatibilityState::CompatibleUncertified
    );
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    let records = restarted.list_external_runtime_certifications().unwrap();
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|record| {
        record.certification_id == "cert-2"
            && record.status == ExternalRuntimeCertificationStatus::Invalidated
    }));
}
