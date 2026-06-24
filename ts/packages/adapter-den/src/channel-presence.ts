import type {
  AdapterId,
  AgentId,
  ChannelBindingRecord,
  ChannelMembershipRecord,
  ChannelMembershipStatus,
  ChannelPresenceRecord,
  ChannelPresenceStatus,
  ChannelProviderRefs,
  ChannelSubscriptionRecord,
  ChannelSubscriptionStatus,
  ChannelSubscriptionTransportKind,
  CoreEventKind,
  EventSubscription,
  ProfileId,
  SessionId,
  SubscriptionHandle,
} from "@rusty-crew/contracts";

export interface RustEventSubscriptionClient {
  subscribeEvents(subscription: EventSubscription): Promise<SubscriptionHandle>;
  unsubscribeEvents(handle: SubscriptionHandle): Promise<unknown>;
}

export interface ChannelMembershipInput {
  bindingId: string;
  adapterId: AdapterId;
  providerRefs: ChannelProviderRefs;
  externalUserId: string;
  displayLabel?: string;
  agentId?: AgentId;
  profileId?: ProfileId;
  roleLabels?: string[];
  status?: ChannelMembershipStatus;
  observedAt: string;
  provenance?: Record<string, unknown>;
}

export interface ChannelPresenceInput {
  bindingId: string;
  adapterId: AdapterId;
  providerRefs: ChannelProviderRefs;
  externalUserId?: string;
  agentId?: AgentId;
  sessionId?: SessionId;
  status?: ChannelPresenceStatus;
  observedAt: string;
  expiresAt?: string;
  provenance?: Record<string, unknown>;
}

export interface ChannelSubscriptionInput {
  bindingId: string;
  adapterId: AdapterId;
  providerRefs: ChannelProviderRefs;
  transportKind: ChannelSubscriptionTransportKind;
  providerSubscriptionId?: string;
  rustSubscriptionHandle?: SubscriptionHandle;
  cursor?: string;
  status?: ChannelSubscriptionStatus;
  observedAt: string;
  degradedReason?: string;
  provenance?: Record<string, unknown>;
}

export interface ChannelBindingDiagnostics {
  bindingId: string;
  adapterId?: AdapterId;
  conversationProjectId?: string;
  conversationChannelId?: number;
  membershipStatus: ChannelMembershipStatus | "missing";
  presenceStatus: ChannelPresenceStatus | "missing";
  subscriptionStatus: ChannelSubscriptionStatus | "missing";
  degradedReason?: string;
  stale: boolean;
}

export interface RustEventSubscriptionRequest {
  binding: ChannelBindingRecord;
  eventKinds: CoreEventKind[];
  adapterId?: AdapterId;
  observedAt: string;
}

export class ChannelBindingActivityTracker {
  readonly #memberships = new Map<string, ChannelMembershipRecord>();
  readonly #presences = new Map<string, ChannelPresenceRecord>();
  readonly #subscriptions = new Map<string, ChannelSubscriptionRecord>();

  upsertMembership(input: ChannelMembershipInput): ChannelMembershipRecord {
    const record: ChannelMembershipRecord = {
      kind: "channel_membership.v1",
      bindingId: input.bindingId,
      adapterId: input.adapterId,
      providerRefs: input.providerRefs,
      externalUserId: input.externalUserId,
      displayLabel: input.displayLabel,
      agentId: input.agentId,
      profileId: input.profileId,
      roleLabels: input.roleLabels ?? [],
      status: input.status ?? "joined",
      observedAt: input.observedAt,
      provenance: input.provenance ?? {},
    };
    this.#memberships.set(input.bindingId, record);
    return record;
  }

  observePresence(input: ChannelPresenceInput): ChannelPresenceRecord {
    const record: ChannelPresenceRecord = {
      kind: "channel_presence.v1",
      bindingId: input.bindingId,
      adapterId: input.adapterId,
      providerRefs: input.providerRefs,
      externalUserId: input.externalUserId,
      agentId: input.agentId,
      sessionId: input.sessionId,
      status: input.status ?? "unknown",
      observedAt: input.observedAt,
      expiresAt: input.expiresAt,
      provenance: input.provenance ?? {},
    };
    this.#presences.set(input.bindingId, record);
    return record;
  }

  upsertSubscription(
    input: ChannelSubscriptionInput,
  ): ChannelSubscriptionRecord {
    const existing = this.#subscriptions.get(input.bindingId);
    const status = input.status ?? "active";
    const record: ChannelSubscriptionRecord = {
      kind: "channel_subscription.v1",
      bindingId: input.bindingId,
      adapterId: input.adapterId,
      providerRefs: input.providerRefs,
      transportKind: input.transportKind,
      providerSubscriptionId: input.providerSubscriptionId,
      rustSubscriptionHandle: input.rustSubscriptionHandle,
      cursor: input.cursor ?? existing?.cursor,
      status,
      lastConnectedAt:
        status === "active" ? input.observedAt : existing?.lastConnectedAt,
      lastSeenAt: input.observedAt,
      lastErrorAt:
        status === "degraded" || status === "disconnected"
          ? input.observedAt
          : existing?.lastErrorAt,
      degradedReason: input.degradedReason,
      provenance: input.provenance ?? existing?.provenance ?? {},
    };
    this.#subscriptions.set(input.bindingId, record);
    return record;
  }

  async subscribeRustEvents(
    client: RustEventSubscriptionClient,
    request: RustEventSubscriptionRequest,
  ): Promise<ChannelSubscriptionRecord> {
    const handle = await client.subscribeEvents({
      eventKinds: request.eventKinds,
      sessionId: request.binding.sessionId,
      agentId: request.binding.agentId,
      adapterId: request.adapterId,
    });
    return this.upsertSubscription({
      bindingId: request.binding.bindingId,
      adapterId: request.binding.adapterId,
      providerRefs: providerRefsFromBinding(request.binding),
      transportKind: "rust_event_subscription",
      rustSubscriptionHandle: handle,
      status: "active",
      observedAt: request.observedAt,
      provenance: {
        eventKinds: request.eventKinds,
      },
    });
  }

  async unsubscribeRustEvents(
    client: RustEventSubscriptionClient,
    bindingId: string,
    observedAt: string,
  ): Promise<ChannelSubscriptionRecord | undefined> {
    const existing = this.#subscriptions.get(bindingId);
    if (existing?.rustSubscriptionHandle === undefined) {
      return existing;
    }
    await client.unsubscribeEvents(existing.rustSubscriptionHandle);
    return this.upsertSubscription({
      ...existing,
      status: "archived",
      observedAt,
      provenance: {
        ...existing.provenance,
        archivedBy: "unsubscribeRustEvents",
      },
    });
  }

  markSubscriptionDegraded(
    bindingId: string,
    reason: string,
    observedAt: string,
  ): ChannelSubscriptionRecord | undefined {
    const existing = this.#subscriptions.get(bindingId);
    if (existing === undefined) return undefined;
    return this.upsertSubscription({
      ...existing,
      status: "degraded",
      observedAt,
      degradedReason: reason,
    });
  }

  diagnostics(now: string): ChannelBindingDiagnostics[] {
    const bindingIds = new Set([
      ...this.#memberships.keys(),
      ...this.#presences.keys(),
      ...this.#subscriptions.keys(),
    ]);
    return [...bindingIds].sort().map((bindingId) => {
      const membership = this.#memberships.get(bindingId);
      const presence = this.#presences.get(bindingId);
      const subscription = this.#subscriptions.get(bindingId);
      return {
        bindingId,
        adapterId:
          subscription?.adapterId ??
          presence?.adapterId ??
          membership?.adapterId,
        conversationChannelId: numericChannelId(
          subscription?.providerRefs.externalChannelId ??
            presence?.providerRefs.externalChannelId ??
            membership?.providerRefs.externalChannelId,
        ),
        membershipStatus: membership?.status ?? "missing",
        presenceStatus: presence?.status ?? "missing",
        subscriptionStatus: subscription?.status ?? "missing",
        degradedReason: subscription?.degradedReason,
        stale: isPresenceStale(presence, now),
      };
    });
  }

  membership(bindingId: string): ChannelMembershipRecord | undefined {
    return this.#memberships.get(bindingId);
  }

  presence(bindingId: string): ChannelPresenceRecord | undefined {
    return this.#presences.get(bindingId);
  }

  subscription(bindingId: string): ChannelSubscriptionRecord | undefined {
    return this.#subscriptions.get(bindingId);
  }
}

function numericChannelId(value: string | undefined): number | undefined {
  if (value === undefined) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

export function providerRefsFromBinding(
  binding: ChannelBindingRecord,
): ChannelProviderRefs {
  return {
    provider: binding.provider,
    externalChannelId: binding.externalChannelId,
    externalThreadId: binding.externalThreadId,
    externalUserId: binding.externalUserId,
  };
}

function isPresenceStale(
  presence: ChannelPresenceRecord | undefined,
  now: string,
): boolean {
  return (
    presence?.expiresAt !== undefined &&
    Date.parse(presence.expiresAt) <= Date.parse(now)
  );
}
