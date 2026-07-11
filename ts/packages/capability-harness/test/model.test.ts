import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  CAPABILITY_EVIDENCE_SCHEMA_VERSION,
  buildEvidenceComparison,
  expandedCapabilityScenarios,
  redactCapabilityEvidence,
  renderScenarioSummary,
  validateCapabilityScenario,
  writeCapabilityArtifacts,
  type CapabilityEvidencePacket,
  type CapabilityScenario,
  type RuntimeEvidence,
} from "../src/index.js";

const scenario: CapabilityScenario = {
  id: "focused_code_edit",
  title: "Focused code edit",
  prompt: "Change the fixture and run its validation.",
  fixture: { kind: "directory", sourceRef: "fixture://focused-code-edit" },
  requiredCapabilities: ["file_write", "command_execution"],
  permittedEffects: ["fixture_repo_write"],
  expectedArtifacts: ["result.txt"],
  validationCommands: ["node test.mjs"],
  runtimeApplicability: {
    codex_app_server: { status: "applicable" },
    direct_brain: { status: "applicable" },
  },
};

test("scenario validation rejects ambiguous empty contracts", () => {
  assert.deepEqual(validateCapabilityScenario(scenario), scenario);
  assert.throws(
    () =>
      validateCapabilityScenario({
        ...scenario,
        requiredCapabilities: [],
      }),
    /requiredCapabilities must not be empty/,
  );
});

test("expanded catalog declares runtime applicability without hiding gaps", () => {
  assert.equal(expandedCapabilityScenarios.length, 8);
  const den = expandedCapabilityScenarios.find(
    (item) => item.id === "den_mcp_read_write",
  );
  assert.deepEqual(den?.runtimeApplicability.codex_app_server, {
    status: "applicable",
  });
  assert.deepEqual(den?.runtimeApplicability.direct_brain, {
    status: "unsupported",
    reason: "certification profile has no MCP binding",
  });
});

test("evidence redaction is bounded and preserves non-secret diagnostics", () => {
  const repeated = { message: "same diagnostic" };
  const redacted = redactCapabilityEvidence({
    authorization: "Bearer actual-token",
    nested: {
      apiKey: "sk-example-secret-123456789",
      message: "provider returned useful failure detail",
      body: "Bearer another-token",
    },
    repeated: [repeated, repeated],
  });
  assert.deepEqual(redacted, {
    authorization: "[REDACTED]",
    nested: {
      apiKey: "[REDACTED]",
      message: "provider returned useful failure detail",
      body: "Bearer [REDACTED]",
    },
    repeated: [repeated, repeated],
  });
});

test("artifact writer emits normalized packet, redacted snapshot, and summary", async () => {
  const directory = await mkdtemp(join(tmpdir(), "capability-evidence-"));
  try {
    const runtimes = [runtimeEvidence("codex", "codex_app_server")];
    const packet: CapabilityEvidencePacket = {
      schemaVersion: CAPABILITY_EVIDENCE_SCHEMA_VERSION,
      runId: "run-1",
      createdAt: "2026-07-10T00:00:00.000Z",
      scenario,
      runtimes,
      comparison: buildEvidenceComparison(runtimes),
    };
    const paths = await writeCapabilityArtifacts(directory, packet, {
      codex: { access_token: "secret", event: "turn/completed" },
    });
    const snapshot = await readFile(paths.debugSnapshot, "utf8");
    const summary = await readFile(paths.scenarioSummary, "utf8");
    assert.match(snapshot, /\[REDACTED\]/);
    assert.doesNotMatch(snapshot, /"secret"/);
    assert.equal(renderScenarioSummary(packet), summary);
    assert.match(summary, /\| codex \| codex_app_server \| yes \|/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

function runtimeEvidence(
  runtimeId: string,
  runtimeKind: RuntimeEvidence["runtimeKind"],
): RuntimeEvidence {
  return {
    runtimeId,
    runtimeKind,
    backend: "live",
    effectiveConfig: {},
    tools: ["write_file", "terminal"],
    startedAt: "2026-07-10T00:00:00.000Z",
    finishedAt: "2026-07-10T00:00:01.000Z",
    durationMs: 1_000,
    lifecycleEvents: [],
    toolEvents: [],
    commands: [],
    fileChanges: [{ path: "result.txt" }],
    tests: [{ command: "node test.mjs", passed: true }],
    interactions: [],
    capabilities: [
      { capability: "file_write", support: "supported" },
      { capability: "command_execution", support: "supported" },
    ],
    failures: [],
    restart: { exercised: false },
  };
}
