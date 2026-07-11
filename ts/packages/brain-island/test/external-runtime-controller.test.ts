import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import {
  CODEX_APP_SERVER_PROTOCOL,
  CodexAppServerDriver,
  type CodexJsonRpcTransport,
  type CodexTransportHandlers,
} from "@rusty-crew/external-runtime-codex";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

import { ServiceExternalRuntimeController } from "../src/service-external-runtime.js";

class FakeTransport implements CodexJsonRpcTransport {
  handlers?: CodexTransportHandlers;
  readonly sent: Array<Record<string, unknown>> = [];

  setHandlers(handlers: CodexTransportHandlers): void {
    this.handlers = handlers;
  }

  async open(): Promise<void> {}

  async send(message: string): Promise<void> {
    const parsed = JSON.parse(message) as Record<string, unknown>;
    this.sent.push(parsed);
    if (parsed.method === "initialize") {
      this.emit({
        id: parsed.id,
        result: {
          userAgent: `fake/${CODEX_APP_SERVER_PROTOCOL.cliVersion}`,
          codexHome: "/tmp/fake-codex-home",
          platformFamily: "unix",
          platformOs: "linux",
        },
      });
    }
    if (parsed.method === "turn/start") {
      this.emit({
        id: parsed.id,
        result: {
          turn: {
            id: "native-turn-1",
            items: [],
            itemsView: "full",
            status: "inProgress",
            error: null,
            startedAt: 1,
            completedAt: null,
            durationMs: null,
          },
        },
      });
    }
  }

  async close(): Promise<void> {}

  emit(value: unknown): void {
    queueMicrotask(() => this.handlers?.onMessage(JSON.stringify(value)));
  }
}

test("controller persists and resolves typed app-server interactions", async () => {
  const dataDir = mkdtempSync(
    join(tmpdir(), "rusty-crew-external-controller-"),
  );
  const bridge = await loadNativeBridge();
  const engine = await bridge.initializeEngine({
    engineDataDir: dataDir,
    clock: "system",
    defaultTurnBudget: 16,
    defaultIdleTimeoutMs: 30_000,
    storage: { backend: "sqlite" },
  });
  const transport = new FakeTransport();
  const controller = new ServiceExternalRuntimeController({
    bridge,
    instanceId: "interaction-test-controller",
    driverFactory: (_registration, authority) =>
      new CodexAppServerDriver(transport, authority),
  });
  const now = (): string => new Date().toISOString();

  try {
    await bridge.registerExternalRuntime({
      registration: {
        runtimeId: "interaction-runtime",
        kind: "codex_app_server",
        endpoint: { transport: "unix_web_socket", address: "/tmp/fake.sock" },
        processOwnership: "attached",
        expectedCliVersion: CODEX_APP_SERVER_PROTOCOL.cliVersion,
        executableSha256: CODEX_APP_SERVER_PROTOCOL.nativeExecutableSha256,
        protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
        desiredState: "enabled",
        observedState: "disconnected",
        revision: 0,
        createdAt: now(),
        updatedAt: now(),
      },
    });
    await controller.connect("interaction-runtime");
    await bridge.ensureConfiguredSession({
      sessionId: "interaction-session",
      agentId: "interaction-agent",
      profileId: "interaction-profile",
      kind: "full",
      toolProfile: { tools: [] },
    });
    await bridge.bindExternalAgent({
      binding: {
        bindingId: "interaction-binding",
        runtimeId: "interaction-runtime",
        sessionId: "interaction-session",
        agentId: "interaction-agent",
        purpose: "crew_agent",
        nativeThreadId: "native-thread-1",
        effectiveConfigFingerprint: "interaction-test",
        status: "active",
        revision: 0,
        createdAt: now(),
        updatedAt: now(),
      },
    });
    await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "interaction-delivery",
      idempotencyKey: "interaction-delivery",
      messageId: "interaction-message",
      toAgentId: "interaction-agent",
      body: "request approval",
      requireWake: true,
      createdAt: now(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    await controller.start();
    await waitUntil(
      async () =>
        (await bridge.listActiveExternalTurns()).some(
          (turn) => turn.nativeTurnId === "native-turn-1",
        ),
      "turn activation",
    );

    transport.emit({
      id: "approval-1",
      method: "item/commandExecution/requestApproval",
      params: {
        threadId: "native-thread-1",
        turnId: "native-turn-1",
        itemId: "item-1",
        startedAtMs: 1,
        environmentId: null,
      },
    });
    await waitUntil(
      async () => (await bridge.listPendingExternalInteractions()).length === 1,
      "interaction persistence",
    );
    const interaction = (await bridge.listPendingExternalInteractions())[0];
    assert.equal(interaction?.kind, "command_approval");
    await controller.resolveInteraction({
      interactionId: interaction?.interactionId ?? "",
      expectedRevision: interaction?.revision ?? 0,
      idempotencyKey: "approval-resolution-1",
      result: { decision: "decline" },
    });
    await waitUntil(
      async () =>
        transport.sent.some(
          (message) =>
            message.id === "approval-1" &&
            JSON.stringify(message.result) ===
              JSON.stringify({ decision: "decline" }),
        ),
      "interaction response",
    );
    assert.equal((await bridge.listActiveExternalTurns())[0]?.phase, "active");
  } finally {
    await controller.stop().catch(() => undefined);
    await bridge.shutdownEngine({ engine, drainTimeoutMs: 5_000 });
    rmSync(dataDir, { recursive: true, force: true });
  }
});

async function waitUntil(
  predicate: () => Promise<boolean>,
  label: string,
): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`timed out waiting for ${label}`);
}
