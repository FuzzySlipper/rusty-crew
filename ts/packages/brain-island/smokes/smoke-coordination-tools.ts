import assert from "node:assert/strict";
import type {
  AgentId,
  AgentRouteKey,
  BrainAction,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  agentRoundTool,
  createCoordinationToolResolver,
  listAgentsTool,
  replyAgentMessageTool,
  sendAgentMessageTool,
  type CoordinationToolRuntime,
} from "../src/coordination-tools.js";
import { defaultBodyDeltaPolicy } from "../src/mid-turn-delta.js";
import { toChatCompletionsTool } from "./support/chat-completions-tool-adapter-test-harness.js";
import {
  buildBuiltInToolCatalog,
  defaultToolRegistry,
} from "../src/tool-registry.js";
import {
  resolveToolSession,
  type BrainActionCollector,
} from "../src/tool-session-selection.js";
import { selectToolProfile } from "../src/tool-profile-selection.js";
import type { BrainWakeInput } from "../src/index.js";

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
  async listAgents() {
    calls.push({ kind: "directory", input: {} });
    return [
      {
        agentId: "coordination-target",
        sessionId: "coordination-target-session" as SessionId,
        profileId: "coordination-target-profile" as ProfileId,
        displayLabel: "Coordination target",
        sessionKind: "full",
        sessionStatus: "idle",
        runtimeKind: "direct_brain",
        routable: true,
      },
    ];
  },
  async listRoutes() {
    return [
      {
        address: "@reviewer",
        routable: true,
        route: {
          routeKey: "reviewer" as AgentRouteKey,
          label: "Reviewer",
          enabled: true,
          target: {
            type: "direct_brain",
            agentId: "coordination-target" as AgentId,
            sessionId: "coordination-target-session" as SessionId,
          },
          requiredRuntimeKind: "direct_brain",
          revision: 3,
          createdAt: "2026-07-20T00:00:00Z",
          updatedAt: "2026-07-20T00:00:00Z",
        },
        resolvedTarget: {
          agentId: "coordination-target" as AgentId,
          sessionId: "coordination-target-session" as SessionId,
          profileId: "coordination-target-profile" as ProfileId,
          displayLabel: "Coordination target",
          runtimeKind: "direct_brain",
        },
      },
    ];
  },
  async routeMessage(input) {
    calls.push({ kind: "route", input });
    return {
      accepted: true,
      sequence: 7,
      wake: {
        status: "completed",
        wakeId: "wake-target",
        summary: `woke ${input.toAddress}`,
      },
      destination: {
        requestedAddress: input.toAddress,
        addressKind: input.toAddress.startsWith("@")
          ? "curated_route"
          : "raw_agent",
        agentId: "coordination-target",
        sessionId: "coordination-target-session",
        runtimeKind: "direct_brain",
        activation: "direct_brain_wake_requested",
      },
    };
  },
  async replyMessage(input) {
    calls.push({ kind: "reply", input });
    return {
      accepted: true,
      sequence: 9,
      wake: {
        status: "completed",
        summary: `replied to ${input.messageId}`,
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
        summary: `round woke ${input.toAddress}`,
      },
      reply: {
        from: input.toAddress,
        to: input.fromAgentId,
        body: `reply:${input.body}`,
        correlationId: input.correlationId,
      },
    };
  },
};

const directoryTool = listAgentsTool({ runtime });
const directoryResult = await directoryTool.execute?.("directory-call", {});
assert.equal(directoryResult?.details.ok, true);
assert.equal(
  directoryResult?.details.agents?.[0]?.agentId,
  "coordination-target",
);
assert.equal(directoryResult?.details.routes?.[0]?.address, "@reviewer");
assert.match(directoryResult?.content[0]?.text ?? "", /Switchboard routes/);
assert.match(directoryResult?.content[0]?.text ?? "", /Unrouted raw agents/);
assert.doesNotMatch(
  directoryResult?.content[0]?.text ?? "",
  /recipient=coordination-target(?:;|\n)/,
);

const ambiguousSend = await sendAgentMessageTool({
  runtime,
}).executeWithContext?.(
  { toAddress: "reviewer", body: "ambiguous" },
  {
    wake,
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    callId: "ambiguous-send-call",
    signal: new AbortController().signal,
  },
);
assert.equal(ambiguousSend?.details.ok, false);
assert.equal(
  ambiguousSend?.details.reasonCode,
  "ambiguous_agent_route_address",
);
assert.match(ambiguousSend?.content[0]?.text ?? "", /use @reviewer/);

const sendTool = sendAgentMessageTool({ runtime });
const sendResult = await sendTool.executeWithContext?.(
  {
    toAddress: "coordination-target",
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
assert.match(sendResult?.content[0]?.text ?? "", /agent=coordination-target/);
assert.match(
  sendResult?.content[0]?.text ?? "",
  /session=coordination-target-session/,
);
assert.deepEqual(
  calls.find((call) => call.kind === "route"),
  {
    kind: "route",
    input: {
      fromAgentId: "coordination-agent",
      fromSessionId: "coordination-session",
      wakeId: "coordination-wake",
      toolCallId: "send-call",
      toAddress: "coordination-target",
      body: "please wake",
      correlationId: "coordination-proof",
      requireWake: true,
    },
  },
);

const replyTool = replyAgentMessageTool({ runtime });
const replyResult = await replyTool.executeWithContext?.(
  {
    messageId: "routed-message-1",
    body: "review complete",
    ttlSeconds: 3_600,
  },
  {
    wake,
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    callId: "reply-call",
    signal: new AbortController().signal,
  },
);
assert.equal(replyResult?.details.ok, true);
assert.deepEqual(
  calls.find((call) => call.kind === "reply"),
  {
    kind: "reply",
    input: {
      fromAgentId: "coordination-agent",
      fromSessionId: "coordination-session",
      wakeId: "coordination-wake",
      toolCallId: "reply-call",
      messageId: "routed-message-1",
      body: "review complete",
      ttlSeconds: 3_600,
    },
  },
);

const roundTool = agentRoundTool({ runtime });
const roundResult = await roundTool.executeWithContext?.(
  {
    toAddress: "coordination-target",
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
    toAddress: "fallback-target",
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
  [
    "list_agents",
    "send_agent_message",
    "reply_agent_message",
    "agent_round",
    "submit_task_for_review",
    "rusty_crew_help",
  ],
);
const resolved = resolveToolSession({
  wake,
  toolProfile: selection.toolProfile,
  resolveTools: createCoordinationToolResolver(runtime),
});
assert.deepEqual(
  resolved.tools.map((tool) => tool.name),
  ["list_agents", "send_agent_message", "reply_agent_message", "agent_round"],
);

const piSendTool = toChatCompletionsTool(
  resolved.tools.find((tool) => tool.name === "send_agent_message")!,
  { wake },
);
const piSend = await piSendTool.execute("chat-send-call", {
  toAddress: "chat-adapted-target",
  body: "adapter keeps context",
  correlationId: "chat-adapter-proof",
});
assert.equal((piSend.details as { ok?: boolean }).ok, true);
assert.equal(
  (calls.at(-1)?.input as { fromAgentId?: string }).fromAgentId,
  "coordination-agent",
);

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
