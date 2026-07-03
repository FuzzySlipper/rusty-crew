import assert from "node:assert/strict";
import test from "node:test";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import type { AdminRouteResult } from "../src/admin-diagnostics-api.js";
import { handleAdminContextStrategiesRequest } from "../src/service-context-strategy-routes.js";
import {
  handleAdminMcpCatalogRequest,
  mcpServerCatalogEntries,
  mcpServerIdFromEndpointRef,
  type AdminMcpCatalogRouteContext,
} from "../src/service-mcp-catalog-routes.js";
import {
  handleSchedulerReadRequest,
  pageParams,
  scheduledJobStatusParam,
  scheduledRunStatusParam,
  scheduledRunTriggerParam,
} from "../src/service-scheduler-routes.js";
import { handleAdminToolsCatalogRequest } from "../src/service-tool-catalog-routes.js";

test("scheduler diagnostics routes validate methods and filters", async () => {
  const methodFailure = await handleSchedulerReadRequest(
    {
      method: "POST",
      requestId: "req-scheduler",
      url: new URL("http://local/v1/admin/scheduler/jobs"),
    },
    schedulerContext(),
  );
  assert.equal(methodFailure.status, 405);
  assert.equal(errorReason(methodFailure), "read_only_route");

  assert.equal(scheduledJobStatusParam("active"), "active");
  assert.equal(scheduledJobStatusParam("bogus"), "invalid");
  assert.equal(scheduledRunStatusParam("completed"), "completed");
  assert.equal(scheduledRunStatusParam("bogus"), "invalid");
  assert.equal(scheduledRunTriggerParam("manual"), "manual");
  assert.equal(scheduledRunTriggerParam("bogus"), "invalid");
  assert.deepEqual(
    pageParams(
      new URL("http://local/v1/admin/scheduler/jobs?limit=7&offset=2"),
    ),
    { limit: 7, offset: 2 },
  );

  const jobs = await handleSchedulerReadRequest(
    {
      method: "GET",
      requestId: "req-scheduler",
      url: new URL(
        "http://local/v1/admin/scheduler/jobs?status=paused&jobKind=cleanup&limit=5",
      ),
    },
    schedulerContext(),
  );
  assert.equal(jobs.status, 200);
  assert.deepEqual(okData<{ jobs: unknown[] }>(jobs).jobs, [
    {
      status: "paused",
      jobKind: "cleanup",
      limit: 5,
    },
  ]);

  const runs = await handleSchedulerReadRequest(
    {
      method: "GET",
      requestId: "req-scheduler",
      url: new URL(
        "http://local/v1/admin/scheduler/runs?status=failed&trigger=due&targetSessionId=session-a",
      ),
    },
    schedulerContext(),
  );
  assert.equal(runs.status, 200);
  assert.deepEqual(okData<{ runs: unknown[] }>(runs).runs, [
    {
      status: "failed",
      trigger: "due",
      targetSessionId: "session-a",
    },
  ]);
});

test("MCP catalog route merges configured servers and resolves bindings", async () => {
  const context: AdminMcpCatalogRouteContext = {
    config: {
      mcp: {
        baseUrl: "http://compat.example/mcp",
        servers: [
          {
            id: "alpha",
            label: "Alpha",
            baseUrl: "http://alpha.example/mcp",
            transport: "streamable_http" as const,
            source: "env" as const,
          },
        ],
      },
    },
    runtimeConfig: {
      mcpServers: [
        {
          id: "beta",
          label: "Beta",
          baseUrl: "http://beta.example/mcp",
          transport: "streamable_http" as const,
          requestTimeoutMs: 1000,
          source: "runtime" as const,
        },
      ],
      mcpBindings: [
        {
          bindingId: "binding-beta",
          adapterId: "mcp-ts-main" as never,
          agentId: "agent-a" as AgentId,
          sessionId: "session-a" as SessionId,
          profileId: "profile-a" as ProfileId,
          serverNames: ["beta"],
          endpointRef: "config://mcp/beta",
          transport: "streamable_http",
          toolProfileKey: "prime",
          status: "active",
          diagnostics: {},
        },
        {
          bindingId: "binding-compat",
          adapterId: "mcp-ts-main" as never,
          agentId: "agent-a" as AgentId,
          sessionId: "session-a" as SessionId,
          profileId: "profile-a" as ProfileId,
          serverNames: ["missing"],
          endpointRef: "config://mcp/missing",
          transport: "streamable_http",
          toolProfileKey: "review",
          status: "degraded",
          degradedReason: "missing server",
          diagnostics: {},
        },
      ],
    },
  };

  assert.equal(mcpServerIdFromEndpointRef("config://mcp/beta"), "beta");
  assert.equal(
    mcpServerIdFromEndpointRef("https://example.test/mcp"),
    undefined,
  );
  assert.deepEqual(
    mcpServerCatalogEntries(context).map((server) => server.id),
    ["alpha", "beta"],
  );

  const result = await handleAdminMcpCatalogRequest(
    { method: "GET", requestId: "req-mcp" },
    context,
  );
  const catalog = okData<{
    schemaVersion: number;
    servers: Array<{ id: string; configuredBindingCount: number }>;
    toolProfiles: string[];
    bindings: Array<{ resolvedServerId?: string }>;
  }>(result);
  assert.equal(result.status, 200);
  assert.equal(catalog.schemaVersion, 1);
  assert.deepEqual(
    catalog.servers.map((server) => [server.id, server.configuredBindingCount]),
    [
      ["alpha", 1],
      ["beta", 1],
    ],
  );
  assert.deepEqual(catalog.toolProfiles, ["prime", "review"]);
  assert.equal(catalog.bindings[0].resolvedServerId, "beta");
  assert.equal(catalog.bindings[1].resolvedServerId, "alpha");
});

test("tool and context catalog routes are read-only envelopes", async () => {
  const toolFailure = await handleAdminToolsCatalogRequest({
    method: "PATCH",
    requestId: "req-tools",
  });
  assert.equal(toolFailure.status, 405);
  assert.equal(errorReason(toolFailure), "tool_catalog_read_only");

  const toolCatalog = await handleAdminToolsCatalogRequest({
    method: "GET",
    requestId: "req-tools",
  });
  assert.equal(toolCatalog.status, 200);
  assert.equal(
    Array.isArray(okData<{ tools: unknown[] }>(toolCatalog).tools),
    true,
  );

  const contextFailure = await handleAdminContextStrategiesRequest({
    method: "POST",
    requestId: "req-context",
  });
  assert.equal(contextFailure.status, 405);
  assert.equal(
    errorReason(contextFailure),
    "context_strategy_catalog_read_only",
  );

  const contextCatalog = await handleAdminContextStrategiesRequest({
    method: "GET",
    requestId: "req-context",
  });
  assert.equal(contextCatalog.status, 200);
  assert.ok(
    Object.keys(
      okData<{ strategies: Record<string, unknown> }>(contextCatalog)
        .strategies,
    ).length > 0,
  );
});

function schedulerContext() {
  return {
    listScheduledJobs: async (input: unknown) => [input],
    listScheduledRuns: async (input: unknown) => [input],
  };
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  return result.body.data as T;
}

function errorReason(result: AdminRouteResult): string {
  assert.equal(result.body.ok, false);
  return result.body.error.reason_code;
}
