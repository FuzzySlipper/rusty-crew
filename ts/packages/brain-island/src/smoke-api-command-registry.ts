import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  ADMIN_CONTROL_CAPABILITIES,
  API_CAPABILITIES,
  SLASH_COMMAND_REGISTRY,
  apiCapabilityRegistry,
  buildRuntimeDiagnosticsProjection,
  chatApiCapabilityPaths,
  chatCommandRegistry,
  handleAdminDiagnosticsRequest,
  routeSlashCommand,
  slashCommandNames,
  type RuntimeCounterSummary,
  type SlashCommandRouteResult,
  type SlashCommandSession,
} from "./index.js";

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

const adminCommandNames = ADMIN_CONTROL_CAPABILITIES.map(
  (capability) => capability.command_name,
);
assertUnique(adminCommandNames, "admin control command");
assert.equal(
  adminCommandNames.length,
  ADMIN_CONTROL_CAPABILITIES.length,
  "every admin control capability must declare a command_name",
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
