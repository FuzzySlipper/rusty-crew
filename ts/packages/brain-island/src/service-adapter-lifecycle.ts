import { join } from "node:path";
import type {
  AdapterId,
  AgentId,
  AgentInstanceId,
  ChannelBindingRecord,
  ChannelMembershipStatus,
  ChannelSubscriptionStatus,
  CoreEvent,
  NormalizedChannelInboundMessage,
  ProfileId,
  SessionId,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeChannelIngressRoutePlan,
} from "@rusty-crew/native-bridge";
import {
  AgentActivityObservationProducer,
  type AgentActivityEventInput,
  type AgentActivityObservationSink,
} from "./agent-activity-observation.js";
import type {
  ChannelAdapterBindingDiagnostics,
  ChannelProjectionFailureRecord,
} from "./adapter-diagnostics.js";
import type { ChannelWakePolicy } from "./channel-wake-policy.js";
import {
  announceConfiguredSessionsToDenGateway,
  denGatewayStartupSummary,
  type DenSuccessorGatewayStartupReport,
} from "./den-successor-service.js";
import {
  runtimeCoreEventObservationInput,
  type RuntimeObservationSessionIdentity,
} from "./runtime-core-event-observation.js";
import type {
  ChannelBindingDiagnostics,
  ChannelIngressRoutePlannerInput,
  ChannelRouteResolution,
  DenConversationChannelResolution,
  DenSuccessorConversationMembership,
  DenSuccessorDeliveryIntent,
  DenSuccessorGatewayClient,
  ServiceAdapterFactories,
  TelegramChannelConnectorPort,
} from "./service-adapter-ports.js";
import type { RustyCrewServiceConfig } from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

export interface AdapterLifecycleServiceEvent {
  source: string;
  eventType: string;
  summary: string;
  severity?: "info" | "warning" | "error";
}

export interface ServiceAdapterLifecycleContext {
  readonly config: RustyCrewServiceConfig;
  readonly bridge: NativeBridgeModule;
  readonly adapterFactories: ServiceAdapterFactories;
  get runtimeConfig(): RustyCrewRuntimeConfig;
  get denGatewayClient(): DenSuccessorGatewayClient | undefined;
  get denObservationSubscription(): SubscriptionHandle | undefined;
  set denObservationSubscription(subscription: SubscriptionHandle | undefined);
  get telegramConnector(): TelegramChannelConnectorPort | undefined;
  set telegramConnector(connector: TelegramChannelConnectorPort | undefined);
  get telegramOutboundSubscription(): SubscriptionHandle | undefined;
  set telegramOutboundSubscription(
    subscription: SubscriptionHandle | undefined,
  );
  readonly timers: Set<NodeJS.Timeout>;
  readonly denConversationChannelResolutionsByBindingId: Map<
    string,
    DenConversationChannelResolution
  >;
  readonly denConversationChannelIdsByExternalId: Map<string, number>;
  readonly denConversationMembershipsByBindingId: Map<
    string,
    DenSuccessorConversationMembership
  >;
  readonly dynamicDenChannelBindings: Map<
    string,
    ChannelAdapterBindingDiagnostics
  >;
  readonly channelProjectionFailures: ChannelProjectionFailureRecord[];
  now(): string;
  isStopping(): boolean;
  recordEvent(event: AdapterLifecycleServiceEvent): void;
  drainSubscriptionEventsUntilIdle(
    subscription: SubscriptionHandle,
  ): Promise<CoreEvent[]>;
  createObservationSink(
    client: DenSuccessorGatewayClient,
  ): AgentActivityObservationSink;
  ensureSessionForChannelBinding(input: {
    binding: ChannelBindingRecord;
  }): Promise<unknown>;
  channelWakePolicyForSession(
    session: RustyCrewRuntimeConfig["sessions"][number],
  ): ChannelWakePolicy;
}

export async function connectDenSuccessorGateway(
  context: ServiceAdapterLifecycleContext,
): Promise<DenSuccessorGatewayStartupReport | undefined> {
  if (context.config.denSuccessorGateway === undefined) {
    return undefined;
  }
  if (context.denGatewayClient === undefined) {
    return undefined;
  }
  let report: DenSuccessorGatewayStartupReport;
  try {
    report = await announceConfiguredSessionsToDenGateway({
      client: context.denGatewayClient,
      sessions: context.runtimeConfig.sessions,
      now: context.now(),
    });
  } catch (error) {
    report = {
      enabled: true,
      sessionsAnnounced: 0,
      runtimeInstancesRegistered: 0,
      runtimeInstancesHeartbeated: 0,
      failures: [
        errorMessage(error, "Den successor Gateway connection failed"),
      ],
    };
  }
  context.recordEvent({
    source: "den-successor-gateway",
    eventType:
      report.failures.length === 0
        ? "den_successor_gateway_connected"
        : "den_successor_gateway_degraded",
    summary: denGatewayStartupSummary(report),
    severity: report.failures.length === 0 ? "info" : "warning",
  });
  return report;
}

export async function startDenObservationProjection(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  if (context.denGatewayClient === undefined) return;
  const subscription = await context.bridge.subscribeEvents({
    eventKinds: [
      "session_created",
      "session_archived",
      "agent_message_routed",
      "delegation_lifecycle_observed",
      "brain_wake_requested",
      "brain_actions_accepted",
      "completion_packet_delivered",
    ],
  });
  context.denObservationSubscription = subscription;
  const timer = setInterval(() => {
    void drainDenObservationProjection(context).catch((error) =>
      context.recordEvent({
        source: "den-successor-gateway",
        eventType: "den_observation_projection_degraded",
        severity: "warning",
        summary: errorMessage(error, "Den Observation projection failed"),
      }),
    );
  }, 1_000);
  context.timers.add(timer);
  context.recordEvent({
    source: "den-successor-gateway",
    eventType: "den_observation_projection_started",
    summary:
      "Den Observation projection subscribed to Rusty Crew runtime events.",
  });
}

export async function drainDenObservationProjection(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  const subscription = context.denObservationSubscription;
  if (subscription === undefined || context.denGatewayClient === undefined)
    return;
  const events = await context.drainSubscriptionEventsUntilIdle(subscription);
  if (events.length === 0) return;

  const sessionLookup = await runtimeObservationSessionLookup(context);
  const producer = new AgentActivityObservationProducer({
    sink: context.createObservationSink(context.denGatewayClient),
    required: true,
  });
  let projected = 0;
  let degraded = 0;
  for (const event of events) {
    const input: AgentActivityEventInput | undefined =
      runtimeCoreEventObservationInput(event, {
        lookupSession: sessionLookup,
        filters: context.runtimeConfig.denObservation?.eventFilters,
      });
    if (input === undefined) continue;
    const result = await producer.publish(input);
    if (result.status === "published") {
      projected += 1;
    } else if (result.status === "degraded") {
      degraded += 1;
    }
  }
  if (projected > 0) {
    context.recordEvent({
      source: "den-successor-gateway",
      eventType: "den_observation_projection_published",
      summary: `Published ${projected} Den Observation runtime event(s).`,
    });
  }
  if (degraded > 0) {
    context.recordEvent({
      source: "den-successor-gateway",
      eventType: "den_observation_projection_degraded",
      severity: "warning",
      summary: `Publishing ${degraded} Den Observation runtime event(s) degraded.`,
    });
  }
}

async function runtimeObservationSessionLookup(
  context: ServiceAdapterLifecycleContext,
): Promise<
  (
    sessionId: SessionId | string,
  ) => RuntimeObservationSessionIdentity | undefined
> {
  const sessions = await context.bridge.listSessions().catch(() => []);
  const byId = new Map<string, RuntimeObservationSessionIdentity>();
  for (const session of sessions) {
    byId.set(session.sessionId, {
      sessionId: session.sessionId,
      agentId: session.agentId,
      profileId: session.profileId,
      kind: session.kind,
    });
  }
  for (const session of context.runtimeConfig.sessions) {
    if (!byId.has(session.sessionId)) {
      byId.set(session.sessionId, {
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: session.kind,
      });
    }
  }
  return (sessionId) => byId.get(String(sessionId));
}

export async function ensureDenConversationChannels(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  if (context.denGatewayClient === undefined) return;
  const bindings = activeDenChannelBindings(
    context.runtimeConfig.channelBindings,
  );
  if (bindings.length === 0) {
    context.denConversationChannelResolutionsByBindingId.clear();
    context.denConversationChannelIdsByExternalId.clear();
    context.denConversationMembershipsByBindingId.clear();
    return;
  }

  try {
    const resolution =
      await context.adapterFactories.resolveDenConversationChannels({
        client: context.denGatewayClient,
        bindings,
        defaultProjectId: context.config.denConversationProjectId,
      });
    context.denConversationChannelResolutionsByBindingId.clear();
    for (const [
      bindingId,
      channelResolution,
    ] of resolution.resolutionsByBindingId) {
      context.denConversationChannelResolutionsByBindingId.set(
        bindingId,
        channelResolution,
      );
    }
    context.denConversationChannelIdsByExternalId.clear();
    for (const [
      externalChannelKey,
      channelId,
    ] of resolution.channelIdsByExternalId) {
      context.denConversationChannelIdsByExternalId.set(
        externalChannelKey,
        channelId,
      );
    }
    context.denConversationMembershipsByBindingId.clear();
    for (const [bindingId, membership] of resolution.membershipsByBindingId) {
      context.denConversationMembershipsByBindingId.set(bindingId, membership);
    }
    if (resolution.membershipResolutionFailure !== undefined) {
      context.recordEvent({
        source: "den-successor-gateway",
        eventType: "den_conversation_memberships_degraded",
        severity: "warning",
        summary: resolution.membershipResolutionFailure,
      });
    }
    context.recordEvent({
      source: "den-successor-gateway",
      eventType: "den_conversation_channels_resolved",
      summary: `Resolved ${resolution.resolutionsByBindingId.size} Den Conversation channel binding(s), created ${resolution.createdCount}.`,
    });
  } catch (error) {
    context.recordEvent({
      source: "den-successor-gateway",
      eventType: "den_conversation_channels_degraded",
      severity: "warning",
      summary: errorMessage(
        error,
        "Den Conversation channel resolution failed",
      ),
    });
  }
}

export function activeDenChannelBindings(
  bindings: readonly ChannelBindingRecord[],
): ChannelBindingRecord[] {
  return bindings.filter(
    (binding) =>
      binding.status === "active" &&
      binding.provider === "den_channels" &&
      binding.externalChannelId.trim(),
  );
}

export function conversationProjectIdForBinding(
  config: RustyCrewServiceConfig,
  binding: ChannelBindingRecord,
): string {
  return (
    binding.conversationProjectId?.trim() ?? config.denConversationProjectId
  );
}

export async function startTelegramConnector(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  if (!context.config.telegram.enabled) return;
  const token = context.config.telegram.botToken;
  if (!token) return;
  const adapterId = context.config.telegram.adapterId as never;
  try {
    await context.bridge.registerPlatformAdapter(
      context.adapterFactories.createTelegramAdapterRegistration(adapterId),
    );
  } catch (error) {
    context.recordEvent({
      source: "telegram",
      eventType: "telegram_adapter_registration_degraded",
      severity: "warning",
      summary: errorMessage(error, "Telegram adapter registration failed"),
    });
  }

  const connector = context.adapterFactories.createTelegramConnector({
    adapterId,
    botToken: token,
    apiBaseUrl: context.config.telegram.apiBaseUrl,
    offsetStorePath: join(
      context.config.paths.dataDir,
      "data",
      "telegram",
      `${context.config.telegram.adapterId}-offset.json`,
    ),
    terminalStorePath: join(
      context.config.paths.dataDir,
      "data",
      "telegram",
      `${context.config.telegram.adapterId}-terminal-updates.jsonl`,
    ),
    bindings: () =>
      activeTelegramChannelBindings(
        context.runtimeConfig.channelBindings,
        context.config.telegram.adapterId,
      ),
    ttlMs: context.config.telegram.messageTtlMs,
    pollIntervalMs: context.config.telegram.pollIntervalMs,
    pollTimeoutSeconds: context.config.telegram.pollTimeoutSeconds,
    updateLimit: context.config.telegram.updateLimit,
    now: context.now,
    onInbound: async (message) => {
      return context.adapterFactories.ingestChannelInboundMessage(message, {
        bridge: {
          injectExternalEvent: (event) =>
            context.bridge.injectExternalEvent(event),
          routeAgentMessage: (agentMessage) =>
            context.bridge.routeAgentMessage(
              agentMessage.from,
              agentMessage.to,
              agentMessage.body,
              agentMessage.correlationId ?? undefined,
            ),
        },
        bindings: context.runtimeConfig.channelBindings,
        ensureSessionForRoute: ({ binding }) =>
          context.ensureSessionForChannelBinding({ binding }),
        routePlanner: (input) => planChannelIngressRoute(context.bridge, input),
        now: context.now(),
      });
    },
  });
  const outboundSubscription = await context.bridge.subscribeEvents({
    eventKinds: ["agent_message_routed"],
  });
  context.telegramConnector = connector;
  context.telegramOutboundSubscription = outboundSubscription;
  await connector.start();
  context.recordEvent({
    source: "telegram",
    eventType: "telegram_connector_started",
    summary: `Telegram connector started with ${connector.diagnostics().bindingCount} active binding(s).`,
  });
}

export async function restartTelegramConnector(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  await stopTelegramConnector(context);
  await startTelegramConnector(context);
}

export async function stopTelegramConnector(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  context.telegramConnector?.stop();
  context.telegramConnector = undefined;
  const subscription = context.telegramOutboundSubscription;
  context.telegramOutboundSubscription = undefined;
  if (subscription !== undefined) {
    await context.bridge.unsubscribeEvents(subscription).catch(() => undefined);
  }
}

export function activeTelegramChannelBindings(
  bindings: readonly ChannelBindingRecord[],
  adapterId: string,
): ChannelBindingRecord[] {
  return bindings.filter(
    (binding) =>
      binding.status === "active" &&
      binding.provider === "telegram" &&
      binding.adapterId === adapterId,
  );
}

export async function planChannelIngressRoute(
  bridge: NativeBridgeModule,
  input: ChannelIngressRoutePlannerInput,
): Promise<ChannelRouteResolution> {
  const plan = await bridge.planChannelIngressRoute({
    message: {
      adapterId: input.message.adapterId,
      bindingId: input.message.bindingId,
      provider: String(input.message.providerRefs.provider),
      externalChannelId: input.message.providerRefs.externalChannelId,
      externalThreadId: input.message.providerRefs.externalThreadId,
      externalUserId: input.message.author.externalUserId,
      body: input.message.body,
      mentions: input.message.mentions,
      expiresAt: input.message.expiresAt,
      idempotencyKey: input.message.idempotencyKey,
      runtimeAgentId: input.message.runtime.agentId,
    },
    bindings: input.bindings.map((binding) => ({
      bindingId: binding.bindingId,
      adapterId: binding.adapterId,
      provider: String(binding.provider),
      agentId: binding.agentId,
      instanceId: binding.instanceId,
      sessionId: binding.sessionId,
      profileId: binding.profileId,
      externalChannelId: binding.externalChannelId,
      externalThreadId: binding.externalThreadId,
      externalUserId: binding.externalUserId,
      conversationProjectId: binding.conversationProjectId,
      conversationChannelId: binding.conversationChannelId,
      providerSubscriptionId: binding.providerSubscriptionId,
      status: binding.status,
    })),
    mentionAliases: input.routing?.mentionAliases,
    systemAgentId: input.routing?.systemAgentId,
    now: input.now,
  });

  return channelIngressRoutePlanToResolution(input.message, plan);
}

function channelIngressRoutePlanToResolution(
  message: NormalizedChannelInboundMessage,
  plan: NativeChannelIngressRoutePlan,
): ChannelRouteResolution {
  if (plan.status === "routed") {
    if (plan.route === undefined || plan.binding === undefined) {
      return {
        status: "denied",
        reason:
          "Rust channel route planner returned a routed decision without route or binding data",
        reasonCode: "route_plan_missing_route",
        correlationId: plan.correlationId,
        candidates: plan.candidates.map(nativeChannelBindingToRecord),
        message,
      };
    }
    return {
      status: "routed",
      binding: nativeChannelBindingToRecord(plan.binding),
      route: {
        from: plan.route.from,
        to: plan.route.to,
        body: plan.route.body,
        correlationId: plan.route.correlationId,
        bindingId: plan.route.bindingId,
        sessionId: plan.route.sessionId,
      },
    };
  }
  return {
    status: plan.status,
    reason: plan.reason,
    reasonCode: plan.reasonCode,
    correlationId: plan.correlationId,
    candidates: plan.candidates.map(nativeChannelBindingToRecord),
    message,
  };
}

function nativeChannelBindingToRecord(
  binding: NativeChannelIngressRoutePlan["candidates"][number],
): ChannelBindingRecord {
  return {
    bindingId: binding.bindingId,
    adapterId: binding.adapterId as AdapterId,
    provider: binding.provider,
    agentId: binding.agentId as AgentId,
    instanceId: binding.instanceId as AgentInstanceId | undefined,
    sessionId: binding.sessionId as SessionId | undefined,
    profileId: binding.profileId as ProfileId,
    externalChannelId: binding.externalChannelId,
    externalThreadId: binding.externalThreadId,
    externalUserId: binding.externalUserId,
    conversationProjectId: binding.conversationProjectId,
    conversationChannelId: binding.conversationChannelId,
    providerSubscriptionId: binding.providerSubscriptionId,
    status: binding.status,
  };
}

export async function drainTelegramOutboundMessages(
  context: ServiceAdapterLifecycleContext,
): Promise<void> {
  const connector = context.telegramConnector;
  const subscription = context.telegramOutboundSubscription;
  if (
    context.isStopping() ||
    connector === undefined ||
    subscription === undefined
  ) {
    return;
  }
  const events = await context.bridge.drainSubscriptionEvents(
    subscription,
    128,
  );
  for (const event of events) {
    if (event.type !== "agent_message_routed") continue;
    const projection = context.adapterFactories.projectAgentMessageToChannel(
      event.message,
      activeTelegramChannelBindings(
        context.runtimeConfig.channelBindings,
        context.config.telegram.adapterId,
      ),
      { now: context.now() },
    );
    if (projection.status === "projected") {
      const dispatch =
        await context.adapterFactories.dispatchChannelMessageProjection(
          {
            sendMessage: async (message) => {
              await connector.sendOutbound(message);
            },
            sendActivity: async () => undefined,
          },
          projection.message,
        );
      if (!dispatch.accepted) {
        recordChannelProjectionFailure(
          context,
          projection.binding.bindingId,
          dispatch.kind,
          dispatch.degradedReason,
        );
      }
      continue;
    }
    if (projection.status !== "not_channel_target") {
      recordChannelProjectionFailure(
        context,
        projection.candidates[0]?.bindingId ?? "telegram:unresolved",
        "message",
        projection.reason,
      );
    }
  }
}

export function recordChannelProjectionFailure(
  context: ServiceAdapterLifecycleContext,
  bindingId: string,
  kind: ChannelProjectionFailureRecord["kind"],
  degradedReason: string,
): void {
  context.channelProjectionFailures.push({
    bindingId,
    kind,
    degradedReason,
    observedAt: context.now(),
  });
  context.channelProjectionFailures.splice(
    0,
    Math.max(0, context.channelProjectionFailures.length - 100),
  );
  context.recordEvent({
    source: "telegram",
    eventType: "telegram_projection_degraded",
    severity: "warning",
    summary: `${bindingId}: ${degradedReason}`,
  });
}

export function telegramChannelActivityDiagnostics(
  context: ServiceAdapterLifecycleContext,
  now: string,
): ChannelBindingDiagnostics[] {
  const connector = context.telegramConnector;
  const diagnostics = connector?.diagnostics();
  return activeTelegramChannelBindings(
    context.runtimeConfig.channelBindings,
    context.config.telegram.adapterId,
  ).map((binding) => ({
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    membershipStatus: "joined",
    presenceStatus: connector === undefined ? "offline" : "online",
    subscriptionStatus:
      connector === undefined
        ? "disconnected"
        : diagnostics?.lastError
          ? "degraded"
          : "active",
    degradedReason:
      connector === undefined
        ? context.config.telegram.enabled
          ? "telegram connector is not running"
          : "telegram connector is disabled"
        : diagnostics?.lastError,
    stale:
      connector === undefined ||
      (diagnostics?.lastPollAt === undefined
        ? false
        : Date.parse(now) - Date.parse(diagnostics.lastPollAt) >
          Math.max(30_000, context.config.telegram.pollIntervalMs * 5)),
  }));
}

export function denConversationChannelActivityDiagnostics(
  context: ServiceAdapterLifecycleContext,
): ChannelBindingDiagnostics[] {
  return activeDenChannelBindings(context.runtimeConfig.channelBindings).map(
    (binding) => {
      const resolution =
        context.denConversationChannelResolutionsByBindingId.get(
          binding.bindingId,
        );
      const channelId = resolution?.channelId;
      const membership = context.denConversationMembershipsByBindingId.get(
        binding.bindingId,
      );
      const membershipStatus =
        membership === undefined
          ? "missing"
          : denConversationMembershipStatus(membership.membership_status);
      const subscriptionStatus = denConversationSubscriptionStatus(membership);
      const resolved = channelId !== undefined;
      return {
        bindingId: binding.bindingId,
        adapterId: binding.adapterId,
        conversationProjectId:
          resolution?.projectId ??
          conversationProjectIdForBinding(context.config, binding),
        conversationChannelId: channelId,
        membershipStatus,
        presenceStatus:
          membershipStatus === "joined"
            ? "online"
            : resolved
              ? "offline"
              : "missing",
        subscriptionStatus,
        degradedReason: denConversationDiagnosticReason({
          resolved,
          membership,
          membershipStatus,
          subscriptionStatus,
        }),
        stale: false,
      };
    },
  );
}

function denConversationMembershipStatus(
  status: string,
): ChannelMembershipStatus {
  switch (status) {
    case "active":
      return "joined";
    case "left":
      return "left";
    case "invited":
      return "invited";
    default:
      return "unknown";
  }
}

function denConversationSubscriptionStatus(
  membership: DenSuccessorConversationMembership | undefined,
): ChannelSubscriptionStatus | "missing" {
  if (membership === undefined) return "missing";
  if (membership.membership_status === "left") return "archived";
  if (membership.membership_status !== "active") return "degraded";
  return membership.wake_policy === "never" ? "paused" : "active";
}

function denConversationDiagnosticReason(input: {
  resolved: boolean;
  membership: DenSuccessorConversationMembership | undefined;
  membershipStatus: ChannelMembershipStatus | "missing";
  subscriptionStatus: ChannelSubscriptionStatus | "missing";
}): string | undefined {
  if (!input.resolved) return "conversation channel has not been resolved";
  if (input.membership === undefined) {
    return "conversation membership has not been resolved";
  }
  if (input.membershipStatus !== "joined") {
    return `conversation membership is ${input.membershipStatus}`;
  }
  if (input.subscriptionStatus !== "active") {
    return `conversation subscription is ${input.subscriptionStatus}`;
  }
  return undefined;
}

export function recordDynamicDenDeliveryChannel(
  context: ServiceAdapterLifecycleContext,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
  deliveryBody: {
    channelId?: number;
    sourceMessageId?: number;
    wakePolicy?: ChannelWakePolicy;
    subscriptionStatus?: string;
    lastError?: string;
  },
): void {
  if (deliveryBody.channelId === undefined) return;
  const bindingId = `gateway-delivery:${session.sessionId}:${deliveryBody.channelId}`;
  context.dynamicDenChannelBindings.set(bindingId, {
    bindingId,
    bindingSource: "gateway_delivery",
    adapterId: "den-successor-gateway",
    agentId: session.agentId,
    sessionId: session.sessionId,
    profileId: session.profileId,
    provider: "den_successor_gateway",
    externalChannelId: `conversation:${deliveryBody.channelId}`,
    conversationChannelId: deliveryBody.channelId,
    sourceMessageId: deliveryBody.sourceMessageId,
    deliveryIntentId: intent.id,
    lastObservedAt: context.now(),
    wakePolicy:
      deliveryBody.wakePolicy ?? context.channelWakePolicyForSession(session),
    status: "active",
    membershipStatus: "dynamic",
    presenceStatus: "delivery_intent",
    subscriptionStatus: deliveryBody.subscriptionStatus ?? "active",
    stalePresence: false,
    droppedProjections: 0,
    lastError: deliveryBody.lastError,
  });
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
