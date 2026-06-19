import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { createLocalBrain } from "./index.js";

const sessionId = "smoke-session" as SessionId;

const brain = createLocalBrain();
const result = await brain.wake({
  wakeId: "smoke-wake-1",
  sessionId,
  systemPrompt: "You are a local smoke-test brain.",
  roleAssembly: {
    instructions: "Return a deterministic completion action.",
    initialMessages: [
      {
        from: "planner" as AgentId,
        to: "smoke-agent" as AgentId,
        body: "smoke",
      },
    ],
  },
  state: {
    session: {
      handle: 1 as SessionHandle,
      sessionId,
      agentId: "smoke-agent" as AgentId,
      profileId: "smoke-profile" as ProfileId,
      kind: "worker",
      resourceLimits: {},
      toolProfile: { tools: [] },
      status: "idle",
      brainTurnCount: 0,
      createdAt: "2026-06-19T00:00:00Z",
      lastActiveAt: "2026-06-19T00:00:00Z",
    },
    pendingMessages: [
      {
        from: "planner" as AgentId,
        to: "smoke-agent" as AgentId,
        body: "please smoke test",
      },
    ],
    recentEvents: [],
    deltaPolicy: {
      mode: "frozen_snapshot_next_wake",
      queueOwner: "body",
      queuedMessageTtlMs: 5_000,
      maxQueuedMessages: 32,
    },
  },
});

console.log(
  JSON.stringify(
    {
      eventTypes: result.events.map((event) => event.event.type),
      actionTypes: result.actions.map((action) => action.type),
    },
    null,
    2,
  ),
);
