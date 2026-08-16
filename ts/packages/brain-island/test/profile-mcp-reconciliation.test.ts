import assert from "node:assert/strict";
import test from "node:test";

import type { McpBindingRecord, SessionState } from "@rusty-crew/contracts";
import {
  materializedBindingId,
  reconcileProfileMcpBindings,
  reconcileRuntimeProfileMcpBindings,
} from "../src/profile-mcp-reconciliation.js";
import {
  profileMcpBindingsForRuntimeMutation,
  profileMcpBindingsFromRegistryRecord,
} from "../src/service-profile-runtime-mutations.js";

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

test("normalizes legacy nested materialization ids to one desired template", () => {
  const result = reconcileProfileMcpBindings({
    profileId: "rusty-engineer",
    desired: [
      {
        serverId: "den",
        bindingId:
          "rusty-engineer-den--session--old-session--session--new-session",
      },
    ],
    sessions: [
      {
        ...session("current-session"),
        agentId: "rusty-engineer" as never,
        profileId: "rusty-engineer" as never,
      },
    ],
    existing: [],
  });

  assert.equal(
    result.materialized[0]?.bindingId,
    "rusty-engineer-den--session--current-session",
  );
  assert.equal(
    result.materialized[0]?.diagnostics.desiredProfileBindingId,
    "rusty-engineer-den",
  );
});

test("profile mutation keeps desired intent instead of copying materialized runtime ids", () => {
  const record = {
    profileId: "rusty-engineer",
    lifecycleStatus: "active",
    defaultSessionKind: "full",
    agentId: "rusty-engineer",
    activeRuntimeSettingsJson: {
      mcpBindings: [
        {
          serverId: "den",
          bindingId: "rusty-engineer-den--session--legacy-session",
        },
      ],
    },
    sourceAssetRefs: [],
    derivedRuntimeRefs: [],
    importExport: { metadataJson: {} },
    revision: 1,
    createdAt: "2026-08-15T00:00:00.000Z",
    updatedAt: "2026-08-15T00:00:00.000Z",
  } as never;
  const runtime = [
    binding(
      "rusty-engineer-den--session--legacy-session--session--current-session",
      "current-session",
      "rusty-engineer",
    ),
  ];

  const identity = (items: Array<{ serverId: string; bindingId?: string }>) =>
    items.map(({ serverId, bindingId }) => ({ serverId, bindingId }));
  assert.deepEqual(identity(profileMcpBindingsFromRegistryRecord(record)), [
    { serverId: "den", bindingId: "rusty-engineer-den" },
  ]);
  assert.deepEqual(
    identity(profileMcpBindingsForRuntimeMutation(record, runtime)),
    [{ serverId: "den", bindingId: "rusty-engineer-den" }],
  );
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

test("archiving one concurrent session removes only that session materialization", () => {
  const active = session("session-active");
  const archived = {
    ...session("session-archived"),
    status: "archived" as const,
  };
  const initial = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "den", bindingId: "ambassador-den" }],
    sessions: [active, { ...archived, status: "active" }],
    existing: [],
  });

  const reconciled = reconcileProfileMcpBindings({
    profileId: "ambassador",
    desired: [{ serverId: "den", bindingId: "ambassador-den" }],
    sessions: [active, archived],
    existing: initial.bindings,
  });

  assert.deepEqual(reconciled.removedBindingIds, [
    "ambassador-den--session--session-archived",
  ]);
  assert.deepEqual(
    reconciled.materialized.map(({ bindingId, sessionId }) => ({
      bindingId,
      sessionId,
    })),
    [
      {
        bindingId: "ambassador-den--session--session-active",
        sessionId: "session-active",
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

test("validates configured sessions before startup materializes them", async () => {
  let plannedSessionIds: string[] = [];
  let plannedProfileIds: string[] = [];
  await reconcileRuntimeProfileMcpBindings({
    bridge: {
      async listSessions() {
        return [];
      },
      async getProfileRegistryRecord() {
        return undefined;
      },
      async planRuntimeConfig(input) {
        plannedSessionIds = input.runtimeConfig.sessions.map(
          (item) => item.sessionId,
        );
        plannedProfileIds = input.profiles.map((item) => item.profileId);
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
      sessions: [
        {
          sessionId: "configured-session",
          agentId: "configured-agent",
          profileId: "reviewer",
          kind: "full",
          workspaceCwd: "/home/dev/rusty-crew",
        },
      ],
      scheduledJobs: [],
      channelBindings: [],
      mcpBindings: [
        binding(
          "reviewer-den--session--configured-session",
          "configured-session",
        ),
      ],
    },
  });

  assert.deepEqual(plannedSessionIds, ["configured-session"]);
  assert.deepEqual(plannedProfileIds.sort(), ["ambassador", "reviewer"]);
});

test("omits archived recovery sessions from the active Rust runtime plan", async () => {
  const archived = {
    ...session("external-session-recovery"),
    profileId: "reviewer" as never,
    status: "archived" as const,
  };
  let plannedSessionIds: string[] = [];
  await reconcileRuntimeProfileMcpBindings({
    bridge: {
      async listSessions() {
        return [archived];
      },
      async getProfileRegistryRecord() {
        return undefined;
      },
      async planRuntimeConfig(input) {
        plannedSessionIds = input.runtimeConfig.sessions.map(
          (item) => item.sessionId,
        );
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

  assert.deepEqual(plannedSessionIds, []);
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
