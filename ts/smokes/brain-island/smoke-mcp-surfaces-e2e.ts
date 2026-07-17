import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  convertMcpToolsToCandidates,
  createSimulatedMcpTransportFactory,
  McpSurfaceManager,
} from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  BodyState,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createMcpToolFinishedEvent,
  createMcpToolStartedEvent,
  createBridgeToolMetadataPolicyValidator,
  evaluateMcpResourceHooks,
  integrateMcpToolsWithRegistry,
  reloadMcpSurface,
} from "../../packages/brain-island/src/index.js";
import type { McpToolExecutor } from "../../packages/brain-island/src/service-adapter-ports.js";
import type { BrainTool } from "../../packages/brain-island/src/brain-tool.js";
import type { BrainWakeInput } from "../../packages/brain-island/src/brain-host-runtime.js";
import { createMcpBrainTool } from "../../packages/brain-island/src/mcp-brain-tools.js";
import {
  brainToolResultIsUnsuccessful,
  executePreparedBrainHostToolRequest,
  prepareBrainHostToolRequest,
} from "../../packages/brain-island/src/tool-execution-host.js";
import { MemoryToolCallDebugStore } from "../../packages/brain-island/src/tool-call-debug-store.js";

const metadataPolicyValidator = createBridgeToolMetadataPolicyValidator(
  await loadNativeBridge(),
);
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

const alphaRegistry = await integrateMcpToolsWithRegistry({
  catalogId: "mcp:prime",
  candidates: alphaDiscovery.candidates,
  metadataPolicyValidator,
  inventoryRequest: { requestedToolsets: ["mcp:prime-mcp"] },
});
const betaRegistry = await integrateMcpToolsWithRegistry({
  catalogId: "mcp:review",
  candidates: betaDiscovery.candidates,
  metadataPolicyValidator,
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

const collisionRegistry = await integrateMcpToolsWithRegistry({
  catalogId: "mcp:collision",
  candidates: [{ ...alphaDiscovery.candidates[0]!, name: "read_file" }],
  metadataPolicyValidator,
  inventoryRequest: { requestedToolsets: ["mcp:prime-mcp"] },
});
assert.equal(collisionRegistry.validation.ok, false);
const namespacedCollision = await integrateMcpToolsWithRegistry({
  catalogId: "mcp:collision",
  candidates: [{ ...alphaDiscovery.candidates[0]!, name: "read_file" }],
  metadataPolicyValidator,
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

const calls: Array<{
  bindingId: string;
  toolName: string;
  arguments: unknown;
}> = [];
const executor: McpToolExecutor = {
  callTool(input) {
    calls.push({
      bindingId: input.binding.bindingId,
      toolName: input.toolName,
      arguments: input.arguments,
    });
    return {
      content: `${input.binding.bindingId}:${input.toolName}`,
      details: { bindingId: input.binding.bindingId },
    };
  },
};
const alphaTool = createMcpBrainTool(
  alphaBinding,
  alphaDiscovery.candidates[0]!,
  executor,
);
const betaTool = createMcpBrainTool(
  betaBinding,
  betaDiscovery.candidates[0]!,
  executor,
);
await alphaTool.execute("call-alpha", { query: "status" });
await betaTool.execute("call-beta", { text: "review this" });
assert.deepEqual(calls, [
  {
    bindingId: "mcp-alpha",
    toolName: "search",
    arguments: { query: "status" },
  },
  {
    bindingId: "mcp-beta",
    toolName: "summarize",
    arguments: { text: "review this" },
  },
]);

const taskListDiscovery = convertMcpToolsToCandidates(alphaBinding, [
  {
    name: "list_tasks",
    description: "List tasks.",
    inputSchema: {
      type: "object",
      properties: {
        project_id: { type: "string" },
        assigned_to: { type: "string" },
        parent_id: { type: "integer", minimum: 1 },
        priority: { type: "integer", minimum: 1, maximum: 5 },
        verbose: { type: "boolean" },
        status: {
          type: ["string", "null"],
          description:
            "Filter by statuses (comma-separated): planned,in_progress,review,blocked,done,cancelled.",
        },
        tags: {
          type: ["string", "null"],
          description:
            "JSON array of string tags. Accepts a native JSON array or a JSON-encoded string for backward compatibility.",
        },
      },
      required: ["project_id"],
    },
  },
]);
const taskListTool = createMcpBrainTool(
  alphaBinding,
  taskListDiscovery.candidates[0]!,
  executor,
);
const normalizedTaskArgs = taskListTool.prepareArguments?.({
  project_id: "asha",
  status: ["planned", "in-progress"],
  tags: ["campaign", "planning"],
});
assert.deepEqual(normalizedTaskArgs, {
  project_id: "asha",
  status: "planned,in_progress",
  tags: '["campaign","planning"]',
});
const prunedTaskArgs = taskListTool.prepareArguments?.({
  assigned_to: "null",
  parent_id: 0,
  priority: 0,
  project_id: "asha",
  status: "planned,in_progress",
  tags: "null",
  verbose: false,
});
assert.deepEqual(prunedTaskArgs, {
  project_id: "asha",
  status: "planned,in_progress",
});

const failingTaskListTool = createMcpBrainTool(
  alphaBinding,
  taskListDiscovery.candidates[0]!,
  {
    callTool() {
      return {
        content: JSON.stringify({
          error: "den_backend_request_failed",
          retryable: false,
          backend: "documents",
          operation: "store_document",
          message: JSON.stringify({
            error: {
              code: "validation_failed",
              message: "project scope not found: _global",
            },
          }),
          status_code: 400,
        }),
        details: {
          isError: true,
          structuredContent: {
            error: "den_backend_request_failed",
            retryable: false,
            backend: "documents",
            operation: "store_document",
            message: JSON.stringify({
              error: {
                code: "validation_failed",
                message: "project scope not found: _global",
              },
            }),
            status_code: 400,
          },
        },
        isError: true,
      };
    },
  },
);
const unsuccessfulMcpResult = await failingTaskListTool.execute(
  "call-task-list-failure",
  {},
);
assert.equal(
  (unsuccessfulMcpResult.details as Record<string, unknown>).ok,
  false,
);
assert.equal(
  (unsuccessfulMcpResult.details as Record<string, unknown>).reasonCode,
  "den_backend_request_failed",
);
assert.equal(
  (unsuccessfulMcpResult.details as Record<string, unknown>).retryable,
  false,
);
assert.equal(
  (unsuccessfulMcpResult.details as Record<string, unknown>).message,
  "project scope not found: _global",
);
assert.equal(brainToolResultIsUnsuccessful(unsuccessfulMcpResult), true);

const toolCallDebugStore = new MemoryToolCallDebugStore();
const failureWake = {
  wakeId: "wake-mcp-structured-failure",
  sessionId: "session-alpha" as SessionId,
  systemPrompt: "MCP structured failure smoke",
  roleAssembly: {},
  state: {} as BodyState,
} as BrainWakeInput;
const preparedFailure = prepareBrainHostToolRequest(
  failureWake,
  {
    wakeId: failureWake.wakeId,
    callId: "call-mcp-structured-failure",
    name: failingTaskListTool.name,
    argumentsJson: "{}",
  },
  new Map([[failingTaskListTool.name, failingTaskListTool as BrainTool]]),
  toolCallDebugStore,
);
const structuredFailure = await executePreparedBrainHostToolRequest(
  failureWake,
  preparedFailure,
  toolCallDebugStore,
);
assert.equal(
  structuredFailure.failure?.reasonCode,
  "den_backend_request_failed",
);
assert.equal(structuredFailure.failure?.retryable, false);
assert.equal(structuredFailure.failure?.action, "failed");
assert.match(
  structuredFailure.failure?.detail ?? "",
  /project scope not found: _global/,
);
assert.match(structuredFailure.failure?.detail ?? "", /status=400/);
assert.match(structuredFailure.output, /den_backend_request_failed/);
assert.match(structuredFailure.output, /project scope not found: _global/);
const structuredFailureDebug = toolCallDebugStore.get({
  sessionId: failureWake.sessionId,
  debugDetailId: preparedFailure.debugDetailId ?? "missing",
});
assert.equal(structuredFailureDebug?.status, "failed");
assert.equal(
  (
    structuredFailureDebug?.final_result?.value as {
      details?: { retryable?: boolean };
    }
  )?.details?.retryable,
  false,
);
assert.match(
  structuredFailureDebug?.error?.message ?? "",
  /project scope not found: _global/,
);

const deniedMcpTool = createMcpBrainTool(
  alphaBinding,
  taskListDiscovery.candidates[0]!,
  {
    callTool() {
      return {
        content: "memory write requires manual review",
        details: {
          structuredContent: {
            action: "denied",
            reason_code: "memory_manual_review_required",
            retryable: false,
            operation: "memory_store",
            message: "memory write requires manual review",
          },
        },
        isError: true,
      };
    },
  },
);
const preparedDenial = prepareBrainHostToolRequest(
  failureWake,
  {
    wakeId: failureWake.wakeId,
    callId: "call-mcp-structured-denial",
    name: deniedMcpTool.name,
    argumentsJson: "{}",
  },
  new Map([[deniedMcpTool.name, deniedMcpTool as BrainTool]]),
  toolCallDebugStore,
);
const structuredDenial = await executePreparedBrainHostToolRequest(
  failureWake,
  preparedDenial,
  toolCallDebugStore,
);
assert.equal(
  structuredDenial.failure?.reasonCode,
  "memory_manual_review_required",
);
assert.equal(structuredDenial.failure?.action, "denied");
assert.equal(structuredDenial.failure?.retryable, false);
assert.match(structuredDenial.output, /memory write requires manual review/);

const transportFailureTool = createMcpBrainTool(
  alphaBinding,
  taskListDiscovery.candidates[0]!,
  {
    callTool() {
      throw new Error("MCP transport unavailable");
    },
  },
);
const preparedTransportFailure = prepareBrainHostToolRequest(
  failureWake,
  {
    wakeId: failureWake.wakeId,
    callId: "call-mcp-transport-failure",
    name: transportFailureTool.name,
    argumentsJson: "{}",
  },
  new Map([[transportFailureTool.name, transportFailureTool as BrainTool]]),
  toolCallDebugStore,
);
const transportFailure = await executePreparedBrainHostToolRequest(
  failureWake,
  preparedTransportFailure,
  toolCallDebugStore,
);
assert.equal(transportFailure.failure?.reasonCode, "tool_exception");
assert.equal(transportFailure.failure?.retryable, true);
assert.match(transportFailure.output, /MCP transport unavailable/);

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
    metadataPolicyValidator,
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
