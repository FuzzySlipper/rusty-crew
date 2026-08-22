import type {
  AgentDirectoryEntry,
  AgentMessageCommand,
  AgentMessageDeliveryReceipt,
  AgentRouteResolution,
} from "@rusty-crew/contracts";
import {
  directBrainCapabilities,
  nativeAttemptRef,
  nativeDeliveryId,
  nativeMessageId,
  operationId,
  type FabricBinding,
  type FabricClaim,
  type FabricClient,
  type FabricDelivery,
  type FabricLease,
} from "./types.js";

export interface CrewServicesRouteBinding {
  /** Fabric-owned logical alias. */
  alias: string;
  /** Rusty Crew's explicitly authored `@route` key. */
  routeKey: string;
  /** The route revision accepted for this adapter configuration. */
  routeRevision: number;
}

export interface CrewServicesAdapterConfig {
  adapterId: string;
  instanceId: string;
  bindings: readonly CrewServicesRouteBinding[];
  leaseDuration?: string;
  renewMs?: number;
  pollMs?: number;
  claimDuration?: string;
}

export interface CrewServicesAdapterStatus {
  readonly started: boolean;
  readonly stopped: boolean;
  readonly leaseExpiresAt?: string;
}

export interface CrewServicesDirectoryEntry {
  readonly alias: string;
  readonly routeRevision: number;
}

export interface CrewServicesSendInput {
  sessionId: string;
  toolCallId: string;
  recipientAlias: string;
  body: string;
  correlationId?: string;
  replyToMessageId?: string;
  ttl?: string;
}

export interface CrewServicesSendReceipt {
  readonly messageId: string;
  readonly replayed: boolean;
}

const defaults = {
  leaseDuration: "2m",
  renewMs: 45_000,
  pollMs: 1_000,
  claimDuration: "45s",
};

type CompleteConfig = Required<Omit<CrewServicesAdapterConfig, "bindings">> & {
  bindings: readonly CrewServicesRouteBinding[];
};

/** Deliberately narrow public-bridge seam; no service-host coupling. */
export interface DirectBrainDeliveryBridge {
  listAgentDirectory(): Promise<AgentDirectoryEntry[]>;
  getAgentRouteResolution(
    routeKey: string,
  ): Promise<AgentRouteResolution | undefined>;
  deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  getAgentMessageDelivery(
    deliveryId: string,
  ): Promise<AgentMessageDeliveryReceipt | undefined>;
}

/**
 * Lifecycle-only adapter for delivering fabric envelopes to exact direct-brain
 * routes. Service config and agent tools compose this class in the next task.
 */
export class CrewServicesDirectBrainAdapter {
  private readonly config: CompleteConfig;
  private lease: FabricLease | undefined;
  private leaseRenewedAt = 0;
  private started = false;
  private stopped = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readonly inFlight = new Set<Promise<void>>();
  private readonly tails = new Map<string, Promise<void>>();
  /** Synchronous visibility projection refreshed from validated exact bindings. */
  private readonly availableSessions = new Set<string>();
  /** Claims survive here only until `begin-dispatch` is known to have started. */
  private readonly claimedBeforeDispatch = new Map<
    string,
    { claimToken: string; attemptCount: number; possiblyBegan: boolean }
  >();

  constructor(
    private readonly fabric: FabricClient,
    private readonly bridge: DirectBrainDeliveryBridge,
    config: CrewServicesAdapterConfig,
  ) {
    validateConfig(config);
    this.config = { ...defaults, ...config };
  }

  status(): CrewServicesAdapterStatus {
    return {
      started: this.started,
      stopped: this.stopped,
      ...(this.lease === undefined
        ? {}
        : { leaseExpiresAt: this.lease.expires_at }),
    };
  }

  /** Safe only as a visibility hint; all tool actions revalidate asynchronously. */
  available(sessionId: string): boolean {
    return this.started && !this.stopped && this.availableSessions.has(sessionId);
  }

  /** Deliberately alias-only view for agent tools; native session ids stay private. */
  async directory(): Promise<readonly CrewServicesDirectoryEntry[]> {
    const entries = await Promise.all(
      this.config.bindings.map(async (binding) => {
        const target = await this.resolveRoutableTarget(binding);
        return target === undefined
          ? undefined
          : { alias: binding.alias, routeRevision: binding.routeRevision };
      }),
    );
    return entries.filter(
      (entry): entry is CrewServicesDirectoryEntry => entry !== undefined,
    );
  }

  /** Directory access is itself limited to a currently exact bound brain route. */
  async directoryForSession(
    sessionId: string,
  ): Promise<readonly CrewServicesDirectoryEntry[]> {
    if ((await this.boundBindingForSession(sessionId)) === undefined) {
      throw new Error("crew-services adapter: calling session is not exactly bound");
    }
    return this.directory();
  }

  /** Submit one ordinary fabric message from an exact configured direct-brain session. */
  async sendFromSession(input: CrewServicesSendInput): Promise<CrewServicesSendReceipt> {
    const sender = await this.boundBindingForSession(input.sessionId);
    const recipient = await this.boundBindingForAlias(input.recipientAlias);
    if (sender === undefined) {
      throw new Error("crew-services adapter: calling session is not exactly bound");
    }
    if (recipient === undefined) {
      throw new Error("crew-services adapter: recipient alias is not exactly bound");
    }
    const lease = await this.ensureLease();
    const submitted = await this.fabric.submit({
      producer_id: this.config.adapterId,
      lease_token: lease.lease_token,
      operation_id: operationId(
        `submit:${sender.alias}:${input.toolCallId}`,
        "submit",
      ),
      sender_address: sender.alias,
      recipient_address: recipient.alias,
      body: input.body,
      activation_policy: "wake_when_idle",
      ttl: input.ttl ?? "24h",
      ...(input.correlationId === undefined
        ? {}
        : { correlation_id: input.correlationId }),
      ...(input.replyToMessageId === undefined
        ? {}
        : { reply_to_message_id: input.replyToMessageId }),
    });
    return {
      messageId: submitted.message.message_id,
      replayed: submitted.replayed,
    };
  }

  async start(): Promise<void> {
    if (this.started && !this.stopped) return;
    this.stopped = false;
    await this.ensureLease();
    await this.syncBindings();
    await this.reconcileDispatching();
    this.started = true;
    this.schedule();
  }

  /** Stop new claims, wait for admitted native calls, release only pre-begin claims. */
  async stop(): Promise<void> {
    this.stopped = true;
    this.availableSessions.clear();
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    await Promise.all([...this.inFlight]);
    const pending = [...this.claimedBeforeDispatch.entries()].filter(
      ([, claim]) => !claim.possiblyBegan,
    );
    this.claimedBeforeDispatch.clear();
    await Promise.all(
      pending.map(async ([deliveryId, claim]) => {
        await this.release(
          deliveryId,
          claim.claimToken,
          claim.attemptCount,
        ).catch(() => undefined);
      }),
    );
  }

  /** Useful to a service host that wants an immediate, explicit pump pass. */
  async tick(): Promise<void> {
    const work = this.tickInner();
    this.observe(work);
    return work;
  }

  private async tickInner(): Promise<void> {
    if (this.stopped) return;
    await this.ensureLease();
    await this.syncBindings();
    await this.reconcileDispatching();
    await Promise.all(
      this.config.bindings.map((binding) => this.pumpAlias(binding)),
    );
  }

  private schedule(): void {
    if (this.stopped) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.tick()
        .catch(() => undefined)
        .finally(() => this.schedule());
    }, this.config.pollMs);
  }

  private observe(work: Promise<void>): void {
    const settled = work
      .catch(() => undefined)
      .finally(() => this.inFlight.delete(settled));
    this.inFlight.add(settled);
  }

  private async ensureLease(): Promise<FabricLease> {
    if (this.lease === undefined) {
      this.lease = await this.fabric.register({
        adapterId: this.config.adapterId,
        instanceId: this.config.instanceId,
        leaseDuration: this.config.leaseDuration,
      });
      this.leaseRenewedAt = Date.now();
    } else if (Date.now() - this.leaseRenewedAt >= this.config.renewMs) {
      this.lease = await this.fabric.renew({
        adapterId: this.config.adapterId,
        leaseToken: this.lease.lease_token,
        leaseDuration: this.config.leaseDuration,
      });
      this.leaseRenewedAt = Date.now();
    }
    return this.lease;
  }

  private async syncBindings(): Promise<void> {
    this.availableSessions.clear();
    const lease = await this.ensureLease();
    const current = await this.fabric.listBindings();
    const byAlias = new Map(
      current.addresses.map((binding) => [binding.address, binding]),
    );
    for (const binding of this.config.bindings) {
      await this.requireRoute(binding);
      const existing = byAlias.get(binding.alias);
      if (sameBinding(existing, this.config.adapterId, binding.routeKey))
        continue;
      await this.fabric.putBinding(binding.alias, {
        actor_adapter_id: this.config.adapterId,
        lease_token: lease.lease_token,
        adapter_id: this.config.adapterId,
        target_ref: binding.routeKey,
        capabilities: directBrainCapabilities,
        ...(existing === undefined
          ? {}
          : { expected_revision: existing.revision }),
      });
    }
    const validated = await Promise.all(
      this.config.bindings.map((binding) => this.resolveRoutableTarget(binding)),
    );
    for (const target of validated) {
      if (target !== undefined) this.availableSessions.add(target.entry.sessionId);
    }
  }

  private pumpAlias(binding: CrewServicesRouteBinding): Promise<void> {
    const prior = this.tails.get(binding.alias) ?? Promise.resolve();
    const next = prior
      .catch(() => undefined)
      .then(() => this.pumpOnce(binding));
    this.tails.set(binding.alias, next);
    return next.finally(() => {
      if (this.tails.get(binding.alias) === next)
        this.tails.delete(binding.alias);
    });
  }

  private async pumpOnce(binding: CrewServicesRouteBinding): Promise<void> {
    if (this.stopped) return;
    const target = await this.resolveRoutableTarget(binding);
    if (target === undefined) return;
    const head = await this.claimHead(binding.alias, target.fabric.generation);
    if (head === undefined) return;
    const lease = await this.ensureLease();
    const claimAttempt =
      head.state === "claimed" ? head.attempt_count : head.attempt_count + 1;
    const claimed = await this.fabric.claim({
      adapter_id: this.config.adapterId,
      lease_token: lease.lease_token,
      operation_id: operationId(
        `${head.delivery_id}:attempt:${claimAttempt}`,
        "claim",
      ),
      recipient_address: binding.alias,
      recipient_generation: target.fabric.generation,
      availability:
        head.state === "claimed"
          ? replayAvailability(head.dispatch_action)
          : availability(target.entry),
      claim_duration: this.config.claimDuration,
    });
    if (
      !claimed.claimed ||
      claimed.delivery === undefined ||
      claimed.message === undefined ||
      claimed.claim_token === undefined
    )
      return;
    this.claimedBeforeDispatch.set(claimed.delivery.delivery_id, {
      claimToken: claimed.claim_token,
      attemptCount: claimed.delivery.attempt_count,
      possiblyBegan: false,
    });
    await this.dispatch(binding, claimed);
  }

  private async dispatch(
    binding: CrewServicesRouteBinding,
    claimed: FabricClaim,
  ): Promise<void> {
    const delivery = claimed.delivery!;
    const message = claimed.message!;
    const claimToken = claimed.claim_token!;
    const target = await this.resolveRoutableTarget(binding);
    if (
      target === undefined ||
      target.fabric.generation !== delivery.recipient_generation
    ) {
      await this.release(
        delivery.delivery_id,
        claimToken,
        delivery.attempt_count,
      );
      return;
    }
    const attempt = nativeAttemptRef(delivery.delivery_id);
    try {
      await this.fabric.beginDispatch(delivery.delivery_id, {
        adapter_id: this.config.adapterId,
        lease_token: (await this.ensureLease()).lease_token,
        operation_id: operationId(delivery.delivery_id, "begin"),
        claim_token: claimToken,
        native_attempt_ref: attempt,
      });
    } catch {
      // A begin error is still pre-native. Reconcile only if a replay proves it began.
      const observed = await this.fabric
        .getDelivery(delivery.delivery_id)
        .catch(() => undefined);
      if (observed === undefined) {
        const current = this.claimedBeforeDispatch.get(delivery.delivery_id);
        if (current !== undefined) current.possiblyBegan = true;
        return;
      }
      if (observed.state !== "dispatching") {
        await this.release(
          delivery.delivery_id,
          claimToken,
          delivery.attempt_count,
        ).catch(() => undefined);
        return;
      }
    }
    this.claimedBeforeDispatch.delete(delivery.delivery_id);

    try {
      const receipt = await this.bridge.deliverAgentMessage({
        caller: {
          type: "system",
          senderAgentId: `fabric:${message.sender_address}`,
        },
        deliveryId: nativeDeliveryId(delivery.delivery_id),
        idempotencyKey: nativeDeliveryId(delivery.delivery_id),
        messageId: nativeMessageId(message.message_id),
        toAddress: binding.routeKey,
        inputKind: "routed_agent_message",
        body: fabricOriginFrame(message),
        ...(message.correlation_id === undefined
          ? {}
          : { correlationId: message.correlation_id }),
        requireWake: true,
        createdAt: deliveryCreatedAt(message.expires_at),
        expiresAt: message.expires_at,
      });
      await this.settleNativeResult(delivery, receipt);
    } catch {
      await this.reconcileNativeOutcome(delivery);
    }
  }

  private async settleNativeResult(
    delivery: FabricDelivery,
    receipt: AgentMessageDeliveryReceipt,
  ): Promise<void> {
    let observed: AgentMessageDeliveryReceipt | undefined;
    try {
      observed = await this.bridge.getAgentMessageDelivery(
        nativeDeliveryId(delivery.delivery_id),
      );
    } catch {
      if (receipt.status === "rejected" || receipt.status === "expired") {
        await this.fail(delivery.delivery_id);
        return;
      }
      throw new Error("native delivery readback failed");
    }
    if (observed?.status === "accepted") {
      await this.acknowledge(delivery.delivery_id);
      return;
    }
    if (
      observed?.status === "rejected" ||
      observed?.status === "expired" ||
      receipt.status === "rejected" ||
      receipt.status === "expired"
    ) {
      await this.fail(delivery.delivery_id);
      return;
    }
    await this.outcomeUnknown(delivery.delivery_id);
  }

  private async reconcileNativeOutcome(
    delivery: FabricDelivery,
  ): Promise<void> {
    const observed = await this.bridge
      .getAgentMessageDelivery(nativeDeliveryId(delivery.delivery_id))
      .catch(() => undefined);
    if (observed?.status === "accepted") {
      await this.acknowledge(delivery.delivery_id);
    } else if (
      observed?.status === "rejected" ||
      observed?.status === "expired"
    ) {
      await this.fail(delivery.delivery_id);
    } else {
      await this.outcomeUnknown(delivery.delivery_id);
    }
  }

  private async reconcileDispatching(): Promise<void> {
    const deliveries = await this.fabric.listDeliveries();
    for (const delivery of deliveries.deliveries) {
      if (
        delivery.state !== "dispatching" ||
        delivery.claim_owner_adapter_id !== this.config.adapterId ||
        delivery.native_attempt_ref !== nativeAttemptRef(delivery.delivery_id)
      )
        continue;
      await this.reconcileNativeOutcome(delivery);
    }
  }

  private async resolveRoutableTarget(
    binding: CrewServicesRouteBinding,
  ): Promise<
    { entry: AgentDirectoryEntry; fabric: FabricBinding } | undefined
  > {
    const [route, directory, bindings] = await Promise.all([
      this.bridge.getAgentRouteResolution(binding.routeKey),
      this.bridge.listAgentDirectory(),
      this.fabric.listBindings(),
    ]);
    if (!isExactDirectRoute(route, binding)) return undefined;
    const target = route!.resolvedTarget!;
    const entry = directory.find(
      (candidate) =>
        candidate.agentId === target.agentId &&
        candidate.sessionId === target.sessionId &&
        candidate.runtimeKind === "direct_brain",
    );
    const fabric = bindings.addresses.find(
      (candidate) => candidate.address === binding.alias,
    );
    if (
      entry === undefined ||
      !entry.routable ||
      entry.sessionStatus === "archived" ||
      fabric === undefined ||
      !sameBinding(fabric, this.config.adapterId, binding.routeKey)
    )
      return undefined;
    return { entry, fabric };
  }

  private async requireRoute(binding: CrewServicesRouteBinding): Promise<void> {
    const route = await this.bridge.getAgentRouteResolution(binding.routeKey);
    if (!isExactDirectRoute(route, binding))
      throw new Error(
        `crew-services adapter: ${binding.alias} must resolve to revision ${binding.routeRevision} direct-brain route ${binding.routeKey}`,
      );
  }

  private async boundBindingForSession(
    sessionId: string,
  ): Promise<CrewServicesRouteBinding | undefined> {
    const candidates = await Promise.all(
      this.config.bindings.map(async (binding) => {
        const target = await this.resolveRoutableTarget(binding);
        return target?.entry.sessionId === sessionId ? binding : undefined;
      }),
    );
    const matched = candidates.filter(
      (binding): binding is CrewServicesRouteBinding => binding !== undefined,
    );
    return matched.length === 1 ? matched[0] : undefined;
  }

  private async boundBindingForAlias(
    alias: string,
  ): Promise<CrewServicesRouteBinding | undefined> {
    const binding = this.config.bindings.find(
      (candidate) => candidate.alias === alias,
    );
    return binding !== undefined && (await this.resolveRoutableTarget(binding))
      ? binding
      : undefined;
  }

  private async claimHead(
    alias: string,
    generation: number,
  ): Promise<FabricDelivery | undefined> {
    const deliveries = await this.fabric.listDeliveries();
    const candidates = deliveries.deliveries.filter(
      (delivery) =>
        delivery.recipient_address === alias &&
        delivery.recipient_generation === generation &&
        (delivery.state === "queued" ||
          (delivery.state === "claimed" &&
            delivery.claim_owner_adapter_id === this.config.adapterId &&
            delivery.claim_owner_instance_id === this.config.instanceId)),
    );
    const ownClaim = candidates.filter(
      (delivery) => delivery.state === "claimed",
    );
    if (ownClaim.length > 0) return ownClaim[0];
    candidates.sort(
      (left, right) =>
        (left.accepted_sequence ?? Number.MAX_SAFE_INTEGER) -
        (right.accepted_sequence ?? Number.MAX_SAFE_INTEGER),
    );
    return candidates[0];
  }

  private async release(
    deliveryId: string,
    claimToken: string,
    attemptCount: number,
  ): Promise<void> {
    await this.fabric.release(deliveryId, {
      adapter_id: this.config.adapterId,
      lease_token: (await this.ensureLease()).lease_token,
      operation_id: operationId(
        `${deliveryId}:attempt:${attemptCount}`,
        "release",
      ),
      claim_token: claimToken,
    });
  }
  private async acknowledge(deliveryId: string): Promise<void> {
    await this.fabric.acknowledge(
      deliveryId,
      await this.reconcileBody(deliveryId, "ack"),
    );
  }
  private async fail(deliveryId: string): Promise<void> {
    await this.fabric.fail(
      deliveryId,
      await this.reconcileBody(deliveryId, "fail"),
    );
  }
  private async outcomeUnknown(deliveryId: string): Promise<void> {
    await this.fabric.outcomeUnknown(
      deliveryId,
      await this.reconcileBody(deliveryId, "unknown"),
    );
  }
  private async reconcileBody(
    deliveryId: string,
    action: string,
  ): Promise<Record<string, unknown>> {
    return {
      adapter_id: this.config.adapterId,
      lease_token: (await this.ensureLease()).lease_token,
      operation_id: operationId(deliveryId, action),
      native_attempt_ref: nativeAttemptRef(deliveryId),
    };
  }
}

function sameBinding(
  binding: FabricBinding | undefined,
  adapterId: string,
  routeKey: string,
): boolean {
  return (
    binding?.bound === true &&
    binding.adapter_id === adapterId &&
    binding.target_ref === routeKey &&
    directBrainCapabilities.every((capability) =>
      binding.capabilities.includes(capability),
    )
  );
}

function isExactDirectRoute(
  route: AgentRouteResolution | undefined,
  binding: CrewServicesRouteBinding,
): boolean {
  if (
    route === undefined ||
    route.routable !== true ||
    route.route?.enabled !== true
  )
    return false;
  const target = route.resolvedTarget;
  if (target == null) return false;
  return (
    route.route.revision === binding.routeRevision &&
    route.route.target.type === "direct_brain" &&
    route.route.target.agentId === target.agentId &&
    route.route.target.sessionId === target.sessionId &&
    target.runtimeKind === "direct_brain"
  );
}

function availability(
  entry: AgentDirectoryEntry,
): "busy" | "idle" | "inactive" {
  if (entry.execution?.phase !== undefined && entry.execution.phase !== "idle")
    return "busy";
  if (entry.sessionStatus === "idle") return "idle";
  return entry.sessionStatus === "active" ? "busy" : "inactive";
}

function replayAvailability(
  dispatchAction: string | undefined,
): "busy" | "idle" | "inactive" {
  switch (dispatchAction) {
    case "register_next_turn":
      return "busy";
    case "deliver_at_idle":
      return "idle";
    case "wake_then_deliver":
      return "inactive";
    default:
      throw new Error(
        "crew-services adapter: claimed delivery has no replayable dispatch action",
      );
  }
}

/** Frames fabric provenance without turning replies into automatic request/reply work. */
export function fabricOriginFrame(message: {
  message_id: string;
  sender_address: string;
  recipient_address: string;
  body: string;
  reply_to_message_id?: string;
}): string {
  const prefix = message.reply_to_message_id === undefined
    ? `[Crew message ${message.message_id} from @${message.sender_address} to @${message.recipient_address}]`
    : `[Crew reply ${message.message_id} to ${message.reply_to_message_id} from @${message.sender_address} to @${message.recipient_address}]`;
  const guidance = message.reply_to_message_id === undefined
    ? "\n\nIf a reply is useful, send one ordinary crew_message with replyToMessageId set to this message id."
    : "\n\nThis is a terminal reply. Do not answer merely because it is a reply; only send a new ordinary crew_message if independently needed, and omit replyToMessageId.";
  return `${prefix}\n${message.body}${guidance}`;
}

/** Native delivery bounds are relative to the adapter's admission, not fabric creation. */
function deliveryCreatedAt(expiresAt: string): string {
  const expires = Date.parse(expiresAt);
  if (!Number.isFinite(expires)) return new Date().toISOString();
  return new Date(Math.max(Date.now(), expires - 60_000)).toISOString();
}

function validateConfig(config: CrewServicesAdapterConfig): void {
  if (!config.adapterId.trim() || !config.instanceId.trim())
    throw new Error(
      "crew-services adapter: adapterId and instanceId are required",
    );
  const aliases = new Set<string>();
  const routes = new Set<string>();
  for (const binding of config.bindings) {
    if (
      !binding.alias.trim() ||
      !binding.routeKey.trim() ||
      !Number.isInteger(binding.routeRevision) ||
      binding.routeRevision <= 0
    )
      throw new Error(
        "crew-services adapter: binding requires alias, routeKey, and positive routeRevision",
      );
    if (aliases.has(binding.alias) || routes.has(binding.routeKey))
      throw new Error(
        "crew-services adapter: aliases and routes must be unique",
      );
    aliases.add(binding.alias);
    routes.add(binding.routeKey);
  }
}
