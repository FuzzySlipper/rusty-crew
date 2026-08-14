import assert from "node:assert/strict";
import test from "node:test";

import type { McpBindingRecord, SessionState } from "@rusty-crew/contracts";
import {
  materializedBindingId,
  reconcileProfileMcpBindings,
  reconcileRuntimeProfileMcpBindings,
} from "../src/profile-mcp-reconciliation.js";

test("materializes distinct exact-session bindings for concurrent sessions", () => {
  const result = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "den", bindingId: "ambassador-den" }],
    sessions: [session("session-b"), session("session-a")],
    existing: [],
  });

  assert.deepEqual(
    result.materialized.map((binding) => ({
      bindingId: binding.bindingId,
      sessionId: binding.sessionId,
    })),
    [
      {
        bindingId: materializedBindingId("ambassador-den", "session-a"),
        sessionId: "session-a",
      },
      {
        bindingId: materializedBindingId("ambassador-den", "session-b"),
        sessionId: "session-b",
      },
    ],
  );
  assert.equal(result.changed, true);
  assert.equal(result.materialized.length, 2);
});

test("removes dangling, duplicate, and partial legacy materializations idempotently", () => {
  const exact = binding("ambassador-den--session--session-a", "session-a");
  const first = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "den", bindingId: "ambassador-den" }],
    sessions: [session("session-a")],
    existing: [
      exact,
      binding("ambassador-den", "deleted-session"),
      binding("ambassador-den-copy", "session-a"),
    ],
  });
  assert.deepEqual(first.removedBindingIds, [
    "ambassador-den",
    "ambassador-den-copy",
  ]);
  assert.equal(first.materialized.length, 1);

  const second = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "den", bindingId: "ambassador-den" }],
    sessions: [session("session-a")],
    existing: first.bindings,
  });
  assert.equal(second.changed, false);
  assert.deepEqual(second.removedBindingIds, []);
});

test("changing desired selection replaces every concurrent exact-session materialization", () => {
  const sessions = [session("session-a"), session("session-b")];
  const initial = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "den", bindingId: "ambassador-den" }],
    sessions,
    existing: [],
  });
  const changed = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "ops", bindingId: "ambassador-ops" }],
    sessions,
    existing: initial.bindings,
  });

  assert.deepEqual(changed.removedBindingIds, [
    "ambassador-den--session--session-a",
    "ambassador-den--session--session-b",
  ]);
  assert.deepEqual(
    changed.materialized.map(({ bindingId, sessionId, serverNames }) => ({
      bindingId,
      sessionId,
      serverNames,
    })),
    [
      {
        bindingId: "ambassador-ops--session--session-a",
        sessionId: "session-a",
        serverNames: ["ops"],
      },
      {
        bindingId: "ambassador-ops--session--session-b",
        sessionId: "session-b",
        serverNames: ["ops"],
      },
    ],
  );
});

test("persists intent without inventing a session when none is active", () => {
  const result = reconcileProfileMcpBindings({
    profileId: "reviewer",
    desired: [{ serverId: "den", bindingId: "reviewer-den" }],
    sessions: [],
    existing: [
      binding("reviewer-session-legacy", "reviewer-session", "reviewer"),
    ],
  });
  assert.deepEqual(result.materialized, []);
  assert.deepEqual(result.bindings, []);
  assert.equal(result.diagnostics.at(-1)?.code, "profile_mcp_binding_removed");
  assert.equal(
    (result.bindings as McpBindingRecord[]).some(
      (item) => item.sessionId === "reviewer-session",
    ),
    false,
  );
});

test("submits exact managed-external session identities to the Rust planner", async () => {
  const external = {
    ...session("external-session-1"),
    agentId: "reviewer-external-agent" as never,
    profileId: "reviewer" as never,
  };
  let plannedBindings: Array<{ sessionId?: string; agentId: string }> = [];
  const result = await reconcileRuntimeProfileMcpBindings({
    bridge: {
      async listSessions() {
        return [external];
      },
      async getProfileRegistryRecord() {
        return {
          profileId: "reviewer",
          lifecycleStatus: "active",
          defaultSessionKind: "full",
          activeRuntimeSettingsJson: {
            mcpBindings: [{ serverId: "den", bindingId: "reviewer-den" }],
          },
          sourceAssetRefs: [],
          derivedRuntimeRefs: [],
          importExport: { metadataJson: {} },
          revision: 2,
          createdAt: "2026-08-14T00:00:00.000Z",
          updatedAt: "2026-08-14T00:00:00.000Z",
        };
      },
      async planRuntimeConfig(input) {
        plannedBindings = input.runtimeConfig.mcpBindings;
        return {
          runtimeConfig: input.runtimeConfig,
          diagnostics: [],
          derivedScheduledJobs: [],
          derivedMcpBindings: [],
        };
      },
    },
    runtimeConfig: {
      profilesDir: "/profiles",
      brains: [],
      sessions: [],
      scheduledJobs: [],
      channelBindings: [],
      mcpBindings: [],
    },
  });

  assert.equal(result.profiles[0]?.materializedCount, 1);
  assert.deepEqual(plannedBindings, [
    {
      bindingId: "reviewer-den--session--external-session-1",
      adapterId: "mcp-ts-main",
      agentId: "reviewer-external-agent",
      instanceId: undefined,
      sessionId: "external-session-1",
      profileId: "reviewer",
      serverNames: ["den"],
      endpointRef: "config://mcp/den",
      transport: "streamable_http",
      toolProfileKey: "reviewer",
      status: "active",
    },
  ]);
});

function session(sessionId: string): SessionState {
  return {
    handle: 1 as never,
    sessionId: sessionId as never,
    agentId: "ambassador-agent" as never,
    profileId: "ambassador" as never,
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
    inferenceOverrides: {},
    status: "active",
    brainTurnCount: 0,
    createdAt: "2026-08-14T00:00:00.000Z",
    lastActiveAt: "2026-08-14T00:00:00.000Z",
  };
}

function binding(
  bindingId: string,
  sessionId: string,
  profileId = "ambassador",
): McpBindingRecord {
  return {
    bindingId,
    adapterId: "mcp-ts-main" as never,
    agentId: "ambassador-agent" as never,
    sessionId: sessionId as never,
    profileId: profileId as never,
    serverNames: ["den"],
    endpointRef: "config://mcp/den",
    transport: "streamable_http",
    toolProfileKey: profileId,
    status: "active",
    diagnostics: {},
  };
}
