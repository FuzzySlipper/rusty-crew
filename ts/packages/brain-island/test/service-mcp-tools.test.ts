import assert from "node:assert/strict";
import test from "node:test";
import type {
  AdapterId,
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { buildServiceMcpToolCatalog } from "../src/service-mcp-tools.js";

const profileId = "reviewer" as ProfileId;

function binding(
  templateId: string,
  sessionId: string,
  agentId: string,
): McpBindingRecord {
  return {
    bindingId: `${templateId}--session--${sessionId}`,
    adapterId: "mcp-ts-main" as AdapterId,
    agentId: agentId as AgentId,
    sessionId: sessionId as SessionId,
    profileId,
    serverNames: ["den"],
    endpointRef: "config://mcp/den",
    transport: "streamable_http",
    toolProfileKey: "reviewer",
    discoveredToolRevision: "den-revision",
    status: "active",
    diagnostics: {},
  };
}

function session(sessionId: string, agentId: string): SessionState {
  return {
    sessionId: sessionId as SessionId,
    agentId: agentId as AgentId,
    profileId,
  } as SessionState;
}

const bridge = {
  validateToolMetadataPolicy: () => ({ ok: true, diagnostics: [] }),
} as unknown as Pick<NativeBridgeModule, "validateToolMetadataPolicy">;

const discoveryClientFactory = () => ({
  listTools: () => [
    {
      name: "get_task",
      description: "Read one Den task",
      inputSchema: {
        type: "object",
        properties: { task_id: { type: "number" } },
        required: ["task_id"],
      },
    },
  ],
});

test("same authored MCP binding remains callable in concurrent profile sessions", async () => {
  const first = binding("reviewer-den", "session-a", "agent-a");
  const second = binding("reviewer-den", "session-b", "agent-b");
  const catalog = await buildServiceMcpToolCatalog({
    bridge,
    runtimeConfig: { mcpBindings: [first, second] },
    discoveryClientFactory,
  });

  assert.ok(catalog.registryForProfile(profileId));
  assert.deepEqual(
    catalog
      .candidatesForSession(session("session-a", "agent-a"))
      .map(({ binding, candidate }) => [binding.bindingId, candidate.name]),
    [[first.bindingId, "den_get_task"]],
  );
  assert.deepEqual(
    catalog
      .candidatesForSession(session("session-b", "agent-b"))
      .map(({ binding, candidate }) => [binding.bindingId, candidate.name]),
    [[second.bindingId, "den_get_task"]],
  );
});

test("distinct authored bindings still expose a real model-name collision", async () => {
  const catalog = await buildServiceMcpToolCatalog({
    bridge,
    runtimeConfig: {
      mcpBindings: [
        binding("reviewer-den-primary", "session-a", "agent-a"),
        binding("reviewer-den-secondary", "session-a", "agent-a"),
      ],
    },
    discoveryClientFactory,
  });

  assert.equal(catalog.registryForProfile(profileId), undefined);
  assert.deepEqual(
    catalog.candidatesForSession(session("session-a", "agent-a")),
    [],
  );
});
