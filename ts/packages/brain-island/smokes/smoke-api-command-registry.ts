import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { manifestOperationNames } from "@rusty-crew/contracts";
import {
  ADMIN_CONTROL_CAPABILITIES,
  API_CAPABILITY_OPENAPI_PATH,
  API_CAPABILITIES,
  SERVICE_API_ROUTE_TABLE,
  SLASH_COMMAND_REGISTRY,
  apiCapabilityRegistry,
  apiCapabilityCoverageInventory,
  apiCapabilityOpenApiDocument,
  buildRuntimeDiagnosticsProjection,
  chatApiCapabilityPaths,
  chatCommandAutocomplete,
  chatCommandRegistry,
  handleAdminDiagnosticsRequest,
  matchServiceApiRoute,
  routeSlashCommand,
  slashCommandHandlerNames,
  slashCommandNames,
  type RuntimeCounterSummary,
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
  slashCommandHandlerNames().sort(),
  [...commandNames].sort(),
  "slash command declarations and execution handlers must match in both directions",
);
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
const routeCoverage = apiCapabilityCoverageInventory();
assert.equal(
  routeCoverage.capability_routes.length,
  API_CAPABILITIES.filter((capability) => capability.public).length,
  "every public capability must resolve to one service route family",
);
assert.deepEqual(
  SERVICE_API_ROUTE_TABLE.map((route) => route.order),
  [...SERVICE_API_ROUTE_TABLE.map((route) => route.order)].sort(
    (left, right) => left - right,
  ),
  "service route table must stay in dispatch order",
);
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
  const commandText =
    command.name === "effort" ? "/effort high" : `/${command.name} smoke args`;
  const routed = intercepted(
    routeSlashCommand({
      text: commandText,
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
  paths: Record<string, { $ref?: string }>;
  components: { schemas: Record<string, { $ref?: string }> };
};
assert.deepEqual(
  Object.keys(contract.paths)
    .filter((path) => path.startsWith("/v1/chat/"))
    .sort(),
  chatApiCapabilityPaths().sort(),
  "chat OpenAPI paths must match registered chat API capabilities",
);
assert.equal(
  contract.paths["/v1/chat/commands"]?.$ref,
  "./rusty-crew-api-capabilities.openapi.json#/paths/~1v1~1chat~1commands",
  "chat command discovery must reference the generated capability OpenAPI",
);
assert.equal(
  contract.components.schemas.ChatCommandRegistry?.$ref,
  "./rusty-crew-api-capabilities.openapi.json#/components/schemas/ChatCommandRegistry",
  "chat command schemas must reference the generated capability OpenAPI",
);

const generatedOpenApiPath = resolve(
  process.cwd(),
  `../../../${API_CAPABILITY_OPENAPI_PATH}`,
);
const generatedOpenApi = JSON.parse(
  readFileSync(generatedOpenApiPath, "utf8"),
) as {
  openapi: string;
  paths: Record<string, Record<string, { operationId?: string }>>;
  components: { schemas: Record<string, unknown> };
};
assert.equal(generatedOpenApi.openapi, "3.1.0");
assert.deepEqual(
  openApiCapabilityRoutes(generatedOpenApi.paths),
  API_CAPABILITIES.filter((capability) => capability.public)
    .map((capability) => `${capability.method} ${capability.path_template}`)
    .sort(),
  "generated OpenAPI operations must match public API capabilities",
);
assert.equal(
  generatedOpenApi.paths["/v1/admin/capabilities"]?.get?.operationId,
  "listApiCapabilities",
);
assert.equal(
  generatedOpenApi.paths["/v1/chat/commands"]?.get?.operationId,
  "listChatCommands",
);
assert.deepEqual(generatedOpenApi, apiCapabilityOpenApiDocument());

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

function openApiCapabilityRoutes(
  paths: Record<string, Record<string, unknown>>,
): string[] {
  const methods = new Set(["delete", "get", "patch", "post"]);
  return Object.entries(paths)
    .flatMap(([path, pathItem]) =>
      Object.keys(pathItem)
        .filter((method) => methods.has(method))
        .map((method) => `${method.toUpperCase()} ${path}`),
    )
    .sort();
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
