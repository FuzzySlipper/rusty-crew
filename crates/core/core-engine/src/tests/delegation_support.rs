use super::*;

pub(super) fn spawn_delegated(
    engine: &CoreEngine,
    planner: &SessionState,
    wake_id: &str,
    max_duration_ms: Option<u32>,
) -> SessionId {
    engine
        .execute_brain_actions(BrainActionBatch {
            wake_id: wake_id.to_string(),
            session_id: planner.session_id.clone(),
            actions: vec![BrainAction::RequestDelegation {
                profile_id: ProfileId::new("coder-profile"),
                task_id: None,
                prompt: "complete a delegated lifecycle slice".to_string(),
                expected_output: None,
                resource_limits: Some(ResourceLimits {
                    workdir: Some("/home/dev/rusty-crew".to_string()),
                    max_duration_ms,
                    max_delegation_depth: Some(0),
                }),
                timeout_ms: max_duration_ms,
                priority: None,
                fan_out_group_id: None,
                fan_out_max_concurrency: None,
                fan_out_failure_policy: None,
                correlation_id: None,
                parent_consumption: None,
                capacity_request: None,
            }],
        })
        .unwrap();
    delegated_session_id(&planner.session_id, wake_id, 0)
}

pub(super) fn fan_out_request(
    index: u32,
    group_id: &str,
    max_concurrency: Option<u32>,
    failure_policy: FanOutFailurePolicy,
) -> BrainAction {
    BrainAction::RequestDelegation {
        profile_id: ProfileId::new(format!("coder-profile-{index}")),
        task_id: Some(rusty_crew_core_protocol::TaskId::new(format!(
            "fan-out-{index}"
        ))),
        prompt: format!("complete fan-out slice {index}"),
        expected_output: Some("completion packet".to_string()),
        resource_limits: Some(ResourceLimits {
            workdir: Some("/home/dev/rusty-crew".to_string()),
            max_duration_ms: Some(30_000),
            max_delegation_depth: Some(0),
        }),
        timeout_ms: Some(30_000),
        priority: None,
        fan_out_group_id: Some(group_id.to_string()),
        fan_out_max_concurrency: max_concurrency,
        fan_out_failure_policy: Some(failure_policy),
        correlation_id: Some(format!("{group_id}:{index}")),
        parent_consumption: Some(ParentConsumptionPolicy::AwaitCompletion),
        capacity_request: None,
    }
}

pub(super) fn deliver_child_completion(
    engine: &CoreEngine,
    parent_session_id: &SessionId,
    parent_wake_id: &str,
    child_index: usize,
    status: CompletionStatus,
) {
    let child_session_id = delegated_session_id(parent_session_id, parent_wake_id, child_index);
    engine
        .execute_brain_actions(BrainActionBatch {
            wake_id: format!("child-wake-{child_index}"),
            session_id: child_session_id.clone(),
            actions: vec![BrainAction::DeliverCompletion {
                packet: CompletionPacket {
                    session_id: child_session_id,
                    summary: format!("fan-out child {child_index} {status:?}"),
                    status,
                },
            }],
        })
        .unwrap();
}
