import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { manifestOperationNames } from "@rusty-crew/contracts";
import {
  ADMIN_CONTROL_CAPABILITIES,
  API_CAPABILITIES,
  SERVICE_API_ROUTE_TABLE,
  SLASH_COMMAND_REGISTRY,
  apiCapabilityRegistry,
  buildRuntimeDiagnosticsProjection,
  chatApiCapabilityPaths,
  chatCommandAutocomplete,
  chatCommandRegistry,
  handleAdminDiagnosticsRequest,
  matchServiceApiRoute,
  routeSlashCommand,
  slashCommandNames,
  type RuntimeCounterSummary,
  type ServiceApiRouteId,
  type SlashCommandRouteResult,
  type SlashCommandSession,
} from "../src/index.js";

const primeSession: SlashCommandSession = {
  sessionId: "session-alpha",
  agentId: "agent-alpha",
  profileId: "prime",
  kind: "full",
};

const registry = apiCapabilityRegistry();
const commandNames = SLASH_COMMAND_REGISTRY.map((command) => command.name);
assert.deepEqual(slashCommandNames(), commandNames);
assert.deepEqual(
  chatCommandRegistry().commands.map((command) => command.name),
  commandNames,
);
assert.deepEqual(
  registry.slash_commands.map((command) => command.name),
  commandNames,
);
const chatCommands = chatCommandRegistry().commands;
for (const command of chatCommands) {
  assert.ok(
    command.args_schema,
    `missing legacy args_schema for ${command.name}`,
  );
  assert.ok(
    Array.isArray(command.positional_args),
    `missing positional args for ${command.name}`,
  );
  assert.ok(
    Array.isArray(command.named_args),
    `missing named args for ${command.name}`,
  );
  assert.ok(
    command.surfaces.includes("chat-input"),
    `missing chat-input surface for ${command.name}`,
  );
  assert.ok(command.source, `missing source for ${command.name}`);
}
const newCommand = chatCommands.find((command) => command.name === "new");
assert.ok(newCommand, "missing /new command");
assert.equal(newCommand.source, "backend-control");
assert.equal(newCommand.rust_plan_operation, "plan_new_session_control");
assert.deepEqual(newCommand.positional_args[0], {
  name: "reason",
  description: "Optional operator-facing reason text.",
  type: "string",
  required: false,
  placeholder: "reason",
});
const statusCommandDescriptor = chatCommands.find(
  (command) => command.name === "status",
);
assert.ok(statusCommandDescriptor, "missing /status command");
assert.equal(statusCommandDescriptor.source, "backend");
assert.deepEqual(
  chatCommandAutocomplete({ commandName: "new", argumentName: "reason" }),
  {
    command_name: "new",
    argument_name: "reason",
    provider: undefined,
    items: [],
    has_more: false,
  },
);
assert.equal(
  chatCommandAutocomplete({ commandName: "new", argumentName: "missing" }),
  undefined,
);

assertUnique(commandNames, "slash command name");
assertUnique(
  API_CAPABILITIES.map((capability) => capability.id),
  "API capability id",
);
assertUnique(
  API_CAPABILITIES.map(
    (capability) => `${capability.method} ${capability.path_template}`,
  ),
  "API capability route",
);
assertUnique(
  SERVICE_API_ROUTE_TABLE.map((route) => route.id),
  "service route id",
);
assertServiceRouteFamilyCoverage();
assert.deepEqual(
  SERVICE_API_ROUTE_TABLE.map((route) => route.order),
  [...SERVICE_API_ROUTE_TABLE.map((route) => route.order)].sort(
    (left, right) => left - right,
  ),
  "service route table must stay in dispatch order",
);
for (const capability of API_CAPABILITIES.filter(
  (candidate) => candidate.public,
)) {
  const samplePath = samplePathTemplate(capability.path_template);
  assert.ok(
    matchServiceApiRoute(samplePath),
    `missing service route table match for API capability ${capability.id}: ${samplePath}`,
  );
}
assert.equal(matchServiceApiRoute("/v1/unknown-route"), undefined);
assert.ok(
  API_CAPABILITIES.some(
    (capability) =>
      capability.id === "admin.tools.catalog" &&
      capability.method === "GET" &&
      capability.path_template === "/v1/admin/tools/catalog",
  ),
  "missing built-in tools catalog API capability",
);
assert.ok(
  API_CAPABILITIES.some(
    (capability) =>
      capability.id === "admin.local_tool_profiles.list" &&
      capability.method === "GET" &&
      capability.path_template === "/v1/admin/local-tool-profiles",
  ),
  "missing local tool profiles list API capability",
);
assert.ok(
  API_CAPABILITIES.some(
    (capability) =>
      capability.id === "admin.local_tool_profiles.update" &&
      capability.method === "PATCH" &&
      capability.path_template === "/v1/admin/local-tool-profiles/{profile_id}",
  ),
  "missing local tool profiles update API capability",
);

for (const command of SLASH_COMMAND_REGISTRY) {
  const routed = intercepted(
    routeSlashCommand({
      text: `/${command.name} smoke args`,
      session: primeSession,
      actor: { id: "human-alpha" },
    }),
  );
  assert.equal(routed.commandName, command.name);
  assert.equal(routed.status, "ok");
  const control = command.control;
  if (control) {
    assert.equal(routed.controlRequest?.commandName, control.commandName);
    assert.equal(routed.controlRequest?.reasonCode, control.reasonCode);
    assert.ok(
      ADMIN_CONTROL_CAPABILITIES.some(
        (capability) =>
          capability.command_name === control.commandName &&
          capability.path_template === control.pathTemplate,
      ),
      `missing admin capability for slash command ${command.name}`,
    );
  } else {
    assert.equal(routed.controlRequest, undefined);
  }
}
assertRustPlanOperationsInManifest(
  chatCommands
    .map((command) => command.rust_plan_operation)
    .filter(isPresentString),
  "slash command rust_plan_operation",
);

const adminCommandNames = ADMIN_CONTROL_CAPABILITIES.map(
  (capability) => capability.command_name,
);
assert.equal(
  adminCommandNames.every((commandName) => typeof commandName === "string"),
  true,
  "every admin control capability must declare a command_name",
);
assertUnique(
  ADMIN_CONTROL_CAPABILITIES.map((capability) => capability.id),
  "admin control capability id",
);
assertUnique(
  ADMIN_CONTROL_CAPABILITIES.map(
    (capability) => `${capability.method} ${capability.path_template}`,
  ),
  "admin control route",
);
assert.ok(
  ADMIN_CONTROL_CAPABILITIES.some(
    (capability) =>
      capability.id === "admin.control.config.wake_timeout.patch" &&
      capability.command_name === "patch_wake_timeout" &&
      capability.method === "POST" &&
      capability.path_template === "/v1/admin/control/config/wake-timeout",
  ),
  "wake-timeout patch capability must advertise the safe config write path",
);
const newSessionCapability = ADMIN_CONTROL_CAPABILITIES.find(
  (capability) => capability.id === "admin.control.sessions.new",
);
assert.equal(
  newSessionCapability?.rust_plan_operation,
  "plan_new_session_control",
);
assertRustPlanOperationsInManifest(
  API_CAPABILITIES.map((capability) => rustPlanOperation(capability)).filter(
    isPresentString,
  ),
  "API capability rust_plan_operation",
);

const contractPath = resolve(
  process.cwd(),
  "../../../docs/rusty-view-chat-api-v0.openapi.json",
);
const contract = JSON.parse(readFileSync(contractPath, "utf8")) as {
  paths: Record<string, unknown>;
};
assert.deepEqual(
  Object.keys(contract.paths).sort(),
  chatApiCapabilityPaths().sort(),
  "chat OpenAPI paths must match registered chat API capabilities",
);

const capabilitiesResponse = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/capabilities",
    requestId: "registry-smoke",
  },
  {
    diagnostics: buildRuntimeDiagnosticsProjection({
      now: "2026-06-24T00:00:00.000Z",
      runtimeSummary: emptyRuntimeCounters(),
      sessions: [],
      delegatedSessions: [],
      tools: [],
    }),
  },
);
assert.equal(capabilitiesResponse.status, 200);
assert.equal(capabilitiesResponse.body.ok, true);
if (!capabilitiesResponse.body.ok) throw new Error("expected ok response");
assert.deepEqual(capabilitiesResponse.body.data, registry);

console.log(
  JSON.stringify(
    {
      slashCommands: commandNames,
      apiCapabilities: API_CAPABILITIES.length,
      adminControls: ADMIN_CONTROL_CAPABILITIES.length,
      chatPaths: chatApiCapabilityPaths().length,
    },
    null,
    2,
  ),
);

function intercepted(
  result: SlashCommandRouteResult,
): Extract<SlashCommandRouteResult, { kind: "intercepted" }> {
  assert.equal(result.kind, "intercepted");
  if (result.kind !== "intercepted") throw new Error("expected interception");
  return result;
}

function assertUnique(values: readonly (string | undefined)[], label: string) {
  const seen = new Set<string>();
  for (const value of values) {
    assert.equal(typeof value, "string", `missing ${label}`);
    if (typeof value !== "string") continue;
    assert.equal(seen.has(value), false, `duplicate ${label}: ${value}`);
    seen.add(value);
  }
}

function assertServiceRouteFamilyCoverage(): void {
  const coveredOrExemptIds = new Set<ServiceApiRouteId>([
    ...serviceRouteFamilyCoverage().map((item) => item.routeId),
    ...serviceRouteCatalogExemptions().map((item) => item.routeId),
  ]);
  for (const route of SERVICE_API_ROUTE_TABLE) {
    assert.equal(
      coveredOrExemptIds.has(route.id),
      true,
      `service route family ${route.id} must have catalog coverage or an explicit exemption`,
    );
  }
  for (const requirement of serviceRouteFamilyCoverage()) {
    const capability = API_CAPABILITIES.find(
      (candidate) =>
        candidate.method === requirement.method &&
        candidate.path_template === requirement.pathTemplate,
    );
    assert.ok(
      capability,
      `missing API capability for service route family ${requirement.routeId}: ${requirement.method} ${requirement.pathTemplate}`,
    );
    assert.equal(
      matchServiceApiRoute(
        samplePathTemplate(requirement.pathTemplate),
        requirement.authPhase,
      )?.id,
      requirement.routeId,
      `service route family ${requirement.routeId} does not own representative catalog path ${requirement.pathTemplate}`,
    );
  }
}

function assertRustPlanOperationsInManifest(
  operations: readonly string[],
  label: string,
): void {
  const manifestOperations = new Set<string>(manifestOperationNames);
  for (const operation of operations) {
    assert.equal(
      manifestOperations.has(operation),
      true,
      `${label} ${operation} is not present in bridge manifest operation names`,
    );
  }
}

function isPresentString(value: string | undefined): value is string {
  return typeof value === "string" && value.length > 0;
}

function rustPlanOperation(capability: unknown): string | undefined {
  if (
    typeof capability !== "object" ||
    capability === null ||
    !("rust_plan_operation" in capability)
  ) {
    return undefined;
  }
  const operation = (capability as { rust_plan_operation?: unknown })
    .rust_plan_operation;
  return typeof operation === "string" ? operation : undefined;
}

function samplePathTemplate(pathTemplate: string): string {
  return pathTemplate.replace(/\{[^}]+\}/g, "sample");
}

function emptyRuntimeCounters(): RuntimeCounterSummary {
  return {
    brainTurns: 0,
    wakes: 0,
    toolCalls: 0,
    toolErrors: 0,
    delegationsCreated: 0,
    delegationsCompleted: 0,
    delegationsFailed: 0,
    delegationsTimedOut: 0,
    delegationsCancelled: 0,
    messages: 0,
    completions: 0,
    queueExpirations: 0,
  };
}

function serviceRouteFamilyCoverage(): readonly {
  routeId: ServiceApiRouteId;
  authPhase: "before_auth" | "after_auth";
  method: "DELETE" | "GET" | "PATCH" | "POST";
  pathTemplate: string;
}[] {
  return [
    {
      routeId: "admin.healthz",
      authPhase: "before_auth",
      method: "GET",
      pathTemplate: "/v1/admin/healthz",
    },
    {
      routeId: "admin.control",
      authPhase: "after_auth",
      method: "POST",
      pathTemplate: "/v1/admin/control/sessions/{session_id}/new",
    },
    {
      routeId: "chat",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/chat/sessions",
    },
    {
      routeId: "admin.scheduler",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/scheduler/jobs",
    },
    {
      routeId: "admin.mcp.servers",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/mcp/servers",
    },
    {
      routeId: "admin.tools.catalog",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/tools/catalog",
    },
    {
      routeId: "admin.brain_catalog",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/brains/catalog",
    },
    {
      routeId: "admin.context_strategies",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/context-strategies",
    },
    {
      routeId: "admin.local_tool_profiles",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/local-tool-profiles",
    },
    {
      routeId: "admin.storage",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/storage/schema",
    },
    {
      routeId: "admin.profile_registry.write",
      authPhase: "after_auth",
      method: "POST",
      pathTemplate: "/v1/admin/profiles/registry/{profile_id}/update/plan",
    },
    {
      routeId: "admin.memory",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/memory/spaces",
    },
    {
      routeId: "admin.diagnostics",
      authPhase: "after_auth",
      method: "GET",
      pathTemplate: "/v1/admin/diagnostics",
    },
  ];
}

function serviceRouteCatalogExemptions(): readonly {
  routeId: ServiceApiRouteId;
  reason: string;
}[] {
  return [
    {
      routeId: "browser.cors",
      reason:
        "preflight route only; cataloging each CORS path would duplicate browser routes",
    },
    {
      routeId: "debug",
      reason:
        "debug routes are intentionally omitted from public capability discovery",
    },
    {
      routeId: "admin.mcp.catalog",
      reason:
        "legacy MCP catalog route is route-table visible but not a Rusty View capability surface",
    },
    {
      routeId: "roleplay",
      reason:
        "roleplay browser/admin API catalog is being tracked separately while roleplay is migrated toward Rust crates",
    },
    {
      routeId: "admin.model_providers",
      reason:
        "model provider admin API still needs a dedicated catalog follow-up; keep the exemption visible until that lands",
    },
  ];
}
