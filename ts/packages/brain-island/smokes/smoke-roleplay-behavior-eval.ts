import assert from "node:assert/strict";
import {
  evaluateFirstResponse,
  firstResponseScenario,
  loadStExampleFixture,
} from "../src/roleplay-st-example-fixture.js";

const fixture = loadStExampleFixture();
const scenario = firstResponseScenario(fixture);

assert.match(scenario.openingAssistant, /House Veranthos/);
assert.match(scenario.firstUserReply, /tea tray/);
assert.match(scenario.referenceAssistant, /Genuinely fond/);

const report = evaluateFirstResponse(scenario.referenceAssistant, {
  scenario,
  promptStackTrace: {
    source: "deterministic_fixture",
    sections: ["character_identity", "player_persona", "relevant_lore_context"],
  },
  loreEvidence: {
    source: "st-example",
    loreEntryCount: Object.keys(fixture.lorebook.entries).length,
  },
});

assert.equal(report.scenarioId, "dark-xavier-first-response");
assert.equal(report.totalChecks, 6);
assert.equal(report.passedChecks, 6, JSON.stringify(report, null, 2));
assert.equal(report.score, 1);
assert.deepEqual(report.notableMisses, []);
assert.ok(report.promptStackTrace);
assert.ok(report.loreEvidence);

const artifactResponse = evaluateFirstResponse("assistant: ```json\n{}\n```", {
  scenario,
});
assert.equal(
  artifactResponse.checks.find((check) => check.id === "clean_narrative_output")
    ?.passed,
  false,
);

console.log(
  JSON.stringify(
    {
      scenarioId: report.scenarioId,
      score: report.score,
      checks: report.checks.map((check) => [check.id, check.passed]),
      loreEntryCount: (report.loreEvidence as any).loreEntryCount,
    },
    null,
    2,
  ),
);
