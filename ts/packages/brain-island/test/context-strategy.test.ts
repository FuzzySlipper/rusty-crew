import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  contextStrategyCatalog,
  contextStrategyPolicyFromPatch,
  defaultContextStrategyPolicy,
  prepareContextStrategyRoleAssembly,
} from "../src/context-strategy.js";
import {
  contextFillPercent,
  evaluateContextCompactionTrigger,
} from "../src/context-compaction-trigger.js";
import type { ContextUsageEstimate } from "../src/context-estimate.js";

describe("context strategy policy", () => {
  it("exposes a defensive catalog copy", () => {
    const first = contextStrategyCatalog();
    first.strategies[0]!.label = "mutated";

    const second = contextStrategyCatalog();

    assert.equal(second.defaultStrategyId, "recent_window");
    assert.equal(second.strategies[0]!.label, "Recent Window");
  });

  it("normalizes snake_case patch fields and reports policy diagnostics", () => {
    const { policy, diagnostics } = contextStrategyPolicyFromPatch({
      strategy_id: "rolling_summary_compaction",
      auto_compaction_enabled: true,
      compact_at_percent: 80.9,
      target_percent_after_compaction: 81,
      max_context_percent_for_wake: 75,
      debug_visibility: "loud",
      include_debug_events_in_model_context: true,
      strategy_config: { summaryWindow: 12 },
    });

    assert.equal(policy.strategyId, "rolling_summary_compaction");
    assert.equal(policy.autoCompactionEnabled, true);
    assert.equal(policy.compactAtPercent, 80);
    assert.equal(policy.targetPercentAfterCompaction, 81);
    assert.equal(policy.maxContextPercentForWake, 75);
    assert.equal(policy.includeDebugEventsInModelContext, true);
    assert.deepEqual(policy.strategyConfig, { summaryWindow: 12 });
    assert.deepEqual(
      diagnostics.map((diagnostic) => diagnostic.code),
      [
        "context_policy_target_not_below_trigger",
        "context_policy_trigger_above_wake_guard",
        "context_policy_debug_visibility_invalid",
      ],
    );
  });

  it("prepares model-facing instructions for rolling summary compaction", () => {
    const policy = {
      ...defaultContextStrategyPolicy(),
      strategyId: "rolling_summary_compaction" as const,
      autoCompactionEnabled: true,
      compactAtPercent: 72,
      targetPercentAfterCompaction: 45,
    };

    const preparation = prepareContextStrategyRoleAssembly(policy);

    assert.equal(preparation.strategyId, "rolling_summary_compaction");
    assert.equal(preparation.diagnostics.length, 0);
    assert.match(preparation.additionalInstructions[0] ?? "", /enabled at 72%/);
    assert.match(preparation.additionalInstructions[0] ?? "", /target 45%/);
  });
});

describe("context compaction trigger", () => {
  function usageEstimate(
    estimatedPromptTokens: number,
    contextWindowTokens: number,
    usableInputTokens = contextWindowTokens,
  ): ContextUsageEstimate {
    return {
      estimateQuality: "approximate",
      estimateMethod: "unit_fixture",
      estimatorId: "unit_fixture",
      estimatedPromptTokens,
      budget: {
        contextWindowTokens,
        usableInputTokens,
      },
      sampledEventCount: 1,
      sampledMessageCount: 1,
    };
  }

  it("uses usable input tokens before full context-window tokens", () => {
    const fillPercent = contextFillPercent(usageEstimate(801, 10_000, 1_000));

    assert.equal(fillPercent, 81);
  });

  it("requests compaction only once per strategy/window until failure", () => {
    const policy = {
      ...defaultContextStrategyPolicy(),
      strategyId: "rolling_summary_compaction" as const,
      autoCompactionEnabled: true,
      compactAtPercent: 80,
      targetPercentAfterCompaction: 55,
    };
    const estimate = usageEstimate(8_500, 10_000);

    assert.equal(
      evaluateContextCompactionTrigger({
        policy,
        estimate,
        windowKey: "session-a:42",
      }).status,
      "request_compaction",
    );
    assert.equal(
      evaluateContextCompactionTrigger({
        policy,
        estimate,
        windowKey: "session-a:42",
        lastAttempt: {
          windowKey: "session-a:42",
          strategyId: "rolling_summary_compaction",
          status: "completed",
        },
      }).status,
      "duplicate_window",
    );
    assert.equal(
      evaluateContextCompactionTrigger({
        policy,
        estimate,
        windowKey: "session-a:42",
        lastAttempt: {
          windowKey: "session-a:42",
          strategyId: "rolling_summary_compaction",
          status: "failed",
        },
      }).status,
      "request_compaction",
    );
  });
});
