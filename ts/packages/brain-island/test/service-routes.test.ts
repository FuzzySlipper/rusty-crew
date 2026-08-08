import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import type { IncomingMessage, ServerResponse } from "node:http";
import test from "node:test";
import type {
  AgentId,
  ExternalAgentSessionCreationRequest,
  ProfileId,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type {
  NativeModelProviderRecord,
  NativeProfileRegistryRecord,
  NativeProfileRegistryWrite,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import {
  handleExternalRuntimeRequest,
  type ExternalRuntimeRouteContext,
} from "../src/service-external-runtime-routes.js";
import {
  handleCoordinationOperatorRequest,
  isCoordinationOperatorRoute,
  type CoordinationOperatorRouteContext,
} from "../src/service-coordination-operator-routes.js";
import {
  ExternalBindingProfileRefreshError,
  ExternalThreadLifecycleError,
} from "../src/service-external-runtime.js";
import { parseExternalRuntimeCommand } from "../src/external-runtime-commands.js";
import type { AdminRouteResult } from "../src/admin-diagnostics-api.js";
import { handleAdminContextStrategiesRequest } from "../src/service-context-strategy-routes.js";
import {
  handleAdminMcpCatalogRequest,
  mcpServerCatalogEntries,
  mcpServerIdFromEndpointRef,
  type AdminMcpCatalogRouteContext,
} from "../src/service-mcp-catalog-routes.js";
import {
  handleSchedulerReadRequest,
  pageParams,
  scheduledJobStatusParam,
  scheduledRunStatusParam,
  scheduledRunTriggerParam,
} from "../src/service-scheduler-routes.js";
import { handleAdminToolsCatalogRequest } from "../src/service-tool-catalog-routes.js";
import {
  handleAdminLocalToolProfilesRequest,
  localToolProfileIdFromPath,
} from "../src/service-local-tool-profile-routes.js";
import {
  handleModelProviderAdminRequest,
  type ModelProviderAdminRouteContext,
} from "../src/service-model-provider-routes.js";
import {
  handleServiceCredentialAdminRequest,
  type ServiceCredentialAdminRouteContext,
} from "../src/service-credential-admin-routes.js";
import {
  handleRustyViewChatStreamRequest,
  isChatRoute,
  writeSseEvent,
  type ChatStreamSubscriber,
  type RustyViewChatStreamRouteContext,
} from "../src/service-chat-stream-routes.js";
import {
  handleProfileRegistryWriteRequest,
  isProfileRegistryWriteRoute,
  parseProfileRegistryWriteRoute,
  type ProfileRegistryRoutePlan,
  type ProfileRegistryWriteRouteContext,
} from "../src/service-profile-registry-routes.js";
import { handleBrowserProfileLoreLayersRequest } from "../src/roleplay/lore-routes.js";
import { type RoleplayRouteContext } from "../src/service-roleplay-routes.js";
import type { ChatEvent } from "../src/rusty-view-chat-api.js";
import type {
  LocalToolProfile,
  LocalToolProfileList,
  LocalToolProfileStore,
  LocalToolProfileWrite,
} from "../src/local-tool-profiles.js";

test("coordination operator routes are deployment-bound and start system rounds", async () => {
  let capturedRound: Record<string, unknown> | undefined;
  const context = {
    deploymentRole: "debug",
    bridge: {
      async listAgentDirectory() {
        return [{ agentId: "agent-a", routable: true }];
      },
      async listAgentMessageTraffic(query: Record<string, unknown>) {
        return [{ status: "queued", query }];
      },
      async beginAgentRound(command: Record<string, unknown>) {
        capturedRound = command;
        return {
          round: {
            roundId: command.roundId,
            recipientAgentId: command.toAddress,
            status: "pending",
            terminalReasonCode: null,
          },
          delivery: {
            request: { deliveryId: `round-delivery:${command.roundId}` },
          },
        };
      },
    },
    now: () => "2026-07-12T00:00:00.000Z",
    requestId: () => "req-coordination-operator",
    readJsonBody: async () => ({
      toAddress: "agent-a",
      body: "reply through the correlated round",
      roundId: "round-a",
      idempotencyKey: "round-key-a",
      messageId: "message-a",
      correlationId: "correlation-a",
      ttlMs: 5_000,
    }),
  } as unknown as CoordinationOperatorRouteContext;

  assert.equal(isCoordinationOperatorRoute("/v1/coordination/agents"), true);
  assert.equal(
    isCoordinationOperatorRoute("/v1/debug/coordination/agents"),
    true,
  );
  const wrongRole = await handleCoordinationOperatorRequest(
    { method: "GET" } as IncomingMessage,
    new URL("http://local/v1/coordination/agents"),
    context,
  );
  assert.equal((wrongRole as AdminRouteResult).status, 409);
  assert.equal(
    errorReason(wrongRole as AdminRouteResult),
    "coordination_deployment_role_mismatch",
  );

  const listed = await handleCoordinationOperatorRequest(
    { method: "GET" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/agents"),
    context,
  );
  assert.equal(
    okData<{ deploymentRole: string }>(listed as AdminRouteResult)
      .deploymentRole,
    "debug",
  );

  const started = await handleCoordinationOperatorRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/rounds"),
    context,
  );
  assert.equal(
    okData<{ roundId: string }>(started as AdminRouteResult).roundId,
    "round-a",
  );
  assert.deepEqual(capturedRound?.caller, {
    type: "system",
    senderAgentId: "rusty-crew-debug-operator",
  });
  assert.match(
    String(capturedRound?.body),
    /recipient rusty-crew-debug-operator/,
  );
  assert.match(String(capturedRound?.body), /correlationId correlation-a/);

  const inbox = await handleCoordinationOperatorRequest(
    { method: "GET" } as IncomingMessage,
    new URL(
      "http://local/v1/debug/coordination/messages?toAgentId=agent-a&toSessionId=session-a&fromAgentId=agent-b&fromSessionId=session-b&correlationId=corr-a&messageId=message-a&limit=25",
    ),
    context,
  );
  assert.deepEqual(
    okData<{ items: unknown[] }>(inbox as AdminRouteResult).items,
    [
      {
        status: "queued",
        query: {
          toAgentId: "agent-a",
          toSessionId: "session-a",
          fromAgentId: "agent-b",
          fromSessionId: "session-b",
          correlationId: "corr-a",
          messageId: "message-a",
          limit: 25,
        },
      },
    ],
  );
});

test("raw agent route resolution exposes typed session ambiguity", async () => {
  const context = {
    deploymentRole: "debug",
    bridge: {
      async resolveAgentAddress() {
        throw new Error(
          "ActionRejected: agent_session_ambiguous: agent shared has multiple active sessions; specify session_id; candidate_session_ids=[session-a,session-b]",
        );
      },
    },
    now: () => "2026-07-12T00:00:00.000Z",
    requestId: () => "req-agent-session-ambiguity",
    readJsonBody: async () => ({ address: "shared" }),
  } as unknown as CoordinationOperatorRouteContext;

  const result = (await handleCoordinationOperatorRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/routes/resolve"),
    context,
  )) as AdminRouteResult;

  assert.equal(result.status, 409);
  assert.equal(errorReason(result), "agent_session_ambiguous");
  assert.equal(result.body.ok, false);
  if (!result.body.ok) {
    assert.match(result.body.error.message, /session-a,session-b/);
  }
});

test("coordination switchboard CRUD resolves and tests exact role-bound addresses", async () => {
  let requestBody: Record<string, unknown> = {};
  let savedRoute: Record<string, unknown> | undefined;
  let capturedDelivery: Record<string, unknown> | undefined;
  const resolution = () => ({
    address: "@reviewer",
    route: savedRoute,
    routable: true,
    resolvedTarget: {
      agentId: "review-agent",
      sessionId: "review-session",
      profileId: "review-profile",
      displayLabel: "Reviewer",
      runtimeKind: "direct_brain",
    },
  });
  const context = {
    deploymentRole: "debug",
    bridge: {
      async listAgentRouteResolutions() {
        return savedRoute === undefined ? [] : [resolution()];
      },
      async putAgentRoute(write: Record<string, unknown>) {
        savedRoute = {
          ...write,
          revision: write.expectedRevision === undefined ? 1 : 2,
          createdAt: "2026-07-20T00:00:00.000Z",
        };
        return savedRoute;
      },
      async getAgentRouteResolution() {
        return savedRoute === undefined ? undefined : resolution();
      },
      async resolveAgentAddress() {
        return resolution();
      },
      async deleteAgentRoute() {
        const deleted = savedRoute;
        savedRoute = undefined;
        return deleted;
      },
      async deliverAgentMessage(command: Record<string, unknown>) {
        capturedDelivery = command;
        return {
          request: {
            deliveryId: command.deliveryId,
            requestedAddress: command.toAddress,
            toAgentId: "review-agent",
          },
          status: "accepted",
          revision: 2,
        };
      },
    },
    now: () => "2026-07-20T00:00:00.000Z",
    requestId: () => "req-switchboard",
    readJsonBody: async () => requestBody,
    settleDelivery: async (receipt: unknown) => receipt,
  } as unknown as CoordinationOperatorRouteContext;

  requestBody = {
    routeKey: "reviewer",
    label: "Reviewer",
    enabled: true,
    target: {
      type: "direct_brain",
      agentId: "review-agent",
      sessionId: "review-session",
    },
    requiredRuntimeKind: "direct_brain",
  };
  const created = await handleCoordinationOperatorRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/routes"),
    context,
  );
  assert.equal(
    okData<{ route: { revision: number } }>(created as AdminRouteResult).route
      .revision,
    1,
  );

  const listed = await handleCoordinationOperatorRequest(
    { method: "GET" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/routes"),
    context,
  );
  assert.equal(
    okData<{ routes: unknown[] }>(listed as AdminRouteResult).routes.length,
    1,
  );

  requestBody = { address: "@reviewer" };
  const resolved = await handleCoordinationOperatorRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/routes/resolve"),
    context,
  );
  assert.equal(
    okData<{ resolution: { address: string } }>(resolved as AdminRouteResult)
      .resolution.address,
    "@reviewer",
  );

  requestBody = { body: "live route proof", ttlMs: 5_000 };
  const tested = await handleCoordinationOperatorRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/routes/reviewer/test"),
    context,
  );
  assert.equal(
    okData<{ status: string }>(tested as AdminRouteResult).status,
    "accepted",
  );
  assert.equal(capturedDelivery?.toAddress, "@reviewer");

  requestBody = {
    label: "Reviewer queue",
    enabled: true,
    expectedRevision: 1,
    target: {
      type: "direct_brain",
      agentId: "review-agent",
      sessionId: "review-session",
    },
  };
  const updated = await handleCoordinationOperatorRequest(
    { method: "PATCH" } as IncomingMessage,
    new URL("http://local/v1/debug/coordination/routes/reviewer"),
    context,
  );
  assert.equal(
    okData<{ route: { revision: number } }>(updated as AdminRouteResult).route
      .revision,
    2,
  );
  const deleted = await handleCoordinationOperatorRequest(
    { method: "DELETE" } as IncomingMessage,
    new URL(
      "http://local/v1/debug/coordination/routes/reviewer?expectedRevision=2",
    ),
    context,
  );
  assert.equal(
    okData<{ route: { routeKey: string } }>(deleted as AdminRouteResult).route
      .routeKey,
    "reviewer",
  );
});

test("external session route translates generated Den task reference wire fields", async () => {
  let captured: ExternalAgentSessionCreationRequest | undefined;
  const body = {
    idempotencyKey: "view:create:task-ref",
    runtimeId: "codex-local",
    profileId: "asha-planner",
    cwd: "/home/dev/asha",
    taskRef: {
      project_id: "asha",
      task_id: "4281",
    },
    label: "Asha planning agent",
  };
  const context = {
    bridge: {},
    controller: {
      async createAgentSession(request: ExternalAgentSessionCreationRequest) {
        captured = request;
        return {
          creation: {
            binding: {
              bindingId: "binding-1",
              profilePromptSnapshot: "private prompt body",
            },
          },
          runtime: {},
          thread: {},
        };
      },
    },
    now: () => "2026-07-11T20:00:00.000Z",
    requestId: () => "req-external-session-create",
    readJsonBody: async () => body,
  } as unknown as ExternalRuntimeRouteContext;

  const result = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-agent-sessions"),
    context,
  );

  const data = okData<{
    creation: { binding: Record<string, unknown> };
  }>(result as AdminRouteResult);
  assert.equal("profilePromptSnapshot" in data.creation.binding, false);
  assert.deepEqual(captured?.taskRef, {
    projectId: "asha",
    taskId: "4281",
  });
  assert.equal(captured?.requestedAt, "2026-07-11T20:00:00.000Z");
});

test("external binding metadata route requires explicit nullable fields", async () => {
  let captured: Record<string, unknown> | undefined;
  const context = {
    bridge: {},
    controller: {
      async updateBindingMetadata(input: Record<string, unknown>) {
        captured = input;
        return { bindingId: input.bindingId, revision: 3 };
      },
    },
    requestId: () => "req-external-binding-metadata",
    readJsonBody: async () => ({
      expectedRevision: 2,
      label: null,
      taskRef: { project_id: "asha", task_id: "4281" },
    }),
  } as unknown as ExternalRuntimeRouteContext;

  const result = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/metadata"),
    context,
  );
  assert.equal((result as AdminRouteResult).status, 200);
  assert.deepEqual(captured, {
    bindingId: "binding-1",
    expectedRevision: 2,
    label: null,
    taskRef: { projectId: "asha", taskId: "4281" },
  });

  const invalid = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/metadata"),
    {
      ...context,
      readJsonBody: async () => ({ expectedRevision: 2, label: null }),
    },
  );
  assert.equal((invalid as AdminRouteResult).status, 400);
  assert.equal(
    errorReason(invalid as AdminRouteResult),
    "external_binding_metadata_invalid_request",
  );
});

test("external binding fleet exposes profile state without prompt bodies", async () => {
  const bindings = [{ bindingId: "binding-1" }];
  const persistedBindings = [
    { bindingId: "binding-1", profilePromptSnapshot: "private prompt body" },
  ];
  const profileStates = [
    {
      bindingId: "binding-1",
      profileId: "profile-1",
      state: "stale",
      refreshRequired: true,
      appliedProfileRevision: 2,
      appliedPromptHash: "a".repeat(64),
      currentProfileRevision: 3,
      currentPromptHash: "b".repeat(64),
    },
  ];
  const result = await handleExternalRuntimeRequest(
    { method: "GET" } as IncomingMessage,
    new URL("http://local/v1/external-bindings"),
    {
      bridge: {
        async listExternalBindings() {
          return persistedBindings;
        },
      },
      controller: {
        async bindingProfileStates() {
          return profileStates;
        },
      },
      requestId: () => "req-external-binding-list",
    } as unknown as ExternalRuntimeRouteContext,
  );
  assert.deepEqual(okData(result as AdminRouteResult), {
    bindings,
    profileStates,
  });
});

test("external binding profile refresh route carries both concurrency guards", async () => {
  let captured: Record<string, unknown> | undefined;
  const expectedProfilePromptHash = "c".repeat(64);
  const context = {
    bridge: {},
    controller: {
      async refreshBindingProfileInstructions(input: Record<string, unknown>) {
        captured = input;
        return { outcome: "thread_replaced" };
      },
    },
    requestId: () => "req-external-binding-profile-refresh",
    readJsonBody: async () => ({
      expectedBindingRevision: 4,
      expectedNativeThreadId: "native-thread-1",
      expectedProfileRevision: 7,
      expectedProfilePromptHash,
    }),
  } as unknown as ExternalRuntimeRouteContext;
  const url = new URL(
    "http://local/v1/external-bindings/binding-1/profile-refresh",
  );
  const result = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    url,
    context,
  );
  assert.equal((result as AdminRouteResult).status, 200);
  assert.deepEqual(captured, {
    bindingId: "binding-1",
    expectedBindingRevision: 4,
    expectedNativeThreadId: "native-thread-1",
    expectedProfileRevision: 7,
    expectedProfilePromptHash,
  });

  const conflict = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    url,
    {
      ...context,
      controller: {
        async refreshBindingProfileInstructions() {
          throw new ExternalBindingProfileRefreshError(
            "external_binding_profile_refresh_revision_conflict",
            "binding changed",
            true,
          );
        },
      },
    } as unknown as ExternalRuntimeRouteContext,
  );
  assert.equal((conflict as AdminRouteResult).status, 409);
  assert.equal(
    errorReason(conflict as AdminRouteResult),
    "external_binding_profile_refresh_revision_conflict",
  );
});

test("external binding restore route preserves explicit selected identities", async () => {
  let captured: Record<string, unknown> | undefined;
  const context = {
    bridge: {},
    controller: {
      async restoreBinding(input: Record<string, unknown>) {
        captured = input;
        return { outcome: "restored" };
      },
    },
    requestId: () => "req-external-binding-restore",
    readJsonBody: async () => ({
      expectedBindingRevision: 7,
      expectedSessionId: "crew-session-1",
      expectedAgentId: "crew-agent-1",
      expectedProfileId: "crew-profile-1",
      expectedNativeThreadId: "native-thread-1",
    }),
  } as unknown as ExternalRuntimeRouteContext;

  const result = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/restore"),
    context,
  );
  assert.equal((result as AdminRouteResult).status, 200);
  assert.deepEqual(captured, {
    bindingId: "binding-1",
    expectedBindingRevision: 7,
    expectedSessionId: "crew-session-1",
    expectedAgentId: "crew-agent-1",
    expectedProfileId: "crew-profile-1",
    expectedNativeThreadId: "native-thread-1",
  });
});

test("external runtime promotion readiness projects exact Rust-owned blockers", async () => {
  const registration = { runtimeId: "runtime-1", observedState: "ready" };
  const activeBinding = {
    bindingId: "binding-1",
    runtimeId: "runtime-1",
    status: "active",
    profilePromptSnapshot: "private prompt body",
  };
  const context = {
    bridge: {
      async getExternalRuntime() {
        return registration;
      },
      async listExternalBindings() {
        return [
          activeBinding,
          { bindingId: "archived", runtimeId: "runtime-1", status: "archived" },
          { bindingId: "other", runtimeId: "runtime-2", status: "active" },
        ];
      },
      async listActiveExternalTurns() {
        return [
          { runtimeId: "runtime-1", request: { requestId: "turn-1" } },
          { runtimeId: "runtime-2", request: { requestId: "turn-2" } },
        ];
      },
      async listPendingExternalInteractions() {
        return [
          { runtimeId: "runtime-1", interactionId: "interaction-1" },
          { runtimeId: "runtime-2", interactionId: "interaction-2" },
        ];
      },
    },
    controller: {
      statuses() {
        return [
          { runtimeId: "runtime-1", driverState: "ready" },
          { runtimeId: "runtime-2", driverState: "ready" },
        ];
      },
    },
    requestId: () => "req-promotion-readiness",
  } as unknown as ExternalRuntimeRouteContext;

  const result = await handleExternalRuntimeRequest(
    { method: "GET" } as IncomingMessage,
    new URL(
      "http://local/v1/admin/external-runtime-promotion-readiness?runtimeId=runtime-1",
    ),
    context,
  );
  assert.deepEqual(okData(result as AdminRouteResult), {
    registration,
    controller: { runtimeId: "runtime-1", driverState: "ready" },
    activeBindings: [
      {
        bindingId: "binding-1",
        runtimeId: "runtime-1",
        status: "active",
      },
    ],
    activeTurns: [{ runtimeId: "runtime-1", request: { requestId: "turn-1" } }],
    pendingInteractions: [
      { runtimeId: "runtime-1", interactionId: "interaction-1" },
    ],
  });
});

test("external thread lifecycle routes expose archive, delete, restore, and archived listing", async () => {
  const calls: string[] = [];
  const context = {
    bridge: {
      async getExternalRuntime(runtimeId: string) {
        return { runtimeId };
      },
    },
    controller: {
      async listThreads(_runtimeId: string, params: unknown) {
        calls.push(`list:${JSON.stringify(params)}`);
        return { items: [], nextCursor: null, backwardsCursor: null };
      },
      async archiveThread(runtimeId: string, threadId: string) {
        calls.push(`archive:${runtimeId}:${threadId}`);
        return {
          runtimeId,
          threadId,
          action: "archive",
          outcome: "applied",
          nativeArchived: true,
          bindings: [],
        };
      },
      async deleteThread(runtimeId: string, threadId: string) {
        calls.push(`delete:${runtimeId}:${threadId}`);
        return {
          runtimeId,
          threadId,
          action: "delete",
          outcome: "applied",
          nativeDeleted: true,
          bindings: [],
        };
      },
      async unarchiveThread(runtimeId: string, threadId: string) {
        calls.push(`unarchive:${runtimeId}:${threadId}`);
        return {
          runtimeId,
          threadId,
          action: "unarchive",
          outcome: "applied",
          nativeArchived: false,
          bindings: [],
        };
      },
    },
    requestId: () => "req-external-thread-lifecycle",
  } as unknown as ExternalRuntimeRouteContext;

  const listed = await handleExternalRuntimeRequest(
    { method: "GET" } as IncomingMessage,
    new URL(
      "http://local/v1/external-runtimes/runtime-1/threads?archived=true&limit=20",
    ),
    context,
  );
  assert.deepEqual(okData(listed as AdminRouteResult), {
    items: [],
    nextCursor: null,
    backwardsCursor: null,
  });
  const archived = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL(
      "http://local/v1/external-runtimes/runtime-1/threads/thread%2F1/archive",
    ),
    context,
  );
  assert.equal(
    okData<{ outcome: string }>(archived as AdminRouteResult).outcome,
    "applied",
  );
  const deleted = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL(
      "http://local/v1/external-runtimes/runtime-1/threads/thread%2F1/delete",
    ),
    context,
  );
  assert.equal(
    okData<{ nativeDeleted: boolean }>(deleted as AdminRouteResult)
      .nativeDeleted,
    true,
  );
  const restored = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL(
      "http://local/v1/external-runtimes/runtime-1/threads/thread%2F1/unarchive",
    ),
    context,
  );
  assert.equal(
    okData<{ nativeArchived: boolean }>(restored as AdminRouteResult)
      .nativeArchived,
    false,
  );
  assert.deepEqual(calls, [
    'list:{"limit":20,"archived":true}',
    "archive:runtime-1:thread/1",
    "delete:runtime-1:thread/1",
    "unarchive:runtime-1:thread/1",
  ]);

  const rejectingContext = {
    ...context,
    controller: {
      ...context.controller,
      async archiveThread() {
        throw new ExternalThreadLifecycleError(
          "external_thread_active",
          "thread is active",
        );
      },
    },
  } as unknown as ExternalRuntimeRouteContext;
  const conflict = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL(
      "http://local/v1/external-runtimes/runtime-1/threads/thread-1/archive",
    ),
    rejectingContext,
  );
  assert.equal((conflict as AdminRouteResult).status, 409);
  assert.equal(
    errorReason(conflict as AdminRouteResult),
    "external_thread_active",
  );
});

test("external runtime event head reads one indexed tail cursor", async () => {
  let query: Record<string, unknown> | undefined;
  const latestEvent = { eventId: "event-latest", sequenceId: 1_000_000 };
  const context = {
    bridge: {
      async getExternalRuntime(runtimeId: string) {
        return { runtimeId };
      },
      async queryExternalRuntimeEvents(input: Record<string, unknown>) {
        query = input;
        return [latestEvent];
      },
    },
    controller: {},
    requestId: () => "req-external-event-head",
  } as unknown as ExternalRuntimeRouteContext;

  const result = await handleExternalRuntimeRequest(
    { method: "GET" } as IncomingMessage,
    new URL("http://local/v1/external-runtimes/runtime-1/events/head"),
    context,
  );

  assert.deepEqual(okData(result as AdminRouteResult), {
    event: latestEvent,
  });
  assert.deepEqual(query, {
    runtimeId: "runtime-1",
    afterSequence: 0,
    limit: 1,
    tail: true,
  });
});

test("external command routes are typed and recognized commands cannot leak to messages", async () => {
  let body: Record<string, unknown> = {};
  let delivered = false;
  let deliveredCommand: Record<string, unknown> | undefined;
  const context = {
    bridge: {
      async getExternalBinding() {
        return {
          bindingId: "binding-1",
          runtimeId: "runtime-1",
          agentId: "agent-1",
          nativeThreadId: "thread-1",
          revision: 3,
        };
      },
      async getAgentMessageDelivery() {
        return undefined;
      },
      async deliverAgentMessage(command: Record<string, unknown>) {
        delivered = true;
        deliveredCommand = command;
        return { status: "accepted", request: command };
      },
    },
    controller: {
      async commandCatalog() {
        return { bindingId: "binding-1", commands: [{ name: "status" }] };
      },
      async executeCommand(input: {
        commandInput: string;
        idempotencyKey: string;
      }) {
        const parsed = parseExternalRuntimeCommand(input.commandInput);
        return {
          command: parsed.command,
          idempotencyKey: input.idempotencyKey,
        };
      },
      async applyCoordinationDelivery(receipt: unknown) {
        return receipt;
      },
    },
    now: () => "2026-07-12T00:00:00.000Z",
    requestId: () => "req-external-command",
    readJsonBody: async () => body,
  } as unknown as ExternalRuntimeRouteContext;

  const catalog = await handleExternalRuntimeRequest(
    { method: "GET" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/commands"),
    context,
  );
  assert.deepEqual(
    okData<{ commands: unknown[] }>(catalog as AdminRouteResult).commands,
    [{ name: "status" }],
  );

  body = { input: "/status", idempotencyKey: "view-command-1" };
  const executed = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/commands"),
    context,
  );
  assert.deepEqual(okData(executed as AdminRouteResult), {
    command: "status",
    idempotencyKey: "view-command-1",
  });

  body = { input: "/not-a-command", idempotencyKey: "view-command-2" };
  const unknown = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/commands"),
    context,
  );
  assert.equal((unknown as AdminRouteResult).status, 400);
  assert.equal(
    errorReason(unknown as AdminRouteResult),
    "external_command_unknown",
  );

  body = { body: "/status" };
  const leaked = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/messages"),
    context,
  );
  assert.equal((leaked as AdminRouteResult).status, 409);
  assert.equal(
    errorReason(leaked as AdminRouteResult),
    "external_command_requires_command_route",
  );
  assert.equal(delivered, false);

  body = { body: "plain operator prompt" };
  const submitted = await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/messages"),
    context,
  );
  assert.equal((submitted as AdminRouteResult).status, 200);
  assert.equal(deliveredCommand?.inputKind, "operator");
  assert.equal(deliveredCommand?.body, "plain operator prompt");
});

test("external control routes preserve Rust-owned precondition reason codes", async () => {
  let rejection =
    "ActionRejected: external_control_native_turn_conflict: expected native turn is not the binding's active turn";
  let bindingExists = true;
  const context = {
    bridge: {
      async getExternalBinding() {
        if (!bindingExists) return undefined;
        return {
          bindingId: "binding-1",
          runtimeId: "runtime-1",
          nativeThreadId: "thread-1",
          revision: 4,
        };
      },
    },
    controller: {
      async executeControl() {
        throw new Error(rejection);
      },
    },
    now: () => "2026-07-15T00:00:00.000Z",
    requestId: () => "req-external-control-conflict",
    readJsonBody: async () => ({
      kind: "steer_turn",
      expectedNativeTurnId: "turn-completed",
      payload: {
        threadId: "thread-1",
        turnId: "turn-completed",
        input: [{ type: "text", text: "follow up" }],
      },
    }),
  } as unknown as ExternalRuntimeRouteContext;

  const staleTurn = (await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/controls"),
    context,
  )) as AdminRouteResult;
  assert.equal(staleTurn.status, 409);
  assert.equal(errorReason(staleTurn), "external_control_native_turn_conflict");
  assert.equal(staleTurn.body.ok, false);
  if (!staleTurn.body.ok) {
    assert.equal(staleTurn.body.error.code, "conflict");
    assert.equal(staleTurn.body.error.retryable, false);
    assert.match(staleTurn.body.error.message, /binding binding-1/);
    assert.match(staleTurn.body.error.message, /native turn turn-completed/);
  }

  rejection =
    "ActionRejected: external_control_binding_revision_conflict: external binding revision mismatch: expected 3, found 4";
  const staleBinding = (await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/controls"),
    context,
  )) as AdminRouteResult;
  assert.equal(staleBinding.status, 409);
  assert.equal(
    errorReason(staleBinding),
    "external_control_binding_revision_conflict",
  );
  assert.equal(staleBinding.body.ok, false);
  if (!staleBinding.body.ok) {
    assert.equal(staleBinding.body.error.code, "conflict");
    assert.equal(staleBinding.body.error.retryable, true);
  }

  bindingExists = false;
  const missingBinding = (await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-missing/controls"),
    context,
  )) as AdminRouteResult;
  assert.equal(missingBinding.status, 404);
  assert.equal(
    errorReason(missingBinding),
    "external_control_binding_not_found",
  );
});

test("external control routes discard workspace overrides", async () => {
  let capturedControl: Record<string, unknown> | undefined;
  const context = {
    bridge: {
      async getExternalBinding() {
        return {
          bindingId: "binding-1",
          runtimeId: "runtime-1",
          nativeThreadId: "thread-1",
          revision: 4,
        };
      },
    },
    controller: {
      async executeControl(control: Record<string, unknown>) {
        capturedControl = control;
        return { status: "applied" };
      },
    },
    now: () => "2026-07-15T00:00:00.000Z",
    requestId: () => "req-external-control-workspace",
    readJsonBody: async () => ({
      kind: "start_or_resume_thread",
      payload: {
        cwd: "/tmp/attacker-cwd",
        environments: [{ environmentId: "attacker", cwd: "/tmp/attacker-cwd" }],
        model: "allowed-payload-field",
      },
    }),
  } as unknown as ExternalRuntimeRouteContext;

  const result = (await handleExternalRuntimeRequest(
    { method: "POST" } as IncomingMessage,
    new URL("http://local/v1/external-bindings/binding-1/controls"),
    context,
  )) as AdminRouteResult;

  assert.equal(result.status, 200);
  assert.deepEqual(capturedControl?.payload, {
    model: "allowed-payload-field",
  });
});

test("roleplay lore layer route delegates browser reads through the bridge boundary", async () => {
  const calls: string[] = [];
  const context = {
    bridge: {
      async listLoreLayers(profileId: string) {
        calls.push(`layers:${profileId}`);
        return [
          {
            layer_id: "world-details",
            profile_id: profileId,
            name: "World Details",
          },
          {
            layer_id: "story-events",
            profile_id: profileId,
            name: "Story Events",
          },
        ];
      },
      async listEntriesByLayer(layerId: string) {
        calls.push(`entries:${layerId}`);
        if (layerId === "world-details") {
          return [{ record_id: "fact-1" }, { record_id: "fact-2" }];
        }
        return [{ record_id: "event-1" }];
      },
    },
    runtimeConfig: { profilesDir: "/tmp/profiles" },
    now: () => "2026-07-07T00:00:00.000Z",
    async applyServiceRuntimeConfigFromDisk() {
      return {};
    },
    async serviceSessionById() {
      throw new Error("serviceSessionById should not be used by lore layers");
    },
    listChatEventsAfterCursor() {
      return [];
    },
  } as unknown as RoleplayRouteContext;

  const result = await handleBrowserProfileLoreLayersRequest(
    {
      method: "GET",
      headers: { "x-request-id": "req-roleplay-lore-layers" },
    } as unknown as IncomingMessage,
    context,
    new URL("http://local/v1/profile/rp-runner/layers"),
    "rp-runner",
  );

  assert.deepEqual(calls, [
    "layers:rp-runner",
    "entries:world-details",
    "entries:story-events",
  ]);
  const data = okData<{
    profileId: string;
    total: number;
    entryCounts: Record<string, number>;
    layers: Array<Record<string, unknown>>;
  }>(result);
  assert.equal(data.profileId, "rp-runner");
  assert.equal(data.total, 2);
  assert.deepEqual(data.entryCounts, {
    "story-events": 1,
    "world-details": 2,
  });
  assert.equal(data.layers[0]?.entry_count, 2);
  assert.equal(data.layers[0]?.entryCount, 2);
});

test("model provider admin routes list, project records, and report revision conflicts", async () => {
  const context = modelProviderRouteContext([
    modelProviderRecord({
      alias: "deepseek-flash",
      temperatureMilli: 500,
      revision: 2,
    }),
  ]);

  const invalidStatus = await handleModelProviderAdminRequest(
    {
      method: "GET",
      url: "http://local/v1/admin/model-providers?status=bogus",
      requestId: "req-model-provider",
    },
    context,
  );
  assert.equal(invalidStatus.status, 400);
  assert.equal(errorReason(invalidStatus), "invalid_model_provider_status");

  const listed = await handleModelProviderAdminRequest(
    {
      method: "GET",
      url: "http://local/v1/admin/model-providers?status=active&limit=5&offset=1",
      requestId: "req-model-provider",
    },
    context,
  );
  assert.deepEqual(context.observedQueries, [
    { status: "active", aliasPrefix: undefined, limit: 5, offset: 1 },
  ]);
  const listData = okData<{
    items: Array<NativeModelProviderRecord & { temperature?: number }>;
    total: number;
    limit: number;
    offset: number;
  }>(listed);
  assert.equal(listData.total, 1);
  assert.equal(listData.limit, 5);
  assert.equal(listData.offset, 1);
  assert.equal(listData.items[0]?.alias, "deepseek-flash");
  assert.equal(listData.items[0]?.temperature, 0.5);

  const conflict = await handleModelProviderAdminRequest(
    {
      method: "PATCH",
      url: "http://local/v1/admin/model-providers/deepseek-flash",
      requestId: "req-model-provider",
      body: {
        modelId: "deepseek/deepseek-chat",
        expectedRevision: 1,
      },
    },
    context,
  );
  assert.equal(conflict.status, 409);
  assert.equal(errorReason(conflict), "model_provider_revision_mismatch");
  const conflictData = (
    conflict.body as {
      data: {
        provider?: NativeModelProviderRecord & { temperature?: number };
        expectedRevision: number;
        currentRevision: number;
      };
    }
  ).data;
  assert.equal(conflictData.provider?.alias, "deepseek-flash");
  assert.equal(conflictData.provider?.temperature, 0.5);
  assert.equal(conflictData.expectedRevision, 1);
  assert.equal(conflictData.currentRevision, 2);

  const deepseek = await handleModelProviderAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/model-providers",
      requestId: "req-deepseek-policy",
      body: {
        alias: "deepseek-v4-pro",
        status: "active",
        protocol: "chat_completions",
        providerKind: "deepseek",
        modelId: "deepseek-v4-pro",
        chatCompletionsDialect: "deepseek",
        thinkingMode: "enabled",
        reasoningHistory: "tool_calls_only",
      },
    },
    context,
  );
  assert.equal(deepseek.status, 200);
  const deepseekData = okData<{
    provider: NativeModelProviderRecord;
  }>(deepseek);
  assert.equal(deepseekData.provider.chatCompletionsDialect, "deepseek");
  assert.equal(deepseekData.provider.thinkingMode, "enabled");
  assert.equal(deepseekData.provider.reasoningHistory, "tool_calls_only");
});

test("model provider admin routes type validation failures without mutating create or update state", async () => {
  const existing = modelProviderRecord({
    alias: "standard-model",
    chatCompletionsDialect: "standard",
    thinkingMode: "provider_default",
    reasoningHistory: "provider_default",
    revision: 3,
  });
  const context = modelProviderRouteContext([existing]);
  const persist = context.upsertModelProvider.bind(context);
  context.upsertModelProvider = async (write) => {
    if (
      write.chatCompletionsDialect === "kimi" &&
      write.thinkingMode === "enabled" &&
      write.temperatureMilli !== undefined
    ) {
      throw new Error(
        "InvalidInput: kimi thinking models do not accept a temperature override",
      );
    }
    if (
      write.chatCompletionsDialect === "standard" &&
      write.reasoningHistory === "preserve_all"
    ) {
      throw new Error(
        "InvalidInput: standard chat completions dialect does not accept vendor thinking settings",
      );
    }
    return persist(write);
  };

  const rejectedCreate = await handleModelProviderAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/model-providers",
      requestId: "req-invalid-kimi-create",
      body: {
        alias: "invalid-kimi",
        protocol: "chat_completions",
        providerKind: "moonshot",
        modelId: "kimi-k2.7",
        chatCompletionsDialect: "kimi",
        thinkingMode: "enabled",
        reasoningHistory: "preserve_all",
        temperature: 0.5,
        maxOutputTokens: 16_000,
      },
    },
    context,
  );
  assert.equal(rejectedCreate.status, 400);
  assert.deepEqual(errorDetails(rejectedCreate), {
    code: "invalid_input",
    reason_code: "invalid_model_provider",
    message: "kimi thinking models do not accept a temperature override",
    retryable: false,
  });
  const missingCreate = await handleModelProviderAdminRequest(
    {
      method: "GET",
      url: "http://local/v1/admin/model-providers/invalid-kimi",
      requestId: "req-invalid-kimi-readback",
    },
    context,
  );
  assert.equal(missingCreate.status, 404);

  const rejectedUpdate = await handleModelProviderAdminRequest(
    {
      method: "PATCH",
      url: "http://local/v1/admin/model-providers/standard-model",
      requestId: "req-invalid-standard-update",
      body: {
        modelId: existing.modelId,
        expectedRevision: existing.revision,
        chatCompletionsDialect: "standard",
        thinkingMode: "enabled",
        reasoningHistory: "preserve_all",
      },
    },
    context,
  );
  assert.equal(rejectedUpdate.status, 400);
  assert.equal(errorReason(rejectedUpdate), "invalid_model_provider");
  const unchangedUpdate = okData<NativeModelProviderRecord>(
    await handleModelProviderAdminRequest(
      {
        method: "GET",
        url: "http://local/v1/admin/model-providers/standard-model",
        requestId: "req-invalid-standard-readback",
      },
      context,
    ),
  );
  assert.deepEqual(unchangedUpdate, existing);

  const invalidRefresh = await handleModelProviderAdminRequest(
    {
      method: "PATCH",
      url: "http://local/v1/admin/model-providers/standard-model?refresh=eventually",
      requestId: "req-invalid-provider-refresh",
      body: {
        modelId: "would-have-mutated",
        expectedRevision: existing.revision,
      },
    },
    context,
  );
  assert.equal(invalidRefresh.status, 400);
  assert.equal(errorReason(invalidRefresh), "invalid_model_provider");
  const unchangedRefresh = okData<NativeModelProviderRecord>(
    await handleModelProviderAdminRequest(
      {
        method: "GET",
        url: "http://local/v1/admin/model-providers/standard-model",
        requestId: "req-invalid-refresh-readback",
      },
      context,
    ),
  );
  assert.deepEqual(unchangedRefresh, existing);
});

test("model provider OpenAI OAuth routes expose status and start without leaking verifier", async () => {
  const context = modelProviderRouteContext(
    [
      modelProviderRecord({
        alias: "gpt",
        protocol: "responses",
        responsesDialect: "openai_stateful",
        providerKind: "openai",
        modelId: "gpt-5",
        credentialId: "openai:primary",
        credential: { hasSecret: true, kind: "openai_oauth", revision: 1 },
      }),
    ],
    [serviceCredentialRecord({ credentialId: "openai:primary" })],
  );

  const status = await handleModelProviderAdminRequest(
    {
      method: "GET",
      url: "http://local/v1/admin/model-providers/gpt/oauth/openai/status",
      requestId: "req-openai-oauth",
    },
    context,
  );
  assert.equal(status.status, 200);
  const statusData = okData<{
    provider: NativeModelProviderRecord;
    credential: NativeModelProviderRecord["credential"];
    loginConfig: { remoteOperatorFlow: string; redirectUri: string };
    pendingLogins: unknown[];
  }>(status);
  assert.equal(statusData.provider.alias, "gpt");
  assert.equal(statusData.credential.kind, "openai_oauth");
  assert.equal(statusData.loginConfig.remoteOperatorFlow, "paste_callback_url");
  assert.equal(
    statusData.loginConfig.redirectUri,
    "http://localhost:1455/auth/callback",
  );
  assert.deepEqual(statusData.pendingLogins, []);

  const start = await handleModelProviderAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/model-providers/gpt/oauth/openai/start",
      requestId: "req-openai-oauth",
      body: { allowedWorkspaceIds: ["workspace-a"] },
    },
    context,
  );
  assert.equal(start.status, 200);
  const startData = okData<{
    pendingLogin: {
      pendingLoginId: string;
      providerAlias: string;
      codeChallenge: string;
      authorizationUrl: string;
      codeVerifier?: string;
    };
  }>(start);
  assert.equal(startData.pendingLogin.providerAlias, "gpt");
  assert.equal(startData.pendingLogin.codeVerifier, undefined);
  assert.match(
    startData.pendingLogin.pendingLoginId,
    /^openai-oauth:openai:primary:/,
  );
  assert.match(
    startData.pendingLogin.authorizationUrl,
    /id_token_add_organizations=true/,
  );
  assert.match(
    startData.pendingLogin.authorizationUrl,
    /codex_cli_simplified_flow=true/,
  );
  assert.equal(context.pendingLogins.size, 1);
});

test("shared credential routes complete OAuth once and guard linked clear and delete", async () => {
  const context = modelProviderRouteContext([
    modelProviderRecord({
      alias: "gpt-main",
      protocol: "responses",
      responsesDialect: "openai_stateful",
      providerKind: "openai",
      modelId: "gpt-5",
    }),
    modelProviderRecord({
      alias: "gpt-fast",
      protocol: "responses",
      responsesDialect: "openai_stateful",
      providerKind: "openai",
      modelId: "gpt-5-mini",
    }),
  ]);

  const created = await handleServiceCredentialAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/service-credentials",
      requestId: "req-shared-credential",
      body: {
        credentialId: "openai:shared",
        displayName: "Shared OpenAI login",
        providerKind: "openai",
        credentialKind: "openai_oauth",
      },
    },
    context,
  );
  assert.equal(created.status, 200);

  for (const alias of ["gpt-main", "gpt-fast"]) {
    const linked = await handleServiceCredentialAdminRequest(
      {
        method: "POST",
        url: `http://local/v1/admin/service-credentials/openai%3Ashared/providers/${alias}/link`,
        requestId: "req-shared-credential",
      },
      context,
    );
    assert.equal(linked.status, 200);
  }

  const started = await handleServiceCredentialAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/service-credentials/openai%3Ashared/oauth/openai/start",
      requestId: "req-shared-credential",
    },
    context,
  );
  const pending = okData<{
    pendingLogin: { pendingLoginId: string; credentialId: string };
  }>(started).pendingLogin;
  const privatePending = context.pendingLogins.get(pending.pendingLoginId);
  assert.ok(privatePending);
  assert.equal(pending.credentialId, "openai:shared");

  const completed = await handleServiceCredentialAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/service-credentials/openai%3Ashared/oauth/openai/complete",
      requestId: "req-shared-credential",
      body: {
        pendingLoginId: pending.pendingLoginId,
        state: privatePending.state,
        testMode: true,
        fakeTokenResponse: {
          idToken: "id-token-must-stay-private",
          accessToken: "access-token-must-stay-private",
          refreshToken: "refresh-token-must-stay-private",
        },
      },
    },
    context,
  );
  assert.equal(completed.status, 200);
  const completedJson = JSON.stringify(completed.body);
  assert.doesNotMatch(completedJson, /token-must-stay-private/u);
  const completedCredential = okData<{
    credential: NativeServiceCredentialRecord;
  }>(completed).credential;
  assert.equal(completedCredential.credential.hasSecret, true);
  assert.deepEqual(completedCredential.linkedProviderAliases.sort(), [
    "gpt-fast",
    "gpt-main",
  ]);

  const impact = await handleServiceCredentialAdminRequest(
    {
      method: "GET",
      url: "http://local/v1/admin/service-credentials/openai%3Ashared/impact",
      requestId: "req-shared-credential",
    },
    context,
  );
  const impactData = okData<{
    linkedProviderAliases: string[];
    canClear: boolean;
    canDelete: boolean;
  }>(impact);
  assert.equal(impactData.linkedProviderAliases.length, 2);
  assert.equal(impactData.canClear, false);
  assert.equal(impactData.canDelete, false);

  const linkedClear = await handleServiceCredentialAdminRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/service-credentials/openai%3Ashared/clear",
      requestId: "req-shared-credential",
    },
    context,
  );
  assert.equal(linkedClear.status, 409);
  assert.equal(errorReason(linkedClear), "service_credential_linked");
  const linkedDelete = await handleServiceCredentialAdminRequest(
    {
      method: "DELETE",
      url: `http://local/v1/admin/service-credentials/openai%3Ashared?expectedRevision=${completedCredential.revision}`,
      requestId: "req-shared-credential",
    },
    context,
  );
  assert.equal(linkedDelete.status, 409);

  for (const alias of ["gpt-main", "gpt-fast"]) {
    const unlinked = await handleServiceCredentialAdminRequest(
      {
        method: "POST",
        url: `http://local/v1/admin/service-credentials/openai%3Ashared/providers/${alias}/unlink`,
        requestId: "req-shared-credential",
      },
      context,
    );
    assert.equal(unlinked.status, 200);
  }
  const current = await context.getServiceCredential("openai:shared");
  assert.ok(current);
  const deleted = await handleServiceCredentialAdminRequest(
    {
      method: "DELETE",
      url: `http://local/v1/admin/service-credentials/openai%3Ashared?expectedRevision=${current.revision}`,
      requestId: "req-shared-credential",
    },
    context,
  );
  assert.equal(deleted.status, 200);
  assert.equal(await context.getServiceCredential("openai:shared"), undefined);
});

test("profile registry write route wrapper plans, applies, and maps missing records", async () => {
  assert.deepEqual(
    parseProfileRegistryWriteRoute(
      "/v1/admin/profiles/registry/field-profile/runtime-config/apply",
    ),
    {
      profileId: "field-profile",
      kind: "runtime-config",
      mode: "apply",
    },
  );
  assert.equal(
    isProfileRegistryWriteRoute(
      "/v1/admin/profiles/registry/field-profile/prompt/plan",
    ),
    true,
  );
  assert.equal(
    isProfileRegistryWriteRoute("/v1/admin/profiles/registry/field-profile"),
    false,
  );

  const context = profileRegistryRouteContext();
  const methodFailure = await handleProfileRegistryWriteRequest(
    {
      method: "GET",
      url: "http://local/v1/admin/profiles/registry/field-profile/update/plan",
      requestId: "req-profile-registry",
    },
    context,
  );
  assert.equal(methodFailure.status, 405);
  assert.equal(
    errorReason(methodFailure),
    "profile_registry_write_requires_post_or_patch",
  );

  const missingRoute = await handleProfileRegistryWriteRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/profiles/registry/field-profile/unknown/plan",
      requestId: "req-profile-registry",
    },
    context,
  );
  assert.equal(missingRoute.status, 404);
  assert.equal(
    errorReason(missingRoute),
    "unknown_profile_registry_write_route",
  );

  context.failMissing = true;
  const missingRecord = await handleProfileRegistryWriteRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/profiles/registry/missing-profile/update/plan",
      requestId: "req-profile-registry",
      body: { expectedRevision: 1 },
    },
    context,
  );
  assert.equal(missingRecord.status, 404);
  assert.equal(errorReason(missingRecord), "profile_registry_record_missing");

  context.failMissing = false;
  context.calls.length = 0;
  const plan = await handleProfileRegistryWriteRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/profiles/registry/field-profile/update/plan",
      requestId: "req-profile-registry",
      body: { expectedRevision: 1 },
    },
    context,
  );
  assert.equal(plan.status, 200);
  assert.equal(okData<{ mode: string; kind: string }>(plan).mode, "plan");
  assert.deepEqual(context.calls, ["plan:update:plan"]);

  const blockedApply = await handleProfileRegistryWriteRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/profiles/registry/field-profile/prompt/apply",
      requestId: "req-profile-registry",
      body: { expectedRevision: 1, forceInvalidPlan: true },
    },
    context,
  );
  assert.equal(blockedApply.status, 200);
  assert.equal(okData<{ ok: boolean }>(blockedApply).ok, false);
  assert.deepEqual(context.calls.slice(-1), ["plan:prompt:apply"]);

  const lifecycleApply = await handleProfileRegistryWriteRequest(
    {
      method: "PATCH",
      url: "http://local/v1/admin/profiles/registry/field-profile/lifecycle/apply",
      requestId: "req-profile-registry",
      body: { expectedRevision: 1 },
    },
    context,
  );
  assert.equal(lifecycleApply.status, 200);
  const lifecycleData = okData<{
    applied: boolean;
    record: NativeProfileRegistryRecord;
    effects: unknown;
  }>(lifecycleApply);
  assert.equal(lifecycleData.applied, true);
  assert.equal(lifecycleData.record.profileId, "field-profile");
  assert.deepEqual(lifecycleData.effects, { lifecycleEffects: true });

  const runtimeApply = await handleProfileRegistryWriteRequest(
    {
      method: "POST",
      url: "http://local/v1/admin/profiles/registry/field-profile/runtime-config/apply",
      requestId: "req-profile-registry",
      body: { expectedRevision: 2 },
    },
    context,
  );
  assert.equal(runtimeApply.status, 200);
  assert.equal(
    okData<{ effects: unknown }>(runtimeApply).effects,
    context.runtimeEffects,
  );
  assert.deepEqual(
    context.calls.slice(-5),
    [
      "plan:update:plan",
      "plan:prompt:apply",
      "plan:lifecycle:apply",
      "update:field-profile:1",
      "lifecycle:field-profile",
      "runtime-plan:runtime-config:apply",
      "update:field-profile:2",
      "runtime:field-profile",
    ].slice(-5),
  );
});

test("Rusty View chat stream route validates, replays once, and cleans subscribers", async () => {
  assert.equal(isChatRoute("/v1/chat"), true);
  assert.equal(isChatRoute("/v1/chat/sessions/field-session"), true);
  assert.equal(isChatRoute("/v1/admin/chat"), false);

  const context = chatStreamContext();
  const notStream = await handleRustyViewChatStreamRequest(
    requestLike("GET"),
    new URL("http://local/v1/chat/sessions/field-session/events"),
    context,
  );
  assert.equal(notStream, undefined);

  context.readAttachmentContent = async (sessionId, attachmentId) => {
    assert.equal(sessionId, "field-session");
    assert.equal(attachmentId, "attachment-1");
    return {
      attachment: {
        attachment_id: attachmentId,
        session_id: sessionId,
        status: "active",
        filename: "generated.png",
        mime_type: "image/png",
        byte_size: 3,
        extracted_text_truncated: false,
        metadata_json: {},
        created_at: "2026-07-05T00:00:00.000Z",
        updated_at: "2026-07-05T00:00:00.000Z",
        links: [],
      },
      bytes: Buffer.from("png"),
    };
  };
  const attachmentContent = await handleRustyViewChatStreamRequest(
    requestLike("GET", { origin: "http://view.test" }),
    new URL(
      "http://local/v1/chat/sessions/field-session/attachments/attachment-1/content",
    ),
    context,
  );
  assert.equal(attachmentContent && "kind" in attachmentContent, true);
  const attachmentResponse = fakeResponse();
  if (attachmentContent && "kind" in attachmentContent) {
    attachmentContent.write(attachmentResponse);
  }
  assert.equal(attachmentResponse.statusCode, 200);
  assert.equal(attachmentResponse.headers["content-type"], "image/png");
  assert.equal(
    attachmentResponse.headers["access-control-allow-origin"],
    "http://view.test",
  );
  assert.equal(attachmentResponse.body, "png");

  const methodFailure = await handleRustyViewChatStreamRequest(
    requestLike("POST", { "x-request-id": "req-chat-stream" }),
    new URL("http://local/v1/chat/sessions/field-session/stream"),
    context,
  );
  if (methodFailure === undefined || "kind" in methodFailure) {
    assert.fail("method failure should return an admin route envelope");
  }
  if (typeof methodFailure.body === "string") {
    assert.fail("method failure should not return a string route response");
  }
  assert.equal(methodFailure.status, 405);
  assert.equal(
    errorReason(methodFailure as AdminRouteResult),
    "chat_stream_requires_get",
  );

  const missing = await handleRustyViewChatStreamRequest(
    requestLike("GET", { "x-request-id": "req-chat-stream" }),
    new URL("http://local/v1/chat/sessions/missing-session/stream"),
    context,
  );
  if (missing === undefined || "kind" in missing) {
    assert.fail("missing session should return an admin route envelope");
  }
  if (typeof missing.body === "string") {
    assert.fail("missing session should not return a string route response");
  }
  assert.equal(missing.status, 404);
  assert.equal(
    errorReason(missing as AdminRouteResult),
    "chat_session_not_found",
  );

  const replayOnly = await handleRustyViewChatStreamRequest(
    requestLike("GET", { origin: "http://view.test" }),
    new URL("http://local/v1/chat/sessions/field-session/stream?once=true"),
    context,
  );
  assert.equal(replayOnly && "kind" in replayOnly, true);
  const replayResponse = fakeResponse();
  if (replayOnly && "kind" in replayOnly) replayOnly.write(replayResponse);
  assert.equal(replayResponse.statusCode, 200);
  assert.equal(
    replayResponse.headers["content-type"],
    "text/event-stream; charset=utf-8",
  );
  assert.equal(
    replayResponse.headers["access-control-allow-origin"],
    "http://view.test",
  );
  assert.match(replayResponse.body, /: connected/);
  assert.match(replayResponse.body, /event: message_created/);
  assert.match(replayResponse.body, /hello from replay/);
  assert.equal(replayResponse.ended, true);
  assert.equal(context.subscribers.size, 0);

  const live = await handleRustyViewChatStreamRequest(
    requestLike("GET"),
    new URL("http://local/v1/chat/sessions/field-session/stream"),
    context,
  );
  const liveResponse = fakeResponse();
  if (live && "kind" in live) live.write(liveResponse);
  assert.equal(context.subscribers.size, 1);
  const subscriber = [...context.subscribers][0];
  subscriber?.write(
    chatEvent("field-session", 7, "assistant_text_delta", { text: "live" }),
  );
  assert.match(liveResponse.body, /event: assistant_text_delta/);
  liveResponse.emit("close");
  assert.equal(context.subscribers.size, 0);
  assert.equal(context.deletedSubscriberSessions[0], "field-session");

  const directResponse = fakeResponse();
  writeSseEvent(
    directResponse,
    chatEvent("field-session", 8, "assistant_turn_finished", {
      status: "completed",
    }),
  );
  assert.match(directResponse.body, /id: field-session:8/);
});

test("scheduler diagnostics routes validate methods and filters", async () => {
  const methodFailure = await handleSchedulerReadRequest(
    {
      method: "POST",
      requestId: "req-scheduler",
      url: new URL("http://local/v1/admin/scheduler/jobs"),
    },
    schedulerContext(),
  );
  assert.equal(methodFailure.status, 405);
  assert.equal(errorReason(methodFailure), "read_only_route");

  assert.equal(scheduledJobStatusParam("active"), "active");
  assert.equal(scheduledJobStatusParam("bogus"), "invalid");
  assert.equal(scheduledRunStatusParam("completed"), "completed");
  assert.equal(scheduledRunStatusParam("bogus"), "invalid");
  assert.equal(scheduledRunTriggerParam("manual"), "manual");
  assert.equal(scheduledRunTriggerParam("bogus"), "invalid");
  assert.deepEqual(
    pageParams(
      new URL("http://local/v1/admin/scheduler/jobs?limit=7&offset=2"),
    ),
    { limit: 7, offset: 2 },
  );

  const jobs = await handleSchedulerReadRequest(
    {
      method: "GET",
      requestId: "req-scheduler",
      url: new URL(
        "http://local/v1/admin/scheduler/jobs?status=paused&jobKind=cleanup&limit=5",
      ),
    },
    schedulerContext(),
  );
  assert.equal(jobs.status, 200);
  assert.deepEqual(okData<{ jobs: unknown[] }>(jobs).jobs, [
    {
      status: "paused",
      jobKind: "cleanup",
      limit: 5,
    },
  ]);

  const runs = await handleSchedulerReadRequest(
    {
      method: "GET",
      requestId: "req-scheduler",
      url: new URL(
        "http://local/v1/admin/scheduler/runs?status=failed&trigger=due&targetSessionId=session-a",
      ),
    },
    schedulerContext(),
  );
  assert.equal(runs.status, 200);
  assert.deepEqual(okData<{ runs: unknown[] }>(runs).runs, [
    {
      status: "failed",
      trigger: "due",
      targetSessionId: "session-a",
    },
  ]);
});

test("MCP catalog route merges configured servers and resolves bindings", async () => {
  const context: AdminMcpCatalogRouteContext = {
    config: {
      mcp: {
        baseUrl: "http://compat.example/mcp",
        servers: [
          {
            id: "alpha",
            label: "Alpha",
            baseUrl: "http://alpha.example/mcp",
            transport: "streamable_http" as const,
            source: "env" as const,
          },
        ],
      },
    },
    runtimeConfig: {
      mcpServers: [
        {
          id: "beta",
          label: "Beta",
          baseUrl: "http://beta.example/mcp",
          transport: "streamable_http" as const,
          requestTimeoutMs: 1000,
          source: "runtime" as const,
        },
      ],
      mcpBindings: [
        {
          bindingId: "binding-beta",
          adapterId: "mcp-ts-main" as never,
          agentId: "agent-a" as AgentId,
          sessionId: "session-a" as SessionId,
          profileId: "profile-a" as ProfileId,
          serverNames: ["beta"],
          endpointRef: "config://mcp/beta",
          transport: "streamable_http",
          toolProfileKey: "prime",
          status: "active",
          diagnostics: {},
        },
        {
          bindingId: "binding-compat",
          adapterId: "mcp-ts-main" as never,
          agentId: "agent-a" as AgentId,
          sessionId: "session-a" as SessionId,
          profileId: "profile-a" as ProfileId,
          serverNames: ["missing"],
          endpointRef: "config://mcp/missing",
          transport: "streamable_http",
          toolProfileKey: "review",
          status: "degraded",
          degradedReason: "missing server",
          diagnostics: {},
        },
      ],
    },
  };

  assert.equal(mcpServerIdFromEndpointRef("config://mcp/beta"), "beta");
  assert.equal(
    mcpServerIdFromEndpointRef("https://example.test/mcp"),
    undefined,
  );
  assert.deepEqual(
    mcpServerCatalogEntries(context).map((server) => server.id),
    ["alpha", "beta"],
  );

  const result = await handleAdminMcpCatalogRequest(
    { method: "GET", requestId: "req-mcp" },
    context,
  );
  const catalog = okData<{
    schemaVersion: number;
    servers: Array<{ id: string; configuredBindingCount: number }>;
    toolProfiles: string[];
    bindings: Array<{ resolvedServerId?: string }>;
  }>(result);
  assert.equal(result.status, 200);
  assert.equal(catalog.schemaVersion, 1);
  assert.deepEqual(
    catalog.servers.map((server) => [server.id, server.configuredBindingCount]),
    [
      ["alpha", 0],
      ["beta", 1],
    ],
  );
  assert.deepEqual(catalog.toolProfiles, ["prime", "review"]);
  assert.equal(catalog.bindings[0].resolvedServerId, "beta");
  assert.equal(catalog.bindings[1].resolvedServerId, undefined);
});

test("tool and context catalog routes are read-only envelopes", async () => {
  const toolFailure = await handleAdminToolsCatalogRequest({
    method: "PATCH",
    requestId: "req-tools",
  });
  assert.equal(toolFailure.status, 405);
  assert.equal(errorReason(toolFailure), "tool_catalog_read_only");

  const toolCatalog = await handleAdminToolsCatalogRequest({
    method: "GET",
    requestId: "req-tools",
  });
  assert.equal(toolCatalog.status, 200);
  assert.equal(
    Array.isArray(okData<{ tools: unknown[] }>(toolCatalog).tools),
    true,
  );

  const contextFailure = await handleAdminContextStrategiesRequest({
    method: "POST",
    requestId: "req-context",
  });
  assert.equal(contextFailure.status, 405);
  assert.equal(
    errorReason(contextFailure),
    "context_strategy_catalog_read_only",
  );

  const contextCatalog = await handleAdminContextStrategiesRequest({
    method: "GET",
    requestId: "req-context",
  });
  assert.equal(contextCatalog.status, 200);
  assert.ok(
    Object.keys(
      okData<{ strategies: Record<string, unknown> }>(contextCatalog)
        .strategies,
    ).length > 0,
  );
});

test("local tool profile routes create, read, delete, and report store errors", async () => {
  const store = inMemoryLocalToolProfileStore();
  assert.equal(
    localToolProfileIdFromPath("/v1/admin/local-tool-profiles/full-agent"),
    "full-agent",
  );
  assert.equal(
    localToolProfileIdFromPath("/v1/admin/local-tool-profiles/full/extra"),
    undefined,
  );

  const methodFailure = await handleAdminLocalToolProfilesRequest(
    {
      method: "PUT",
      requestId: "req-local-tools",
      url: new URL("http://local/v1/admin/local-tool-profiles"),
      readBody: async () => undefined,
    },
    { store },
  );
  assert.equal(methodFailure.status, 405);
  assert.equal(
    errorReason(methodFailure),
    "local_tool_profiles_method_not_allowed",
  );

  const create = await handleAdminLocalToolProfilesRequest(
    {
      method: "POST",
      requestId: "req-local-tools",
      url: new URL("http://local/v1/admin/local-tool-profiles"),
      readBody: async () => ({
        id: "full-agent",
        displayName: "Full Agent",
        toolsets: ["default"],
        tools: ["read_file"],
      }),
    },
    { store },
  );
  assert.equal(create.status, 200);
  assert.equal(
    okData<{ profile: LocalToolProfile }>(create).profile.id,
    "full-agent",
  );

  const get = await handleAdminLocalToolProfilesRequest(
    {
      method: "GET",
      requestId: "req-local-tools",
      url: new URL("http://local/v1/admin/local-tool-profiles/full-agent"),
      readBody: async () => undefined,
    },
    { store },
  );
  assert.equal(get.status, 200);
  assert.deepEqual(
    okData<{ profile: LocalToolProfile }>(get).profile.toolsets,
    ["default"],
  );

  const missing = await handleAdminLocalToolProfilesRequest(
    {
      method: "GET",
      requestId: "req-local-tools",
      url: new URL("http://local/v1/admin/local-tool-profiles/missing"),
      readBody: async () => undefined,
    },
    { store },
  );
  assert.equal(missing.status, 404);
  assert.equal(errorReason(missing), "local_tool_profile_not_found");

  const deleted = await handleAdminLocalToolProfilesRequest(
    {
      method: "DELETE",
      requestId: "req-local-tools",
      url: new URL("http://local/v1/admin/local-tool-profiles/full-agent"),
      readBody: async () => undefined,
    },
    { store },
  );
  assert.equal(deleted.status, 200);
  assert.equal(okData<{ deleted: boolean }>(deleted).deleted, true);
});

function schedulerContext() {
  return {
    listScheduledJobs: async (input: unknown) => [input],
    listScheduledRuns: async (input: unknown) => [input],
  };
}

function chatStreamContext() {
  const session = sessionState("field-session");
  const subscribers = new Set<ChatStreamSubscriber>();
  const deletedSubscriberSessions: string[] = [];
  const context: RustyViewChatStreamRouteContext & {
    subscribers: Set<ChatStreamSubscriber>;
    deletedSubscriberSessions: string[];
  } = {
    subscribers,
    deletedSubscriberSessions,
    timers: new Set(),
    async listSessions() {
      return [session];
    },
    async streamReplayEvents(replaySession, cursor, url) {
      assert.equal(replaySession.sessionId, "field-session");
      assert.equal(cursor, undefined);
      assert.equal(url.pathname, "/v1/chat/sessions/field-session/stream");
      return [
        chatEvent("field-session", 1, "message_created", {
          body: "hello from replay",
        }),
      ];
    },
    subscribersForSession(sessionId) {
      assert.equal(sessionId, "field-session");
      return subscribers;
    },
    deleteSubscribersForSession(sessionId) {
      deletedSubscriberSessions.push(sessionId);
    },
    corsHeaders(request) {
      return {
        "access-control-allow-origin":
          typeof request.headers.origin === "string"
            ? request.headers.origin
            : "*",
      };
    },
    async readAttachmentContent() {
      throw new Error(
        "attachment content is not configured in this route fixture",
      );
    },
  };
  return context;
}

function sessionState(sessionId: string): SessionState {
  return {
    sessionId: sessionId as SessionId,
    agentId: "field-agent" as AgentId,
    profileId: "field-profile" as ProfileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: { allowedTools: [] },
    handle: 1 as never,
    status: "idle",
    brainTurnCount: 0,
    createdAt: "2026-07-05T00:00:00.000Z",
    lastActiveAt: "2026-07-05T00:00:00.000Z",
  } as unknown as SessionState;
}

function chatEvent(
  sessionId: string,
  sequence: number,
  kind: ChatEvent["kind"],
  payload: Record<string, unknown>,
): ChatEvent {
  return {
    event_id: `${sessionId}:${sequence}`,
    session_id: sessionId,
    sequence_id: sequence,
    created_at: "2026-07-05T00:00:00.000Z",
    kind,
    payload,
  };
}

function requestLike(
  method: string,
  headers: Record<string, string> = {},
): IncomingMessage {
  return { method, headers } as IncomingMessage;
}

function fakeResponse(): ServerResponse & {
  body: string;
  headers: Record<string, string>;
  ended: boolean;
} {
  const response = new EventEmitter() as ServerResponse & {
    body: string;
    headers: Record<string, string>;
    ended: boolean;
  };
  response.body = "";
  response.headers = {};
  response.ended = false;
  response.destroyed = false;
  response.writeHead = (statusCode, headers) => {
    response.statusCode = statusCode;
    response.headers = Object.fromEntries(
      Object.entries(headers ?? {}).map(([key, value]) => [key, String(value)]),
    );
    return response;
  };
  response.write = (chunk: unknown) => {
    response.body += String(chunk);
    return true;
  };
  response.end = (chunk?: unknown) => {
    if (chunk !== undefined) response.body += String(chunk);
    response.ended = true;
    return response;
  };
  return response;
}

function profileRegistryRouteContext() {
  const calls: string[] = [];
  const runtimeEffects = { runtimeEffects: true };
  const context: ProfileRegistryWriteRouteContext & {
    calls: string[];
    failMissing: boolean;
    runtimeEffects: unknown;
  } = {
    calls,
    failMissing: false,
    runtimeEffects,
    async planRegistryWrite(route, body) {
      calls.push(`plan:${route.kind}:${route.mode}`);
      if (context.failMissing)
        throw missingProfileRegistryRecord(route.profileId);
      const record = profileRegistryRecord(route.profileId);
      return {
        ok: !(
          typeof body === "object" &&
          body !== null &&
          "forceInvalidPlan" in body
        ),
        profileId: route.profileId,
        kind: route.kind,
        mode: route.mode,
        expectedRevision: 1,
        current: record,
        next: record,
        nextWrite: profileRegistryWrite(record),
        diagnostics: [],
      } as ProfileRegistryRoutePlan & Record<string, unknown>;
    },
    async planRuntimeConfigWrite(route) {
      calls.push(`runtime-plan:${route.kind}:${route.mode}`);
      if (context.failMissing)
        throw missingProfileRegistryRecord(route.profileId);
      const record = profileRegistryRecord(route.profileId, { revision: 2 });
      return {
        ok: true,
        profileId: route.profileId,
        mode: route.mode,
        expectedRevision: 2,
        current: record,
        next: record,
        nextWrite: profileRegistryWrite(record),
        runtimeConfig: { providerAlias: "default", mcpBindings: [] },
        diagnostics: [],
      } as ProfileRegistryRoutePlan & Record<string, unknown>;
    },
    async updateProfileRegistryRecord(input) {
      calls.push(`update:${input.write.profileId}:${input.expectedRevision}`);
      return profileRegistryRecord(input.write.profileId, {
        revision: input.expectedRevision + 1,
      });
    },
    async applyLifecycleEffects(record) {
      calls.push(`lifecycle:${record.profileId}`);
      return { lifecycleEffects: true };
    },
    async applyPromptEffects(record) {
      calls.push(`prompt:${record.profileId}`);
      return { promptEffects: true };
    },
    async applyRuntimeConfigEffects(record) {
      calls.push(`runtime:${record.profileId}`);
      return runtimeEffects;
    },
  };
  return context;
}

function missingProfileRegistryRecord(profileId: string): Error {
  return new Error(
    `profile registry record ${profileId} was not found; create or import a DB-backed profile before registry mutation`,
  );
}

function profileRegistryRecord(
  profileId: string,
  overrides: Partial<NativeProfileRegistryRecord> = {},
): NativeProfileRegistryRecord {
  return {
    profileId,
    lifecycleStatus: overrides.lifecycleStatus ?? "active",
    displayName: overrides.displayName,
    summary: overrides.summary,
    defaultSessionKind: overrides.defaultSessionKind ?? "full",
    agentId: overrides.agentId ?? `${profileId}-agent`,
    ownerId: overrides.ownerId,
    promptSoulMarkdown: overrides.promptSoulMarkdown,
    promptMemoryMarkdown: overrides.promptMemoryMarkdown,
    activeRuntimeSettingsJson: overrides.activeRuntimeSettingsJson ?? {},
    sourceAssetRefs: overrides.sourceAssetRefs ?? [],
    derivedRuntimeRefs: overrides.derivedRuntimeRefs ?? [],
    importExport: overrides.importExport ?? { metadataJson: {} },
    revision: overrides.revision ?? 1,
    createdAt: overrides.createdAt ?? "2026-07-05T00:00:00.000Z",
    updatedAt: overrides.updatedAt ?? "2026-07-05T00:00:00.000Z",
  };
}

function profileRegistryWrite(
  record: NativeProfileRegistryRecord,
): NativeProfileRegistryWrite {
  return {
    profileId: record.profileId,
    lifecycleStatus: record.lifecycleStatus,
    displayName: record.displayName,
    summary: record.summary,
    defaultSessionKind: record.defaultSessionKind,
    agentId: record.agentId,
    ownerId: record.ownerId,
    promptSoulMarkdown: record.promptSoulMarkdown,
    promptMemoryMarkdown: record.promptMemoryMarkdown,
    activeRuntimeSettingsJson: record.activeRuntimeSettingsJson,
    sourceAssetRefs: record.sourceAssetRefs,
    derivedRuntimeRefs: record.derivedRuntimeRefs,
    importExport: record.importExport,
    now: "2026-07-05T00:00:00.000Z",
  };
}

function modelProviderRouteContext(
  providers: NativeModelProviderRecord[],
  credentials: NativeServiceCredentialRecord[] = [],
) {
  const items = new Map(
    providers.map((provider) => [provider.alias, provider] as const),
  );
  const credentialItems = new Map(
    credentials.map(
      (credential) => [credential.credentialId, credential] as const,
    ),
  );
  const pendingLogins = new Map();
  const observedQueries: unknown[] = [];
  const context: ModelProviderAdminRouteContext &
    ServiceCredentialAdminRouteContext & {
      observedQueries: unknown[];
    } = {
    observedQueries,
    pendingLogins,
    openAiOauth: {
      issuer: "https://auth.openai.com",
      clientId: "rusty-crew-test",
      redirectUri: "http://localhost:1455/auth/callback",
      allowRedirectUriOverride: false,
      originator: "rusty-crew-test",
    },
    now: () => "2026-07-05T00:00:00.000Z",
    async listModelProviders(query) {
      observedQueries.push(query);
      return [...items.values()].filter(
        (provider) =>
          !("status" in query) ||
          query.status === undefined ||
          provider.status === query.status,
      );
    },
    async getModelProvider(alias) {
      return items.get(alias);
    },
    async upsertModelProvider(write) {
      const current = items.get(write.alias);
      if (
        current !== undefined &&
        write.expectedRevision !== undefined &&
        write.expectedRevision !== current.revision
      ) {
        throw new Error(
          `model provider ${write.alias} revision mismatch: expected ${write.expectedRevision}, found ${current.revision}`,
        );
      }
      const provider = modelProviderRecord({
        ...(current ?? {}),
        alias: write.alias,
        status: write.status,
        protocol: write.protocol,
        providerKind: write.providerKind,
        displayName: write.displayName,
        description: write.description,
        baseUrl: write.baseUrl,
        modelId: write.modelId,
        contextWindowTokens: write.contextWindowTokens,
        maxOutputTokens: write.maxOutputTokens,
        temperatureMilli: write.temperatureMilli,
        reasoningEffort: write.reasoningEffort,
        reasoningFormat: write.reasoningFormat,
        chatCompletionsDialect: write.chatCompletionsDialect,
        thinkingMode: write.thinkingMode,
        reasoningHistory: write.reasoningHistory,
        reasoningBudgetTokens: write.reasoningBudgetTokens,
        promptCaching: write.promptCaching,
        credentialId: current?.credentialId,
        metadataJson: write.metadataJson ?? current?.metadataJson ?? {},
        revision: (current?.revision ?? 0) + 1,
        credential:
          write.clearSecret === true
            ? { hasSecret: false }
            : write.secret !== undefined
              ? { hasSecret: true, kind: "api_key" }
              : (current?.credential ?? { hasSecret: false }),
        createdAt: current?.createdAt ?? write.now,
        updatedAt: write.now,
      });
      items.set(provider.alias, provider);
      return provider;
    },
    async listServiceCredentials(query) {
      return [...credentialItems.values()]
        .filter(
          (credential) =>
            query.providerKind === undefined ||
            credential.providerKind === query.providerKind,
        )
        .slice(query.offset ?? 0, (query.offset ?? 0) + (query.limit ?? 100));
    },
    async getServiceCredential(credentialId) {
      return credentialItems.get(credentialId);
    },
    async upsertServiceCredential(write) {
      const current = credentialItems.get(write.credentialId);
      if (
        write.expectedRevision !== undefined &&
        write.expectedRevision !== (current?.revision ?? 0)
      ) {
        throw new Error(
          `service credential ${write.credentialId} revision mismatch: expected ${write.expectedRevision}, found ${current?.revision ?? 0}`,
        );
      }
      if (
        write.clearSecret &&
        (current?.linkedProviderAliases.length ?? 0) > 0
      ) {
        throw new Error(
          `cannot clear service credential ${write.credentialId} while linked to model providers: ${current?.linkedProviderAliases.join(", ")}`,
        );
      }
      const hasSecret = write.clearSecret
        ? false
        : write.secret !== undefined
          ? true
          : (current?.credential.hasSecret ?? false);
      const credential = serviceCredentialRecord({
        ...(current ?? {}),
        credentialId: write.credentialId,
        displayName: write.displayName,
        providerKind: write.providerKind,
        credentialKind: write.credentialKind,
        credential: {
          hasSecret,
          kind: write.credentialKind,
          revision: (current?.revision ?? 0) + 1,
        },
        linkedProviderAliases: current?.linkedProviderAliases ?? [],
        revision: (current?.revision ?? 0) + 1,
        createdAt: current?.createdAt ?? write.now,
        updatedAt: write.now,
      });
      credentialItems.set(credential.credentialId, credential);
      for (const alias of credential.linkedProviderAliases) {
        const provider = items.get(alias);
        if (provider) {
          items.set(alias, {
            ...provider,
            credentialId: credential.credentialId,
            credential: credential.credential,
          });
        }
      }
      return credential;
    },
    async deleteServiceCredential(deleteRequest) {
      const credential = credentialItems.get(deleteRequest.credentialId);
      if (!credential) {
        throw new Error(
          `service credential ${deleteRequest.credentialId} not found`,
        );
      }
      if (
        deleteRequest.expectedRevision !== undefined &&
        deleteRequest.expectedRevision !== credential.revision
      ) {
        throw new Error(
          `service credential ${deleteRequest.credentialId} revision mismatch: expected ${deleteRequest.expectedRevision}, found ${credential.revision}`,
        );
      }
      if (credential.linkedProviderAliases.length > 0) {
        throw new Error(
          `cannot delete service credential ${credential.credentialId} while linked to model providers: ${credential.linkedProviderAliases.join(", ")}`,
        );
      }
      credentialItems.delete(credential.credentialId);
      return credential;
    },
    async linkModelProviderCredential(link) {
      const provider = items.get(link.providerAlias);
      const credential = credentialItems.get(link.credentialId);
      if (!provider || !credential)
        throw new Error("credential link target not found");
      if (
        link.expectedProviderRevision !== undefined &&
        link.expectedProviderRevision !== provider.revision
      ) {
        throw new Error("model provider revision mismatch");
      }
      if (
        link.expectedCredentialRevision !== undefined &&
        link.expectedCredentialRevision !== credential.revision
      ) {
        throw new Error("service credential revision mismatch");
      }
      const updatedProvider = {
        ...provider,
        credentialId: credential.credentialId,
        credential: credential.credential,
        revision: provider.revision + 1,
        updatedAt: link.now,
      };
      const updatedCredential = {
        ...credential,
        linkedProviderAliases: [
          ...new Set([...credential.linkedProviderAliases, provider.alias]),
        ],
      };
      items.set(provider.alias, updatedProvider);
      credentialItems.set(credential.credentialId, updatedCredential);
      return { provider: updatedProvider, credential: updatedCredential };
    },
    async unlinkModelProviderCredential(unlink) {
      const provider = items.get(unlink.providerAlias);
      if (!provider) throw new Error("model provider not found");
      if (
        unlink.expectedProviderRevision !== undefined &&
        unlink.expectedProviderRevision !== provider.revision
      ) {
        throw new Error("model provider revision mismatch");
      }
      if (provider.credentialId) {
        const credential = credentialItems.get(provider.credentialId);
        if (credential) {
          credentialItems.set(credential.credentialId, {
            ...credential,
            linkedProviderAliases: credential.linkedProviderAliases.filter(
              (alias) => alias !== provider.alias,
            ),
          });
        }
      }
      const updated = {
        ...provider,
        credentialId: undefined,
        credential: { hasSecret: false },
        revision: provider.revision + 1,
        updatedAt: unlink.now,
      };
      items.set(provider.alias, updated);
      return updated;
    },
    async exchangeOpenAiOauthCode() {
      return {
        ok: false,
        error: {
          code: "unsupported",
          reasonCode: "test_exchange_not_configured",
          message: "test exchange not configured",
          retryable: false,
        },
      };
    },
    async refreshAfterWrite({ refreshMode }) {
      return {
        refresh: {
          mode: refreshMode,
          affectedProfiles: [],
          outcomes: [],
        },
      };
    },
  };
  return context;
}

function modelProviderRecord(
  overrides: Partial<NativeModelProviderRecord> & { alias: string },
): NativeModelProviderRecord {
  return {
    alias: overrides.alias,
    status: overrides.status ?? "active",
    protocol: overrides.protocol ?? "chat_completions",
    providerKind: overrides.providerKind ?? "custom",
    displayName: overrides.displayName,
    description: overrides.description,
    baseUrl: overrides.baseUrl ?? "http://model-provider.test/v1",
    modelId: overrides.modelId ?? "test-model",
    contextWindowTokens: overrides.contextWindowTokens,
    maxOutputTokens: overrides.maxOutputTokens,
    temperatureMilli: overrides.temperatureMilli,
    reasoningEffort: overrides.reasoningEffort,
    reasoningFormat: overrides.reasoningFormat,
    responsesDialect:
      overrides.responsesDialect ??
      (overrides.protocol === "responses" ? "openai_stateful" : undefined),
    chatCompletionsDialect: overrides.chatCompletionsDialect ?? "standard",
    thinkingMode: overrides.thinkingMode ?? "provider_default",
    reasoningHistory: overrides.reasoningHistory ?? "provider_default",
    reasoningBudgetTokens: overrides.reasoningBudgetTokens,
    promptCaching: overrides.promptCaching ?? "disabled",
    credentialId: overrides.credentialId,
    credential: overrides.credential ?? { hasSecret: false },
    metadataJson: overrides.metadataJson ?? {},
    revision: overrides.revision ?? 1,
    createdAt: overrides.createdAt ?? "2026-07-05T00:00:00.000Z",
    updatedAt: overrides.updatedAt ?? "2026-07-05T00:00:00.000Z",
  };
}

function serviceCredentialRecord(
  overrides: Partial<NativeServiceCredentialRecord> & { credentialId: string },
): NativeServiceCredentialRecord {
  return {
    credentialId: overrides.credentialId,
    displayName: overrides.displayName ?? overrides.credentialId,
    providerKind: overrides.providerKind ?? "openai",
    credentialKind: overrides.credentialKind ?? "openai_oauth",
    credential: overrides.credential ?? {
      hasSecret: true,
      kind: overrides.credentialKind ?? "openai_oauth",
      revision: overrides.revision ?? 1,
    },
    linkedProviderAliases: overrides.linkedProviderAliases ?? [],
    revision: overrides.revision ?? 1,
    createdAt: overrides.createdAt ?? "2026-07-05T00:00:00.000Z",
    updatedAt: overrides.updatedAt ?? "2026-07-05T00:00:00.000Z",
  };
}

function inMemoryLocalToolProfileStore(): LocalToolProfileStore {
  const items = new Map<string, LocalToolProfile>();
  const now = "2026-07-05T00:00:00.000Z";
  const toProfile = (
    write: LocalToolProfileWrite,
    current?: LocalToolProfile,
  ): LocalToolProfile => ({
    schemaVersion: 1,
    id: write.id ?? current?.id ?? "missing",
    displayName: write.displayName ?? current?.displayName ?? "Missing",
    description: write.description ?? current?.description,
    enabled: write.enabled ?? current?.enabled ?? true,
    system: current?.system ?? false,
    readOnly: current?.readOnly ?? false,
    toolsets: write.toolsets ?? current?.toolsets ?? [],
    tools: write.tools ?? current?.tools ?? [],
    createdAt: current?.createdAt ?? now,
    updatedAt: now,
    revision: (current?.revision ?? 0) + 1,
  });
  return {
    async list(): Promise<LocalToolProfileList> {
      return {
        schemaVersion: 1,
        catalogId: "local-tool-profiles",
        builtInCatalogId: "default-local-tools",
        items: [...items.values()],
        total: items.size,
      };
    },
    async get(id) {
      return items.get(id);
    },
    async create(write) {
      const profile = toProfile(write);
      items.set(profile.id, profile);
      return profile;
    },
    async update(id, write) {
      const current = items.get(id);
      if (current === undefined) throw new Error("test profile missing");
      const profile = toProfile({ ...write, id }, current);
      items.set(id, profile);
      return profile;
    },
    async delete(id) {
      const current = items.get(id);
      if (current === undefined) throw new Error("test profile missing");
      items.delete(id);
      return current;
    },
    async resolve(id) {
      const profile = items.get(id);
      if (profile === undefined) throw new Error("test profile missing");
      return {
        id,
        toolPolicy: {
          requestedToolsets: profile.toolsets,
          requestedTools: profile.tools,
        },
      };
    },
  };
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  return result.body.data as T;
}

function errorReason(result: AdminRouteResult): string {
  assert.equal(result.body.ok, false);
  return result.body.error.reason_code;
}

function errorDetails(result: AdminRouteResult): Record<string, unknown> {
  assert.equal(result.body.ok, false);
  return result.body.error;
}
