import type {
  AgentId,
  ChannelBindingRecord,
  NormalizedChannelInboundMessage,
} from "@rusty-crew/contracts";

export type ChannelRouteResolution =
  | {
      status: "routed";
      route: ChannelRouteRequest;
      binding: ChannelBindingRecord;
    }
  | {
      status: "no_binding" | "ambiguous" | "inactive_binding";
      reason: string;
      candidates: ChannelBindingRecord[];
      message: NormalizedChannelInboundMessage;
    };

export interface ChannelRouteRequest {
  from: AgentId;
  to: AgentId;
  body: string;
  correlationId: string;
  bindingId: string;
  sessionId?: string;
}

export interface ChannelRoutingOptions {
  systemAgentId?: AgentId;
  mentionAliases?: Record<string, AgentId>;
}

export function resolveChannelRoute(
  message: NormalizedChannelInboundMessage,
  bindings: readonly ChannelBindingRecord[],
  options: ChannelRoutingOptions = {},
): ChannelRouteResolution {
  const matchingSurface = bindings
    .filter((binding) => binding.status === "active")
    .filter((binding) => binding.provider === message.providerRefs.provider)
    .filter(
      (binding) =>
        binding.externalChannelId === message.providerRefs.externalChannelId,
    )
    .filter(
      (binding) =>
        message.providerRefs.externalThreadId === undefined ||
        binding.externalThreadId === undefined ||
        binding.externalThreadId === message.providerRefs.externalThreadId,
    );

  if (matchingSurface.length === 0) {
    const inactiveCandidates = bindings.filter(
      (binding) =>
        binding.provider === message.providerRefs.provider &&
        binding.externalChannelId === message.providerRefs.externalChannelId,
    );
    return {
      status: inactiveCandidates.length > 0 ? "inactive_binding" : "no_binding",
      reason:
        inactiveCandidates.length > 0
          ? "matching channel bindings are not active"
          : "no active channel binding matches provider/channel",
      candidates: inactiveCandidates,
      message,
    };
  }

  const explicitBinding = matchingSurface.filter(
    (binding) => binding.bindingId === message.bindingId,
  );
  const mentionTargets = mentionedAgentIds(message, options.mentionAliases);
  const mentionedBindings =
    mentionTargets.length > 0
      ? matchingSurface.filter((binding) =>
          mentionTargets.includes(binding.agentId),
        )
      : [];
  const runtimeBinding =
    message.runtime.agentId === undefined
      ? []
      : matchingSurface.filter(
          (binding) => binding.agentId === message.runtime.agentId,
        );

  const candidates = firstNonEmpty([
    explicitBinding,
    mentionedBindings,
    runtimeBinding,
    matchingSurface.length === 1 ? matchingSurface : [],
  ]);

  if (candidates.length === 0 || candidates.length > 1) {
    return {
      status: "ambiguous",
      reason:
        candidates.length > 1
          ? "multiple bindings matched channel route"
          : "multiple bindings share this channel and no mention/runtime binding disambiguated them",
      candidates: candidates.length > 1 ? candidates : matchingSurface,
      message,
    };
  }

  const binding = candidates[0]!;
  return {
    status: "routed",
    binding,
    route: {
      from:
        options.systemAgentId ??
        (`channel:${message.providerRefs.provider}:${message.author.externalUserId}` as AgentId),
      to: binding.agentId,
      body: message.body,
      correlationId: `channel:${message.bindingId}:${message.idempotencyKey}`,
      bindingId: binding.bindingId,
      sessionId: binding.sessionId,
    },
  };
}

export function routeRequestToBridgeArgs(
  route: ChannelRouteRequest,
): [from: string, to: string, body: string] {
  return [route.from, route.to, route.body];
}

function mentionedAgentIds(
  message: NormalizedChannelInboundMessage,
  aliases: Record<string, AgentId> | undefined,
): AgentId[] {
  const direct = message.mentions.map((mention) => mention as AgentId);
  const resolved =
    aliases === undefined
      ? []
      : message.mentions
          .map((mention) => aliases[mention])
          .filter((agentId): agentId is AgentId => agentId !== undefined);
  return dedupe([...direct, ...resolved]);
}

function firstNonEmpty<T>(groups: readonly T[][]): T[] {
  return groups.find((group) => group.length > 0) ?? [];
}

function dedupe<T>(values: readonly T[]): T[] {
  const result: T[] = [];
  for (const value of values) {
    if (!result.includes(value)) {
      result.push(value);
    }
  }
  return result;
}
