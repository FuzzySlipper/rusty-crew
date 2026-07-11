export const CAPABILITY_EVIDENCE_SCHEMA_VERSION = 2 as const;

export type CapabilitySupport = "supported" | "unsupported" | "not_exercised";
export type HarnessRuntimeKind = "codex_app_server" | "direct_brain";

export interface CapabilityScenario {
  id: string;
  title: string;
  prompt: string;
  fixture: {
    kind: "directory";
    sourceRef: string;
  };
  requiredCapabilities: string[];
  permittedEffects: string[];
  expectedArtifacts: string[];
  validationCommands: string[];
  runtimeApplicability: Record<HarnessRuntimeKind, RuntimeApplicability>;
}

export type RuntimeApplicability =
  | { status: "applicable" }
  | { status: "unsupported"; reason: string };

export interface CapabilityObservation {
  capability: string;
  support: CapabilitySupport;
  evidence?: string;
  reason?: string;
}

export interface RuntimeEvidence {
  runtimeId: string;
  runtimeKind: HarnessRuntimeKind;
  backend: string;
  executable?: {
    version: string;
    sha256?: string;
    protocolSchemaSha256?: string;
  };
  model?: string;
  effort?: string;
  effectiveConfig: Record<string, unknown>;
  tools: string[];
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  usage?: Record<string, number>;
  lifecycleEvents: Array<Record<string, unknown>>;
  toolEvents: Array<Record<string, unknown>>;
  commands: Array<Record<string, unknown>>;
  fileChanges: Array<Record<string, unknown>>;
  tests: Array<Record<string, unknown>>;
  interactions: Array<Record<string, unknown>>;
  capabilities: CapabilityObservation[];
  finalResponse?: string;
  failures: Array<{ code: string; message: string }>;
  restart: {
    exercised: boolean;
    recovered?: boolean;
    evidence?: string;
  };
}

export interface EvidenceComparison {
  scenarioPassedByRuntime: Record<string, boolean>;
  artifactQuality: Record<string, string>;
  interventionCount: Record<string, number>;
  latencyMs: Record<string, number>;
  usageSignals: Record<string, Record<string, number>>;
  recovery: Record<string, string>;
  unsupportedCapabilities: Record<string, string[]>;
}

export interface CapabilityEvidencePacket {
  schemaVersion: typeof CAPABILITY_EVIDENCE_SCHEMA_VERSION;
  runId: string;
  createdAt: string;
  scenario: CapabilityScenario;
  runtimes: RuntimeEvidence[];
  comparison: EvidenceComparison;
}

export interface CapabilityDebugSnapshot {
  schemaVersion: typeof CAPABILITY_EVIDENCE_SCHEMA_VERSION;
  runId: string;
  capturedAt: string;
  rawByRuntime: Record<string, unknown>;
}

export function validateCapabilityScenario(
  scenario: CapabilityScenario,
): CapabilityScenario {
  requireIdentifier(scenario.id, "scenario.id");
  requireText(scenario.title, "scenario.title");
  requireText(scenario.prompt, "scenario.prompt");
  requireText(scenario.fixture.sourceRef, "scenario.fixture.sourceRef");
  requireNonEmpty(scenario.requiredCapabilities, "requiredCapabilities");
  requireNonEmpty(scenario.permittedEffects, "permittedEffects");
  requireNonEmpty(scenario.expectedArtifacts, "expectedArtifacts");
  requireNonEmpty(scenario.validationCommands, "validationCommands");
  for (const runtimeKind of ["codex_app_server", "direct_brain"] as const) {
    const applicability = scenario.runtimeApplicability[runtimeKind];
    if (applicability.status === "unsupported") {
      requireText(
        applicability.reason,
        `runtimeApplicability.${runtimeKind}.reason`,
      );
    }
  }
  return structuredClone(scenario);
}

export function buildEvidenceComparison(
  runtimes: readonly RuntimeEvidence[],
): EvidenceComparison {
  const comparison: EvidenceComparison = {
    scenarioPassedByRuntime: {},
    artifactQuality: {},
    interventionCount: {},
    latencyMs: {},
    usageSignals: {},
    recovery: {},
    unsupportedCapabilities: {},
  };
  for (const runtime of runtimes) {
    comparison.scenarioPassedByRuntime[runtime.runtimeId] =
      runtime.failures.length === 0 &&
      runtime.capabilities.every((item) => item.support !== "unsupported");
    comparison.artifactQuality[runtime.runtimeId] =
      runtime.fileChanges.length > 0 ? "changed_and_inspectable" : "no_change";
    comparison.interventionCount[runtime.runtimeId] =
      runtime.interactions.length;
    comparison.latencyMs[runtime.runtimeId] = runtime.durationMs;
    comparison.usageSignals[runtime.runtimeId] = runtime.usage ?? {};
    comparison.recovery[runtime.runtimeId] = runtime.restart.exercised
      ? runtime.restart.recovered === true
        ? "recovered"
        : "failed_or_unproven"
      : "not_exercised";
    comparison.unsupportedCapabilities[runtime.runtimeId] = runtime.capabilities
      .filter((item) => item.support === "unsupported")
      .map((item) => item.capability);
  }
  return comparison;
}

function requireIdentifier(value: string, path: string): void {
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(value)) {
    throw new Error(`${path} must be a stable lowercase identifier`);
  }
}

function requireText(value: string, path: string): void {
  if (value.trim().length === 0) throw new Error(`${path} must not be empty`);
}

function requireNonEmpty(values: readonly string[], path: string): void {
  if (values.length === 0) throw new Error(`${path} must not be empty`);
  for (const [index, value] of values.entries()) {
    requireText(value, `${path}[${index}]`);
  }
}
