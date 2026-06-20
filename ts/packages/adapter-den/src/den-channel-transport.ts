import type {
  NormalizedChannelInboundMessage,
  NormalizedChannelOutboundMessage,
} from "@rusty-crew/contracts";
import type {
  DenChannelsBindingContext,
  DenChannelsInboundFixture,
  DenChannelsPostMessageRequest,
} from "./den-channels.js";
import {
  isExpiredChannelInboundMessage,
  normalizeDenChannelsInboundEvent,
  toDenChannelsPostMessageRequest,
} from "./den-channels.js";

export type DenChannelsTransportKind = "websocket" | "http_poll" | "simulation";
export type DenChannelsConnectivityState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "degraded";

export interface DenChannelsCursorStore {
  read(key: string): Promise<string | undefined> | string | undefined;
  write(key: string, value: string): Promise<void> | void;
}

export interface DenChannelsRetryPolicy {
  maxAttempts: number;
  backoffMs: readonly number[];
}

export interface DenChannelsTransportOpenRequest {
  cursor?: string;
}

export interface DenChannelsTransport {
  readonly kind: DenChannelsTransportKind;
  readonly name: string;
  open(request: DenChannelsTransportOpenRequest): Promise<void> | void;
  close(): Promise<void> | void;
  send(request: DenChannelsPostMessageRequest): Promise<void> | void;
}

export interface DenChannelsTransportControllerOptions {
  binding: DenChannelsBindingContext;
  cursorStore: DenChannelsCursorStore;
  cursorKey: string;
  transports: readonly DenChannelsTransport[];
  retryPolicy?: DenChannelsRetryPolicy;
  subscriptionCursor?: () => Promise<string | undefined> | string | undefined;
  maxDedupeKeys?: number;
}

export interface DenChannelsTransportStatus {
  state: DenChannelsConnectivityState;
  activeTransport?: string;
  activeTransportKind?: DenChannelsTransportKind;
  lastCursor?: string;
  droppedDuplicates: number;
  droppedExpired: number;
  droppedStaleCursor: number;
  reconnectAttempts: number;
  lastError?: string;
}

export type DenChannelsInboundDecision =
  | {
      accepted: true;
      message: NormalizedChannelInboundMessage;
      cursor?: string;
    }
  | {
      accepted: false;
      reason: "duplicate" | "expired" | "stale_cursor";
      message: NormalizedChannelInboundMessage;
      cursor?: string;
    };

export interface DenChannelsReconnectAttempt {
  attempt: number;
  transport: string;
  transportKind: DenChannelsTransportKind;
  delayMs: number;
  cursor?: string;
  connected: boolean;
  error?: string;
}

export class InMemoryDenChannelsCursorStore implements DenChannelsCursorStore {
  readonly #values = new Map<string, string>();

  read(key: string): string | undefined {
    return this.#values.get(key);
  }

  write(key: string, value: string): void {
    this.#values.set(key, value);
  }
}

export class DenChannelsTransportController {
  readonly #binding: DenChannelsBindingContext;
  readonly #cursorStore: DenChannelsCursorStore;
  readonly #cursorKey: string;
  readonly #transports: readonly DenChannelsTransport[];
  readonly #retryPolicy: DenChannelsRetryPolicy;
  readonly #subscriptionCursor:
    | (() => Promise<string | undefined> | string | undefined)
    | undefined;
  readonly #maxDedupeKeys: number;
  readonly #dedupeKeys: string[] = [];

  #state: DenChannelsConnectivityState = "disconnected";
  #activeTransport: DenChannelsTransport | undefined;
  #lastCursor: string | undefined;
  #droppedDuplicates = 0;
  #droppedExpired = 0;
  #droppedStaleCursor = 0;
  #reconnectAttempts = 0;
  #lastError: string | undefined;

  constructor(options: DenChannelsTransportControllerOptions) {
    if (options.transports.length === 0) {
      throw new Error("Den Channels transport controller requires a transport");
    }

    this.#binding = options.binding;
    this.#cursorStore = options.cursorStore;
    this.#cursorKey = options.cursorKey;
    this.#transports = options.transports;
    this.#retryPolicy = options.retryPolicy ?? {
      maxAttempts: 3,
      backoffMs: [250, 1_000, 5_000],
    };
    this.#subscriptionCursor = options.subscriptionCursor;
    this.#maxDedupeKeys = options.maxDedupeKeys ?? 1_000;
  }

  status(): DenChannelsTransportStatus {
    return {
      state: this.#state,
      activeTransport: this.#activeTransport?.name,
      activeTransportKind: this.#activeTransport?.kind,
      lastCursor: this.#lastCursor,
      droppedDuplicates: this.#droppedDuplicates,
      droppedExpired: this.#droppedExpired,
      droppedStaleCursor: this.#droppedStaleCursor,
      reconnectAttempts: this.#reconnectAttempts,
      lastError: this.#lastError,
    };
  }

  async connect(): Promise<DenChannelsReconnectAttempt[]> {
    return this.#connectWithAttempts(false);
  }

  async reconnect(): Promise<DenChannelsReconnectAttempt[]> {
    this.#state = "degraded";
    return this.#connectWithAttempts(true);
  }

  async disconnect(): Promise<void> {
    await this.#activeTransport?.close();
    this.#activeTransport = undefined;
    this.#state = "disconnected";
  }

  async send(message: NormalizedChannelOutboundMessage): Promise<void> {
    if (this.#activeTransport === undefined || this.#state !== "connected") {
      throw new Error("Den Channels transport is not connected");
    }
    await this.#activeTransport.send(toDenChannelsPostMessageRequest(message));
  }

  async acceptInbound(
    event: DenChannelsInboundFixture,
    now?: string,
  ): Promise<DenChannelsInboundDecision> {
    const message = normalizeDenChannelsInboundEvent(event, this.#binding);
    const cursor = message.cursor;

    if (isExpiredChannelInboundMessage(message, now)) {
      this.#droppedExpired += 1;
      return { accepted: false, reason: "expired", message, cursor };
    }

    if (this.#dedupeKeys.includes(message.idempotencyKey)) {
      this.#droppedDuplicates += 1;
      return { accepted: false, reason: "duplicate", message, cursor };
    }

    if (isStaleCursor(cursor, this.#lastCursor)) {
      this.#droppedStaleCursor += 1;
      return { accepted: false, reason: "stale_cursor", message, cursor };
    }

    this.#rememberDedupeKey(message.idempotencyKey);
    await this.#advanceCursor(cursor);
    return { accepted: true, message, cursor };
  }

  async #connectWithAttempts(
    reconnecting: boolean,
  ): Promise<DenChannelsReconnectAttempt[]> {
    this.#state = "connecting";
    const attempts: DenChannelsReconnectAttempt[] = [];
    const cursor = await this.#resumeCursor();
    const maxAttempts = Math.max(1, this.#retryPolicy.maxAttempts);

    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      const transport =
        this.#transports[(attempt - 1) % this.#transports.length]!;
      const delayMs = reconnecting ? this.#retryDelay(attempt) : 0;
      try {
        await transport.open({ cursor });
        this.#activeTransport = transport;
        this.#state = "connected";
        this.#lastError = undefined;
        attempts.push({
          attempt,
          transport: transport.name,
          transportKind: transport.kind,
          delayMs,
          cursor,
          connected: true,
        });
        return attempts;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        this.#lastError = message;
        this.#reconnectAttempts += 1;
        attempts.push({
          attempt,
          transport: transport.name,
          transportKind: transport.kind,
          delayMs,
          cursor,
          connected: false,
          error: message,
        });
      }
    }

    this.#activeTransport = undefined;
    this.#state = "degraded";
    return attempts;
  }

  async #resumeCursor(): Promise<string | undefined> {
    const stored = await this.#cursorStore.read(this.#cursorKey);
    const subscription = await this.#subscriptionCursor?.();
    const cursor = greatestCursor(stored, subscription);
    this.#lastCursor = cursor;
    return cursor;
  }

  async #advanceCursor(cursor: string | undefined): Promise<void> {
    if (cursor === undefined) return;
    if (isStaleCursor(cursor, this.#lastCursor)) return;
    this.#lastCursor = cursor;
    await this.#cursorStore.write(this.#cursorKey, cursor);
  }

  #rememberDedupeKey(key: string): void {
    this.#dedupeKeys.push(key);
    if (this.#dedupeKeys.length > this.#maxDedupeKeys) {
      this.#dedupeKeys.splice(0, this.#dedupeKeys.length - this.#maxDedupeKeys);
    }
  }

  #retryDelay(attempt: number): number {
    return (
      this.#retryPolicy.backoffMs[attempt - 1] ??
      this.#retryPolicy.backoffMs.at(-1) ??
      0
    );
  }
}

export interface SimulatedDenChannelsTransport extends DenChannelsTransport {
  readonly openedWithCursors: Array<string | undefined>;
  readonly sent: DenChannelsPostMessageRequest[];
  failNextOpen(error?: Error): void;
}

export function createSimulatedDenChannelsTransport(
  name = "simulation",
): SimulatedDenChannelsTransport {
  let open = false;
  let nextOpenFailure: Error | undefined;
  const openedWithCursors: Array<string | undefined> = [];
  const sent: DenChannelsPostMessageRequest[] = [];

  return {
    kind: "simulation",
    name,
    openedWithCursors,
    sent,

    failNextOpen(error = new Error("simulated Den Channels open failure")) {
      nextOpenFailure = error;
    },

    open(request): void {
      openedWithCursors.push(request.cursor);
      if (nextOpenFailure) {
        const error = nextOpenFailure;
        nextOpenFailure = undefined;
        throw error;
      }
      open = true;
    },

    close(): void {
      open = false;
    },

    send(request): void {
      if (!open) {
        throw new Error("simulated Den Channels transport is closed");
      }
      sent.push(request);
    },
  };
}

function isStaleCursor(
  cursor: string | undefined,
  lastCursor: string | undefined,
): boolean {
  if (cursor === undefined || lastCursor === undefined) return false;
  const current = Number(cursor);
  const last = Number(lastCursor);
  if (Number.isSafeInteger(current) && Number.isSafeInteger(last)) {
    return current <= last;
  }
  return cursor <= lastCursor;
}

function greatestCursor(
  left: string | undefined,
  right: string | undefined,
): string | undefined {
  if (left === undefined) return right;
  if (right === undefined) return left;
  return isStaleCursor(left, right) ? right : left;
}
