import assert from "node:assert/strict";
import {
  contextFillPercent,
  defaultContextStrategyPolicy,
  evaluateContextCompactionTrigger,
  type ContextUsageEstimate,
} from "./index.js";

const estimate: ContextUsageEstimate = {
  estimateQuality: "approximate",
  estimateMethod: "fixture",
  estimatorId: "fixture_estimator",
  estimatedPromptTokens: 85,
  estimatedRemainingTokens: 15,
  budget: {
    contextWindowTokens: 100,
    maxOutputTokens: 10,
    reservedResponseTokens: 10,
    safetyMarginTokens: 0,
    usableInputTokens: 100,
  },
  sampledEventCount: 4,
  sampledMessageCount: 2,
};
assert.equal(contextFillPercent(estimate), 85);

const disabled = evaluateContextCompactionTrigger({
  policy: {
    ...defaultContextStrategyPolicy(),
    strategyId: "rolling_summary_compaction",
    autoCompactionEnabled: false,
  },
  estimate,
  windowKey: "session:branch:head-1",
});
assert.equal(disabled.status, "disabled");

const below = evaluateContextCompactionTrigger({
  policy: {
    ...defaultContextStrategyPolicy(),
    strategyId: "rolling_summary_compaction",
    autoCompactionEnabled: true,
    compactAtPercent: 90,
    targetPercentAfterCompaction: 50,
  },
  estimate,
  windowKey: "session:branch:head-1",
});
assert.equal(below.status, "below_threshold");
assert.equal(below.fillPercent, 85);

const request = evaluateContextCompactionTrigger({
  policy: {
    ...defaultContextStrategyPolicy(),
    strategyId: "rolling_summary_compaction",
    autoCompactionEnabled: true,
    compactAtPercent: 80,
    targetPercentAfterCompaction: 50,
  },
  estimate,
  windowKey: "session:branch:head-1",
});
assert.equal(request.status, "request_compaction");
assert.equal(request.reasonCode, "context_fill_threshold_exceeded");

const duplicate = evaluateContextCompactionTrigger({
  policy: {
    ...defaultContextStrategyPolicy(),
    strategyId: "rolling_summary_compaction",
    autoCompactionEnabled: true,
    compactAtPercent: 80,
    targetPercentAfterCompaction: 50,
  },
  estimate,
  windowKey: "session:branch:head-1",
  lastAttempt: {
    windowKey: "session:branch:head-1",
    strategyId: "rolling_summary_compaction",
    status: "completed",
  },
});
assert.equal(duplicate.status, "duplicate_window");

console.log(
  JSON.stringify(
    {
      fillPercent: request.fillPercent,
      disabled: disabled.status,
      below: below.status,
      request: request.status,
      duplicate: duplicate.status,
    },
    null,
    2,
  ),
);
