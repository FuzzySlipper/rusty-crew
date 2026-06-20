import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  convertMcpToolsToCandidates,
  createMcpPiAgentTool,
  createSimulatedMcpTransportFactory,
  McpSurfaceManager,
  type McpToolExecutor,
} from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createMcpToolFinishedEvent,
  createMcpToolStartedEvent,
  evaluateMcpResourceHooks,
  integrateMcpToolsWithRegistry,
  reloadMcpSurface,
} from "./index.js";

const adapterId = "mcp-ts-main" as AdapterId;
const alphaBinding = mcpBinding(
  "mcp-alpha",
  "agent-alpha",
  "session-alpha",
  "prime",
  "alpha",
);
const betaBinding = mcpBinding(
  "mcp-beta",
  "agent-beta",
  "session-beta",
  "review",
  "beta",
);

const manager = new McpSurfaceManager({
  transports: [createSimulatedMcpTransportFactory("stdio")],
  now: () => "2026-06-20T11:50:00.000Z",
});
await manager.connect(alphaBinding);
await manager.connect(betaBinding);
assert.equal(manager.diagnostics("mcp-alpha")?.status, "active");
assert.equal(manager.diagnostics("mcp-beta")?.status, "active");

const alphaDiscovery = convertMcpToolsToCandidates(alphaBinding, [
  {
    name: "search",
    description: "Search alpha memory.",
    inputSchema: {
      type: "object",
      properties: { query: { type: "string", minLength: 1 } },
      required: ["query"],
    },
  },
]);
const betaDiscovery = convertMcpToolsToCandidates(betaBinding, [
  {
    name: "summarize",
    description: "Summarize beta context.",
    inputSchema: {
      type: "object",
      properties: { text: { type: "string", minLength: 1 } },
      required: ["text"],
    },
  },
]);

const alphaRegistry = integrateMcpToolsWithRegistry({
  catalogId: "mcp:prime",
  candidates: alphaDiscovery.candidates,
  inventoryRequest: { requestedToolsets: ["mcp:prime-mcp"] },
});
const betaRegistry = integrateMcpToolsWithRegistry({
  catalogId: "mcp:review",
  candidates: betaDiscovery.candidates,
  inventoryRequest: { requestedToolsets: ["mcp:review-mcp"] },
});
assert.equal(alphaRegistry.validation.ok, true);
assert.equal(betaRegistry.validation.ok, true);
assert.deepEqual(
  alphaRegistry.inventory?.selectedTools.map((tool) => tool.name),
  ["alpha_search"],
);
assert.deepEqual(
  betaRegistry.inventory?.selectedTools.map((tool) => tool.name),
  ["beta_summarize"],
);

const collisionRegistry = integrateMcpToolsWithRegistry({
  catalogId: "mcp:collision",
  candidates: [{ ...alphaDiscovery.candidates[0]!, name: "read_file" }],
  inventoryRequest: { requestedToolsets: ["mcp:prime-mcp"] },
});
assert.equal(collisionRegistry.validation.ok, false);
const namespacedCollision = integrateMcpToolsWithRegistry({
  catalogId: "mcp:collision",
  candidates: [{ ...alphaDiscovery.candidates[0]!, name: "read_file" }],
  inventoryRequest: { requestedToolsets: ["mcp:prime-mcp"] },
  nameCollisionPolicy: "prefix_source",
});
assert.equal(namespacedCollision.validation.ok, true);
assert.equal(namespacedCollision.mcpEntries[0]?.name, "alpha_read_file");

const deniedAcrossProfiles = evaluateMcpResourceHooks({
  binding: alphaBinding,
  candidate: alphaDiscovery.candidates[0]!,
  toolProfile: { tools: betaRegistry.inventory?.selectedDescriptors ?? [] },
});
assert.equal(deniedAcrossProfiles.allowed, false);
assert.equal(deniedAcrossProfiles.denialReason, "tool_profile_denied");

const calls: Array<{ bindingId: string; toolName: string }> = [];
const executor: McpToolExecutor = {
  callTool(input) {
    calls.push({
      bindingId: input.binding.bindingId,
      toolName: input.toolName,
    });
    return {
      content: `${input.binding.bindingId}:${input.toolName}`,
      details: { bindingId: input.binding.bindingId },
    };
  },
};
const alphaTool = createMcpPiAgentTool(
  alphaBinding,
  alphaDiscovery.candidates[0]!,
  executor,
);
const betaTool = createMcpPiAgentTool(
  betaBinding,
  betaDiscovery.candidates[0]!,
  executor,
);
await alphaTool.execute("call-alpha", { query: "status" });
await betaTool.execute("call-beta", { text: "review this" });
assert.deepEqual(calls, [
  { bindingId: "mcp-alpha", toolName: "search" },
  { bindingId: "mcp-beta", toolName: "summarize" },
]);

const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-mcp-surfaces-e2e-engine-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T11:51:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  await native.createSession({
    sessionId: "session-alpha",
    agentId: "agent-alpha",
    profileId: "prime",
    kind: "full",
  });
  await native.createSession({
    sessionId: "session-beta",
    agentId: "agent-beta",
    profileId: "review",
    kind: "full",
  });
  const events = await native.subscribeEvents({
    eventKinds: ["brain_event_observed", "external_event_injected"],
  });

  await native.injectExternalEvent({
    adapterId,
    source: "mcp:mcp-alpha",
    payload: alphaRegistry.catalogChangedPayload,
  });
  await native.injectExternalEvent({
    adapterId,
    source: "mcp:mcp-beta",
    payload: betaRegistry.catalogChangedPayload,
  });

  await submitMcpToolTelemetry(
    "wake-alpha",
    alphaBinding,
    alphaDiscovery.candidates[0]!,
  );
  await submitMcpToolTelemetry(
    "wake-beta",
    betaBinding,
    betaDiscovery.candidates[0]!,
  );

  const observed = await native.drainSubscriptionEvents(events, 12);
  const alphaToolStart = observed.find(
    (event) =>
      event.type === "brain_event_observed" &&
      event.event.type === "tool_call_started" &&
      event.event.toolName === "alpha_search",
  );
  const betaToolFinish = observed.find(
    (event) =>
      event.type === "brain_event_observed" &&
      event.event.type === "tool_call_finished" &&
      event.event.toolName === "beta_summarize",
  );
  assert.equal(alphaToolStart?.type, "brain_event_observed");
  assert.equal(alphaToolStart.event.type, "tool_call_started");
  assert.equal(alphaToolStart.event.metadata?.toolProfileKey, "prime-mcp");
  assert.equal(betaToolFinish?.type, "brain_event_observed");
  assert.equal(betaToolFinish.event.type, "tool_call_finished");
  assert.equal(betaToolFinish.event.metadata?.profileId, "review");
  assert.equal(await native.diagnosticCountRows("tool_call_history"), 4);

  const alphaReload = await reloadMcpSurface({
    binding: alphaBinding,
    manager,
    catalogId: "mcp:prime",
    previousToolNames: ["alpha_search"],
    inventoryRequest: { requestedToolsets: ["mcp:prime-mcp"] },
    requestedBy: "smoke",
    reason: "alpha catalog refresh",
    now: () => "2026-06-20T11:52:00.000Z",
    discoveryClient: {
      listTools: () => [
        {
          name: "search",
          description: "Search alpha memory.",
          inputSchema: true,
        },
        {
          name: "lookup",
          description: "Lookup alpha resources.",
          inputSchema: true,
        },
      ],
    },
  });
  assert.equal(alphaReload.status, "reloaded");
  assert.deepEqual(alphaReload.toolDiff.addedTools, ["alpha_lookup"]);
  assert.equal(manager.diagnostics("mcp-beta")?.status, "active");

  await native.unsubscribeEvents(events);

  console.log(
    JSON.stringify(
      {
        alphaTools: alphaRegistry.inventory?.selectedTools.map(
          (tool) => tool.name,
        ),
        betaTools: betaRegistry.inventory?.selectedTools.map(
          (tool) => tool.name,
        ),
        calls,
        toolTelemetryRows:
          await native.diagnosticCountRows("tool_call_history"),
        betaStatusAfterAlphaReload: manager.diagnostics("mcp-beta")?.status,
        collisionBlocked: !collisionRegistry.validation.ok,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

async function submitMcpToolTelemetry(
  wakeId: string,
  binding: McpBindingRecord,
  candidate: (typeof alphaDiscovery.candidates)[number],
): Promise<void> {
  await native.submitBrainEvent({
    wakeId,
    sessionId: binding.sessionId!,
    event: createMcpToolStartedEvent({
      binding,
      toolName: candidate.name,
      sourceToolName: candidate.source.sourceToolName,
      catalogRevision: candidate.source.catalogRevision,
      timeoutMs: 5_000,
    }),
  });
  await native.submitBrainEvent({
    wakeId,
    sessionId: binding.sessionId!,
    event: createMcpToolFinishedEvent({
      binding,
      toolName: candidate.name,
      sourceToolName: candidate.source.sourceToolName,
      catalogRevision: candidate.source.catalogRevision,
      isError: false,
      allowed: true,
      timeoutMs: 5_000,
    }),
  });
}

function mcpBinding(
  bindingId: string,
  agentId: string,
  sessionId: string,
  profileId: string,
  serverName: string,
): McpBindingRecord {
  return {
    bindingId,
    adapterId,
    agentId: agentId as AgentId,
    sessionId: sessionId as SessionId,
    profileId: profileId as ProfileId,
    serverNames: [serverName],
    endpointRef: `config://mcp/${serverName}`,
    transport: "stdio",
    toolProfileKey: `${profileId}-mcp`,
    discoveredToolRevision: `${profileId}-rev`,
    status: "active",
    diagnostics: {},
  };
}
