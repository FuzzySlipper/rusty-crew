import assert from "node:assert/strict";
import test from "node:test";

import { ReviewGitHubGateEventConsumer } from "@rusty-crew/adapter-den";

test("Review terminal-event polling authenticates through a remote Gateway", async () => {
  let request: Request | undefined;
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: new URL("http://den-gateway.test"),
    projectId: "rusty-crew",
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
    projectId: "rusty-crew",
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
