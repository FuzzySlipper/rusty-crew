import type { ContextUsageEstimate } from "./context-estimate.js";
import type { ContextStrategyPolicy } from "./context-strategy.js";

export type ContextCompactionDecisionStatus =
  | "disabled"
  | "below_threshold"
  | "request_compaction"
  | "duplicate_window";

export type ContextCompactionAttemptStatus =
  | "requested"
  | "running"
  | "completed"
  | "failed";

export interface ContextCompactionAttemptRef {
  windowKey: string;
  strategyId: string;
  status: ContextCompactionAttemptStatus;
  observedAt?: string;
}

export interface ContextCompactionTriggerInput {
  policy: ContextStrategyPolicy;
  estimate: ContextUsageEstimate;
  windowKey: string;
  lastAttempt?: ContextCompactionAttemptRef;
}

export interface ContextCompactionTriggerDecision {
  status: ContextCompactionDecisionStatus;
  strategyId: string;
  fillPercent?: number;
  compactAtPercent: number;
  targetPercentAfterCompaction: number;
  windowKey: string;
  reasonCode: string;
}

export function evaluateContextCompactionTrigger(
  input: ContextCompactionTriggerInput,
): ContextCompactionTriggerDecision {
  const base = {
    strategyId: input.policy.strategyId,
    compactAtPercent: input.policy.compactAtPercent,
    targetPercentAfterCompaction: input.policy.targetPercentAfterCompaction,
    windowKey: input.windowKey,
  };
  if (!input.policy.enabled || !input.policy.autoCompactionEnabled) {
    return {
      ...base,
      status: "disabled",
      reasonCode: "context_compaction_disabled",
    };
  }
  if (
    input.lastAttempt?.windowKey === input.windowKey &&
    input.lastAttempt.strategyId === input.policy.strategyId &&
    input.lastAttempt.status !== "failed"
  ) {
    return {
      ...base,
      status: "duplicate_window",
      reasonCode: "context_compaction_already_attempted_for_window",
    };
  }
  const fillPercent = contextFillPercent(input.estimate);
  if (
    fillPercent === undefined ||
    fillPercent < input.policy.compactAtPercent
  ) {
    return {
      ...base,
      status: "below_threshold",
      fillPercent,
      reasonCode:
        fillPercent === undefined
          ? "context_fill_unavailable"
          : "context_fill_below_threshold",
    };
  }
  return {
    ...base,
    status: "request_compaction",
    fillPercent,
    reasonCode: "context_fill_threshold_exceeded",
  };
}

export function contextFillPercent(
  estimate: Pick<ContextUsageEstimate, "estimatedPromptTokens" | "budget">,
): number | undefined {
  const used = estimate.estimatedPromptTokens;
  const denominator =
    estimate.budget.usableInputTokens ?? estimate.budget.contextWindowTokens;
  if (
    used === undefined ||
    denominator === undefined ||
    denominator <= 0 ||
    !Number.isFinite(used) ||
    !Number.isFinite(denominator)
  ) {
    return undefined;
  }
  return Math.max(0, Math.min(100, Math.ceil((used / denominator) * 100)));
}
