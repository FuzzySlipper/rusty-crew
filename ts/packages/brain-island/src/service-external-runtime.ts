import { createHash, randomUUID } from "node:crypto";

import type {
  AgentMessageDeliveryReceipt,
  DenRuntimeReference,
  ExternalAgentBinding,
  ExternalAgentSessionCreationRecord,
  ExternalAgentSessionCreationRequest,
  ExternalControlReceipt,
  ExternalControlRequest,
  ExternalControllerContext,
  ExternalControllerLease,
  ExternalInteractionRecord,
  ExternalRuntimeRegistration,
  ExternalRuntimeCompatibilityProbeReport,
  ExternalTurnCorrelation,
  NormalizedExternalRuntimeEvent,
} from "@rusty-crew/contracts";
import {
  CODEX_APP_SERVER_PROTOCOL,
  CODEX_COORDINATION_DYNAMIC_TOOLS,
  CodexAppServerDriver,
  CodexRpcError,
  UnixWebSocketTransport,
  captureBoundedRawDetail,
  type CollaborationMode,
  type CodexControllerAuthority,
  type CodexInitializeIdentity,
  type Model,
  type CodexProtocolFault,
  type CodexServerRequestContext,
  type NeutralExternalRuntimeEvent,
  type ServerRequestResolution,
} from "@rusty-crew/external-runtime-codex";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { resolveCodexCoordinationToolCall } from "./external-runtime-coordination.js";
import {
  EXTERNAL_RUNTIME_COMMAND_DEFINITIONS,
  ExternalRuntimeCommandInputError,
  parseExternalRuntimeCommand,
  type ParsedExternalRuntimeCommand,
} from "./external-runtime-commands.js";
import type {
  ExternalAgentSessionCreateResult,
  ExternalAgentMessagePhase,
  ExternalThreadDeleteReceipt,
  ExternalRuntimeCommandCatalog,
  ExternalRuntimeCommandExecutionResult,
  ExternalRuntimeCommandResultData,
  ExternalRuntimeModelOption,
  ExternalThreadCommandStatus,
  ExternalThreadSettingsProjection,
  ExternalThreadUsageProjection,
  ExternalThreadItemProjection,
  ExternalThreadLifecycleReceipt,
  ExternalThreadPage,
  ExternalThreadProjection,
  ExternalThreadReadResult,
  ExternalThreadTurnProjection,
} from "./external-runtime-api-contract.js";

const CONTROLLER_LEASE_MS = 30_000;
const RAW_DETAIL_LIMIT = 256;
export const EXTERNAL_AGENT_SESSION_CREATION_REASON_CODES = [
  "external_agent_creation_idempotency_key_required",
  "external_agent_creation_idempotency_conflict",
  "external_agent_creation_runtime_unavailable",
  "external_agent_creation_profile_invalid",
  "external_agent_creation_profile_revision_conflict",
  "external_agent_creation_cwd_invalid",
  "external_agent_creation_revision_conflict",
  "external_agent_creation_binding_conflict",
  "external_agent_creation_native_thread_conflict",
  "external_agent_creation_capacity_conflict",
  "external_agent_creation_native_start_failed",
  "external_agent_creation_recovery_required",
] as const;

interface ControlledRuntime {
  registration: ExternalRuntimeRegistration;
  lease: ExternalControllerLease;
  driver: CodexAppServerDriver;
  handshakeIdentity?: CodexInitializeIdentity;
  bindingResumeFailures: ExternalBindingResumeFailure[];
  rawDetails: Map<string, ExternalRuntimeRawDetail>;
  threadSettings: Map<string, ControlledThreadSettings>;
  threadUsage: Map<string, ExternalThreadUsageProjection>;
  archivedThreadIds: Set<string>;
}

type ControlledThreadSettings = CollaborationMode["settings"] & {
  readonly modelProvider: string;
};

interface PendingInteractionResolution {
  resolve(value: ServerRequestResolution): void;
  timer: NodeJS.Timeout;
  interaction: ExternalInteractionRecord;
}

type NativeCodexThread = Awaited<
  ReturnType<CodexAppServerDriver["threadList"]>
>["data"][number];

interface NativeThreadCatalogEntry {
  readonly thread: NativeCodexThread;
  readonly archived: boolean;
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
  readonly observedCliVersion: string | null;
  readonly consumedContractRevision: string | null;
  readonly compatibilityState: ExternalRuntimeRegistration["compatibilityState"];
  readonly compatibilityDiagnostic:
    | "certified"
    | "compatible_uncertified"
    | "incompatible"
    | "probe_failed"
    | "disconnected";
  readonly lastCompatibilityProbe: ExternalRuntimeCompatibilityProbeReport | null;
  readonly bindingResumeFailures: readonly ExternalBindingResumeFailure[];
}

export interface ExternalBindingResumeFailure {
  readonly bindingId: string;
  readonly nativeThreadId: string;
  readonly reason: string;
  readonly observedAt: string;
}

export class ExternalAgentSessionCreationError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
    readonly retryable: boolean,
  ) {
    super(`${reasonCode}: ${message}`);
    this.name = "ExternalAgentSessionCreationError";
  }
}

export class ExternalThreadLifecycleError extends Error {
  constructor(
    readonly reasonCode:
      | "external_thread_not_found"
      | "external_thread_active"
      | "external_thread_interaction_pending"
      | "external_thread_listing_limit_exceeded"
      | "external_thread_binding_reconciliation_failed"
      | "external_thread_native_delete_failed",
    message: string,
  ) {
    super(`${reasonCode}: ${message}`);
    this.name = "ExternalThreadLifecycleError";
  }
}

export class ExternalRuntimeCommandError extends Error {
  constructor(
    readonly reasonCode:
      | "external_command_capability_unavailable"
      | "external_command_model_invalid"
      | "external_command_effort_invalid"
      | "external_command_settings_unavailable"
      | "external_command_thread_busy"
      | "external_command_restart_failed",
    message: string,
    readonly retryable = false,
  ) {
    super(`${reasonCode}: ${message}`);
    this.name = "ExternalRuntimeCommandError";
  }
}

export class ExternalBindingMetadataError extends Error {
  constructor(
    readonly reasonCode:
      | "external_binding_not_found"
      | "external_binding_metadata_revision_conflict"
      | "external_binding_metadata_native_sync_failed"
      | "external_binding_metadata_compensation_failed",
    message: string,
    readonly retryable = false,
  ) {
    super(`${reasonCode}: ${message}`);
    this.name = "ExternalBindingMetadataError";
  }
}

class ExternalTurnDispatchError extends Error {
  constructor(
    readonly phase: "failed" | "outcome_unknown",
    readonly reasonCode:
      | "external_turn_preflight_failed"
      | "external_turn_start_outcome_unknown",
    message: string,
    cause: unknown,
  ) {
    super(message, { cause });
    this.name = "ExternalTurnDispatchError";
  }
}

export class ServiceExternalRuntimeController {
  readonly #bridge: NativeBridgeModule;
  readonly #now: () => Date;
  readonly #instanceId: string;
  readonly #onCoordinationDelivery?: (
    receipt: AgentMessageDeliveryReceipt,
  ) => Promise<AgentMessageDeliveryReceipt>;
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
    onCoordinationDelivery?: (
      receipt: AgentMessageDeliveryReceipt,
    ) => Promise<AgentMessageDeliveryReceipt>;
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
    return [...this.#controlled.values()].map((controlled) =>
      this.#status(controlled),
    );
  }

  async connect(runtimeId: string): Promise<ExternalRuntimeControllerStatus> {
    const existing = this.#controlled.get(runtimeId);
    if (existing !== undefined && existing.driver.state === "ready") {
      existing.lease = await this.#acquireLease(runtimeId);
      await this.#resumePersistedBindings(existing);
      existing.registration =
        (await this.#bridge.getExternalRuntime(runtimeId)) ??
        existing.registration;
      if (existing.registration.observedState !== "ready") {
        await this.#restoreReadyRegistration(existing);
      }
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
      bindingResumeFailures: [],
      rawDetails: new Map(),
      threadSettings: new Map(),
      threadUsage: new Map(),
      archivedThreadIds: new Set(),
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
      if (
        controlled.driver.state !== "incompatible" &&
        controlled.registration.lastCompatibilityProbe?.outcome !==
          "transport_retryable"
      ) {
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
    const input = params as Parameters<CodexAppServerDriver["threadList"]>[0];
    const result = await controlled.driver.threadList(input);
    const bindingsByThread = await this.#bindingsByThread(runtimeId);
    const items: ExternalThreadProjection[] = [];
    for (const thread of result.data) {
      if (input.archived === true) {
        controlled.archivedThreadIds.add(thread.id);
      } else {
        controlled.archivedThreadIds.delete(thread.id);
      }
      items.push(
        await this.#projectExternalThread(
          controlled,
          thread,
          input.archived === true,
          bindingsByThread.get(thread.id),
        ),
      );
    }
    return {
      items,
      nextCursor: result.nextCursor,
      backwardsCursor: result.backwardsCursor,
    };
  }

  async readThread(
    runtimeId: string,
    params: unknown,
  ): Promise<ExternalThreadReadResult> {
    const controlled = await this.#requireControlled(runtimeId);
    const input = params as Parameters<CodexAppServerDriver["threadRead"]>[0];
    const result = await controlled.driver
      .threadRead(input)
      .catch(async (error: unknown) => {
        if (input.includeTurns !== false && isUnmaterializedThreadRead(error)) {
          return controlled.driver.threadRead({
            ...input,
            includeTurns: false,
          });
        }
        throw error;
      });
    const bindingsByThread = await this.#bindingsByThread(runtimeId);
    return {
      thread: await this.#projectExternalThread(
        controlled,
        result.thread,
        false,
        bindingsByThread.get(result.thread.id),
      ),
    };
  }

  async updateBindingMetadata(input: {
    bindingId: string;
    expectedRevision: number;
    label: string | null;
    taskRef: DenRuntimeReference | null;
  }): Promise<ExternalAgentBinding> {
    const current = await this.#bridge.getExternalBinding(input.bindingId);
    if (current === undefined) {
      throw new ExternalBindingMetadataError(
        "external_binding_not_found",
        `external binding ${input.bindingId} was not found`,
      );
    }
    if (current.revision !== input.expectedRevision) {
      throw new ExternalBindingMetadataError(
        "external_binding_metadata_revision_conflict",
        `expected ${input.expectedRevision}, found ${current.revision}`,
      );
    }
    const updatedAt = this.#now().toISOString();
    let saved: ExternalAgentBinding;
    try {
      saved = await this.#bridge.updateExternalBindingMetadata({
        bindingId: input.bindingId,
        expectedRevision: input.expectedRevision,
        label: input.label,
        taskRef: input.taskRef,
        updatedAt,
      });
    } catch (error) {
      if (
        String(error).includes("external_binding_metadata_revision_conflict")
      ) {
        throw new ExternalBindingMetadataError(
          "external_binding_metadata_revision_conflict",
          String(error),
        );
      }
      throw error;
    }

    if (input.label !== null && typeof saved.nativeThreadId === "string") {
      try {
        const controlled = await this.#requireControlled(saved.runtimeId);
        await controlled.driver.threadSetName({
          threadId: saved.nativeThreadId,
          name: input.label,
        });
      } catch (error) {
        try {
          await this.#bridge.updateExternalBindingMetadata({
            bindingId: current.bindingId,
            expectedRevision: saved.revision,
            label: current.label ?? null,
            taskRef: current.taskRef ?? null,
            updatedAt: this.#now().toISOString(),
          });
        } catch (compensationError) {
          throw new ExternalBindingMetadataError(
            "external_binding_metadata_compensation_failed",
            `native label synchronization failed: ${String(error)}; durable rollback failed: ${String(compensationError)}`,
            true,
          );
        }
        throw new ExternalBindingMetadataError(
          "external_binding_metadata_native_sync_failed",
          String(error),
          true,
        );
      }
    }
    return saved;
  }

  async archiveThread(
    runtimeId: string,
    threadId: string,
  ): Promise<ExternalThreadLifecycleReceipt> {
    const controlled = await this.#requireControlled(runtimeId);
    const bindings = await this.#bindingsForThread(runtimeId, threadId);
    await this.#assertThreadHasNoCrewWork(runtimeId, threadId, bindings);
    const state = await this.#locateThread(controlled, threadId);
    if (state === undefined) {
      throw new ExternalThreadLifecycleError(
        "external_thread_not_found",
        `native thread ${threadId} was not found in runtime ${runtimeId}`,
      );
    }
    if (state !== "archived" && state.thread.status.type === "active") {
      throw new ExternalThreadLifecycleError(
        "external_thread_active",
        `native thread ${threadId} is active`,
      );
    }

    const nativeArchiveApplied = state !== "archived";
    if (nativeArchiveApplied) {
      await controlled.driver.threadArchive({ threadId });
      controlled.threadSettings.delete(threadId);
      controlled.threadUsage.delete(threadId);
    }
    const saved: Array<{
      readonly before: ExternalAgentBinding;
      readonly after: ExternalAgentBinding;
    }> = [];
    try {
      for (const binding of bindings) {
        if (binding.status === "archived") continue;
        const after = await this.#bridge.bindExternalAgent({
          binding: {
            ...binding,
            status: "archived",
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: binding.revision,
        });
        saved.push({ before: binding, after });
      }
    } catch (error) {
      const compensationFailures: string[] = [];
      for (const transition of saved.reverse()) {
        await this.#bridge
          .bindExternalAgent({
            binding: {
              ...transition.after,
              status: transition.before.status,
              updatedAt: this.#now().toISOString(),
            },
            expectedRevision: transition.after.revision,
          })
          .catch((compensationError: unknown) => {
            compensationFailures.push(String(compensationError));
          });
      }
      if (nativeArchiveApplied) {
        await controlled.driver
          .threadUnarchive({ threadId })
          .catch((compensationError: unknown) => {
            compensationFailures.push(String(compensationError));
          });
      }
      throw new ExternalThreadLifecycleError(
        "external_thread_binding_reconciliation_failed",
        `binding reconciliation failed after native archive: ${String(error)}; compensation failures: ${compensationFailures.length === 0 ? "none" : compensationFailures.join("; ")}`,
      );
    }

    controlled.archivedThreadIds.add(threadId);

    return this.#threadLifecycleReceipt(
      runtimeId,
      threadId,
      "archive",
      nativeArchiveApplied ? "applied" : "already_archived",
      true,
      bindings,
      new Map(saved.map(({ before, after }) => [before.bindingId, after])),
    );
  }

  async deleteThread(
    runtimeId: string,
    threadId: string,
  ): Promise<ExternalThreadDeleteReceipt> {
    const controlled = await this.#requireControlled(runtimeId);
    const nativeScope = await this.#threadDeletionScope(controlled, threadId);
    const scopedThreadIds = new Set(
      nativeScope.map((entry) => entry.thread.id),
    );
    scopedThreadIds.add(threadId);
    const bindings = (await this.#bridge.listExternalBindings()).filter(
      (binding) =>
        binding.runtimeId === runtimeId &&
        typeof binding.nativeThreadId === "string" &&
        scopedThreadIds.has(binding.nativeThreadId),
    );
    await this.#assertThreadsHaveNoCrewWork(
      runtimeId,
      scopedThreadIds,
      bindings,
    );
    const active = nativeScope.find(
      (entry) => !entry.archived && entry.thread.status.type === "active",
    );
    if (active !== undefined) {
      throw new ExternalThreadLifecycleError(
        "external_thread_active",
        `native thread ${active.thread.id} in deletion scope ${threadId} is active`,
      );
    }

    const saved: Array<{
      readonly before: ExternalAgentBinding;
      readonly after: ExternalAgentBinding;
    }> = [];
    try {
      for (const binding of [...bindings].sort((left, right) =>
        left.bindingId.localeCompare(right.bindingId),
      )) {
        if (binding.status === "archived") continue;
        const after = await this.#bridge.bindExternalAgent({
          binding: {
            ...binding,
            status: "archived",
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: binding.revision,
        });
        saved.push({ before: binding, after });
      }
    } catch (error) {
      const compensationFailures = await this.#restoreBindingTransitions(saved);
      throw new ExternalThreadLifecycleError(
        "external_thread_binding_reconciliation_failed",
        `binding reconciliation failed before native delete: ${String(error)}; compensation failures: ${compensationFailures.length === 0 ? "none" : compensationFailures.join("; ")}`,
      );
    }

    try {
      await controlled.driver.threadDelete({ threadId });
    } catch (error) {
      if (nativeScope.length === 0 && isMissingThreadDelete(error)) {
        return this.#threadDeleteReceipt(
          runtimeId,
          threadId,
          "already_deleted",
          bindings,
          saved,
        );
      }
      if (!(error instanceof CodexRpcError)) {
        let remaining: Set<string>;
        try {
          remaining = await this.#remainingNativeThreadIds(
            controlled,
            scopedThreadIds,
          );
        } catch (verificationError) {
          throw new ExternalThreadLifecycleError(
            "external_thread_native_delete_failed",
            `native delete failed with an ambiguous outcome: ${String(error)}; verification failed: ${String(verificationError)}; bindings remain archived`,
          );
        }
        if (remaining.size === 0) {
          return this.#threadDeleteReceipt(
            runtimeId,
            threadId,
            nativeScope.length === 0 ? "already_deleted" : "applied",
            bindings,
            saved,
          );
        }
        if (remaining.size !== scopedThreadIds.size) {
          throw new ExternalThreadLifecycleError(
            "external_thread_native_delete_failed",
            `native delete partially completed after ${String(error)}; remaining native threads: ${[...remaining].sort().join(", ")}; bindings remain archived`,
          );
        }
      }
      const compensationFailures = await this.#restoreBindingTransitions(saved);
      throw new ExternalThreadLifecycleError(
        "external_thread_native_delete_failed",
        `native delete failed: ${String(error)}; binding compensation failures: ${compensationFailures.length === 0 ? "none" : compensationFailures.join("; ")}`,
      );
    }

    return this.#threadDeleteReceipt(
      runtimeId,
      threadId,
      nativeScope.length === 0 ? "already_deleted" : "applied",
      bindings,
      saved,
    );
  }

  async unarchiveThread(
    runtimeId: string,
    threadId: string,
  ): Promise<ExternalThreadLifecycleReceipt> {
    const controlled = await this.#requireControlled(runtimeId);
    const bindings = await this.#bindingsForThread(runtimeId, threadId);
    await this.#assertThreadHasNoCrewWork(runtimeId, threadId, bindings);
    const state = await this.#locateThread(controlled, threadId);
    if (state !== undefined && state !== "archived") {
      return this.#threadLifecycleReceipt(
        runtimeId,
        threadId,
        "unarchive",
        "already_active",
        false,
        bindings,
      );
    }
    if (state === undefined) {
      throw new ExternalThreadLifecycleError(
        "external_thread_not_found",
        `native thread ${threadId} was not found in runtime ${runtimeId}`,
      );
    }
    await controlled.driver.threadUnarchive({ threadId });
    controlled.archivedThreadIds.delete(threadId);
    return this.#threadLifecycleReceipt(
      runtimeId,
      threadId,
      "unarchive",
      "applied",
      false,
      bindings,
    );
  }

  async commandCatalog(
    bindingId: string,
  ): Promise<ExternalRuntimeCommandCatalog> {
    const binding = await this.#requireCommandBinding(bindingId);
    const controlled = await this.#requireControlled(binding.runtimeId);
    const settings = await this.#effectiveThreadSettings(controlled, binding);
    const modelResult = await this.#tryModelCatalog(controlled);
    return {
      contractVersion: "0.8.0",
      runtimeId: binding.runtimeId,
      bindingId: binding.bindingId,
      nativeThreadId: binding.nativeThreadId,
      commands: EXTERNAL_RUNTIME_COMMAND_DEFINITIONS.map((definition) => {
        const requiresModels = definition.requiredCapabilities.some(
          (capability) =>
            capability === "model/list" ||
            capability === "thread/settings/update",
        );
        return {
          ...definition,
          available: !requiresModels || modelResult.reasonCode === null,
          unavailableReasonCode: requiresModels ? modelResult.reasonCode : null,
        };
      }),
      settings: projectThreadSettings(settings),
      models: modelResult.models,
    };
  }

  async executeCommand(input: {
    bindingId: string;
    commandInput: string;
    idempotencyKey: string;
    expectedBindingRevision?: number;
  }): Promise<ExternalRuntimeCommandExecutionResult> {
    const parsed = parseExternalRuntimeCommand(input.commandInput);
    const binding = await this.#requireCommandBinding(input.bindingId);
    const controlled = await this.#requireControlled(binding.runtimeId);
    const commandId = `external-command:${createHash("sha256")
      .update(`${binding.bindingId}\0${input.idempotencyKey}`)
      .digest("hex")
      .slice(0, 32)}`;
    const request: ExternalControlRequest = {
      controlId: commandId,
      idempotencyKey: input.idempotencyKey,
      bindingId: binding.bindingId,
      expectedBindingRevision:
        input.expectedBindingRevision ?? binding.revision,
      kind: "execute_thread_command",
      payload: {
        command: parsed.command,
        argument: parsed.argument,
      },
      requestedAt: this.#now().toISOString(),
    };
    let receipt = await this.#bridge.submitExternalControl(request);
    if (receipt.status !== "pending") {
      return commandExecutionResult(parsed, receipt);
    }
    await this.#recordCommandEvent(
      controlled,
      binding,
      receipt,
      "command_started",
    );
    try {
      const outcome = browserSafeNativeValue(
        await this.#applyThreadCommand(
          controlled,
          binding,
          parsed,
          receipt.request.controlId,
        ),
      );
      receipt = await this.#bridge.completeExternalControl({
        controller: this.#controllerContext(controlled),
        controlId: receipt.request.controlId,
        status: "applied",
        outcome,
        now: this.#now().toISOString(),
      });
      await this.#recordCommandEvent(
        controlled,
        binding,
        receipt,
        "command_completed",
      );
    } catch (error) {
      const commandError =
        error instanceof ExternalRuntimeCommandError
          ? error
          : codexCapabilityError(error);
      receipt = await this.#bridge.completeExternalControl({
        controller: this.#controllerContext(controlled),
        controlId: receipt.request.controlId,
        status: commandError === undefined ? "failed" : "rejected",
        reasonCode:
          commandError?.reasonCode ?? "external_command_driver_failed",
        outcome: {
          message:
            commandError?.message ?? "external command driver call failed",
        },
        now: this.#now().toISOString(),
      });
      await this.#recordCommandEvent(
        controlled,
        binding,
        receipt,
        "command_failed",
      );
    }
    return commandExecutionResult(parsed, receipt);
  }

  async createAgentSession(
    request: ExternalAgentSessionCreationRequest,
  ): Promise<ExternalAgentSessionCreateResult> {
    const controlled = await this.#requireControlled(request.runtimeId).catch(
      (error: unknown) => {
        throw new ExternalAgentSessionCreationError(
          "external_agent_creation_runtime_unavailable",
          String(error),
          true,
        );
      },
    );
    let creation =
      await this.#bridge.prepareExternalAgentSessionCreation(request);
    if (creation.phase === "ready") {
      return this.#externalAgentSessionCreationResult(controlled, creation);
    }

    let nativeThreadId: string | undefined;
    try {
      creation = await this.#bridge.markExternalAgentSessionNativeStarting({
        controller: this.#controllerContext(controlled),
        creationId: creation.creationId,
        expectedRevision: creation.revision,
        now: this.#now().toISOString(),
      });
      const developerInstructions = await this.#developerInstructionsForBinding(
        creation.binding,
      );
      const recovered = await this.#findThreadBySource(
        controlled,
        creation.nativeThreadSource,
      );
      if (recovered !== undefined) {
        nativeThreadId = recovered.id;
        const resumed = await controlled.driver.threadResume({
          threadId: recovered.id,
          cwd: creation.request.cwd,
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          excludeTurns: true,
          developerInstructions,
        });
        controlled.threadSettings.set(recovered.id, {
          model: resumed.model,
          modelProvider: resumed.modelProvider,
          reasoning_effort: resumed.reasoningEffort,
          developer_instructions: developerInstructions,
        });
      } else {
        const cwd = creation.request.cwd;
        const started = await controlled.driver.threadStart({
          cwd,
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          ephemeral: false,
          environments: [{ environmentId: "local", cwd }],
          dynamicTools: [...CODEX_COORDINATION_DYNAMIC_TOOLS],
          threadSource: creation.nativeThreadSource,
          developerInstructions,
        });
        nativeThreadId = started.thread.id;
        controlled.threadSettings.set(nativeThreadId, {
          model: started.model,
          modelProvider: started.modelProvider,
          reasoning_effort: started.reasoningEffort,
          developer_instructions: developerInstructions,
        });
      }
      creation = await this.#bridge.completeExternalAgentSessionCreation({
        controller: this.#controllerContext(controlled),
        creationId: creation.creationId,
        expectedRevision: creation.revision,
        nativeThreadId,
        now: this.#now().toISOString(),
      });
      return this.#externalAgentSessionCreationResult(controlled, creation);
    } catch (error) {
      const failureReason = externalAgentSessionCreationFailureReason(
        error,
        nativeThreadId,
      );
      const reconciled = await this.#bridge
        .prepareExternalAgentSessionCreation(request)
        .catch(() => undefined);
      if (reconciled?.phase === "ready") {
        return this.#externalAgentSessionCreationResult(controlled, reconciled);
      }
      if (reconciled !== undefined) {
        await this.#bridge
          .recordExternalAgentSessionCreationFailure({
            controller: this.#controllerContext(controlled),
            creationId: reconciled.creationId,
            expectedRevision: reconciled.revision,
            reasonCode: failureReason,
            reasonMessage: String(error),
            now: this.#now().toISOString(),
          })
          .catch(() => undefined);
      }
      throw new ExternalAgentSessionCreationError(
        failureReason,
        String(error),
        true,
      );
    }
  }

  async refreshProfileInstructions(profileId: string): Promise<{
    profileId: string;
    profileRevision: number;
    refreshedBindings: string[];
    unchangedBindings: string[];
  }> {
    const profile = await this.#profileDeveloperInstructions(profileId);
    const bindings = (await this.#bridge.listExternalBindings()).filter(
      (binding) =>
        binding.purpose === "crew_agent" &&
        binding.status === "active" &&
        binding.profileId === profileId,
    );
    const refreshedBindings: string[] = [];
    const unchangedBindings: string[] = [];
    for (const binding of bindings) {
      if (
        binding.profileRevision === profile.revision &&
        binding.profilePromptHash === profile.promptHash
      ) {
        unchangedBindings.push(binding.bindingId);
        continue;
      }
      if (typeof binding.nativeThreadId !== "string") {
        throw new ExternalAgentSessionCreationError(
          "external_agent_creation_recovery_required",
          `external binding ${binding.bindingId} has no native thread to refresh`,
          true,
        );
      }
      const controlled = await this.#requireControlled(binding.runtimeId);
      await this.#assertThreadHasNoCrewWork(
        binding.runtimeId,
        binding.nativeThreadId,
        [binding],
      );
      const previousThreadId = binding.nativeThreadId;
      const forked = await controlled.driver.threadFork({
        threadId: previousThreadId,
        ...(typeof binding.cwd === "string" ? { cwd: binding.cwd } : {}),
        approvalPolicy: "never",
        sandbox: "danger-full-access",
        developerInstructions: profile.developerInstructions,
        excludeTurns: true,
      });
      const nextThreadId = forked.thread.id;
      try {
        await this.#bridge.bindExternalAgent({
          binding: {
            ...binding,
            nativeThreadId: nextThreadId,
            profileRevision: profile.revision,
            profilePromptHash: profile.promptHash,
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: binding.revision,
        });
      } catch (error) {
        await controlled.driver
          .threadDelete({ threadId: nextThreadId })
          .catch(() => undefined);
        throw error;
      }
      controlled.threadSettings.delete(previousThreadId);
      controlled.threadUsage.delete(previousThreadId);
      controlled.threadSettings.set(nextThreadId, {
        model: forked.model,
        modelProvider: forked.modelProvider,
        reasoning_effort: forked.reasoningEffort,
        developer_instructions: profile.developerInstructions,
      });
      if (typeof binding.label === "string") {
        await controlled.driver.threadSetName({
          threadId: nextThreadId,
          name: binding.label,
        });
      }
      await controlled.driver
        .threadArchive({ threadId: previousThreadId })
        .then(() => controlled.archivedThreadIds.add(previousThreadId))
        .catch(() => undefined);
      refreshedBindings.push(binding.bindingId);
    }
    return {
      profileId,
      profileRevision: profile.revision,
      refreshedBindings,
      unchangedBindings,
    };
  }

  async applyCoordinationDelivery(
    receipt: AgentMessageDeliveryReceipt,
  ): Promise<AgentMessageDeliveryReceipt> {
    if (
      receipt.status !== "pending" ||
      receipt.activation?.type !== "external_turn_steer_requested"
    ) {
      return receipt;
    }
    const completedAt = this.#now().toISOString();
    if (receipt.request.expiresAt <= completedAt) {
      return this.#bridge.completeAgentMessageDelivery({
        deliveryId: receipt.request.deliveryId,
        expectedRevision: receipt.revision,
        status: "expired",
        reasonCode: "agent_message_expired_before_steer",
        completedAt,
      });
    }
    const activation = receipt.activation;
    const binding = await this.#bridge.getExternalBinding(activation.bindingId);
    const activeTurn = (await this.#bridge.listActiveExternalTurns()).find(
      (candidate) =>
        candidate.request.requestId === activation.requestId &&
        candidate.nativeThreadId === activation.nativeThreadId &&
        candidate.nativeTurnId === activation.nativeTurnId &&
        candidate.phase === "active",
    );
    if (binding === undefined || activeTurn === undefined) {
      return this.#bridge.completeAgentMessageDelivery({
        deliveryId: receipt.request.deliveryId,
        expectedRevision: receipt.revision,
        status: "rejected",
        reasonCode: "external_turn_steer_precondition_changed",
        completedAt,
      });
    }
    const controlled = await this.#requireControlled(binding.runtimeId);
    try {
      await controlled.driver.turnSteer({
        threadId: activation.nativeThreadId,
        expectedTurnId: activation.nativeTurnId,
        clientUserMessageId: `rusty-crew:${receipt.request.messageId}`,
        input: [
          {
            type: "text" as const,
            text: activation.messageText,
            text_elements: [],
          },
        ],
      });
      return this.#bridge.completeAgentMessageDelivery({
        deliveryId: receipt.request.deliveryId,
        expectedRevision: receipt.revision,
        status: "accepted",
        reasonCode: "external_turn_steer_accepted",
        completedAt: this.#now().toISOString(),
      });
    } catch (error) {
      return this.#bridge.completeAgentMessageDelivery({
        deliveryId: receipt.request.deliveryId,
        expectedRevision: receipt.revision,
        status: "rejected",
        reasonCode:
          error instanceof CodexRpcError
            ? "external_turn_steer_rejected"
            : "external_turn_steer_outcome_unknown",
        completedAt: this.#now().toISOString(),
      });
    }
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
          controlled.driver.state === "disconnected" ||
          (controlled.driver.state === "ready" &&
            registration.observedState !== "ready")
        ) {
          await this.connect(registration.runtimeId).catch(() => undefined);
          continue;
        }
        controlled.registration = registration;
        controlled.lease = await this.#acquireLease(registration.runtimeId);
      }
      await this.#bridge.expireExternalTurnDispatches(
        this.#now().toISOString(),
      );
      await this.#dispatchAcceptedTurns();
    } finally {
      this.#ticking = false;
    }
  }

  async #dispatchAcceptedTurns(): Promise<void> {
    const turns = await this.#bridge.listActiveExternalTurns();
    for (const turn of turns) {
      if (turn.phase !== "accepted") continue;
      const controlled = this.#controlled.get(turn.runtimeId);
      if (controlled === undefined || controlled.driver.state !== "ready") {
        continue;
      }
      const binding = await this.#bridge.getExternalBinding(
        turn.request.bindingId,
      );
      if (binding === undefined || binding.status !== "active") {
        await this.#terminalizeDispatchFailure(
          controlled,
          turn,
          binding === undefined
            ? "external_turn_binding_missing"
            : "external_turn_binding_not_active",
          binding === undefined
            ? `external binding ${turn.request.bindingId} was not found`
            : `external binding ${turn.request.bindingId} is ${binding.status}`,
          "failed",
        );
        continue;
      }
      await this.#startAcceptedTurn(controlled, binding, turn).catch(
        async (error) => {
          const dispatchError =
            error instanceof ExternalTurnDispatchError ? error : undefined;
          await this.#terminalizeDispatchFailure(
            controlled,
            turn,
            dispatchError?.reasonCode ?? "external_turn_start_failed",
            error instanceof Error ? error.message : String(error),
            dispatchError?.phase ?? "failed",
          );
        },
      );
    }
  }

  async #terminalizeDispatchFailure(
    controlled: ControlledRuntime,
    turn: ExternalTurnCorrelation,
    reasonCode: string,
    message: string,
    phase: "failed" | "outcome_unknown",
  ): Promise<void> {
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: phase,
      terminalReasonCode: reasonCode,
      now: this.#now().toISOString(),
    });
    await this.#bridge
      .recordExternalRuntimeEvent({
        controller: this.#controllerContext(controlled),
        event: {
          eventId: `${controlled.registration.runtimeId}:controller:${randomUUID()}`,
          sessionId: turn.request.sessionId,
          createdAt: this.#now().toISOString(),
          kind: "runtime_status",
          runtimeId: controlled.registration.runtimeId,
          nativeThreadId: turn.nativeThreadId,
          requestId: turn.request.requestId,
          payload: {
            nativeMethod: "rustyCrew/externalTurnDispatchFailed",
            status: reasonCode,
            message: message.slice(0, 2_000),
          },
        },
      })
      .catch(() => undefined);
  }

  async #resumePersistedBindings(controlled: ControlledRuntime): Promise<void> {
    controlled.bindingResumeFailures = [];
    const bindings = await this.#bridge.listExternalBindings();
    for (const binding of bindings) {
      if (
        binding.runtimeId !== controlled.registration.runtimeId ||
        binding.status !== "active" ||
        typeof binding.nativeThreadId !== "string"
      ) {
        continue;
      }
      try {
        const promptContext = await this.#bindingPromptContext(binding);
        const currentBinding = promptContext.binding;
        if (typeof currentBinding.nativeThreadId !== "string") {
          throw new ExternalAgentSessionCreationError(
            "external_agent_creation_profile_invalid",
            `external binding ${currentBinding.bindingId} lost its native thread during profile repair`,
            false,
          );
        }
        const nativeThreadId = currentBinding.nativeThreadId;
        const resumed = await controlled.driver.threadResume({
          threadId: nativeThreadId,
          ...(typeof currentBinding.cwd === "string"
            ? { cwd: currentBinding.cwd }
            : {}),
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          excludeTurns: true,
          developerInstructions: promptContext.developerInstructions,
        });
        controlled.threadSettings.set(nativeThreadId, {
          model: resumed.model,
          modelProvider: resumed.modelProvider,
          reasoning_effort: resumed.reasoningEffort,
          developer_instructions: promptContext.developerInstructions,
        });
        if (typeof currentBinding.label === "string") {
          await controlled.driver.threadSetName({
            threadId: nativeThreadId,
            name: currentBinding.label,
          });
        }
      } catch (error) {
        controlled.bindingResumeFailures.push({
          bindingId: binding.bindingId,
          nativeThreadId: binding.nativeThreadId,
          reason: String(error),
          observedAt: this.#now().toISOString(),
        });
      }
    }
  }

  async #developerInstructionsForBinding(
    binding: ExternalAgentBinding,
  ): Promise<string | null> {
    return (await this.#bindingPromptContext(binding)).developerInstructions;
  }

  async #bindingPromptContext(binding: ExternalAgentBinding): Promise<{
    binding: ExternalAgentBinding;
    developerInstructions: string | null;
  }> {
    if (
      binding.profileId == null &&
      binding.profileRevision == null &&
      binding.profilePromptHash == null
    ) {
      if (typeof binding.sessionId !== "string") {
        throw new ExternalAgentSessionCreationError(
          "external_agent_creation_profile_invalid",
          `external binding ${binding.bindingId} has no Crew session for profile repair`,
          false,
        );
      }
      const session = (await this.#bridge.listSessions()).find(
        (candidate) => candidate.sessionId === binding.sessionId,
      );
      if (session === undefined) {
        throw new ExternalAgentSessionCreationError(
          "external_agent_creation_profile_invalid",
          `external binding ${binding.bindingId} Crew session ${binding.sessionId} was not found`,
          false,
        );
      }
      const profile = await this.#profileDeveloperInstructions(
        session.profileId,
      );
      const repaired = await this.#bridge.bindExternalAgent({
        binding: {
          ...binding,
          profileId: session.profileId,
          profileRevision: profile.revision,
          profilePromptHash: profile.promptHash,
          updatedAt: this.#now().toISOString(),
        },
        expectedRevision: binding.revision,
      });
      return {
        binding: repaired,
        developerInstructions: profile.developerInstructions,
      };
    }
    if (
      typeof binding.profileId !== "string" ||
      typeof binding.profileRevision !== "number" ||
      typeof binding.profilePromptHash !== "string"
    ) {
      throw new ExternalAgentSessionCreationError(
        "external_agent_creation_profile_invalid",
        `external binding ${binding.bindingId} has no profile prompt provenance`,
        false,
      );
    }
    const profile = await this.#profileDeveloperInstructions(binding.profileId);
    if (
      profile.revision !== binding.profileRevision ||
      profile.promptHash !== binding.profilePromptHash
    ) {
      throw new ExternalAgentSessionCreationError(
        "external_agent_creation_profile_revision_conflict",
        `profile ${binding.profileId} revision or prompt hash changed; refresh the bound Codex thread`,
        false,
      );
    }
    return { binding, developerInstructions: profile.developerInstructions };
  }

  async #profileDeveloperInstructions(profileId: string): Promise<{
    revision: number;
    promptHash: string;
    developerInstructions: string | null;
  }> {
    const profile = await this.#bridge.getProfileRegistryRecord(profileId);
    if (profile === undefined || profile.lifecycleStatus !== "active") {
      throw new ExternalAgentSessionCreationError(
        "external_agent_creation_profile_invalid",
        `profile ${profileId} is missing or inactive`,
        false,
      );
    }
    const soul = profile.promptSoulMarkdown?.trim() ?? "";
    return {
      revision: profile.revision,
      promptHash: createHash("sha256").update(soul).digest("hex"),
      developerInstructions: soul === "" ? null : soul,
    };
  }

  async #restoreReadyRegistration(
    controlled: ControlledRuntime,
  ): Promise<void> {
    const identity = controlled.handshakeIdentity;
    if (identity === undefined) {
      throw new Error(
        `external runtime ${controlled.registration.runtimeId} has no accepted handshake identity`,
      );
    }
    const probeReport =
      controlled.driver.lastCompatibilityProbe ??
      controlled.registration.lastCompatibilityProbe;
    if (probeReport == null) {
      throw new Error(
        `external runtime ${controlled.registration.runtimeId} has no compatibility probe report`,
      );
    }
    const decision = await this.#authorizeHandshake(
      controlled,
      identity,
      probeReport,
    );
    if (!decision.accepted) {
      throw new Error(
        decision.reasonCode ??
          `external runtime ${controlled.registration.runtimeId} handshake reconciliation failed`,
      );
    }
  }

  async #findThreadBySource(
    controlled: ControlledRuntime,
    threadSource: string,
    match: "exact" | "prefix" = "exact",
  ) {
    let cursor: string | null = null;
    for (let page = 0; page < 100; page += 1) {
      const result = await controlled.driver.threadList({
        cursor,
        limit: 100,
        sortKey: "created_at",
        sortDirection: "desc",
        archived: false,
        useStateDbOnly: true,
      });
      const found = result.data.find(
        (candidate) =>
          candidate.threadSource === threadSource ||
          (match === "prefix" &&
            candidate.threadSource?.startsWith(threadSource)),
      );
      if (found !== undefined) return found;
      cursor = result.nextCursor;
      if (cursor === null) return undefined;
    }
    throw new ExternalAgentSessionCreationError(
      "external_agent_creation_recovery_required",
      "Codex threadSource recovery exceeded the bounded thread listing window",
      true,
    );
  }

  async #externalAgentSessionCreationResult(
    controlled: ControlledRuntime,
    creation: ExternalAgentSessionCreationRecord,
  ): Promise<ExternalAgentSessionCreateResult> {
    if (
      creation.phase !== "ready" ||
      typeof creation.nativeThreadId !== "string"
    ) {
      throw new ExternalAgentSessionCreationError(
        "external_agent_creation_recovery_required",
        "external agent session creation is not ready",
        true,
      );
    }
    const currentBinding = await this.#bridge.getExternalBinding(
      creation.binding.bindingId,
    );
    if (currentBinding === undefined) {
      throw new ExternalAgentSessionCreationError(
        "external_agent_creation_recovery_required",
        `external binding ${creation.binding.bindingId} was not found`,
        true,
      );
    }
    const requestedLabel = creation.request.label ?? null;
    const requestedTaskRef = creation.request.taskRef ?? null;
    const binding = bindingMetadataMatches(
      currentBinding,
      requestedLabel,
      requestedTaskRef,
    )
      ? currentBinding
      : await this.updateBindingMetadata({
          bindingId: currentBinding.bindingId,
          expectedRevision: currentBinding.revision,
          label: requestedLabel,
          taskRef: requestedTaskRef,
        });
    if (
      requestedLabel !== null &&
      bindingMetadataMatches(currentBinding, requestedLabel, requestedTaskRef)
    ) {
      await controlled.driver.threadSetName({
        threadId: creation.nativeThreadId,
        name: requestedLabel,
      });
    }
    const read = await controlled.driver
      .threadRead({
        threadId: creation.nativeThreadId,
        includeTurns: false,
      })
      .catch((error: unknown) => {
        throw new ExternalAgentSessionCreationError(
          "external_agent_creation_recovery_required",
          `native thread projection failed: ${String(error)}`,
          true,
        );
      });
    return {
      creation: { ...creation, binding },
      runtime: controlled.registration,
      thread: await this.#projectExternalThread(
        controlled,
        read.thread,
        false,
        binding,
      ),
    };
  }

  async #startAcceptedTurn(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding,
    turn: ExternalTurnCorrelation,
  ): Promise<void> {
    let collaborationMode: CollaborationMode;
    try {
      await this.#developerInstructionsForBinding(binding);
      collaborationMode = await this.#resolveCollaborationMode(
        controlled,
        binding,
        turn.request.collaborationMode ?? "default",
      );
    } catch (error) {
      throw new ExternalTurnDispatchError(
        "failed",
        "external_turn_preflight_failed",
        error instanceof Error ? error.message : String(error),
        error,
      );
    }
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: "starting",
      now: this.#now().toISOString(),
    });
    try {
      const cwd = binding.cwd ?? "/home";
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
        cwd,
        environments: [{ environmentId: "local", cwd }],
        approvalPolicy: "never",
        sandboxPolicy: { type: "dangerFullAccess" },
        collaborationMode,
      });
      await this.#bridge.transitionExternalTurn({
        controller: this.#controllerContext(controlled),
        requestId: turn.request.requestId,
        nextPhase: "active",
        nativeTurnId: started.turn.id,
        now: this.#now().toISOString(),
      });
    } catch (error) {
      throw new ExternalTurnDispatchError(
        "outcome_unknown",
        "external_turn_start_outcome_unknown",
        error instanceof Error ? error.message : String(error),
        error,
      );
    }
  }

  async #resolveCollaborationMode(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding,
    mode: CollaborationMode["mode"],
  ) {
    const presets = await controlled.driver.collaborationModeList();
    const preset = presets.data.find((candidate) => candidate.mode === mode);
    if (preset === undefined) {
      throw new Error(
        `Codex app-server did not advertise the ${mode} collaboration preset`,
      );
    }
    const current = controlled.threadSettings.get(binding.nativeThreadId ?? "");
    const model = preset.model ?? current?.model;
    if (model === undefined) {
      throw new Error(`Codex ${mode} mode could not resolve the thread model`);
    }
    return {
      mode,
      settings: {
        model,
        reasoning_effort:
          preset.reasoning_effort ?? current?.reasoning_effort ?? null,
        developer_instructions: current?.developer_instructions ?? null,
      },
    };
  }

  async #modelCatalog(
    controlled: ControlledRuntime,
  ): Promise<ExternalRuntimeModelOption[]> {
    const models: ExternalRuntimeModelOption[] = [];
    const seenCursors = new Set<string>();
    let cursor: string | null = null;
    for (let page = 0; page < 100; page += 1) {
      const response = await controlled.driver.modelList({
        cursor,
        limit: 100,
        includeHidden: true,
      });
      models.push(...response.data.map(projectExternalRuntimeModel));
      if (response.nextCursor === null) return models;
      if (seenCursors.has(response.nextCursor)) {
        throw new ExternalRuntimeCommandError(
          "external_command_capability_unavailable",
          "Codex model/list returned a repeated pagination cursor",
          true,
        );
      }
      seenCursors.add(response.nextCursor);
      cursor = response.nextCursor;
    }
    throw new ExternalRuntimeCommandError(
      "external_command_capability_unavailable",
      "Codex model/list exceeded the 100-page safety bound",
      true,
    );
  }

  async #tryModelCatalog(controlled: ControlledRuntime): Promise<{
    models: ExternalRuntimeModelOption[];
    reasonCode: string | null;
  }> {
    try {
      return { models: await this.#modelCatalog(controlled), reasonCode: null };
    } catch {
      return {
        models: [],
        reasonCode: "external_command_capability_unavailable",
      };
    }
  }

  async #effectiveThreadSettings(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding & { nativeThreadId: string },
  ): Promise<ControlledThreadSettings> {
    const current = controlled.threadSettings.get(binding.nativeThreadId);
    if (current !== undefined) return current;
    try {
      const resumed = await controlled.driver.threadResume({
        threadId: binding.nativeThreadId,
        excludeTurns: true,
      });
      const settings = threadSettingsFromResume(resumed);
      controlled.threadSettings.set(binding.nativeThreadId, settings);
      return settings;
    } catch (error) {
      throw new ExternalRuntimeCommandError(
        "external_command_settings_unavailable",
        `Codex thread settings could not be read: ${String(error)}`,
        true,
      );
    }
  }

  async #projectExternalThread(
    controlled: ControlledRuntime,
    value: unknown,
    forceUnavailable = false,
    binding?: ExternalAgentBinding,
  ): Promise<ExternalThreadProjection> {
    const thread = requireNativeRecord(value, "thread");
    const threadId = requireNativeString(thread.id, "thread.id");
    const status = projectNativeStatus(thread.status);
    const loaded =
      status === "idle" || status === "active" || status === "systemError";
    let effectiveModel: string | null = null;
    if (
      !forceUnavailable &&
      !controlled.archivedThreadIds.has(threadId) &&
      loaded
    ) {
      let settings = controlled.threadSettings.get(threadId);
      if (settings === undefined) {
        try {
          const resumed = await controlled.driver.threadResume({
            threadId,
            excludeTurns: true,
          });
          settings = threadSettingsFromResume(resumed);
        } catch {
          settings = undefined;
        }
      } else {
        settings = {
          ...settings,
          modelProvider:
            nativeString(thread.modelProvider) ?? settings.modelProvider,
        };
      }
      if (settings !== undefined) {
        controlled.threadSettings.set(threadId, settings);
        effectiveModel = settings.model;
      }
    }
    const projected = projectExternalThread(
      thread,
      effectiveModel,
      binding === undefined ? undefined : (binding.label ?? null),
    );
    if (projected.turns.length === 0) return projected;
    return reconcileExternalThreadProjection(
      projected,
      await this.#bridge.listExternalTurnsForNativeThread(
        controlled.registration.runtimeId,
        threadId,
      ),
    );
  }

  async #bindingsByThread(
    runtimeId: string,
  ): Promise<Map<string, ExternalAgentBinding>> {
    const result = new Map<string, ExternalAgentBinding>();
    for (const binding of await this.#bridge.listExternalBindings()) {
      if (
        binding.runtimeId !== runtimeId ||
        typeof binding.nativeThreadId !== "string"
      ) {
        continue;
      }
      const existing = result.get(binding.nativeThreadId);
      if (
        existing === undefined ||
        (existing.purpose !== "crew_agent" &&
          binding.purpose === "crew_agent") ||
        (binding.purpose === existing.purpose &&
          binding.revision > existing.revision)
      ) {
        result.set(binding.nativeThreadId, binding);
      }
    }
    return result;
  }

  async #applyThreadCommand(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding & { nativeThreadId: string },
    command: ParsedExternalRuntimeCommand,
    controlId: string,
  ): Promise<{ message: string; result: ExternalRuntimeCommandResultData }> {
    switch (command.command) {
      case "help":
      case "commands": {
        const catalog = await this.commandCatalog(binding.bindingId);
        return {
          message: catalog.commands
            .map((entry) => `${entry.usage} - ${entry.description}`)
            .join("\n"),
          result: { catalog },
        };
      }
      case "status": {
        const status = await this.#threadCommandStatus(controlled, binding);
        const usage = status.usage;
        return {
          message: [
            `Runtime: ${status.runtimeId} (${status.controller.driverState})`,
            `Thread: ${status.nativeThreadId}`,
            `Model: ${status.settings.model} via ${status.settings.modelProvider}`,
            `Effort: ${status.settings.effort ?? "default"}`,
            `Active turn: ${status.activeNativeTurnId ?? "none"}`,
            usage === null
              ? "Context usage: not reported yet"
              : `Context usage: ${usage.total.totalTokens ?? 0}/${usage.modelContextWindow ?? "unknown"}${usage.contextWindowUsedPercent === null ? "" : ` (${usage.contextWindowUsedPercent.toFixed(1)}%)`}`,
          ].join("\n"),
          result: { status },
        };
      }
      case "new":
      case "restart":
        return this.#replaceNativeThread(controlled, binding, controlId);
      case "model": {
        const settings = await this.#effectiveThreadSettings(
          controlled,
          binding,
        );
        const models = await this.#modelCatalog(controlled);
        if (command.argument === null) {
          return {
            message: `Current model: ${settings.model}\nAvailable models: ${models.map((model) => model.id).join(", ")}`,
            result: { settings: projectThreadSettings(settings), models },
          };
        }
        const selected = models.find(
          (model) =>
            model.id === command.argument || model.model === command.argument,
        );
        if (selected === undefined) {
          throw new ExternalRuntimeCommandError(
            "external_command_model_invalid",
            `model ${command.argument} is not in the live Codex model catalog`,
          );
        }
        const selectedEffort = selected.supportedEfforts.some(
          (effort) => effort.value === settings.reasoning_effort,
        )
          ? settings.reasoning_effort
          : selected.defaultEffort;
        await controlled.driver.threadSettingsUpdate({
          threadId: binding.nativeThreadId,
          model: selected.model,
          effort: selectedEffort,
        });
        const readback = await this.#refreshThreadSettings(
          controlled,
          binding.nativeThreadId,
          {
            model: selected.model,
            effort: selectedEffort,
          },
        );
        return {
          message: `Model set to ${readback.model}; effort is ${readback.reasoning_effort ?? "default"}.`,
          result: { settings: projectThreadSettings(readback), models },
        };
      }
      case "effort": {
        const settings = await this.#effectiveThreadSettings(
          controlled,
          binding,
        );
        const models = await this.#modelCatalog(controlled);
        const selectedModel = models.find(
          (model) =>
            model.id === settings.model || model.model === settings.model,
        );
        if (selectedModel === undefined) {
          throw new ExternalRuntimeCommandError(
            "external_command_model_invalid",
            `current model ${settings.model} is not in the live Codex model catalog`,
          );
        }
        if (command.argument === null) {
          return {
            message: `Current effort: ${settings.reasoning_effort ?? "default"}\nValid efforts: ${selectedModel.supportedEfforts.map((effort) => effort.value).join(", ")}`,
            result: {
              settings: projectThreadSettings(settings),
              validEfforts: selectedModel.supportedEfforts,
            },
          };
        }
        if (
          !selectedModel.supportedEfforts.some(
            (effort) => effort.value === command.argument,
          )
        ) {
          throw new ExternalRuntimeCommandError(
            "external_command_effort_invalid",
            `effort ${command.argument} is not supported by ${selectedModel.id}`,
          );
        }
        await controlled.driver.threadSettingsUpdate({
          threadId: binding.nativeThreadId,
          effort: command.argument,
        });
        const readback = await this.#refreshThreadSettings(
          controlled,
          binding.nativeThreadId,
          { effort: command.argument },
        );
        return {
          message: `Reasoning effort set to ${readback.reasoning_effort ?? "default"}.`,
          result: {
            settings: projectThreadSettings(readback),
            validEfforts: selectedModel.supportedEfforts,
          },
        };
      }
      case "compact": {
        const nativeResult = await controlled.driver.compactThread({
          threadId: binding.nativeThreadId,
        });
        return {
          message: "Native Codex compaction started.",
          result: { nativeResult },
        };
      }
    }
  }

  async #replaceNativeThread(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding & { nativeThreadId: string },
    controlId: string,
  ): Promise<{ message: string; result: ExternalRuntimeCommandResultData }> {
    try {
      await this.#assertThreadHasNoCrewWork(
        binding.runtimeId,
        binding.nativeThreadId,
        [binding],
      );
    } catch (error) {
      throw new ExternalRuntimeCommandError(
        "external_command_thread_busy",
        error instanceof Error ? error.message : String(error),
        true,
      );
    }

    let promptContext: {
      binding: ExternalAgentBinding;
      developerInstructions: string | null;
    };
    try {
      promptContext = await this.#bindingPromptContext(binding);
    } catch (error) {
      throw new ExternalRuntimeCommandError(
        "external_command_restart_failed",
        error instanceof Error ? error.message : String(error),
        true,
      );
    }
    const currentBinding = promptContext.binding;
    if (typeof currentBinding.nativeThreadId !== "string") {
      throw new ExternalRuntimeCommandError(
        "external_command_restart_failed",
        `external binding ${currentBinding.bindingId} lost its native thread during profile repair`,
        true,
      );
    }
    const currentNativeThreadId = currentBinding.nativeThreadId;
    let previousNativeThreadId = currentNativeThreadId;
    const cwd = currentBinding.cwd ?? "/home";
    const developerInstructions = promptContext.developerInstructions;
    const previousSettings =
      controlled.threadSettings.get(previousNativeThreadId) ??
      (await this.#effectiveThreadSettings(controlled, {
        ...currentBinding,
        nativeThreadId: currentNativeThreadId,
      }).catch(() => undefined));
    const threadSourcePrefix = `rusty-crew:command:${controlId}:replace:`;
    const threadSource = `${threadSourcePrefix}${previousNativeThreadId}`;
    let nativeThreadId: string | undefined;
    let nextSettings: ControlledThreadSettings | undefined;
    let rebound: ExternalAgentBinding | undefined;
    let bindingAlreadyRebound = false;
    let candidateMayBeDeleted = false;

    try {
      const recovered = await this.#findThreadBySource(
        controlled,
        threadSourcePrefix,
        "prefix",
      );
      if (recovered === undefined) {
        const started = await controlled.driver.threadStart({
          ...(previousSettings === undefined
            ? {}
            : {
                model: previousSettings.model,
                modelProvider: previousSettings.modelProvider,
                ...(previousSettings.reasoning_effort === null
                  ? {}
                  : {
                      config: {
                        model_reasoning_effort:
                          previousSettings.reasoning_effort,
                      },
                    }),
              }),
          cwd,
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          ephemeral: false,
          environments: [{ environmentId: "local", cwd }],
          dynamicTools: [...CODEX_COORDINATION_DYNAMIC_TOOLS],
          threadSource,
          developerInstructions,
        });
        nativeThreadId = started.thread.id;
        candidateMayBeDeleted = true;
        nextSettings = {
          model: started.model,
          modelProvider: started.modelProvider,
          reasoning_effort: started.reasoningEffort,
          developer_instructions: developerInstructions,
        };
      } else {
        if (
          typeof recovered.threadSource !== "string" ||
          !recovered.threadSource.startsWith(threadSourcePrefix)
        ) {
          throw new Error(
            `recovered Codex thread ${recovered.id} has invalid replacement provenance`,
          );
        }
        previousNativeThreadId = recovered.threadSource.slice(
          threadSourcePrefix.length,
        );
        if (previousNativeThreadId.length === 0) {
          throw new Error(
            `recovered Codex thread ${recovered.id} has no replaced thread identity`,
          );
        }
        nativeThreadId = recovered.id;
        bindingAlreadyRebound = currentBinding.nativeThreadId === recovered.id;
        candidateMayBeDeleted = !bindingAlreadyRebound;
        const resumed = await controlled.driver.threadResume({
          threadId: recovered.id,
          cwd,
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          excludeTurns: true,
          developerInstructions,
        });
        nextSettings = threadSettingsFromResume(resumed);
      }

      if (
        previousSettings !== undefined &&
        nextSettings.modelProvider !== previousSettings.modelProvider
      ) {
        throw new Error(
          `fresh Codex thread selected provider ${nextSettings.modelProvider}; expected ${previousSettings.modelProvider}`,
        );
      }
      if (
        previousSettings !== undefined &&
        (nextSettings.model !== previousSettings.model ||
          nextSettings.reasoning_effort !== previousSettings.reasoning_effort)
      ) {
        await controlled.driver.threadSettingsUpdate({
          threadId: nativeThreadId,
          model: previousSettings.model,
          effort: previousSettings.reasoning_effort,
        });
        nextSettings = await this.#refreshThreadSettings(
          controlled,
          nativeThreadId,
          {
            model: previousSettings.model,
            effort: previousSettings.reasoning_effort,
          },
        );
      }
      if (typeof currentBinding.label === "string") {
        await controlled.driver.threadSetName({
          threadId: nativeThreadId,
          name: currentBinding.label,
        });
      }
      if (bindingAlreadyRebound) {
        rebound = currentBinding;
      } else {
        rebound = await this.#bridge.bindExternalAgent({
          binding: {
            ...currentBinding,
            nativeThreadId,
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: currentBinding.revision,
        });
        candidateMayBeDeleted = false;
      }
    } catch (error) {
      if (nativeThreadId !== undefined && candidateMayBeDeleted) {
        await controlled.driver
          .threadDelete({ threadId: nativeThreadId })
          .catch(() => undefined);
      }
      throw new ExternalRuntimeCommandError(
        "external_command_restart_failed",
        error instanceof Error ? error.message : String(error),
        true,
      );
    }

    controlled.threadSettings.delete(previousNativeThreadId);
    controlled.threadUsage.delete(previousNativeThreadId);
    controlled.threadSettings.set(nativeThreadId, nextSettings);
    const previousNativeThreadArchived =
      previousNativeThreadId === nativeThreadId
        ? false
        : await controlled.driver
            .threadArchive({ threadId: previousNativeThreadId })
            .then(() => {
              controlled.archivedThreadIds.add(previousNativeThreadId);
              return true;
            })
            .catch(() => false);
    return {
      message: `Started a fresh Codex thread for ${currentBinding.label ?? currentBinding.bindingId}.`,
      result: {
        threadReplacement: {
          bindingId: rebound.bindingId,
          bindingRevision: rebound.revision,
          sessionId: rebound.sessionId ?? null,
          profileId: rebound.profileId ?? null,
          cwd,
          label: rebound.label ?? null,
          taskRef: rebound.taskRef ?? null,
          previousNativeThreadId,
          nativeThreadId,
          previousNativeThreadArchived,
          settingsPreserved: previousSettings !== undefined,
          settings: projectThreadSettings(nextSettings),
        },
      },
    };
  }

  async #refreshThreadSettings(
    controlled: ControlledRuntime,
    threadId: string,
    expected: {
      readonly model?: string;
      readonly effort?: string | null;
    },
  ): Promise<ControlledThreadSettings> {
    const deadline = Date.now() + 5_000;
    while (Date.now() < deadline) {
      const settings = controlled.threadSettings.get(threadId);
      if (
        settings !== undefined &&
        (expected.model === undefined || settings.model === expected.model) &&
        (expected.effort === undefined ||
          settings.reasoning_effort === expected.effort)
      ) {
        return settings;
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    throw new ExternalRuntimeCommandError(
      "external_command_settings_unavailable",
      "Codex accepted thread/settings/update but did not emit matching authoritative settings",
      true,
    );
  }

  async #threadCommandStatus(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding & { nativeThreadId: string },
  ): Promise<ExternalThreadCommandStatus> {
    const settings = await this.#effectiveThreadSettings(controlled, binding);
    const activeTurn = (await this.#bridge.listActiveExternalTurns()).find(
      (turn) => turn.request.bindingId === binding.bindingId,
    );
    return {
      runtimeId: controlled.registration.runtimeId,
      runtimeKind: controlled.registration.kind,
      runtimeObservedState: controlled.registration.observedState,
      controller: this.#status(controlled),
      bindingId: binding.bindingId,
      bindingRevision: binding.revision,
      bindingStatus: binding.status,
      sessionId: binding.sessionId ?? null,
      agentId: binding.agentId ?? null,
      nativeThreadId: binding.nativeThreadId,
      activeNativeTurnId: activeTurn?.nativeTurnId ?? null,
      settings: projectThreadSettings(settings),
      usage: controlled.threadUsage.get(binding.nativeThreadId) ?? null,
    };
  }

  async #applyControl(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding,
    request: ExternalControlRequest,
  ): Promise<unknown> {
    switch (request.kind) {
      case "start_or_resume_thread": {
        const developerInstructions =
          await this.#developerInstructionsForBinding(binding);
        if (typeof binding.nativeThreadId === "string") {
          const resumed = await controlled.driver.threadResume({
            threadId: binding.nativeThreadId,
            ...(isRecord(request.payload) ? request.payload : {}),
            baseInstructions: undefined,
            developerInstructions,
          });
          controlled.threadSettings.set(binding.nativeThreadId, {
            model: resumed.model,
            modelProvider: resumed.modelProvider,
            reasoning_effort: resumed.reasoningEffort,
            developer_instructions: developerInstructions,
          });
          return resumed;
        }
        const cwd = binding.cwd ?? "/home";
        const started = await controlled.driver.threadStart({
          cwd,
          approvalPolicy: "never",
          sandbox: "danger-full-access",
          ephemeral: false,
          environments: [{ environmentId: "local", cwd }],
          dynamicTools: [...CODEX_COORDINATION_DYNAMIC_TOOLS],
          ...(isRecord(request.payload) ? request.payload : {}),
          baseInstructions: undefined,
          developerInstructions,
        });
        controlled.threadSettings.set(started.thread.id, {
          model: started.model,
          modelProvider: started.modelProvider,
          reasoning_effort: started.reasoningEffort,
          developer_instructions: developerInstructions,
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
        return this.#interruptTurn(controlled, binding, request);
      case "compact_thread":
        return controlled.driver.compactThread(
          request.payload as Parameters<
            CodexAppServerDriver["compactThread"]
          >[0],
        );
      case "execute_thread_command": {
        const payload = isRecord(request.payload) ? request.payload : {};
        const command = stringValue(payload.command);
        const argument =
          payload.argument === null ? null : stringValue(payload.argument);
        if (command === undefined || argument === undefined) {
          throw new ExternalRuntimeCommandInputError(
            "external_command_invalid_input",
            "external command control payload is malformed",
          );
        }
        return this.#applyThreadCommand(
          controlled,
          await this.#requireCommandBinding(binding.bindingId),
          parseExternalRuntimeCommand(
            `/${command}${argument === null ? "" : ` ${argument}`}`,
          ),
          request.controlId,
        );
      }
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

  async #interruptTurn(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding,
    request: ExternalControlRequest,
  ): Promise<{
    readonly interrupted: true;
    readonly nativeThreadId: string;
    readonly nativeTurnId: string;
    readonly nativeResult: unknown;
  }> {
    if (typeof binding.nativeThreadId !== "string") {
      throw new Error(
        "external interrupt requires a binding with a native thread",
      );
    }
    if (typeof request.expectedNativeTurnId !== "string") {
      throw new Error(
        "external interrupt requires a Rust-validated expected native turn",
      );
    }
    const nativeResult = await controlled.driver.turnInterrupt({
      threadId: binding.nativeThreadId,
      turnId: request.expectedNativeTurnId,
    });
    return {
      interrupted: true,
      nativeThreadId: binding.nativeThreadId,
      nativeTurnId: request.expectedNativeTurnId,
      nativeResult,
    };
  }

  #authority(controlled: ControlledRuntime): CodexControllerAuthority {
    return {
      authorizeHandshake: (identity, probeReport) =>
        this.#authorizeHandshake(controlled, identity, probeReport),
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
    probeReport: ExternalRuntimeCompatibilityProbeReport,
  ): Promise<{
    accepted: boolean;
    retryable?: boolean;
    reasonCode?: string;
    message?: string;
  }> {
    const decision = await this.#bridge.authorizeExternalRuntimeHandshake({
      runtimeId: controlled.registration.runtimeId,
      controller: this.#controllerContext(controlled),
      cliVersion: parseCodexCliVersion(identity.userAgent),
      consumedContractRevision: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
      probeReport,
      observedAt: this.#now().toISOString(),
    });
    controlled.registration = decision.registration;
    if (decision.accepted) {
      controlled.handshakeIdentity = identity;
    }
    return {
      accepted: decision.accepted,
      retryable: decision.retryable,
      ...(decision.reasonCode == null
        ? {}
        : {
            reasonCode: decision.reasonCode,
            message: `Rust authority rejected ${identity.userAgent}: ${probeReport.steps.find((step) => step.status === "failed")?.detail ?? "required compatibility probe failed"}`,
          }),
    };
  }

  async #recordEvent(
    controlled: ControlledRuntime,
    event: NeutralExternalRuntimeEvent,
  ): Promise<void> {
    if (
      event.threadId !== undefined &&
      (event.method === "thread/closed" ||
        event.method === "thread/archived" ||
        event.method === "thread/deleted")
    ) {
      controlled.threadSettings.delete(event.threadId);
      controlled.threadUsage.delete(event.threadId);
    }
    if (event.threadId !== undefined && event.method === "thread/archived") {
      controlled.archivedThreadIds.add(event.threadId);
    }
    if (event.threadId !== undefined && event.method === "thread/unarchived") {
      controlled.archivedThreadIds.delete(event.threadId);
    }
    if (event.threadId !== undefined && event.payload.settings !== undefined) {
      const current = controlled.threadSettings.get(event.threadId);
      controlled.threadSettings.set(event.threadId, {
        model: event.payload.settings.model,
        modelProvider: event.payload.settings.modelProvider,
        reasoning_effort: event.payload.settings.effort,
        developer_instructions: current?.developer_instructions ?? null,
      });
    }
    if (event.threadId !== undefined && event.payload.usage !== undefined) {
      controlled.threadUsage.set(
        event.threadId,
        projectThreadUsage(event.payload.usage),
      );
    }
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

  async #recordCommandEvent(
    controlled: ControlledRuntime,
    binding: ExternalAgentBinding & { nativeThreadId: string },
    receipt: ExternalControlReceipt,
    kind: "command_started" | "command_completed" | "command_failed",
  ): Promise<void> {
    const payload = isRecord(receipt.request.payload)
      ? receipt.request.payload
      : {};
    const outcome = isRecord(receipt.outcome) ? receipt.outcome : {};
    const message = stringValue(outcome.message);
    await this.#bridge.recordExternalRuntimeEvent({
      controller: this.#controllerContext(controlled),
      event: {
        eventId: `${receipt.request.controlId}:${kind}`,
        ...(binding.sessionId === undefined
          ? {}
          : { sessionId: binding.sessionId }),
        createdAt:
          kind === "command_started"
            ? receipt.request.requestedAt
            : receipt.updatedAt,
        kind,
        runtimeId: binding.runtimeId,
        nativeThreadId: binding.nativeThreadId,
        requestId: receipt.request.controlId,
        payload: {
          nativeMethod: "rustyCrew/externalCommand",
          status: kind === "command_started" ? "pending" : receipt.status,
          command: stringValue(payload.command) ?? "unknown",
          argument: payload.argument ?? null,
          controlId: receipt.request.controlId,
          reasonCode: receipt.reasonCode ?? null,
          ...(message === undefined ? {} : { message }),
        },
      },
    });
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
    const error = terminalError(event);
    await this.#bridge.transitionExternalTurn({
      controller: this.#controllerContext(controlled),
      requestId: turn.request.requestId,
      nextPhase: phase,
      ...(phase === "completed"
        ? {}
        : { terminalReasonCode: `codex_${phase}` }),
      ...(error === undefined ? {} : { terminalError: error }),
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
    let controlled = this.#controlled.get(runtimeId);
    if (controlled === undefined || controlled.driver.state !== "ready") {
      await this.connect(runtimeId);
      controlled = this.#controlled.get(runtimeId);
    } else if (!this.#hasLease(controlled)) {
      controlled.lease = await this.#acquireLease(runtimeId);
    }
    if (controlled === undefined || controlled.driver.state !== "ready") {
      throw new Error(`external runtime ${runtimeId} controller is not ready`);
    }
    return controlled;
  }

  async #bindingsForThread(
    runtimeId: string,
    threadId: string,
  ): Promise<ExternalAgentBinding[]> {
    return (await this.#bridge.listExternalBindings()).filter(
      (binding) =>
        binding.runtimeId === runtimeId && binding.nativeThreadId === threadId,
    );
  }

  async #assertThreadHasNoCrewWork(
    runtimeId: string,
    threadId: string,
    bindings: readonly ExternalAgentBinding[],
  ): Promise<void> {
    await this.#assertThreadsHaveNoCrewWork(
      runtimeId,
      new Set([threadId]),
      bindings,
    );
  }

  async #assertThreadsHaveNoCrewWork(
    runtimeId: string,
    threadIds: ReadonlySet<string>,
    bindings: readonly ExternalAgentBinding[],
  ): Promise<void> {
    const bindingIds = new Set(bindings.map((binding) => binding.bindingId));
    const activeTurn = (await this.#bridge.listActiveExternalTurns()).find(
      (turn) =>
        turn.runtimeId === runtimeId &&
        (threadIds.has(turn.nativeThreadId ?? "") ||
          bindingIds.has(turn.request.bindingId)),
    );
    if (activeTurn !== undefined) {
      throw new ExternalThreadLifecycleError(
        "external_thread_active",
        `thread lifecycle scope has active Crew turn ${activeTurn.request.requestId}`,
      );
    }
    const interaction = (
      await this.#bridge.listPendingExternalInteractions()
    ).find(
      (candidate) =>
        candidate.runtimeId === runtimeId &&
        threadIds.has(candidate.nativeThreadId),
    );
    if (interaction !== undefined) {
      throw new ExternalThreadLifecycleError(
        "external_thread_interaction_pending",
        `thread ${interaction.nativeThreadId} has unresolved interaction ${interaction.interactionId}`,
      );
    }
  }

  async #threadDeletionScope(
    controlled: ControlledRuntime,
    threadId: string,
  ): Promise<NativeThreadCatalogEntry[]> {
    const [active, archived] = await Promise.all([
      this.#listNativeThreadCatalog(controlled, false),
      this.#listNativeThreadCatalog(controlled, true),
    ]);
    const catalog = new Map<string, NativeThreadCatalogEntry>();
    for (const entry of [...archived, ...active]) {
      catalog.set(entry.thread.id, entry);
    }
    const scope = new Set([threadId]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const entry of catalog.values()) {
        if (
          entry.thread.parentThreadId !== null &&
          scope.has(entry.thread.parentThreadId) &&
          !scope.has(entry.thread.id)
        ) {
          scope.add(entry.thread.id);
          changed = true;
        }
      }
    }
    return [...scope]
      .sort()
      .map((id) => catalog.get(id))
      .filter(
        (entry): entry is NativeThreadCatalogEntry => entry !== undefined,
      );
  }

  async #listNativeThreadCatalog(
    controlled: ControlledRuntime,
    archived: boolean,
  ): Promise<NativeThreadCatalogEntry[]> {
    const entries: NativeThreadCatalogEntry[] = [];
    let cursor: string | undefined;
    const seenCursors = new Set<string>();
    for (let page = 0; page < 100; page += 1) {
      const result = await controlled.driver.threadList({
        archived,
        limit: 1_000,
        useStateDbOnly: true,
        ...(cursor === undefined ? {} : { cursor }),
      });
      entries.push(
        ...result.data.map((thread) => ({ thread, archived }) as const),
      );
      if (result.nextCursor === null) return entries;
      if (seenCursors.has(result.nextCursor)) break;
      seenCursors.add(result.nextCursor);
      cursor = result.nextCursor;
    }
    throw new ExternalThreadLifecycleError(
      "external_thread_listing_limit_exceeded",
      "could not enumerate the bounded native thread catalog for deletion",
    );
  }

  async #remainingNativeThreadIds(
    controlled: ControlledRuntime,
    scopedThreadIds: ReadonlySet<string>,
  ): Promise<Set<string>> {
    const [active, archived] = await Promise.all([
      this.#listNativeThreadCatalog(controlled, false),
      this.#listNativeThreadCatalog(controlled, true),
    ]);
    return new Set(
      [...active, ...archived]
        .map((entry) => entry.thread.id)
        .filter((id) => scopedThreadIds.has(id)),
    );
  }

  async #locateThread(
    controlled: ControlledRuntime,
    threadId: string,
  ): Promise<{ readonly thread: NativeCodexThread } | "archived" | undefined> {
    const active = await this.#findThread(controlled, threadId, false);
    if (active !== undefined) return { thread: active };
    const archived = await this.#findThread(controlled, threadId, true);
    return archived === undefined ? undefined : "archived";
  }

  async #findThread(
    controlled: ControlledRuntime,
    threadId: string,
    archived: boolean,
  ): Promise<NativeCodexThread | undefined> {
    let cursor: string | undefined;
    const seenCursors = new Set<string>();
    for (let page = 0; page < 100; page += 1) {
      const result = await controlled.driver.threadList({
        archived,
        limit: 1_000,
        useStateDbOnly: true,
        ...(cursor === undefined ? {} : { cursor }),
      });
      const found = result.data.find((thread) => thread.id === threadId);
      if (found !== undefined) return found;
      if (result.nextCursor === null) return undefined;
      if (seenCursors.has(result.nextCursor)) break;
      seenCursors.add(result.nextCursor);
      cursor = result.nextCursor;
    }
    throw new ExternalThreadLifecycleError(
      "external_thread_listing_limit_exceeded",
      `could not locate thread ${threadId} within the bounded native thread catalog`,
    );
  }

  #threadLifecycleReceipt(
    runtimeId: string,
    threadId: string,
    action: "archive" | "unarchive",
    outcome: ExternalThreadLifecycleReceipt["outcome"],
    nativeArchived: boolean,
    bindings: readonly ExternalAgentBinding[],
    saved = new Map<string, ExternalAgentBinding>(),
  ): ExternalThreadLifecycleReceipt {
    return {
      runtimeId,
      threadId,
      action,
      outcome,
      nativeArchived,
      bindings: this.#bindingTransitions(bindings, saved),
    };
  }

  #threadDeleteReceipt(
    runtimeId: string,
    threadId: string,
    outcome: ExternalThreadDeleteReceipt["outcome"],
    bindings: readonly ExternalAgentBinding[],
    saved: readonly {
      readonly before: ExternalAgentBinding;
      readonly after: ExternalAgentBinding;
    }[],
  ): ExternalThreadDeleteReceipt {
    return {
      runtimeId,
      threadId,
      action: "delete",
      outcome,
      nativeDeleted: true,
      bindings: this.#bindingTransitions(
        bindings,
        new Map(saved.map(({ before, after }) => [before.bindingId, after])),
      ),
    };
  }

  #bindingTransitions(
    bindings: readonly ExternalAgentBinding[],
    saved = new Map<string, ExternalAgentBinding>(),
  ): ExternalThreadLifecycleReceipt["bindings"] {
    return bindings.map((binding) => {
      const current = saved.get(binding.bindingId) ?? binding;
      return {
        bindingId: binding.bindingId,
        previousStatus: binding.status,
        currentStatus: current.status,
        revision: current.revision,
      };
    });
  }

  async #restoreBindingTransitions(
    saved: readonly {
      readonly before: ExternalAgentBinding;
      readonly after: ExternalAgentBinding;
    }[],
  ): Promise<string[]> {
    const failures: string[] = [];
    for (const transition of [...saved].reverse()) {
      await this.#bridge
        .bindExternalAgent({
          binding: {
            ...transition.after,
            status: transition.before.status,
            updatedAt: this.#now().toISOString(),
          },
          expectedRevision: transition.after.revision,
        })
        .catch((error: unknown) => failures.push(String(error)));
    }
    return failures;
  }

  async #requireBinding(bindingId: string): Promise<ExternalAgentBinding> {
    const binding = await this.#bridge.getExternalBinding(bindingId);
    if (binding === undefined) {
      throw new Error(`external binding ${bindingId} was not found`);
    }
    return binding;
  }

  async #requireCommandBinding(
    bindingId: string,
  ): Promise<ExternalAgentBinding & { nativeThreadId: string }> {
    const binding = await this.#requireBinding(bindingId);
    if (binding.status !== "active") {
      throw new ExternalRuntimeCommandError(
        "external_command_settings_unavailable",
        `external binding ${bindingId} is not active`,
      );
    }
    if (binding.nativeThreadId === undefined) {
      throw new ExternalRuntimeCommandError(
        "external_command_settings_unavailable",
        `external binding ${bindingId} has no native thread`,
      );
    }
    return binding as ExternalAgentBinding & { nativeThreadId: string };
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
      observedCliVersion: controlled.registration.observedCliVersion ?? null,
      consumedContractRevision:
        controlled.registration.consumedContractRevision ?? null,
      compatibilityState: controlled.registration.compatibilityState,
      compatibilityDiagnostic: compatibilityDiagnostic(controlled.registration),
      lastCompatibilityProbe:
        controlled.registration.lastCompatibilityProbe ?? null,
      bindingResumeFailures: controlled.bindingResumeFailures.map(
        (failure) => ({ ...failure }),
      ),
    };
  }
}

function compatibilityDiagnostic(
  registration: ExternalRuntimeRegistration,
): ExternalRuntimeControllerStatus["compatibilityDiagnostic"] {
  if (registration.observedState === "disconnected") return "disconnected";
  if (registration.lastCompatibilityProbe?.outcome === "transport_retryable") {
    return "probe_failed";
  }
  if (registration.compatibilityState === "unassessed") return "probe_failed";
  return registration.compatibilityState;
}

function externalAgentSessionCreationFailureReason(
  error: unknown,
  nativeThreadId: string | undefined,
): string {
  if (error instanceof ExternalAgentSessionCreationError) {
    return error.reasonCode;
  }
  const message = String(error).toLowerCase();
  const stableReasonCode = EXTERNAL_AGENT_SESSION_CREATION_REASON_CODES.find(
    (candidate) => message.includes(candidate),
  );
  if (stableReasonCode !== undefined) {
    return stableReasonCode;
  }
  if (
    message.includes("capacity") ||
    message.includes("too many pending") ||
    message.includes("max pending") ||
    message.includes("resource exhausted")
  ) {
    return "external_agent_creation_capacity_conflict";
  }
  return nativeThreadId === undefined
    ? "external_agent_creation_native_start_failed"
    : "external_agent_creation_recovery_required";
}

function bindingMetadataMatches(
  binding: ExternalAgentBinding,
  label: string | null,
  taskRef: DenRuntimeReference | null,
): boolean {
  return (
    (binding.label ?? null) === label &&
    denRuntimeReferenceKey(binding.taskRef ?? null) ===
      denRuntimeReferenceKey(taskRef)
  );
}

function denRuntimeReferenceKey(reference: DenRuntimeReference | null): string {
  if (reference === null) return "";
  const raw = reference as DenRuntimeReference & {
    project_id?: string;
    task_id?: string;
  };
  return `${reference.projectId ?? raw.project_id ?? ""}\u0000${reference.taskId ?? raw.task_id ?? ""}`;
}

function projectExternalRuntimeModel(model: Model): ExternalRuntimeModelOption {
  return {
    id: model.id,
    model: model.model,
    displayName: model.displayName,
    description: model.description,
    hidden: model.hidden,
    isDefault: model.isDefault,
    defaultEffort: model.defaultReasoningEffort,
    supportedEfforts: model.supportedReasoningEfforts.map((option) => ({
      value: option.reasoningEffort,
      description: option.description,
    })),
  };
}

function threadSettingsFromResume(
  resumed: Awaited<ReturnType<CodexAppServerDriver["threadResume"]>>,
): ControlledThreadSettings {
  return {
    model: resumed.model,
    modelProvider: resumed.modelProvider,
    reasoning_effort: resumed.reasoningEffort,
    developer_instructions: null,
  };
}

function projectThreadSettings(
  settings: ControlledThreadSettings,
): ExternalThreadSettingsProjection {
  return {
    model: settings.model,
    modelProvider: settings.modelProvider,
    effort: settings.reasoning_effort,
  };
}

function projectThreadUsage(
  usage: NonNullable<NeutralExternalRuntimeEvent["payload"]["usage"]>,
): ExternalThreadUsageProjection {
  const totalTokens = usage.total.totalTokens;
  const contextWindowUsedPercent =
    typeof totalTokens === "number" &&
    usage.modelContextWindow !== null &&
    usage.modelContextWindow > 0
      ? (totalTokens / usage.modelContextWindow) * 100
      : null;
  return {
    total: usage.total,
    last: usage.last,
    modelContextWindow: usage.modelContextWindow,
    contextWindowUsedPercent,
  };
}

function commandExecutionResult(
  command: ParsedExternalRuntimeCommand,
  receipt: ExternalControlReceipt,
): ExternalRuntimeCommandExecutionResult {
  const outcome = isRecord(receipt.outcome) ? receipt.outcome : {};
  const result = isRecord(outcome.result)
    ? (outcome.result as ExternalRuntimeCommandResultData)
    : {};
  return {
    commandId: receipt.request.controlId,
    input: command.input,
    command: command.command,
    argument: command.argument,
    status: receipt.status,
    reasonCode: receipt.reasonCode ?? null,
    message:
      stringValue(outcome.message) ??
      (receipt.status === "applied"
        ? `/${command.command} completed.`
        : `/${command.command} ${receipt.status}.`),
    result,
    receipt,
  };
}

function projectExternalThread(
  value: unknown,
  effectiveModel: string | null,
  managedLabel?: string | null,
): ExternalThreadProjection {
  const thread = requireNativeRecord(value, "thread");
  return {
    threadId: requireNativeString(thread.id, "thread.id"),
    sessionId: requireNativeString(thread.sessionId, "thread.sessionId"),
    parentThreadId: nullableNativeString(thread.parentThreadId),
    preview: nativeString(thread.preview) ?? "",
    ephemeral: thread.ephemeral === true,
    modelProvider: nativeString(thread.modelProvider) ?? "unknown",
    effectiveModel,
    createdAt: nativeNumber(thread.createdAt) ?? 0,
    updatedAt: nativeNumber(thread.updatedAt) ?? 0,
    status: projectNativeStatus(thread.status),
    cwd: nativeString(thread.cwd) ?? "/home",
    cliVersion: nativeString(thread.cliVersion) ?? "unknown",
    name:
      managedLabel === undefined
        ? nullableNativeString(thread.name)
        : managedLabel,
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
    statusSource: "native",
    terminalReasonCode: null,
    error: projectNativeTurnError(turn.error),
    startedAt: nullableNativeNumber(turn.startedAt),
    completedAt: nullableNativeNumber(turn.completedAt),
    durationMs: nullableNativeNumber(turn.durationMs),
    items: Array.isArray(turn.items)
      ? turn.items.map(projectExternalThreadItem)
      : [],
  };
}

function reconcileExternalThreadProjection(
  thread: ExternalThreadProjection,
  correlations: readonly ExternalTurnCorrelation[],
): ExternalThreadProjection {
  const byNativeTurnId = new Map(
    correlations.flatMap((correlation) =>
      correlation.nativeTurnId === null
        ? []
        : [[correlation.nativeTurnId, correlation] as const],
    ),
  );
  return {
    ...thread,
    turns: thread.turns.map((turn) => {
      const correlation = byNativeTurnId.get(turn.turnId);
      const reconciledStatus = crewTerminalStatus(correlation?.phase);
      if (correlation === undefined || reconciledStatus === undefined) {
        return turn;
      }
      return {
        ...turn,
        status: reconciledStatus,
        statusSource: "crew_terminal" as const,
        terminalReasonCode: correlation.terminalReasonCode ?? null,
        error:
          correlation.terminalError == null
            ? turn.error
            : {
                message: correlation.terminalError.message,
                code: correlation.terminalError.code ?? null,
                additionalDetails:
                  correlation.terminalError.additionalDetails ?? null,
                willRetry: correlation.terminalError.willRetry ?? null,
              },
      };
    }),
  };
}

function crewTerminalStatus(
  phase: ExternalTurnCorrelation["phase"] | undefined,
) {
  if (phase === "failed") return "failed";
  if (phase === "interrupted") return "interrupted";
  if (phase === "outcome_unknown") return "outcomeUnknown";
  return undefined;
}

function projectNativeTurnError(value: unknown) {
  const error = isRecord(value) ? value : undefined;
  const message = error === undefined ? undefined : nativeString(error.message);
  if (error === undefined || message === undefined) return null;
  return {
    message,
    code: nativeCodexErrorCode(error.codexErrorInfo),
    additionalDetails: nullableNativeString(error.additionalDetails),
    willRetry: null,
  };
}

function nativeCodexErrorCode(value: unknown): string | null {
  if (typeof value === "string" && value.length > 0) return value;
  const record = isRecord(value) ? value : undefined;
  return record === undefined ? null : (Object.keys(record)[0] ?? null);
}

function terminalError(
  event: NormalizedExternalRuntimeEvent,
): ExternalTurnCorrelation["terminalError"] | undefined {
  if (!isRecord(event.payload) || !isRecord(event.payload.error)) {
    return undefined;
  }
  const message = stringValue(event.payload.error.message);
  if (message === undefined) return undefined;
  return {
    message,
    code: stringValue(event.payload.error.code) ?? null,
    additionalDetails:
      stringValue(event.payload.error.additionalDetails) ?? null,
    willRetry:
      typeof event.payload.error.willRetry === "boolean"
        ? event.payload.error.willRetry
        : null,
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
  const messagePhase =
    kind === "agentMessage" ? projectAgentMessagePhase(item.phase) : undefined;
  return {
    itemId,
    kind,
    ...(status === undefined ? {} : { status }),
    ...(text === undefined ? {} : { text }),
    ...(summary === undefined || summary.length === 0 ? {} : { summary }),
    ...(messagePhase === undefined ? {} : { messagePhase }),
  };
}

function isUnmaterializedThreadRead(error: unknown): boolean {
  return (
    error instanceof CodexRpcError &&
    error.message.includes(
      "is not materialized yet; includeTurns is unavailable before first user message",
    )
  );
}

function codexCapabilityError(
  error: unknown,
): ExternalRuntimeCommandError | undefined {
  if (!(error instanceof CodexRpcError)) return undefined;
  if (
    error.code !== -32601 &&
    !(error.code === -32600 && error.message.includes("experimentalApi"))
  ) {
    return undefined;
  }
  return new ExternalRuntimeCommandError(
    "external_command_capability_unavailable",
    `Codex app-server did not accept the command capability: ${error.message}`,
  );
}

function isMissingThreadDelete(error: unknown): boolean {
  return (
    error instanceof CodexRpcError &&
    error.message.includes("no rollout found for thread id")
  );
}

function projectAgentMessagePhase(
  value: unknown,
): ExternalAgentMessagePhase | undefined {
  if (value === undefined || value === null) return undefined;
  if (value === "commentary" || value === "final_answer") return value;
  return "unknown";
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
  if (source === "turn/failed") return "failed";
  if (source === "error") {
    const error = isRecord(event.payload) ? event.payload.error : undefined;
    if (isRecord(error) && error.willRetry === true) return undefined;
    return "failed";
  }
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
