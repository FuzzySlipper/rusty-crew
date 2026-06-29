import assert from "node:assert/strict";
import type {
  AgentId,
  BrainAction,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  agentRoundTool,
  createCoordinationToolResolver,
  isCorrelatedReply,
  replyFromEvent,
  sendAgentMessageTool,
  type CoordinationToolRuntime,
} from "./coordination-tools.js";
import { defaultBodyDeltaPolicy } from "./mid-turn-delta.js";
import { toPiAgentTool } from "./pi-tool-adapter.js";
import {
  buildBuiltInToolCatalog,
  defaultToolRegistry,
} from "./tool-registry.js";
import {
  resolveToolSession,
  type BrainActionCollector,
} from "./tool-session-selection.js";
import { selectToolProfile } from "./tool-profile-selection.js";
import type { BrainWakeInput } from "./index.js";

class MemoryActionCollector implements BrainActionCollector {
  readonly actions: BrainAction[] = [];

  add(action: BrainAction): void {
    this.actions.push(action);
  }

  addMany(actions: readonly BrainAction[]): void {
    this.actions.push(...actions);
  }
}

const wake = fakeWake();
const calls: Array<{ kind: string; input: unknown }> = [];
const runtime: CoordinationToolRuntime = {
  async routeMessage(input) {
    calls.push({ kind: "route", input });
    return {
      accepted: true,
      sequence: 7,
      wake: {
        status: "completed",
        wakeId: "wake-target",
        summary: `woke ${input.toAgentId}`,
      },
    };
  },
  async roundTrip(input) {
    calls.push({ kind: "round", input });
    return {
      accepted: true,
      sequence: 8,
      wake: {
        status: "completed",
        wakeId: "wake-round-target",
        summary: `round woke ${input.toAgentId}`,
      },
      reply: {
        from: input.toAgentId,
        to: input.fromAgentId,
        body: `reply:${input.body}`,
        correlationId: input.correlationId,
      },
    };
  },
};

const sendTool = sendAgentMessageTool({ runtime });
const sendResult = await sendTool.executeWithContext?.(
  {
    toAgentId: "coordination-target",
    body: "please wake",
    correlationId: "coordination-proof",
  },
  {
    wake,
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    callId: "send-call",
    signal: new AbortController().signal,
  },
);
assert.equal(sendResult?.details.ok, true);
assert.deepEqual(calls[0], {
  kind: "route",
  input: {
    fromAgentId: "coordination-agent",
    toAgentId: "coordination-target",
    body: "please wake",
    correlationId: "coordination-proof",
    requireWake: true,
  },
});

const roundTool = agentRoundTool({ runtime });
const roundResult = await roundTool.executeWithContext?.(
  {
    toAgentId: "coordination-target",
    body: "one round please",
    correlationId: "round-proof",
    timeoutMs: 250,
  },
  {
    wake,
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    callId: "round-call",
    signal: new AbortController().signal,
  },
);
assert.equal(roundResult?.details.ok, true);
assert.equal(roundResult?.details.round?.reply?.body, "reply:one round please");

const collector = new MemoryActionCollector();
const fallbackTool = sendAgentMessageTool({ actions: collector });
const fallback = await fallbackTool.executeWithContext?.(
  {
    toAgentId: "fallback-target",
    body: "post-turn route",
    correlationId: "fallback-proof",
  },
  {
    wake,
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    callId: "fallback-call",
    signal: new AbortController().signal,
  },
);
assert.equal(fallback?.details.ok, true);
assert.deepEqual(collector.actions, [
  {
    type: "send_message",
    message: {
      from: "coordination-agent",
      to: "fallback-target",
      body: "post-turn route",
      correlationId: "fallback-proof",
    },
  },
]);

const selection = selectToolProfile({
  profileId: "coordination-profile" as ProfileId,
  policy: { requestedToolsets: ["agent_coordination"] },
});
assert.deepEqual(
  selection.toolProfile.tools.map((tool) => tool.name),
  ["send_agent_message", "agent_round"],
);
const resolved = resolveToolSession({
  wake,
  toolProfile: selection.toolProfile,
  resolveTools: createCoordinationToolResolver(runtime),
});
assert.deepEqual(
  resolved.tools.map((tool) => tool.name),
  ["send_agent_message", "agent_round"],
);

const piSendTool = toPiAgentTool(resolved.tools[0]!, { wake });
const piSend = await piSendTool.execute("pi-send-call", {
  toAgentId: "pi-adapted-target",
  body: "adapter keeps context",
  correlationId: "pi-adapter-proof",
});
assert.equal((piSend.details as { ok?: boolean }).ok, true);
assert.equal(
  (calls.at(-1)?.input as { fromAgentId?: string }).fromAgentId,
  "coordination-agent",
);

const replyEvent = {
  type: "agent_message_routed",
  message: {
    from: "coordination-target" as AgentId,
    to: "coordination-agent" as AgentId,
    body: "confirmed",
    correlationId: "round-proof",
  },
} as const;
assert.equal(
  isCorrelatedReply(replyEvent, {
    fromAgentId: "coordination-agent",
    toAgentId: "coordination-target",
    correlationId: "round-proof",
  }),
  true,
);
assert.deepEqual(replyFromEvent(replyEvent), {
  from: "coordination-target",
  to: "coordination-agent",
  body: "confirmed",
  correlationId: "round-proof",
});

const catalog = buildBuiltInToolCatalog(defaultToolRegistry);
assert.ok(
  catalog.toolsets.some((toolset) => toolset.id === "agent_coordination"),
);
assert.ok(
  defaultToolRegistry
    .buildInventory({ requestedToolsets: ["full_agent"] })
    .selectedTools.some((tool) => tool.name === "agent_round"),
);

console.log(
  JSON.stringify(
    {
      selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
      runtimeCalls: calls.length,
      fallbackActions: collector.actions.length,
      fullAgentIncludesRound: true,
    },
    null,
    2,
  ),
);

function fakeWake(): BrainWakeInput {
  const sessionId = "coordination-session" as SessionId;
  return {
    wakeId: "coordination-wake",
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "test coordination tools" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "coordination-agent" as AgentId,
        profileId: "coordination-profile" as ProfileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: { tools: [] },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-28T00:00:00Z",
        lastActiveAt: "2026-06-28T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    },
  };
}
