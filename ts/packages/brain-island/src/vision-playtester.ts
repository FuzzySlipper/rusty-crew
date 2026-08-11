export const VISION_PLAYTESTER_PROVIDER_ALIAS =
  "deepseek-flash-responses" as const;
export const VISION_PLAYTESTER_LOCAL_TOOL_PROFILE_ID =
  "vision_playtester" as const;
export const VISION_PLAYTESTER_SOURCE_TOOL_NAMES = [
  "playtest_start",
  "playtest_observe",
  "playtest_act",
  "playtest_finish",
] as const;

export type VisionPlaytesterSourceToolName =
  (typeof VISION_PLAYTESTER_SOURCE_TOOL_NAMES)[number];
export type VisionPlaytestOutcome =
  | "pass"
  | "fail"
  | "uncertain"
  | "infrastructure_error";

export interface VisionPlaytestBudget {
  maxActions: number;
  maxSessionMinutes: number;
  maxEstimatedCostUsd?: number;
}

export interface VisionPlaytestResumeState {
  playtestSessionId: string;
  lastObservationOffset?: string;
  artifactRefs?: string[];
}

export interface VisionPlaytestMission {
  project: string;
  repositoryRevision: string;
  projectManifest: string;
  mission: string;
  controls: string[];
  startupExpectations?: string[];
  budget: VisionPlaytestBudget;
  artifactPolicy: string;
  denTaskId?: number;
  correlationId?: string;
  resume?: VisionPlaytestResumeState;
}

export interface VisionPlaytestFinding {
  summary: string;
  classification: "product" | "configuration" | "infrastructure" | "uncertain";
  evidenceRefs: string[];
  reproduction: "attempted" | "not_useful" | "not_possible";
}

export interface VisionPlaytestReport {
  outcome: VisionPlaytestOutcome;
  summary: string;
  playtestSessionId: string;
  providerAlias: string;
  modelId: string;
  evidenceIndex: string;
  artifactRefs: string[];
  actionsUsed: number;
  estimatedCostUsd?: number;
  findings: VisionPlaytestFinding[];
  uncertainty?: string;
}

export interface VisionPlaytestReportValidation {
  ok: boolean;
  diagnostics: string[];
}

export const VISION_PLAYTESTER_SYSTEM_PROMPT = [
  "You are an operator and observer, not a fixer.",
  "Attempt the supplied mission using only the supplied playtest controls and the ordinary visible interface.",
  "If an action cannot be completed, capture what happened, make at most one bounded reproduction attempt when useful, and report the result.",
  "After an operation fails, make no more than one additional call for that operation. Do not create a third attempt by changing harness parameters or inventing another path.",
  "Do not modify configuration, repair services, invent alternate input paths, deploy replacements, inspect hidden application state, or broaden the mission.",
  "The orchestrator decides whether errors should be retried or repaired.",
  "A playtest is successful when it produces a well-supported result. Product failure, configuration warning, infrastructure error, and honest uncertainty are as valuable as completing the gameplay mission.",
  "Return exactly one outcome: pass, fail, uncertain, or infrastructure_error. Reference observation, action, timeline, screenshot, frame, or evidence-index offsets for every finding.",
  "Do not use one screenshot as sole evidence for motion, input handedness, collision, or a state transition. Compare observations or a frame burst.",
  "For deliver_completion_md use frontmatter status: completed for every well-supported playtest outcome, and put pass, fail, uncertain, or infrastructure_error in the report body.",
  "The model-facing tool surface is task-focus friction, not a security sandbox. Follow the operator role even if the underlying testing installation has broader capabilities outside this profile.",
].join("\n\n");

export function renderVisionPlaytestMission(
  mission: VisionPlaytestMission,
): string {
  assertMission(mission);
  const resume = mission.resume;
  return [
    "# Playtest Mission",
    `Project: ${mission.project}`,
    `Exact repository revision: ${mission.repositoryRevision}`,
    `Project manifest: ${mission.projectManifest}`,
    mission.denTaskId === undefined
      ? undefined
      : `Den task: ${mission.denTaskId}`,
    mission.correlationId ? `Correlation: ${mission.correlationId}` : undefined,
    "",
    "## Mission",
    mission.mission,
    "",
    "## Supplied Controls",
    ...mission.controls.map((control) => `- ${control}`),
    ...(mission.startupExpectations?.length
      ? [
          "",
          "## Startup Expectations",
          ...mission.startupExpectations.map((item) => `- ${item}`),
        ]
      : []),
    "",
    "## Declared Budget",
    `- actions: ${mission.budget.maxActions}`,
    `- session minutes: ${mission.budget.maxSessionMinutes}`,
    mission.budget.maxEstimatedCostUsd === undefined
      ? undefined
      : `- estimated model cost USD: ${mission.budget.maxEstimatedCostUsd}`,
    `- artifact policy: ${mission.artifactPolicy}`,
    `Pass ${mission.repositoryRevision} unchanged as expected_revision to playtest_start, along with the action, session-minute, and optional estimated-cost budget. The adapter rejects a different repository HEAD before launch and verifies that the broker evidence index records the same commit. It rejects further act/observe work after the delegated action or session budget, while finish remains available for an evidence-backed terminal report.`,
    ...(resume
      ? [
          "",
          "## Resume Existing Mission",
          `- reuse playtest session: ${resume.playtestSessionId}`,
          resume.lastObservationOffset
            ? `- last observation: ${resume.lastObservationOffset}`
            : undefined,
          ...(resume.artifactRefs ?? []).map(
            (ref) => `- retained artifact: ${ref}`,
          ),
          "Do not start over. Continue the same playtest session and retain its prior evidence references.",
        ]
      : []),
    "",
    "## Completion",
    "Finish the broker session with pass, fail, uncertain, or infrastructure_error, then deliver a concise report containing the actual provider/model, session id, evidence index, action count, artifact offsets, and any uncertainty.",
    "Stopping because the product, configuration, browser, provider, or infrastructure failed is a valid completion when the evidence supports it.",
  ]
    .filter((line): line is string => line !== undefined)
    .join("\n");
}

export function validateVisionPlaytestReport(
  report: VisionPlaytestReport,
  budget?: VisionPlaytestBudget,
): VisionPlaytestReportValidation {
  const diagnostics: string[] = [];
  if (!report.summary.trim()) diagnostics.push("summary is required");
  if (!report.playtestSessionId.trim())
    diagnostics.push("playtest session id is required");
  if (!report.providerAlias.trim())
    diagnostics.push("provider alias is required");
  if (!report.modelId.trim()) diagnostics.push("model id is required");
  if (!report.evidenceIndex.trim())
    diagnostics.push("evidence index is required");
  if (!Number.isInteger(report.actionsUsed) || report.actionsUsed < 0) {
    diagnostics.push("actions used must be a non-negative integer");
  }
  if (budget && report.actionsUsed > budget.maxActions) {
    diagnostics.push(
      `reported actions ${report.actionsUsed} exceed declared budget ${budget.maxActions}`,
    );
  }
  if (
    budget?.maxEstimatedCostUsd !== undefined &&
    report.estimatedCostUsd !== undefined &&
    report.estimatedCostUsd > budget.maxEstimatedCostUsd
  ) {
    diagnostics.push("reported estimated cost exceeds the declared budget");
  }
  if (report.outcome === "uncertain" && !report.uncertainty?.trim()) {
    diagnostics.push("uncertain outcome requires an uncertainty explanation");
  }
  for (const [index, finding] of report.findings.entries()) {
    if (!finding.summary.trim())
      diagnostics.push(`finding ${index} summary is required`);
    if (finding.evidenceRefs.length === 0) {
      diagnostics.push(
        `finding ${index} requires at least one evidence reference`,
      );
    }
  }
  return { ok: diagnostics.length === 0, diagnostics };
}

function assertMission(mission: VisionPlaytestMission): void {
  for (const [label, value] of [
    ["project", mission.project],
    ["repository revision", mission.repositoryRevision],
    ["project manifest", mission.projectManifest],
    ["mission", mission.mission],
    ["artifact policy", mission.artifactPolicy],
  ] as const) {
    if (!value.trim()) throw new Error(`${label} is required`);
  }
  if (mission.controls.length === 0)
    throw new Error("at least one control is required");
  if (
    !Number.isInteger(mission.budget.maxActions) ||
    mission.budget.maxActions < 1
  ) {
    throw new Error("maxActions must be a positive integer");
  }
  if (
    !Number.isFinite(mission.budget.maxSessionMinutes) ||
    mission.budget.maxSessionMinutes <= 0
  ) {
    throw new Error("maxSessionMinutes must be greater than zero");
  }
}
