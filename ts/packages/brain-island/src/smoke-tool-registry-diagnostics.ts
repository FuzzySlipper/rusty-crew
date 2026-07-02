import assert from "node:assert/strict";
import {
  buildToolRegistryDiagnostics,
  defaultToolRegistry,
  formatToolRegistryDiagnosticsMarkdown,
} from "./index.js";
import type { ToolRegistryEntry } from "./index.js";

const report = buildToolRegistryDiagnostics({
  inventoryRequest: {
    requestedToolsets: ["local_code_read", "web_research", "browser_vision"],
    requestedTools: ["missing_tool"],
    sessionDeniedTools: ["git_diff"],
    resourceDeniedTools: ["web_search", "browser_vision"],
    resourceDeniedReasons: {
      web_search: "web search provider is not configured",
      browser_vision: "browser binary is not configured",
    },
  },
});

assert.equal(
  report.summary.registeredTools,
  defaultToolRegistry.entries.length,
);
assert.equal(report.summary.selectedTools, 4);
assert.equal(report.summary.deniedTools, 3);
assert.equal(report.summary.missingTools, 1);
assert.equal(report.validation.ok, true);
assert.equal(report.debug, undefined);
assert.equal(Object.hasOwn(report.tools[0]!, "implementationModule"), false);
assert.equal(
  report.tools.find((tool) => tool.name === "git_diff")?.status,
  "session_denied",
);
assert.equal(
  report.tools
    .find((tool) => tool.name === "git_diff")
    ?.safety.includes("read_only"),
  true,
);
assert.equal(
  report.tools
    .find((tool) => tool.name === "git_diff")
    ?.surfaces.includes("brain"),
  true,
);
assert.equal(
  report.tools.find((tool) => tool.name === "git_diff")?.sourceHint,
  "local",
);
assert.equal(
  report.tools.find((tool) => tool.name === "web_extract")?.status,
  "selected",
);
assert.equal(
  report.tools.find((tool) => tool.name === "web_search")?.reasons[0],
  "web search provider is not configured",
);
assert.equal(
  report.tools.find((tool) => tool.name === "browser_vision")?.reasons[0],
  "browser binary is not configured",
);

const debugReport = buildToolRegistryDiagnostics({
  includeDebugBindings: true,
});
assert.ok(debugReport.debug);
assert.equal(
  debugReport.debug.bindings.some(
    (binding) =>
      binding.name === "read_file" &&
      binding.implementationModule === "./local-code-tools.js#readFileTool",
  ),
  true,
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
assert.match(markdown, /web_search/);
assert.match(markdown, /browser_vision/);
assert.doesNotMatch(markdown, /implementation/i);
assert.doesNotMatch(markdown, /local-code-tools/);

console.log(
  JSON.stringify(
    {
      summary: report.summary,
      debugBindings: debugReport.debug.bindings.length,
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
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape,
    version: "1.0.0",
  };
}
