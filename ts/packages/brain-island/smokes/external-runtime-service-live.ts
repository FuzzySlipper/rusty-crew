import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { CODEX_APP_SERVER_PROTOCOL } from "@rusty-crew/external-runtime-codex";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

import { ServiceExternalRuntimeController } from "../src/service-external-runtime.js";
import { handleExternalRuntimeRequest } from "../src/service-external-runtime-routes.js";
import {
  chatCorsHeaders,
  readJsonBody,
  requestId,
  writeJsonResponse,
} from "../src/service-http-route-helpers.js";

const socketPath =
  process.env.CODEX_APP_SERVER_SOCKET ??
  "/run/user/1001/codex-app-server/app-server.sock";
const timeoutMs = Number(
  process.env.CODEX_APP_SERVER_SERVICE_LIVE_TIMEOUT_MS ?? 300_000,
);
const dataDir = mkdtempSync(join(tmpdir(), "rusty-crew-external-service-"));
const runtimeId = "codex-service-live";
const sessionId = "codex-service-live-session";
const agentId = "codex-service-live-agent";
const bindingId = "codex-service-live-binding";
const peerSessionId = "codex-service-live-peer-session";
const peerAgentId = "codex-service-live-peer-agent";
const peerBindingId = "codex-service-live-peer-binding";
const staleSessionId = "codex-service-live-stale-session";
const staleAgentId = "codex-service-live-stale-agent";
const staleBindingId = "codex-service-live-stale-binding";
const staleNativeThreadId = "codex-service-live-missing-native-thread";
const browserProfileId = "codex-service-live-browser-profile";
const now = (): string => new Date().toISOString();

const bridge = await loadNativeBridge();
const engineConfig = {
  engineDataDir: dataDir,
  clock: "system",
  defaultTurnBudget: 16,
  defaultIdleTimeoutMs: 30_000,
  storage: { backend: "sqlite" as const },
};
let engine = await bridge.initializeEngine(engineConfig);
let controller = new ServiceExternalRuntimeController({ bridge });
const timers = new Set<NodeJS.Timeout>();

let server: ReturnType<typeof createServer> | undefined;
try {
  await bridge.registerExternalRuntime({
    registration: {
      runtimeId,
      kind: "codex_app_server",
      endpoint: { transport: "unix_web_socket", address: socketPath },
      processOwnership: "attached",
      codexHomeRef: "/home/agent/.codex",
      expectedCliVersion: CODEX_APP_SERVER_PROTOCOL.cliVersion,
      executableSha256: CODEX_APP_SERVER_PROTOCOL.nativeExecutableSha256,
      protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
      desiredState: "enabled",
      observedState: "disconnected",
      observedReasonCode: "live_smoke_start",
      revision: 0,
      createdAt: now(),
      updatedAt: now(),
    },
  });
  await bridge.createProfileRegistryRecord({
    profileId: browserProfileId,
    lifecycleStatus: "active",
    displayName: "Codex service live browser profile",
    defaultSessionKind: "full",
    agentId: browserProfileId,
    activeRuntimeSettingsJson: {},
    sourceAssetRefs: [],
    derivedRuntimeRefs: [],
    importExport: { metadataJson: {} },
    now: now(),
  });
  await bridge.ensureConfiguredSession({
    sessionId,
    agentId,
    profileId: "codex-service-live-profile",
    kind: "full",
    resourceLimits: { workdir: dataDir },
    toolProfile: { tools: [] },
  });
  await bridge.ensureConfiguredSession({
    sessionId: peerSessionId,
    agentId: peerAgentId,
    profileId: "codex-service-live-peer-profile",
    kind: "full",
    resourceLimits: { workdir: dataDir },
    toolProfile: { tools: [] },
  });
  const binding = await bridge.bindExternalAgent({
    binding: {
      bindingId,
      runtimeId,
      sessionId,
      agentId,
      purpose: "crew_agent",
      cwd: dataDir,
      effectiveConfigFingerprint: "external-service-live-v1",
      status: "active",
      revision: 0,
      createdAt: now(),
      updatedAt: now(),
    },
  });
  const peerBinding = await bridge.bindExternalAgent({
    binding: {
      bindingId: peerBindingId,
      runtimeId,
      sessionId: peerSessionId,
      agentId: peerAgentId,
      purpose: "crew_agent",
      cwd: dataDir,
      effectiveConfigFingerprint: "external-service-live-peer-v1",
      status: "active",
      revision: 0,
      createdAt: now(),
      updatedAt: now(),
    },
  });
  await controller.start();
  const threadControl = await controller.executeControl({
    controlId: "external-service-live-thread",
    idempotencyKey: "external-service-live-thread",
    bindingId,
    expectedBindingRevision: binding.revision,
    kind: "start_or_resume_thread",
    payload: {
      baseInstructions:
        "You are a live Rusty Crew external-runtime acceptance agent.",
    },
    requestedAt: now(),
  });
  assert.equal(threadControl.status, "applied");
  const peerThreadControl = await controller.executeControl({
    controlId: "external-service-live-peer-thread",
    idempotencyKey: "external-service-live-peer-thread",
    bindingId: peerBindingId,
    expectedBindingRevision: peerBinding.revision,
    kind: "start_or_resume_thread",
    payload: {
      baseInstructions:
        "You are the peer in a Rusty Crew correlated-round acceptance test. Follow explicit coordination-tool instructions exactly.",
    },
    requestedAt: now(),
  });
  assert.equal(peerThreadControl.status, "applied");

  const delivery = await bridge.deliverAgentMessage({
    caller: { type: "system", senderAgentId: "external-service-live-operator" },
    deliveryId: "external-service-live-delivery",
    idempotencyKey: "external-service-live-delivery",
    messageId: "external-service-live-message",
    toAgentId: agentId,
    body: "Reply with exactly EXTERNAL_SERVICE_LIVE_OK and nothing else.",
    requireWake: true,
    createdAt: now(),
    expiresAt: new Date(Date.now() + 60_000).toISOString(),
  });
  assert.equal(delivery.status, "accepted");
  assert.equal(delivery.activation?.type, "external_turn_requested");
  await controller.tick();

  const terminal = await waitForTerminalEvent();
  assert.equal(terminal.payload.nativeMethod, "turn/completed");
  assert.equal(typeof terminal.nativeThreadId, "string");
  const primaryThreadId = terminal.nativeThreadId;

  server = createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    void handleExternalRuntimeRequest(request, url, {
      bridge,
      controller,
      startInterval: (callback, intervalMs) => {
        const timer = setInterval(callback, intervalMs);
        timers.add(timer);
        return timer;
      },
      stopInterval: (timer) => {
        clearInterval(timer);
        timers.delete(timer);
      },
      now,
      requestId,
      readJsonBody,
      corsHeaders: chatCorsHeaders,
    }).then((result) => writeJsonResponse(response, result));
  });
  await new Promise<void>((resolve) => server?.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address !== null && typeof address !== "string");
  const baseUrl = `http://127.0.0.1:${address.port}`;

  const browserCreationRequest = {
    idempotencyKey: "external-service-live-browser-create",
    runtimeId,
    profileId: browserProfileId,
    cwd: dataDir,
    taskRef: { project_id: "rusty-crew", task_id: "5678" },
    label: "Live browser-created Codex agent",
  };
  const browserCreationResponse = await fetch(
    `${baseUrl}/v1/external-agent-sessions`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(browserCreationRequest),
    },
  );
  assert.equal(browserCreationResponse.status, 200);
  const browserCreation = (await browserCreationResponse.json()) as {
    ok: boolean;
    data: {
      creation: {
        creationId: string;
        nativeThreadId: string;
        phase: string;
        session: { sessionId: string; profileId: string };
        binding: {
          bindingId: string;
          nativeThreadId: string;
          taskRef: { project_id: string; task_id: string };
        };
      };
      thread: { threadId: string };
    };
  };
  assert.equal(browserCreation.ok, true);
  assert.equal(browserCreation.data.creation.phase, "ready");
  assert.equal(
    browserCreation.data.creation.session.profileId,
    browserProfileId,
  );
  assert.equal(
    browserCreation.data.thread.threadId,
    browserCreation.data.creation.nativeThreadId,
  );
  assert.deepEqual(browserCreation.data.creation.binding.taskRef, {
    project_id: "rusty-crew",
    task_id: "5678",
  });

  const browserCreationRetryResponse = await fetch(
    `${baseUrl}/v1/external-agent-sessions`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(browserCreationRequest),
    },
  );
  assert.equal(browserCreationRetryResponse.status, 200);
  const browserCreationRetry = (await browserCreationRetryResponse.json()) as {
    data: { creation: { creationId: string; nativeThreadId: string } };
  };
  assert.equal(
    browserCreationRetry.data.creation.creationId,
    browserCreation.data.creation.creationId,
  );
  assert.equal(
    browserCreationRetry.data.creation.nativeThreadId,
    browserCreation.data.creation.nativeThreadId,
  );

  const browserCreationConflict = await fetch(
    `${baseUrl}/v1/external-agent-sessions`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...browserCreationRequest,
        label: "Changed retry intent",
      }),
    },
  );
  assert.equal(browserCreationConflict.status, 409);
  const browserConflictBody = (await browserCreationConflict.json()) as {
    error: { reason_code: string };
  };
  assert.equal(
    browserConflictBody.error.reason_code,
    "external_agent_creation_idempotency_conflict",
  );

  const browserDelivery = await deliverLiveMessage(
    baseUrl,
    "external-service-live-browser-created",
    "Reply with exactly EXTERNAL_BROWSER_CREATE_OK and nothing else.",
    true,
    undefined,
    browserCreation.data.creation.binding.bindingId,
  );
  assert.equal(browserDelivery.activation?.type, "external_turn_requested");
  const browserTurn = await waitForActiveTurn(
    browserCreation.data.creation.session.sessionId,
  );
  const browserTerminal = await waitForTerminalEvent(browserTurn.nativeTurnId);
  assert.equal(
    browserTerminal.nativeThreadId,
    browserCreation.data.creation.nativeThreadId,
  );
  const browserTurnText = (
    await bridge.queryExternalRuntimeEvents({
      runtimeId,
      afterSequence: 0,
      limit: 1_000,
    })
  )
    .filter(
      (event) =>
        event.nativeTurnId === browserTurn.nativeTurnId &&
        event.kind === "assistant_text_delta",
    )
    .map((event) =>
      typeof event.payload === "object" &&
      event.payload !== null &&
      "text" in event.payload
        ? String(event.payload.text)
        : "",
    )
    .join("");
  assert.match(browserTurnText, /EXTERNAL_BROWSER_CREATE_OK/);

  const runtimeResponse = await fetch(
    `${baseUrl}/v1/external-runtimes/${runtimeId}`,
  );
  assert.equal(runtimeResponse.status, 200);
  const runtimeBody = (await runtimeResponse.json()) as {
    ok: boolean;
    data: { registration: { observedState: string } };
  };
  assert.equal(runtimeBody.ok, true);
  assert.equal(runtimeBody.data.registration.observedState, "ready");

  const threadListResponse = await fetch(
    `${baseUrl}/v1/external-runtimes/${runtimeId}/threads?limit=20`,
  );
  assert.equal(threadListResponse.status, 200);
  const threadListBody = (await threadListResponse.json()) as {
    ok: boolean;
    data: {
      items: Array<{ threadId: string; turns: unknown[] }>;
      nextCursor: string | null;
      backwardsCursor: string | null;
    };
  };
  assert.equal(threadListBody.ok, true);
  assert.equal("data" in threadListBody.data, false);
  assert.ok(
    threadListBody.data.items.some(
      (thread) => thread.threadId === primaryThreadId,
    ),
  );

  const threadReadResponse = await fetch(
    `${baseUrl}/v1/external-runtimes/${runtimeId}/threads/read`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        threadId: primaryThreadId,
        includeTurns: true,
      }),
    },
  );
  assert.equal(threadReadResponse.status, 200);
  const threadReadBody = (await threadReadResponse.json()) as {
    ok: boolean;
    data: {
      thread: {
        threadId: string;
        status: string;
        turns: Array<{
          turnId: string;
          items: Array<{ itemId: string; kind: string }>;
        }>;
      };
    };
  };
  assert.equal(threadReadBody.ok, true);
  assert.equal(threadReadBody.data.thread.threadId, primaryThreadId);
  assert.ok(threadReadBody.data.thread.turns.length > 0);
  assert.ok(
    threadReadBody.data.thread.turns.every((turn) =>
      turn.items.every(
        (item) => item.itemId.length > 0 && item.kind.length > 0,
      ),
    ),
  );

  const sseResponse = await fetch(
    `${baseUrl}/v1/external-runtimes/${runtimeId}/stream?cursor=0&once=true`,
  );
  assert.equal(sseResponse.status, 200);
  const sse = await sseResponse.text();
  assert.match(sse, /event: assistant_text_delta/);
  assert.match(sse, /EXTERNAL_SERVICE_LIVE_OK/);
  assert.match(sse, /event: turn_lifecycle/);

  const queueFirst = await deliverLiveMessage(
    baseUrl,
    "external-service-live-queue-first",
    "Reply with exactly QUEUE_FIRST_OK.",
    false,
  );
  assert.equal(queueFirst.activation?.type, "external_turn_requested");
  const queueSecond = await deliverLiveMessage(
    baseUrl,
    "external-service-live-queue-second",
    "Reply with exactly QUEUE_SECOND_OK.",
    false,
  );
  assert.equal(queueSecond.activation?.type, "queued_for_next_turn");
  const queueSecondReplay = await deliverLiveMessage(
    baseUrl,
    "external-service-live-queue-second",
    "Reply with exactly QUEUE_SECOND_OK.",
    false,
  );
  assert.deepEqual(queueSecondReplay, queueSecond);
  await controller.tick();
  const queueFirstTurn = await waitForActiveTurn();
  await waitForTerminalEvent(queueFirstTurn.nativeTurnId);
  const queueSecondTurn = await waitForActiveTurn();
  await waitForTerminalEvent(queueSecondTurn.nativeTurnId);
  assert.notEqual(queueSecondTurn.nativeTurnId, queueFirstTurn.nativeTurnId);

  const planCursor = await latestRuntimeSequence();
  const planDelivery = await deliverLiveMessage(
    baseUrl,
    "external-service-live-plan-input",
    [
      "Use request_user_input exactly once.",
      "Ask which certification color to use and offer blue and green.",
      "After the answer, reply with exactly PLAN_MODE_INPUT_OK:<answer>.",
    ].join(" "),
    true,
    "plan",
  );
  assert.equal(planDelivery.activation?.type, "external_turn_requested");
  const planInteraction = await waitForPendingInteraction(baseUrl);
  assert.equal(planInteraction.kind, "request_user_input");
  const questionId = interactionQuestionId(planInteraction.prompt);
  const waitingTurn = (await bridge.listActiveExternalTurns()).find(
    (turn) => turn.request.requestId === planInteraction.requestId,
  );
  assert.equal(waitingTurn?.phase, "waiting_interaction");
  const beforeResolution = await bridge.queryExternalRuntimeEvents({
    runtimeId,
    afterSequence: planCursor,
    limit: 1_000,
  });
  assert.equal(
    beforeResolution.some(
      (event) =>
        event.nativeTurnId === planInteraction.nativeTurnId &&
        isNativeTerminalEvent(event.payload),
    ),
    false,
  );
  const resolveResponse = await fetch(
    `${baseUrl}/v1/external-interactions/${encodeURIComponent(planInteraction.interactionId)}/resolve`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        expectedRevision: planInteraction.revision,
        idempotencyKey: "external-service-live-plan-input-resolution",
        result: { answers: { [questionId]: { answers: ["blue"] } } },
      }),
    },
  );
  assert.equal(resolveResponse.status, 200);
  const resolvedInteraction = (await resolveResponse.json()) as {
    ok: boolean;
    data: { status: string };
  };
  assert.equal(resolvedInteraction.ok, true);
  assert.equal(resolvedInteraction.data.status, "resolved");
  await waitForTerminalEvent(planInteraction.nativeTurnId);
  const planEvents = await bridge.queryExternalRuntimeEvents({
    runtimeId,
    afterSequence: planCursor,
    limit: 1_000,
  });
  const planAnswer = planEvents
    .filter(
      (event) =>
        event.nativeTurnId === planInteraction.nativeTurnId &&
        event.kind === "assistant_text_delta",
    )
    .map((event) =>
      typeof event.payload === "object" &&
      event.payload !== null &&
      "text" in event.payload
        ? String(event.payload.text)
        : "",
    )
    .join("");
  assert.match(planAnswer, /PLAN_MODE_INPUT_OK:blue/i);

  const coordinationCursor = await latestRuntimeSequence();
  const coordinationId = `codex-codex-${Date.now()}`;
  const coordinationDelivery = await deliverLiveMessage(
    baseUrl,
    "external-service-live-codex-round",
    [
      "Call rusty_crew.agent_round exactly once.",
      `Use recipient ${peerAgentId}.`,
      `Use correlationId ${coordinationId}.`,
      "Use timeoutMs 120000.",
      `Use this body exactly: Call rusty_crew.send_agent_message exactly once with recipient ${agentId}, body CODEX_CODEX_REPLY_OK, and correlationId ${coordinationId}. Then reply PEER_DONE.`,
      "After the round tool returns, reply with exactly CODEX_CODEX_ROUND_OK.",
    ].join("\n"),
  );
  assert.equal(
    coordinationDelivery.activation?.type,
    "external_turn_requested",
  );
  const coordinationTurn = await waitForActiveTurn();
  await waitForTerminalEvent(coordinationTurn.nativeTurnId);
  const coordinationEvents =
    await waitForCoordinationEvidence(coordinationCursor);
  const peerThreadId = (await bridge.getExternalBinding(peerBindingId))
    ?.nativeThreadId;
  assert.equal(typeof peerThreadId, "string");
  const peerTerminal = coordinationEvents.find(
    (event) =>
      event.nativeThreadId === peerThreadId &&
      isNativeTerminalEvent(event.payload),
  );
  assert(peerTerminal !== undefined);
  const dynamicToolIds = [
    ...new Set(
      coordinationEvents
        .filter((event) => event.kind === "dynamic_tool_activity")
        .map((event) => event.itemId ?? event.requestId)
        .filter((value): value is string => typeof value === "string"),
    ),
  ];
  assert(dynamicToolIds.length >= 2);
  const roundId = [
    "codex-round",
    bindingId,
    coordinationTurn.nativeThreadId,
    coordinationTurn.nativeTurnId,
    dynamicToolIds[0],
  ].join(":");
  const correlatedRound = await bridge.getAgentRound(roundId);
  assert.equal(correlatedRound?.status, "replied");
  assert.equal(correlatedRound?.correlationId, coordinationId);

  const steerDelivery = await deliverLiveMessage(
    baseUrl,
    "external-service-live-steer",
    "Run the shell command sleep 10, then reply STEER_ORIGINAL.",
  );
  assert.equal(steerDelivery.activation?.type, "external_turn_requested");
  const steerTurn = await waitForActiveTurn();
  const steerResponse = await postControl(baseUrl, {
    kind: "steer_turn",
    expectedNativeTurnId: steerTurn.nativeTurnId,
    payload: {
      threadId: steerTurn.nativeThreadId,
      expectedTurnId: steerTurn.nativeTurnId,
      input: [
        {
          type: "text",
          text: "Stop waiting and reply STEER_ACCEPTED now.",
          text_elements: [],
        },
      ],
    },
  });
  assert.equal(steerResponse.status, "applied");
  const steerTerminal = await waitForTerminalEvent(steerTurn.nativeTurnId);

  const interruptDelivery = await deliverLiveMessage(
    baseUrl,
    "external-service-live-interrupt",
    "Run the shell command sleep 30, then reply INTERRUPT_MISSED.",
  );
  assert.equal(interruptDelivery.activation?.type, "external_turn_requested");
  const interruptTurn = await waitForActiveTurn();
  const interruptResponse = await postControl(baseUrl, {
    kind: "interrupt_turn",
    expectedNativeTurnId: interruptTurn.nativeTurnId,
    payload: {
      threadId: interruptTurn.nativeThreadId,
      turnId: interruptTurn.nativeTurnId,
    },
  });
  assert.equal(interruptResponse.status, "applied");
  const interrupted = await waitForTerminalEvent(interruptTurn.nativeTurnId);
  assert.equal(interrupted.payload.nativeMethod, "turn/completed");
  assert.equal(interrupted.payload.status, "interrupted");

  const bindingBeforeRestart = await bridge.getExternalBinding(bindingId);
  assert.equal(typeof bindingBeforeRestart?.nativeThreadId, "string");
  await bridge.ensureConfiguredSession({
    sessionId: staleSessionId,
    agentId: staleAgentId,
    profileId: "codex-service-live-stale-profile",
    kind: "full",
    resourceLimits: { workdir: dataDir },
    toolProfile: { tools: [] },
  });
  await bridge.bindExternalAgent({
    binding: {
      bindingId: staleBindingId,
      runtimeId,
      sessionId: staleSessionId,
      agentId: staleAgentId,
      nativeThreadId: staleNativeThreadId,
      purpose: "crew_agent",
      cwd: dataDir,
      effectiveConfigFingerprint: "external-service-live-stale-v1",
      status: "active",
      revision: 0,
      createdAt: now(),
      updatedAt: now(),
    },
  });
  const controllerGenerationBeforeRestart =
    controller.statuses()[0]?.controllerGeneration;
  await controller.stop();
  await bridge.shutdownEngine({ engine, drainTimeoutMs: 5_000 });

  const appServerRestarted =
    process.env.CODEX_APP_SERVER_RESTART_SERVICE === "1";
  if (appServerRestarted) {
    execFileSync("systemctl", [
      "--user",
      "restart",
      "codex-app-server.service",
    ]);
  }

  engine = await bridge.initializeEngine(engineConfig);
  await bridge.ensureConfiguredSession({
    sessionId,
    agentId,
    profileId: "codex-service-live-profile",
    kind: "full",
    resourceLimits: { workdir: dataDir },
    toolProfile: { tools: [] },
  });
  await bridge.ensureConfiguredSession({
    sessionId: peerSessionId,
    agentId: peerAgentId,
    profileId: "codex-service-live-peer-profile",
    kind: "full",
    resourceLimits: { workdir: dataDir },
    toolProfile: { tools: [] },
  });
  controller = new ServiceExternalRuntimeController({ bridge });
  await waitForControllerReady();
  const recoveredStatus = controller.statuses()[0];
  assert.equal(recoveredStatus?.bindingResumeFailures.length, 1);
  assert.equal(
    recoveredStatus?.bindingResumeFailures[0]?.bindingId,
    staleBindingId,
  );
  assert.equal(
    recoveredStatus?.bindingResumeFailures[0]?.nativeThreadId,
    staleNativeThreadId,
  );
  assert.equal(
    (await bridge.getExternalRuntime(runtimeId))?.observedState,
    "ready",
  );
  const bindingAfterRestart = await bridge.getExternalBinding(bindingId);
  assert.equal(
    bindingAfterRestart?.nativeThreadId,
    bindingBeforeRestart?.nativeThreadId,
  );
  const controllerGenerationAfterRestart =
    controller.statuses()[0]?.controllerGeneration;
  assert.notEqual(
    controllerGenerationAfterRestart,
    controllerGenerationBeforeRestart,
  );

  const postRestartCreationResponse = await fetch(
    `${baseUrl}/v1/external-agent-sessions`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ...browserCreationRequest,
        idempotencyKey: "external-service-live-post-restart-create",
        label: "Post-restart browser-created Codex agent",
      }),
    },
  );
  assert.equal(postRestartCreationResponse.status, 200);
  const postRestartCreation = (await postRestartCreationResponse.json()) as {
    data: { creation: { phase: string; binding: { bindingId: string } } };
  };
  assert.equal(postRestartCreation.data.creation.phase, "ready");

  const restartDelivery = await deliverLiveMessage(
    baseUrl,
    "external-service-live-restart",
    "Reply with exactly EXTERNAL_RESTART_RESUME_OK.",
  );
  assert.equal(
    restartDelivery.activation?.type,
    "external_turn_requested",
    JSON.stringify(restartDelivery),
  );
  const restartTurn = await waitForActiveTurn();
  const restartTerminal = await waitForTerminalEvent(restartTurn.nativeTurnId);
  assert.equal(
    restartTerminal.nativeThreadId,
    bindingBeforeRestart?.nativeThreadId,
  );

  console.log(
    JSON.stringify(
      {
        runtimeId,
        sessionId,
        bindingId,
        nativeThreadId: terminal.nativeThreadId,
        nativeTurnId: terminal.nativeTurnId,
        terminalSequenceId: terminal.sequenceId,
        steerTurnId: steerTerminal.nativeTurnId,
        interruptedTurnId: interrupted.nativeTurnId,
        restartTurnId: restartTerminal.nativeTurnId,
        exactThreadRestartResume: true,
        staleBindingRecovery: {
          bindingId: staleBindingId,
          nativeThreadId: staleNativeThreadId,
          isolatedFailureCount:
            recoveredStatus?.bindingResumeFailures.length ?? 0,
          postRestartBindingId:
            postRestartCreation.data.creation.binding.bindingId,
        },
        browserAgentSessionCreation: {
          creationId: browserCreation.data.creation.creationId,
          sessionId: browserCreation.data.creation.session.sessionId,
          bindingId: browserCreation.data.creation.binding.bindingId,
          nativeThreadId: browserCreation.data.creation.nativeThreadId,
          nativeTurnId: browserTerminal.nativeTurnId,
          duplicateRetryWasIdempotent: true,
          changedRetryRejected: true,
        },
        appServerRestarted,
        controllerGenerations: [
          controllerGenerationBeforeRestart,
          controllerGenerationAfterRestart,
        ],
        queuedTurnIds: [
          queueFirstTurn.nativeTurnId,
          queueSecondTurn.nativeTurnId,
        ],
        duplicateQueuedDeliveryWasIdempotent: true,
        codexToCodexRound: {
          correlationId: coordinationId,
          senderTurnId: coordinationTurn.nativeTurnId,
          peerThreadId,
          peerTurnId: peerTerminal.nativeTurnId,
          dynamicToolIds,
          roundId,
        },
        planInteraction: {
          interactionId: planInteraction.interactionId,
          nativeTurnId: planInteraction.nativeTurnId,
          questionId,
          selectedAnswer: "blue",
          blockedBeforeResolution: true,
          finalAnswer: planAnswer,
        },
        sseReplay: true,
        browserControls: ["steer_turn", "interrupt_turn"],
      },
      null,
      2,
    ),
  );
} finally {
  for (const timer of timers) clearInterval(timer);
  await new Promise<void>(
    (resolve) => server?.close(() => resolve()) ?? resolve(),
  );
  await controller.stop().catch(() => undefined);
  await bridge
    .shutdownEngine({ engine, drainTimeoutMs: 5_000 })
    .catch(() => undefined);
  rmSync(dataDir, { recursive: true, force: true });
}

async function waitForTerminalEvent(nativeTurnId?: string) {
  const deadline = Date.now() + timeoutMs;
  let cursor = 0;
  while (Date.now() < deadline) {
    await controller.tick();
    const events = await bridge.queryExternalRuntimeEvents({
      runtimeId,
      afterSequence: cursor,
      limit: 200,
    });
    for (const event of events) {
      cursor = event.sequenceId;
      if (
        typeof event.payload === "object" &&
        event.payload !== null &&
        "nativeMethod" in event.payload &&
        (event.payload.nativeMethod === "turn/completed" ||
          event.payload.nativeMethod === "turn/interrupted") &&
        (nativeTurnId === undefined || event.nativeTurnId === nativeTurnId)
      ) {
        return event;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `external runtime live turn did not complete after ${timeoutMs}ms`,
  );
}

async function waitForActiveTurn(targetSessionId = sessionId) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await controller.tick();
    const active = (await bridge.listActiveExternalTurns()).find(
      (turn) =>
        turn.request.sessionId === targetSessionId &&
        turn.phase === "active" &&
        typeof turn.nativeTurnId === "string",
    );
    if (active?.nativeTurnId !== undefined) {
      const events = await bridge.queryExternalRuntimeEvents({
        runtimeId,
        afterSequence: 0,
        limit: 500,
      });
      const nativeTurnStarted = events.some(
        (event) =>
          event.nativeTurnId === active.nativeTurnId &&
          typeof event.payload === "object" &&
          event.payload !== null &&
          "nativeMethod" in event.payload &&
          event.payload.nativeMethod === "turn/started",
      );
      if (nativeTurnStarted) {
        return {
          nativeThreadId: active.nativeThreadId,
          nativeTurnId: active.nativeTurnId,
        };
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `external runtime turn did not become active after ${timeoutMs}ms`,
  );
}

async function waitForControllerReady(): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await controller.start().catch(() => undefined);
    if (controller.statuses()[0]?.driverState === "ready") return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `external runtime controller did not recover after ${timeoutMs}ms`,
  );
}

async function latestRuntimeSequence(): Promise<number> {
  const events = await bridge.queryExternalRuntimeEvents({
    runtimeId,
    afterSequence: 0,
    limit: 1_000,
  });
  return events.at(-1)?.sequenceId ?? 0;
}

async function waitForCoordinationEvidence(afterSequence: number) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await controller.tick();
    const events = await bridge.queryExternalRuntimeEvents({
      runtimeId,
      afterSequence,
      limit: 1_000,
    });
    const peerThreadId = (await bridge.getExternalBinding(peerBindingId))
      ?.nativeThreadId;
    if (
      typeof peerThreadId === "string" &&
      events.some(
        (event) =>
          event.nativeThreadId === peerThreadId &&
          isNativeTerminalEvent(event.payload),
      ) &&
      events.filter((event) => event.kind === "dynamic_tool_activity").length >=
        2
    ) {
      return events;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `Codex-to-Codex coordination evidence did not complete after ${timeoutMs}ms`,
  );
}

function isNativeTerminalEvent(payload: unknown): boolean {
  return (
    typeof payload === "object" &&
    payload !== null &&
    "nativeMethod" in payload &&
    (payload.nativeMethod === "turn/completed" ||
      payload.nativeMethod === "turn/interrupted")
  );
}

async function deliverLiveMessage(
  baseUrl: string,
  label: string,
  body: string,
  tick = true,
  collaborationMode?: "plan",
  targetBindingId = bindingId,
) {
  const response = await fetch(
    `${baseUrl}/v1/external-bindings/${targetBindingId}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        deliveryId: `${label}-delivery`,
        idempotencyKey: `${label}-delivery`,
        messageId: `${label}-message`,
        body,
        ...(collaborationMode === undefined ? {} : { collaborationMode }),
        ttlMs: 60_000,
      }),
    },
  );
  assert.equal(response.status, 200);
  const envelope = (await response.json()) as {
    ok: boolean;
    data: Awaited<ReturnType<typeof bridge.deliverAgentMessage>>;
  };
  assert.equal(envelope.ok, true);
  if (tick) await controller.tick();
  return envelope.data;
}

interface PendingInteraction {
  interactionId: string;
  requestId: string;
  nativeTurnId: string;
  kind: string;
  prompt: unknown;
  revision: number;
}

async function waitForPendingInteraction(
  baseUrl: string,
): Promise<PendingInteraction> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await controller.tick();
    const response = await fetch(`${baseUrl}/v1/external-interactions`);
    assert.equal(response.status, 200);
    const envelope = (await response.json()) as {
      ok: boolean;
      data: { interactions: PendingInteraction[] };
    };
    const interaction = envelope.data.interactions.find(
      (candidate) => candidate.kind === "request_user_input",
    );
    if (interaction !== undefined) return interaction;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `Plan collaboration turn did not request user input after ${timeoutMs}ms`,
  );
}

function interactionQuestionId(prompt: unknown): string {
  if (
    typeof prompt === "object" &&
    prompt !== null &&
    "questions" in prompt &&
    Array.isArray(prompt.questions)
  ) {
    const question = prompt.questions[0];
    if (
      typeof question === "object" &&
      question !== null &&
      "id" in question &&
      typeof question.id === "string"
    ) {
      return question.id;
    }
  }
  throw new Error(
    `request_user_input prompt has no question id: ${JSON.stringify(prompt)}`,
  );
}

async function postControl(
  baseUrl: string,
  body: Record<string, unknown>,
): Promise<{ status: string }> {
  const response = await fetch(
    `${baseUrl}/v1/external-bindings/${bindingId}/controls`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    },
  );
  assert.equal(response.status, 200);
  const envelope = (await response.json()) as {
    ok: boolean;
    data: { status: string };
  };
  assert.equal(envelope.ok, true);
  return envelope.data;
}
