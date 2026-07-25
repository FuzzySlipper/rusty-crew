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
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createBridgeToolMetadataPolicyValidator,
  defaultToolRegistry,
  integrateMcpToolsWithRegistry,
} from "../../packages/brain-island/src/index.js";

const native = await loadNativeBridge();
const metadataPolicyValidator = createBridgeToolMetadataPolicyValidator(native);

const externalMemoryToolNames = [
  "memory_recall",
  "memory_read",
  "memory_search",
  "memory_store",
  "memory_propose",
] as const;

for (const toolName of externalMemoryToolNames) {
  const description = defaultToolRegistry.get(toolName)?.description;
  assert.ok(
    description,
    `${toolName} must have model-facing registry metadata`,
  );
  assert.match(description, /configured external memory/i);
  assert.match(
    description,
    /not (?:a )?Den document, task, project, or guidance/i,
    `${toolName} must explicitly exclude Den planning surfaces`,
  );
  assert.match(
    description,
    /Den MCP planning tools/i,
    `${toolName} must direct Den planning requests to MCP`,
  );
}

const denBinding: McpBindingRecord = {
  bindingId: "mcp-den-planning",
  adapterId: "mcp-ts-main" as AdapterId,
  agentId: "asha-planner-agent" as AgentId,
  sessionId: "asha-planner-session" as SessionId,
  profileId: "asha-planner" as ProfileId,
  serverNames: ["den"],
  endpointRef: "config://mcp/den-planning",
  transport: "streamable_http",
  toolProfileKey: "den-planning",
  discoveredToolRevision: "rev-den-planning",
  status: "active",
  diagnostics: {},
};

const denPlanningDiscovery = convertMcpToolsToCandidates(denBinding, [
  {
    name: "read_document",
    description: "Read a Den document by project and slug.",
    inputSchema: {
      type: "object",
      properties: {
        project_id: { type: "string", minLength: 1 },
        slug: { type: "string", minLength: 1 },
      },
      required: ["project_id", "slug"],
    },
  },
  {
    name: "list_tasks",
    description: "List Den tasks for a project.",
    inputSchema: {
      type: "object",
      properties: { project_id: { type: "string", minLength: 1 } },
      required: ["project_id"],
    },
  },
  {
    name: "get_agent_guidance",
    description: "Resolve Den project guidance.",
    inputSchema: {
      type: "object",
      properties: { project_id: { type: "string", minLength: 1 } },
      required: ["project_id"],
    },
  },
]);

const unavailableExternalMemoryPlan = await native.planToolAvailability({
  selectedTools: [
    "memory_recall",
    "memory_read",
    "memory_search",
    "memory_store",
    "memory_propose",
    "dense_profile_memory",
    "memory_space_catalog",
    "memory_space_read",
    "session_search",
    "recall_lore",
    "search_lore",
    "get_scene_state",
    ...denPlanningDiscovery.candidates.map((candidate) => candidate.name),
  ],
  denMemory: {
    configured: true,
    clientAvailable: false,
    mode: "candidate",
    lastError: "external memory endpoint refused connection",
  },
});

assert.deepEqual(
  unavailableExternalMemoryPlan.selectedTools.filter((tool) =>
    tool.startsWith("memory_"),
  ),
  ["memory_space_catalog", "memory_space_read"],
);
assert.deepEqual(
  unavailableExternalMemoryPlan.omittedTools.map(
    (omission) => omission.toolName,
  ),
  externalMemoryToolNames,
);
assert.equal(
  unavailableExternalMemoryPlan.omittedTools[0]?.reasonCode,
  "memory_external_dependency_unavailable",
);
for (const required of [
  "dense_profile_memory",
  "memory_space_catalog",
  "memory_space_read",
  "session_search",
  "recall_lore",
  "search_lore",
  "get_scene_state",
  "den_read_document",
  "den_list_tasks",
  "den_get_agent_guidance",
]) {
  assert.ok(
    unavailableExternalMemoryPlan.selectedTools.includes(required),
    `external memory availability planner must not omit ${required}`,
  );
}

const metadataExternalMemoryPlan = await native.planToolAvailability({
  selectedTools: [
    "memory_recall",
    "memory_read",
    "memory_search",
    "memory_store",
    "memory_propose",
    "den_read_document",
    "den_list_tasks",
    "den_get_agent_guidance",
  ],
  denMemory: {
    configured: true,
    clientAvailable: true,
    mode: "metadata",
  },
});
assert.deepEqual(metadataExternalMemoryPlan.selectedTools, [
  "memory_recall",
  "memory_read",
  "memory_search",
  "den_read_document",
  "den_list_tasks",
  "den_get_agent_guidance",
]);
assert.deepEqual(
  metadataExternalMemoryPlan.omittedTools.map((omission) => omission.toolName),
  ["memory_store", "memory_propose"],
);

const mergedRegistry = await integrateMcpToolsWithRegistry({
  catalogId: "mcp:den-planning",
  candidates: denPlanningDiscovery.candidates,
  metadataPolicyValidator,
  inventoryRequest: {
    requestedToolsets: [
      "memory_external_read",
      "memory_external_write",
      "memory_profile",
      "planning_session",
      "roleplay_lore_read",
      "roleplay_scene_state",
      "mcp:den-planning",
    ],
    resourceDeniedTools: unavailableExternalMemoryPlan.omittedTools.map(
      (omission) => omission.toolName,
    ),
    resourceDeniedReasons: Object.fromEntries(
      unavailableExternalMemoryPlan.omittedTools.map((omission) => [
        omission.toolName,
        `${omission.reasonCode}: ${omission.message}`,
      ]),
    ),
  },
});

assert.equal(
  mergedRegistry.validation.ok,
  true,
  JSON.stringify(mergedRegistry.validation.issues),
);
const selectedToolNames =
  mergedRegistry.inventory?.selectedTools.map((tool) => tool.name) ?? [];
assert.deepEqual(selectedToolNames, [
  "dense_profile_memory",
  "memory_space_catalog",
  "memory_space_read",
  "recall_lore",
  "search_lore",
  "list_lore_layers",
  "get_lore_layer_config",
  "get_scene_state",
  "update_scene_state",
  "todo",
  "session_search",
  "den_read_document",
  "den_list_tasks",
  "den_get_agent_guidance",
]);
assert.equal(
  selectedToolNames.some((tool) => tool.startsWith("den_memory_")),
  false,
  "model-facing external memory tools must not use den_memory_* aliases",
);
assert.equal(
  selectedToolNames.includes("memory_search"),
  false,
  "unavailable external memory must be omitted before wake",
);
assert.equal(
  mergedRegistry.inventory?.items.find((item) => item.name === "memory_search")
    ?.status,
  "resource_denied",
);
assert.match(
  mergedRegistry.inventory?.items.find((item) => item.name === "memory_search")
    ?.reasons[0] ?? "",
  /memory_external_dependency_unavailable/,
);

const denDocumentTool = mergedRegistry.mcpEntries.find(
  (tool) => tool.name === "den_read_document",
);
assert.equal(denDocumentTool?.category, "mcp");
assert.equal(denDocumentTool?.surfaces.includes("mcp"), true);
assert.equal(denDocumentTool?.description.includes("document"), true);

for (const candidate of denPlanningDiscovery.candidates) {
  assertDenPlanningCandidate(candidate);
}

console.log(
  JSON.stringify(
    {
      selectedTools: selectedToolNames,
      omittedExternalMemory: unavailableExternalMemoryPlan.omittedTools.map(
        (omission) => omission.toolName,
      ),
      denPlanningTools: denPlanningDiscovery.candidates.map(
        (candidate) => candidate.name,
      ),
      catalogToolCount: defaultToolRegistry.entries.length,
    },
    null,
    2,
  ),
);

function assertDenPlanningCandidate(candidate: McpRegistryCandidate): void {
  assert.equal(
    candidate.name.startsWith("den_memory_"),
    false,
    `${candidate.name} must not look like a Den memory tool`,
  );
  assert.equal(candidate.source.serverNames.includes("den"), true);
  assert.equal(candidate.category, "mcp");
}
