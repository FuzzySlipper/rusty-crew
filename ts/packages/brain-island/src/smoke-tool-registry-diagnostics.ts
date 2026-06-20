import assert from "node:assert/strict";
import {
  buildToolRegistryDiagnostics,
  formatToolRegistryDiagnosticsMarkdown,
} from "./index.js";
import type { ToolRegistryEntry } from "./index.js";

const report = buildToolRegistryDiagnostics({
  inventoryRequest: {
    requestedToolsets: ["local_code_read"],
    requestedTools: ["missing_tool"],
    sessionDeniedTools: ["git_diff"],
  },
});

assert.equal(report.summary.registeredTools, 7);
assert.equal(report.summary.selectedTools, 3);
assert.equal(report.summary.deniedTools, 1);
assert.equal(report.summary.missingTools, 1);
assert.equal(report.validation.ok, true);
assert.equal(
  report.tools.find((tool) => tool.name === "git_diff")?.status,
  "session_denied",
);

const invalidReport = buildToolRegistryDiagnostics({
  entries: [
    entry("do_thing", "common.result.v1"),
    entry("do_thing2", "common.result.v1"),
    {
      ...entry("old_thing", "old.result.v1"),
      deprecated: {
        reason: "testing missing replacement diagnostics",
        since: "0.1.0",
      },
    },
  ],
});

assert.equal(invalidReport.validation.ok, false);
assert.deepEqual(
  invalidReport.validation.issues
    .filter((issue) => issue.severity === "error")
    .map((issue) => issue.code)
    .sort(),
  ["capability_collision", "deprecated_without_replacement"],
);
assert.equal(invalidReport.tools[0]?.status, "invalid_registry");

const markdown = formatToolRegistryDiagnosticsMarkdown(report);
assert.match(markdown, /Tool Registry Diagnostics/);
assert.match(markdown, /git_diff/);
assert.match(markdown, /session_denied/);

console.log(
  JSON.stringify(
    {
      summary: report.summary,
      invalidIssues: invalidReport.validation.issues.map((issue) => issue.code),
      markdownLines: markdown.split("\n").length,
    },
    null,
    2,
  ),
);

function entry(name: string, outputShape: string): ToolRegistryEntry {
  return {
    name,
    description: `${name} description`,
    category: "local",
    toolsets: ["local_code_read"],
    implementationModule: `./tools.js#${name}`,
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape,
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry-diagnostics",
  };
}
