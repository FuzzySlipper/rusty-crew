import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createInMemoryChannelReadbackStore,
  normalizeDenChannelsInboundEvent,
} from "@rusty-crew/adapter-den";
import { channelReadbackTool, defaultToolRegistry } from "./index.js";

const adapterId = "den-adapter" as AdapterId;
const agentAlpha = "agent-alpha" as AgentId;
const agentBeta = "agent-beta" as AgentId;
const sessionAlpha = "session-alpha" as SessionId;
const profileAlpha = "profile-alpha" as ProfileId;
const now = "2026-06-20T07:00:00Z";

const store = createInMemoryChannelReadbackStore({
  now: () => now,
  maxLimit: 3,
  defaultMaxBodyChars: 16,
});

store.record(
  normalizeDenChannelsInboundEvent(
    {
      kind: "channel.message.created",
      channel: { id: "channel-a" },
      thread: { id: "thread-a" },
      message: {
        id: "m1",
        text: "First alpha channel note with a long enough body to truncate.",
        createdAt: "2026-06-20T06:50:00Z",
      },
      author: { id: "user-1", displayName: "User One" },
      cursor: "1",
    },
    {
      adapterId,
      bindingId: "binding-alpha",
      agentId: agentAlpha,
      sessionId: sessionAlpha,
      profileId: profileAlpha,
      ttlMs: 30 * 60 * 1_000,
    },
  ),
);
store.record(
  normalizeDenChannelsInboundEvent(
    {
      kind: "channel.message.created",
      channel: { id: "channel-a" },
      thread: { id: "thread-a" },
      message: {
        id: "m2",
        text: "Second alpha channel note.",
        createdAt: "2026-06-20T06:55:00Z",
      },
      author: { id: "user-2" },
      cursor: "2",
    },
    {
      adapterId,
      bindingId: "binding-alpha",
      agentId: agentAlpha,
      sessionId: sessionAlpha,
      profileId: profileAlpha,
      ttlMs: 30 * 60 * 1_000,
    },
  ),
);
store.record(
  normalizeDenChannelsInboundEvent(
    {
      kind: "channel.message.created",
      channel: { id: "channel-a" },
      thread: { id: "thread-a" },
      message: {
        id: "m3",
        text: "Expired alpha note should be hidden.",
        createdAt: "2026-06-20T05:00:00Z",
      },
      author: { id: "user-3" },
      cursor: "3",
    },
    {
      adapterId,
      bindingId: "binding-alpha",
      agentId: agentAlpha,
      sessionId: sessionAlpha,
      profileId: profileAlpha,
      ttlMs: 1_000,
    },
  ),
);
store.record(
  normalizeDenChannelsInboundEvent(
    {
      kind: "channel.message.created",
      channel: { id: "channel-b" },
      message: {
        id: "m4",
        text: "Beta binding should not leak.",
        createdAt: "2026-06-20T06:58:00Z",
      },
      author: { id: "user-4" },
      cursor: "4",
    },
    {
      adapterId,
      bindingId: "binding-beta",
      agentId: agentBeta,
      ttlMs: 30 * 60 * 1_000,
    },
  ),
);

const tool = channelReadbackTool({
  client: store,
  requester: {
    agentId: agentAlpha,
    sessionId: sessionAlpha,
    profileId: profileAlpha,
  },
  allowedBindingIds: ["binding-alpha"],
  maxLimit: 3,
  defaultMaxBodyChars: 16,
});

const result = await tool.execute("read-alpha", {
  bindingId: "binding-alpha",
  externalChannelId: "channel-a",
  limit: 10,
});
assert.equal(result.details.ok, true);
assert.equal(result.details.response?.messages.length, 2);
assert.equal(result.details.response?.truncated, false);
assert.equal(result.details.response?.messages[0]?.bodySnippet.length, 16);
assert.equal(result.details.response?.messages[0]?.truncated, true);
assert.deepEqual(
  result.details.response?.messages.map(
    (message) => message.providerRefs.externalMessageId,
  ),
  ["m1", "m2"],
);

const denied = await tool.execute("read-beta", {
  bindingId: "binding-beta",
});
assert.equal(denied.details.ok, false);
assert.equal(denied.details.reasonCode, "channel_readback_binding_denied");

const missingRequester = await channelReadbackTool({
  client: store,
  requester: {},
  allowedBindingIds: ["binding-alpha"],
}).execute("missing-requester", {
  bindingId: "binding-alpha",
});
assert.equal(missingRequester.details.ok, false);
assert.equal(
  missingRequester.details.reasonCode,
  "channel_readback_requester_missing",
);

const registryEntry = defaultToolRegistry.resolve("channel_readback");
assert.equal(
  registryEntry?.implementationModule,
  "./planning-tools.js#channelReadbackTool",
);

console.log(
  JSON.stringify(
    {
      messages: result.details.response?.messages.length,
      newestCursor: result.details.response?.cursorBoundaries.newestCursor,
      denied: denied.details.reasonCode,
      registry: registryEntry?.outputShape,
    },
    null,
    2,
  ),
);
