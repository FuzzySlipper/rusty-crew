import assert from "node:assert/strict";
import type {
  CrewAgentSessionCreationRecord,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  archiveCrewSession,
  createFreshCrewSession,
  CrewSessionLifecycleError,
  type CrewSessionLifecycleContext,
} from "../src/service-crew-session-lifecycle.js";
import { handleRustyViewChatRequest } from "../src/rusty-view-chat-api.js";

const session = sessionState("session-alpha", "active");
const order: string[] = [];
let runtimeValue: Record<string, unknown> = {
  profilesDir: "/tmp/profiles",
  brains: [],
  sessions: [
    {
      sessionId: session.sessionId,
      agentId: session.agentId,
      profileId: session.profileId,
      kind: "full",
    },
  ],
  channelBindings: [{ bindingId: "channel-1", sessionId: session.sessionId }],
  mcpBindings: [{ bindingId: "mcp-1", sessionId: session.sessionId }],
  scheduledJobs: [{ id: "job-1", targetSessionId: session.sessionId }],
};
const context = lifecycleContext();
const archived = await archiveCrewSession(context, {
  sessionId: session.sessionId,
  commandName: "archive",
  requestId: "archive-request",
  actorId: "operator",
});
assert.deepEqual(order.slice(0, 3), ["write", "command_completed", "archive"]);
assert.equal(archived.session.status, "archived");
assert.equal(archived.commandEventCursor, "session-alpha:2");
assert.equal((runtimeValue.sessions as unknown[]).length, 0);
assert.equal(
  (runtimeValue.channelBindings as Array<Record<string, unknown>>)[0]
    ?.sessionId,
  undefined,
);
assert.equal(
  (runtimeValue.mcpBindings as Array<Record<string, unknown>>)[0]?.sessionId,
  undefined,
);
assert.equal((runtimeValue.scheduledJobs as unknown[]).length, 0);

const inFlightContext = lifecycleContext(new Set([session.sessionId]));
await assert.rejects(
  archiveCrewSession(inFlightContext, { sessionId: session.sessionId }),
  (error: unknown) =>
    error instanceof CrewSessionLifecycleError &&
    error.reasonCode === "crew_session_archive_in_flight",
);

runtimeValue = runtimeConfigWithSession();
const failedArchiveContext = lifecycleContext(new Set(), true);
await assert.rejects(
  archiveCrewSession(failedArchiveContext, { sessionId: session.sessionId }),
  (error: unknown) =>
    error instanceof CrewSessionLifecycleError &&
    error.reasonCode === "crew_session_archive_commit_failed",
);
assert.equal(
  (runtimeValue.sessions as Array<Record<string, unknown>>)[0]?.sessionId,
  session.sessionId,
  "failed native archive must restore the persisted runtime config",
);

runtimeValue = {
  profilesDir: "/tmp/profiles",
  brains: [],
  sessions: [],
  channelBindings: [],
  mcpBindings: [],
  scheduledJobs: [],
};
const creation = creationRecord();
const created = await createFreshCrewSession(lifecycleContext(), {
  idempotencyKey: "create-key",
  profileId: "prime" as never,
  expectedProfileRevision: 4,
  requestedAt: "2026-07-28T00:00:00Z",
});
assert.equal(created.creation.session.sessionId, creation.session.sessionId);
assert.equal((runtimeValue.sessions as unknown[]).length, 1);

const api = await handleRustyViewChatRequest(
  {
    method: "POST",
    url: "/v1/chat/sessions",
    headers: { "Idempotency-Key": "browser-create" },
    body: { profile_id: "prime", expected_profile_revision: 4 },
    requestId: "browser-request",
  },
  {
    listSessions: async () => [],
    createSession: async (input) => {
      assert.equal(input.profileId, "prime");
      assert.equal(input.expectedProfileRevision, 4);
      assert.equal(input.idempotencyKey, "browser-create");
      return { creation, applyResult: { sessionsAlreadyPresent: 1 } };
    },
  },
);
assert.equal(api.status, 200);
assert.equal(
  (api.body as { ok: boolean; data?: { creation?: { outcome?: string } } }).data
    ?.creation?.outcome,
  "created",
);

console.log(
  JSON.stringify({
    archiveOrder: order.slice(0, 4),
    archivedStatus: archived.session.status,
    createdSessionId: created.creation.session.sessionId,
    browserStatus: api.status,
  }),
);

function lifecycleContext(
  inFlightWakes: ReadonlySet<SessionId> = new Set(),
  failArchive = false,
): CrewSessionLifecycleContext {
  return {
    bridge: {
      archiveSession: async () => {
        order.push("archive");
        if (failArchive) throw new Error("native archive failed");
        return { ...session, status: "archived" } as never;
      },
      createCrewAgentSession: async () => creationRecord(),
      getProfileRegistryRecord: async () => undefined,
      updateProfileRegistryRecord: async () => {
        throw new Error("unexpected profile update");
      },
    },
    runtimeConfig: { profilesDir: "/tmp/profiles" } as never,
    serviceConfigFile: "/tmp/service.json",
    inFlightWakes,
    now: () => "2026-07-28T00:00:00Z",
    readRuntimeConfigFile: async () => ({
      value: runtimeValue,
      array(key) {
        const current = runtimeValue[key];
        if (Array.isArray(current)) return current;
        const created: unknown[] = [];
        runtimeValue[key] = created;
        return created;
      },
    }),
    validateRuntimeConfigFile: async () => ({ ok: true, diagnostics: [] }),
    writeRuntimeConfigFile: async (value) => {
      order.push("write");
      runtimeValue = value as Record<string, unknown>;
    },
    applyRuntimeConfigFromDisk: async () => ({}) as never,
    sessionById: async () => session,
    appendChatEvent: async (_sessionId, event) => {
      order.push(event.kind);
      return {
        event_id: "session-alpha:2",
        session_id: "session-alpha",
        sequence_id: 2,
        created_at: "2026-07-28T00:00:00Z",
        kind: event.kind,
        payload: event.payload,
      };
    },
  };
}

function runtimeConfigWithSession(): Record<string, unknown> {
  return {
    profilesDir: "/tmp/profiles",
    brains: [],
    sessions: [
      {
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: "full",
      },
    ],
    channelBindings: [],
    mcpBindings: [],
    scheduledJobs: [],
  };
}

function sessionState(
  sessionId: string,
  status: SessionState["status"],
): SessionState {
  return {
    handle: 1 as never,
    sessionId: sessionId as SessionId,
    agentId: "prime" as never,
    profileId: "prime" as never,
    kind: "full",
    delegation: null,
    resourceLimits: {
      workdir: "/home",
      maxDurationMs: null,
      maxDelegationDepth: null,
    },
    toolProfile: { tools: [] },
    historyWindow: null,
    inferenceOverrides: {},
    status,
    brainTurnCount: 0,
    createdAt: "2026-07-28T00:00:00Z",
    lastActiveAt: "2026-07-28T00:00:00Z",
  };
}

function creationRecord(): CrewAgentSessionCreationRecord {
  return {
    requestFingerprint: "fingerprint",
    profileRevision: 5,
    templateSessionId: null,
    outcome: "created",
    session: sessionState("crew-session-created", "active"),
  };
}
