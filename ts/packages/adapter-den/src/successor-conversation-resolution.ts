import type { ChannelBindingRecord } from "@rusty-crew/contracts";

import type {
  DenSuccessorConversationMembership,
  DenSuccessorGatewayClient,
} from "./successor-gateway.js";

type DenConversationGatewayClient = Pick<
  DenSuccessorGatewayClient,
  | "listConversationChannels"
  | "createConversationChannel"
  | "listConversationMemberships"
>;

export interface DenConversationChannelResolution {
  channelId: number;
  projectId: string;
  slug: string;
}

export interface DenConversationChannelResolutionResult {
  resolutionsByBindingId: ReadonlyMap<string, DenConversationChannelResolution>;
  channelIdsByExternalId: ReadonlyMap<string, number>;
  membershipsByBindingId: ReadonlyMap<
    string,
    DenSuccessorConversationMembership
  >;
  membershipResolutionFailure?: string;
  createdCount: number;
}

export async function resolveDenConversationChannels(input: {
  client: DenConversationGatewayClient;
  bindings: readonly ChannelBindingRecord[];
  defaultProjectId: string;
}): Promise<DenConversationChannelResolutionResult> {
  const channelsByProjectId = new Map<
    string,
    Map<string, { id: number; slug: string }>
  >();
  const resolutionsByBindingId = new Map<
    string,
    DenConversationChannelResolution
  >();
  const channelIdsByExternalId = new Map<string, number>();
  let createdCount = 0;

  for (const binding of input.bindings) {
    const projectId = conversationProjectIdForBinding(
      binding,
      input.defaultProjectId,
    );
    const slug = binding.externalChannelId;
    if (binding.conversationChannelId !== undefined) {
      resolutionsByBindingId.set(binding.bindingId, {
        channelId: binding.conversationChannelId,
        projectId,
        slug,
      });
      channelIdsByExternalId.set(
        conversationExternalChannelKey(projectId, slug),
        binding.conversationChannelId,
      );
      continue;
    }

    let channelsBySlug = channelsByProjectId.get(projectId);
    if (channelsBySlug === undefined) {
      const channels = await input.client.listConversationChannels({
        projectId,
        limit: 100,
      });
      channelsBySlug = new Map(
        channels.map((channel) => [
          channel.slug,
          { id: channel.id, slug: channel.slug },
        ]),
      );
      channelsByProjectId.set(projectId, channelsBySlug);
    }

    const existing = channelsBySlug.get(slug);
    if (existing !== undefined) {
      resolutionsByBindingId.set(binding.bindingId, {
        channelId: existing.id,
        projectId,
        slug: existing.slug,
      });
      channelIdsByExternalId.set(
        conversationExternalChannelKey(projectId, slug),
        existing.id,
      );
      continue;
    }

    const channel = await input.client.createConversationChannel({
      slug,
      display_name: displayNameForConversationBinding(binding),
      kind: "agent_channel",
      project_id: projectId,
      created_by: "rusty-crew",
      visibility: "normal",
      settings: {
        adapter_id: binding.adapterId,
        binding_id: binding.bindingId,
        provider: binding.provider,
        profile_id: binding.profileId,
        agent_id: binding.agentId,
      },
    });
    createdCount += 1;
    channelsBySlug.set(channel.slug, { id: channel.id, slug: channel.slug });
    resolutionsByBindingId.set(binding.bindingId, {
      channelId: channel.id,
      projectId,
      slug: channel.slug,
    });
    channelIdsByExternalId.set(
      conversationExternalChannelKey(projectId, slug),
      channel.id,
    );
  }

  let membershipsByBindingId: ReadonlyMap<
    string,
    DenSuccessorConversationMembership
  > = new Map();
  let membershipResolutionFailure: string | undefined;
  try {
    membershipsByBindingId = await resolveDenConversationMemberships({
      client: input.client,
      bindings: input.bindings,
      defaultProjectId: input.defaultProjectId,
      resolutionsByBindingId,
    });
  } catch (error) {
    membershipResolutionFailure = errorMessage(
      error,
      "Den Conversation membership resolution failed",
    );
  }

  return {
    resolutionsByBindingId,
    channelIdsByExternalId,
    membershipsByBindingId,
    membershipResolutionFailure,
    createdCount,
  };
}

async function resolveDenConversationMemberships(input: {
  client: DenConversationGatewayClient;
  bindings: readonly ChannelBindingRecord[];
  defaultProjectId: string;
  resolutionsByBindingId: ReadonlyMap<string, DenConversationChannelResolution>;
}): Promise<ReadonlyMap<string, DenSuccessorConversationMembership>> {
  const projectIds = [
    ...new Set(
      input.bindings.map((binding) =>
        conversationProjectIdForBinding(binding, input.defaultProjectId),
      ),
    ),
  ];
  const memberships = (
    await Promise.all(
      projectIds.map((projectId) =>
        input.client.listConversationMemberships({
          projectId,
          includeLeft: true,
          limit: Math.max(100, input.bindings.length * 2),
        }),
      ),
    )
  ).flat();
  const membershipByChannelAndMember = new Map<
    string,
    DenSuccessorConversationMembership
  >();
  for (const membership of memberships) {
    const key = conversationMembershipKey(
      membership.channel_id,
      membership.member_identity,
    );
    const existing = membershipByChannelAndMember.get(key);
    if (preferConversationMembership(membership, existing)) {
      membershipByChannelAndMember.set(key, membership);
    }
  }

  const membershipsByBindingId = new Map<
    string,
    DenSuccessorConversationMembership
  >();
  for (const binding of input.bindings) {
    const resolution = input.resolutionsByBindingId.get(binding.bindingId);
    if (resolution === undefined) continue;
    const membership = membershipByChannelAndMember.get(
      conversationMembershipKey(resolution.channelId, binding.agentId),
    );
    if (membership !== undefined) {
      membershipsByBindingId.set(binding.bindingId, membership);
    }
  }
  return membershipsByBindingId;
}

function conversationProjectIdForBinding(
  binding: ChannelBindingRecord,
  defaultProjectId: string,
): string {
  return binding.conversationProjectId?.trim() || defaultProjectId;
}

function conversationExternalChannelKey(
  projectId: string,
  slug: string,
): string {
  return `${projectId}:${slug}`;
}

function displayNameForConversationBinding(
  binding: ChannelBindingRecord,
): string {
  return `${binding.agentId} (${binding.externalChannelId})`;
}

function conversationMembershipKey(
  channelId: number,
  memberIdentity: string,
): string {
  return `${channelId}:${memberIdentity}`;
}

function preferConversationMembership(
  candidate: DenSuccessorConversationMembership,
  existing: DenSuccessorConversationMembership | undefined,
): boolean {
  if (existing === undefined) return true;
  return (
    conversationMembershipRank(candidate.membership_status) >
    conversationMembershipRank(existing.membership_status)
  );
}

function conversationMembershipRank(status: string): number {
  switch (status) {
    case "active":
      return 3;
    case "invited":
      return 2;
    case "left":
      return 1;
    default:
      return 0;
  }
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
