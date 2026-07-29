import type {
  CoreEvent,
  LogicalTurnLifecycleEvent,
  RunId,
  SessionId,
} from "@rusty-crew/contracts";

type DelegationLifecycleEvent = Extract<
  CoreEvent,
  { type: "delegation_lifecycle_observed" }
>["lifecycle"];

export const toNativeDelegationLifecycleEvent = (
  lifecycle: DelegationLifecycleEvent,
): RawDelegationLifecycleEvent => ({
  parent_session_id: lifecycle.parentSessionId,
  delegated_session_id: lifecycle.delegatedSessionId,
  run_id: lifecycle.runId,
  phase: lifecycle.phase,
  detail: lifecycle.detail,
});

export function toDelegationLifecycleEvent(
  lifecycle: RawDelegationLifecycleEvent,
): DelegationLifecycleEvent {
  return {
    parentSessionId: lifecycle.parent_session_id,
    delegatedSessionId: lifecycle.delegated_session_id,
    runId: lifecycle.run_id,
    phase: lifecycle.phase,
    detail: lifecycle.detail,
  };
}

export interface RawDelegationLifecycleEvent {
  parent_session_id: SessionId;
  delegated_session_id: SessionId;
  run_id?: RunId;
  phase: DelegationLifecycleEvent["phase"];
  detail?: string | null;
}

// Logical-turn lifecycle payloads deliberately use the generated camelCase
// contract on both sides of the native boundary.
export type RawLogicalTurnLifecycleEvent = LogicalTurnLifecycleEvent;
