import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import type {
  ExternalAgentBinding,
  ExternalAgentSessionCreationRequest,
  SessionId,
} from "@rusty-crew/contracts";
import {
  CODEX_ERROR_DIAGNOSTIC_LIMITS,
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

function isCompatibilityProbeThreadId(threadId: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
    threadId,
  );
}

function nativeErrorPayload(payload: unknown):
  | {
      nativeMethod: "error";
      error: {
        message: string;
        code: string | null;
        additionalDetails: string | null;
        willRetry: boolean;
      };
    }
  | undefined {
  if (typeof payload !== "object" || payload === null) return undefined;
  const candidate = payload as Record<string, unknown>;
  if (candidate.nativeMethod !== "error") return undefined;
  if (typeof candidate.error !== "object" || candidate.error === null) {
    return undefined;
  }
  const error = candidate.error as Record<string, unknown>;
  if (
    typeof error.message !== "string" ||
    (typeof error.code !== "string" && error.code !== null) ||
    (typeof error.additionalDetails !== "string" &&
      error.additionalDetails !== null) ||
    typeof error.willRetry !== "boolean"
  ) {
    return undefined;
  }
  return {
    nativeMethod: "error",
    error: {
      message: error.message,
      code: error.code,
      additionalDetails: error.additionalDetails,
      willRetry: error.willRetry,
    },
  };
}

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
    if (parsed.method === "model/list") {
      this.emit({ id: parsed.id, result: { data: [], nextCursor: null } });
    }
    if (parsed.method === "thread/list") {
      this.emit({
        id: parsed.id,
        result: { data: [], nextCursor: null, backwardsCursor: null },
      });
    }
    if (parsed.method === "thread/read") {
      const threadId = String(
        (parsed.params as Record<string, unknown>).threadId,
      );
      this.emit({
        id: parsed.id,
        error: { code: -32600, message: `thread not loaded: ${threadId}` },
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
    if (parsed.method === "turn/steer") {
      this.emit({
        id: parsed.id,
        result: {
          turnId: String(
            (parsed.params as Record<string, unknown>).expectedTurnId,
          ),
        },
      });
    }
    if (parsed.method === "turn/interrupt") {
      const params = parsed.params as Record<string, unknown>;
      this.emit({ id: parsed.id, result: {} });
      this.emit({
        method: "turn/completed",
        params: {
          threadId: params.threadId,
          turn: {
            id: params.turnId,
            items: [],
            itemsView: "full",
            status: "interrupted",
            error: null,
            startedAt: 1,
            completedAt: 2,
            durationMs: 1_000,
          },
        },
      });
    }
    if (parsed.method === "thread/resume") {
      const threadId = String(
        (parsed.params as Record<string, unknown>).threadId,
      );
      if (isCompatibilityProbeThreadId(threadId)) {
        this.emit({
          id: parsed.id,
          error: {
            code: -32600,
            message: `no rollout found for thread id ${threadId}`,
          },
        });
        return;
      }
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

  disconnect(reason = "test app-server restart"): void {
    this.handlers?.onClose(reason);
  }

  emit(value: unknown): void {
    queueMicrotask(() => this.handlers?.onMessage(JSON.stringify(value)));
  }
}

class TurnStartTimeoutTransport extends FakeTransport {
  override async send(message: string): Promise<void> {
    const parsed = JSON.parse(message) as Record<string, unknown>;
    if (parsed.method === "turn/start") {
      this.sent.push(parsed);
      return;
    }
    await super.send(message);
  }
}

class FakeCreationTransport implements CodexJsonRpcTransport {
  handlers?: CodexTransportHandlers;
  readonly sent: Array<Record<string, unknown>> = [];
  readonly threads: Array<Record<string, unknown>> = [];
  modelListNotificationMessage?: string;
  readonly resumeFailureThreadIds = new Set<string>();
  settingsUpdateError?: { code: number; message: string };
  nameSetError?: { code: number; message: string };
  readonly archivedThreadIds = new Set<string>();
  readonly unmaterializedThreadIds = new Set<string>();
  readonly threadDynamicTools = new Map<string, unknown>();
  readonly threadSettings = new Map<
    string,
    { model: string; modelProvider: string; effort: string | null }
  >();
  deleteFailureMessage?: string;
  loseNextDeleteResponse = false;
  #nextTurn = 1;
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
    if (parsed.method === "model/list") {
      if (this.modelListNotificationMessage !== undefined) {
        this.emit({
          method: "future/runtime-status",
          params: { text: this.modelListNotificationMessage },
        });
      }
      this.emit({
        id: parsed.id,
        result: {
          data: [
            fakeModel("gpt-5.4", ["low", "medium", "high"], "medium", true),
            fakeModel("gpt-5.4-mini", ["low", "medium"], "low", false),
          ],
          nextCursor: null,
        },
      });
      return;
    }
    if (parsed.method === "collaborationMode/list") {
      this.emit({
        id: parsed.id,
        result: {
          data: [
            {
              name: "Default",
              mode: "default",
              model: null,
              reasoning_effort: null,
            },
          ],
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
        this.threadDynamicTools.delete(deletedThreadId);
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
      if (thread === undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: "thread not found" },
        });
        return;
      }
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
      this.threadDynamicTools.set(
        String(thread.id),
        params.dynamicTools ?? null,
      );
      const config = (params.config ?? {}) as Record<string, unknown>;
      this.threadSettings.set(String(thread.id), {
        model: typeof params.model === "string" ? params.model : "gpt-5.4",
        modelProvider:
          typeof params.modelProvider === "string"
            ? params.modelProvider
            : "openai",
        effort:
          typeof config.model_reasoning_effort === "string"
            ? config.model_reasoning_effort
            : null,
      });
      if (this.#loseFirstStartResponse) {
        this.#loseFirstStartResponse = false;
        return;
      }
      this.emit({
        id: parsed.id,
        result: fakeThreadStartResponse(
          thread,
          this.threadSettings.get(String(thread.id)),
        ),
      });
      return;
    }
    if (parsed.method === "turn/start") {
      const thread = this.threads.find(
        (candidate) => candidate.id === params.threadId,
      );
      if (thread === undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: "thread not found" },
        });
        return;
      }
      const turn = {
        id: `native-turn-${this.#nextTurn}`,
        items: [],
        itemsView: "full",
        status: "inProgress",
        error: null,
        startedAt: 1,
        completedAt: null,
        durationMs: null,
      };
      this.#nextTurn += 1;
      (thread.turns as Array<Record<string, unknown>>).push(turn);
      this.emit({ id: parsed.id, result: { turn } });
      return;
    }
    if (parsed.method === "thread/fork") {
      const source = this.threads.find(
        (candidate) => candidate.id === params.threadId,
      );
      if (source === undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: "thread not found" },
        });
        return;
      }
      const thread = {
        ...source,
        id: `forked-thread-${this.threads.length + 1}`,
        sessionId: `native-session-fork-${this.threads.length + 1}`,
        forkedFromId: params.threadId,
        parentThreadId: params.threadId,
        cwd: params.cwd ?? source.cwd,
      };
      this.threads.push(thread);
      this.threadDynamicTools.set(
        String(thread.id),
        this.threadDynamicTools.get(String(params.threadId)) ?? null,
      );
      this.threadSettings.set(String(thread.id), {
        model: "gpt-5.4",
        modelProvider: "openai",
        effort: null,
      });
      this.emit({
        id: parsed.id,
        result: fakeThreadStartResponse(
          thread,
          this.threadSettings.get(String(thread.id)),
        ),
      });
      return;
    }
    if (parsed.method === "thread/read") {
      const threadId = String(params.threadId);
      if (isCompatibilityProbeThreadId(threadId)) {
        this.emit({
          id: parsed.id,
          error: { code: -32600, message: `thread not loaded: ${threadId}` },
        });
        return;
      }
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
      if (thread === undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: "thread not found" },
        });
        return;
      }
      this.emit({ id: parsed.id, result: { thread } });
      return;
    }
    if (parsed.method === "thread/resume") {
      const threadId = String(params.threadId);
      if (isCompatibilityProbeThreadId(threadId)) {
        this.emit({
          id: parsed.id,
          error: {
            code: -32600,
            message: `no rollout found for thread id ${threadId}`,
          },
        });
        return;
      }
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
      if (thread === undefined) {
        this.emit({
          id: parsed.id,
          error: { code: -32000, message: "thread not found" },
        });
        return;
      }
      this.emit({
        id: parsed.id,
        result: {
          ...fakeThreadStartResponse(
            thread,
            this.threadSettings.get(String(params.threadId)),
          ),
          initialTurnsPage: null,
        },
      });
      return;
    }
    if (parsed.method === "thread/settings/update") {
      if (this.settingsUpdateError !== undefined) {
        this.emit({ id: parsed.id, error: this.settingsUpdateError });
        return;
      }
      const threadId = String(params.threadId);
      const current = this.threadSettings.get(threadId) ?? {
        model: "gpt-5.4",
        modelProvider: "openai",
        effort: null,
      };
      const next = {
        ...current,
        ...(typeof params.model === "string" ? { model: params.model } : {}),
        ...(typeof params.effort === "string" ? { effort: params.effort } : {}),
      };
      this.threadSettings.set(threadId, next);
      this.emit({ id: parsed.id, result: {} });
      this.emit({
        method: "thread/settings/updated",
        params: {
          threadId,
          threadSettings: fakeNativeThreadSettings(next),
        },
      });
      return;
    }
    if (parsed.method === "thread/name/set") {
      if (this.nameSetError !== undefined) {
        this.emit({ id: parsed.id, error: this.nameSetError });
        return;
      }
      const thread = this.threads.find(
        (candidate) => candidate.id === params.threadId,
      );
      if (thread !== undefined) thread.name = params.name;
      this.emit({ id: parsed.id, result: {} });
      return;
    }
    if (parsed.method === "thread/compact/start") {
      this.emit({ id: parsed.id, result: {} });
      this.emit({
        method: "thread/compacted",
        params: { threadId: String(params.threadId), turnId: "compact-turn-1" },
      });
    }
  }

  async close(): Promise<void> {}

  disconnect(reason = "test app-server restart"): void {
    this.handlers?.onClose(reason);
  }

  emit(value: unknown): void {
    queueMicrotask(() => this.handlers?.onMessage(JSON.stringify(value)));
  }
}

class ProbeTimeoutCreationTransport extends FakeCreationTransport {
  override async send(message: string): Promise<void> {
    const parsed = JSON.parse(message) as Record<string, unknown>;
    if (parsed.method === "model/list") {
      this.sent.push(parsed);
      return;
    }
    await super.send(message);
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
  settings: {
    model: string;
    modelProvider: string;
    effort: string | null;
  } = { model: "gpt-5.4", modelProvider: "openai", effort: null },
): Record<string, unknown> {
  return {
    thread,
    model: settings.model,
    modelProvider: settings.modelProvider,
    serviceTier: null,
    cwd: thread.cwd,
    runtimeWorkspaceRoots: [],
    instructionSources: [],
    approvalPolicy: "never",
    approvalsReviewer: "user",
    sandbox: { type: "dangerFullAccess" },
    activePermissionProfile: null,
    reasoningEffort: settings.effort,
    multiAgentMode: "explicitRequestOnly",
  };
}

function fakeModel(
  id: string,
  efforts: readonly string[],
  defaultEffort: string,
  isDefault: boolean,
): Record<string, unknown> {
  return {
    id,
    model: id,
    upgrade: null,
    upgradeInfo: null,
    availabilityNux: null,
    displayName: id,
    description: `${id} test model`,
    hidden: false,
    supportedReasoningEfforts: efforts.map((reasoningEffort) => ({
      reasoningEffort,
      description: `${reasoningEffort} reasoning`,
    })),
    defaultReasoningEffort: defaultEffort,
    inputModalities: ["text"],
    supportsPersonality: true,
    additionalSpeedTiers: [],
    serviceTiers: [],
    defaultServiceTier: null,
    isDefault,
  };
}

function fakeNativeThreadSettings(settings: {
  model: string;
  modelProvider: string;
  effort: string | null;
}): Record<string, unknown> {
  return {
    cwd: "/home",
    approvalPolicy: "never",
    approvalsReviewer: "user",
    sandboxPolicy: { type: "dangerFullAccess" },
    activePermissionProfile: null,
    model: settings.model,
    modelProvider: settings.modelProvider,
    serviceTier: null,
    effort: settings.effort,
    summary: null,
    collaborationMode: {
      mode: "default",
      settings: {
        model: settings.model,
        reasoning_effort: settings.effort,
        developer_instructions: null,
      },
    },
    multiAgentMode: "explicitRequestOnly",
    personality: null,
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
    assert.equal(created.thread.modelProvider, "openai");
    assert.equal(created.thread.effectiveModel, "gpt-5.4");
    assert.equal(created.creation.session.profileId, fixture.profileId);
    assert.equal(created.creation.binding.nativeThreadId, "created-thread-1");
    assert.equal(created.creation.binding.label, "Browser Codex agent");
    assert.equal(created.thread.name, "Browser Codex agent");
    const startRequest = fixture.transport.sent.find(
      (message) => message.method === "thread/start",
    );
    assert.equal(
      (startRequest?.params as Record<string, unknown>).developerInstructions,
      "CREATION_PROFILE_SOUL_MARKER",
    );
    assert.equal(
      Object.hasOwn(
        startRequest?.params as Record<string, unknown>,
        "baseInstructions",
      ),
      false,
    );
    assert.equal(created.creation.binding.profileId, fixture.profileId);
    assert.equal(created.creation.binding.profileRevision, 1);
    assert.equal(typeof created.creation.binding.profilePromptHash, "string");
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
    assert.equal(persistedBinding?.label, "Browser Codex agent");

    const renamed = await fixture.controller.updateBindingMetadata({
      bindingId: created.creation.binding.bindingId,
      expectedRevision: created.creation.binding.revision,
      label: "Asha planning follow-up",
      taskRef: null,
    });
    assert.equal(renamed.label, "Asha planning follow-up");
    assert.equal(renamed.taskRef, null);
    const renamedThreads = await fixture.controller.listThreads(
      fixture.runtimeId,
      { limit: 10 },
    );
    assert.equal(renamedThreads.items[0]?.name, "Asha planning follow-up");

    const cleared = await fixture.controller.updateBindingMetadata({
      bindingId: renamed.bindingId,
      expectedRevision: renamed.revision,
      label: null,
      taskRef: null,
    });
    assert.equal(cleared.label, null);
    const clearedThreads = await fixture.controller.listThreads(
      fixture.runtimeId,
      { limit: 10 },
    );
    assert.equal(clearedThreads.items[0]?.name, null);
    await assert.rejects(
      fixture.controller.updateBindingMetadata({
        bindingId: cleared.bindingId,
        expectedRevision: renamed.revision,
        label: "stale rename",
        taskRef: null,
      }),
      /external_binding_metadata_revision_conflict/,
    );

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

    const beforeFailedRename = await fixture.bridge.getExternalBinding(
      created.creation.binding.bindingId,
    );
    assert.ok(beforeFailedRename);
    fixture.transport.nameSetError = {
      code: -32000,
      message: "native naming unavailable",
    };
    await assert.rejects(
      fixture.controller.updateBindingMetadata({
        bindingId: beforeFailedRename.bindingId,
        expectedRevision: beforeFailedRename.revision,
        label: "must roll back",
        taskRef: null,
      }),
      /external_binding_metadata_native_sync_failed/,
    );
    fixture.transport.nameSetError = undefined;
    const afterFailedRename = await fixture.bridge.getExternalBinding(
      beforeFailedRename.bindingId,
    );
    assert.equal(afterFailedRename?.label, beforeFailedRename.label);
    assert.deepEqual(afterFailedRename?.taskRef, beforeFailedRename.taskRef);

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

test("controller scopes Codex coordination tools by the bound reviewer profile", async () => {
  const fixture = await externalCreationFixture(
    false,
    undefined,
    undefined,
    false,
    "reviewer",
  );
  try {
    await fixture.controller.createAgentSession({
      idempotencyKey: "reviewer-dynamic-tools",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const startRequest = fixture.transport.sent.find(
      (message) => message.method === "thread/start",
    );
    const params = startRequest?.params as Record<string, unknown>;
    const namespaces = params.dynamicTools as Array<{
      type?: string;
      tools?: Array<{ name?: string }>;
    }>;
    const coordination = namespaces.find(
      (namespace) => namespace.type === "namespace",
    );
    assert.deepEqual(
      coordination?.tools?.map((tool) => tool.name),
      [
        "list_agents",
        "send_agent_message",
        "agent_round",
        "submit_task_for_review",
        "complete_routed_review",
      ],
    );
  } finally {
    await fixture.cleanup();
  }
});

test("reconnect replaces a native thread when its dynamic tool catalog is stale", async () => {
  const fixture = await externalCreationFixture(
    false,
    undefined,
    undefined,
    false,
    "reviewer",
  );
  let recoveredController: ServiceExternalRuntimeController | undefined;
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "dynamic-catalog-reconnect",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
      label: "dynamic catalog reconnect",
    });
    const before = created.creation.binding;
    assert.equal(typeof before.dynamicToolCatalogFingerprint, "string");
    const stale = await fixture.bridge.bindExternalAgent({
      binding: {
        ...before,
        dynamicToolCatalogFingerprint: null,
        updatedAt: new Date().toISOString(),
      },
      expectedRevision: before.revision,
    });
    await fixture.controller.stop();

    recoveredController = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "reconnect-test-controller",
      driverFactory: (_registration, authority) =>
        new CodexAppServerDriver(fixture.transport, authority, {
          requestTimeoutMs: 50,
        }),
    });
    await recoveredController.connect(fixture.runtimeId);

    const after = await fixture.bridge.getExternalBinding(stale.bindingId);
    assert.ok(after);
    assert.notEqual(after.nativeThreadId, before.nativeThreadId);
    assert.equal(
      after.dynamicToolCatalogFingerprint,
      before.dynamicToolCatalogFingerprint,
    );
    assert.equal(after.sessionId, before.sessionId);
    assert.equal(after.agentId, before.agentId);
    assert.equal(after.label, before.label);
    assert.ok(
      fixture.transport.archivedThreadIds.has(before.nativeThreadId as string),
    );

    const refreshStart = [...fixture.transport.sent]
      .reverse()
      .find(
        (message) =>
          message.method === "thread/start" &&
          String(
            (message.params as Record<string, unknown>).threadSource,
          ).startsWith("rusty-crew:dynamic-tools-refresh:"),
      );
    assert.ok(refreshStart);
    const dynamicTools = (refreshStart.params as Record<string, unknown>)
      .dynamicTools as Array<{
      type?: string;
      tools?: Array<{ name?: string }>;
    }>;
    const namespace = dynamicTools.find((entry) => entry.type === "namespace");
    assert.ok(
      namespace?.tools?.some((tool) => tool.name === "submit_task_for_review"),
    );
    assert.ok(
      namespace?.tools?.some((tool) => tool.name === "complete_routed_review"),
    );
    assert.equal(
      namespace?.tools?.some((tool) => tool.name === "reply_agent_message"),
      false,
    );
    const refreshEvents = await fixture.bridge.queryExternalRuntimeEvents({
      runtimeId: fixture.runtimeId,
      afterSequence: 0,
      limit: 100,
    });
    assert.ok(
      refreshEvents.some(
        (event) =>
          event.kind === "dynamic_tool_catalog_refreshed" &&
          typeof event.payload === "object" &&
          event.payload !== null &&
          (event.payload as Record<string, unknown>).previousNativeThreadId ===
            before.nativeThreadId &&
          (event.payload as Record<string, unknown>).nativeThreadId ===
            after.nativeThreadId,
      ),
    );

    const startCount = fixture.transport.sent.filter(
      (message) => message.method === "thread/start",
    ).length;
    await recoveredController.connect(fixture.runtimeId);
    assert.equal(
      fixture.transport.sent.filter(
        (message) => message.method === "thread/start",
      ).length,
      startCount,
    );
  } finally {
    await recoveredController?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

test("profile prompt refresh replaces the native thread and preserves Crew identity", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "profile-refresh-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const before = created.creation.binding;
    const current = await fixture.bridge.getProfileRegistryRecord(
      fixture.profileId,
    );
    assert.ok(current);
    const updated = await fixture.bridge.updateProfileRegistryRecord({
      write: {
        profileId: current.profileId,
        lifecycleStatus: current.lifecycleStatus,
        displayName: current.displayName,
        summary: current.summary,
        defaultSessionKind: current.defaultSessionKind,
        agentId: current.agentId,
        ownerId: current.ownerId,
        promptSoulMarkdown: "REFRESHED_PROFILE_SOUL_MARKER",
        promptMemoryMarkdown: current.promptMemoryMarkdown,
        activeRuntimeSettingsJson: current.activeRuntimeSettingsJson,
        sourceAssetRefs: current.sourceAssetRefs,
        derivedRuntimeRefs: current.derivedRuntimeRefs,
        importExport: current.importExport,
        now: new Date().toISOString(),
      },
      expectedRevision: current.revision,
    });

    const stale = await fixture.controller.profileInstructionStatus(
      fixture.profileId,
    );
    assert.equal(stale.bindings[0]?.state, "stale");
    const receipt = await fixture.controller.refreshBindingProfileInstructions({
      bindingId: before.bindingId,
      expectedBindingRevision: before.revision,
      expectedNativeThreadId: before.nativeThreadId as string,
      expectedProfileRevision: updated.revision,
      expectedProfilePromptHash: createHash("sha256")
        .update("REFRESHED_PROFILE_SOUL_MARKER")
        .digest("hex"),
    });
    assert.equal(receipt.outcome, "thread_replaced");
    const after = await fixture.bridge.getExternalBinding(before.bindingId);
    assert.ok(after);
    assert.equal(after.sessionId, before.sessionId);
    assert.equal(after.agentId, before.agentId);
    assert.notEqual(after.nativeThreadId, before.nativeThreadId);
    assert.equal(after.profileRevision, updated.revision);
    assert.equal(receipt.profileState.state, "current");
    const replacementRequest = fixture.transport.sent.filter(
      (message) => message.method === "thread/start",
    )[1];
    assert.ok(replacementRequest);
    assert.equal(
      (replacementRequest.params as Record<string, unknown>)
        .developerInstructions,
      "REFRESHED_PROFILE_SOUL_MARKER",
    );
    assert.equal(
      Object.hasOwn(
        replacementRequest.params as Record<string, unknown>,
        "baseInstructions",
      ),
      false,
    );
    const replacementCount = fixture.transport.sent.filter(
      (message) => message.method === "thread/start",
    ).length;
    await assert.rejects(
      fixture.controller.refreshBindingProfileInstructions({
        bindingId: before.bindingId,
        expectedBindingRevision: before.revision,
        expectedNativeThreadId: before.nativeThreadId as string,
        expectedProfileRevision: updated.revision,
        expectedProfilePromptHash: after.profilePromptHash as string,
      }),
      /external_binding_profile_refresh_revision_conflict/,
    );
    await assert.rejects(
      fixture.controller.refreshBindingProfileInstructions({
        bindingId: after.bindingId,
        expectedBindingRevision: after.revision,
        expectedNativeThreadId: after.nativeThreadId as string,
        expectedProfileRevision: updated.revision - 1,
        expectedProfilePromptHash: after.profilePromptHash as string,
      }),
      /external_binding_profile_refresh_profile_revision_conflict/,
    );
    assert.equal(
      fixture.transport.sent.filter(
        (message) => message.method === "thread/start",
      ).length,
      replacementCount,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("profile lifecycle-only revision repair preserves the native thread", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "profile-revision-repair",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const before = created.creation.binding;
    const current = await fixture.bridge.getProfileRegistryRecord(
      fixture.profileId,
    );
    assert.ok(current);
    const updated = await fixture.bridge.updateProfileRegistryRecord({
      write: {
        profileId: current.profileId,
        lifecycleStatus: current.lifecycleStatus,
        displayName: current.displayName,
        summary: "metadata-only profile edit",
        defaultSessionKind: current.defaultSessionKind,
        agentId: current.agentId,
        ownerId: current.ownerId,
        promptSoulMarkdown: current.promptSoulMarkdown,
        promptMemoryMarkdown: current.promptMemoryMarkdown,
        activeRuntimeSettingsJson: current.activeRuntimeSettingsJson,
        sourceAssetRefs: current.sourceAssetRefs,
        derivedRuntimeRefs: current.derivedRuntimeRefs,
        importExport: current.importExport,
        now: new Date().toISOString(),
      },
      expectedRevision: current.revision,
    });
    const sentBefore = fixture.transport.sent.length;
    const delivery = await fixture.bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "profile-revision-repair-delivery",
      idempotencyKey: "profile-revision-repair-delivery",
      messageId: "profile-revision-repair-message",
      toAddress: created.creation.session.agentId,
      inputKind: "operator",
      body: "continue after a profile metadata-only edit",
      requireWake: true,
      createdAt: new Date().toISOString(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(delivery.activation?.type, "external_turn_requested");
    await fixture.controller.tick();
    const after = await fixture.bridge.getExternalBinding(before.bindingId);
    assert.ok(after);
    assert.equal(after.nativeThreadId, before.nativeThreadId);
    assert.equal(after.profileRevision, updated.revision);
    assert.equal(
      fixture.transport.sent
        .slice(sentBefore)
        .some((message) => message.method === "thread/fork"),
      false,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("profile prompt drift keeps turns and restart hydration on the applied snapshot", async () => {
  const fixture = await externalCreationFixture(false);
  let reloaded: ServiceExternalRuntimeController | undefined;
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "profile-drift-fail-open",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const before = created.creation.binding;
    const profile = await fixture.bridge.getProfileRegistryRecord(
      fixture.profileId,
    );
    assert.ok(profile);
    await fixture.bridge.updateProfileRegistryRecord({
      write: {
        profileId: profile.profileId,
        lifecycleStatus: profile.lifecycleStatus,
        displayName: profile.displayName,
        summary: profile.summary,
        defaultSessionKind: profile.defaultSessionKind,
        agentId: profile.agentId,
        ownerId: profile.ownerId,
        promptSoulMarkdown: "DESIRED_BUT_NOT_APPLIED_PROFILE_SOUL",
        promptMemoryMarkdown: profile.promptMemoryMarkdown,
        activeRuntimeSettingsJson: profile.activeRuntimeSettingsJson,
        sourceAssetRefs: profile.sourceAssetRefs,
        derivedRuntimeRefs: profile.derivedRuntimeRefs,
        importExport: profile.importExport,
        now: new Date().toISOString(),
      },
      expectedRevision: profile.revision,
    });

    const status = await fixture.controller.profileInstructionStatus(
      fixture.profileId,
    );
    assert.equal(status.bindings[0]?.state, "stale");
    assert.equal(
      status.bindings[0]?.appliedPromptHash,
      before.profilePromptHash,
    );

    const delivery = await fixture.bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "profile-drift-fail-open-delivery",
      idempotencyKey: "profile-drift-fail-open-delivery",
      messageId: "profile-drift-fail-open-message",
      toAddress: created.creation.session.agentId,
      inputKind: "operator",
      body: "continue on the already-applied profile prompt",
      requireWake: true,
      createdAt: new Date().toISOString(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(delivery.activation?.type, "external_turn_requested");
    await fixture.controller.tick();
    const turnStart = fixture.transport.sent.find(
      (message) => message.method === "turn/start",
    );
    assert.ok(turnStart);
    const collaborationMode = (turnStart.params as Record<string, unknown>)
      .collaborationMode as { settings?: Record<string, unknown> };
    assert.equal(
      collaborationMode.settings?.developer_instructions,
      "CREATION_PROFILE_SOUL_MARKER",
    );
    const activeTurn = (await fixture.bridge.listActiveExternalTurns())[0];
    assert.ok(activeTurn?.nativeTurnId);
    fixture.transport.emit({
      method: "turn/completed",
      params: {
        threadId: before.nativeThreadId,
        turn: {
          id: activeTurn.nativeTurnId,
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
      async () => (await fixture.bridge.listActiveExternalTurns()).length === 0,
      "stale-profile native turn completion",
    );
    assert.equal(
      (await fixture.bridge.getExternalTurn(activeTurn.request.requestId))
        ?.phase,
      "completed",
    );
    const afterTurn = await fixture.bridge.getExternalBinding(before.bindingId);
    assert.equal(afterTurn?.nativeThreadId, before.nativeThreadId);
    assert.equal(afterTurn?.profilePromptHash, before.profilePromptHash);

    await fixture.controller.stop();
    reloaded = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "profile-drift-reload-controller",
      driverFactory: (_registration, authority) =>
        new CodexAppServerDriver(fixture.transport, authority, {
          requestTimeoutMs: 50,
        }),
    });
    const sentBeforeReload = fixture.transport.sent.length;
    await reloaded.connect(fixture.runtimeId);
    const resume = fixture.transport.sent
      .slice(sentBeforeReload)
      .find(
        (message) =>
          message.method === "thread/resume" &&
          (message.params as Record<string, unknown>).threadId ===
            before.nativeThreadId,
      );
    assert.ok(resume);
    assert.equal(
      (resume?.params as Record<string, unknown>).developerInstructions,
      "CREATION_PROFILE_SOUL_MARKER",
    );
    const afterReload = await fixture.bridge.getExternalBinding(
      before.bindingId,
    );
    assert.equal(afterReload?.nativeThreadId, before.nativeThreadId);
    assert.equal(afterReload?.profilePromptHash, before.profilePromptHash);
  } finally {
    await reloaded?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

test("archived binding restore resumes the exact native thread and Crew identity", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "archived-binding-restore",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const before = created.creation.binding;
    assert.ok(before.sessionId);
    assert.ok(before.agentId);
    assert.ok(before.profileId);
    assert.ok(before.nativeThreadId);
    await fixture.bridge.archiveSession(before.sessionId as SessionId);
    const archived = await fixture.bridge.getExternalBinding(before.bindingId);
    assert.ok(archived);
    assert.equal(archived.status, "archived");

    const profile = await fixture.bridge.getProfileRegistryRecord(
      fixture.profileId,
    );
    assert.ok(profile);
    await fixture.bridge.updateProfileRegistryRecord({
      write: {
        profileId: profile.profileId,
        lifecycleStatus: profile.lifecycleStatus,
        displayName: profile.displayName,
        summary: "reactivated without prompt change",
        defaultSessionKind: profile.defaultSessionKind,
        agentId: profile.agentId,
        ownerId: profile.ownerId,
        promptSoulMarkdown: profile.promptSoulMarkdown,
        promptMemoryMarkdown: profile.promptMemoryMarkdown,
        activeRuntimeSettingsJson: profile.activeRuntimeSettingsJson,
        sourceAssetRefs: profile.sourceAssetRefs,
        derivedRuntimeRefs: profile.derivedRuntimeRefs,
        importExport: profile.importExport,
        now: new Date().toISOString(),
      },
      expectedRevision: profile.revision,
    });
    const sentBefore = fixture.transport.sent.length;
    const receipt = await fixture.controller.restoreBinding({
      bindingId: archived.bindingId,
      expectedBindingRevision: archived.revision,
      expectedSessionId: before.sessionId,
      expectedAgentId: before.agentId,
      expectedProfileId: before.profileId,
      expectedNativeThreadId: before.nativeThreadId,
    });
    assert.equal(receipt.outcome, "restored");
    assert.equal(receipt.binding.bindingId, before.bindingId);
    assert.equal(receipt.binding.nativeThreadId, before.nativeThreadId);
    assert.equal(receipt.session.sessionId, before.sessionId);
    assert.equal(receipt.profileRevisionUpdated, true);
    assert.ok(
      fixture.transport.sent
        .slice(sentBefore)
        .some(
          (message) =>
            message.method === "thread/resume" &&
            (message.params as Record<string, unknown>).threadId ===
              before.nativeThreadId,
        ),
    );
    assert.equal(
      fixture.transport.sent
        .slice(sentBefore)
        .some(
          (message) =>
            message.method === "thread/start" ||
            message.method === "thread/fork",
        ),
      false,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("external commands use native catalogs and settings without creating turns", async () => {
  const fixture = await externalCreationFixture(
    false,
    undefined,
    undefined,
    true,
  );
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "command-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const bindingId = created.creation.binding.bindingId;
    const threadId = created.thread.threadId;

    const catalog = await fixture.controller.commandCatalog(bindingId);
    assert.deepEqual(
      catalog.models.map((model) => model.id),
      ["gpt-5.4", "gpt-5.4-mini"],
    );
    assert.deepEqual(
      catalog.models[1]?.supportedEfforts.map((effort) => effort.value),
      ["low", "medium"],
    );
    assert.ok(
      catalog.commands.some(
        (command) =>
          command.name === "new" && command.aliases.includes("restart"),
      ),
    );

    fixture.transport.emit({
      method: "thread/tokenUsage/updated",
      params: {
        threadId,
        turnId: "usage-turn-1",
        tokenUsage: {
          total: {
            totalTokens: 10_000,
            inputTokens: 9_000,
            cachedInputTokens: 1_000,
            outputTokens: 1_000,
            reasoningOutputTokens: 400,
          },
          last: {
            totalTokens: 1_000,
            inputTokens: 900,
            cachedInputTokens: 100,
            outputTokens: 100,
            reasoningOutputTokens: 40,
          },
          modelContextWindow: 200_000,
        },
      },
    });
    await waitUntil(
      async () =>
        (
          await fixture.bridge.queryExternalRuntimeEvents({
            runtimeId: fixture.runtimeId,
            afterSequence: 0,
            limit: 1_000,
          })
        ).some((event) => event.kind === "usage"),
      "usage event",
    );

    const status = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/status",
      idempotencyKey: "command-status",
    });
    assert.equal(status.status, "applied");
    assert.equal(status.result.status?.usage?.contextWindowUsedPercent, 5);
    assert.equal(
      fixture.transport.sent.some((message) => message.method === "turn/start"),
      false,
    );
    const replay = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/status",
      idempotencyKey: "command-status",
    });
    assert.equal(replay.commandId, status.commandId);

    const model = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/model gpt-5.4-mini",
      idempotencyKey: "command-model-mini",
    });
    assert.equal(model.status, "applied");
    assert.equal(model.result.settings?.model, "gpt-5.4-mini");
    assert.equal(model.result.settings?.effort, "low");

    const effort = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/effort medium",
      idempotencyKey: "command-effort-medium",
    });
    assert.equal(effort.status, "applied");
    assert.equal(effort.result.settings?.effort, "medium");

    const invalid = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/effort high",
      idempotencyKey: "command-effort-invalid",
    });
    assert.equal(invalid.status, "rejected");
    assert.equal(invalid.reasonCode, "external_command_effort_invalid");

    fixture.transport.settingsUpdateError = {
      code: -32601,
      message: "method not found: thread/settings/update",
    };
    const unavailable = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/model gpt-5.4",
      idempotencyKey: "command-model-unavailable",
    });
    assert.equal(unavailable.status, "rejected");
    assert.equal(
      unavailable.reasonCode,
      "external_command_capability_unavailable",
    );
    fixture.transport.settingsUpdateError = undefined;

    const compact = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/compact",
      idempotencyKey: "command-compact",
    });
    assert.equal(compact.status, "applied");
    assert.equal(
      fixture.transport.sent.some(
        (message) => message.method === "thread/start",
      ),
      true,
      "initial session creation should be the only thread start before /new",
    );
    const bindingBeforeRestart =
      await fixture.bridge.getExternalBinding(bindingId);
    assert.ok(bindingBeforeRestart);
    const restart = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/restart",
      idempotencyKey: "command-restart",
    });
    assert.equal(restart.status, "applied", JSON.stringify(restart));
    assert.equal(restart.command, "restart");
    const replacement = restart.result.threadReplacement;
    assert.ok(replacement);
    assert.equal(replacement.bindingId, bindingId);
    assert.equal(replacement.previousNativeThreadId, threadId);
    assert.notEqual(replacement.nativeThreadId, threadId);
    assert.equal(replacement.previousNativeThreadArchived, true);
    assert.equal(replacement.settingsPreserved, true);
    assert.equal(replacement.settings.model, "gpt-5.4-mini");
    assert.equal(replacement.settings.effort, "medium");
    assert.equal(replacement.cwd, bindingBeforeRestart.cwd);
    assert.equal(replacement.label, bindingBeforeRestart.label ?? null);
    assert.equal(replacement.profileId, fixture.profileId);
    assert.deepEqual(replacement.taskRef, bindingBeforeRestart.taskRef ?? null);
    const bindingAfterRestart =
      await fixture.bridge.getExternalBinding(bindingId);
    assert.ok(bindingAfterRestart);
    assert.equal(bindingAfterRestart.sessionId, bindingBeforeRestart.sessionId);
    assert.equal(bindingAfterRestart.profileId, bindingBeforeRestart.profileId);
    assert.equal(bindingAfterRestart.cwd, bindingBeforeRestart.cwd);
    assert.equal(bindingAfterRestart.label, bindingBeforeRestart.label);
    assert.deepEqual(bindingAfterRestart.taskRef, bindingBeforeRestart.taskRef);
    assert.equal(
      bindingAfterRestart.nativeThreadId,
      replacement.nativeThreadId,
    );
    assert.ok(fixture.transport.archivedThreadIds.has(threadId));
    const replacementNativeThread = fixture.transport.threads.find(
      (thread) => thread.id === replacement.nativeThreadId,
    );
    assert.deepEqual(replacementNativeThread?.turns, []);
    const replacementStart = fixture.transport.sent
      .filter((message) => message.method === "thread/start")
      .at(-1);
    assert.equal(
      (replacementStart?.params as Record<string, unknown>).cwd,
      bindingBeforeRestart.cwd,
    );
    assert.equal(
      (replacementStart?.params as Record<string, unknown>).model,
      "gpt-5.4-mini",
    );
    assert.deepEqual(
      (replacementStart?.params as Record<string, unknown>).config,
      { model_reasoning_effort: "medium" },
    );
    assert.equal(
      (replacementStart?.params as Record<string, unknown>)
        .developerInstructions,
      "CREATION_PROFILE_SOUL_MARKER",
    );

    const recoveryIdempotencyKey = "command-restart-after-rebind-crash";
    const recoveryControlId = `external-command:${createHash("sha256")
      .update(`${bindingId}\0${recoveryIdempotencyKey}`)
      .digest("hex")
      .slice(0, 32)}`;
    const recoveryThreadId = "created-thread-recovered-after-rebind";
    fixture.transport.threads.push(
      fakeCreationThread(
        recoveryThreadId,
        bindingAfterRestart.cwd ?? fixture.dataDir,
        `rusty-crew:command:${recoveryControlId}:replace:${replacement.nativeThreadId}`,
      ),
    );
    fixture.transport.threadSettings.set(recoveryThreadId, {
      model: "gpt-5.4-mini",
      modelProvider: "openai",
      effort: "medium",
    });
    await fixture.bridge.bindExternalAgent({
      binding: {
        ...bindingAfterRestart,
        nativeThreadId: recoveryThreadId,
        updatedAt: new Date().toISOString(),
      },
      expectedRevision: bindingAfterRestart.revision,
    });
    const recoveredRestart = await fixture.controller.executeCommand({
      bindingId,
      commandInput: "/new",
      idempotencyKey: recoveryIdempotencyKey,
    });
    assert.equal(
      recoveredRestart.status,
      "applied",
      JSON.stringify(recoveredRestart),
    );
    assert.equal(
      recoveredRestart.result.threadReplacement?.previousNativeThreadId,
      replacement.nativeThreadId,
    );
    assert.equal(
      recoveredRestart.result.threadReplacement?.nativeThreadId,
      recoveryThreadId,
    );
    assert.ok(
      fixture.transport.archivedThreadIds.has(replacement.nativeThreadId),
    );
    assert.equal(
      fixture.transport.archivedThreadIds.has(recoveryThreadId),
      false,
      "crash recovery archived the already rebound current thread",
    );
    assert.equal(
      (await fixture.bridge.getExternalBinding(bindingId))?.nativeThreadId,
      recoveryThreadId,
    );
    await waitUntil(async () => {
      const events = await fixture.bridge.queryExternalRuntimeEvents({
        runtimeId: fixture.runtimeId,
        afterSequence: 0,
        limit: 1_000,
      });
      return (
        events.some((event) => event.kind === "command_started") &&
        events.some((event) => event.kind === "command_completed") &&
        events.some((event) => event.kind === "command_failed") &&
        events.some((event) => event.kind === "thread_lifecycle") &&
        events.some((event) => event.kind === "compaction")
      );
    }, "command lifecycle events");
    assert.equal(
      fixture.transport.sent.some((message) => message.method === "turn/start"),
      false,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("thread list and read project the authoritative next-effective model", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "effective-model-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const threadId = created.thread.threadId;

    const initialList = await fixture.controller.listThreads(
      fixture.runtimeId,
      { limit: 50, archived: false },
    );
    assert.equal(initialList.items[0]?.modelProvider, "openai");
    assert.equal(initialList.items[0]?.effectiveModel, "gpt-5.4");
    assert.equal(
      (
        await fixture.controller.readThread(fixture.runtimeId, {
          threadId,
          includeTurns: false,
        })
      ).thread.effectiveModel,
      "gpt-5.4",
    );

    fixture.transport.threadSettings.set(threadId, {
      model: "gpt-5.4-mini",
      modelProvider: "openai",
      effort: "medium",
    });
    fixture.transport.emit({
      method: "thread/settings/updated",
      params: {
        threadId,
        threadSettings: fakeNativeThreadSettings({
          model: "gpt-5.4-mini",
          modelProvider: "openai",
          effort: "medium",
        }),
      },
    });
    await waitUntil(
      async () =>
        (
          await fixture.controller.readThread(fixture.runtimeId, {
            threadId,
            includeTurns: false,
          })
        ).thread.effectiveModel === "gpt-5.4-mini",
      "sticky per-turn model settings projection",
    );
    assert.equal(
      (
        await fixture.controller.listThreads(fixture.runtimeId, {
          limit: 50,
          archived: false,
        })
      ).items[0]?.effectiveModel,
      "gpt-5.4-mini",
    );

    const nativeThread = fixture.transport.threads.find(
      (thread) => thread.id === threadId,
    );
    assert.ok(nativeThread);
    nativeThread.status = { type: "notLoaded" };
    assert.equal(
      (
        await fixture.controller.readThread(fixture.runtimeId, {
          threadId,
          includeTurns: false,
        })
      ).thread.effectiveModel,
      null,
    );
    assert.equal(
      (
        await fixture.controller.listThreads(fixture.runtimeId, {
          limit: 50,
          archived: false,
        })
      ).items[0]?.effectiveModel,
      null,
    );

    nativeThread.status = { type: "idle" };
    await fixture.controller.archiveThread(fixture.runtimeId, threadId);
    const archivedList = await fixture.controller.listThreads(
      fixture.runtimeId,
      { limit: 50, archived: true },
    );
    assert.equal(archivedList.items[0]?.effectiveModel, null);
    assert.equal(
      (
        await fixture.controller.readThread(fixture.runtimeId, {
          threadId,
          includeTurns: false,
        })
      ).thread.effectiveModel,
      null,
    );
  } finally {
    await fixture.cleanup();
  }
});

test("thread read bounds direct native Codex error diagnostics", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "bounded-native-error-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const nativeThread = fixture.transport.threads.find(
      (thread) => thread.id === created.thread.threadId,
    );
    assert.ok(nativeThread);
    nativeThread.turns = [
      {
        id: "native-turn-with-error",
        items: [],
        itemsView: "full",
        status: "failed",
        error: {
          message: `native message\u0000${"m".repeat(8_000)}`,
          codexErrorInfo: { responseStreamDisconnected: {} },
          additionalDetails: `native details\u0007${"d".repeat(16_000)}`,
        },
        startedAt: 1,
        completedAt: 2,
        durationMs: 1_000,
      },
    ];

    const read = await fixture.controller.readThread(fixture.runtimeId, {
      threadId: created.thread.threadId,
      includeTurns: true,
    });
    const error = read.thread.turns[0]?.error;
    assert.ok(error);
    assert.equal(error.message.length, CODEX_ERROR_DIAGNOSTIC_LIMITS.message);
    assert.equal(error.code, "responseStreamDisconnected");
    assert.equal(
      error.additionalDetails?.length,
      CODEX_ERROR_DIAGNOSTIC_LIMITS.additionalDetails,
    );
    assert.equal(error.message.includes("\u0000"), false);
    assert.equal(error.additionalDetails?.includes("\u0007"), false);
    assert.match(error.message, /\.\.\.\[truncated\]$/);
    assert.match(error.additionalDetails ?? "", /\.\.\.\[truncated\]$/);
  } finally {
    await fixture.cleanup();
  }
});

test("failed native turns retain diagnostics after raw cache eviction and controller restart", async () => {
  const fixture = await externalCreationFixture(false);
  let reloaded: ServiceExternalRuntimeController | undefined;
  try {
    const created = await fixture.controller.createAgentSession({
      idempotencyKey: "durable-turn-error-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const delivery = await fixture.bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "durable-turn-error-delivery",
      idempotencyKey: "durable-turn-error-delivery",
      messageId: "durable-turn-error-message",
      toAddress: created.creation.session.agentId,
      inputKind: "operator",
      body: "exercise durable external failure projection",
      requireWake: true,
      createdAt: new Date().toISOString(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(delivery.activation?.type, "external_turn_requested");
    await fixture.controller.tick();
    await waitUntil(
      async () =>
        fixture.transport.sent.some(
          (message) => message.method === "turn/start",
        ),
      "native turn start",
    );
    const active = (await fixture.bridge.listActiveExternalTurns())[0];
    assert.equal(active?.nativeTurnId, "native-turn-1");
    const nativeThread = fixture.transport.threads.find(
      (thread) => thread.id === created.thread.threadId,
    );
    assert.ok(nativeThread);
    const nativeTurn = (
      nativeThread.turns as Array<Record<string, unknown>>
    )[0];
    assert.ok(nativeTurn);
    nativeTurn.status = "completed";
    nativeTurn.completedAt = 2;
    nativeTurn.durationMs = 1_000;

    fixture.transport.emit({
      method: "error",
      params: {
        threadId: created.thread.threadId,
        turnId: "native-turn-1",
        error: {
          message: "temporary stream interruption",
          codexErrorInfo: {
            responseStreamConnectionFailed: { httpStatusCode: 503 },
          },
          additionalDetails: null,
        },
        willRetry: true,
      },
    });
    await waitUntil(async () => {
      const events = await fixture.bridge.queryExternalRuntimeEvents({
        runtimeId: fixture.runtimeId,
        afterSequence: 0,
        limit: 1_000,
      });
      return events.some(
        (event) => nativeErrorPayload(event.payload)?.error.willRetry === true,
      );
    }, "retrying native error");
    assert.equal(
      (await fixture.bridge.listActiveExternalTurns())[0]?.phase,
      "active",
    );

    fixture.transport.emit({
      method: "error",
      params: {
        threadId: created.thread.threadId,
        turnId: "native-turn-1",
        error: {
          message: "response stream disconnected before final answer",
          codexErrorInfo: {
            responseStreamDisconnected: { httpStatusCode: 502 },
          },
          additionalDetails: "upstream stream closed",
        },
        willRetry: false,
      },
    });
    fixture.transport.emit({
      method: "turn/completed",
      params: {
        threadId: created.thread.threadId,
        turn: {
          ...nativeTurn,
          status: "failed",
          error: {
            message: "response stream disconnected before final answer",
            codexErrorInfo: {
              responseStreamDisconnected: { httpStatusCode: 502 },
            },
            additionalDetails: "upstream stream closed",
          },
        },
      },
    });
    await waitUntil(
      async () => (await fixture.bridge.listActiveExternalTurns()).length === 0,
      "durable failed external turn",
    );
    const failedTurn = await fixture.bridge.getExternalTurn(
      active?.request.requestId ?? "missing-request",
    );
    assert.deepEqual(failedTurn?.terminalError, {
      message: "response stream disconnected before final answer",
      code: "responseStreamDisconnected",
      additionalDetails: "upstream stream closed",
      willRetry: false,
    });

    for (let index = 0; index < 300; index += 1) {
      fixture.transport.emit({
        method: "warning",
        params: { message: `cache churn ${index}` },
      });
    }
    await waitUntil(async () => {
      const events = await fixture.bridge.queryExternalRuntimeEvents({
        runtimeId: fixture.runtimeId,
        afterSequence: 0,
        limit: 1_000,
      });
      return events.length >= 302;
    }, "raw detail cache churn persistence");
    const durableEvents = await fixture.bridge.queryExternalRuntimeEvents({
      runtimeId: fixture.runtimeId,
      afterSequence: 0,
      limit: 1_000,
    });
    const failureEvent = durableEvents.find(
      (event) => nativeErrorPayload(event.payload)?.error.willRetry === false,
    );
    assert.ok(failureEvent);
    assert.deepEqual(nativeErrorPayload(failureEvent.payload)?.error, {
      message: "response stream disconnected before final answer",
      code: "responseStreamDisconnected",
      additionalDetails: "upstream stream closed",
      willRetry: false,
    });
    assert.equal(
      fixture.controller.rawDetail(
        fixture.runtimeId,
        failureEvent.rawDetailRef ?? "missing-detail",
      ),
      undefined,
    );

    await fixture.controller.stop();
    reloaded = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "durable-turn-error-reloaded-controller",
      driverFactory: (_registration, authority) =>
        new CodexAppServerDriver(fixture.transport, authority, {
          requestTimeoutMs: 50,
        }),
    });
    await reloaded.connect(fixture.runtimeId);
    const read = await reloaded.readThread(fixture.runtimeId, {
      threadId: created.thread.threadId,
      includeTurns: true,
    });
    assert.equal(read.thread.turns[0]?.status, "failed");
    assert.equal(read.thread.turns[0]?.statusSource, "crew_terminal");
    assert.equal(read.thread.turns[0]?.terminalReasonCode, "codex_failed");
    assert.deepEqual(read.thread.turns[0]?.error, {
      message: "response stream disconnected before final answer",
      code: "responseStreamDisconnected",
      additionalDetails: "upstream stream closed",
      willRetry: false,
    });
  } finally {
    await reloaded?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

for (const nativeStatus of ["completed", "interrupted"] as const) {
  test(`controller reconciles a stale Crew turn from native ${nativeStatus} state after reconnect`, async () => {
    const fixture = await externalCreationFixture(false);
    let reloaded: ServiceExternalRuntimeController | undefined;
    try {
      const created = await fixture.controller.createAgentSession({
        idempotencyKey: `reconcile-${nativeStatus}-session`,
        runtimeId: fixture.runtimeId,
        profileId: fixture.profileId,
        cwd: fixture.dataDir,
        requestedAt: new Date().toISOString(),
      });
      const firstDelivery = await fixture.bridge.deliverAgentMessage({
        caller: { type: "system", senderAgentId: "operator" },
        deliveryId: `reconcile-${nativeStatus}-delivery-1`,
        idempotencyKey: `reconcile-${nativeStatus}-delivery-1`,
        messageId: `reconcile-${nativeStatus}-message-1`,
        toAddress: created.creation.session.agentId,
        inputKind: "operator",
        body: "start a turn that will finish while Crew is disconnected",
        requireWake: true,
        createdAt: new Date().toISOString(),
        expiresAt: new Date(Date.now() + 60_000).toISOString(),
      });
      assert.equal(firstDelivery.activation?.type, "external_turn_requested");
      await fixture.controller.tick();
      await waitUntil(
        async () =>
          (await fixture.bridge.listActiveExternalTurns()).some(
            (turn) => turn.nativeTurnId === "native-turn-1",
          ),
        "first native turn activation",
      );
      const active = (await fixture.bridge.listActiveExternalTurns())[0];
      assert.ok(active);
      const nativeThread = fixture.transport.threads.find(
        (thread) => thread.id === created.thread.threadId,
      );
      assert.ok(nativeThread);
      const nativeTurn = (
        nativeThread.turns as Array<Record<string, unknown>>
      )[0];
      assert.ok(nativeTurn);

      await fixture.controller.stop();
      nativeTurn.status = nativeStatus;
      nativeTurn.completedAt = 2;
      nativeTurn.durationMs = 1_000;

      reloaded = new ServiceExternalRuntimeController({
        bridge: fixture.bridge,
        instanceId: `reconcile-${nativeStatus}-controller`,
        driverFactory: (_registration, authority) =>
          new CodexAppServerDriver(fixture.transport, authority, {
            requestTimeoutMs: 50,
          }),
      });
      await reloaded.connect(fixture.runtimeId);

      assert.equal((await fixture.bridge.listActiveExternalTurns()).length, 0);
      const reconciled = await fixture.bridge.getExternalTurn(
        active.request.requestId,
      );
      assert.equal(reconciled?.phase, nativeStatus);
      assert.equal(
        reconciled?.terminalReasonCode,
        nativeStatus === "completed" ? null : "codex_interrupted",
      );

      const nextDelivery = await fixture.bridge.deliverAgentMessage({
        caller: { type: "system", senderAgentId: "operator" },
        deliveryId: `reconcile-${nativeStatus}-delivery-2`,
        idempotencyKey: `reconcile-${nativeStatus}-delivery-2`,
        messageId: `reconcile-${nativeStatus}-message-2`,
        toAddress: created.creation.session.agentId,
        inputKind: "operator",
        body: "start a fresh turn after reconnect",
        requireWake: true,
        createdAt: new Date().toISOString(),
        expiresAt: new Date(Date.now() + 60_000).toISOString(),
      });
      assert.equal(nextDelivery.activation?.type, "external_turn_requested");
      await reloaded.tick();
      await waitUntil(
        async () =>
          fixture.transport.sent.filter(
            (message) => message.method === "turn/start",
          ).length === 2,
        "fresh native turn after reconnect",
      );
    } finally {
      await reloaded?.stop().catch(() => undefined);
      await fixture.cleanup();
    }
  });
}

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

test("controller does not report a missing root deleted while descendants remain", async () => {
  const fixture = await externalCreationFixture(false);
  try {
    const root = await fixture.controller.createAgentSession({
      idempotencyKey: "missing-delete-root-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const child = await fixture.controller.createAgentSession({
      idempotencyKey: "surviving-delete-child-session",
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

    const rootIndex = fixture.transport.threads.findIndex(
      (thread) => thread.id === rootThreadId,
    );
    assert.notEqual(rootIndex, -1);
    fixture.transport.threads.splice(rootIndex, 1);

    childThread.status = { type: "active", activeFlags: [] };
    await assert.rejects(
      fixture.controller.deleteThread(fixture.runtimeId, rootThreadId),
      (error: unknown) =>
        error instanceof ExternalThreadLifecycleError &&
        error.reasonCode === "external_thread_active",
    );
    assert.equal(
      (
        await fixture.bridge.getExternalBinding(
          child.creation.binding.bindingId,
        )
      )?.status,
      "active",
    );

    childThread.status = { type: "idle" };
    await assert.rejects(
      fixture.controller.deleteThread(fixture.runtimeId, rootThreadId),
      (error: unknown) =>
        error instanceof ExternalThreadLifecycleError &&
        error.reasonCode === "external_thread_native_delete_failed",
    );
    assert.equal(
      fixture.transport.threads.some((thread) => thread.id === childThreadId),
      true,
    );
    assert.equal(
      (
        await fixture.bridge.getExternalBinding(
          child.creation.binding.bindingId,
        )
      )?.status,
      "active",
    );
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
    const reloadedStatus = reloaded.statuses()[0];
    assert.equal(reloadedStatus?.compatibilityState, "compatible_uncertified");
    assert.equal(reloadedStatus?.lastCompatibilityProbe?.outcome, "passed");
    assert.equal(
      (await fixture.bridge.getExternalRuntime(fixture.runtimeId))
        ?.lastCompatibilityProbe?.outcome,
      "passed",
    );
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

test("controller reconnect is single-flight and resumes bindings after an app-server bounce", async () => {
  const fixture = await externalCreationFixture(false);
  let controller: ServiceExternalRuntimeController | undefined;
  try {
    await fixture.controller.stop();
    const transports: FakeCreationTransport[] = [];
    controller = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "single-flight-recovery-controller",
      recoveryBaseDelayMs: 10,
      recoveryMaxDelayMs: 100,
      driverFactory: (_registration, authority) => {
        const transport = new FakeCreationTransport();
        transport.modelListNotificationMessage = `connection-${transports.length + 1}`;
        const previous = transports.at(-1);
        if (previous !== undefined) {
          transport.threads.push(...previous.threads);
          for (const [threadId, settings] of previous.threadSettings) {
            transport.threadSettings.set(threadId, settings);
          }
        }
        transports.push(transport);
        return new CodexAppServerDriver(transport, authority, {
          requestTimeoutMs: 50,
        });
      },
    });
    await controller.connect(fixture.runtimeId);
    const created = await controller.createAgentSession({
      idempotencyKey: "single-flight-recovery-session",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const nativeThreadId = created.creation.binding.nativeThreadId;
    assert.equal(typeof nativeThreadId, "string");

    transports[0]?.disconnect("app-server process replaced");
    await waitUntil(
      async () =>
        (await fixture.bridge.getExternalRuntime(fixture.runtimeId))
          ?.observedState === "disconnected",
      "runtime disconnect observation",
    );

    await Promise.all([
      controller.tick(),
      controller.connect(fixture.runtimeId),
      controller.connect(fixture.runtimeId),
    ]);

    assert.equal(transports.length, 2);
    const status = controller.statuses()[0];
    assert.equal(status?.driverState, "ready");
    assert.equal(status?.recovery.phase, "succeeded");
    assert.equal(status?.recovery.totalAttempts, 1);
    assert.equal(status?.recovery.consecutiveFailures, 0);
    assert.equal(status?.recovery.nextAttemptAt, null);
    assert.equal(typeof status?.recovery.lastRecoveredAt, "string");
    assert.deepEqual(status?.bindingResumeFailures, []);
    const connectionEvents = (
      await fixture.bridge.queryExternalRuntimeEvents({
        runtimeId: fixture.runtimeId,
        afterSequence: 0,
        limit: 100,
      })
    ).filter((event) => event.kind === "unknown_native_notification");
    assert.deepEqual(
      connectionEvents.map((event) => event.payload),
      [
        {
          nativeMethod: "future/runtime-status",
          text: "connection-1",
        },
        {
          nativeMethod: "future/runtime-status",
          text: "connection-2",
        },
      ],
    );
    assert.notEqual(connectionEvents[0]?.eventId, connectionEvents[1]?.eventId);
    assert.ok(
      transports[1]?.sent.some(
        (message) =>
          message.method === "thread/resume" &&
          (message.params as Record<string, unknown>).threadId ===
            nativeThreadId,
      ),
    );

    transports[0]?.handlers?.onError(
      new Error("late error from superseded socket"),
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.equal(
      (await fixture.bridge.getExternalRuntime(fixture.runtimeId))
        ?.observedState,
      "ready",
    );
  } finally {
    await controller?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

test("automatic reconnect failures back off before retrying", async () => {
  const fixture = await externalCreationFixture(false);
  let controller: ServiceExternalRuntimeController | undefined;
  try {
    await fixture.controller.stop();
    let nowMs = Date.now() + 1_000;
    const firstRetryAt = new Date(nowMs + 100).toISOString();
    const transports: FakeCreationTransport[] = [];
    controller = new ServiceExternalRuntimeController({
      bridge: fixture.bridge,
      instanceId: "backoff-recovery-controller",
      now: () => new Date(nowMs),
      recoveryBaseDelayMs: 100,
      recoveryMaxDelayMs: 400,
      driverFactory: (_registration, authority) => {
        const transport =
          transports.length === 1
            ? new ProbeTimeoutCreationTransport()
            : new FakeCreationTransport();
        transports.push(transport);
        return new CodexAppServerDriver(transport, authority, {
          requestTimeoutMs: 20,
          compatibilityProbeTimeoutMs: 20,
        });
      },
    });
    await controller.connect(fixture.runtimeId);
    transports[0]?.disconnect("app-server process replaced");
    await waitUntil(
      async () => controller?.statuses()[0]?.driverState === "disconnected",
      "driver disconnect",
    );

    await controller.tick();
    assert.equal(transports.length, 2);
    const failed = controller.statuses()[0]?.recovery;
    assert.equal(failed?.phase, "failed");
    assert.equal(failed?.totalAttempts, 1);
    assert.equal(failed?.consecutiveFailures, 1);
    assert.equal(failed?.nextAttemptAt, firstRetryAt);

    transports[1]?.disconnect("late close after failed probe");
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.equal(controller.statuses()[0]?.recovery.phase, "failed");
    assert.equal(
      controller.statuses()[0]?.recovery.nextAttemptAt,
      firstRetryAt,
    );

    await controller.tick();
    assert.equal(transports.length, 2);

    nowMs += 100;
    await controller.tick();
    assert.equal(transports.length, 3);
    const recovered = controller.statuses()[0]?.recovery;
    assert.equal(recovered?.phase, "succeeded");
    assert.equal(recovered?.totalAttempts, 2);
    assert.equal(recovered?.consecutiveFailures, 0);
    assert.equal(recovered?.nextAttemptAt, null);
  } finally {
    await controller?.stop().catch(() => undefined);
    await fixture.cleanup();
  }
});

test("controller repairs historical profileless binding reads before startup resume", async () => {
  const fixture = await externalCreationFixture(false);
  let recoveryController: ServiceExternalRuntimeController | undefined;
  try {
    const original = await fixture.controller.createAgentSession({
      idempotencyKey: "profileless-startup-repair-original",
      runtimeId: fixture.runtimeId,
      profileId: fixture.profileId,
      cwd: fixture.dataDir,
      requestedAt: new Date().toISOString(),
    });
    const nativeThreadId = original.creation.binding.nativeThreadId;
    assert.equal(typeof nativeThreadId, "string");
    const originalRevision = original.creation.binding.revision;
    await fixture.controller.stop();

    const recoveryBridge = new Proxy(fixture.bridge, {
      get(target, property, receiver) {
        if (property === "listExternalBindings") {
          return async () =>
            (await target.listExternalBindings()).map((binding) =>
              withoutProfileProvenance(binding),
            );
        }
        return Reflect.get(target, property, receiver) as unknown;
      },
    }) as NativeBridgeModule;
    recoveryController = new ServiceExternalRuntimeController({
      bridge: recoveryBridge,
      instanceId: "profileless-startup-repair-controller",
      driverFactory: (_registration, authority) =>
        new CodexAppServerDriver(fixture.transport, authority, {
          requestTimeoutMs: 50,
        }),
    });

    const recovered = await recoveryController.connect(fixture.runtimeId);
    assert.deepEqual(recovered.bindingResumeFailures, []);
    const repaired = await fixture.bridge.getExternalBinding(
      original.creation.binding.bindingId,
    );
    assert.ok(repaired);
    assert.equal(repaired.profileId, fixture.profileId);
    assert.equal(repaired.profileRevision, 1);
    assert.equal(typeof repaired.profilePromptHash, "string");
    assert.ok(repaired.revision > originalRevision);
    const resume = fixture.transport.sent
      .filter((message) => message.method === "thread/resume")
      .at(-1);
    assert.equal(
      (resume?.params as Record<string, unknown>).threadId,
      nativeThreadId,
    );
    assert.equal(
      (resume?.params as Record<string, unknown>).developerInstructions,
      "CREATION_PROFILE_SOUL_MARKER",
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
    await bridge.createProfileRegistryRecord({
      profileId: "interaction-profile",
      lifecycleStatus: "active",
      displayName: "Interaction profile",
      defaultSessionKind: "full",
      agentId: "interaction-agent",
      activeRuntimeSettingsJson: {},
      sourceAssetRefs: [],
      derivedRuntimeRefs: [],
      importExport: { metadataJson: {} },
      now: now(),
    });
    await bridge.registerExternalRuntime({
      registration: {
        runtimeId: "interaction-runtime",
        kind: "codex_app_server",
        endpoint: { transport: "unix_web_socket", address: "/tmp/fake.sock" },
        processOwnership: "attached",
        observedCliVersion: null,
        consumedContractRevision: null,
        compatibilityState: "unassessed",
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
        profileId: "interaction-profile",
        profileRevision: 1,
        profilePromptHash:
          "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        messageDeliveryPolicy: "immediate_steer",
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
    const connectedRuntime = await bridge.getExternalRuntime(
      "interaction-runtime",
    );
    assert.equal(
      connectedRuntime?.compatibilityState,
      "compatible_uncertified",
    );
    assert.equal(
      connectedRuntime?.observedCliVersion,
      CODEX_APP_SERVER_PROTOCOL.cliVersion,
    );
    assert.equal(
      connectedRuntime?.consumedContractRevision,
      CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
    );
    const planDelivery = await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "interaction-delivery",
      idempotencyKey: "interaction-delivery",
      messageId: "interaction-message",
      toAddress: "interaction-agent",
      inputKind: "operator",
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
    assert.deepEqual(
      (turnStart?.params as Record<string, unknown>)?.environments,
      [{ environmentId: "local", cwd: "/home" }],
    );
    assert.deepEqual((turnStart?.params as Record<string, unknown>)?.input, [
      {
        type: "text",
        text: "request approval",
        text_elements: [],
      },
    ]);

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

    const steerPending = await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "active-steer-delivery",
      idempotencyKey: "active-steer-delivery",
      messageId: "active-steer-message",
      toAddress: "interaction-agent",
      inputKind: "operator",
      body: "include the new constraint",
      correlationId: "review-constraint-1",
      requireWake: true,
      createdAt: now(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(steerPending.status, "pending");
    assert.equal(
      steerPending.activation?.type,
      "external_turn_steer_requested",
    );
    const steered = await controller.applyCoordinationDelivery(steerPending);
    assert.equal(steered.status, "accepted");
    assert.equal(steered.reasonCode, "external_turn_steer_accepted");
    const steerRequest = transport.sent.find(
      (message) => message.method === "turn/steer",
    );
    assert.equal(
      (steerRequest?.params as Record<string, unknown>).expectedTurnId,
      "native-turn-1",
    );
    assert.deepEqual((steerRequest?.params as Record<string, unknown>).input, [
      {
        type: "text",
        text: "include the new constraint",
        text_elements: [],
      },
    ]);

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
      toAddress: "interaction-agent",
      inputKind: "operator",
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
    const activeBinding = await bridge.getExternalBinding(
      "interaction-binding",
    );
    assert.ok(activeBinding);
    const interruptReceipt = await controller.executeControl({
      controlId: "interaction-interrupt",
      idempotencyKey: "interaction-interrupt",
      bindingId: activeBinding.bindingId,
      expectedBindingRevision: activeBinding.revision,
      expectedNativeTurnId: "native-turn-2",
      kind: "interrupt_turn",
      payload: {},
      requestedAt: now(),
    });
    assert.equal(interruptReceipt.status, "applied");
    assert.deepEqual(interruptReceipt.outcome, {
      interrupted: true,
      nativeThreadId: "native-thread-1",
      nativeTurnId: "native-turn-2",
      nativeResult: {},
    });
    const interruptRequest = transport.sent.find(
      (message) => message.method === "turn/interrupt",
    );
    assert.deepEqual(interruptRequest?.params, {
      threadId: "native-thread-1",
      turnId: "native-turn-2",
    });
    await waitUntil(
      async () => (await bridge.listActiveExternalTurns()).length === 0,
      "interrupted turn completion",
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
    assert.deepEqual(
      (defaultTurnStart?.params as Record<string, unknown>)?.environments,
      [{ environmentId: "local", cwd: "/home" }],
    );
  } finally {
    await controller.stop().catch(() => undefined);
    await bridge.shutdownEngine({ engine, drainTimeoutMs: 5_000 });
    rmSync(dataDir, { recursive: true, force: true });
  }
});

test("controller expires undispatched turns and reports ambiguous native starts durably", async () => {
  const dataDir = mkdtempSync(
    join(tmpdir(), "rusty-crew-external-dispatch-controller-"),
  );
  const bridge = await loadNativeBridge();
  const engine = await bridge.initializeEngine({
    engineDataDir: dataDir,
    clock: "system",
    defaultTurnBudget: 16,
    defaultIdleTimeoutMs: 30_000,
    storage: { backend: "sqlite" },
  });
  const transport = new TurnStartTimeoutTransport();
  const controller = new ServiceExternalRuntimeController({
    bridge,
    instanceId: "dispatch-test-controller",
    driverFactory: (_registration, authority) =>
      new CodexAppServerDriver(transport, authority, { requestTimeoutMs: 25 }),
  });
  const runtimeId = "dispatch-runtime";
  const sessionId = "dispatch-session";
  const agentId = "dispatch-agent";
  const bindingId = "dispatch-binding";
  const now = (): string => new Date().toISOString();

  try {
    await bridge.createProfileRegistryRecord({
      profileId: "dispatch-profile",
      lifecycleStatus: "active",
      displayName: "Dispatch profile",
      defaultSessionKind: "full",
      agentId,
      activeRuntimeSettingsJson: {},
      sourceAssetRefs: [],
      derivedRuntimeRefs: [],
      importExport: { metadataJson: {} },
      now: now(),
    });
    await bridge.registerExternalRuntime({
      registration: {
        runtimeId,
        kind: "codex_app_server",
        endpoint: { transport: "unix_web_socket", address: "/tmp/fake.sock" },
        processOwnership: "attached",
        observedCliVersion: null,
        consumedContractRevision: null,
        compatibilityState: "unassessed",
        desiredState: "enabled",
        observedState: "disconnected",
        revision: 0,
        createdAt: now(),
        updatedAt: now(),
      },
    });
    await bridge.ensureConfiguredSession({
      sessionId,
      agentId,
      profileId: "dispatch-profile",
      kind: "full",
      toolProfile: { tools: [] },
    });
    await bridge.bindExternalAgent({
      binding: {
        bindingId,
        runtimeId,
        sessionId,
        agentId,
        profileId: "dispatch-profile",
        profileRevision: 1,
        profilePromptHash:
          "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        messageDeliveryPolicy: "immediate_steer",
        purpose: "crew_agent",
        nativeThreadId: "dispatch-thread",
        effectiveConfigFingerprint: "dispatch-test",
        status: "active",
        revision: 0,
        createdAt: now(),
        updatedAt: now(),
      },
    });
    await controller.connect(runtimeId);

    const expiring = await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "dispatch-expiry-delivery",
      idempotencyKey: "dispatch-expiry-delivery",
      messageId: "dispatch-expiry-message",
      toAddress: agentId,
      inputKind: "operator",
      body: "expire before dispatch",
      requireWake: true,
      createdAt: now(),
      expiresAt: new Date(Date.now() + 10).toISOString(),
    });
    assert.equal(expiring.activation?.type, "external_turn_requested");
    await new Promise((resolve) => setTimeout(resolve, 20));
    await controller.tick();
    const expired = await bridge.getExternalTurn(
      expiring.activation?.type === "external_turn_requested"
        ? expiring.activation.requestId
        : "",
    );
    assert.equal(expired?.phase, "failed");
    assert.equal(expired?.terminalReasonCode, "external_turn_dispatch_expired");
    assert.equal(
      transport.sent.some((message) => message.method === "turn/start"),
      false,
    );

    const ambiguous = await bridge.deliverAgentMessage({
      caller: { type: "system", senderAgentId: "operator" },
      deliveryId: "dispatch-timeout-delivery",
      idempotencyKey: "dispatch-timeout-delivery",
      messageId: "dispatch-timeout-message",
      toAddress: agentId,
      inputKind: "operator",
      body: "time out after native dispatch",
      requireWake: true,
      createdAt: now(),
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    assert.equal(ambiguous.activation?.type, "external_turn_requested");
    await controller.tick();
    const requestId =
      ambiguous.activation?.type === "external_turn_requested"
        ? ambiguous.activation.requestId
        : "";
    const terminal = await bridge.getExternalTurn(requestId);
    assert.equal(terminal?.phase, "outcome_unknown");
    assert.equal(
      terminal?.terminalReasonCode,
      "external_turn_start_outcome_unknown",
    );
    const events = await bridge.queryExternalRuntimeEvents({
      runtimeId,
      afterSequence: 0,
      limit: 1_000,
    });
    assert.ok(
      events.some(
        (event) =>
          event.requestId === requestId &&
          typeof event.payload === "object" &&
          event.payload !== null &&
          "nativeMethod" in event.payload &&
          "status" in event.payload &&
          event.payload.nativeMethod ===
            "rustyCrew/externalTurnDispatchFailed" &&
          event.payload.status === "external_turn_start_outcome_unknown",
      ),
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
  profilelessBindingReads = false,
  profileIdOverride = "creation-profile",
) {
  const dataDir = mkdtempSync(
    join(tmpdir(), "rusty-crew-external-creation-controller-"),
  );
  const runtimeId = "creation-runtime";
  const profileId = profileIdOverride;
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
      if (profilelessBindingReads && property === "getExternalBinding") {
        return async (bindingId: string) => {
          const binding = await target.getExternalBinding(bindingId);
          if (binding === undefined) return undefined;
          return withoutProfileProvenance(binding);
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
    promptSoulMarkdown: "CREATION_PROFILE_SOUL_MARKER",
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
      observedCliVersion: null,
      consumedContractRevision: null,
      compatibilityState: "unassessed",
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

function withoutProfileProvenance(
  binding: ExternalAgentBinding,
): ExternalAgentBinding {
  return {
    ...binding,
    profileId: null,
    profileRevision: null,
    profilePromptHash: null,
    profilePromptSnapshot: null,
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
