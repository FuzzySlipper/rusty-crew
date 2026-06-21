import assert from "node:assert/strict";
import type { SessionId } from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  createMemoryAgentActivityObservationSink,
  runDelegatedResourceCleanup,
} from "./index.js";

const observationSink = createMemoryAgentActivityObservationSink();
const result = await runDelegatedResourceCleanup({
  runtime: {
    async cleanupDelegatedResources() {
      return {
        cleanedAt: "2026-06-21T00:00:00.000Z",
        terminalArchived: ["delegated-terminal" as SessionId],
        orphanedArchived: ["delegated-orphan" as SessionId],
        expiredArchived: ["delegated-expired" as SessionId],
        resourcesReleased: 0,
      };
    },
  },
  adapters: [
    {
      adapter: "browser",
      cleanup: () => ({ adapter: "browser", released: 1, degraded: 0 }),
    },
    {
      adapter: "mcp",
      cleanup: () => ({ adapter: "mcp", released: 0, degraded: 1 }),
    },
  ],
  observation: {
    producer: new AgentActivityObservationProducer({
      sink: observationSink,
      required: true,
    }),
    identity: {
      profile: "operator",
      instance_id: "cleanup-loop",
      session_key: "cleanup-session",
    },
    workRef: { run_id: "cleanup-run-1" },
  },
});

assert.equal(result.runtime.terminalArchived.length, 1);
assert.equal(result.runtime.orphanedArchived.length, 1);
assert.equal(result.runtime.expiredArchived.length, 1);
assert.equal(result.adapters.length, 2);
assert.equal(result.observation.started, "published");
assert.equal(result.observation.terminal, "published");
assert.equal(observationSink.events.length, 2);
assert.equal(observationSink.events[0]?.event_type, "work_started");
assert.equal(observationSink.events[1]?.event_type, "work_completed");
assert.match(
  observationSink.events[1]?.payload.summary ?? "",
  /archived 3 session/,
);

console.log("delegated resource cleanup smoke passed");
