import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  contextStrategyCatalog,
  contextStrategyPolicyFromPatch,
  defaultContextStrategyPolicy,
  prepareContextStrategyRoleAssembly,
} from "../src/context-strategy.js";

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

  it("exposes the Roleplay strategy as an active Crew lifecycle consumer", () => {
    const descriptor = contextStrategyCatalog().strategies.find(
      (strategy) => strategy.id === "roleplay_scene_aware_compaction",
    );
    assert.deepEqual(descriptor, {
      id: "roleplay_scene_aware_compaction",
      label: "Roleplay Scene-Aware Compaction",
      description:
        "Uses Crew's generic lifecycle with Roleplay-owned scene, voice, emotional-continuity, and lore-provenance preservation.",
      status: "active",
      supportsAutoCompaction: true,
      modelFacingDebugDefault: false,
    });
    const preparation = prepareContextStrategyRoleAssembly({
      ...defaultContextStrategyPolicy(),
      strategyId: "roleplay_scene_aware_compaction",
      autoCompactionEnabled: true,
      compactAtPercent: 70,
      targetPercentAfterCompaction: 45,
    });
    assert.match(
      preparation.additionalInstructions[0] ?? "",
      /director context as derived narrative continuity/,
    );
    assert.match(
      preparation.additionalInstructions[0] ?? "",
      /rather than inventing missing facts/,
    );
  });
});
