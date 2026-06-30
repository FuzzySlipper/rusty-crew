import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import { inspectDirectDebugSession } from "./direct-debug-service.js";
import { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import {
  effectiveSessionDefaults,
  effectiveWakeTimeoutMs,
  sessionWithProfileDefaults,
  type RustyCrewConfiguredSession,
} from "./service-runtime-config.js";

const profile = {
  profileId: "runner-profile" as ProfileId,
  modelConfig: {
    provider: "local" as const,
    modelName: "deterministic",
  },
  runtime: {
    maxTurnDurationMs: 180_000,
    defaultResourceLimits: {
      workdir: "/home/dev/rusty-crew",
      maxDurationMs: 30_000,
    },
  },
  sessionDefaults: {
    ownerId: "owner:profile",
    maxHistoryMessages: 200,
    turnTimeoutMs: 1_800_000,
  },
};
const profileContext = {
  profile,
  skills: [],
  toolSelection: {
    profileId: "runner-profile" as ProfileId,
    catalogId: "session-defaults-smoke",
    inventory: {
      selectedTools: [],
      selectedBindings: [],
      selectedDescriptors: [{ name: "read_file", description: "Read a file." }],
      items: [],
    },
    toolProfile: {
      tools: [{ name: "read_file", description: "Read a file." }],
    },
  },
};

const profileWithoutResourceLimits = {
  ...profileContext,
  profile: {
    ...profile,
    runtime: {
      ...profile.runtime,
      defaultResourceLimits: undefined,
    },
  },
};

const inherited = sessionWithProfileDefaults(
  {
    sessionId: "runner-session" as SessionId,
    agentId: "runner" as AgentId,
    profileId: "runner-profile" as ProfileId,
    kind: "full",
  },
  profileContext,
);
assert.equal(inherited.ownerId, "owner:profile");
assert.equal(inherited.maxHistoryMessages, 200);
assert.equal(inherited.turnTimeoutMs, 1_800_000);
assert.equal(inherited.resourceLimits?.workdir, "/home/dev/rusty-crew");
assert.equal(inherited.toolProfile?.tools[0]?.name, "read_file");

const serviceDefaultWorkdir = sessionWithProfileDefaults(
  {
    sessionId: "default-workdir-session" as SessionId,
    agentId: "runner" as AgentId,
    profileId: "runner-profile" as ProfileId,
    kind: "full",
  },
  profileWithoutResourceLimits,
  "/home",
);
assert.equal(serviceDefaultWorkdir.resourceLimits?.workdir, "/home");

const explicitSessionWorkdir = sessionWithProfileDefaults(
  {
    sessionId: "explicit-workdir-session" as SessionId,
    agentId: "runner" as AgentId,
    profileId: "runner-profile" as ProfileId,
    kind: "full",
    resourceLimits: { workdir: "/tmp/session-workdir" },
  },
  profileContext,
  "/home",
);
assert.equal(
  explicitSessionWorkdir.resourceLimits?.workdir,
  "/tmp/session-workdir",
);

const explicit = {
  sessionId: "explicit-session" as SessionId,
  agentId: "runner" as AgentId,
  profileId: "runner-profile" as ProfileId,
  kind: "full" as const,
  ownerId: "owner:service",
  maxHistoryMessages: 25,
  turnTimeoutMs: 45_000,
} satisfies RustyCrewConfiguredSession;
assert.deepEqual(effectiveSessionDefaults(explicit, profile), {
  ownerId: "owner:service",
  maxHistoryMessages: 25,
  turnTimeoutMs: 45_000,
});
assert.equal(effectiveWakeTimeoutMs({ session: explicit, profile }), 45_000);
assert.equal(effectiveWakeTimeoutMs({ profile }), 180_000);

const diagnostics = buildRuntimeDiagnosticsProjection({
  now: "2026-06-22T00:00:00.000Z",
  sessions: [session("explicit-session")],
  sessionDefaults: new Map([
    [
      "explicit-session" as SessionId,
      {
        ...effectiveSessionDefaults(explicit, profile),
        wakeTimeoutMs: effectiveWakeTimeoutMs({
          session: explicit,
          profile,
        }),
      },
    ],
  ]),
});
const sessionDiagnostics = diagnostics.runtime.sessions[0];
assert.equal(sessionDiagnostics?.effectiveDefaults?.ownerId, "owner:service");
assert.equal(sessionDiagnostics?.effectiveDefaults?.maxHistoryMessages, 25);
assert.equal(sessionDiagnostics?.effectiveDefaults?.turnTimeoutMs, 45_000);
assert.equal(sessionDiagnostics?.effectiveDefaults?.wakeTimeoutMs, 45_000);

const debug = inspectDirectDebugSession(
  { sessionId: "explicit-session" },
  {
    diagnostics,
    sessions: [
      {
        session: session("explicit-session"),
        profileContext,
        toolSelection: profileContext.toolSelection,
      },
    ],
    now: () => "2026-06-22T00:00:00.000Z",
  },
);
assert.equal(debug.ok, true);
if (!debug.ok) throw new Error("expected debug inspection to succeed");
assert.equal(debug.data.session.effectiveDefaults?.ownerId, "owner:service");
assert.equal(debug.data.session.effectiveDefaults?.wakeTimeoutMs, 45_000);

console.log(
  JSON.stringify(
    {
      inherited: inherited.ownerId,
      explicit: debug.data.session.effectiveDefaults,
      wakeTimeoutWithoutExplicit: effectiveWakeTimeoutMs({ profile }),
    },
    null,
    2,
  ),
);

function session(sessionId: string): SessionState {
  return {
    handle: 1 as SessionHandle,
    sessionId: sessionId as SessionId,
    agentId: "runner" as AgentId,
    profileId: "runner-profile" as ProfileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: {
      tools: [{ name: "read_file", description: "Read a file." }],
    },
    status: "idle",
    brainTurnCount: 0,
    createdAt: "2026-06-22T00:00:00.000Z",
    lastActiveAt: "2026-06-22T00:00:00.000Z",
  };
}
