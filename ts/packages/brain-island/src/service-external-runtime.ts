import { randomUUID } from "node:crypto";

import type {
  AgentMessageDeliveryReceipt,
  ExternalAgentBinding,
  ExternalControlReceipt,
  ExternalControlRequest,
  ExternalControllerContext,
  ExternalControllerLease,
  ExternalInteractionRecord,
  ExternalRuntimeRegistration,
  ExternalTurnCorrelation,
  NormalizedExternalRuntimeEvent,
} from "@rusty-crew/contracts";
import {
  CODEX_APP_SERVER_PROTOCOL,
  CODEX_COORDINATION_DYNAMIC_TOOLS,
  CodexAppServerDriver,
  UnixWebSocketTransport,
  captureBoundedRawDetail,
  type CodexControllerAuthority,
  type CodexInitializeIdentity,
  type CodexProtocolFault,
  type CodexServerRequestContext,
  type NeutralExternalRuntimeEvent,
  type ServerRequestResolution,
} from "@rusty-crew/external-runtime-codex";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { resolveCodexCoordinationToolCall } from "./external-runtime-coordination.js";
import type {
  ExternalThreadItemProjection,
  ExternalThreadPage,
  ExternalThreadProjection,
  ExternalThreadReadResult,
  ExternalThreadTurnProjection,
} from "./external-runtime-api-contract.js";

const CONTROLLER_LEASE_MS = 30_000;
const RAW_DETAIL_LIMIT = 256;

interface ControlledRuntime {
  registration: ExternalRuntimeRegistration;
  lease: ExternalControllerLease;
  driver: CodexAppServerDriver;
  rawDetails: Map<string, ExternalRuntimeRawDetail>;
}

interface PendingInteractionResolution {
  resolve(value: ServerRequestResolution): void;
  timer: NodeJS.Timeout;
  interaction: ExternalInteractionRecord;
}

export interface ExternalRuntimeRawDetail {
  readonly detailId: string;
  readonly runtimeId: string;
  readonly json: string;
  readonly originalSha256: string;
  readonly truncated: boolean;
  readonly redactedKeys: readonly string[];
}

export interface ExternalRuntimeControllerStatus {
  readonly runtimeId: string;
  readonly driverState: string;
  readonly controllerInstanceId: string;
  readonly controllerGeneration: number;
  readonly leaseExpiresAt: string;
}

export class ServiceExternalRuntimeController {
  readonly #bridge: NativeBridgeModule;
  readonly #now: () => Date;
  readonly #instanceId: string;
  readonly #onCoordinationDelivery?: (
    receipt: AgentMessageDeliveryReceipt,
  ) => void;
  readonly #driverFactory: (
    registration: ExternalRuntimeRegistration,
    authority: CodexControllerAuthority,
  ) => CodexAppServerDriver;
  readonly #controlled = new Map<string, ControlledRuntime>();
  readonly #pendingInteractions = new Map<
    string,
    PendingInteractionResolution
  >();
  #ticking = false;

  constructor(input: {
    bridge: NativeBridgeModule;
    now?: () => Date;
    instanceId?: string;
    driverFactory?: (
      registration: ExternalRuntimeRegistration,
      authority: CodexControllerAuthority,
    ) => CodexAppServerDriver;
    onCoordinationDelivery?: (receipt: AgentMessageDeliveryReceipt) => void;
  }) {
    this.#bridge = input.bridge;
    this.#now = input.now ?? (() => new Date());
    this.#instanceId = input.instanceId ?? `service-host:${randomUUID()}`;
    this.#onCoordinationDelivery = input.onCoordinationDelivery;
    this.#driverFactory =
      input.driverFactory ??
      ((registration, authority) =>
        new CodexAppServerDriver(
          new UnixWebSocketTransport(registration.endpoint.address),
          authority,
        ));
  }

  async start(): Promise<void> {
    await this.tick();
  }

  async stop(): Promise<void> {
    for (const pending of this.#pendingInteractions.values()) {
      clearTimeout(pending.timer);
      pending.resolve({
        type: "error",
        code: -32002,
        message: "Rusty Crew controller stopped before interaction resolution",
      });
    }
    this.#pendingInteractions.clear();
    for (const controlled of this.#controlled.values()) {
      await controlled.driver.close().catch(() => undefined);
      await this.#bridge
        .releaseExternalController({
          runtimeId: controlled.registration.runtimeId,
          holderInstanceId: this.#instanceId,
          generation: controlled.lease.generation,
          now: this.#now().toISOString(),
        })
        .catch(() => undefined);
    }
    this.#controlled.clear();
  }

  statuses(): ExternalRuntimeControllerStatus[] {
    return [...this.#controlled.values()].map((controlled) => ({
      runtimeId: controlled.registration.runtimeId,
      driverState: controlled.driver.state,
      controllerInstanceId: this.#instanceId,
      controllerGeneration: controlled.lease.generation,
      leaseExpiresAt: controlled.lease.expiresAt,
    }));
  }

  async connect(runtimeId: string): Promise<ExternalRuntimeControllerStatus> {
    const existing = this.#controlled.get(runtimeId);
    if (existing !== undefined && existing.driver.state === "ready") {
      return this.#status(existing);
    }
    const registration = await this.#bridge.getExternalRuntime(runtimeId);
    if (registration === undefined) {
      throw new Error(`external runtime ${runtimeId} was not found`);
    }
    if (registration.desiredState !== "enabled") {
      throw new Error(`external runtime ${runtimeId} is disabled`);
    }
    if (registration.kind !== "codex_app_server") {
      throw new Error(`unsupported external runtime kind ${registration.kind}`);
    }
    const lease = await this.#acquireLease(runtimeId);
    const controller: ExternalControllerContext = {
      holderInstanceId: this.#instanceId,
      generation: lease.generation,
    };
    await this.#bridge.recordExternalRuntimeState({
      runtimeId,
      controller,
      observedState: "connecting",
      reasonCode: "controller_connecting",
      observedAt: this.#now().toISOString(),
    });
    const controlled: ControlledRuntime = {
      registration,
      lease,
      driver: undefined as unknown as CodexAppServerDriver,
      rawDetails: new Map(),
    };
    const authority = this.#authority(controlled);
    controlled.driver = this.#driverFactory(registration, authority);
    this.#controlled.set(runtimeId, controlled);
    try {
      await controlled.driver.connect();
      await this.#resumePersistedBindings(controlled);
      controlled.registration =
        (await this.#bridge.getExternalRuntime(runtimeId)) ?? registration;
      return this.#status(controlled);
    } catch (error) {
      if (controlled.driver.state !== "incompatible") {
        await this.#recordState(
          controlled,
          "degraded",
          "controller_connect_failed",
        );
      }
      throw error;
    }
  }

  async listThreads(
    runtimeId: string,
    params: unknown,
  ): Promise<ExternalThreadPage> {
    const controlled = await this.#requireControlled(runtimeId);
    const result = await controlled.driver.threadList(
      params as Parameters<CodexAppServerDriver["threadList"]>[0],
    );
    return {
      items: result.data.map(projectExternalThread),
      nextCursor: result.nextCursor,
      backwardsCursor: result.backwardsCursor,
    };
  }

  async readThread(
    runtimeId: string,
    params: unknown,
  ): Promise<ExternalThreadReadResult> {
    const controlled = await this.#requireControlled(runtimeId);
    const result = await controlled.driver.threadRead(
      params as Parameters<CodexAppServerDriver["threadRead"]>[0],
    );
    return { thread: projectExternalThread(result.thread) };
  }

  async executeControl(
    request: ExternalControlRequest,
  ): Promise<ExternalControlReceipt> {
    const receipt = await this.#bridge.submitExternalControl(request);
    if (receipt.status !== "pending") return receipt;
    const binding = await this.#requireBinding(request.bindingId);
    const controlled = await this.#requireControlled(binding.runtimeId);
    const controller = this.#controllerContext(controlled);
    try {
      const outcome = browserSafeNativeValue(
        await this.#applyControl(controlled, binding, request),
      );
      return await this.#bridge.completeExternalControl({
        controller,
        controlId: request.controlId,
        status: "applied",
        outcome,
        now: this.#now().toISOString(),
      });
    } catch (error) {
      await this.#bridge.completeExternalControl({
        controller,
        controlId: request.controlId,
        status: "failed",
        reasonCode: "external_control_driver_failed",
        outcome: { message: String(error) },
        now: this.#now().toISOString(),
      });
      throw error;
    }
  }

  async resolveInteraction(input: {
    interactionId: string;
    expectedRevision: number;
    idempotencyKey: string;
    result: unknown;
  }): Promise<ExternalInteractionRecord> {
    const interaction = (
      await this.#bridge.listPendingExternalInteractions()
    ).find((candidate) => candidate.interactionId === input.interactionId);
    if (interaction === undefined) {
      throw new Error(
        `pending external interaction ${input.interactionId} was not found`,
      );
    }
    const next: ExternalInteractionRecord = {
      ...interaction,
      status: "resolved",
      resolutionIdempotencyKey: input.idempotencyKey,
      outcome: input.result,
      resolvedAt: this.#now().toISOString(),
    };
    const saved = await this.#bridge.resolveExternalInteraction({
      interaction: next,
      expectedRevision: input.expectedRevision,
    });
    const turn = (await this.#bridge.listActiveExternalTurns()).find(
      (candidate) => candidate.request.requestId === saved.requestId,
    );
    const controlled = this.#controlled.get(saved.runtimeId);
    if (
      turn?.phase === "waiting_interaction" &&
      controlled !== undefined &&
      controlled.driver.state === "ready"
    ) {
      await this.#bridge.transitionExternalTurn({
        controller: this.#controllerContext(controlled),
        requestId: turn.request.requestId,
        nextPhase: "active",
        now: this.#now().toISOString(),
      });
    }
    const pending = this.#pendingInteractions.get(input.interactionId);
    if (pending !== undefined) {
      clearTimeout(pending.timer);
      this.#pendingInteractions.delete(input.interactionId);
      pending.resolve({ type: "success", result: input.result });
    }
    return saved;
  }

  rawDetail(
    runtimeId: string,
    detailId: string,
  ): ExternalRuntimeRawDetail | undefined {
    return this.#controlled.get(runtimeId)?.rawDetails.get(detailId);
  }

  async tick(): Promise<void> {
    if (this.#ticking) return;
    this.#ticking = true;
    try {
      const runtimes = await this.#bridge.listExternalRuntimes();
      for (const registration of runtimes) {
        if (registration.desiredState !== "enabled") continue;
        const controlled = this.#controlled.get(registration.runtimeId);
        if (
          controlled === undefined ||
          controlled.driver.state === "disconnected"
        ) {
          await this.connect(registration.runtimeId).catch(() => undefined);
          continue;
        }
        controlled.registration = registration;
        controlled.lease = await this.#acquireLease(registration.runtimeId);
      }
      await this.#dispatchAcceptedTurns();
    } finally {
      this.#ticking = false;
    }
  }

  async #dispatchAcceptedTurns(): Promise<void> {
    const turns = await this.#bridge.listActiveExternalTurns();
    for (const turn of turns) {
      if (turn.phase !== "accepted") continue;
      const binding = await this.#bridge.getExternalBinding(
        turn.request.bindingId,
      );
      if (binding === undefined) continue;
      const controlled = await this.#requireControlled(binding.runtimeId).catch(
        () => undefined,
      );
      if (controlled === undefined) continue;
      await this.#startAcceptedTurn(controlled, binding, turn).catch(
        async (error) => {
          await this.#bridge.transitionExternalTurn({
            controller: this.#controllerContext(controlled),
            requestId: turn.request.requestId,
            nextPhase: "failed",
            terminalReasonCode: `external_turn_start_failed:${String(error)}`,
            now: this.#now().toISOString(),
          });
        },
      );
    }
  }

  async #resumePersistedBindings(controlled: ControlledRuntime): Promise<void> {
    const bindings = await this.#bridge.listExternalBindings();
    for (const binding of bindings) {
      if (
        binding.runtimeId !== controlled.registration.runtimeId ||
        binding.status !== "active" ||
        typeof binding.nativeThreadId !== "string"
      ) {
        continue;
      }
      await controlled.driver.threadResume({
        threadId: binding.nativeThreadId,
        ...(typeof binding.cwd === "string" ? { cwd: binding.cwd } : {}),
        approvalPolicy: "never",
        sandbox: "danger-full-access",
        excludeTurns: true,
      });
    }
  }

  async #startAcceptedTurn(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding,
    turn: ExternalTurnCorrelation,
  ): Promise<void> {
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: "starting",
      now: this.#now().toISOString(),
    });
    const started = await controlled.driver.turnStart({
      threadId: turn.nativeThreadId,
      input: turn.request.input.map((part) =>
        part.type === "text"
          ? { type: "text" as const, text: part.text, text_elements: [] }
          : {
              type: "text" as const,
              text: `[${part.type}] ${JSON.stringify(part)}`,
              text_elements: [],
            },
      ),
      ...(typeof binding.cwd === "string" ? { cwd: binding.cwd } : {}),
      approvalPolicy: "never",
      sandboxPolicy: { type: "dangerFullAccess" },
    });
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: "active",
      nativeTurnId: started.turn.id,
      now: this.#now().toISOString(),
    });
  }

  async #applyControl(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding,
    request: ExternalControlRequest,
  ): Promise<unknown> {
    switch (request.kind) {
      case "start_or_resume_thread": {
        if (typeof binding.nativeThreadId === "string") {
          return controlled.driver.threadResume({
            threadId: binding.nativeThreadId,
            ...(isRecord(request.payload) ? request.payload : {}),
          });
        }
        const started = await controlled.driver.threadStart({
          cwd: binding.cwd ?? "/home",
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          ephemeral: false,
          dynamicTools: [...CODEX_COORDINATION_DYNAMIC_TOOLS],
          ...(isRecord(request.payload) ? request.payload : {}),
        });
        await this.#bridge.bindExternalAgent({
          binding: {
            ...binding,
            nativeThreadId: started.thread.id,
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: binding.revision,
        });
        return started;
      }
      case "steer_turn":
        return controlled.driver.turnSteer(
          request.payload as Parameters<CodexAppServerDriver["turnSteer"]>[0],
        );
      case "interrupt_turn":
        return controlled.driver.turnInterrupt(
          request.payload as Parameters<
            CodexAppServerDriver["turnInterrupt"]
          >[0],
        );
      case "compact_thread":
        return controlled.driver.compactThread(
          request.payload as Parameters<
            CodexAppServerDriver["compactThread"]
          >[0],
        );
      case "reconcile_runtime":
        return typeof binding.nativeThreadId !== "string"
          ? { reconciled: true, nativeThreadId: null }
          : controlled.driver.threadRead({
              threadId: binding.nativeThreadId,
              includeTurns: true,
            });
      case "archive_binding":
        return this.#bridge.bindExternalAgent({
          binding: {
            ...binding,
            status: "archived",
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: binding.revision,
        });
      case "start_turn":
        throw new Error(
          "start_turn is admitted through Rust agent activation, not a raw browser control",
        );
      case "resolve_interaction":
        throw new Error(
          "resolve_interaction uses the typed interaction resolution operation",
        );
    }
  }

  #authority(controlled: ControlledRuntime): CodexControllerAuthority {
    return {
      authorizeHandshake: (identity) =>
        this.#authorizeHandshake(controlled, identity),
      hasControllerLease: () => this.#hasLease(controlled),
      onEvent: (event) => this.#recordEvent(controlled, event),
      resolveServerRequest: (context) =>
        this.#resolveServerRequest(controlled, context),
      onProtocolFault: (fault) => this.#recordProtocolFault(controlled, fault),
      onDisconnected: ({ reason }) => this.#onDisconnected(controlled, reason),
    };
  }

  async #authorizeHandshake(
    controlled: ControlledRuntime,
    identity: CodexInitializeIdentity,
  ): Promise<{ accepted: boolean; reasonCode?: string; message?: string }> {
    const decision = await this.#bridge.authorizeExternalRuntimeHandshake({
      runtimeId: controlled.registration.runtimeId,
      controller: this.#controllerContext(controlled),
      cliVersion: parseCodexCliVersion(identity.userAgent),
      executableSha256: CODEX_APP_SERVER_PROTOCOL.nativeExecutableSha256,
      protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
      observedAt: this.#now().toISOString(),
    });
    controlled.registration = decision.registration;
    return {
      accepted: decision.accepted,
      ...(decision.reasonCode == null
        ? {}
        : {
            reasonCode: decision.reasonCode,
            message: `Rust authority rejected ${identity.userAgent}`,
          }),
    };
  }

  async #recordEvent(
    controlled: ControlledRuntime,
    event: NeutralExternalRuntimeEvent,
  ): Promise<void> {
    const detailId = `${controlled.registration.runtimeId}:${controlled.lease.generation}:${event.transportSequence}`;
    this.#rememberRawDetail(controlled, {
      detailId,
      runtimeId: controlled.registration.runtimeId,
      ...event.rawDetail,
    });
    const bindings = await this.#bridge.listExternalBindings();
    const binding = bindings.find(
      (candidate) =>
        candidate.runtimeId === controlled.registration.runtimeId &&
        candidate.nativeThreadId === event.threadId,
    );
    const saved = await this.#bridge.recordExternalRuntimeEvent({
      controller: this.#controllerContext(controlled),
      event: {
        eventId: detailId,
        ...(binding?.sessionId === undefined
          ? {}
          : { sessionId: binding.sessionId }),
        createdAt: this.#now().toISOString(),
        kind: event.kind,
        runtimeId: controlled.registration.runtimeId,
        ...(event.threadId === undefined
          ? {}
          : { nativeThreadId: event.threadId }),
        ...(event.turnId === undefined ? {} : { nativeTurnId: event.turnId }),
        ...(event.itemId === undefined ? {} : { itemId: event.itemId }),
        ...(event.nativeRequestId === undefined
          ? {}
          : { requestId: String(event.nativeRequestId) }),
        payload: browserSafePayload(event),
        rawDetailRef: detailId,
      },
    });
    await this.#applyTerminalEvent(controlled, saved);
  }

  async #applyTerminalEvent(
    controlled: ControlledRuntime,
    event: NormalizedExternalRuntimeEvent,
  ): Promise<void> {
    const phase = terminalPhase(event);
    if (phase === undefined || event.nativeTurnId === undefined) return;
    const turn = (await this.#bridge.listActiveExternalTurns()).find(
      (candidate) =>
        candidate.runtimeId === event.runtimeId &&
        candidate.nativeTurnId === event.nativeTurnId,
    );
    if (turn === undefined) return;
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: phase,
      ...(phase === "completed"
        ? {}
        : { terminalReasonCode: `codex_${phase}` }),
      now: this.#now().toISOString(),
    });
  }

  async #resolveServerRequest(
    controlled: ControlledRuntime,
    context: CodexServerRequestContext,
  ): Promise<ServerRequestResolution> {
    if (context.request.method === "item/tool/call") {
      const params = context.request.params;
      const binding = (await this.#bridge.listExternalBindings()).find(
        (candidate) =>
          candidate.runtimeId === controlled.registration.runtimeId &&
          candidate.nativeThreadId === params.threadId,
      );
      if (binding === undefined) {
        return {
          type: "error",
          code: -32004,
          message: "dynamic tool call has no active Crew agent binding",
        };
      }
      const result = await resolveCodexCoordinationToolCall({
        params,
        binding: {
          runtimeId: binding.runtimeId,
          bindingId: binding.bindingId,
          controllerInstanceId: this.#instanceId,
          controllerGeneration: controlled.lease.generation,
        },
        port: this.#bridge,
        onDelivery: this.#onCoordinationDelivery,
        now: this.#now,
      });
      return result === undefined
        ? {
            type: "error",
            code: -32601,
            message: "unsupported Rusty Crew dynamic tool",
          }
        : { type: "success", result };
    }
    return this.#waitForInteraction(controlled, context);
  }

  async #waitForInteraction(
    controlled: ControlledRuntime,
    context: CodexServerRequestContext,
  ): Promise<ServerRequestResolution> {
    const params = context.request.params as Record<string, unknown>;
    const threadId = stringValue(params.threadId);
    const turnId = stringValue(params.turnId);
    if (threadId === undefined || turnId === undefined) {
      return {
        type: "error",
        code: -32602,
        message: "interactive request lacks threadId or turnId",
      };
    }
    const binding = (await this.#bridge.listExternalBindings()).find(
      (candidate) =>
        candidate.runtimeId === controlled.registration.runtimeId &&
        candidate.nativeThreadId === threadId,
    );
    const turn = (await this.#bridge.listActiveExternalTurns()).find(
      (candidate) => candidate.nativeTurnId === turnId,
    );
    if (binding === undefined || turn === undefined) {
      return {
        type: "error",
        code: -32004,
        message: "interactive request does not match an active Crew turn",
      };
    }
    const interactionId = `${controlled.registration.runtimeId}:${String(context.request.id)}`;
    const now = this.#now();
    const expiresAt = new Date(now.getTime() + 5 * 60_000);
    const savedInteraction = await this.#bridge.recordExternalInteraction({
      controller: this.#controllerContext(controlled),
      interaction: {
        interactionId,
        runtimeId: controlled.registration.runtimeId,
        bindingId: binding.bindingId,
        requestId: turn.request.requestId,
        nativeThreadId: threadId,
        nativeTurnId: turnId,
        nativeRequestId: String(context.request.id),
        kind: interactionKind(context.request.method),
        prompt: browserSafeRawDetail(context.rawDetail),
        allowedResponses: allowedInteractionResponses(context.request.method),
        status: "pending",
        rawDetailRef: interactionId,
        requestedAt: now.toISOString(),
        expiresAt: expiresAt.toISOString(),
        revision: 1,
      },
    });
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: "waiting_interaction",
      now: this.#now().toISOString(),
    });
    return new Promise<ServerRequestResolution>((resolve) => {
      const timer = setTimeout(() => {
        this.#pendingInteractions.delete(interactionId);
        void this.#bridge
          .terminalizeExternalInteraction({
            controller: this.#controllerContext(controlled),
            interaction: {
              ...savedInteraction,
              status: "expired",
              resolvedAt: this.#now().toISOString(),
            },
            expectedRevision: savedInteraction.revision,
          })
          .finally(() =>
            resolve({
              type: "error",
              code: -32005,
              message: "external interaction expired before operator response",
            }),
          );
      }, expiresAt.getTime() - now.getTime());
      timer.unref();
      this.#pendingInteractions.set(interactionId, {
        resolve,
        timer,
        interaction: savedInteraction,
      });
    });
  }

  async #recordProtocolFault(
    controlled: ControlledRuntime,
    fault: CodexProtocolFault,
  ): Promise<void> {
    await this.#recordState(
      controlled,
      fault.fatal ? "degraded" : "disconnected",
      fault.reasonCode,
    ).catch(() => undefined);
  }

  async #onDisconnected(
    controlled: ControlledRuntime,
    reason: string,
  ): Promise<void> {
    for (const [interactionId, pending] of this.#pendingInteractions) {
      if (pending.interaction.runtimeId !== controlled.registration.runtimeId) {
        continue;
      }
      clearTimeout(pending.timer);
      this.#pendingInteractions.delete(interactionId);
      await this.#bridge
        .terminalizeExternalInteraction({
          controller: this.#controllerContext(controlled),
          interaction: {
            ...pending.interaction,
            status: "lost",
            resolvedAt: this.#now().toISOString(),
          },
          expectedRevision: pending.interaction.revision,
        })
        .catch(() => undefined);
      pending.resolve({
        type: "error",
        code: -32006,
        message: "external interaction was lost with the controller connection",
      });
    }
    await this.#recordState(
      controlled,
      "disconnected",
      `controller_disconnected:${reason}`,
    ).catch(() => undefined);
  }

  async #recordState(
    controlled: ControlledRuntime,
    observedState: "connecting" | "disconnected" | "degraded",
    reasonCode: string,
  ): Promise<void> {
    controlled.registration = await this.#bridge.recordExternalRuntimeState({
      runtimeId: controlled.registration.runtimeId,
      controller: this.#controllerContext(controlled),
      observedState,
      reasonCode,
      observedAt: this.#now().toISOString(),
    });
  }

  async #acquireLease(runtimeId: string): Promise<ExternalControllerLease> {
    const now = this.#now();
    return this.#bridge.acquireExternalController({
      lease: {
        runtimeId,
        holderInstanceId: this.#instanceId,
        generation: 0,
        acquiredAt: now.toISOString(),
        renewedAt: now.toISOString(),
        expiresAt: new Date(now.getTime() + CONTROLLER_LEASE_MS).toISOString(),
        revision: 0,
      },
      now: now.toISOString(),
    });
  }

  #hasLease(controlled: ControlledRuntime): boolean {
    return (
      controlled.lease.holderInstanceId === this.#instanceId &&
      Date.parse(controlled.lease.expiresAt) > this.#now().getTime()
    );
  }

  #controllerContext(controlled: ControlledRuntime): ExternalControllerContext {
    return {
      holderInstanceId: this.#instanceId,
      generation: controlled.lease.generation,
    };
  }

  async #requireControlled(runtimeId: string): Promise<ControlledRuntime> {
    await this.connect(runtimeId);
    const controlled = this.#controlled.get(runtimeId);
    if (controlled === undefined || controlled.driver.state !== "ready") {
      throw new Error(`external runtime ${runtimeId} controller is not ready`);
    }
    return controlled;
  }

  async #requireBinding(bindingId: string): Promise<ExternalAgentBinding> {
    const binding = await this.#bridge.getExternalBinding(bindingId);
    if (binding === undefined) {
      throw new Error(`external binding ${bindingId} was not found`);
    }
    return binding;
  }

  #rememberRawDetail(
    controlled: ControlledRuntime,
    detail: ExternalRuntimeRawDetail,
  ): void {
    controlled.rawDetails.set(detail.detailId, detail);
    while (controlled.rawDetails.size > RAW_DETAIL_LIMIT) {
      const oldest = controlled.rawDetails.keys().next().value as
        | string
        | undefined;
      if (oldest === undefined) break;
      controlled.rawDetails.delete(oldest);
    }
  }

  #status(controlled: ControlledRuntime): ExternalRuntimeControllerStatus {
    return {
      runtimeId: controlled.registration.runtimeId,
      driverState: controlled.driver.state,
      controllerInstanceId: this.#instanceId,
      controllerGeneration: controlled.lease.generation,
      leaseExpiresAt: controlled.lease.expiresAt,
    };
  }
}

function projectExternalThread(value: unknown): ExternalThreadProjection {
  const thread = requireNativeRecord(value, "thread");
  return {
    threadId: requireNativeString(thread.id, "thread.id"),
    sessionId: requireNativeString(thread.sessionId, "thread.sessionId"),
    parentThreadId: nullableNativeString(thread.parentThreadId),
    preview: nativeString(thread.preview) ?? "",
    ephemeral: thread.ephemeral === true,
    modelProvider: nativeString(thread.modelProvider) ?? "unknown",
    createdAt: nativeNumber(thread.createdAt) ?? 0,
    updatedAt: nativeNumber(thread.updatedAt) ?? 0,
    status: projectNativeStatus(thread.status),
    cwd: nativeString(thread.cwd) ?? "/home",
    cliVersion: nativeString(thread.cliVersion) ?? "unknown",
    name: nullableNativeString(thread.name),
    agentNickname: nullableNativeString(thread.agentNickname),
    agentRole: nullableNativeString(thread.agentRole),
    turns: Array.isArray(thread.turns)
      ? thread.turns.map(projectExternalThreadTurn)
      : [],
  };
}

function projectExternalThreadTurn(
  value: unknown,
): ExternalThreadTurnProjection {
  const turn = requireNativeRecord(value, "thread turn");
  return {
    turnId: requireNativeString(turn.id, "turn.id"),
    status: nativeString(turn.status) ?? "unknown",
    startedAt: nullableNativeNumber(turn.startedAt),
    completedAt: nullableNativeNumber(turn.completedAt),
    durationMs: nullableNativeNumber(turn.durationMs),
    items: Array.isArray(turn.items)
      ? turn.items.map(projectExternalThreadItem)
      : [],
  };
}

function projectExternalThreadItem(
  value: unknown,
): ExternalThreadItemProjection {
  const item = requireNativeRecord(value, "thread item");
  const kind = requireNativeString(item.type, "thread item type");
  const itemId = requireNativeString(item.id, "thread item id");
  const status = projectOptionalNativeStatus(item.status);
  const text = projectThreadItemText(kind, item);
  const summary = Array.isArray(item.summary)
    ? item.summary.filter((entry): entry is string => typeof entry === "string")
    : undefined;
  return {
    itemId,
    kind,
    ...(status === undefined ? {} : { status }),
    ...(text === undefined ? {} : { text }),
    ...(summary === undefined || summary.length === 0 ? {} : { summary }),
  };
}

function projectThreadItemText(
  kind: string,
  item: Record<string, unknown>,
): string | undefined {
  if (typeof item.text === "string") return item.text;
  if (kind === "commandExecution") {
    const command = nativeString(item.command);
    const output = nativeString(item.aggregatedOutput);
    return [command, output].filter((part) => part !== undefined).join("\n");
  }
  if (kind === "fileChange" && Array.isArray(item.changes)) {
    return `${item.changes.length} file change${item.changes.length === 1 ? "" : "s"}`;
  }
  if (kind === "mcpToolCall") {
    return [nativeString(item.server), nativeString(item.tool)]
      .filter((part) => part !== undefined)
      .join("/");
  }
  if (kind === "dynamicToolCall") return nativeString(item.tool);
  if (kind === "userMessage" && Array.isArray(item.content)) {
    const text = item.content
      .map((entry) =>
        isRecord(entry)
          ? (nativeString(entry.text) ?? nativeString(entry.value))
          : undefined,
      )
      .filter((entry): entry is string => entry !== undefined)
      .join("\n");
    return text === "" ? undefined : text;
  }
  return undefined;
}

function projectNativeStatus(value: unknown): string {
  return projectOptionalNativeStatus(value) ?? "unknown";
}

function projectOptionalNativeStatus(value: unknown): string | undefined {
  if (typeof value === "string") return value;
  return isRecord(value) ? nativeString(value.type) : undefined;
}

function requireNativeRecord(
  value: unknown,
  label: string,
): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`invalid native ${label} payload`);
  return value;
}

function requireNativeString(value: unknown, label: string): string {
  const parsed = nativeString(value);
  if (parsed === undefined) throw new Error(`invalid native ${label}`);
  return parsed;
}

function nativeString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function nullableNativeString(value: unknown): string | null {
  return nativeString(value) ?? null;
}

function nativeNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function nullableNativeNumber(value: unknown): number | null {
  return nativeNumber(value) ?? null;
}

function parseCodexCliVersion(userAgent: string): string {
  const match = /^(?:codex_cli_rs|codex-cli|[^/\s]+)\/([^\s]+)/.exec(userAgent);
  return match?.[1] ?? userAgent;
}

function browserSafeNativeValue(value: unknown): unknown {
  const detail = captureBoundedRawDetail(value, 1024 * 1024);
  if (detail.truncated) {
    return {
      truncated: true,
      originalSha256: detail.originalSha256,
      redactedKeys: detail.redactedKeys,
    };
  }
  return JSON.parse(detail.json) as unknown;
}

function browserSafePayload(
  event: NeutralExternalRuntimeEvent,
): NeutralExternalRuntimeEvent["payload"] {
  return event.payload;
}

function browserSafeRawDetail(
  detail: CodexServerRequestContext["rawDetail"],
): unknown {
  if (detail.truncated) {
    return { truncated: true, originalSha256: detail.originalSha256 };
  }
  try {
    const parsed = JSON.parse(detail.json) as unknown;
    return isRecord(parsed) && "params" in parsed ? parsed.params : parsed;
  } catch {
    return { malformed: true, originalSha256: detail.originalSha256 };
  }
}

function terminalPhase(
  event: NormalizedExternalRuntimeEvent,
): "completed" | "failed" | "interrupted" | undefined {
  const method = isRecord(event.payload)
    ? stringValue(event.payload.nativeMethod)
    : undefined;
  const source = method ?? event.kind;
  if (source === "turn/completed") {
    const status = isRecord(event.payload)
      ? stringValue(event.payload.status)
      : undefined;
    if (status === "interrupted") return "interrupted";
    if (status === "failed") return "failed";
    return "completed";
  }
  if (source === "turn/interrupted") return "interrupted";
  if (source === "turn/failed" || source === "error") return "failed";
  return undefined;
}

function interactionKind(method: string): ExternalInteractionRecord["kind"] {
  if (method.includes("commandExecution")) return "command_approval";
  if (method.includes("fileChange")) return "file_approval";
  if (method.includes("requestUserInput")) return "request_user_input";
  if (method.includes("permissions")) return "permission_request";
  if (method.startsWith("mcpServer/elicitation")) return "mcp_elicitation";
  return "unsupported";
}

function allowedInteractionResponses(method: string): string[] {
  if (method.includes("requestUserInput")) return ["answers"];
  if (method.startsWith("mcpServer/elicitation")) {
    return ["accept", "decline", "cancel"];
  }
  if (method.includes("permissions")) return ["permissions"];
  return ["accept", "acceptForSession", "decline", "cancel"];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}
