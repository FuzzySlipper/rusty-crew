import assert from "node:assert/strict";
import type { McpRegistryCandidate } from "@rusty-crew/adapter-mcp";
import { convertMcpToolsToCandidates } from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  defaultToolRegistry,
  integrateMcpToolsWithRegistry,
  type ToolRegistryEntry,
} from "./index.js";

const binding: McpBindingRecord = {
  bindingId: "mcp-alpha",
  adapterId: "mcp-ts-main" as AdapterId,
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  profileId: "prime-profile" as ProfileId,
  serverNames: ["den"],
  endpointRef: "config://mcp/alpha",
  transport: "stdio",
  toolProfileKey: "prime-mcp",
  discoveredToolRevision: "rev-alpha",
  status: "active",
  diagnostics: {},
};

const discovery = convertMcpToolsToCandidates(binding, [
  {
    name: "search",
    description: "Search Den memory.",
    inputSchema: {
      type: "object",
      properties: { query: { type: "string", minLength: 1 } },
      required: ["query"],
    },
  },
  {
    name: "read_resource",
    description: "Read an MCP resource.",
    inputSchema: true,
  },
]);

const report = integrateMcpToolsWithRegistry({
  catalogId: "mcp:prime-mcp",
  candidates: discovery.candidates,
  inventoryRequest: {
    requestedToolsets: ["local_code_read", "mcp:prime-mcp"],
  },
  unavailableTools: ["den_read_resource"],
});

assert.equal(report.validation.ok, true);
assert.ok(report.registry);
assert.ok(report.inventory);
assert.equal(report.catalogChangedPayload.type, "tool_catalog_changed");
assert.equal(report.catalogChangedPayload.catalogId, "mcp:prime-mcp");
assert.equal(report.mcpEntries.length, 2);
assert.equal(report.mcpBindings.length, 2);
for (const entry of report.mcpEntries) {
  assert.equal(Object.hasOwn(entry, "implementationModule"), false);
  assert.equal(Object.hasOwn(entry, "inventoryTest"), false);
  assert.equal(Object.hasOwn(entry, "mcpSource"), false);
}
assert.equal(
  report.inventory.selectedTools.some((entry) => entry.name === "den_search"),
  true,
);
const selectedDenSearch = report.inventory.selectedTools.find(
  (entry) => entry.name === "den_search",
);
const selectedDenSearchDescriptor = report.inventory.selectedDescriptors.find(
  (descriptor) => descriptor.name === "den_search",
);
assert.equal(selectedDenSearchDescriptor?.description, "Search Den memory.");
assert.equal(
  selectedDenSearchDescriptor?.description,
  selectedDenSearch?.description,
);
const selectedDenSearchBinding = report.inventory.selectedBindings.find(
  (binding) => binding.name === "den_search",
);
assert.ok(selectedDenSearchBinding);
assert.equal(
  selectedDenSearchBinding.implementationModule,
  "@rusty-crew/adapter-mcp#mcpToolExecutor:mcp-alpha:search",
);
const denSearchBinding = report.mcpBindings.find(
  (entry) => entry.name === "den_search",
);
assert.equal(denSearchBinding?.mcpSource.endpointRef, "config://mcp/alpha");
assert.equal(denSearchBinding?.mcpSource.sourceToolName, "search");
assert.equal(
  denSearchBinding?.implementationModule,
  selectedDenSearchBinding.implementationModule,
);
const deniedReadResource = report.inventory.items.find(
  (item) => item.name === "den_read_resource",
);
assert.equal(deniedReadResource?.status, "resource_denied");
assert.notEqual(deniedReadResource?.status, "deprecated");

const localCollisionCandidate: McpRegistryCandidate = {
  ...discovery.candidates[0]!,
  name: "read_file",
  outputShape: "mcp.den.read_file.result.v1",
};
const collisionReport = integrateMcpToolsWithRegistry({
  catalogId: "mcp:collision",
  candidates: [localCollisionCandidate],
});
assert.equal(collisionReport.validation.ok, false);
assert.equal(collisionReport.registry, undefined);
assert.equal(collisionReport.inventory, undefined);
assert.equal(
  collisionReport.validation.issues.find(
    (issue) => issue.code === "duplicate_name",
  )?.toolName,
  "read_file",
);

const prefixedReport = integrateMcpToolsWithRegistry({
  catalogId: "mcp:prefixed",
  candidates: [localCollisionCandidate],
  nameCollisionPolicy: "prefix_source",
  inventoryRequest: {
    requestedToolsets: ["mcp:prime-mcp"],
  },
});
assert.equal(prefixedReport.validation.ok, true);
assert.equal(prefixedReport.mcpEntries[0]?.name, "den_read_file");
assert.equal(prefixedReport.mcpBindings[0]?.name, "den_read_file");
assert.equal(prefixedReport.mcpBindings[0]?.mcpSource.sourceToolName, "search");
assert.equal(
  prefixedReport.inventory?.selectedTools.some(
    (entry) => entry.name === "den_read_file",
  ),
  true,
);

const duplicateMcpReport = integrateMcpToolsWithRegistry({
  catalogId: "mcp:duplicate",
  candidates: [
    discovery.candidates[0]!,
    {
      ...discovery.candidates[0]!,
      source: { ...discovery.candidates[0]!.source, bindingId: "mcp-beta" },
    },
  ],
});
assert.equal(duplicateMcpReport.validation.ok, false);
assert.equal(duplicateMcpReport.registry, undefined);
assert.equal(duplicateMcpReport.inventory, undefined);
assert.equal(
  duplicateMcpReport.validation.issues.find(
    (issue) => issue.code === "duplicate_name",
  )?.toolName,
  "den_search",
);

console.log(
  JSON.stringify(
    {
      baseTools: defaultToolRegistry.entries.length,
      selectedTools: report.inventory?.selectedTools.length,
      mcpEntries: report.mcpEntries.map((entry) => entry.name),
      collisionIssue: collisionReport.validation.issues[0]?.code,
      prefixedName: prefixedReport.mcpEntries[0]?.name,
      duplicateIssue: duplicateMcpReport.validation.issues[0]?.code,
    },
    null,
    2,
  ),
);
