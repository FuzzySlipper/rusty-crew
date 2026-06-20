import assert from "node:assert/strict";
import {
  createSimulatedMcpTransportFactory,
  McpSurfaceManager,
} from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { reloadMcpSurface } from "./index.js";

const adapterId = "mcp-ts-main" as AdapterId;
const alphaBinding: McpBindingRecord = {
  bindingId: "mcp-alpha",
  adapterId,
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  profileId: "prime-profile" as ProfileId,
  serverNames: ["den"],
  endpointRef: "config://mcp/alpha",
  transport: "stdio",
  toolProfileKey: "prime-mcp",
  discoveredToolRevision: "rev-2",
  status: "active",
  diagnostics: {},
};
const betaBinding: McpBindingRecord = {
  ...alphaBinding,
  bindingId: "mcp-beta",
  agentId: "agent-beta" as AgentId,
  sessionId: "session-beta" as SessionId,
  profileId: "review-profile" as ProfileId,
  endpointRef: "config://mcp/beta",
  toolProfileKey: "review-mcp",
};
const optionalBrokenBinding: McpBindingRecord = {
  ...alphaBinding,
  bindingId: "mcp-optional",
  transport: "websocket",
  endpointRef: "config://mcp/optional",
  diagnostics: { notes: "optional" },
};

let tick = 0;
const manager = new McpSurfaceManager({
  transports: [createSimulatedMcpTransportFactory("stdio")],
  now: () => `2026-06-20T07:00:${String(tick++).padStart(2, "0")}.000Z`,
});

await manager.connect(alphaBinding);
await manager.connect(betaBinding);
assert.equal(manager.diagnostics("mcp-beta")?.status, "active");

const reloadReport = await reloadMcpSurface({
  binding: alphaBinding,
  manager,
  catalogId: "mcp:prime-mcp",
  previousToolNames: ["den_old_tool", "den_stable"],
  inventoryRequest: {
    requestedToolsets: ["mcp:prime-mcp"],
  },
  requestedBy: "operator",
  reason: "catalog refresh after profile update",
  now: () => `2026-06-20T07:01:${String(tick++).padStart(2, "0")}.000Z`,
  discoveryClient: {
    listTools: () => [
      {
        name: "stable",
        description: "Stable tool.",
        inputSchema: true,
      },
      {
        name: "new_tool",
        description: "New tool.",
        inputSchema: {
          type: "object",
          properties: { value: { type: "string" } },
        },
      },
    ],
  },
});

assert.equal(reloadReport.status, "reloaded");
assert.equal(reloadReport.requestedBy, "operator");
assert.equal(reloadReport.reason, "catalog refresh after profile update");
assert.deepEqual(reloadReport.toolDiff.addedTools, ["den_new_tool"]);
assert.deepEqual(reloadReport.toolDiff.removedTools, ["den_old_tool"]);
assert.deepEqual(reloadReport.toolDiff.unchangedTools, ["den_stable"]);
assert.equal(reloadReport.discoveryIssueCount, 0);
assert.equal(reloadReport.collisionCount, 0);
assert.equal(
  reloadReport.registry?.catalogChangedPayload.type,
  "tool_catalog_changed",
);
assert.equal(manager.diagnostics("mcp-beta")?.status, "active");

const degradedReport = await reloadMcpSurface({
  binding: optionalBrokenBinding,
  manager,
  catalogId: "mcp:optional",
  previousToolNames: ["optional_tool"],
  requestedBy: "operator",
  reason: "optional surface retry",
  now: () => `2026-06-20T07:02:${String(tick++).padStart(2, "0")}.000Z`,
  discoveryClient: {
    listTools: () => {
      throw new Error("discovery should not run when connect fails");
    },
  },
});

assert.equal(degradedReport.status, "degraded");
assert.equal(degradedReport.optionalServerFailures.length, 1);
assert.deepEqual(degradedReport.toolDiff.removedTools, ["optional_tool"]);
assert.match(degradedReport.degradedReason ?? "", /no MCP transport factory/);

console.log(
  JSON.stringify(
    {
      status: reloadReport.status,
      added: reloadReport.toolDiff.addedTools,
      removed: reloadReport.toolDiff.removedTools,
      betaStatus: manager.diagnostics("mcp-beta")?.status,
      optionalFailures: degradedReport.optionalServerFailures.length,
      durationMs: reloadReport.durationMs,
    },
    null,
    2,
  ),
);
