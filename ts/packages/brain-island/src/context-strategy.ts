export type ContextStrategyId =
  | "recent_window"
  | "session_memory_augmented"
  | "rolling_summary_compaction";

export type ContextDebugVisibility = "off" | "status" | "verbose";

export interface ContextStrategyDescriptor {
  id: ContextStrategyId;
  label: string;
  description: string;
  status: "active" | "planned";
  supportsAutoCompaction: boolean;
  modelFacingDebugDefault: false;
}

export interface ContextStrategyPolicy {
  enabled: boolean;
  strategyId: ContextStrategyId;
  autoCompactionEnabled: boolean;
  compactAtPercent: number;
  targetPercentAfterCompaction: number;
  maxContextPercentForWake: number;
  debugVisibility: ContextDebugVisibility;
  includeDebugEventsInModelContext: boolean;
  strategyConfig: Record<string, unknown>;
}

export interface ContextStrategyPolicyDiagnostic {
  severity: "error" | "warning";
  code: string;
  path: string;
  message: string;
}

export interface ContextStrategyCatalog {
  schemaVersion: 1;
  defaultStrategyId: ContextStrategyId;
  policyDefaults: ContextStrategyPolicy;
  strategies: ContextStrategyDescriptor[];
  percentRange: {
    min: 1;
    max: 100;
  };
}

const CONTEXT_STRATEGY_DESCRIPTORS: ContextStrategyDescriptor[] = [
  {
    id: "recent_window",
    label: "Recent Window",
    description:
      "Compatibility strategy that preserves the current wake assembly behavior.",
    status: "active",
    supportsAutoCompaction: false,
    modelFacingDebugDefault: false,
  },
  {
    id: "session_memory_augmented",
    label: "Session Memory Augmented",
    description:
      "Uses Rust-selected session memory alongside the recent wake window.",
    status: "planned",
    supportsAutoCompaction: false,
    modelFacingDebugDefault: false,
  },
  {
    id: "rolling_summary_compaction",
    label: "Rolling Summary Compaction",
    description:
      "Plans context-fill-triggered compaction into durable summary artifacts.",
    status: "planned",
    supportsAutoCompaction: true,
    modelFacingDebugDefault: false,
  },
];

export function defaultContextStrategyPolicy(): ContextStrategyPolicy {
  return {
    enabled: true,
    strategyId: "recent_window",
    autoCompactionEnabled: false,
    compactAtPercent: 80,
    targetPercentAfterCompaction: 55,
    maxContextPercentForWake: 95,
    debugVisibility: "status",
    includeDebugEventsInModelContext: false,
    strategyConfig: {},
  };
}

export function contextStrategyCatalog(): ContextStrategyCatalog {
  return {
    schemaVersion: 1,
    defaultStrategyId: "recent_window",
    policyDefaults: defaultContextStrategyPolicy(),
    strategies: CONTEXT_STRATEGY_DESCRIPTORS.map((descriptor) => ({
      ...descriptor,
    })),
    percentRange: {
      min: 1,
      max: 100,
    },
  };
}

export function contextStrategyDescriptor(
  id: string | undefined,
): ContextStrategyDescriptor | undefined {
  return CONTEXT_STRATEGY_DESCRIPTORS.find(
    (descriptor) => descriptor.id === id,
  );
}

export function contextStrategyPolicyFromUnknown(
  value: unknown,
  fallback: ContextStrategyPolicy = defaultContextStrategyPolicy(),
): ContextStrategyPolicy {
  if (!isRecord(value)) {
    return { ...fallback, strategyConfig: { ...fallback.strategyConfig } };
  }
  const diagnostics: ContextStrategyPolicyDiagnostic[] = [];
  return normalizeContextStrategyPolicy(value, fallback, diagnostics);
}

export function contextStrategyPolicyFromPatch(
  value: unknown,
  fallback: ContextStrategyPolicy = defaultContextStrategyPolicy(),
): {
  policy: ContextStrategyPolicy;
  diagnostics: ContextStrategyPolicyDiagnostic[];
} {
  const diagnostics: ContextStrategyPolicyDiagnostic[] = [];
  if (!isRecord(value)) {
    diagnostics.push({
      severity: "error",
      code: "context_policy_invalid",
      path: "contextPolicy",
      message: "contextPolicy must be an object",
    });
    return { policy: fallback, diagnostics };
  }
  return {
    policy: normalizeContextStrategyPolicy(value, fallback, diagnostics),
    diagnostics,
  };
}

function normalizeContextStrategyPolicy(
  raw: Record<string, unknown>,
  fallback: ContextStrategyPolicy,
  diagnostics: ContextStrategyPolicyDiagnostic[],
): ContextStrategyPolicy {
  const strategyId = stringValue(raw.strategyId ?? raw.strategy_id);
  const resolvedStrategyId: ContextStrategyId =
    strategyId === undefined
      ? fallback.strategyId
      : contextStrategyDescriptor(strategyId) === undefined
        ? fallback.strategyId
        : (strategyId as ContextStrategyId);
  if (
    strategyId !== undefined &&
    contextStrategyDescriptor(strategyId) === undefined
  ) {
    diagnostics.push({
      severity: "error",
      code: "context_strategy_unknown",
      path: "contextPolicy.strategyId",
      message: `unknown context strategy ${strategyId}`,
    });
  }

  const compactAtPercent = percentValue(
    raw.compactAtPercent ?? raw.compact_at_percent,
    fallback.compactAtPercent,
    "contextPolicy.compactAtPercent",
    diagnostics,
  );
  const targetPercentAfterCompaction = percentValue(
    raw.targetPercentAfterCompaction ?? raw.target_percent_after_compaction,
    fallback.targetPercentAfterCompaction,
    "contextPolicy.targetPercentAfterCompaction",
    diagnostics,
  );
  const maxContextPercentForWake = percentValue(
    raw.maxContextPercentForWake ?? raw.max_context_percent_for_wake,
    fallback.maxContextPercentForWake,
    "contextPolicy.maxContextPercentForWake",
    diagnostics,
  );
  if (targetPercentAfterCompaction >= compactAtPercent) {
    diagnostics.push({
      severity: "error",
      code: "context_policy_target_not_below_trigger",
      path: "contextPolicy.targetPercentAfterCompaction",
      message:
        "targetPercentAfterCompaction must be lower than compactAtPercent",
    });
  }
  if (compactAtPercent > maxContextPercentForWake) {
    diagnostics.push({
      severity: "error",
      code: "context_policy_trigger_above_wake_guard",
      path: "contextPolicy.compactAtPercent",
      message: "compactAtPercent must not exceed maxContextPercentForWake",
    });
  }

  const debugVisibility = debugVisibilityValue(
    raw.debugVisibility ?? raw.debug_visibility,
    fallback.debugVisibility,
    diagnostics,
  );
  const rawStrategyConfig = raw.strategyConfig ?? raw.strategy_config;
  const strategyConfig = isRecord(rawStrategyConfig)
    ? { ...rawStrategyConfig }
    : { ...fallback.strategyConfig };

  return {
    enabled: boolValue(raw.enabled, fallback.enabled),
    strategyId: resolvedStrategyId,
    autoCompactionEnabled: boolValue(
      raw.autoCompactionEnabled ?? raw.auto_compaction_enabled,
      fallback.autoCompactionEnabled,
    ),
    compactAtPercent,
    targetPercentAfterCompaction,
    maxContextPercentForWake,
    debugVisibility,
    includeDebugEventsInModelContext: boolValue(
      raw.includeDebugEventsInModelContext ??
        raw.include_debug_events_in_model_context,
      fallback.includeDebugEventsInModelContext,
    ),
    strategyConfig,
  };
}

function percentValue(
  value: unknown,
  fallback: number,
  path: string,
  diagnostics: ContextStrategyPolicyDiagnostic[],
): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  const rounded = Math.floor(value);
  if (rounded < 1 || rounded > 100) {
    diagnostics.push({
      severity: "error",
      code: "context_policy_percent_out_of_range",
      path,
      message: `${path} must be between 1 and 100`,
    });
    return fallback;
  }
  return rounded;
}

function debugVisibilityValue(
  value: unknown,
  fallback: ContextDebugVisibility,
  diagnostics: ContextStrategyPolicyDiagnostic[],
): ContextDebugVisibility {
  if (value === "off" || value === "status" || value === "verbose") {
    return value;
  }
  if (value !== undefined) {
    diagnostics.push({
      severity: "error",
      code: "context_policy_debug_visibility_invalid",
      path: "contextPolicy.debugVisibility",
      message: "debugVisibility must be off, status, or verbose",
    });
  }
  return fallback;
}

function boolValue(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
