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
import { ExternalThreadLifecycleError } from "../src/service-external-runtime.js";
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
      async beginAgentRound(command: Record<string, unknown>) {
        capturedRound = command;
        return {
          round: {
            roundId: command.roundId,
            recipientAgentId: command.toAgentId,
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
      toAgentId: "agent-a",
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
        return { accepted: true };
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

  okData<{ accepted: boolean }>(result as AdminRouteResult);
  assert.deepEqual(captured?.taskRef, {
    projectId: "asha",
    taskId: "4281",
  });
  assert.equal(captured?.requestedAt, "2026-07-11T20:00:00.000Z");
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
});

test("model provider OpenAI OAuth routes expose status and start without leaking verifier", async () => {
  const context = modelProviderRouteContext([
    modelProviderRecord({
      alias: "gpt",
      protocol: "responses",
      providerKind: "openai",
      modelId: "gpt-5",
      credential: { hasSecret: true, kind: "openai_oauth" },
    }),
  ]);

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
  assert.match(startData.pendingLogin.pendingLoginId, /^openai-oauth:gpt:/);
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

function modelProviderRouteContext(providers: NativeModelProviderRecord[]) {
  const items = new Map(
    providers.map((provider) => [provider.alias, provider] as const),
  );
  const pendingLogins = new Map();
  const observedQueries: unknown[] = [];
  const context: ModelProviderAdminRouteContext & {
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
          query.status === undefined || provider.status === query.status,
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
    credential: overrides.credential ?? { hasSecret: false },
    metadataJson: overrides.metadataJson ?? {},
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
