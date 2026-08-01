use super::*;
use rusty_crew_core_bridge_api::{
    runtime_dispatch_activity_id, runtime_provider_activity_id, runtime_wake_activity_id,
};
use std::collections::{HashMap, HashSet, VecDeque};

impl NativeBridge {
    pub(crate) fn begin_runtime_activity(
        &self,
        input: RuntimeActivityBegin,
    ) -> CoreResult<RuntimeActivityRecord> {
        self.engine()?.begin_runtime_activity(input)
    }

    pub(crate) fn progress_runtime_activity(
        &self,
        input: RuntimeActivityProgress,
    ) -> CoreResult<RuntimeActivityRecord> {
        self.engine()?.progress_runtime_activity(input)
    }

    pub(crate) fn finish_runtime_activity(
        &self,
        input: RuntimeActivityFinish,
    ) -> CoreResult<RuntimeActivityRecord> {
        self.engine()?.finish_runtime_activity(input)
    }

    pub(crate) fn settle_runtime_activity_wake(
        &self,
        input: RuntimeActivityWakeSettlement,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        self.engine()?.settle_runtime_activity_wake(input)
    }

    pub(crate) fn runtime_activity_census(
        &self,
        mut query: RuntimeActivityCensusQuery,
    ) -> CoreResult<RuntimeActivityCensus> {
        let engine = self.engine()?;
        let diagnostics = self
            .buffered_brain_run_diagnostics()
            .map_err(brain_runtime_error_to_core)?;
        for run in diagnostics.runs.into_iter().filter(|run| !run.terminal) {
            let session_id = Some(SessionId::new(run.session_id));
            let agent_id = run.agent_id.map(rusty_crew_core_bridge_api::AgentId::new);
            let profile_id = run.profile_id.map(ProfileId::new);
            let wake_activity_id = runtime_wake_activity_id(&run.wake_id);
            query.live_evidence.push(RuntimeActivityLiveEvidence {
                activity_id: wake_activity_id.clone(),
                parent_activity_id: Some(runtime_dispatch_activity_id(&run.wake_id)),
                kind: RuntimeActivityKind::Wake,
                owner: RuntimeActivityOwner::RustBrain,
                agent_id: agent_id.clone(),
                profile_id: profile_id.clone(),
                session_id: session_id.clone(),
                wake_id: Some(run.wake_id.clone()),
                phase: run.phase.clone(),
                summary: Some(format!("{} buffered brain wake", run.module_label)),
                process_id: None,
                started_at: run.started_at.clone(),
                last_progress_at: run.last_transition_at.clone(),
            });
            query.live_evidence.push(RuntimeActivityLiveEvidence {
                activity_id: runtime_provider_activity_id(&run.wake_id),
                parent_activity_id: Some(wake_activity_id),
                kind: RuntimeActivityKind::ProviderRequest,
                owner: RuntimeActivityOwner::RustBrain,
                agent_id,
                profile_id,
                session_id,
                wake_id: Some(run.wake_id),
                phase: run.phase,
                summary: Some(format!("{} provider loop", run.module_label)),
                process_id: None,
                started_at: run.started_at,
                last_progress_at: run.last_transition_at,
            });
        }
        query
            .live_evidence
            .extend(linux_descendant_process_evidence(&engine.diagnostic_now()));
        engine.runtime_activity_census(query)
    }
}

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn begin_runtime_activity_json(&self, input_json: String) -> napi::Result<String> {
        let input = serde_json::from_str::<RuntimeActivityBegin>(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid runtime activity begin JSON: {error}"),
            )
        })?;
        let bridge = self.bridge()?;
        serialize_runtime_activity_result(
            "runtime activity begin",
            bridge
                .begin_runtime_activity(input)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn progress_runtime_activity_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            serde_json::from_str::<RuntimeActivityProgress>(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime activity progress JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        serialize_runtime_activity_result(
            "runtime activity progress",
            bridge
                .progress_runtime_activity(input)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn finish_runtime_activity_json(&self, input_json: String) -> napi::Result<String> {
        let input =
            serde_json::from_str::<RuntimeActivityFinish>(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime activity finish JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        serialize_runtime_activity_result(
            "runtime activity finish",
            bridge
                .finish_runtime_activity(input)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn settle_runtime_activity_wake_json(&self, input_json: String) -> napi::Result<String> {
        let input = serde_json::from_str::<RuntimeActivityWakeSettlement>(&input_json).map_err(
            |error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime activity wake settlement JSON: {error}"),
                )
            },
        )?;
        let bridge = self.bridge()?;
        serialize_runtime_activity_result(
            "runtime activity wake settlement",
            bridge
                .settle_runtime_activity_wake(input)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn runtime_activity_census_json(&self, input_json: String) -> napi::Result<String> {
        let mut query =
            serde_json::from_str::<RuntimeActivityCensusQuery>(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime activity census JSON: {error}"),
                )
            })?;
        let defaults = RuntimeActivityCensusQuery::default();
        query.stall_after_ms = query.stall_after_ms.or(defaults.stall_after_ms);
        query.recent_abnormal_limit = query
            .recent_abnormal_limit
            .or(defaults.recent_abnormal_limit);
        let bridge = self.bridge()?;
        serialize_runtime_activity_result(
            "runtime activity census",
            bridge
                .runtime_activity_census(query)
                .map_err(to_napi_error)?,
        )
    }
}

fn serialize_runtime_activity_result(
    label: &str,
    value: impl serde::Serialize,
) -> napi::Result<String> {
    serde_json::to_string(&value).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize {label}: {error}"),
        )
    })
}

#[cfg(target_os = "linux")]
fn linux_descendant_process_evidence(now: &str) -> Vec<RuntimeActivityLiveEvidence> {
    const MAX_PROC_SCAN: usize = 8_192;
    const MAX_DESCENDANTS: usize = 512;

    let mut children = HashMap::<u32, Vec<u32>>::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in entries.flatten().take(MAX_PROC_SCAN) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(after_name) = stat.rsplit_once(") ").map(|(_, remainder)| remainder) else {
            continue;
        };
        let Some(parent_pid) = after_name
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        children.entry(parent_pid).or_default().push(pid);
    }

    let current_pid = std::process::id();
    let mut queue = VecDeque::from([current_pid]);
    let mut seen = HashSet::from([current_pid]);
    let mut descendants = Vec::new();
    while let Some(parent_pid) = queue.pop_front() {
        for pid in children.get(&parent_pid).into_iter().flatten() {
            if !seen.insert(*pid) {
                continue;
            }
            descendants.push((*pid, parent_pid));
            if descendants.len() >= MAX_DESCENDANTS {
                break;
            }
            queue.push_back(*pid);
        }
        if descendants.len() >= MAX_DESCENDANTS {
            break;
        }
    }

    descendants
        .into_iter()
        .map(|(pid, parent_pid)| {
            let process_name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .map(|value| value.trim().chars().take(128).collect::<String>())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "service descendant".into());
            RuntimeActivityLiveEvidence {
                activity_id: RuntimeActivityId::new(format!("process:{pid}")),
                parent_activity_id: (parent_pid != current_pid)
                    .then(|| RuntimeActivityId::new(format!("process:{parent_pid}"))),
                kind: RuntimeActivityKind::Subprocess,
                owner: RuntimeActivityOwner::TypeScriptHost,
                agent_id: None,
                profile_id: None,
                session_id: None,
                wake_id: None,
                phase: "running".into(),
                summary: Some(process_name),
                process_id: Some(pid),
                started_at: now.into(),
                last_progress_at: now.into(),
            }
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn linux_descendant_process_evidence(_now: &str) -> Vec<RuntimeActivityLiveEvidence> {
    Vec::new()
}
