import assert from "node:assert/strict";
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
const now = (): string => new Date().toISOString();

const bridge = await loadNativeBridge();
const engine = await bridge.initializeEngine({
  engineDataDir: dataDir,
  clock: "system",
  defaultTurnBudget: 16,
  defaultIdleTimeoutMs: 30_000,
  storage: { backend: "sqlite" },
});
const controller = new ServiceExternalRuntimeController({ bridge });
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
  await bridge.ensureConfiguredSession({
    sessionId,
    agentId,
    profileId: "codex-service-live-profile",
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

  const sseResponse = await fetch(
    `${baseUrl}/v1/external-runtimes/${runtimeId}/stream?cursor=0&once=true`,
  );
  assert.equal(sseResponse.status, 200);
  const sse = await sseResponse.text();
  assert.match(sse, /event: assistant_text_delta/);
  assert.match(sse, /EXTERNAL_SERVICE_LIVE_OK/);
  assert.match(sse, /event: turn_lifecycle/);

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
  assert.equal(
    typeof interrupted.payload.turn === "object" &&
      interrupted.payload.turn !== null &&
      "status" in interrupted.payload.turn
      ? interrupted.payload.turn.status
      : undefined,
    "interrupted",
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

async function waitForActiveTurn() {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const active = (await bridge.listActiveExternalTurns()).find(
      (turn) =>
        turn.request.sessionId === sessionId &&
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

async function deliverLiveMessage(
  baseUrl: string,
  label: string,
  body: string,
) {
  const response = await fetch(
    `${baseUrl}/v1/external-bindings/${bindingId}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        deliveryId: `${label}-delivery`,
        idempotencyKey: `${label}-delivery`,
        messageId: `${label}-message`,
        body,
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
  await controller.tick();
  return envelope.data;
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
