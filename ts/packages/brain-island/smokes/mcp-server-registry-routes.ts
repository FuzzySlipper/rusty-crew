import assert from "node:assert/strict";
import {
  handleAdminMcpServerRegistryRequest,
  mcpServerWriteFromBody,
  type AdminMcpServerRegistryRouteContext,
} from "../src/service-mcp-server-registry-routes.js";
import type { RustyCrewRuntimeConfig } from "../src/service-runtime-config.js";
import type { RustyCrewMcpServerConfig } from "../src/service-config.js";

const runtimeServer: RustyCrewMcpServerConfig = {
  id: "runtime-den",
  label: "Runtime Den",
  baseUrl: "http://127.0.0.1:5199/mcp",
  transport: "streamable_http",
  source: "runtime",
};

const runtimeConfig: Pick<
  RustyCrewRuntimeConfig,
  "mcpServers" | "mcpBindings"
> = {
  mcpServers: [runtimeServer],
  mcpBindings: [],
};
let runtimeConfigFile: Record<string, unknown> = {
  mcpServers: [runtimeServer],
};
const applyEvents: string[] = [];

const context: AdminMcpServerRegistryRouteContext = {
  config: () => ({
    mcp: {
      servers: [
        {
          id: "env-default",
          label: "Env Default",
          baseUrl: "http://127.0.0.1:5199/mcp",
          transport: "streamable_http",
          source: "env",
        },
      ],
    },
  }),
  runtimeConfig: () => runtimeConfig,
  async readRuntimeConfigFile() {
    return {
      value: runtimeConfigFile,
      array(key) {
        const value = runtimeConfigFile[key];
        if (value === undefined) {
          const created: unknown[] = [];
          runtimeConfigFile[key] = created;
          return created;
        }
        assert(Array.isArray(value), `${key} should be an array`);
        return value;
      },
    };
  },
  async writeRuntimeConfigFile(value) {
    runtimeConfigFile = value;
    runtimeConfig.mcpServers = value.mcpServers as RustyCrewMcpServerConfig[];
  },
  async applyRuntimeConfigFromDisk(input) {
    applyEvents.push(input.eventType);
    return { applied: true, eventType: input.eventType };
  },
  async withRuntimeConfigMutation(mutation) {
    return mutation();
  },
};

const getResult = await handleAdminMcpServerRegistryRequest(
  {
    method: "GET",
    url: new URL("http://rusty-crew.local/v1/admin/mcp/servers"),
    requestId: "req_get",
  },
  context,
);
assert.equal(getResult.status, 200);
assert.equal(getResult.body.ok, true);
assert.equal(getResult.body.data.servers.length, 2);

const createResult = await handleAdminMcpServerRegistryRequest(
  {
    method: "POST",
    url: new URL("http://rusty-crew.local/v1/admin/mcp/servers"),
    requestId: "req_post",
    body: {
      id: "profile-extra",
      label: "Profile Extra",
      baseUrl: "http://127.0.0.1:6200/mcp",
      requestTimeoutMs: 15_000,
    },
  },
  context,
);
assert.equal(createResult.status, 200);
assert.equal(createResult.body.ok, true);
assert.equal(createResult.body.data.status, "created");
assert.deepEqual(applyEvents, ["mcp_server_registry_updated"]);
assert(
  runtimeConfig.mcpServers.some((server) => server.id === "profile-extra"),
);

runtimeConfig.mcpBindings = [
  {
    bindingId: "binding-1",
    adapterId: "mcp",
    agentId: "agent",
    sessionId: "session",
    profileId: "profile",
    endpointRef: "config://mcp/profile-extra",
    transport: "streamable_http",
    serverNames: ["profile-extra"],
    toolProfileKey: "profile",
    status: "active",
  },
];
const blockedDelete = await handleAdminMcpServerRegistryRequest(
  {
    method: "DELETE",
    url: new URL("http://rusty-crew.local/v1/admin/mcp/servers/profile-extra"),
    requestId: "req_delete",
  },
  context,
);
assert.equal(blockedDelete.status, 409);
assert.equal(blockedDelete.body.ok, false);
assert.equal(
  blockedDelete.body.error.reason_code,
  "mcp_server_has_active_bindings",
);

assert.throws(
  () =>
    mcpServerWriteFromBody(
      {
        id: "wrong",
        baseUrl: "http://127.0.0.1:6200/mcp",
      },
      "right",
    ),
  /body id must match path id/,
);

console.log("mcp server registry route smoke passed");
