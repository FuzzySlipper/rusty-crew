import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { Type } from "typebox";
import type { BrainTool, BrainWakeInput } from "./index.js";
import { defaultBodyDeltaPolicy } from "./mid-turn-delta.js";
import { toPiAgentTool } from "./pi-tool-adapter.js";

const parameters = Type.Object({
  subject: Type.String(),
});

const wake = fakeWake();
const partials: string[] = [];
const seen: {
  wakeId?: string;
  sessionId?: string;
  callId?: string;
  aborted?: boolean;
} = {};

const tool: BrainTool<typeof parameters, { subject: string; via: string }> = {
  name: "neutral_echo",
  label: "Neutral Echo",
  description: "Echo through the neutral BrainTool contract.",
  parameters,
  execute: async () => {
    throw new Error("expected pi adapter to prefer executeWithContext");
  },
  async executeWithContext(params, context) {
    seen.wakeId = context.wakeId;
    seen.sessionId = context.sessionId;
    seen.callId = context.callId;
    seen.aborted = context.signal.aborted;
    context.onUpdate?.({
      content: [{ type: "text", text: `partial:${params.subject}` }],
      details: { subject: params.subject, via: "partial" },
    });
    return {
      content: [{ type: "text", text: `done:${params.subject}` }],
      details: { subject: params.subject, via: "neutral" },
    };
  },
};

const piTool = toPiAgentTool(tool, { wake });
const result = await piTool.execute(
  "neutral-call-1",
  { subject: "rusty" },
  new AbortController().signal,
  (partial) => {
    const text = partial.content
      .filter((item) => item.type === "text")
      .map((item) => item.text)
      .join("");
    partials.push(text);
  },
);

assert.equal(piTool.name, "neutral_echo");
assert.equal(seen.wakeId, wake.wakeId);
assert.equal(seen.sessionId, wake.sessionId);
assert.equal(seen.callId, "neutral-call-1");
assert.equal(seen.aborted, false);
assert.deepEqual(partials, ["partial:rusty"]);
assert.deepEqual(result, {
  content: [{ type: "text", text: "done:rusty" }],
  details: { subject: "rusty", via: "neutral" },
});

console.log(
  JSON.stringify(
    {
      toolName: piTool.name,
      partials,
      result,
      context: seen,
    },
    null,
    2,
  ),
);

function fakeWake(): BrainWakeInput {
  const sessionId = "brain-tool-pi-adapter-session" as SessionId;
  return {
    wakeId: "brain-tool-pi-adapter-wake",
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "test neutral tools" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "brain-tool-agent" as AgentId,
        profileId: "brain-tool-profile" as ProfileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: { tools: [] },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-23T00:00:00Z",
        lastActiveAt: "2026-06-23T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    },
  };
}
