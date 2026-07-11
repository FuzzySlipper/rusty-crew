import type { InitializeParams } from "../protocol/0.144.1/ts/InitializeParams";
import type { InitializeResponse } from "../protocol/0.144.1/ts/InitializeResponse";
import type { ThreadCompactStartParams } from "../protocol/0.144.1/ts/v2/ThreadCompactStartParams";
import type { ThreadCompactStartResponse } from "../protocol/0.144.1/ts/v2/ThreadCompactStartResponse";
import type { ThreadItemsListParams } from "../protocol/0.144.1/ts/v2/ThreadItemsListParams";
import type { ThreadItemsListResponse } from "../protocol/0.144.1/ts/v2/ThreadItemsListResponse";
import type { ThreadListParams } from "../protocol/0.144.1/ts/v2/ThreadListParams";
import type { ThreadListResponse } from "../protocol/0.144.1/ts/v2/ThreadListResponse";
import type { ThreadLoadedListParams } from "../protocol/0.144.1/ts/v2/ThreadLoadedListParams";
import type { ThreadLoadedListResponse } from "../protocol/0.144.1/ts/v2/ThreadLoadedListResponse";
import type { ThreadReadParams } from "../protocol/0.144.1/ts/v2/ThreadReadParams";
import type { ThreadReadResponse } from "../protocol/0.144.1/ts/v2/ThreadReadResponse";
import type { ThreadResumeParams } from "../protocol/0.144.1/ts/v2/ThreadResumeParams";
import type { ThreadResumeResponse } from "../protocol/0.144.1/ts/v2/ThreadResumeResponse";
import type { ThreadStartParams } from "../protocol/0.144.1/ts/v2/ThreadStartParams";
import type { ThreadStartResponse } from "../protocol/0.144.1/ts/v2/ThreadStartResponse";
import type { ThreadTurnsListParams } from "../protocol/0.144.1/ts/v2/ThreadTurnsListParams";
import type { ThreadTurnsListResponse } from "../protocol/0.144.1/ts/v2/ThreadTurnsListResponse";
import type { TurnInterruptParams } from "../protocol/0.144.1/ts/v2/TurnInterruptParams";
import type { TurnInterruptResponse } from "../protocol/0.144.1/ts/v2/TurnInterruptResponse";
import type { TurnStartParams } from "../protocol/0.144.1/ts/v2/TurnStartParams";
import type { TurnStartResponse } from "../protocol/0.144.1/ts/v2/TurnStartResponse";
import type { TurnSteerParams } from "../protocol/0.144.1/ts/v2/TurnSteerParams";
import type { TurnSteerResponse } from "../protocol/0.144.1/ts/v2/TurnSteerResponse";
import {
  CodexProtocolCodec,
  CodexProtocolError,
  type JsonRpcResponseMessage,
} from "./codec.js";
import {
  mapNotification,
  mapUnsupportedServerRequest,
} from "./event-mapper.js";
import { CODEX_APP_SERVER_PROTOCOL } from "./protocol-manifest.js";
import { captureBoundedRawDetail } from "./raw-detail.js";
import type { CodexJsonRpcTransport } from "./transport.js";
import type {
  CodexControllerAuthority,
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

  constructor(
    transport: CodexJsonRpcTransport,
    authority: CodexControllerAuthority,
    options: CodexDriverOptions = {},
  ) {
    this.#transport = transport;
    this.#authority = authority;
    this.#requestTimeoutMs = options.requestTimeoutMs ?? 30_000;
    this.#maxPendingRequests = options.maxPendingRequests ?? 256;
    this.#maxRawDetailBytes = options.maxRawDetailBytes ?? 64 * 1024;
    if (
      !Number.isFinite(this.#requestTimeoutMs) ||
      this.#requestTimeoutMs <= 0 ||
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

  async connect(): Promise<InitializeResponse> {
    if (this.#state !== "disconnected") {
      throw new Error(`cannot connect Codex driver from ${this.#state}`);
    }
    this.#state = "connecting";
    this.#disconnectNotified = false;
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
        void this.#reportFault({
          reasonCode: "transport_error",
          message: error.message,
          fatal: true,
        });
        void this.#disconnect(`transport error: ${error.message}`);
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
      const authorization = await this.#authority.authorizeHandshake({
        userAgent: initialized.userAgent,
        codexHome: initialized.codexHome,
        platformFamily: initialized.platformFamily,
        platformOs: initialized.platformOs,
        protocol: CODEX_APP_SERVER_PROTOCOL,
      });
      if (!authorization.accepted) {
        this.#state = "incompatible";
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

  threadList(
    params: ThreadListParams,
    signal?: AbortSignal,
  ): Promise<ThreadListResponse> {
    return this.#request("thread/list", params, signal);
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

  threadStart(
    params: ThreadStartParams,
    signal?: AbortSignal,
  ): Promise<ThreadStartResponse> {
    return this.#request("thread/start", params, signal);
  }

  threadResume(
    params: ThreadResumeParams,
    signal?: AbortSignal,
  ): Promise<ThreadResumeResponse> {
    return this.#request("thread/resume", params, signal);
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
        await this.#handleServerRequest(decoded.request, sequence);
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
    const fault: CodexProtocolFault = {
      reasonCode:
        protocolError?.reasonCode === "malformed_known_notification"
          ? "malformed_known_notification"
          : protocolError?.reasonCode === "malformed_known_request"
            ? "malformed_known_request"
            : "malformed_message",
      message: String(error),
      fatal: true,
      rawDetail: captureBoundedRawDetail(raw, this.#maxRawDetailBytes),
    };
    this.#state = "incompatible";
    await this.#reportFault(fault);
    await this.#transport.close();
    await this.#disconnect(`protocol failure: ${fault.message}`);
  }

  async #reportFault(fault: CodexProtocolFault): Promise<void> {
    await this.#authority.onProtocolFault(fault);
  }

  async #disconnect(reason: string): Promise<void> {
    if (this.#disconnectNotified) return;
    this.#disconnectNotified = true;
    if (this.#state !== "closed" && this.#state !== "incompatible") {
      this.#state = "disconnected";
    }
    const pendingClientRequestIds = [...this.#pending.keys()];
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.abortCleanup?.();
      pending.reject(new Error(`Codex app-server disconnected: ${reason}`));
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
