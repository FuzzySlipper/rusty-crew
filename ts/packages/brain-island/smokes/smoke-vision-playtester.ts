import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import { parseProfileConfigDraft } from "../src/profile-loading.js";
import { selectToolProfile } from "../src/tool-profile-selection.js";
import {
  createVisionPlaytesterCliRuntime,
  playtestActParameters,
  playtestObserveParameters,
  playtestStartParameters,
  withVisionPlaytesterBudgets,
  visionPlaytesterTools,
  type VisionPlaytesterOperation,
  type VisionPlaytesterRuntime,
} from "../src/vision-playtester-tools.js";
import {
  renderVisionPlaytestMission,
  validateVisionPlaytestReport,
  VISION_PLAYTESTER_PROVIDER_ALIAS,
  VISION_PLAYTESTER_SOURCE_TOOL_NAMES,
  VISION_PLAYTESTER_SYSTEM_PROMPT,
  type VisionPlaytestOutcome,
  type VisionPlaytestReport,
} from "../src/vision-playtester.js";

const inventory = selectToolProfile({
  profileId: "vision-playtester" as ProfileId,
  policy: {
    requestedToolsets: ["vision_playtester"],
    requestedTools: ["deliver_completion_md"],
  },
}).inventory;
assert.deepEqual(
  new Set(inventory.selectedTools.map((tool) => tool.name)),
  new Set([
    ...VISION_PLAYTESTER_SOURCE_TOOL_NAMES,
    "deliver_completion_md",
    "rusty_crew_help",
  ]),
);
assert.equal(
  inventory.selectedTools.some((tool) =>
    /terminal|file|browser|inspect|eval|cdp|http|config/.test(tool.name),
  ),
  false,
);

const actSchemaText = JSON.stringify(playtestActParameters);
for (const visibleAction of [
  "keyboard_press",
  "keyboard_down",
  "keyboard_up",
  "mouse_move",
  "mouse_click",
  "mouse_down",
  "mouse_up",
  "mouse_wheel",
  "wait",
]) {
  assert.match(actSchemaText, new RegExp(visibleAction));
}
for (const bypass of [
  "evaluate",
  "selector",
  "dispatch",
  "cdp",
  "request",
  "add_script",
  "goto",
]) {
  assert.doesNotMatch(actSchemaText, new RegExp(bypass));
}
const observeSchemaText = JSON.stringify(playtestObserveParameters);
assert.match(observeSchemaText, /screenshot/);
assert.match(observeSchemaText, /frameBurst/);
assert.doesNotMatch(observeSchemaText, /expressions|storage|dom|application/);
const startSchemaText = JSON.stringify(playtestStartParameters);
assert.doesNotMatch(startSchemaText, /headed|record_video|viewport/);
assert.match(startSchemaText, /expected_revision/);

const staleRevisionStart = await createVisionPlaytesterCliRuntime({
  cliPath: "/must-not-run/den-playwright",
  configPath: "/tmp/playtest-config.yaml",
  resolveRevision: async () => "1".repeat(40),
}).execute("start", {
  project: "revision-probe",
  repo_root: "/tmp/revision-probe",
  expected_revision: "2".repeat(40),
  manifest_path: "/tmp/revision-probe/.den-playwright.json",
  scenario: "reject-stale-revision",
  budget: { max_actions: 1, max_session_minutes: 1 },
});
assert.equal(staleRevisionStart.ok, false);
assert.match(staleRevisionStart.error ?? "", /revision mismatch/);
assert.match(staleRevisionStart.error ?? "", /infrastructure_error/);

const temp = await mkdtemp(join(tmpdir(), "rusty-crew-playtester-"));
try {
  const screenshot = join(temp, "visible.png");
  await writeFile(
    screenshot,
    Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
      "base64",
    ),
  );
  const calls: Array<{
    operation: VisionPlaytesterOperation;
    request: Record<string, unknown>;
  }> = [];
  const runtime: VisionPlaytesterRuntime = {
    execute(operation, request) {
      calls.push({ operation, request });
      if (operation === "observe") {
        return Promise.resolve({
          ok: true,
          operation,
          value: {
            session_id: request.session_id,
            index_path: join(temp, "playtest-index.json"),
            result: { screenshot: "visible.png" },
          },
          imagePaths: [screenshot],
        });
      }
      return Promise.resolve({
        ok: true,
        operation,
        value: { session_id: request.session_id ?? "playtest-existing-1" },
        imagePaths: [],
      });
    },
  };
  const tools = visionPlaytesterTools(runtime);
  assert.deepEqual(
    tools.map((tool) => tool.name),
    VISION_PLAYTESTER_SOURCE_TOOL_NAMES,
  );
  const observe = tools.find((tool) => tool.name === "playtest_observe")!;
  const observation = await observe.execute("observe-1", {
    session_id: "playtest-existing-1",
    screenshot: true,
    label: "after-pointer-lock",
  });
  assert.equal(observation.content[0]?.type, "text");
  assert.equal(observation.content[1]?.type, "image");

  // Recreating the brain-facing tools after a provider interruption does not
  // create a broker session. The same durable session id is supplied again.
  const resumedTools = visionPlaytesterTools(runtime);
  const act = resumedTools.find((tool) => tool.name === "playtest_act")!;
  await act.execute("act-after-resume", {
    session_id: "playtest-existing-1",
    actions: [{ type: "keyboard_press", key: "KeyW" }],
  });
  assert.deepEqual(
    calls.map((call) => [call.operation, call.request.session_id]),
    [
      ["observe", "playtest-existing-1"],
      ["act", "playtest-existing-1"],
    ],
  );

  const failedRuntime: VisionPlaytesterRuntime = {
    execute(operation) {
      return Promise.resolve({
        ok: false,
        operation,
        error: "browser process exited before observation",
        imagePaths: [],
      });
    },
  };
  const failedObservation = await visionPlaytesterTools(failedRuntime)
    .find((tool) => tool.name === "playtest_observe")!
    .execute("observe-failed", {
      session_id: "playtest-existing-1",
      screenshot: true,
    });
  assert.match(
    failedObservation.content[0]?.type === "text"
      ? failedObservation.content[0].text
      : "",
    /browser process exited/,
  );
} finally {
  await rm(temp, { recursive: true, force: true });
}

let nowMs = 1_000;
const budgetedCalls: VisionPlaytesterOperation[] = [];
const budgetedRuntime = withVisionPlaytesterBudgets(
  {
    execute(operation) {
      budgetedCalls.push(operation);
      return Promise.resolve({
        ok: true,
        operation,
        value: { session_id: "budgeted-playtest-1" },
        imagePaths: [],
      });
    },
  },
  () => nowMs,
);
await budgetedRuntime.execute("start", {
  budget: {
    max_actions: 2,
    max_session_minutes: 1,
    max_estimated_cost_usd: 0.25,
  },
});
await budgetedRuntime.execute("act", {
  session_id: "budgeted-playtest-1",
  actions: [{ type: "keyboard_press", key: "KeyW" }],
});
const actionBudgetFailure = await budgetedRuntime.execute("act", {
  session_id: "budgeted-playtest-1",
  actions: [
    { type: "mouse_move", x: 10, y: 10 },
    { type: "mouse_click", x: 10, y: 10 },
  ],
});
assert.equal(actionBudgetFailure.ok, false);
assert.match(actionBudgetFailure.error ?? "", /action budget exhausted/);
assert.deepEqual(budgetedCalls, ["start", "act"]);
nowMs += 61_000;
const sessionBudgetFailure = await budgetedRuntime.execute("observe", {
  session_id: "budgeted-playtest-1",
  screenshot: true,
});
assert.equal(sessionBudgetFailure.ok, false);
assert.match(sessionBudgetFailure.error ?? "", /session budget exhausted/);
// Finishing remains possible after budget exhaustion so the model can write a
// supported terminal result and release broker resources.
const overBudgetFinish = await budgetedRuntime.execute("finish", {
  session_id: "budgeted-playtest-1",
  outcome: "uncertain",
  annotation: "delegated budget exhausted",
});
assert.equal(overBudgetFinish.ok, true);
assert.deepEqual(budgetedCalls, ["start", "act", "finish"]);

const mission = renderVisionPlaytestMission({
  project: "rusty-engine-demo",
  repositoryRevision: "0123456789abcdef0123456789abcdef01234567",
  projectManifest: "/home/dev/rusty-engine-demo/.den-playwright.json",
  mission:
    "Enter pointer lock, move forward, turn left, and report visible behavior.",
  controls: ["click canvas: mouse_click", "move: KeyW", "turn: mouse_move"],
  startupExpectations: ["A WebGL canvas becomes visible."],
  budget: {
    maxActions: 24,
    maxSessionMinutes: 15,
    maxEstimatedCostUsd: 0.25,
  },
  artifactPolicy: "screenshots, frame bursts, trace, and final evidence index",
  denTaskId: 6785,
  correlationId: "playtest-6785-a",
  resume: {
    playtestSessionId: "playtest-existing-1",
    lastObservationOffset: "timeline:7",
    artifactRefs: ["screenshots/after-pointer-lock.png"],
  },
});
assert.match(mission, /reuse playtest session: playtest-existing-1/);
assert.match(mission, /Do not start over/);
assert.match(mission, /actions: 24/);
assert.match(mission, /infrastructure_error/);

for (const outcome of [
  "pass",
  "fail",
  "uncertain",
  "infrastructure_error",
] satisfies VisionPlaytestOutcome[]) {
  const result = validateVisionPlaytestReport(validReport(outcome), {
    maxActions: 24,
    maxSessionMinutes: 15,
    maxEstimatedCostUsd: 0.25,
  });
  assert.deepEqual(result, { ok: true, diagnostics: [] });
}

const overBudget = validateVisionPlaytestReport(
  { ...validReport("fail"), actionsUsed: 25, estimatedCostUsd: 0.3 },
  { maxActions: 24, maxSessionMinutes: 15, maxEstimatedCostUsd: 0.25 },
);
assert.equal(overBudget.ok, false);
assert.match(overBudget.diagnostics.join("\n"), /actions.*exceed/);
assert.match(overBudget.diagnostics.join("\n"), /cost exceeds/);

const unsupportedFinding = validateVisionPlaytestReport({
  ...validReport("fail"),
  findings: [
    {
      summary: "Movement did not visibly change the scene.",
      classification: "product",
      evidenceRefs: [],
      reproduction: "attempted",
    },
  ],
});
assert.equal(unsupportedFinding.ok, false);
assert.match(unsupportedFinding.diagnostics.join("\n"), /evidence reference/);

const template = JSON.parse(
  await readFile(
    new URL(
      "../../../../docs/profile-templates/vision-playtester.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as Record<string, unknown>;
const parsedTemplate = parseProfileConfigDraft({
  profilesDir: "/tmp/vision-playtester-profile-cert",
  profileId: "vision-playtester" as ProfileId,
  profileConfig: template,
});
assert.equal(parsedTemplate.providerAlias, VISION_PLAYTESTER_PROVIDER_ALIAS);
assert.equal(parsedTemplate.brain?.module, "openai-responses");
assert.equal(
  parsedTemplate.runtime?.defaultResourceLimits?.maxDurationMs,
  undefined,
);
assert.equal(
  parsedTemplate.runtime?.defaultResourceLimits?.maxDelegationDepth,
  0,
);
assert.deepEqual(parsedTemplate.toolPolicy?.requestedToolsets, [
  "vision_playtester",
]);
assert.match(
  VISION_PLAYTESTER_SYSTEM_PROMPT,
  /operator and observer, not a fixer/,
);
assert.match(
  VISION_PLAYTESTER_SYSTEM_PROMPT,
  /no more than one additional call/,
);
assert.match(JSON.stringify(template.prompt), /not a security sandbox/);

console.log(
  JSON.stringify(
    {
      providerAlias: template.providerAlias,
      selectedTools: [
        ...VISION_PLAYTESTER_SOURCE_TOOL_NAMES,
        "deliver_completion_md",
        "rusty_crew_help",
      ],
      outcomes: ["pass", "fail", "uncertain", "infrastructure_error"],
      resumeSession: "playtest-existing-1",
      toolSurfaceIsSecurityBoundary: false,
    },
    null,
    2,
  ),
);

function validReport(outcome: VisionPlaytestOutcome): VisionPlaytestReport {
  return {
    outcome,
    summary: `${outcome} result with evidence`,
    playtestSessionId: "playtest-existing-1",
    providerAlias: VISION_PLAYTESTER_PROVIDER_ALIAS,
    modelId: "deepseek-v4-flash",
    evidenceIndex: "/tmp/playtest-existing-1/playtest-index.json",
    artifactRefs: ["timeline:7", "screenshots/after-pointer-lock.png"],
    actionsUsed: 12,
    estimatedCostUsd: 0.1,
    findings: [
      {
        summary: "Visible result recorded.",
        classification:
          outcome === "infrastructure_error" ? "infrastructure" : "product",
        evidenceRefs: ["timeline:7", "screenshots/after-pointer-lock.png"],
        reproduction: outcome === "pass" ? "not_useful" : "attempted",
      },
    ],
    uncertainty:
      outcome === "uncertain"
        ? "Motion evidence remained ambiguous."
        : undefined,
  };
}
