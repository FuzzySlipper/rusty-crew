import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import { inspectDirectDebugSession } from "../src/direct-debug-service.js";
import { buildRuntimeDiagnosticsProjection } from "../src/runtime-diagnostics.js";
import {
  effectiveSessionDefaults,
  sessionWithProfileDefaults,
  type RustyCrewConfiguredSession,
} from "../src/service-runtime-config.js";

const profile = {
  profileId: "runner-profile" as ProfileId,
  modelConfig: {
    provider: "local" as const,
    modelName: "deterministic",
  },
  runtime: {
    defaultResourceLimits: {
      maxDurationMs: 30_000,
    },
  },
  sessionDefaults: {
    ownerId: "owner:profile",
    maxHistoryMessages: 200,
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
assert.equal(inherited.resourceLimits?.maxDurationMs, 30_000);
assert.equal(inherited.toolProfile?.tools[0]?.name, "read_file");

const explicit = {
  sessionId: "explicit-session" as SessionId,
  agentId: "runner" as AgentId,
  profileId: "runner-profile" as ProfileId,
  kind: "full" as const,
  ownerId: "owner:service",
  maxHistoryMessages: 25,
} satisfies RustyCrewConfiguredSession;
assert.deepEqual(effectiveSessionDefaults(explicit, profile), {
  ownerId: "owner:service",
  maxHistoryMessages: 25,
});

const diagnostics = buildRuntimeDiagnosticsProjection({
  now: "2026-06-22T00:00:00.000Z",
  sessions: [session("explicit-session")],
  sessionDefaults: new Map([
    [
      "explicit-session" as SessionId,
      effectiveSessionDefaults(explicit, profile),
    ],
  ]),
});
const sessionDiagnostics = diagnostics.runtime.sessions[0];
assert.equal(sessionDiagnostics?.effectiveDefaults?.ownerId, "owner:service");
assert.equal(sessionDiagnostics?.effectiveDefaults?.maxHistoryMessages, 25);
assert.equal(
  "turnTimeoutMs" in (sessionDiagnostics?.effectiveDefaults ?? {}),
  false,
);
assert.equal(
  "wakeTimeoutMs" in (sessionDiagnostics?.effectiveDefaults ?? {}),
  false,
);

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
assert.equal(
  "wakeTimeoutMs" in (debug.data.session.effectiveDefaults ?? {}),
  false,
);

console.log(
  JSON.stringify(
    {
      inherited: inherited.ownerId,
      explicit: debug.data.session.effectiveDefaults,
      finiteTurnLifetimeReported: false,
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
    workspace: {
      cwd: "/home/dev/rusty-crew",
      revision: 1,
      updatedAt: "2026-06-22T00:00:00.000Z",
    },
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
