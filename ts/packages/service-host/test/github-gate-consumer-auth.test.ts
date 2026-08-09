import assert from "node:assert/strict";
import test from "node:test";

import { ReviewGitHubGateEventConsumer } from "@rusty-crew/adapter-den";

test("Review terminal-event polling authenticates through a remote Gateway", async () => {
  let request: Request | undefined;
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: new URL("http://den-gateway.test"),
    projectIds: async () => ["rusty-crew"],
    bearerToken: "gateway-review-token",
    waitMs: 0,
    bridge: {
      async consumeGitHubGateTerminalEvent() {
        throw new Error("no events expected");
      },
      async gitHubGateEventCursor() {
        return 0;
      },
      async recoverGitHubGateWakes() {
        return 0;
      },
    },
    fetch: async (input, init) => {
      request = new Request(input, init);
      return Response.json({ events: [], next_cursor: 0 });
    },
  });

  await consumer.hydrate();
  await consumer.pollOnce();

  assert.equal(
    request?.url,
    "http://den-gateway.test/v1/projects/rusty-crew/review/github-check-gate-events?after_id=0&limit=100&wait_ms=0",
  );
  assert.equal(
    request?.headers.get("authorization"),
    "Bearer gateway-review-token",
  );
});

test("Review terminal-event polling omits authorization for loopback deployments", async () => {
  let request: Request | undefined;
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: new URL("http://127.0.0.1:8096"),
    projectIds: async () => ["rusty-crew"],
    waitMs: 0,
    bridge: {
      async consumeGitHubGateTerminalEvent() {
        throw new Error("no events expected");
      },
      async gitHubGateEventCursor() {
        return 0;
      },
      async recoverGitHubGateWakes() {
        return 0;
      },
    },
    fetch: async (input, init) => {
      request = new Request(input, init);
      return Response.json({ events: [], next_cursor: 0 });
    },
  });

  await consumer.hydrate();
  await consumer.pollOnce();

  assert.equal(request?.headers.get("authorization"), null);
});

test("Review terminal-event polling visits every durable submission project scope", async () => {
  const requestedProjects: string[] = [];
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: new URL("http://den-gateway.test"),
    projectIds: async () => ["rusty-crew", "rusty-engine-demo"],
    waitMs: 0,
    bridge: {
      async consumeGitHubGateTerminalEvent() {
        throw new Error("no events expected");
      },
      async gitHubGateEventCursor() {
        return 0;
      },
      async recoverGitHubGateWakes() {
        return 0;
      },
    },
    fetch: async (input) => {
      requestedProjects.push(new URL(input.toString()).pathname);
      return Response.json({ events: [], next_cursor: 0 });
    },
  });

  await consumer.hydrate();
  await consumer.pollOnce();

  assert.deepEqual(requestedProjects, [
    "/v1/projects/rusty-crew/review/github-check-gate-events",
    "/v1/projects/rusty-engine-demo/review/github-check-gate-events",
  ]);
});

test("Review terminal-event polling discovers project scopes added after startup", async () => {
  let projectIds: readonly string[] = [];
  const requestedProjects: string[] = [];
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: new URL("http://den-gateway.test"),
    projectIds: async () => projectIds,
    waitMs: 0,
    bridge: {
      async consumeGitHubGateTerminalEvent() {
        throw new Error("no events expected");
      },
      async gitHubGateEventCursor() {
        return 0;
      },
      async recoverGitHubGateWakes() {
        return 0;
      },
    },
    fetch: async (input) => {
      requestedProjects.push(new URL(input.toString()).pathname);
      return Response.json({ events: [], next_cursor: 0 });
    },
  });

  await consumer.hydrate();
  await consumer.pollOnce();
  projectIds = ["rusty-engine-demo"];
  await consumer.pollOnce();

  assert.deepEqual(requestedProjects, [
    "/v1/projects/rusty-engine-demo/review/github-check-gate-events",
  ]);
});

test("Review terminal-event polling replays a project added during an active poll", async () => {
  let projectIds: readonly string[] = ["project-a"];
  const consumed: string[] = [];
  const requestedCursors: string[] = [];
  let firstProjectAPoll = true;
  const event = (id: number, projectId: string) => ({
    id,
    gate_id: id,
    project_id: projectId,
    task_id: id,
    commit_sha: String(id).padStart(40, "0"),
    status: "passed",
    terminal_reason: "checks_passed",
    completed_at: "2026-08-09T01:00:00Z",
  });
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: new URL("http://den-gateway.test"),
    projectIds: async () => projectIds,
    waitMs: 0,
    bridge: {
      async consumeGitHubGateTerminalEvent(value) {
        consumed.push(`${value.projectId}:${value.eventId}`);
        return {
          eventId: value.eventId,
          cursor: Math.max(
            ...consumed.map((item) => Number(item.split(":")[1])),
          ),
          duplicate: false,
          wakeScheduled: false,
        };
      },
      async gitHubGateEventCursor() {
        return 0;
      },
      async recoverGitHubGateWakes() {
        return 0;
      },
    },
    fetch: async (input) => {
      const url = new URL(input.toString());
      const projectId = url.pathname.split("/")[3];
      requestedCursors.push(`${projectId}:${url.searchParams.get("after_id")}`);
      if (projectId === "project-a" && firstProjectAPoll) {
        firstProjectAPoll = false;
        projectIds = ["project-a", "project-b"];
        return Response.json({ events: [event(2, projectId)], next_cursor: 2 });
      }
      if (projectId === "project-b") {
        return Response.json({ events: [event(1, projectId)], next_cursor: 1 });
      }
      return Response.json({ events: [], next_cursor: 2 });
    },
  });

  await consumer.hydrate();
  await consumer.pollOnce();
  await consumer.pollOnce();

  assert.deepEqual(consumed, ["project-a:2", "project-b:1"]);
  assert.deepEqual(requestedCursors, [
    "project-a:0",
    "project-a:2",
    "project-b:0",
  ]);
});
