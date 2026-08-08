import type { ResourceLimits } from "@rusty-crew/contracts";

export interface RawSessionResourceLimits {
  max_duration_ms?: number;
  max_delegation_depth?: number;
}

export function toSessionResourceLimits(
  limits: RawSessionResourceLimits | undefined,
): ResourceLimits {
  return {
    maxDurationMs: limits?.max_duration_ms,
    maxDelegationDepth: limits?.max_delegation_depth,
  };
}
