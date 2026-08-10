import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import type {
  AdapterId,
  AgentId,
  ChannelAttachmentRef,
  ChannelBindingRecord,
  ChannelVisibility,
  NormalizedChannelInboundMessage,
  NormalizedChannelOutboundMessage,
  PlatformAdapterRegistration,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";

export function createTelegramAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "telegram", displayName: "Telegram" };
}

export interface TelegramUser {
  id: number;
  username?: string;
  first_name?: string;
  last_name?: string;
  is_bot?: boolean;
}

export interface TelegramMessageEntity {
  type: string;
  offset: number;
  length: number;
  user?: TelegramUser;
}

export interface TelegramChat {
  id: number | string;
  type: "private" | "group" | "supergroup" | "channel" | string;
  title?: string;
  username?: string;
}

export interface TelegramPhotoSize {
  file_id: string;
  file_unique_id?: string;
  file_size?: number;
  width?: number;
  height?: number;
}

export interface TelegramDocument {
  file_id: string;
  file_unique_id?: string;
  file_name?: string;
  mime_type?: string;
  file_size?: number;
}

export interface TelegramFile {
  file_id: string;
  file_unique_id: string;
  file_size?: number;
  file_path?: string;
}

export interface TelegramDownloadedFile {
  bytes: Uint8Array;
  contentType?: string;
}

export interface TelegramMessage {
  message_id: number;
  message_thread_id?: number;
  date: number;
  edit_date?: number;
  chat: TelegramChat;
  from?: TelegramUser;
  sender_chat?: TelegramChat;
  text?: string;
  entities?: TelegramMessageEntity[];
  caption?: string;
  caption_entities?: TelegramMessageEntity[];
  photo?: TelegramPhotoSize[];
  document?: TelegramDocument;
  reply_to_message?: Pick<TelegramMessage, "message_id" | "from">;
}

export interface TelegramUpdate {
  update_id: number;
  message?: TelegramMessage;
  edited_message?: TelegramMessage;
  channel_post?: TelegramMessage;
  edited_channel_post?: TelegramMessage;
}

export interface TelegramGetUpdatesRequest {
  offset?: number;
  limit?: number;
  timeout?: number;
  allowed_updates?: string[];
}

export interface TelegramSendMessageRequest {
  chat_id: number | string;
  text: string;
  message_thread_id?: number;
  reply_parameters?: {
    message_id: number;
  };
  parse_mode?: "MarkdownV2" | "HTML";
  link_preview_options?: {
    is_disabled: boolean;
  };
}

export interface TelegramBotApiClient {
  getMe?(): Promise<TelegramUser> | TelegramUser;
  getUpdates?(
    request?: TelegramGetUpdatesRequest,
  ): Promise<TelegramUpdate[]> | TelegramUpdate[];
  sendMessage(request: TelegramSendMessageRequest): Promise<unknown> | unknown;
  getFile?(fileId: string): Promise<TelegramFile> | TelegramFile;
  downloadFile?(
    filePath: string,
    maxBytes: number,
  ): Promise<TelegramDownloadedFile> | TelegramDownloadedFile;
}

export interface TelegramBindingInput {
  adapterId: AdapterId;
  bindingId: string;
  agentId: AgentId;
  profileId: ProfileId;
  chat: TelegramChat;
  sessionId?: SessionId;
  threadId?: number;
  externalUserId?: string;
  status?: ChannelBindingRecord["status"];
  createdAt?: string;
  updatedAt?: string;
}

export interface TelegramNormalizeContext {
  binding: ChannelBindingRecord;
  ttlMs: number;
  visibility?: ChannelVisibility;
}

export interface TelegramChannelAdapterOptions {
  adapterId: AdapterId;
  bot: TelegramBotApiClient;
}

export interface TelegramChannelAdapter {
  registration(): PlatformAdapterRegistration;
  normalizeUpdate(
    update: TelegramUpdate,
    context: TelegramNormalizeContext,
  ): NormalizedChannelInboundMessage | undefined;
  sendOutbound(message: NormalizedChannelOutboundMessage): Promise<unknown>;
}

export function createTelegramChannelAdapter(
  options: TelegramChannelAdapterOptions,
): TelegramChannelAdapter {
  return {
    registration(): PlatformAdapterRegistration {
      return createTelegramAdapterRegistration(options.adapterId);
    },
    normalizeUpdate(update, context) {
      return normalizeTelegramUpdate(update, context);
    },
    sendOutbound(message) {
      return Promise.resolve(
        options.bot.sendMessage(toTelegramSendMessageRequest(message)),
      );
    },
  };
}

export interface TelegramBotApiHttpClientOptions {
  token: string;
  baseUrl?: string;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
}

export function createTelegramBotApiHttpClient(
  options: TelegramBotApiHttpClientOptions,
): TelegramBotApiClient {
  const token = options.token.trim();
  if (!token) throw new Error("Telegram bot token must not be empty");
  const baseUrl = (options.baseUrl ?? "https://api.telegram.org").replace(
    /\/+$/,
    "",
  );
  const timeoutMs = options.timeoutMs ?? 30_000;
  const fetchImpl = options.fetchImpl ?? fetch;
  const apiUrl = (method: string) => `${baseUrl}/bot${token}/${method}`;
  const fileUrl = (filePath: string) =>
    `${baseUrl}/file/bot${token}/${filePath.replace(/^\/+/, "")}`;

  return {
    async getMe() {
      return telegramApiRequest<TelegramUser>(fetchImpl, {
        url: apiUrl("getMe"),
        body: {},
        timeoutMs,
        resultName: "getMe",
      });
    },
    async getUpdates(request = {}) {
      return telegramApiRequest<TelegramUpdate[]>(fetchImpl, {
        url: apiUrl("getUpdates"),
        body: request,
        timeoutMs,
        resultName: "getUpdates",
      });
    },
    async sendMessage(request) {
      return telegramApiRequest(fetchImpl, {
        url: apiUrl("sendMessage"),
        body: request,
        timeoutMs,
        resultName: "sendMessage",
      });
    },
    async getFile(fileId) {
      return telegramApiRequest<TelegramFile>(fetchImpl, {
        url: apiUrl("getFile"),
        body: { file_id: fileId },
        timeoutMs,
        resultName: "getFile",
      });
    },
    async downloadFile(filePath, maxBytes) {
      return telegramFileRequest(fetchImpl, {
        url: fileUrl(filePath),
        timeoutMs,
        maxBytes,
      });
    },
  };
}

export interface TelegramApiResponse<T> {
  ok: boolean;
  result?: T;
  description?: string;
  error_code?: number;
  parameters?: {
    retry_after?: number;
  };
}

export class TelegramBotApiError extends Error {
  readonly status: number;
  readonly errorCode: number | undefined;
  readonly retryAfterSeconds: number | undefined;
  readonly retryable: boolean;

  constructor(input: {
    method: string;
    status: number;
    description: string;
    errorCode?: number;
    retryAfterSeconds?: number;
    retryable?: boolean;
  }) {
    super(
      `Telegram Bot API ${input.method} failed: ${input.status}${input.errorCode === undefined ? "" : `/${input.errorCode}`} ${input.description}`,
    );
    this.name = "TelegramBotApiError";
    this.status = input.status;
    this.errorCode = input.errorCode;
    this.retryAfterSeconds = input.retryAfterSeconds;
    this.retryable =
      input.retryable ??
      (input.status === 408 ||
        input.status === 409 ||
        input.status === 425 ||
        input.status === 429 ||
        input.status >= 500);
  }
}

async function telegramApiRequest<T>(
  fetchImpl: typeof fetch,
  input: {
    url: string;
    body: unknown;
    timeoutMs: number;
    resultName: string;
  },
): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), input.timeoutMs);
  try {
    const response = await fetchImpl(input.url, {
      method: "POST",
      signal: controller.signal,
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(input.body),
    });
    const text = await response.text();
    let parsed: TelegramApiResponse<T>;
    try {
      parsed = text.trim()
        ? (JSON.parse(text) as TelegramApiResponse<T>)
        : {
            ok: false,
            description: "empty response",
          };
    } catch {
      throw new TelegramBotApiError({
        method: input.resultName,
        status: response.status,
        description: "response was not valid JSON",
        retryable: true,
      });
    }
    if (!response.ok || !parsed.ok) {
      const detail = parsed.description ?? response.statusText;
      throw new TelegramBotApiError({
        method: input.resultName,
        status: response.status,
        description: detail,
        errorCode: parsed.error_code,
        retryAfterSeconds: parsed.parameters?.retry_after,
      });
    }
    if (parsed.result === undefined) {
      throw new Error(
        `Telegram Bot API ${input.resultName} returned no result`,
      );
    }
    return parsed.result;
  } finally {
    clearTimeout(timer);
  }
}

async function telegramFileRequest(
  fetchImpl: typeof fetch,
  input: { url: string; timeoutMs: number; maxBytes: number },
): Promise<TelegramDownloadedFile> {
  if (!Number.isSafeInteger(input.maxBytes) || input.maxBytes < 1) {
    throw new Error("Telegram download byte limit must be positive");
  }
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), input.timeoutMs);
  try {
    const response = await fetchImpl(input.url, {
      method: "GET",
      signal: controller.signal,
      headers: { Accept: "*/*" },
    });
    if (!response.ok) {
      throw new TelegramBotApiError({
        method: "file download",
        status: response.status,
        description: response.statusText || "download failed",
      });
    }
    const contentLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(contentLength) && contentLength > input.maxBytes) {
      throw new TelegramMediaError(
        "telegram_media_oversized",
        `Telegram file is ${contentLength} bytes; maximum is ${input.maxBytes}`,
        false,
      );
    }
    if (response.body === null) {
      throw new TelegramMediaError(
        "telegram_media_download_empty",
        "Telegram file download returned no response body",
        true,
      );
    }
    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let length = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > input.maxBytes) {
        await reader.cancel().catch(() => undefined);
        throw new TelegramMediaError(
          "telegram_media_oversized",
          `Telegram file exceeds maximum ${input.maxBytes} bytes`,
          false,
        );
      }
      chunks.push(value);
    }
    if (length === 0) {
      throw new TelegramMediaError(
        "telegram_media_download_empty",
        "Telegram file download was empty",
        true,
      );
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return {
      bytes,
      contentType: response.headers.get("content-type") ?? undefined,
    };
  } finally {
    clearTimeout(timer);
  }
}

export interface TelegramUpdateOffsetStore {
  read(): Promise<number | undefined> | number | undefined;
  write(offset: number): Promise<void> | void;
}

export class MemoryTelegramUpdateOffsetStore implements TelegramUpdateOffsetStore {
  #offset: number | undefined;

  constructor(initialOffset?: number) {
    this.#offset = initialOffset;
  }

  read(): number | undefined {
    return this.#offset;
  }

  write(offset: number): void {
    this.#offset = offset;
  }
}

export class FileTelegramUpdateOffsetStore implements TelegramUpdateOffsetStore {
  readonly #path: string;

  constructor(path: string) {
    this.#path = path;
  }

  async read(): Promise<number | undefined> {
    try {
      const raw = await readFile(this.#path, "utf8");
      const parsed = JSON.parse(raw) as { offset?: unknown };
      return safeTelegramOffset(parsed.offset);
    } catch (error) {
      if (isNodeError(error) && error.code === "ENOENT") return undefined;
      throw error;
    }
  }

  async write(offset: number): Promise<void> {
    await mkdir(dirname(this.#path), { recursive: true, mode: 0o750 });
    await writeFile(
      this.#path,
      JSON.stringify({ offset, updatedAt: new Date().toISOString() }, null, 2),
      { mode: 0o640 },
    );
  }
}

export type TelegramTerminalUpdateRecord =
  | {
      disposition: "non_executable";
      updateId: number;
      reason: TelegramNonExecutableUpdate["reason"];
      recordedAt: string;
    }
  | ({ disposition: "quarantined" } & TelegramQuarantinedUpdate);

export interface TelegramUpdateTerminalStore {
  record(update: TelegramTerminalUpdateRecord): Promise<void> | void;
}

export class MemoryTelegramUpdateTerminalStore implements TelegramUpdateTerminalStore {
  readonly records: TelegramTerminalUpdateRecord[] = [];

  record(update: TelegramTerminalUpdateRecord): void {
    this.records.push(update);
  }
}

export class FileTelegramUpdateTerminalStore implements TelegramUpdateTerminalStore {
  readonly #path: string;

  constructor(path: string) {
    this.#path = path;
  }

  async record(update: TelegramTerminalUpdateRecord): Promise<void> {
    await mkdir(dirname(this.#path), { recursive: true, mode: 0o750 });
    await appendFile(this.#path, `${JSON.stringify(update)}\n`, {
      encoding: "utf8",
      mode: 0o640,
    });
  }
}

export interface TelegramConnectorIngestResult {
  status: string;
  reason?: string;
  retryable?: boolean;
}

export interface TelegramQuarantinedUpdate {
  updateId: number;
  attempts: number;
  reason: string;
  quarantinedAt: string;
  updateShape: string;
}

export interface TelegramNonExecutableUpdate {
  updateId: number;
  reason: "edited_message" | "edited_channel_post" | "unsupported_update";
  message?: NormalizedChannelInboundMessage;
}

export interface TelegramDeliveryReceipt {
  idempotencyKey: string;
  chunkCount: number;
  attempts: number;
  externalMessageIds: string[];
}

export interface TelegramMediaPersistenceInput {
  sessionId: string;
  adapterId: AdapterId;
  botUserId?: string;
  bindingId: string;
  fileId: string;
  fileUniqueId: string;
  filename: string;
  mediaType: string;
  bytes: Uint8Array;
  provenance: {
    externalChannelId: string;
    externalThreadId?: string;
    externalMessageId: string;
    externalUserId: string;
    updateId: number;
  };
}

export interface TelegramMediaPersistenceResult {
  attachmentId: string;
  filename: string;
  mediaType: string;
  byteSize: number;
  sha256: string;
  contentUrl: string;
}

export class TelegramMediaError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
    readonly retryable: boolean,
  ) {
    super(message);
    this.name = "TelegramMediaError";
  }
}

export interface TelegramConnectorOptions {
  adapterId: AdapterId;
  bot: TelegramBotApiClient;
  offsetStore: TelegramUpdateOffsetStore;
  terminalStore: TelegramUpdateTerminalStore;
  bindings: () => readonly ChannelBindingRecord[];
  ingest(
    message: NormalizedChannelInboundMessage,
  ): Promise<TelegramConnectorIngestResult> | TelegramConnectorIngestResult;
  ttlMs: number;
  visibility?: ChannelVisibility;
  pollIntervalMs?: number;
  pollTimeoutSeconds?: number;
  updateLimit?: number;
  maxInboundAttempts?: number;
  maxOutboundAttempts?: number;
  maxMessageChars?: number;
  maxImageBytes?: number;
  maxDocumentBytes?: number;
  botUserId?: string;
  botUsername?: string;
  participationMode?:
    | "all_delivered"
    | "mention_or_reply"
    | "topic_human_messages";
  isCorrelatedBotMessage?: (
    message: NormalizedChannelInboundMessage,
  ) => boolean;
  persistMedia?: (
    input: TelegramMediaPersistenceInput,
  ) => Promise<TelegramMediaPersistenceResult> | TelegramMediaPersistenceResult;
  onNonExecutableUpdate?: (
    update: TelegramNonExecutableUpdate,
  ) => Promise<void> | void;
  onQuarantine?: (update: TelegramQuarantinedUpdate) => Promise<void> | void;
  now?: () => string;
  wait?: (delayMs: number) => Promise<void>;
  setTimer?: typeof setTimeout;
  clearTimer?: typeof clearTimeout;
}

export interface TelegramConnectorDiagnostics {
  enabled: boolean;
  running: boolean;
  adapterId: AdapterId;
  bindingCount: number;
  pollCount: number;
  lastPollAt?: string;
  lastInboundAt?: string;
  lastOutboundAt?: string;
  lastUpdateId?: number;
  nextOffset?: number;
  lastError?: string;
  botIdentity?: {
    userId: string;
    username?: string;
    displayLabel?: string;
  };
  candidates: TelegramDiplomatSurfaceCandidate[];
  inbound: {
    routed: number;
    unbound: number;
    ambiguous: number;
    expired: number;
    duplicate: number;
    staleCursor: number;
    failed: number;
    humanMessages: number;
    botMessages: number;
    ignored: number;
    edited: number;
    unsupported: number;
    retryPending: number;
    quarantined: number;
    loopTerminated: number;
    rateLimited: number;
  };
  outbound: {
    sent: number;
    chunksSent: number;
    retried: number;
    failed: number;
    lastError?: string;
    lastExternalMessageId?: string;
  };
  media: {
    available: number;
    duplicate: number;
    unsupported: number;
    oversized: number;
    expired: number;
    failed: number;
    retried: number;
    bytesStored: number;
    lastError?: string;
  };
}

export interface TelegramDiplomatSurfaceCandidate {
  externalChatId: string;
  externalThreadId?: string;
  chatType: string;
  title?: string;
  username?: string;
  lastObservedAt: string;
  lastUpdateId: number;
}

export class TelegramChannelConnector {
  readonly #adapterId: AdapterId;
  readonly #bot: TelegramBotApiClient;
  readonly #adapter: TelegramChannelAdapter;
  readonly #offsetStore: TelegramUpdateOffsetStore;
  readonly #terminalStore: TelegramUpdateTerminalStore;
  readonly #bindings: () => readonly ChannelBindingRecord[];
  readonly #ingest: (
    message: NormalizedChannelInboundMessage,
  ) => Promise<TelegramConnectorIngestResult> | TelegramConnectorIngestResult;
  readonly #ttlMs: number;
  readonly #visibility: ChannelVisibility | undefined;
  readonly #pollIntervalMs: number;
  readonly #pollTimeoutSeconds: number;
  readonly #updateLimit: number;
  readonly #maxInboundAttempts: number;
  readonly #maxOutboundAttempts: number;
  readonly #maxMessageChars: number;
  readonly #maxImageBytes: number;
  readonly #maxDocumentBytes: number;
  readonly #botUserId: string | undefined;
  readonly #botUsername: string | undefined;
  readonly #participationMode: NonNullable<
    TelegramConnectorOptions["participationMode"]
  >;
  readonly #isCorrelatedBotMessage:
    | TelegramConnectorOptions["isCorrelatedBotMessage"]
    | undefined;
  readonly #persistMedia: TelegramConnectorOptions["persistMedia"];
  readonly #onNonExecutableUpdate:
    | TelegramConnectorOptions["onNonExecutableUpdate"]
    | undefined;
  readonly #onQuarantine: TelegramConnectorOptions["onQuarantine"] | undefined;
  readonly #now: () => string;
  readonly #wait: (delayMs: number) => Promise<void>;
  readonly #setTimer: typeof setTimeout;
  readonly #clearTimer: typeof clearTimeout;

  #running = false;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #polling = false;
  #lastPollAt: string | undefined;
  #lastInboundAt: string | undefined;
  #lastOutboundAt: string | undefined;
  #lastUpdateId: number | undefined;
  #nextOffset: number | undefined;
  #lastError: string | undefined;
  #pollCount = 0;
  #botIdentity: TelegramConnectorDiagnostics["botIdentity"];
  #candidates = new Map<string, TelegramDiplomatSurfaceCandidate>();
  #inboundAttempts = new Map<number, number>();
  #inbound = {
    routed: 0,
    unbound: 0,
    ambiguous: 0,
    expired: 0,
    duplicate: 0,
    staleCursor: 0,
    failed: 0,
    humanMessages: 0,
    botMessages: 0,
    ignored: 0,
    edited: 0,
    unsupported: 0,
    retryPending: 0,
    quarantined: 0,
    loopTerminated: 0,
    rateLimited: 0,
  };
  #outbound = {
    sent: 0,
    chunksSent: 0,
    retried: 0,
    failed: 0,
    lastError: undefined as string | undefined,
    lastExternalMessageId: undefined as string | undefined,
  };
  #media = {
    available: 0,
    duplicate: 0,
    unsupported: 0,
    oversized: 0,
    expired: 0,
    failed: 0,
    retried: 0,
    bytesStored: 0,
    lastError: undefined as string | undefined,
  };

  constructor(options: TelegramConnectorOptions) {
    this.#adapterId = options.adapterId;
    this.#bot = options.bot;
    this.#adapter = createTelegramChannelAdapter({
      adapterId: options.adapterId,
      bot: options.bot,
    });
    this.#offsetStore = options.offsetStore;
    this.#terminalStore = options.terminalStore;
    this.#bindings = options.bindings;
    this.#ingest = options.ingest;
    this.#ttlMs = options.ttlMs;
    this.#visibility = options.visibility;
    this.#pollIntervalMs = options.pollIntervalMs ?? 2_000;
    this.#pollTimeoutSeconds = options.pollTimeoutSeconds ?? 20;
    this.#updateLimit = options.updateLimit ?? 50;
    this.#maxInboundAttempts = options.maxInboundAttempts ?? 3;
    this.#maxOutboundAttempts = options.maxOutboundAttempts ?? 3;
    this.#maxMessageChars = options.maxMessageChars ?? 4_096;
    this.#maxImageBytes = options.maxImageBytes ?? 20 * 1024 * 1024;
    this.#maxDocumentBytes = options.maxDocumentBytes ?? 1024 * 1024;
    if (
      !Number.isSafeInteger(this.#maxInboundAttempts) ||
      this.#maxInboundAttempts < 1
    ) {
      throw new Error("Telegram inbound attempts must be a positive integer");
    }
    if (
      !Number.isSafeInteger(this.#maxOutboundAttempts) ||
      this.#maxOutboundAttempts < 1
    ) {
      throw new Error("Telegram outbound attempts must be a positive integer");
    }
    if (
      !Number.isSafeInteger(this.#maxMessageChars) ||
      this.#maxMessageChars < 1
    ) {
      throw new Error("Telegram message character limit must be positive");
    }
    if (!Number.isSafeInteger(this.#maxImageBytes) || this.#maxImageBytes < 1) {
      throw new Error("Telegram image byte limit must be positive");
    }
    if (
      !Number.isSafeInteger(this.#maxDocumentBytes) ||
      this.#maxDocumentBytes < 1
    ) {
      throw new Error("Telegram document byte limit must be positive");
    }
    this.#botUserId = options.botUserId;
    this.#botUsername = normalizeTelegramUsername(options.botUsername);
    this.#participationMode = options.participationMode ?? "all_delivered";
    this.#isCorrelatedBotMessage = options.isCorrelatedBotMessage;
    this.#persistMedia = options.persistMedia;
    this.#onNonExecutableUpdate = options.onNonExecutableUpdate;
    this.#onQuarantine = options.onQuarantine;
    this.#now = options.now ?? (() => new Date().toISOString());
    this.#wait = options.wait ?? waitForTelegramRetry;
    this.#setTimer = options.setTimer ?? setTimeout;
    this.#clearTimer = options.clearTimer ?? clearTimeout;
  }

  async start(): Promise<void> {
    if (this.#running) return;
    this.#running = true;
    this.#nextOffset = await this.#offsetStore.read();
    if (this.#bot.getMe !== undefined) {
      try {
        const identity = await this.#bot.getMe();
        this.#botIdentity = {
          userId: String(identity.id),
          username: identity.username,
          displayLabel:
            [identity.first_name, identity.last_name]
              .filter(Boolean)
              .join(" ") || undefined,
        };
      } catch (error) {
        this.#lastError = telegramErrorMessage(error);
        this.#running = false;
        return;
      }
    }
    this.#schedule(0);
  }

  stop(): void {
    this.#running = false;
    if (this.#timer !== undefined) {
      this.#clearTimer(this.#timer);
      this.#timer = undefined;
    }
  }

  async pollOnce(): Promise<void> {
    if (this.#polling) return;
    if (this.#bot.getUpdates === undefined) {
      this.#lastError = "Telegram Bot API client does not implement getUpdates";
      return;
    }
    if (!this.#running && this.#nextOffset === undefined) {
      this.#nextOffset = await this.#offsetStore.read();
    }
    this.#polling = true;
    this.#lastPollAt = this.#now();
    this.#pollCount += 1;
    try {
      const updates = await this.#bot.getUpdates({
        offset: this.#nextOffset,
        limit: this.#updateLimit,
        timeout: this.#pollTimeoutSeconds,
        allowed_updates: [
          "message",
          "edited_message",
          "channel_post",
          "edited_channel_post",
        ],
      });
      let retryPending = false;
      for (const update of [...updates].sort(
        (left, right) => left.update_id - right.update_id,
      )) {
        const disposition = await this.#handleUpdate(update);
        if (disposition === "retry_pending") {
          retryPending = true;
          break;
        }
      }
      if (!retryPending) this.#lastError = undefined;
    } catch (error) {
      this.#lastError = telegramErrorMessage(error);
    } finally {
      this.#polling = false;
    }
  }

  async sendOutbound(
    message: NormalizedChannelOutboundMessage,
  ): Promise<TelegramDeliveryReceipt> {
    const chunks = splitTelegramText(
      telegramOutboundBody(message),
      this.#maxMessageChars,
    );
    const externalMessageIds: string[] = [];
    let attempts = 0;
    try {
      for (const [index, chunk] of chunks.entries()) {
        const request = {
          ...toTelegramSendMessageRequest(message),
          text: chunk,
          reply_parameters:
            index === 0
              ? telegramReplyParameters(message.replyToExternalMessageId)
              : undefined,
        };
        const sent = await this.#sendMessageWithRetry(request);
        attempts += sent.attempts;
        const externalMessageId = telegramSentMessageId(sent.response);
        if (externalMessageId !== undefined) {
          externalMessageIds.push(externalMessageId);
          this.#outbound.lastExternalMessageId = externalMessageId;
        }
        this.#outbound.chunksSent += 1;
      }
      this.#outbound.sent += 1;
      this.#lastOutboundAt = this.#now();
      this.#outbound.lastError = undefined;
      return {
        idempotencyKey: message.idempotencyKey,
        chunkCount: chunks.length,
        attempts,
        externalMessageIds,
      };
    } catch (error) {
      this.#outbound.failed += 1;
      this.#outbound.lastError = telegramErrorMessage(error);
      throw error;
    }
  }

  diagnostics(): TelegramConnectorDiagnostics {
    return {
      enabled: true,
      running: this.#running,
      adapterId: this.#adapterId,
      bindingCount: this.#activeTelegramBindings().length,
      pollCount: this.#pollCount,
      lastPollAt: this.#lastPollAt,
      lastInboundAt: this.#lastInboundAt,
      lastOutboundAt: this.#lastOutboundAt,
      lastUpdateId: this.#lastUpdateId,
      nextOffset: this.#nextOffset,
      lastError: this.#lastError,
      botIdentity: this.#botIdentity,
      candidates: [...this.#candidates.values()].sort((left, right) =>
        right.lastObservedAt.localeCompare(left.lastObservedAt),
      ),
      inbound: { ...this.#inbound },
      outbound: { ...this.#outbound },
      media: { ...this.#media },
    };
  }

  #schedule(delayMs: number): void {
    if (!this.#running) return;
    this.#timer = this.#setTimer(() => {
      void this.pollOnce().finally(() => this.#schedule(this.#pollIntervalMs));
    }, delayMs);
  }

  async #sendMessageWithRetry(request: TelegramSendMessageRequest): Promise<{
    response: unknown;
    attempts: number;
  }> {
    let attempt = 0;
    for (;;) {
      attempt += 1;
      try {
        return {
          response: await this.#bot.sendMessage(request),
          attempts: attempt,
        };
      } catch (error) {
        if (
          attempt >= this.#maxOutboundAttempts ||
          !isRetryableTelegramError(error)
        ) {
          throw error;
        }
        this.#outbound.retried += 1;
        await this.#wait(telegramRetryDelayMs(error, attempt));
      }
    }
  }

  async #handleUpdate(
    update: TelegramUpdate,
  ): Promise<"advanced" | "retry_pending"> {
    const updateOffset = update.update_id + 1;
    this.#lastInboundAt = this.#now();
    try {
      this.#observeCandidate(update);
      const updateShape = telegramUpdateShape(update);
      const binding = this.#normalizationBinding(update);
      let message = this.#adapter.normalizeUpdate(update, {
        binding,
        ttlMs: this.#ttlMs,
        visibility: this.#visibility,
      });
      if (message === undefined) {
        await this.#terminalStore.record({
          disposition: "non_executable",
          updateId: update.update_id,
          reason: "unsupported_update",
          recordedAt: this.#now(),
        });
        await this.#notifyNonExecutableUpdate({
          updateId: update.update_id,
          reason: "unsupported_update",
        });
        this.#inbound.unsupported += 1;
        this.#inboundAttempts.delete(update.update_id);
        await this.#advanceOffset(updateOffset);
        return "advanced";
      }
      if (
        updateShape === "edited_message" ||
        updateShape === "edited_channel_post"
      ) {
        await this.#terminalStore.record({
          disposition: "non_executable",
          updateId: update.update_id,
          reason: updateShape,
          recordedAt: this.#now(),
        });
        await this.#notifyNonExecutableUpdate({
          updateId: update.update_id,
          reason: updateShape,
          message,
        });
        this.#inbound.edited += 1;
        this.#inboundAttempts.delete(update.update_id);
        await this.#advanceOffset(updateOffset);
        return "advanced";
      }
      if (message.author.isBot) {
        this.#inbound.botMessages += 1;
      } else {
        this.#inbound.humanMessages += 1;
      }
      if (!this.#shouldParticipate(message)) {
        this.#inbound.ignored += 1;
        this.#inboundAttempts.delete(update.update_id);
        await this.#advanceOffset(updateOffset);
        return "advanced";
      }
      message = await this.#materializeMedia(update, message);
      const result = await this.#ingest(message);
      if (
        result.retryable === true ||
        (!isTerminalTelegramIngestStatus(result.status) &&
          result.retryable !== false)
      ) {
        throw new Error(
          result.reason ??
            `Telegram ingress returned non-terminal status ${result.status}`,
        );
      }
      this.#countIngestResult(result.status);
      this.#inboundAttempts.delete(update.update_id);
      await this.#advanceOffset(updateOffset);
      return "advanced";
    } catch (error) {
      return this.#retryOrQuarantine(update, error);
    }
  }

  #observeCandidate(update: TelegramUpdate): void {
    const message =
      update.message ??
      update.edited_message ??
      update.channel_post ??
      update.edited_channel_post;
    if (message === undefined) return;
    const externalChatId = String(message.chat.id);
    const externalThreadId =
      message.message_thread_id === undefined
        ? undefined
        : String(message.message_thread_id);
    this.#candidates.set(`${externalChatId}:${externalThreadId ?? ""}`, {
      externalChatId,
      externalThreadId,
      chatType: message.chat.type,
      title: message.chat.title,
      username: message.chat.username,
      lastObservedAt: this.#now(),
      lastUpdateId: update.update_id,
    });
  }

  #shouldParticipate(message: NormalizedChannelInboundMessage): boolean {
    if (this.#participationMode === "all_delivered") return true;
    const addressed = telegramMessageAddressesBot(message, {
      botUserId: this.#botIdentity?.userId ?? this.#botUserId,
      botUsername: this.#botIdentity?.username ?? this.#botUsername,
    });
    if (message.author.isBot) {
      return addressed || (this.#isCorrelatedBotMessage?.(message) ?? false);
    }
    return this.#participationMode === "topic_human_messages" || addressed;
  }

  async #materializeMedia(
    update: TelegramUpdate,
    message: NormalizedChannelInboundMessage,
  ): Promise<NormalizedChannelInboundMessage> {
    const candidates = telegramMediaCandidates(update);
    if (candidates.length === 0) return message;
    const resolved: ChannelAttachmentRef[] = [];
    const seenUniqueIds = new Set<string>();
    for (const candidate of candidates) {
      const uniqueId = candidate.fileUniqueId ?? candidate.fileId;
      if (seenUniqueIds.has(uniqueId)) {
        this.#media.duplicate += 1;
        continue;
      }
      seenUniqueIds.add(uniqueId);
      try {
        resolved.push(
          await this.#materializeMediaCandidate(candidate, message, update),
        );
      } catch (error) {
        const terminalError = terminalTelegramMediaError(error);
        if (terminalError !== undefined) {
          this.#countTerminalMediaFailure(
            terminalError.reasonCode,
            terminalError.message,
          );
          resolved.push({
            ref: `telegram:file:${candidate.fileId}`,
            mediaType: candidate.mediaType,
            label: candidate.filename,
            filename: candidate.filename,
            byteSize: candidate.fileSize,
            state: telegramMediaFailureState(terminalError.reasonCode),
            reasonCode: terminalError.reasonCode,
          });
          continue;
        }
        this.#media.retried += 1;
        this.#media.lastError = telegramErrorMessage(error);
        throw error;
      }
    }
    return { ...message, attachments: resolved };
  }

  async #materializeMediaCandidate(
    candidate: TelegramMediaCandidate,
    message: NormalizedChannelInboundMessage,
    update: TelegramUpdate,
  ): Promise<ChannelAttachmentRef> {
    if (message.runtime.sessionId === undefined) {
      throw new TelegramMediaError(
        "telegram_media_session_unbound",
        "Telegram media cannot be stored until the channel is bound to a session",
        false,
      );
    }
    if (
      this.#bot.getFile === undefined ||
      this.#bot.downloadFile === undefined ||
      this.#persistMedia === undefined
    ) {
      throw new TelegramMediaError(
        "telegram_media_pipeline_unconfigured",
        "Telegram media retrieval is not configured",
        false,
      );
    }
    const maxBytes =
      candidate.kind === "image" ? this.#maxImageBytes : this.#maxDocumentBytes;
    if (candidate.fileSize !== undefined && candidate.fileSize > maxBytes) {
      throw new TelegramMediaError(
        "telegram_media_oversized",
        `Telegram ${candidate.kind} is ${candidate.fileSize} bytes; maximum is ${maxBytes}`,
        false,
      );
    }
    const file = await this.#bot.getFile(candidate.fileId);
    if (
      candidate.fileUniqueId !== undefined &&
      file.file_unique_id !== candidate.fileUniqueId
    ) {
      throw new TelegramMediaError(
        "telegram_media_identity_mismatch",
        "Telegram getFile returned a different unique file identity",
        false,
      );
    }
    if (file.file_size !== undefined && file.file_size > maxBytes) {
      throw new TelegramMediaError(
        "telegram_media_oversized",
        `Telegram ${candidate.kind} is ${file.file_size} bytes; maximum is ${maxBytes}`,
        false,
      );
    }
    if (!file.file_path) {
      throw new TelegramMediaError(
        "telegram_media_expired",
        "Telegram no longer provides a downloadable path for this file",
        false,
      );
    }
    const downloaded = await this.#bot.downloadFile(file.file_path, maxBytes);
    if (
      file.file_size !== undefined &&
      downloaded.bytes.byteLength !== file.file_size
    ) {
      throw new TelegramMediaError(
        "telegram_media_download_incomplete",
        `Telegram file download returned ${downloaded.bytes.byteLength} of ${file.file_size} bytes`,
        true,
      );
    }
    const mediaType = validatedTelegramMediaType(
      candidate,
      downloaded.contentType,
    );
    const persisted = await this.#persistMedia({
      sessionId: message.runtime.sessionId,
      adapterId: this.#adapterId,
      botUserId: this.#botUserId,
      bindingId: message.bindingId,
      fileId: file.file_id,
      fileUniqueId: file.file_unique_id,
      filename: candidate.filename,
      mediaType,
      bytes: downloaded.bytes,
      provenance: {
        externalChannelId: message.providerRefs.externalChannelId,
        externalThreadId: message.providerRefs.externalThreadId,
        externalMessageId: message.providerRefs.externalMessageId ?? "unknown",
        externalUserId: message.author.externalUserId,
        updateId: update.update_id,
      },
    });
    this.#media.available += 1;
    this.#media.bytesStored += persisted.byteSize;
    this.#media.lastError = undefined;
    return {
      ref: `crew:attachment:${persisted.attachmentId}`,
      mediaType: persisted.mediaType,
      label: persisted.filename,
      attachmentId: persisted.attachmentId,
      filename: persisted.filename,
      byteSize: persisted.byteSize,
      sha256: persisted.sha256,
      contentUrl: persisted.contentUrl,
      state: "available" as const,
    };
  }

  #countTerminalMediaFailure(reasonCode: string, message: string): void {
    const state = telegramMediaFailureState(reasonCode);
    this.#media[state] += 1;
    this.#media.lastError = message;
  }

  async #retryOrQuarantine(
    update: TelegramUpdate,
    error: unknown,
  ): Promise<"advanced" | "retry_pending"> {
    const attempts = (this.#inboundAttempts.get(update.update_id) ?? 0) + 1;
    this.#inboundAttempts.set(update.update_id, attempts);
    this.#lastError = telegramErrorMessage(error);
    if (attempts < this.#maxInboundAttempts) {
      this.#inbound.retryPending += 1;
      return "retry_pending";
    }
    const quarantined: TelegramQuarantinedUpdate = {
      updateId: update.update_id,
      attempts,
      reason: telegramErrorMessage(error),
      quarantinedAt: this.#now(),
      updateShape: telegramUpdateShape(update),
    };
    try {
      await this.#terminalStore.record({
        disposition: "quarantined",
        ...quarantined,
      });
    } catch (quarantineError) {
      this.#lastError = `Telegram quarantine failed: ${telegramErrorMessage(quarantineError)}`;
      this.#inbound.retryPending += 1;
      return "retry_pending";
    }
    try {
      await this.#onQuarantine?.(quarantined);
    } catch (notificationError) {
      this.#lastError = `Telegram quarantine notification failed: ${telegramErrorMessage(notificationError)}`;
    }
    this.#inbound.failed += 1;
    this.#inbound.quarantined += 1;
    this.#inboundAttempts.delete(update.update_id);
    await this.#advanceOffset(update.update_id + 1);
    return "advanced";
  }

  async #notifyNonExecutableUpdate(
    update: TelegramNonExecutableUpdate,
  ): Promise<void> {
    try {
      await this.#onNonExecutableUpdate?.(update);
    } catch (error) {
      this.#lastError = `Telegram non-executable update notification failed: ${telegramErrorMessage(error)}`;
    }
  }

  #normalizationBinding(update: TelegramUpdate): ChannelBindingRecord {
    const refs = telegramUpdateRefs(update);
    if (refs === undefined) {
      return syntheticTelegramBinding(this.#adapterId, {
        externalChannelId: "unknown",
      });
    }
    const candidates = this.#telegramBindingsForRefs(refs);
    if (candidates.length === 1) return candidates[0]!;
    return syntheticTelegramBinding(this.#adapterId, refs);
  }

  #telegramBindingsForRefs(refs: {
    externalChannelId: string;
    externalThreadId?: string;
  }): ChannelBindingRecord[] {
    return this.#telegramBindings().filter((binding) => {
      if (binding.externalChannelId !== refs.externalChannelId) return false;
      if (refs.externalThreadId === undefined) return true;
      if (binding.externalThreadId === undefined) return true;
      return binding.externalThreadId === refs.externalThreadId;
    });
  }

  #activeTelegramBindings(): ChannelBindingRecord[] {
    return this.#telegramBindings().filter(
      (binding) => binding.status === "active",
    );
  }

  #telegramBindings(): ChannelBindingRecord[] {
    return this.#bindings().filter(
      (binding) =>
        binding.provider === "telegram" &&
        binding.adapterId === this.#adapterId,
    );
  }

  #countIngestResult(status: string): void {
    switch (status) {
      case "routed":
      case "accepted":
        this.#inbound.routed += 1;
        return;
      case "expired":
        this.#inbound.expired += 1;
        return;
      case "duplicate":
        this.#inbound.duplicate += 1;
        return;
      case "stale_cursor":
        this.#inbound.staleCursor += 1;
        return;
      case "ambiguous":
        this.#inbound.ambiguous += 1;
        return;
      case "telegram_bot_pair_rate_limited":
      case "rate_limited":
        this.#inbound.rateLimited += 1;
        return;
      case "telegram_bot_loop_depth_exceeded":
      case "telegram_bot_interaction_expired":
      case "telegram_bot_interaction_terminal":
      case "loop_terminated":
        this.#inbound.loopTerminated += 1;
        return;
      case "no_binding":
      case "inactive_binding":
        this.#inbound.unbound += 1;
        return;
      default:
        this.#inbound.failed += 1;
    }
  }

  async #advanceOffset(offset: number): Promise<void> {
    this.#lastUpdateId = offset - 1;
    this.#nextOffset = Math.max(this.#nextOffset ?? 0, offset);
    await this.#offsetStore.write(this.#nextOffset);
  }
}

export function telegramBindingFromChat(
  input: TelegramBindingInput,
): ChannelBindingRecord {
  return {
    bindingId: input.bindingId,
    adapterId: input.adapterId,
    provider: "telegram",
    agentId: input.agentId,
    sessionId: input.sessionId,
    profileId: input.profileId,
    externalChannelId: telegramChatId(input.chat),
    externalThreadId:
      input.threadId === undefined ? undefined : String(input.threadId),
    externalUserId: input.externalUserId,
    status: input.status ?? "active",
    createdAt: input.createdAt,
    updatedAt: input.updatedAt,
  };
}

function syntheticTelegramBinding(
  adapterId: AdapterId,
  refs: { externalChannelId: string; externalThreadId?: string },
): ChannelBindingRecord {
  return {
    bindingId: [
      "telegram",
      sanitizeTelegramBindingIdPart(refs.externalChannelId),
      sanitizeTelegramBindingIdPart(refs.externalThreadId ?? "main"),
    ].join(":"),
    adapterId,
    provider: "telegram",
    agentId: "telegram:unbound" as AgentId,
    profileId: "telegram:unbound" as ProfileId,
    externalChannelId: refs.externalChannelId,
    ...(refs.externalThreadId === undefined
      ? {}
      : { externalThreadId: refs.externalThreadId }),
    status: "active",
  };
}

function sanitizeTelegramBindingIdPart(value: string): string {
  const sanitized = value.replace(/[^A-Za-z0-9._:-]/g, "_");
  return sanitized.length === 0 ? "unknown" : sanitized;
}

export function normalizeTelegramUpdate(
  update: TelegramUpdate,
  context: TelegramNormalizeContext,
): NormalizedChannelInboundMessage | undefined {
  const message =
    update.message ??
    update.edited_message ??
    update.channel_post ??
    update.edited_channel_post;
  if (message === undefined) return undefined;

  const body = message.text ?? message.caption ?? "";
  const receivedAt = new Date(message.date * 1000).toISOString();
  const expiresAt = new Date(
    Date.parse(receivedAt) + context.ttlMs,
  ).toISOString();
  const externalThreadId =
    message.message_thread_id === undefined
      ? context.binding.externalThreadId
      : String(message.message_thread_id);
  const author = telegramAuthor(message);
  const updateShape = telegramUpdateShape(update);
  const messageMutation =
    updateShape === "edited_message" || updateShape === "edited_channel_post"
      ? "edited"
      : "original";
  const providerRefs = {
    provider: "telegram",
    externalChannelId: telegramChatId(message.chat),
    externalThreadId,
    externalMessageId: String(message.message_id),
    externalUserId: author.externalUserId,
  };

  return {
    kind: "channel_inbound_message.v1",
    adapterId: context.binding.adapterId,
    bindingId: context.binding.bindingId,
    runtime: {
      agentId: context.binding.agentId,
      sessionId: context.binding.sessionId,
      profileId: context.binding.profileId,
    },
    providerRefs,
    author,
    replyToExternalMessageId:
      message.reply_to_message?.message_id === undefined
        ? undefined
        : String(message.reply_to_message.message_id),
    messageMutation,
    body,
    summary: summarize(body),
    attachments: telegramAttachments(message),
    mentions: telegramMentions(
      body,
      message.entities ?? message.caption_entities ?? [],
    ),
    receivedAt,
    ttlMs: context.ttlMs,
    expiresAt,
    cursor: String(update.update_id),
    idempotencyKey: telegramIdempotencyKey(
      providerRefs,
      messageMutation,
      message.edit_date ?? update.update_id,
    ),
    visibility: context.visibility ?? "conversation",
    provenance: {
      sourceShape: updateShape,
      chatType: message.chat.type,
      senderKind: author.kind,
      senderUsername: author.username,
      senderIsBot: author.isBot,
      replyToExternalMessageId:
        message.reply_to_message?.message_id === undefined
          ? undefined
          : String(message.reply_to_message.message_id),
      replyToAuthorExternalUserId:
        message.reply_to_message?.from?.id === undefined
          ? undefined
          : String(message.reply_to_message.from.id),
      replyToAuthorIsBot: message.reply_to_message?.from?.is_bot === true,
      editDate:
        message.edit_date === undefined
          ? undefined
          : new Date(message.edit_date * 1_000).toISOString(),
      entityTypes: (message.entities ?? message.caption_entities ?? []).map(
        (entity) => entity.type,
      ),
    },
  };
}

export function toTelegramSendMessageRequest(
  message: NormalizedChannelOutboundMessage,
): TelegramSendMessageRequest {
  return {
    chat_id: parseTelegramChatId(message.providerRefs.externalChannelId),
    message_thread_id: parseOptionalTelegramNumber(
      message.providerRefs.externalThreadId,
    ),
    reply_parameters: telegramReplyParameters(message.replyToExternalMessageId),
    text: telegramOutboundBody(message),
    link_preview_options: { is_disabled: true },
  };
}

function telegramOutboundBody(
  message: NormalizedChannelOutboundMessage,
): string {
  const artifacts = (message.attachments ?? [])
    .filter(
      (attachment) =>
        attachment.state === "available" &&
        typeof attachment.contentUrl === "string" &&
        attachment.contentUrl.trim().length > 0,
    )
    .map(
      (attachment) =>
        `${attachment.filename ?? attachment.label ?? "artifact"}: ${attachment.contentUrl}`,
    );
  return artifacts.length === 0
    ? message.body
    : `${message.body}\n\nArtifacts:\n${artifacts.join("\n")}`;
}

function telegramChatId(chat: TelegramChat): string {
  return String(chat.id);
}

function telegramUpdateRefs(
  update: TelegramUpdate,
): { externalChannelId: string; externalThreadId?: string } | undefined {
  const message =
    update.message ??
    update.edited_message ??
    update.channel_post ??
    update.edited_channel_post;
  if (message === undefined) return undefined;
  return {
    externalChannelId: telegramChatId(message.chat),
    externalThreadId:
      message.message_thread_id === undefined
        ? undefined
        : String(message.message_thread_id),
  };
}

function parseTelegramChatId(value: string): number | string {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : value;
}

function parseOptionalTelegramNumber(
  value: string | undefined,
): number | undefined {
  if (value === undefined) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : undefined;
}

function telegramReplyParameters(
  value: string | undefined,
): { message_id: number } | undefined {
  const messageId = parseOptionalTelegramNumber(value);
  return messageId === undefined ? undefined : { message_id: messageId };
}

function telegramAuthor(message: TelegramMessage): {
  externalUserId: string;
  displayLabel?: string;
  username?: string;
  kind: "human" | "bot" | "sender_chat";
  isBot: boolean;
} {
  if (message.from) {
    return {
      externalUserId: String(message.from.id),
      displayLabel: [message.from.first_name, message.from.last_name]
        .filter(Boolean)
        .join(" "),
      username: message.from.username,
      kind: message.from.is_bot ? "bot" : "human",
      isBot: message.from.is_bot === true,
    };
  }
  if (message.sender_chat) {
    return {
      externalUserId: telegramChatId(message.sender_chat),
      displayLabel: message.sender_chat.title ?? message.sender_chat.username,
      username: message.sender_chat.username,
      kind: "sender_chat",
      isBot: false,
    };
  }
  return {
    externalUserId: telegramChatId(message.chat),
    displayLabel: message.chat.title ?? message.chat.username,
    username: message.chat.username,
    kind: "sender_chat",
    isBot: false,
  };
}

function telegramAttachments(message: TelegramMessage) {
  return telegramMediaCandidatesForMessage(message).map((candidate) => ({
    ref: `telegram:file:${candidate.fileId}`,
    mediaType: candidate.mediaType,
    label: candidate.filename,
    filename: candidate.filename,
    byteSize: candidate.fileSize,
    state: "pending" as const,
  }));
}

interface TelegramMediaCandidate {
  kind: "image" | "document";
  fileId: string;
  fileUniqueId?: string;
  filename: string;
  mediaType: string;
  fileSize?: number;
}

function telegramMediaCandidates(
  update: TelegramUpdate,
): TelegramMediaCandidate[] {
  const message =
    update.message ??
    update.edited_message ??
    update.channel_post ??
    update.edited_channel_post;
  return message === undefined
    ? []
    : telegramMediaCandidatesForMessage(message);
}

function telegramMediaCandidatesForMessage(
  message: TelegramMessage,
): TelegramMediaCandidate[] {
  const photos = [...(message.photo ?? [])].sort(
    (left, right) =>
      (right.width ?? 0) * (right.height ?? 0) -
        (left.width ?? 0) * (left.height ?? 0) ||
      (right.file_size ?? 0) - (left.file_size ?? 0),
  );
  const photo = photos[0];
  const candidates: TelegramMediaCandidate[] = [];
  if (photo !== undefined) {
    candidates.push({
      kind: "image",
      fileId: photo.file_id,
      fileUniqueId: photo.file_unique_id,
      filename: `telegram-photo-${message.message_id}.jpg`,
      mediaType: "image/jpeg",
      fileSize: photo.file_size,
    });
  }
  if (message.document !== undefined) {
    const mediaType = normalizeTelegramMediaType(
      message.document.mime_type ??
        mimeTypeFromFilename(message.document.file_name),
    );
    candidates.push({
      kind: mediaType.startsWith("image/") ? "image" : "document",
      fileId: message.document.file_id,
      fileUniqueId: message.document.file_unique_id,
      filename:
        message.document.file_name ??
        `telegram-document-${message.message_id}${extensionForMediaType(mediaType)}`,
      mediaType,
      fileSize: message.document.file_size,
    });
  }
  return candidates;
}

function validatedTelegramMediaType(
  candidate: TelegramMediaCandidate,
  responseContentType: string | undefined,
): string {
  const expected = normalizeTelegramMediaType(candidate.mediaType);
  const actual = normalizeTelegramMediaType(responseContentType);
  const selected =
    actual === "application/octet-stream" || actual === "" ? expected : actual;
  if (
    expected !== "application/octet-stream" &&
    actual !== "application/octet-stream" &&
    actual !== "" &&
    expected !== actual
  ) {
    throw new TelegramMediaError(
      "telegram_media_mime_mismatch",
      `Telegram declared ${expected} but downloaded ${actual}`,
      false,
    );
  }
  if (!isSupportedTelegramMediaType(selected, candidate.kind)) {
    throw new TelegramMediaError(
      "telegram_media_unsupported",
      `Telegram media type ${selected || "unknown"} is not supported`,
      false,
    );
  }
  return selected;
}

function normalizeTelegramMediaType(value: string | undefined): string {
  return (value ?? "application/octet-stream")
    .split(";", 1)[0]!
    .trim()
    .toLowerCase();
}

function isSupportedTelegramMediaType(
  value: string,
  kind: TelegramMediaCandidate["kind"],
): boolean {
  if (kind === "image") {
    return ["image/png", "image/jpeg", "image/gif", "image/webp"].includes(
      value,
    );
  }
  return (
    value.startsWith("text/") ||
    [
      "application/json",
      "application/pdf",
      "application/sql",
      "application/xml",
      "application/yaml",
      "application/x-yaml",
      "application/zip",
      "application/octet-stream",
    ].includes(value)
  );
}

function mimeTypeFromFilename(filename: string | undefined): string {
  const extension = filename?.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    case "json":
      return "application/json";
    case "pdf":
      return "application/pdf";
    case "md":
      return "text/markdown";
    case "txt":
    case "log":
      return "text/plain";
    default:
      return "application/octet-stream";
  }
}

function extensionForMediaType(mediaType: string): string {
  switch (mediaType) {
    case "image/png":
      return ".png";
    case "image/jpeg":
      return ".jpg";
    case "image/gif":
      return ".gif";
    case "image/webp":
      return ".webp";
    case "application/json":
      return ".json";
    case "application/pdf":
      return ".pdf";
    case "text/plain":
      return ".txt";
    default:
      return ".bin";
  }
}

function telegramMediaFailureState(
  reasonCode: string,
): "unsupported" | "oversized" | "expired" | "failed" {
  if (
    reasonCode === "telegram_media_unsupported" ||
    reasonCode.endsWith("mime_mismatch")
  ) {
    return "unsupported";
  }
  if (reasonCode === "telegram_media_oversized") return "oversized";
  if (reasonCode === "telegram_media_expired") return "expired";
  return "failed";
}

function telegramMentions(
  body: string,
  entities: readonly TelegramMessageEntity[],
): string[] {
  const entityMentions = entities.flatMap((entity) => {
    if (entity.type === "mention") {
      const value = body.slice(entity.offset, entity.offset + entity.length);
      return value.startsWith("@")
        ? [normalizeTelegramUsername(value.slice(1))]
        : [];
    }
    if (entity.type === "text_mention" && entity.user !== undefined) {
      return [
        ...(entity.user.username === undefined
          ? []
          : [normalizeTelegramUsername(entity.user.username)]),
        String(entity.user.id),
      ];
    }
    return [];
  });
  const parsed = [...body.matchAll(/@([A-Za-z0-9_]{3,32})/g)].map((match) =>
    normalizeTelegramUsername(match[1]),
  );
  return dedupeTelegramStrings([...entityMentions, ...parsed]);
}

function telegramIdempotencyKey(
  providerRefs: {
    externalChannelId: string;
    externalThreadId?: string;
    externalMessageId?: string;
  },
  mutation: "original" | "edited",
  mutationId: number,
): string {
  const base = [
    "telegram",
    providerRefs.externalChannelId,
    providerRefs.externalThreadId ?? "main",
    providerRefs.externalMessageId ?? "unknown",
  ].join(":");
  return mutation === "original" ? base : `${base}:edited:${mutationId}`;
}

function telegramUpdateShape(update: TelegramUpdate): string {
  if (update.message) return "message";
  if (update.edited_message) return "edited_message";
  if (update.channel_post) return "channel_post";
  if (update.edited_channel_post) return "edited_channel_post";
  return "unknown";
}

function summarize(body: string): string {
  const trimmed = body.trim();
  return trimmed.length <= 120 ? trimmed : `${trimmed.slice(0, 117)}...`;
}

function safeTelegramOffset(value: unknown): number | undefined {
  if (typeof value !== "number") return undefined;
  if (!Number.isSafeInteger(value) || value < 0) return undefined;
  return value;
}

export function splitTelegramText(body: string, maxChars = 4_096): string[] {
  if (!Number.isSafeInteger(maxChars) || maxChars < 1) {
    throw new Error("Telegram message character limit must be positive");
  }
  const characters = Array.from(body);
  if (characters.length <= maxChars) return [body];
  const chunks: string[] = [];
  for (let index = 0; index < characters.length; index += maxChars) {
    chunks.push(characters.slice(index, index + maxChars).join(""));
  }
  return chunks;
}

function telegramMessageAddressesBot(
  message: NormalizedChannelInboundMessage,
  bot: { botUserId?: string; botUsername?: string },
): boolean {
  const mentions = message.mentions.map(normalizeTelegramUsername);
  if (
    bot.botUsername !== undefined &&
    mentions.includes(normalizeTelegramUsername(bot.botUsername))
  ) {
    return true;
  }
  if (bot.botUserId !== undefined && mentions.includes(bot.botUserId)) {
    return true;
  }
  const provenance = message.provenance;
  return (
    provenance.replyToAuthorIsBot === true &&
    bot.botUserId !== undefined &&
    provenance.replyToAuthorExternalUserId === bot.botUserId
  );
}

function normalizeTelegramUsername(value: string | undefined): string {
  return (value ?? "").replace(/^@/, "").trim().toLowerCase();
}

function dedupeTelegramStrings(values: readonly string[]): string[] {
  const result: string[] = [];
  for (const value of values) {
    if (value && !result.includes(value)) result.push(value);
  }
  return result;
}

function isTerminalTelegramIngestStatus(status: string): boolean {
  return [
    "routed",
    "accepted",
    "expired",
    "duplicate",
    "stale_cursor",
    "ambiguous",
    "no_binding",
    "inactive_binding",
    "denied",
    "rate_limited",
    "loop_terminated",
    "telegram_bot_pair_rate_limited",
    "telegram_bot_loop_depth_exceeded",
    "telegram_bot_interaction_expired",
    "telegram_bot_interaction_terminal",
  ].includes(status);
}

function isRetryableTelegramError(error: unknown): boolean {
  if (error instanceof TelegramBotApiError) return error.retryable;
  if (error instanceof TelegramMediaError) return error.retryable;
  if (error instanceof Error && error.name === "AbortError") return true;
  return error instanceof TypeError;
}

function terminalTelegramMediaError(
  error: unknown,
): TelegramMediaError | undefined {
  if (error instanceof TelegramMediaError) {
    return error.retryable ? undefined : error;
  }
  if (error instanceof TelegramBotApiError && !error.retryable) {
    return new TelegramMediaError(
      error.status === 400 || error.status === 404
        ? "telegram_media_expired"
        : "telegram_media_download_rejected",
      error.message,
      false,
    );
  }
  return undefined;
}

function telegramRetryDelayMs(error: unknown, attempt: number): number {
  if (
    error instanceof TelegramBotApiError &&
    error.retryAfterSeconds !== undefined
  ) {
    return Math.min(60_000, Math.max(0, error.retryAfterSeconds * 1_000));
  }
  return Math.min(30_000, 250 * 2 ** Math.max(0, attempt - 1));
}

function waitForTelegramRetry(delayMs: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

function telegramSentMessageId(response: unknown): string | undefined {
  if (
    typeof response !== "object" ||
    response === null ||
    !("message_id" in response)
  ) {
    return undefined;
  }
  const messageId = response.message_id;
  return typeof messageId === "number" || typeof messageId === "string"
    ? String(messageId)
    : undefined;
}

function telegramErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
