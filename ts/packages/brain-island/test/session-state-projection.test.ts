import assert from "node:assert/strict";
import test from "node:test";
import type { SessionId, SessionState } from "@rusty-crew/contracts";
import { projectInFlightSessionState } from "../src/service-session-state-projection.js";

test("projects an in-flight idle session as active without mutating durable state", () => {
  const session = testSession("idle");
  const projected = projectInFlightSessionState(
    session,
    new Set([session.sessionId]),
  );

  assert.equal(session.status, "idle");
  assert.equal(projected.status, "active");
  assert.notEqual(projected, session);
});

test("does not reactivate archived sessions or unrelated idle sessions", () => {
  const archived = testSession("archived", "archived-session");
  const idle = testSession("idle", "idle-session");
  const inFlight = new Set<SessionId>([archived.sessionId]);

  assert.equal(projectInFlightSessionState(archived, inFlight), archived);
  assert.equal(projectInFlightSessionState(idle, inFlight), idle);
});

function testSession(
  status: SessionState["status"],
  id = "projection-session",
): SessionState {
  return {
    handle: 1 as never,
    sessionId: id as SessionId,
    agentId: "projection-agent" as never,
    profileId: "projection-profile" as never,
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
    inferenceOverrides: {},
    status,
    brainTurnCount: 0,
    createdAt: "2026-07-22T00:00:00.000Z",
    lastActiveAt: "2026-07-22T00:00:00.000Z",
  };
}
