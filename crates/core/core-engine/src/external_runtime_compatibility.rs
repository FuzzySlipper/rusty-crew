//! Rust-owned classification for external runtime compatibility probes.

use rusty_crew_core_protocol::{
    ExternalRuntimeCompatibilityProbeOutcome, ExternalRuntimeCompatibilityProbeReport,
    ExternalRuntimeCompatibilityState, ExternalRuntimeDesiredState,
};

pub(super) struct ProbeClassification {
    pub compatibility_state: ExternalRuntimeCompatibilityState,
    pub reason_code: Option<String>,
    pub retryable: bool,
}

pub(super) fn classify_probe(
    desired_state: ExternalRuntimeDesiredState,
    report: &ExternalRuntimeCompatibilityProbeReport,
    has_active_certification: bool,
) -> ProbeClassification {
    let compatibility_state = match report.outcome {
        ExternalRuntimeCompatibilityProbeOutcome::Passed if has_active_certification => {
            ExternalRuntimeCompatibilityState::Certified
        }
        ExternalRuntimeCompatibilityProbeOutcome::Passed => {
            ExternalRuntimeCompatibilityState::CompatibleUncertified
        }
        ExternalRuntimeCompatibilityProbeOutcome::TransportRetryable => {
            ExternalRuntimeCompatibilityState::Unassessed
        }
        ExternalRuntimeCompatibilityProbeOutcome::Incompatible => {
            ExternalRuntimeCompatibilityState::Incompatible
        }
    };
    let probe_reason = report
        .steps
        .iter()
        .find_map(|step| step.reason_code.as_deref());
    let reason_code = if desired_state != ExternalRuntimeDesiredState::Enabled {
        Some("external_runtime_disabled")
    } else {
        match report.outcome {
            ExternalRuntimeCompatibilityProbeOutcome::Passed => None,
            ExternalRuntimeCompatibilityProbeOutcome::TransportRetryable => {
                Some(probe_reason.unwrap_or("external_runtime_probe_transport_retryable"))
            }
            ExternalRuntimeCompatibilityProbeOutcome::Incompatible => {
                Some(probe_reason.unwrap_or("external_runtime_required_probe_failed"))
            }
        }
    };
    ProbeClassification {
        compatibility_state,
        reason_code: reason_code.map(str::to_owned),
        retryable: desired_state == ExternalRuntimeDesiredState::Enabled
            && report.outcome == ExternalRuntimeCompatibilityProbeOutcome::TransportRetryable,
    }
}
