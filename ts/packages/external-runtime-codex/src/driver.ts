import { randomUUID } from "node:crypto";
import type { InitializeParams } from "../protocol/0.144.1/ts/InitializeParams.js";
import type { InitializeResponse } from "../protocol/0.144.1/ts/InitializeResponse.js";
import type { ModelListParams } from "../protocol/0.144.1/ts/v2/ModelListParams.js";
import type { ModelListResponse } from "../protocol/0.144.1/ts/v2/ModelListResponse.js";
import type { ThreadCompactStartParams } from "../protocol/0.144.1/ts/v2/ThreadCompactStartParams.js";
import type { ThreadCompactStartResponse } from "../protocol/0.144.1/ts/v2/ThreadCompactStartResponse.js";
import type { ThreadArchiveParams } from "../protocol/0.144.1/ts/v2/ThreadArchiveParams.js";
import type { ThreadArchiveResponse } from "../protocol/0.144.1/ts/v2/ThreadArchiveResponse.js";
import type { ThreadDeleteParams } from "../protocol/0.144.1/ts/v2/ThreadDeleteParams.js";
import type { ThreadDeleteResponse } from "../protocol/0.144.1/ts/v2/ThreadDeleteResponse.js";
import type { CollaborationModeListParams } from "../protocol/0.144.1/ts/v2/CollaborationModeListParams.js";
import type { CollaborationModeListResponse } from "../protocol/0.144.1/ts/v2/CollaborationModeListResponse.js";
import type { ThreadItemsListParams } from "../protocol/0.144.1/ts/v2/ThreadItemsListParams.js";
import type { ThreadItemsListResponse } from "../protocol/0.144.1/ts/v2/ThreadItemsListResponse.js";
import type { ThreadListParams } from "../protocol/0.144.1/ts/v2/ThreadListParams.js";
import type { ThreadListResponse } from "../protocol/0.144.1/ts/v2/ThreadListResponse.js";
import type { ThreadLoadedListParams } from "../protocol/0.144.1/ts/v2/ThreadLoadedListParams.js";
import type { ThreadLoadedListResponse } from "../protocol/0.144.1/ts/v2/ThreadLoadedListResponse.js";
import type { ThreadReadParams } from "../protocol/0.144.1/ts/v2/ThreadReadParams.js";
import type { ThreadReadResponse } from "../protocol/0.144.1/ts/v2/ThreadReadResponse.js";
import type { ThreadUnarchiveParams } from "../protocol/0.144.1/ts/v2/ThreadUnarchiveParams.js";
import type { ThreadUnarchiveResponse } from "../protocol/0.144.1/ts/v2/ThreadUnarchiveResponse.js";
import type { ThreadResumeParams } from "../protocol/0.144.1/ts/v2/ThreadResumeParams.js";
import type { ThreadResumeResponse } from "../protocol/0.144.1/ts/v2/ThreadResumeResponse.js";
import type { ThreadForkParams } from "../protocol/0.144.1/ts/v2/ThreadForkParams.js";
import type { ThreadForkResponse } from "../protocol/0.144.1/ts/v2/ThreadForkResponse.js";
import type { ThreadStartParams } from "../protocol/0.144.1/ts/v2/ThreadStartParams.js";
import type { ThreadStartResponse } from "../protocol/0.144.1/ts/v2/ThreadStartResponse.js";
import type { ThreadSettingsUpdateParams } from "../protocol/0.144.1/ts/v2/ThreadSettingsUpdateParams.js";
import type { ThreadSettingsUpdateResponse } from "../protocol/0.144.1/ts/v2/ThreadSettingsUpdateResponse.js";
import type { ThreadSetNameParams } from "../protocol/0.144.1/ts/v2/ThreadSetNameParams.js";
import type { ThreadSetNameResponse } from "../protocol/0.144.1/ts/v2/ThreadSetNameResponse.js";
import type { ThreadTurnsListParams } from "../protocol/0.144.1/ts/v2/ThreadTurnsListParams.js";
import type { ThreadTurnsListResponse } from "../protocol/0.144.1/ts/v2/ThreadTurnsListResponse.js";
import type { TurnInterruptParams } from "../protocol/0.144.1/ts/v2/TurnInterruptParams.js";
import type { TurnInterruptResponse } from "../protocol/0.144.1/ts/v2/TurnInterruptResponse.js";
import type { TurnStartParams } from "../protocol/0.144.1/ts/v2/TurnStartParams.js";
import type { TurnStartResponse } from "../protocol/0.144.1/ts/v2/TurnStartResponse.js";
import type { TurnSteerParams } from "../protocol/0.144.1/ts/v2/TurnSteerParams.js";
import type { TurnSteerResponse } from "../protocol/0.144.1/ts/v2/TurnSteerResponse.js";
import {
  CodexProtocolCodec,
  CodexProtocolError,
  type JsonRpcResponseMessage,
} from "./codec.js";
import {
  mapNotification,
  mapUnsupportedServerRequest,
} from "./event-mapper.js";
import { CODEX_COORDINATION_DYNAMIC_TOOLS } from "./coordination.js";
import { CODEX_APP_SERVER_PROTOCOL } from "./protocol-manifest.js";
import { captureBoundedRawDetail } from "./raw-detail.js";
import type { CodexJsonRpcTransport } from "./transport.js";
import type {
  CodexControllerAuthority,
  CodexCompatibilityProbeReport,
  CodexCompatibilityProbeStep,
  CodexDriverOptions,
  CodexDriverState,
  CodexProtocolFault,
  JsonRpcId,
  ServerRequestResolution,
} from "./types.js";

interface PendingRequest {
  readonly method: string;
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: Error) => void;
  readonly timer: NodeJS.Timeout;
  readonly abortCleanup?: () => void;
}

const COMPATIBILITY_PROBE_SUITE_REVISION = "codex-required-capabilities-v2";

function isExpectedMissingThreadError(
  error: CodexRpcError,
  threadId: string,
  operation: "read" | "resume",
): boolean {
  if (error.code !== -32600) return false;
  const expectedMessage =
    operation === "read"
      ? `thread not loaded: ${threadId}`
      : `no rollout found for thread id ${threadId}`;
  return error.message === expectedMessage;
}

export class CodexRpcError extends Error {
  constructor(
    readonly code: number,
    message: string,
    readonly data?: unknown,
  ) {
    super(message);
  }
}

export class CodexAppServerDriver {
  readonly #transport: CodexJsonRpcTransport;
  readonly #authority: CodexControllerAuthority;
  readonly #codec = new CodexProtocolCodec();
  readonly #requestTimeoutMs: number;
  readonly #compatibilityProbeTimeoutMs: number;
  readonly #maxPendingRequests: number;
  readonly #maxRawDetailBytes: number;
  readonly #clientInfo: InitializeParams["clientInfo"];
  readonly #pending = new Map<JsonRpcId, PendingRequest>();
  readonly #pendingServerRequests = new Set<JsonRpcId>();
  readonly #completedResponseIds = new Set<JsonRpcId>();
  readonly #completedResponseOrder: JsonRpcId[] = [];
  #nextRequestId = 1;
  #transportSequence = 0;
  #receiveChain = Promise.resolve();
  #state: CodexDriverState = "disconnected";
  #disconnectNotified = false;
  #disconnectCause: Error | undefined;
  #lastCompatibilityProbe?: CodexCompatibilityProbeReport;

  constructor(
    transport: CodexJsonRpcTransport,
    authority: CodexControllerAuthority,
    options: CodexDriverOptions = {},
  ) {
    this.#transport = transport;
    this.#authority = authority;
    this.#requestTimeoutMs = options.requestTimeoutMs ?? 30_000;
    this.#compatibilityProbeTimeoutMs =
      options.compatibilityProbeTimeoutMs ?? 15_000;
    this.#maxPendingRequests = options.maxPendingRequests ?? 256;
    this.#maxRawDetailBytes = options.maxRawDetailBytes ?? 64 * 1024;
    if (
      !Number.isFinite(this.#requestTimeoutMs) ||
      this.#requestTimeoutMs <= 0 ||
      !Number.isFinite(this.#compatibilityProbeTimeoutMs) ||
      this.#compatibilityProbeTimeoutMs <= 0 ||
      !Number.isInteger(this.#maxPendingRequests) ||
      this.#maxPendingRequests <= 0 ||
      !Number.isInteger(this.#maxRawDetailBytes) ||
      this.#maxRawDetailBytes <= 0
    ) {
      throw new Error("Codex driver limits must be positive finite values");
    }
    this.#clientInfo = {
      name: options.clientName ?? "rusty_crew",
      title: options.clientTitle ?? "Rusty Crew External Runtime Controller",
      version: options.clientVersion ?? "0.1.0",
    };
  }

  get state(): CodexDriverState {
    return this.#state;
  }

  get lastCompatibilityProbe(): CodexCompatibilityProbeReport | undefined {
    return this.#lastCompatibilityProbe;
  }

  async connect(): Promise<InitializeResponse> {
    if (this.#state !== "disconnected") {
      throw new Error(`cannot connect Codex driver from ${this.#state}`);
    }
    this.#state = "connecting";
    this.#disconnectNotified = false;
    this.#disconnectCause = undefined;
    this.#transport.setHandlers({
      onMessage: (message) => {
        this.#receiveChain = this.#receiveChain
          .then(() => this.#receive(message))
          .catch((error: unknown) =>
            this.#handleReceiveFailure(error, message),
          );
      },
      onClose: (reason) => void this.#disconnect(reason),
      onError: (error) => {
        this.#disconnectCause = error;
        void this.#reportFault({
          reasonCode: "transport_error",
          message: error.message,
          fatal: true,
        });
        void this.#disconnect(`transport error: ${error.message}`, error);
      },
    });
    try {
      await this.#transport.open();
      const initialized = await this.#request<InitializeResponse>(
        "initialize",
        {
          clientInfo: this.#clientInfo,
          capabilities: {
            experimentalApi: true,
            requestAttestation: false,
            mcpServerOpenaiFormElicitation: true,
          },
        } satisfies InitializeParams,
        undefined,
        false,
      );
      const identity = {
        userAgent: initialized.userAgent,
        codexHome: initialized.codexHome,
        platformFamily: initialized.platformFamily,
        platformOs: initialized.platformOs,
        protocol: CODEX_APP_SERVER_PROTOCOL,
      } as const;
      const probeReport = await this.#runCompatibilityProbe();
      this.#lastCompatibilityProbe = probeReport;
      const authorization = await this.#authority.authorizeHandshake(
        identity,
        probeReport,
      );
      if (!authorization.accepted) {
        this.#state =
          authorization.retryable === true ? "disconnected" : "incompatible";
        await this.#transport.close();
        throw new Error(
          `${authorization.reasonCode ?? "codex_protocol_incompatible"}: ${authorization.message ?? "Rust authority rejected Codex app-server handshake"}`,
        );
      }
      this.#state = "ready";
      return initialized;
    } catch (error) {
      if (this.#state !== "incompatible") this.#state = "disconnected";
      throw error;
    }
  }

  async #runCompatibilityProbe(): Promise<CodexCompatibilityProbeReport> {
    const controller = new AbortController();
    const timer = setTimeout(
      () => controller.abort(new Error("compatibility probe timed out")),
      this.#compatibilityProbeTimeoutMs,
    );
    const steps: CodexCompatibilityProbeStep[] = [];
    const sentinelThreadId = randomUUID();
    let outcome: CodexCompatibilityProbeReport["outcome"] = "passed";
    const run = async (
      stepId: string,
      operation: () => Promise<void> | void,
      options: {
        acceptRpcError?: (error: CodexRpcError) => boolean;
      } = {},
    ): Promise<boolean> => {
      const startedAt = performance.now();
      try {
        await operation();
        steps.push({
          stepId,
          status: "passed",
          durationMs: Math.max(0, Math.round(performance.now() - startedAt)),
        });
        return true;
      } catch (error) {
        if (
          options.acceptRpcError !== undefined &&
          error instanceof CodexRpcError &&
          options.acceptRpcError(error)
        ) {
          steps.push({
            stepId,
            status: "passed",
            durationMs: Math.max(0, Math.round(performance.now() - startedAt)),
            detail: "method recognized; sentinel resource was rejected",
          });
          return true;
        }
        const transportRetryable =
          controller.signal.aborted ||
          (!(error instanceof CodexProtocolError) &&
            !(error instanceof CodexRpcError));
        outcome = transportRetryable ? "transport_retryable" : "incompatible";
        steps.push({
          stepId,
          status: "failed",
          durationMs: Math.max(0, Math.round(performance.now() - startedAt)),
          reasonCode: transportRetryable
            ? "external_runtime_probe_transport_retryable"
            : error instanceof CodexRpcError && error.code === -32601
              ? "external_runtime_required_method_missing"
              : "external_runtime_required_contract_incompatible",
          detail: String(error).slice(0, 1_024),
        });
        return false;
      }
    };
    const remaining = [
      "dynamic_tools",
      "model_list",
      "thread_list",
      "thread_read",
      "thread_resume",
    ];
    try {
      if (
        !(await run("consumed_codec", () =>
          this.#codec.assertConsumedContractReady(),
        ))
      ) {
        for (const stepId of remaining) steps.push(skippedProbeStep(stepId));
      } else if (!(await run("dynamic_tools", assertDynamicToolReadiness))) {
        for (const stepId of remaining.slice(1))
          steps.push(skippedProbeStep(stepId));
      } else if (
        !(await run("model_list", async () => {
          await this.#request(
            "model/list",
            { limit: 1 },
            controller.signal,
            false,
          );
        }))
      ) {
        for (const stepId of remaining.slice(2))
          steps.push(skippedProbeStep(stepId));
      } else if (
        !(await run("thread_list", async () => {
          await this.#request<ThreadListResponse>(
            "thread/list",
            { limit: 1, useStateDbOnly: true },
            controller.signal,
            false,
          );
        }))
      ) {
        steps.push(
          skippedProbeStep("thread_read"),
          skippedProbeStep("thread_resume"),
        );
      } else {
        if (
          !(await run(
            "thread_read",
            async () => {
              await this.#request(
                "thread/read",
                { threadId: sentinelThreadId, includeTurns: false },
                controller.signal,
                false,
              );
            },
            {
              acceptRpcError: (error) =>
                isExpectedMissingThreadError(error, sentinelThreadId, "read"),
            },
          ))
        ) {
          steps.push(skippedProbeStep("thread_resume"));
        } else {
          await run(
            "thread_resume",
            async () => {
              await this.#request(
                "thread/resume",
                { threadId: sentinelThreadId, excludeTurns: true },
                controller.signal,
                false,
              );
            },
            {
              acceptRpcError: (error) =>
                isExpectedMissingThreadError(error, sentinelThreadId, "resume"),
            },
          );
        }
      }
    } finally {
      clearTimeout(timer);
    }
    return {
      suiteRevision: COMPATIBILITY_PROBE_SUITE_REVISION,
      outcome,
      steps,
      completedAt: new Date().toISOString(),
    };
  }

  threadList(
    params: ThreadListParams,
    signal?: AbortSignal,
  ): Promise<ThreadListResponse> {
    return this.#request("thread/list", params, signal);
  }

  modelList(
    params: ModelListParams = {},
    signal?: AbortSignal,
  ): Promise<ModelListResponse> {
    return this.#request("model/list", params, signal);
  }

  collaborationModeList(
    params: CollaborationModeListParams = {},
    signal?: AbortSignal,
  ): Promise<CollaborationModeListResponse> {
    return this.#request("collaborationMode/list", params, signal);
  }

  loadedThreadList(
    params: ThreadLoadedListParams,
    signal?: AbortSignal,
  ): Promise<ThreadLoadedListResponse> {
    return this.#request("thread/loaded/list", params, signal);
  }

  threadRead(
    params: ThreadReadParams,
    signal?: AbortSignal,
  ): Promise<ThreadReadResponse> {
    return this.#request("thread/read", params, signal);
  }

  threadArchive(
    params: ThreadArchiveParams,
    signal?: AbortSignal,
  ): Promise<ThreadArchiveResponse> {
    return this.#request("thread/archive", params, signal);
  }

  threadDelete(
    params: ThreadDeleteParams,
    signal?: AbortSignal,
  ): Promise<ThreadDeleteResponse> {
    return this.#request("thread/delete", params, signal);
  }

  threadUnarchive(
    params: ThreadUnarchiveParams,
    signal?: AbortSignal,
  ): Promise<ThreadUnarchiveResponse> {
    return this.#request("thread/unarchive", params, signal);
  }

  threadStart(
    params: ThreadStartParams,
    signal?: AbortSignal,
  ): Promise<ThreadStartResponse> {
    for (const dynamicTool of params.dynamicTools ?? []) {
      if (dynamicTool.type === "namespace" && dynamicTool.name === "mcp") {
        throw new Error(
          "dynamic tool namespace mcp is reserved by Codex; use an adapter-owned namespace",
        );
      }
    }
    return this.#request("thread/start", params, signal);
  }

  threadResume(
    params: ThreadResumeParams,
    signal?: AbortSignal,
  ): Promise<ThreadResumeResponse> {
    return this.#request("thread/resume", params, signal);
  }

  threadFork(
    params: ThreadForkParams,
    signal?: AbortSignal,
  ): Promise<ThreadForkResponse> {
    return this.#request("thread/fork", params, signal);
  }

  threadSetName(
    params: ThreadSetNameParams,
    signal?: AbortSignal,
  ): Promise<ThreadSetNameResponse> {
    return this.#request("thread/name/set", params, signal);
  }

  threadTurnsList(
    params: ThreadTurnsListParams,
    signal?: AbortSignal,
  ): Promise<ThreadTurnsListResponse> {
    return this.#request("thread/turns/list", params, signal);
  }

  threadItemsList(
    params: ThreadItemsListParams,
    signal?: AbortSignal,
  ): Promise<ThreadItemsListResponse> {
    return this.#request("thread/items/list", params, signal);
  }

  turnStart(
    params: TurnStartParams,
    signal?: AbortSignal,
  ): Promise<TurnStartResponse> {
    return this.#request("turn/start", params, signal);
  }

  turnSteer(
    params: TurnSteerParams,
    signal?: AbortSignal,
  ): Promise<TurnSteerResponse> {
    return this.#request("turn/steer", params, signal);
  }

  turnInterrupt(
    params: TurnInterruptParams,
    signal?: AbortSignal,
  ): Promise<TurnInterruptResponse> {
    return this.#request("turn/interrupt", params, signal);
  }

  compactThread(
    params: ThreadCompactStartParams,
    signal?: AbortSignal,
  ): Promise<ThreadCompactStartResponse> {
    return this.#request("thread/compact/start", params, signal);
  }

  threadSettingsUpdate(
    params: ThreadSettingsUpdateParams,
    signal?: AbortSignal,
  ): Promise<ThreadSettingsUpdateResponse> {
    return this.#request("thread/settings/update", params, signal);
  }

  async close(): Promise<void> {
    if (this.#state === "closed") return;
    this.#state = "closed";
    await this.#transport.close();
    await this.#disconnect("controller closed");
  }

  async #request<Result>(
    method: string,
    params: unknown,
    signal?: AbortSignal,
    requireReady = true,
  ): Promise<Result> {
    if (requireReady && this.#state !== "ready") {
      throw new Error(`Codex driver is not ready (${this.#state})`);
    }
    if (!requireReady && this.#state !== "connecting") {
      throw new Error(`Codex driver cannot initialize from ${this.#state}`);
    }
    if (this.#pending.size >= this.#maxPendingRequests) {
      throw new Error("Codex driver pending request capacity exceeded");
    }
    if (signal?.aborted === true) throw signal.reason;
    const id = this.#nextRequestId++;
    this.#codec.assertClientRequest({ method, id, params });
    const result = new Promise<Result>((resolve, reject) => {
      const timer = setTimeout(() => {
        signal?.removeEventListener("abort", onAbort);
        this.#pending.delete(id);
        reject(
          new Error(
            `Codex app-server ${method} timed out after ${this.#requestTimeoutMs}ms`,
          ),
        );
      }, this.#requestTimeoutMs);
      const onAbort = (): void => {
        const pending = this.#pending.get(id);
        if (pending === undefined) return;
        clearTimeout(pending.timer);
        this.#pending.delete(id);
        reject(signal?.reason ?? new Error(`${method} aborted`));
      };
      signal?.addEventListener("abort", onAbort, { once: true });
      this.#pending.set(id, {
        method,
        resolve: (value) => resolve(value as Result),
        reject,
        timer,
        ...(signal === undefined
          ? {}
          : {
              abortCleanup: () => signal.removeEventListener("abort", onAbort),
            }),
      });
    });
    try {
      await this.#transport.send(JSON.stringify({ method, id, params }));
    } catch (error) {
      const pending = this.#pending.get(id);
      if (pending !== undefined) {
        clearTimeout(pending.timer);
        pending.abortCleanup?.();
        this.#pending.delete(id);
        pending.reject(error as Error);
      }
    }
    return result;
  }

  async #receive(raw: string): Promise<void> {
    const sequence = ++this.#transportSequence;
    const decoded = this.#codec.decode(raw);
    switch (decoded.type) {
      case "response":
        await this.#handleResponse(decoded.response, raw);
        return;
      case "request":
        void this.#handleDetachedServerRequest(decoded.request, sequence, raw);
        return;
      case "unknown_request":
        await this.#authority.onEvent(
          mapUnsupportedServerRequest(
            decoded,
            sequence,
            this.#maxRawDetailBytes,
          ),
        );
        await this.#sendError(
          decoded.id,
          -32601,
          `unsupported app-server request ${decoded.method}`,
        );
        return;
      case "notification":
        await this.#authority.onEvent(
          mapNotification(
            decoded.notification,
            sequence,
            this.#maxRawDetailBytes,
            true,
          ),
        );
        return;
      case "unknown_notification":
        await this.#authority.onEvent(
          mapNotification(decoded, sequence, this.#maxRawDetailBytes, false),
        );
    }
  }

  async #handleResponse(
    response: JsonRpcResponseMessage,
    raw: string,
  ): Promise<void> {
    const pending = this.#pending.get(response.id);
    if (pending === undefined) {
      const duplicate = this.#completedResponseIds.has(response.id);
      await this.#reportFault({
        reasonCode: duplicate ? "duplicate_response" : "unknown_response",
        message: `${duplicate ? "duplicate" : "unknown"} app-server response id ${String(response.id)}`,
        fatal: false,
        rawDetail: captureBoundedRawDetail(raw, this.#maxRawDetailBytes),
      });
      return;
    }
    clearTimeout(pending.timer);
    pending.abortCleanup?.();
    this.#pending.delete(response.id);
    this.#rememberCompletedResponse(response.id);
    if (response.error !== undefined) {
      pending.reject(
        new CodexRpcError(
          response.error.code,
          response.error.message,
          response.error.data,
        ),
      );
      return;
    }
    try {
      this.#codec.assertClientResponse(pending.method, response.result);
      pending.resolve(response.result);
    } catch (error) {
      pending.reject(error as Error);
      throw error;
    }
  }

  async #handleServerRequest(
    request: Parameters<
      CodexControllerAuthority["resolveServerRequest"]
    >[0]["request"],
    sequence: number,
  ): Promise<void> {
    if (
      this.#state !== "ready" ||
      !(await this.#authority.hasControllerLease())
    ) {
      await this.#sendError(
        request.id,
        -32001,
        "Rusty Crew controller does not hold the active runtime lease",
      );
      return;
    }
    this.#pendingServerRequests.add(request.id);
    const context = {
      transportSequence: sequence,
      request,
      rawDetail: captureBoundedRawDetail(request, this.#maxRawDetailBytes),
    };
    let resolution: ServerRequestResolution;
    try {
      resolution = await this.#authority.resolveServerRequest(context);
    } catch (error) {
      resolution = {
        type: "error",
        code: -32603,
        message: `Crew server-request authority failed: ${String(error)}`,
      };
    } finally {
      this.#pendingServerRequests.delete(request.id);
    }
    if (
      this.#state !== "ready" ||
      !(await this.#authority.hasControllerLease())
    ) {
      return;
    }
    if (resolution.type === "error") {
      await this.#sendError(
        request.id,
        resolution.code,
        resolution.message,
        resolution.data,
      );
      return;
    }
    try {
      this.#codec.assertServerRequestResolution(
        request.method,
        resolution.result,
      );
    } catch (error) {
      await this.#reportFault({
        reasonCode: "malformed_server_request_resolution",
        message: String(error),
        fatal: false,
        rawDetail: captureBoundedRawDetail(
          resolution.result,
          this.#maxRawDetailBytes,
        ),
      });
      await this.#sendError(
        request.id,
        -32603,
        "Crew produced an invalid response for the app-server request",
      );
      return;
    }
    await this.#transport.send(
      JSON.stringify({ id: request.id, result: resolution.result }),
    );
  }

  async #handleDetachedServerRequest(
    request: Parameters<
      CodexControllerAuthority["resolveServerRequest"]
    >[0]["request"],
    sequence: number,
    raw: string,
  ): Promise<void> {
    try {
      await this.#handleServerRequest(request, sequence);
    } catch (error) {
      await this.#handleReceiveFailure(error, raw);
    }
  }

  async #sendError(
    id: JsonRpcId,
    code: number,
    message: string,
    data?: unknown,
  ): Promise<void> {
    await this.#transport.send(
      JSON.stringify({
        id,
        error: {
          code,
          message,
          ...(data === undefined ? {} : { data }),
        },
      }),
    );
  }

  async #handleReceiveFailure(error: unknown, raw: string): Promise<void> {
    const protocolError =
      error instanceof CodexProtocolError ? error : undefined;
    const disconnectCause =
      error instanceof Error ? error : new Error(String(error));
    const fault: CodexProtocolFault = {
      reasonCode:
        protocolError?.reasonCode === "malformed_known_notification"
          ? "malformed_known_notification"
          : protocolError?.reasonCode === "malformed_known_request"
            ? "malformed_known_request"
            : protocolError?.reasonCode === "malformed_response"
              ? "malformed_response"
              : "malformed_message",
      message: String(error),
      fatal: true,
      rawDetail: captureBoundedRawDetail(raw, this.#maxRawDetailBytes),
    };
    this.#state = "incompatible";
    // A transport close can arrive before close() resolves. Preserve the
    // originating protocol failure so pending requests keep its stable class.
    this.#disconnectCause = disconnectCause;
    await this.#reportFault(fault);
    await this.#transport.close();
    await this.#disconnect(
      `protocol failure: ${fault.message}`,
      disconnectCause,
    );
  }

  async #reportFault(fault: CodexProtocolFault): Promise<void> {
    await this.#authority.onProtocolFault(fault);
  }

  async #disconnect(reason: string, cause?: Error): Promise<void> {
    if (cause !== undefined) this.#disconnectCause = cause;
    if (this.#disconnectNotified) return;
    this.#disconnectNotified = true;
    if (this.#state !== "closed" && this.#state !== "incompatible") {
      this.#state = "disconnected";
    }
    const pendingClientRequestIds = [...this.#pending.keys()];
    const pendingFailure =
      this.#disconnectCause ??
      new Error(`Codex app-server disconnected: ${reason}`);
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.abortCleanup?.();
      pending.reject(pendingFailure);
    }
    this.#pending.clear();
    await this.#authority.onDisconnected({
      reason,
      pendingClientRequestIds,
      pendingServerRequestIds: [...this.#pendingServerRequests],
    });
    this.#pendingServerRequests.clear();
  }

  #rememberCompletedResponse(id: JsonRpcId): void {
    this.#completedResponseIds.add(id);
    this.#completedResponseOrder.push(id);
    if (this.#completedResponseOrder.length > 1_024) {
      const oldest = this.#completedResponseOrder.shift();
      if (oldest !== undefined) this.#completedResponseIds.delete(oldest);
    }
  }
}

function skippedProbeStep(stepId: string): CodexCompatibilityProbeStep {
  return {
    stepId,
    status: "skipped",
    durationMs: 0,
    reasonCode: "previous_required_probe_failed",
  };
}

function assertDynamicToolReadiness(): void {
  const namespace = CODEX_COORDINATION_DYNAMIC_TOOLS.find(
    (entry) => entry.type === "namespace" && entry.name === "rusty_crew",
  );
  if (namespace?.type !== "namespace") {
    throw new CodexProtocolError(
      "malformed_response",
      "Rusty Crew dynamic-tool namespace is not registered",
    );
  }
  const names = new Set(namespace.tools.map((tool) => tool.name));
  for (const required of [
    "list_agents",
    "send_agent_message",
    "agent_round",
    "submit_task_for_review",
    "complete_routed_review",
  ]) {
    if (!names.has(required)) {
      throw new CodexProtocolError(
        "malformed_response",
        `Rusty Crew dynamic tool ${required} is not registered`,
      );
    }
  }
}
