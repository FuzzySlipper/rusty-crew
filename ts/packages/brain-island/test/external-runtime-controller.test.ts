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
  #nextTurn = 1;

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
      const turnId = `native-turn-${this.#nextTurn}`;
      this.#nextTurn += 1;
      this.emit({
        id: parsed.id,
        result: {
          turn: {
            id: turnId,
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
    if (parsed.method === "thread/resume") {
      this.emit({
        id: parsed.id,
        result: {
          thread: {
            id: "native-thread-1",
            extra: null,
            sessionId: "native-session-1",
            forkedFromId: null,
            parentThreadId: null,
            preview: "",
            ephemeral: false,
            historyMode: "paginated",
            modelProvider: "openai",
            createdAt: 1,
            updatedAt: 1,
            recencyAt: 1,
            status: { type: "idle" },
            path: null,
            cwd: "/home",
            cliVersion: CODEX_APP_SERVER_PROTOCOL.cliVersion,
            source: "unknown",
            threadSource: null,
            agentNickname: null,
            agentRole: null,
            gitInfo: null,
            name: null,
            turns: [],
          },
          model: "gpt-5.4",
          modelProvider: "openai",
          serviceTier: null,
          cwd: "/home",
          runtimeWorkspaceRoots: [],
          instructionSources: [],
          approvalPolicy: "never",
          approvalsReviewer: "user",
          sandbox: { type: "dangerFullAccess" },
          activePermissionProfile: null,
          reasoningEffort: null,
          multiAgentMode: "explicitRequestOnly",
          initialTurnsPage: null,
        },
      });
    }
    if (parsed.method === "collaborationMode/list") {
      this.emit({
        id: parsed.id,
        result: {
          data: [
            {
              name: "Plan",
              mode: "plan",
              model: "gpt-5.4",
              reasoning_effort: "medium",
            },
            {
              name: "Default",
              mode: "default",
              model: null,
              reasoning_effort: null,
            },
          ],
        },
      });
    }
  }

  async close(): Promise<void> {}

  emit(value: unknown): void {
    queueMicrotask(() => this.handlers?.onMessage(JSON.stringify(value)));
  }
}

test("controller resolves typed interactions and resets one-shot Plan mode", async () => {
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
    await controller.connect("interaction-runtime");
    const planDelivery = await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "interaction-delivery",
      idempotencyKey: "interaction-delivery",
      messageId: "interaction-message",
      toAgentId: "interaction-agent",
      body: "request approval",
      collaborationMode: "plan",
      requireWake: true,
      createdAt: now(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(planDelivery.activation?.type, "external_turn_requested");
    await controller.start();
    await waitUntil(
      async () =>
        (await bridge.listActiveExternalTurns()).some(
          (turn) => turn.nativeTurnId === "native-turn-1",
        ),
      "turn activation",
    );
    const collaborationList = transport.sent.find(
      (message) => message.method === "collaborationMode/list",
    );
    assert.equal(collaborationList?.method, "collaborationMode/list");
    const turnStart = transport.sent.find(
      (message) => message.method === "turn/start",
    );
    assert.deepEqual(
      (turnStart?.params as Record<string, unknown>)?.collaborationMode,
      {
        mode: "plan",
        settings: {
          model: "gpt-5.4",
          reasoning_effort: "medium",
          developer_instructions: null,
        },
      },
    );
    assert.equal(
      (turnStart?.params as Record<string, unknown>)?.approvalPolicy,
      "never",
    );
    assert.deepEqual(
      (turnStart?.params as Record<string, unknown>)?.sandboxPolicy,
      { type: "dangerFullAccess" },
    );

    transport.emit({
      id: "input-1",
      method: "item/tool/requestUserInput",
      params: {
        threadId: "native-thread-1",
        turnId: "native-turn-1",
        itemId: "item-1",
        questions: [],
        autoResolutionMs: null,
      },
    });
    await waitUntil(
      async () => (await bridge.listPendingExternalInteractions()).length === 1,
      "interaction persistence",
    );
    const interaction = (await bridge.listPendingExternalInteractions())[0];
    assert.equal(interaction?.kind, "request_user_input");
    await controller.resolveInteraction({
      interactionId: interaction?.interactionId ?? "",
      expectedRevision: interaction?.revision ?? 0,
      idempotencyKey: "input-resolution-1",
      result: { answers: {} },
    });
    await waitUntil(
      async () =>
        transport.sent.some(
          (message) =>
            message.id === "input-1" &&
            JSON.stringify(message.result) === JSON.stringify({ answers: {} }),
        ),
      "interaction response",
    );
    assert.equal((await bridge.listActiveExternalTurns())[0]?.phase, "active");

    transport.emit({
      method: "turn/completed",
      params: {
        threadId: "native-thread-1",
        turn: {
          id: "native-turn-1",
          items: [],
          itemsView: "full",
          status: "completed",
          error: null,
          startedAt: 1,
          completedAt: 2,
          durationMs: 1_000,
        },
      },
    });
    await waitUntil(
      async () => (await bridge.listActiveExternalTurns()).length === 0,
      "first turn completion",
    );
    const defaultDelivery = await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "default-delivery",
      idempotencyKey: "default-delivery",
      messageId: "default-message",
      toAgentId: "interaction-agent",
      body: "perform a normal mutation",
      requireWake: true,
      createdAt: now(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(defaultDelivery.activation?.type, "external_turn_requested");
    await controller.tick();
    assert.equal(
      transport.sent.filter((message) => message.method === "turn/start")
        .length,
      2,
    );
    await waitUntil(
      async () =>
        (await bridge.listActiveExternalTurns()).some(
          (turn) => turn.nativeTurnId === "native-turn-2",
        ),
      "default turn activation",
    );
    const turnStarts = transport.sent.filter(
      (message) => message.method === "turn/start",
    );
    const defaultTurnStart = turnStarts[1];
    assert.deepEqual(
      (defaultTurnStart?.params as Record<string, unknown>)?.collaborationMode,
      {
        mode: "default",
        settings: {
          model: "gpt-5.4",
          reasoning_effort: null,
          developer_instructions: null,
        },
      },
    );
    assert.equal(
      (defaultTurnStart?.params as Record<string, unknown>)?.approvalPolicy,
      "never",
    );
    assert.deepEqual(
      (defaultTurnStart?.params as Record<string, unknown>)?.sandboxPolicy,
      { type: "dangerFullAccess" },
    );
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
