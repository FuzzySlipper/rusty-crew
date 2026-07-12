import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import type { ExternalAgentSessionCreationRequest } from "@rusty-crew/contracts";
import {
  CODEX_APP_SERVER_PROTOCOL,
  CodexAppServerDriver,
  type CodexJsonRpcTransport,
  type CodexTransportHandlers,
} from "@rusty-crew/external-runtime-codex";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";

import {
  ExternalAgentSessionCreationError,
  ExternalThreadLifecycleError,
  ServiceExternalRuntimeController,
} from "../src/service-external-runtime.js";

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

class FakeCreationTransport implements CodexJsonRpcTransport {
  handlers?: CodexTransportHandlers;
  readonly sent: Array<Record<string, unknown>> = [];
  readonly threads: Array<Record<string, unknown>> = [];
  readonly resumeFailureThreadIds = new Set<string>();
  readonly archivedThreadIds = new Set<string>();
  readonly unmaterializedThreadIds = new Set<string>();
  deleteFailureMessage?: string;
  loseNextDeleteResponse = false;
  #loseFirstStartResponse: boolean;
  readonly #startFailureMessage?: string;

  constructor(loseFirstStartResponse = false, startFailureMessage?: string) {
    this.#loseFirstStartResponse = loseFirstStartResponse;
    this.#startFailureMessage = startFailureMessage;
  }

  setHandlers(handlers: CodexTransportHandlers): void {
    this.handlers = handlers;
  }

  async open(): Promise<void> {}

  async send(message: string): Promise<void> {
    const parsed = JSON.parse(message) as Record<string, unknown>;
    this.sent.push(parsed);
    const params = (parsed.params ?? {}) as Record<string, unknown>;
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
      return;
    }
    if (parsed.method === "thread/list") {
      this.emit({
        id: parsed.id,
        result: {
          data: this.threads.filter((thread) =>
            params.archived === true
              ? this.archivedThreadIds.has(String(thread.id))
              : !this.archivedThreadIds.has(String(thread.id)),
          ),
          nextCursor: null,
          backwardsCursor: null,
        },
      });
      return;
    }
    if (parsed.method === "thread/archive") {
      this.archivedThreadIds.add(String(params.threadId));
      this.emit({ id: parsed.id, result: {} });
      return;
    }
    if (parsed.method === "thread/delete") {
      if (this.deleteFailureMessage !== undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: this.deleteFailureMessage },
        });
        return;
      }
      const rootThreadId = String(params.threadId);
      if (!this.threads.some((thread) => thread.id === rootThreadId)) {
        this.emit({
          id: parsed.id,
          error: {
            code: -32000,
            message: `no rollout found for thread id ${rootThreadId}`,
          },
        });
        return;
      }
      const deletedThreadIds = new Set([rootThreadId]);
      let changed = true;
      while (changed) {
        changed = false;
        for (const thread of this.threads) {
          if (
            typeof thread.parentThreadId === "string" &&
            deletedThreadIds.has(thread.parentThreadId) &&
            !deletedThreadIds.has(String(thread.id))
          ) {
            deletedThreadIds.add(String(thread.id));
            changed = true;
          }
        }
      }
      for (let index = this.threads.length - 1; index >= 0; index -= 1) {
        const candidate = this.threads[index];
        if (
          candidate !== undefined &&
          deletedThreadIds.has(String(candidate.id))
        ) {
          this.threads.splice(index, 1);
        }
      }
      for (const deletedThreadId of deletedThreadIds) {
        this.archivedThreadIds.delete(deletedThreadId);
        this.unmaterializedThreadIds.delete(deletedThreadId);
      }
      if (this.loseNextDeleteResponse) {
        this.loseNextDeleteResponse = false;
      } else {
        this.emit({ id: parsed.id, result: {} });
      }
      for (const deletedThreadId of deletedThreadIds) {
        this.emit({
          method: "thread/deleted",
          params: { threadId: deletedThreadId },
        });
      }
      return;
    }
    if (parsed.method === "thread/unarchive") {
      this.archivedThreadIds.delete(String(params.threadId));
      const thread = this.threads.find(
        (candidate) => candidate.id === params.threadId,
      );
      this.emit({ id: parsed.id, result: { thread } });
      return;
    }
    if (parsed.method === "thread/start") {
      if (this.#startFailureMessage !== undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: this.#startFailureMessage },
        });
        return;
      }
      const thread = fakeCreationThread(
        `created-thread-${this.threads.length + 1}`,
        String(params.cwd),
        String(params.threadSource),
      );
      this.threads.push(thread);
      if (this.#loseFirstStartResponse) {
        this.#loseFirstStartResponse = false;
        return;
      }
      this.emit({
        id: parsed.id,
        result: fakeThreadStartResponse(thread),
      });
      return;
    }
    if (parsed.method === "thread/read") {
      if (
        params.includeTurns !== false &&
        this.unmaterializedThreadIds.has(String(params.threadId))
      ) {
        this.emit({
          id: parsed.id,
          error: {
            code: -32000,
            message: `thread ${String(params.threadId)} is not materialized yet; includeTurns is unavailable before first user message`,
          },
        });
        return;
      }
      const thread = this.threads.find(
        (candidate) => candidate.id === params.threadId,
      );
      this.emit({ id: parsed.id, result: { thread } });
      return;
    }
    if (parsed.method === "thread/resume") {
      if (this.resumeFailureThreadIds.has(String(params.threadId))) {
        this.emit({
          id: parsed.id,
          error: {
            code: -32001,
            message: `native thread ${String(params.threadId)} was not found`,
          },
        });
        return;
      }
      const thread = this.threads.find(
        (candidate) => candidate.id === params.threadId,
      );
      this.emit({
        id: parsed.id,
        result: {
          ...fakeThreadStartResponse(thread ?? {}),
          initialTurnsPage: null,
        },
      });
    }
  }

  async close(): Promise<void> {}

  emit(value: unknown): void {
    queueMicrotask(() => this.handlers?.onMessage(JSON.stringify(value)));
  }
}

function fakeCreationThread(
  id: string,
  cwd: string,
  threadSource: string,
): Record<string, unknown> {
  return {
    id,
    extra: null,
    sessionId: `native-session-${id}`,
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
    cwd,
    cliVersion: CODEX_APP_SERVER_PROTOCOL.cliVersion,
    source: "appServer",
    threadSource,
    agentNickname: null,
    agentRole: null,
    gitInfo: null,
    name: null,
    turns: [],
  };
}

function fakeThreadStartResponse(
  thread: Record<string, unknown>,
): Record<string, unknown> {
  return {
    thread,
    model: "gpt-5.4",
    modelProvider: "openai",
    serviceTier: null,
    cwd: thread.cwd,
    runtimeWorkspaceRoots: [],
    instructionSources: [],
    approvalPolicy: "never",
    approvalsReviewer: "user",
    sandbox: { type: "dangerFullAccess" },
    activePermissionProfile: null,
    reasoningEffort: null,
    multiAgentMode: "explicitRequestOnly",
  };
}

test("controller atomically creates and idempotently reuses an external agent session", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const request = {
      idempotencyKey: "browser-create-1",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      taskRef: {
        projectId: "rusty-crew",
        taskId: "5678",
      } as ExternalAgentSessionCreationRequest["taskRef"],
      label: "Browser Codex agent",
      requestedAt: new Date().toISOString(),
    } as const;
    const created = await fixture.controller.createAgentSession(request);
    assert.equal(created.creation.phase, "ready");
    assert.equal(created.thread.threadId, "created-thread-1");
    assert.equal(created.creation.session.profileId, fixture.profileId);
    assert.equal(created.creation.binding.nativeThreadId, "created-thread-1");
    assert.deepEqual(created.creation.request.taskRef, {
      project_id: "rusty-crew",
      task_id: "5678",
    });
    assert.deepEqual(created.creation.binding.taskRef, {
      project_id: "rusty-crew",
      task_id: "5678",
    });
    const persistedBinding = await fixture.bridge.getExternalBinding(
      created.creation.binding.bindingId,
    );
    assert.deepEqual(persistedBinding?.taskRef, {
      project_id: "rusty-crew",
      task_id: "5678",
    });

    const retried = await fixture.controller.createAgentSession({
      ...request,
      requestedAt: new Date(Date.now() + 1_000).toISOString(),
    });
    assert.equal(retried.creation.creationId, created.creation.creationId);
    assert.equal(
      fixture.transport.sent.filter(
        (message) => message.method === "thread/start",
      ).length,
      1,
    );

    await assert.rejects(
      fixture.controller.createAgentSession({
        ...request,
        label: "Changed intent",
      }),
      /external_agent_creation_idempotency_conflict/,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("controller archives native history with bindings and restores history explicitly", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "archive-browser-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const threadId = created.thread.threadId;
    const bindingId = created.creation.binding.bindingId;
    const nativeThread = fixture.transport.threads.find(
      (thread) => thread.id === threadId,
    );
    assert.ok(nativeThread);
    nativeThread.status = { type: "active", activeFlags: [] };
    await assert.rejects(
      fixture.controller.archiveThread(fixture.runtimeId, threadId),
      (error: unknown) =>
        error instanceof ExternalThreadLifecycleError &&
        error.reasonCode === "external_thread_active",
    );
    nativeThread.status = { type: "idle" };

    const archived = await fixture.controller.archiveThread(
      fixture.runtimeId,
      threadId,
    );
    assert.equal(archived.outcome, "applied");
    assert.equal(archived.nativeArchived, true);
    assert.equal(archived.bindings[0]?.currentStatus, "archived");
    assert.equal(
      (await fixture.bridge.getExternalBinding(bindingId))?.status,
      "archived",
    );
    assert.deepEqual(
      (
        await fixture.controller.listThreads(fixture.runtimeId, {
          limit: 50,
          archived: false,
        })
      ).items,
      [],
    );
    assert.equal(
      (await fixture.controller.archiveThread(fixture.runtimeId, threadId))
        .outcome,
      "already_archived",
    );

    const restored = await fixture.controller.unarchiveThread(
      fixture.runtimeId,
      threadId,
    );
    assert.equal(restored.outcome, "applied");
    assert.equal(restored.nativeArchived, false);
    assert.equal(restored.bindings[0]?.currentStatus, "archived");
    assert.equal(
      (await fixture.bridge.getExternalBinding(bindingId))?.status,
      "archived",
    );
    assert.equal(
      (await fixture.controller.unarchiveThread(fixture.runtimeId, threadId))
        .outcome,
      "already_active",
    );

    await assert.rejects(
      fixture.controller.archiveThread(fixture.runtimeId, "missing-thread"),
      (error: unknown) =>
        error instanceof ExternalThreadLifecycleError &&
        error.reasonCode === "external_thread_not_found",
    );
  } finally {
    await fixture.cleanup();
  }
});

test("controller deletes native thread trees only after durable binding reconciliation", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const root = await fixture.controller.createAgentSession({
      idempotencyKey: "delete-root-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const child = await fixture.controller.createAgentSession({
      idempotencyKey: "delete-child-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const rootThreadId = root.thread.threadId;
    const childThreadId = child.thread.threadId;
    const childThread = fixture.transport.threads.find(
      (thread) => thread.id === childThreadId,
    );
    assert.ok(childThread);
    childThread.parentThreadId = rootThreadId;
    childThread.status = { type: "active", activeFlags: [] };

    await assert.rejects(
      fixture.controller.deleteThread(fixture.runtimeId, rootThreadId),
      (error: unknown) =>
        error instanceof ExternalThreadLifecycleError &&
        error.reasonCode === "external_thread_active",
    );
    childThread.status = { type: "idle" };

    fixture.transport.deleteFailureMessage = "simulated native delete failure";
    await assert.rejects(
      fixture.controller.deleteThread(fixture.runtimeId, rootThreadId),
      (error: unknown) =>
        error instanceof ExternalThreadLifecycleError &&
        error.reasonCode === "external_thread_native_delete_failed",
    );
    assert.equal(
      (await fixture.bridge.getExternalBinding(root.creation.binding.bindingId))
        ?.status,
      "active",
    );
    assert.equal(
      (
        await fixture.bridge.getExternalBinding(
          child.creation.binding.bindingId,
        )
      )?.status,
      "active",
    );

    fixture.transport.deleteFailureMessage = undefined;
    fixture.transport.loseNextDeleteResponse = true;
    const deleted = await fixture.controller.deleteThread(
      fixture.runtimeId,
      rootThreadId,
    );
    assert.equal(deleted.outcome, "applied");
    assert.equal(deleted.nativeDeleted, true);
    assert.deepEqual(
      deleted.bindings.map((binding) => binding.currentStatus).sort(),
      ["archived", "archived"],
    );
    assert.deepEqual(fixture.transport.threads, []);
    assert.equal(
      (await fixture.bridge.getExternalBinding(root.creation.binding.bindingId))
        ?.status,
      "archived",
    );
    assert.equal(
      (
        await fixture.bridge.getExternalBinding(
          child.creation.binding.bindingId,
        )
      )?.status,
      "archived",
    );

    const repeated = await fixture.controller.deleteThread(
      fixture.runtimeId,
      rootThreadId,
    );
    assert.equal(repeated.outcome, "already_deleted");
    assert.equal(repeated.nativeDeleted, true);
  } finally {
    await fixture.cleanup();
  }
});

test("thread snapshots preserve message phase across controller reload", async () => {
  const fixture = await externalCreationFixture(false);
  let reloaded: ServiceExternalRuntimeController | undefined;
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "phase-snapshot-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const thread = fixture.transport.threads.find(
      (candidate) => candidate.id === created.thread.threadId,
    );
    assert.ok(thread);
    thread.turns = [
      {
        id: "phase-turn-1",
        itemsView: "full",
        status: "completed",
        error: null,
        startedAt: 1,
        completedAt: 2,
        durationMs: 1_000,
        items: [
          {
            type: "agentMessage",
            id: "phase-commentary-1",
            text: "Checking files.",
            phase: "commentary",
            memoryCitation: null,
          },
          {
            type: "reasoning",
            id: "phase-reasoning-1",
            summary: ["Compared contracts"],
            content: [],
          },
          {
            type: "agentMessage",
            id: "phase-final-1",
            text: "Finished.",
            phase: "final_answer",
            memoryCitation: null,
          },
          {
            type: "agentMessage",
            id: "phase-legacy-1",
            text: "Legacy phase-less message.",
            phase: null,
            memoryCitation: null,
          },
        ],
      },
    ];

    const before = await fixture.controller.readThread(fixture.runtimeId, {
      threadId: created.thread.threadId,
      includeTurns: true,
    });
    assert.deepEqual(
      before.thread.turns[0]?.items.map((item) => ({
        kind: item.kind,
        messagePhase: item.messagePhase,
      })),
      [
        { kind: "agentMessage", messagePhase: "commentary" },
        { kind: "reasoning", messagePhase: undefined },
        { kind: "agentMessage", messagePhase: "final_answer" },
        { kind: "agentMessage", messagePhase: undefined },
      ],
    );

    await fixture.controller.stop();
    reloaded = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "phase-reload-controller",
      driverFactory: (_registration, authority) =>
        new CodexAppServerDriver(fixture.transport, authority),
    });
    await reloaded.connect(fixture.runtimeId);
    const after = await reloaded.readThread(fixture.runtimeId, {
      threadId: created.thread.threadId,
      includeTurns: true,
    });
    assert.deepEqual(after.thread.turns, before.thread.turns);
  } finally {
    await reloaded?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

test("thread read returns phase-neutral metadata before first message materializes", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "unmaterialized-thread-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    fixture.transport.unmaterializedThreadIds.add(created.thread.threadId);

    const read = await fixture.controller.readThread(fixture.runtimeId, {
      threadId: created.thread.threadId,
      includeTurns: true,
    });
    assert.equal(read.thread.threadId, created.thread.threadId);
    assert.deepEqual(read.thread.turns, []);
    assert.deepEqual(
      fixture.transport.sent
        .filter((message) => message.method === "thread/read")
        .slice(-2)
        .map((message) => message.params),
      [
        { threadId: created.thread.threadId, includeTurns: true },
        { threadId: created.thread.threadId, includeTurns: false },
      ],
    );
  } finally {
    await fixture.cleanup();
  }
});

test("controller isolates stale binding resume failures and repairs degraded readiness", async () => {
  const fixture = await externalCreationFixture(false);
  let recoveryController: ServiceExternalRuntimeController | undefined;
  try {
    const original = await fixture.controller.createAgentSession({
      idempotencyKey: "restart-stale-binding-original",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const staleThreadId = original.creation.binding.nativeThreadId;
    assert.equal(typeof staleThreadId, "string");
    await fixture.controller.stop();

    const recoveryTransport = new FakeCreationTransport();
    recoveryTransport.resumeFailureThreadIds.add(staleThreadId!);
    recoveryTransport.threads.push(
      fakeCreationThread(
        "unrelated-native-thread",
        fixture.dataDir,
        "unrelated-thread-source",
      ),
    );
    recoveryController = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "restart-recovery-controller",
      driverFactory: (_registration, authority) =>
        new CodexAppServerDriver(recoveryTransport, authority, {
          requestTimeoutMs: 50,
        }),
    });

    const recovered = await recoveryController.connect(fixture.runtimeId);
    assert.equal(recovered.driverState, "ready");
    assert.equal(recovered.bindingResumeFailures.length, 1);
    assert.equal(
      recovered.bindingResumeFailures[0]?.bindingId,
      original.creation.binding.bindingId,
    );
    assert.equal(
      recovered.bindingResumeFailures[0]?.nativeThreadId,
      staleThreadId,
    );
    assert.match(
      recovered.bindingResumeFailures[0]?.reason ?? "",
      new RegExp(`native thread ${staleThreadId} was not found`),
    );
    assert.equal(
      typeof recovered.bindingResumeFailures[0]?.observedAt,
      "string",
    );
    assert.equal(
      (await fixture.bridge.getExternalRuntime(fixture.runtimeId))
        ?.observedState,
      "ready",
    );

    await fixture.bridge.recordExternalRuntimeState({
      runtimeId: fixture.runtimeId,
      controller: {
        holderInstanceId: recovered.controllerInstanceId,
        generation: recovered.controllerGeneration,
      },
      observedState: "degraded",
      reasonCode: "controller_connect_failed",
      observedAt: new Date().toISOString(),
    });
    const reconciled = await recoveryController.connect(fixture.runtimeId);
    assert.equal(reconciled.driverState, "ready");
    assert.equal(reconciled.bindingResumeFailures.length, 1);
    const registration = await fixture.bridge.getExternalRuntime(
      fixture.runtimeId,
    );
    assert.equal(registration?.observedState, "ready");
    assert.equal(registration?.observedReasonCode, null);
    const resumeCallsBeforeCreate = recoveryTransport.sent.filter(
      (message) => message.method === "thread/resume",
    ).length;

    const created = await recoveryController.createAgentSession({
      idempotencyKey: "restart-stale-binding-new-browser-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    assert.equal(created.creation.phase, "ready");
    assert.notEqual(
      created.creation.binding.bindingId,
      original.creation.binding.bindingId,
    );
    assert.equal(
      recoveryTransport.sent.filter(
        (message) => message.method === "thread/resume",
      ).length,
      resumeCallsBeforeCreate,
    );
  } finally {
    await recoveryController?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

test("controller recovers a lost native thread start response without duplicating the thread", async () => {
  const fixture = await externalCreationFixture(true);
  try {
    const request = {
      idempotencyKey: "browser-create-lost-response",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    } as const;
    await assert.rejects(
      fixture.controller.createAgentSession(request),
      /external_agent_creation_native_start_failed/,
    );
    const recovered = await fixture.controller.createAgentSession(request);
    assert.equal(recovered.creation.phase, "ready");
    assert.equal(recovered.thread.threadId, "created-thread-1");
    assert.equal(fixture.transport.threads.length, 1);
    assert.equal(
      fixture.transport.sent.filter(
        (message) => message.method === "thread\/start",
      ).length,
      1,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("controller reports native capacity rejection with a stable retryable reason", async () => {
  const fixture = await externalCreationFixture(
    false,
    "resource exhausted: external runtime capacity reached",
  );
  try {
    await assert.rejects(
      fixture.controller.createAgentSession({
        idempotencyKey: "browser-create-capacity",
        runtimeId: fixture.runtimeId,
        profileId: fixture.profileId,
        cwd: fixture.dataDir,
        requestedAt: new Date().toISOString(),
      }),
      (error: unknown) =>
        error instanceof ExternalAgentSessionCreationError &&
        error.reasonCode === "external_agent_creation_capacity_conflict" &&
        error.retryable,
    );
    assert.equal(fixture.transport.threads.length, 0);
  } finally {
    await fixture.cleanup();
  }
});

test("controller preserves a Rust revision conflict before native thread start", async () => {
  const fixture = await externalCreationFixture(false, undefined, {
    operation: "mark_native_starting",
    message: "external_agent_creation_revision_conflict: expected 1, found 2",
  });
  try {
    await assert.rejects(
      fixture.controller.createAgentSession({
        idempotencyKey: "browser-create-revision-conflict",
        runtimeId: fixture.runtimeId,
        profileId: fixture.profileId,
        cwd: fixture.dataDir,
        requestedAt: new Date().toISOString(),
      }),
      (error: unknown) =>
        error instanceof ExternalAgentSessionCreationError &&
        error.reasonCode === "external_agent_creation_revision_conflict",
    );
    assert.equal(fixture.transport.threads.length, 0);
  } finally {
    await fixture.cleanup();
  }
});

test("controller preserves a Rust native thread conflict after native start", async () => {
  const fixture = await externalCreationFixture(false, undefined, {
    operation: "complete",
    message:
      "external_agent_creation_native_thread_conflict: creation is already bound to a different native thread",
  });
  try {
    await assert.rejects(
      fixture.controller.createAgentSession({
        idempotencyKey: "browser-create-native-thread-conflict",
        runtimeId: fixture.runtimeId,
        profileId: fixture.profileId,
        cwd: fixture.dataDir,
        requestedAt: new Date().toISOString(),
      }),
      (error: unknown) =>
        error instanceof ExternalAgentSessionCreationError &&
        error.reasonCode === "external_agent_creation_native_thread_conflict",
    );
    assert.equal(fixture.transport.threads.length, 1);
  } finally {
    await fixture.cleanup();
  }
});

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

async function externalCreationFixture(
  loseFirstStartResponse: boolean,
  startFailureMessage?: string,
  bridgeFailure?: {
    operation: "mark_native_starting" | "complete";
    message: string;
  },
) {
  const dataDir = mkdtempSync(
    join(tmpdir(), "rusty-crew-external-creation-controller-"),
  );
  const runtimeId = "creation-runtime";
  const profileId = "creation-profile";
  const bridge = await loadNativeBridge();
  const controllerBridge = new Proxy(bridge, {
    get(target, property, receiver) {
      if (
        bridgeFailure?.operation === "mark_native_starting" &&
        property === "markExternalAgentSessionNativeStarting"
      ) {
        return async () => {
          throw new Error(bridgeFailure.message);
        };
      }
      if (
        bridgeFailure?.operation === "complete" &&
        property === "completeExternalAgentSessionCreation"
      ) {
        return async () => {
          throw new Error(bridgeFailure.message);
        };
      }
      return Reflect.get(target, property, receiver) as unknown;
    },
  }) as NativeBridgeModule;
  const engine = await bridge.initializeEngine({
    engineDataDir: dataDir,
    clock: "system",
    defaultTurnBudget: 16,
    defaultIdleTimeoutMs: 30_000,
    storage: { backend: "sqlite" },
  });
  const transport = new FakeCreationTransport(
    loseFirstStartResponse,
    startFailureMessage,
  );
  const controller = new ServiceExternalRuntimeController({
    bridge: controllerBridge,
    instanceId: "creation-test-controller",
    driverFactory: (_registration, authority) =>
      new CodexAppServerDriver(transport, authority, {
        requestTimeoutMs: 50,
      }),
  });
  const now = new Date().toISOString();
  await bridge.createProfileRegistryRecord({
    profileId,
    lifecycleStatus: "active",
    displayName: "Creation profile",
    defaultSessionKind: "full",
    agentId: profileId,
    activeRuntimeSettingsJson: {},
    sourceAssetRefs: [],
    derivedRuntimeRefs: [],
    importExport: { metadataJson: {} },
    now,
  });
  await bridge.registerExternalRuntime({
    registration: {
      runtimeId,
      kind: "codex_app_server",
      endpoint: { transport: "unix_web_socket", address: "/tmp/fake.sock" },
      processOwnership: "attached",
      expectedCliVersion: CODEX_APP_SERVER_PROTOCOL.cliVersion,
      executableSha256: CODEX_APP_SERVER_PROTOCOL.nativeExecutableSha256,
      protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
      desiredState: "enabled",
      observedState: "disconnected",
      revision: 0,
      createdAt: now,
      updatedAt: now,
    },
  });
  await controller.connect(runtimeId);
  return {
    dataDir,
    runtimeId,
    profileId,
    bridge,
    transport,
    controller,
    cleanup: async () => {
      await controller.stop().catch(() => undefined);
      await bridge.shutdownEngine({ engine, drainTimeoutMs: 5_000 });
      rmSync(dataDir, { recursive: true, force: true });
    },
  };
}

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
