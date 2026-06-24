import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  AgentInstanceId,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  convertMcpToolsToCandidates,
  createMcpAdapterRegistration,
  createMcpBrainTool,
  createSimulatedMcpTransportFactory,
  discoverMcpToolCandidates,
  McpSurfaceManager,
} from "./index.js";

const adapterId = "mcp-ts-main" as AdapterId;
const registration = createMcpAdapterRegistration(adapterId);
assert.equal(registration.kind, "mcp");

const alphaBinding: McpBindingRecord = {
  bindingId: "mcp-alpha",
  adapterId,
  agentId: "agent-alpha" as AgentId,
  instanceId: "instance-alpha" as AgentInstanceId,
  sessionId: "session-alpha" as SessionId,
  profileId: "prime-profile" as ProfileId,
  serverNames: ["den", "filesystem"],
  endpointRef: "config://mcp/alpha",
  transport: "stdio",
  toolProfileKey: "prime-mcp",
  status: "active",
  diagnostics: {},
};

const betaBinding: McpBindingRecord = {
  bindingId: "mcp-beta",
  adapterId,
  agentId: "agent-beta" as AgentId,
  instanceId: "instance-beta" as AgentInstanceId,
  sessionId: "session-beta" as SessionId,
  profileId: "review-profile" as ProfileId,
  serverNames: ["den"],
  endpointRef: "config://mcp/beta",
  transport: "streamable_http",
  toolProfileKey: "review-mcp",
  status: "active",
  diagnostics: {
    notes: "optional review helper",
  },
};

const unsupportedBinding: McpBindingRecord = {
  ...betaBinding,
  bindingId: "mcp-unsupported",
  transport: "websocket",
  endpointRef: "config://mcp/unsupported",
  toolProfileKey: "unsupported-mcp",
};

const stdioFactory = createSimulatedMcpTransportFactory("stdio");
const httpFactory = createSimulatedMcpTransportFactory("streamable_http", {
  failConnects: 1,
});
let nowIndex = 0;
const manager = new McpSurfaceManager({
  transports: [stdioFactory, httpFactory],
  backoff: { maxAttempts: 2, backoffMs: [10, 20] },
  now: () => `2026-06-20T06:00:0${nowIndex++}.000Z`,
});

const alphaConnect = await manager.connect(alphaBinding);
assert.equal(alphaConnect.status, "active");
assert.equal(alphaConnect.attemptCount, 1);
assert.equal(stdioFactory.opened.length, 1);
assert.equal(stdioFactory.opened[0]?.endpointRef, "config://mcp/alpha");
assert.deepEqual(stdioFactory.opened[0]?.serverNames, ["den", "filesystem"]);

const betaConnect = await manager.connect(betaBinding);
assert.equal(betaConnect.status, "active");
assert.equal(betaConnect.attemptCount, 2);
assert.equal(betaConnect.optional, true);
assert.equal(httpFactory.opened.length, 1);
assert.equal(httpFactory.opened[0]?.binding.bindingId, "mcp-beta");

const alphaIdentity = manager.identity("mcp-alpha");
assert.equal(alphaIdentity?.agentId, "agent-alpha");
assert.equal(alphaIdentity?.toolProfileKey, "prime-mcp");

const alphaDiagnostics = manager.diagnostics("mcp-alpha");
assert.ok(!Array.isArray(alphaDiagnostics));
assert.equal(alphaDiagnostics?.status, "active");
assert.equal(alphaDiagnostics?.endpointRef, "config://mcp/alpha");
assert.equal(alphaDiagnostics?.lastError, undefined);

const unsupportedConnect = await manager.connect(unsupportedBinding);
assert.equal(unsupportedConnect.status, "degraded");
assert.match(
  unsupportedConnect.degradedReason ?? "",
  /no MCP transport factory registered/,
);

const reloadedAlpha = await manager.reload({
  ...alphaBinding,
  serverNames: ["den"],
  discoveredToolRevision: "rev-alpha-2",
});
assert.equal(reloadedAlpha.status, "active");
assert.equal(stdioFactory.opened.length, 2);
assert.deepEqual(manager.identity("mcp-alpha")?.serverNames, ["den"]);
assert.deepEqual(stdioFactory.disconnected, ["mcp-alpha"]);

const archivedBeta = await manager.archive("mcp-beta");
assert.equal(archivedBeta?.status, "archived");
assert.deepEqual(httpFactory.disconnected, ["mcp-beta"]);

const shutdownDiagnostics = await manager.shutdown();
assert.equal(shutdownDiagnostics.length, 3);
assert.deepEqual(
  shutdownDiagnostics.map((entry) => [entry.bindingId, entry.status]),
  [
    ["mcp-alpha", "archived"],
    ["mcp-beta", "archived"],
    ["mcp-unsupported", "archived"],
  ],
);

const discovered = await discoverMcpToolCandidates(alphaBinding, {
  listTools: () => [
    {
      name: "ReadResource",
      description: "Read a resource from the connected MCP server.",
      inputSchema: {
        type: "object",
        properties: {
          uri: { type: "string", minLength: 1 },
          includeMetadata: { type: ["boolean", "null"] },
        },
        required: ["uri"],
        additionalProperties: false,
      },
      outputSchema: {
        type: "object",
        properties: { text: { type: "string" } },
      },
      annotations: { readOnlyHint: true, title: "Read resource" },
    },
    {
      name: "echo-text",
      description: "Echo text.",
      inputSchema: { type: "string" },
      annotations: { destructiveHint: true },
    },
  ],
});

assert.equal(discovered.candidates.length, 2);
assert.equal(discovered.candidates[0]?.name, "mcp_read_resource");
assert.equal(
  discovered.candidates[0]?.source.endpointRef,
  "config://mcp/alpha",
);
assert.deepEqual(discovered.candidates[0]?.toolsets, ["mcp:prime-mcp"]);
assert.equal(discovered.candidates[1]?.name, "mcp_echo_text");
assert.deepEqual(discovered.candidates[1]?.safety, [
  "network_access",
  "external_write",
]);
assert.equal(
  discovered.issues.find((issue) => issue.code === "schema_wrapped")?.toolName,
  "echo-text",
);

const duplicateReport = convertMcpToolsToCandidates(betaBinding, [
  { name: "search", inputSchema: true },
  { name: "search", inputSchema: false },
]);
assert.equal(
  duplicateReport.issues.find((issue) => issue.code === "duplicate_source_tool")
    ?.severity,
  "error",
);
assert.equal(
  duplicateReport.issues.find((issue) => issue.code === "schema_sanitized")
    ?.toolName,
  "search",
);

const executorCalls: unknown[] = [];
const brainTool = createMcpBrainTool(alphaBinding, discovered.candidates[0]!, {
  callTool(input) {
    executorCalls.push(input);
    return {
      content: `read ${JSON.stringify(input.arguments)}`,
      details: {
        bindingId: input.binding.bindingId,
        sourceToolName: input.toolName,
      },
    };
  },
});

assert.equal(brainTool.name, "mcp_read_resource");
assert.equal(brainTool.label, "Read resource");
const brainToolResult = await brainTool.execute("tool-call-1", {
  uri: "den://doc/example",
  includeMetadata: null,
});
assert.equal(brainToolResult.content[0]?.type, "text");
assert.equal(executorCalls.length, 1);

console.log(
  JSON.stringify(
    {
      registration,
      alphaStatus: alphaConnect.status,
      betaAttempts: betaConnect.attemptCount,
      unsupportedStatus: unsupportedConnect.status,
      surfaces: shutdownDiagnostics.length,
      discoveredCandidates: discovered.candidates.map(
        (candidate) => candidate.name,
      ),
      duplicateIssueCount: duplicateReport.issues.length,
      independentToolProfiles: [
        alphaBinding.toolProfileKey,
        betaBinding.toolProfileKey,
      ],
    },
    null,
    2,
  ),
);
