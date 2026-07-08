import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  ChannelBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";

import {
  resolveDenConversationChannels,
  type DenSuccessorConversationMembership,
  type DenSuccessorGatewayClient,
} from "./index.js";

const calls: string[] = [];
const createdChannels: Array<{ slug: string; project_id?: string }> = [];

const client = {
  async listConversationChannels(input = {}) {
    calls.push(`channels:${input.projectId ?? "default"}`);
    if (input.projectId === "rusty-crew") {
      return [
        {
          id: 42,
          slug: "existing-room",
          display_name: "Existing Room",
          kind: "agent_channel",
          project_id: "rusty-crew",
          created_by: "den",
          visibility: "normal",
          created_at: "2026-07-08T00:00:00.000Z",
          updated_at: "2026-07-08T00:00:00.000Z",
        },
      ];
    }
    return [];
  },
  async createConversationChannel(input) {
    calls.push(`create:${input.project_id}:${input.slug}`);
    createdChannels.push({
      slug: input.slug,
      project_id: input.project_id,
    });
    return {
      id: 77,
      slug: input.slug,
      display_name: input.display_name,
      kind: input.kind,
      project_id: input.project_id,
      created_by: input.created_by,
      visibility: input.visibility,
      settings: input.settings,
      created_at: "2026-07-08T00:00:00.000Z",
      updated_at: "2026-07-08T00:00:00.000Z",
    };
  },
  async listConversationMemberships(input = {}) {
    calls.push(`memberships:${input.projectId ?? "default"}`);
    const memberships: DenSuccessorConversationMembership[] = [
      membership({
        id: 1,
        channel_id: 42,
        member_identity: "existing-agent",
        membership_status: "active",
      }),
      membership({
        id: 2,
        channel_id: 77,
        member_identity: "created-agent",
        membership_status: "left",
      }),
      membership({
        id: 3,
        channel_id: 77,
        member_identity: "created-agent",
        membership_status: "active",
      }),
      membership({
        id: 4,
        channel_id: 88,
        member_identity: "pinned-agent",
        membership_status: "invited",
      }),
    ];
    return memberships.filter(
      (item) =>
        input.projectId !== "rusty-crew-extra" || item.channel_id !== 42,
    );
  },
} satisfies Pick<
  DenSuccessorGatewayClient,
  | "listConversationChannels"
  | "createConversationChannel"
  | "listConversationMemberships"
>;

const bindings: ChannelBindingRecord[] = [
  channelBinding({
    bindingId: "existing-binding",
    agentId: "existing-agent",
    externalChannelId: "existing-room",
  }),
  channelBinding({
    bindingId: "created-binding",
    agentId: "created-agent",
    externalChannelId: "created-room",
    conversationProjectId: "rusty-crew-extra",
  }),
  channelBinding({
    bindingId: "pinned-binding",
    agentId: "pinned-agent",
    externalChannelId: "pinned-room",
    conversationChannelId: 88,
  }),
];

const result = await resolveDenConversationChannels({
  client,
  bindings,
  defaultProjectId: "rusty-crew",
});

assert.equal(result.createdCount, 1);
assert.deepEqual(createdChannels, [
  { slug: "created-room", project_id: "rusty-crew-extra" },
]);
assert.equal(
  result.resolutionsByBindingId.get("existing-binding")?.channelId,
  42,
);
assert.equal(
  result.resolutionsByBindingId.get("created-binding")?.channelId,
  77,
);
assert.equal(
  result.resolutionsByBindingId.get("pinned-binding")?.channelId,
  88,
);
assert.equal(result.channelIdsByExternalId.get("rusty-crew:existing-room"), 42);
assert.equal(
  result.channelIdsByExternalId.get("rusty-crew-extra:created-room"),
  77,
);
assert.equal(
  result.membershipsByBindingId.get("created-binding")?.membership_status,
  "active",
);
assert.equal(
  result.membershipsByBindingId.get("pinned-binding")?.membership_status,
  "invited",
);
assert.deepEqual(calls, [
  "channels:rusty-crew",
  "channels:rusty-crew-extra",
  "create:rusty-crew-extra:created-room",
  "memberships:rusty-crew",
  "memberships:rusty-crew-extra",
]);

const membershipFailureResult = await resolveDenConversationChannels({
  client: {
    ...client,
    async listConversationMemberships() {
      throw new Error("membership service down");
    },
  },
  bindings: [bindings[0]!],
  defaultProjectId: "rusty-crew",
});
assert.equal(
  membershipFailureResult.resolutionsByBindingId.get("existing-binding")
    ?.channelId,
  42,
);
assert.equal(membershipFailureResult.membershipsByBindingId.size, 0);
assert.equal(
  membershipFailureResult.membershipResolutionFailure,
  "membership service down",
);

console.log("successor conversation resolution smoke passed");

function channelBinding(
  input: {
    bindingId: string;
    agentId: string;
    externalChannelId: string;
  } & Partial<Omit<ChannelBindingRecord, "agentId">>,
): ChannelBindingRecord {
  return {
    provider: "den_channels",
    sessionId: `${input.agentId}-session` as SessionId,
    profileId: `${input.agentId}-profile` as ProfileId,
    externalUserId: `${input.agentId}-external`,
    status: "active",
    ...input,
    adapterId: (input.adapterId ?? "den-successor") as AdapterId,
    agentId: input.agentId as AgentId,
  };
}

function membership(input: {
  id: number;
  channel_id: number;
  member_identity: string;
  membership_status: string;
}): DenSuccessorConversationMembership {
  return {
    member_type: "agent",
    profile_identity: `${input.member_identity}-profile`,
    wake_policy: "on_message",
    can_send: true,
    can_react: true,
    can_invite: false,
    membership_purpose: "ordinary",
    created_at: "2026-07-08T00:00:00.000Z",
    updated_at: "2026-07-08T00:00:00.000Z",
    ...input,
  };
}
