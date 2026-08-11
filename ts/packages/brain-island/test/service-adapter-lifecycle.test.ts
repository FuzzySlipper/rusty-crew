import assert from "node:assert/strict";
import test from "node:test";
import type { NormalizedChannelInboundMessage } from "@rusty-crew/contracts";
import type { ServiceWakeDispatchReport } from "../src/service-wake-dispatch.js";
import type {
  NativeBridgeModule,
  NativeInstallDiplomatBindingRecord,
} from "@rusty-crew/native-bridge";
import {
  projectTelegramDiplomatWakeReplies,
  startTelegramConnector,
  telegramBotTokenFromServiceCredentialSecret,
  type ServiceAdapterLifecycleContext,
} from "../src/service-adapter-lifecycle.js";
import type {
  ServiceAdapterFactories,
  TelegramConnectorFactoryInput,
} from "../src/service-adapter-ports.js";

const now = "2026-08-11T08:00:00Z";

const diplomatBinding: NativeInstallDiplomatBindingRecord = {
  schemaVersion: "telegram_install_diplomat.v1",
  bindingId: "binding-1",
  revision: 1,
  installationId: "install-1",
  installationLabel: "Workshop",
  adapterId: "telegram-main",
  botUserId: "42",
  botUsername: "diplomat_bot",
  agentId: "agent-1",
  sessionId: "session-1",
  externalChatId: "-1001",
  participationMode: "mention_or_reply",
  status: "active",
  createdAt: now,
  updatedAt: now,
};

const inboundMessage: NormalizedChannelInboundMessage = {
  kind: "channel_inbound_message.v1",
  adapterId: "telegram-main" as never,
  bindingId: "binding-1",
  runtime: {
    agentId: "agent-1" as never,
    sessionId: "session-1" as never,
    profileId: "profile-1" as never,
  },
  providerRefs: {
    provider: "telegram",
    externalChannelId: "-1001",
    externalMessageId: "message-1",
    externalUserId: "human-1",
  },
  author: {
    kind: "human",
    externalUserId: "human-1",
    username: "operator",
  },
  body: "@diplomat_bot inspect the service",
  attachments: [],
  mentions: ["diplomat_bot"],
  receivedAt: now,
  ttlMs: 300_000,
  expiresAt: "2026-08-11T08:05:00Z",
  idempotencyKey: "telegram:-1001:message-1",
  visibility: "conversation",
  provenance: {},
};

test("Telegram connector unwraps persisted API-key credential envelopes", () => {
  assert.equal(
    telegramBotTokenFromServiceCredentialSecret(
      JSON.stringify({ kind: "api_key", version: 1, value: "bot-token" }),
    ),
    "bot-token",
  );
});

test("Telegram connector preserves legacy raw credentials", () => {
  assert.equal(
    telegramBotTokenFromServiceCredentialSecret("  legacy-bot-token  "),
    "legacy-bot-token",
  );
});

test("Telegram connector rejects incompatible credential envelopes without exposing secrets", () => {
  const secret = "must-not-appear";
  assert.throws(
    () =>
      telegramBotTokenFromServiceCredentialSecret(
        JSON.stringify({
          kind: "openai_oauth",
          version: 1,
          access_token: secret,
        }),
      ),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /version 1 API-key secret/u);
      assert.doesNotMatch(error.message, new RegExp(secret, "u"));
      return true;
    },
  );
});

test("Telegram connector rejects malformed JSON envelopes without exposing input", () => {
  const secret = '{"kind":"api_key","value":"must-not-appear"';
  assert.throws(
    () => telegramBotTokenFromServiceCredentialSecret(secret),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /secret envelope is invalid/u);
      assert.doesNotMatch(error.message, /must-not-appear/u);
      return true;
    },
  );
});

test("Telegram connector rejects array-shaped stored JSON without exposing input", () => {
  const secret = '["must-not-appear"]';
  assert.throws(
    () => telegramBotTokenFromServiceCredentialSecret(secret),
    (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /secret envelope is invalid/u);
      assert.doesNotMatch(error.message, /must-not-appear/u);
      return true;
    },
  );
});

test("routed Telegram diplomat ingress delivers once without generic channel bindings", async () => {
  let connectorInput: TelegramConnectorFactoryInput | undefined;
  let deliveryCount = 0;
  const outbound: Array<Record<string, unknown>> = [];
  let holdNextOutbound = false;
  let releaseHeldOutbound: (() => void) | undefined;
  let heldOutboundStarted: (() => void) | undefined;
  const bridge = {
    getServiceCredentialSecret: async () =>
      JSON.stringify({ kind: "api_key", version: 1, value: "bot-token" }),
    listInstallDiplomatBindings: async () => [diplomatBinding],
    listSessions: async () => [
      {
        sessionId: "session-1",
        agentId: "agent-1",
        profileId: "profile-1",
      },
    ],
    registerPlatformAdapter: async () => undefined,
    subscribeEvents: async () => "subscription-1",
    planTelegramDiplomatIngress: async () => ({
      binding: diplomatBinding,
      decision: "routed",
      reasonCode: "telegram_diplomat_routed",
      sender: { kind: "human", externalUserId: "human-1" },
      targetSessionId: "session-1",
      crewCorrelationId: "telegram-correlation-1",
    }),
    deliverAgentMessage: async (request: Record<string, unknown>) => {
      deliveryCount += 1;
      assert.equal(request.toAddress, "agent-1");
      assert.equal(request.correlationId, "telegram-correlation-1");
      assert.equal(request.inputKind, "operator");
      return {
        status: "accepted",
        sequence: 1,
        activation: {
          type: "direct_brain_wake_requested",
          sessionId: "session-1",
          wakeId: "wake-1",
        },
      };
    },
  } as unknown as NativeBridgeModule;
  const adapterFactories = {
    createTelegramAdapterRegistration: () => ({
      adapterId: "telegram-main" as never,
      kind: "telegram",
      displayName: "Telegram",
    }),
    createTelegramConnector: (input: TelegramConnectorFactoryInput) => {
      connectorInput = input;
      return {
        start: async () => undefined,
        stop: () => undefined,
        pollOnce: async () => undefined,
        sendOutbound: async (message: Record<string, unknown>) => {
          if (holdNextOutbound) {
            holdNextOutbound = false;
            heldOutboundStarted?.();
            await new Promise<void>((resolve) => {
              releaseHeldOutbound = resolve;
            });
          }
          outbound.push(message);
        },
        diagnostics: () => ({ bindingCount: 1 }),
      } as never;
    },
    ingestChannelInboundMessage: async (
      message: NormalizedChannelInboundMessage,
      options: Parameters<
        ServiceAdapterFactories["ingestChannelInboundMessage"]
      >[1],
    ) => {
      assert.deepEqual(options.bindings, []);
      assert.ok(options.routePlanner);
      const resolution = await options.routePlanner({
        message,
        bindings: [],
        now,
      });
      assert.equal(resolution.status, "routed");
      if (resolution.status !== "routed") return resolution;
      assert.equal(resolution.route.sessionId, "session-1");
      assert.ok(options.deliverRoutedMessage);
      await options.deliverRoutedMessage({
        message,
        binding: resolution.binding,
        route: resolution.route,
      });
      return { status: "routed", message } as const;
    },
  } as unknown as ServiceAdapterFactories;
  const context = {
    config: {
      telegram: {
        enabled: true,
        adapterId: "telegram-main",
        credentialId: "telegram-main",
        pollIntervalMs: 2_000,
        pollTimeoutSeconds: 20,
        updateLimit: 50,
        messageTtlMs: 300_000,
      },
      paths: { dataDir: "/tmp/rusty-crew-test" },
    },
    bridge,
    adapterFactories,
    runtimeConfig: { sessions: [], channelBindings: [] },
    telegramConnector: undefined,
    telegramOutboundSubscription: undefined,
    timers: new Set(),
    denConversationChannelResolutionsByBindingId: new Map(),
    denConversationChannelIdsByExternalId: new Map(),
    denConversationMembershipsByBindingId: new Map(),
    dynamicDenChannelBindings: new Map(),
    channelProjectionFailures: [],
    telegramDiplomatPendingReplies: new Map(),
    telegramDiplomatPendingWakeReports: [],
    telegramDiplomatProcessedWakeReports: new Set(),
    telegramDiplomatReplyProjectionRunning: false,
    now: () => now,
    isStopping: () => false,
    recordEvent: () => undefined,
    drainSubscriptionEventsUntilIdle: async () => [],
    createObservationSink: () => ({ publish: async () => undefined }),
    ensureSessionForChannelBinding: async () => undefined,
    channelWakePolicyForSession: () => ({ mode: "immediate" }),
    persistTelegramMedia: async () => {
      throw new Error("not used");
    },
  } as unknown as ServiceAdapterLifecycleContext;

  await startTelegramConnector(context);
  assert.ok(connectorInput);
  const result = await connectorInput.onInbound(inboundMessage);
  assert.equal(result.status, "routed");
  assert.equal(deliveryCount, 1);
  const report = {
    sessionId: "session-1",
    wakeId: "wake-1",
    status: "completed",
    summary: "completed",
    observedEvents: [
      {
        type: "brain_event_observed",
        sessionId: "session-1",
        wakeId: "wake-1",
        event: { type: "text_delta", text: "I will inspect it." },
      },
      {
        type: "brain_event_observed",
        sessionId: "session-1",
        wakeId: "wake-1",
        event: {
          type: "tool_call_started",
          toolName: "inspect_service",
          argumentsJson: "{}",
        },
      },
      {
        type: "brain_event_observed",
        sessionId: "session-1",
        wakeId: "wake-1",
        event: {
          type: "tool_call_finished",
          toolName: "inspect_service",
          resultJson: "{}",
          isError: false,
        },
      },
      {
        type: "brain_event_observed",
        sessionId: "session-1",
        wakeId: "wake-1",
        event: { type: "text_delta", text: "Service " },
      },
      {
        type: "brain_event_observed",
        sessionId: "session-1",
        wakeId: "wake-1",
        event: { type: "text_delta", text: "is healthy." },
      },
    ],
  } as unknown as ServiceWakeDispatchReport;
  await Promise.all([
    projectTelegramDiplomatWakeReplies(context, [report]),
    projectTelegramDiplomatWakeReplies(context, [report]),
  ]);
  await projectTelegramDiplomatWakeReplies(context, [report]);
  assert.equal(outbound.length, 1);
  assert.equal(outbound[0]?.body, "Service is healthy.");
  assert.equal(outbound[0]?.bindingId, "binding-1");
  assert.equal(outbound[0]?.replyToExternalMessageId, "message-1");
  assert.equal(outbound[0]?.correlationId, "telegram-correlation-1");
  assert.equal(
    outbound[0]?.idempotencyKey,
    "telegram-diplomat-reply:telegram:-1001:message-1",
  );

  const nextInbound = {
    ...inboundMessage,
    providerRefs: {
      ...inboundMessage.providerRefs,
      externalMessageId: "message-2",
    },
    idempotencyKey: "telegram:-1001:message-2",
  };
  await connectorInput.onInbound(nextInbound);
  await projectTelegramDiplomatWakeReplies(context, [
    {
      ...report,
      wakeId: "wake-2",
      observedEvents: [
        ...(report.observedEvents ?? []),
        {
          type: "logical_turn_lifecycle_observed",
          lifecycle: { kind: "cancelled" },
        },
      ],
    } as never,
  ]);
  assert.equal(outbound.length, 1);
  assert.equal(context.telegramDiplomatPendingReplies.size, 0);

  const thirdInbound = {
    ...inboundMessage,
    providerRefs: {
      ...inboundMessage.providerRefs,
      externalMessageId: "message-3",
    },
    idempotencyKey: "telegram:-1001:message-3",
  };
  const fourthInbound = {
    ...inboundMessage,
    providerRefs: {
      ...inboundMessage.providerRefs,
      externalMessageId: "message-4",
    },
    idempotencyKey: "telegram:-1001:message-4",
  };
  await connectorInput.onInbound(thirdInbound);
  await connectorInput.onInbound(fourthInbound);
  holdNextOutbound = true;
  const heldStarted = new Promise<void>((resolve) => {
    heldOutboundStarted = resolve;
  });
  const firstProjection = projectTelegramDiplomatWakeReplies(context, [
    {
      ...report,
      wakeId: "wake-3",
      observedEvents: [
        {
          type: "brain_event_observed",
          sessionId: "session-1",
          wakeId: "wake-3",
          event: { type: "text_delta", text: "First final reply." },
        },
      ],
    } as never,
  ]);
  await heldStarted;
  const duplicateProjection = projectTelegramDiplomatWakeReplies(context, [
    {
      ...report,
      wakeId: "wake-3",
      observedEvents: [
        {
          type: "brain_event_observed",
          sessionId: "session-1",
          wakeId: "wake-3",
          event: { type: "text_delta", text: "First final reply." },
        },
      ],
    } as never,
  ]);
  const secondProjection = projectTelegramDiplomatWakeReplies(context, [
    {
      ...report,
      wakeId: "wake-4",
      observedEvents: [
        {
          type: "brain_event_observed",
          sessionId: "session-1",
          wakeId: "wake-4",
          event: { type: "text_delta", text: "Second final reply." },
        },
      ],
    } as never,
  ]);
  releaseHeldOutbound?.();
  await Promise.all([firstProjection, duplicateProjection, secondProjection]);
  assert.deepEqual(
    outbound.slice(1).map((message) => message.body),
    ["First final reply.", "Second final reply."],
  );
  assert.equal(context.telegramDiplomatPendingReplies.size, 0);
  assert.equal(context.telegramDiplomatPendingWakeReports.length, 0);
});
