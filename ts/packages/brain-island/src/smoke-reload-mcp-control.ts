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
import {
  AgentActivityObservationProducer,
  createReloadMcpControlExecutor,
  handleAdminControlRequest,
  type AdminControlResponse,
  type AdminRouteResult,
} from "./index.js";
import {
  createMemoryAdminControlAuditSink,
  createMemoryAgentActivityObservationSink,
  createMemoryReloadMcpLifecycleAuditSink,
} from "./test-support.js";

const adapterId = "mcp-main" as AdapterId;
const alphaBinding = binding(
  "mcp-alpha",
  "agent-alpha",
  "session-alpha",
  "prime",
);
const betaBinding = binding("mcp-beta", "agent-beta", "session-beta", "review");
let tick = 0;
const manager = new McpSurfaceManager({
  transports: [createSimulatedMcpTransportFactory("stdio")],
  now: () => `2026-06-20T17:00:${String(tick++).padStart(2, "0")}.000Z`,
});
await manager.connect(alphaBinding);
await manager.connect(betaBinding);

const lifecycleAudit = createMemoryReloadMcpLifecycleAuditSink();
const adminAudit = createMemoryAdminControlAuditSink();
const observationSink = createMemoryAgentActivityObservationSink();
const observationProducer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const reloadMcp = createReloadMcpControlExecutor({
  resolveBinding(sessionId) {
    return sessionId === "session-alpha" ? alphaBinding : undefined;
  },
  manager,
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
  catalogId: (currentBinding) => `mcp:${currentBinding.toolProfileKey}`,
  previousToolNames: () => ["den_old_tool", "den_stable"],
  inventoryRequest: (currentBinding) => ({
    requestedToolsets: [`mcp:${currentBinding.toolProfileKey}`],
  }),
  auditSink: lifecycleAudit,
  observationProducer,
  observationIdentity({ binding: currentBinding }) {
    return {
      profile: currentBinding.profileId,
      instance_id: currentBinding.agentId,
      session_key: currentBinding.sessionId,
    };
  },
  now: () => `2026-06-20T17:01:${String(tick++).padStart(2, "0")}.000Z`,
});

const routeResult = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/mcp/session-alpha/reload",
    headers: {
      authorization: "Bearer control-token",
      "x-rusty-crew-operator": "operator-alpha",
    },
    body: {
      reason: "catalog refresh",
      reasonCode: "slash_reload_mcp",
    },
  },
  {
    auth: { bearerToken: "control-token" },
    executor: { reloadMcp },
    auditSink: adminAudit,
    now: () => "2026-06-20T17:02:00.000Z",
  },
);

assert.equal(routeResult.status, 200);
const data = okData<AdminControlResponse>(routeResult);
const result = data.outcome.result as {
  oldToolCount: number;
  newToolCount: number;
  addedTools: string[];
  removedTools: string[];
  collisionCount: number;
  durationMs: number;
  observation: string;
};
assert.equal(data.outcome.status, "completed");
assert.equal(data.outcome.reasonCode, "mcp_reloaded");
assert.equal(result.oldToolCount, 2);
assert.equal(result.newToolCount, 2);
assert.deepEqual(result.addedTools, ["den_new_tool"]);
assert.deepEqual(result.removedTools, ["den_old_tool"]);
assert.equal(result.collisionCount, 0);
assert.equal(result.observation, "published");
assert.equal(manager.diagnostics("mcp-beta")?.status, "active");
assert.deepEqual(
  lifecycleAudit.events.map((event) => event.phase),
  ["reload_started", "reloaded"],
);
assert.equal(observationSink.events[0]?.event_type, "adapter_recovered");

const missingBinding = await reloadMcp({
  name: "reload_mcp",
  target: { sessionId: "session-missing" },
  actor: { operatorId: "operator-alpha" },
  requestId: "req-reload",
  reason: "missing",
  reasonCode: "slash_reload_mcp",
  body: {},
});
assert.equal(missingBinding.status, "failed");
assert.equal(missingBinding.reasonCode, "mcp_binding_not_found");

const mismatch = await createReloadMcpControlExecutor({
  resolveBinding: () => betaBinding,
  manager,
  discoveryClient: { listTools: () => [] },
  catalogId: () => "mcp:review",
})({
  name: "reload_mcp",
  target: { sessionId: "session-alpha" },
  actor: { operatorId: "operator-alpha" },
  requestId: "req-reload",
  reason: "mismatch",
  reasonCode: "slash_reload_mcp",
  body: {},
});
assert.equal(mismatch.status, "failed");
assert.equal(mismatch.reasonCode, "mcp_binding_session_mismatch");

console.log(
  JSON.stringify(
    {
      status: data.outcome.status,
      added: result.addedTools,
      removed: result.removedTools,
      betaStatus: manager.diagnostics("mcp-beta")?.status,
      auditPhases: lifecycleAudit.events.map((event) => event.phase),
      observation: observationSink.events[0]?.event_type,
      missingBinding: missingBinding.reasonCode,
      mismatch: mismatch.reasonCode,
    },
    null,
    2,
  ),
);

function binding(
  bindingId: string,
  agentId: string,
  sessionId: string,
  profileId: string,
): McpBindingRecord {
  return {
    bindingId,
    adapterId,
    agentId: agentId as AgentId,
    sessionId: sessionId as SessionId,
    profileId: profileId as ProfileId,
    serverNames: ["den"],
    endpointRef: `config://mcp/${bindingId}`,
    transport: "stdio",
    toolProfileKey: `${profileId}-mcp`,
    discoveredToolRevision: "rev-2",
    status: "active",
    diagnostics: {},
  };
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}
