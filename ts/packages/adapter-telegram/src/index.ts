import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import type {
  AdapterId,
  AgentId,
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

export interface TelegramChat {
  id: number | string;
  type: "private" | "group" | "supergroup" | "channel" | string;
  title?: string;
  username?: string;
}

export interface TelegramPhotoSize {
  file_id: string;
  file_unique_id?: string;
  width?: number;
  height?: number;
}

export interface TelegramDocument {
  file_id: string;
  file_unique_id?: string;
  file_name?: string;
  mime_type?: string;
}

export interface TelegramMessage {
  message_id: number;
  message_thread_id?: number;
  date: number;
  chat: TelegramChat;
  from?: TelegramUser;
  sender_chat?: TelegramChat;
  text?: string;
  caption?: string;
  photo?: TelegramPhotoSize[];
  document?: TelegramDocument;
  reply_to_message?: Pick<TelegramMessage, "message_id">;
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
  reply_to_message_id?: number;
  parse_mode?: "MarkdownV2" | "HTML";
  disable_web_page_preview?: boolean;
}

export interface TelegramBotApiClient {
  getUpdates?(
    request?: TelegramGetUpdatesRequest,
  ): Promise<TelegramUpdate[]> | TelegramUpdate[];
  sendMessage(request: TelegramSendMessageRequest): Promise<unknown> | unknown;
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

  return {
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
  };
}

export interface TelegramApiResponse<T> {
  ok: boolean;
  result?: T;
  description?: string;
  error_code?: number;
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
    const parsed = text.trim()
      ? (JSON.parse(text) as TelegramApiResponse<T>)
      : ({
          ok: false,
          description: "empty response",
        } as TelegramApiResponse<T>);
    if (!response.ok || !parsed.ok) {
      const detail = parsed.description ?? response.statusText;
      throw new Error(
        `Telegram Bot API ${input.resultName} failed: ${response.status}${parsed.error_code ? `/${parsed.error_code}` : ""} ${detail}`,
      );
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

export interface TelegramConnectorIngestResult {
  status: string;
  reason?: string;
}

export interface TelegramConnectorOptions {
  adapterId: AdapterId;
  bot: TelegramBotApiClient;
  offsetStore: TelegramUpdateOffsetStore;
  bindings: () => readonly ChannelBindingRecord[];
  ingest(
    message: NormalizedChannelInboundMessage,
  ): Promise<TelegramConnectorIngestResult> | TelegramConnectorIngestResult;
  ttlMs: number;
  visibility?: ChannelVisibility;
  pollIntervalMs?: number;
  pollTimeoutSeconds?: number;
  updateLimit?: number;
  now?: () => string;
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
  lastUpdateId?: number;
  nextOffset?: number;
  lastError?: string;
  inbound: {
    routed: number;
    unbound: number;
    ambiguous: number;
    expired: number;
    duplicate: number;
    staleCursor: number;
    failed: number;
  };
  outbound: {
    sent: number;
    failed: number;
    lastError?: string;
  };
}

export class TelegramChannelConnector {
  readonly #adapterId: AdapterId;
  readonly #bot: TelegramBotApiClient;
  readonly #adapter: TelegramChannelAdapter;
  readonly #offsetStore: TelegramUpdateOffsetStore;
  readonly #bindings: () => readonly ChannelBindingRecord[];
  readonly #ingest: (
    message: NormalizedChannelInboundMessage,
  ) => Promise<TelegramConnectorIngestResult> | TelegramConnectorIngestResult;
  readonly #ttlMs: number;
  readonly #visibility: ChannelVisibility | undefined;
  readonly #pollIntervalMs: number;
  readonly #pollTimeoutSeconds: number;
  readonly #updateLimit: number;
  readonly #now: () => string;
  readonly #setTimer: typeof setTimeout;
  readonly #clearTimer: typeof clearTimeout;

  #running = false;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #polling = false;
  #lastPollAt: string | undefined;
  #lastUpdateId: number | undefined;
  #nextOffset: number | undefined;
  #lastError: string | undefined;
  #pollCount = 0;
  #inbound = {
    routed: 0,
    unbound: 0,
    ambiguous: 0,
    expired: 0,
    duplicate: 0,
    staleCursor: 0,
    failed: 0,
  };
  #outbound = {
    sent: 0,
    failed: 0,
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
    this.#bindings = options.bindings;
    this.#ingest = options.ingest;
    this.#ttlMs = options.ttlMs;
    this.#visibility = options.visibility;
    this.#pollIntervalMs = options.pollIntervalMs ?? 2_000;
    this.#pollTimeoutSeconds = options.pollTimeoutSeconds ?? 20;
    this.#updateLimit = options.updateLimit ?? 50;
    this.#now = options.now ?? (() => new Date().toISOString());
    this.#setTimer = options.setTimer ?? setTimeout;
    this.#clearTimer = options.clearTimer ?? clearTimeout;
  }

  async start(): Promise<void> {
    if (this.#running) return;
    this.#running = true;
    this.#nextOffset = await this.#offsetStore.read();
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
      for (const update of [...updates].sort(
        (left, right) => left.update_id - right.update_id,
      )) {
        await this.#handleUpdate(update);
      }
      this.#lastError = undefined;
    } catch (error) {
      this.#lastError = telegramErrorMessage(error);
    } finally {
      this.#polling = false;
    }
  }

  async sendOutbound(message: NormalizedChannelOutboundMessage): Promise<void> {
    try {
      await this.#adapter.sendOutbound(message);
      this.#outbound.sent += 1;
      this.#outbound.lastError = undefined;
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
      lastUpdateId: this.#lastUpdateId,
      nextOffset: this.#nextOffset,
      lastError: this.#lastError,
      inbound: { ...this.#inbound },
      outbound: { ...this.#outbound },
    };
  }

  #schedule(delayMs: number): void {
    if (!this.#running) return;
    this.#timer = this.#setTimer(() => {
      void this.pollOnce().finally(() => this.#schedule(this.#pollIntervalMs));
    }, delayMs);
  }

  async #handleUpdate(update: TelegramUpdate): Promise<void> {
    const updateOffset = update.update_id + 1;
    try {
      const binding = this.#resolveBinding(update);
      if (binding.status === "unbound") {
        this.#inbound.unbound += 1;
        return;
      }
      if (binding.status === "ambiguous") {
        this.#inbound.ambiguous += 1;
        return;
      }
      const message = this.#adapter.normalizeUpdate(update, {
        binding: binding.binding,
        ttlMs: this.#ttlMs,
        visibility: this.#visibility,
      });
      if (message === undefined) {
        this.#inbound.unbound += 1;
        return;
      }
      const result = await this.#ingest(message);
      this.#countIngestResult(result.status);
    } catch (error) {
      this.#inbound.failed += 1;
      this.#lastError = telegramErrorMessage(error);
    } finally {
      await this.#advanceOffset(updateOffset);
    }
  }

  #resolveBinding(
    update: TelegramUpdate,
  ):
    | { status: "routed"; binding: ChannelBindingRecord }
    | { status: "unbound" }
    | { status: "ambiguous" } {
    const refs = telegramUpdateRefs(update);
    if (refs === undefined) return { status: "unbound" };
    const matches = this.#activeTelegramBindings().filter((binding) => {
      if (binding.externalChannelId !== refs.externalChannelId) return false;
      if (binding.externalThreadId === undefined) {
        return refs.externalThreadId === undefined;
      }
      return binding.externalThreadId === refs.externalThreadId;
    });
    if (matches.length === 0) return { status: "unbound" };
    if (matches.length > 1) return { status: "ambiguous" };
    return { status: "routed", binding: matches[0]! };
  }

  #activeTelegramBindings(): ChannelBindingRecord[] {
    return this.#bindings().filter(
      (binding) =>
        binding.status === "active" &&
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
    body,
    summary: summarize(body),
    attachments: telegramAttachments(message),
    mentions: telegramMentions(body),
    receivedAt,
    ttlMs: context.ttlMs,
    expiresAt,
    cursor: String(update.update_id),
    idempotencyKey: telegramIdempotencyKey(providerRefs),
    visibility: context.visibility ?? "conversation",
    provenance: {
      sourceShape: telegramUpdateShape(update),
      chatType: message.chat.type,
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
    reply_to_message_id: parseOptionalTelegramNumber(
      message.replyToExternalMessageId,
    ),
    text: message.body,
    disable_web_page_preview: true,
  };
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

function telegramAuthor(message: TelegramMessage): {
  externalUserId: string;
  displayLabel?: string;
} {
  if (message.from) {
    return {
      externalUserId: String(message.from.id),
      displayLabel: [message.from.first_name, message.from.last_name]
        .filter(Boolean)
        .join(" "),
    };
  }
  if (message.sender_chat) {
    return {
      externalUserId: telegramChatId(message.sender_chat),
      displayLabel: message.sender_chat.title ?? message.sender_chat.username,
    };
  }
  return {
    externalUserId: telegramChatId(message.chat),
    displayLabel: message.chat.title ?? message.chat.username,
  };
}

function telegramAttachments(message: TelegramMessage) {
  const photo = (message.photo ?? []).map((item) => ({
    ref: `telegram:file:${item.file_id}`,
    mediaType: "image/*",
    label: item.file_unique_id,
  }));
  const document = message.document
    ? [
        {
          ref: `telegram:file:${message.document.file_id}`,
          mediaType: message.document.mime_type,
          label: message.document.file_name,
        },
      ]
    : [];
  return [...photo, ...document];
}

function telegramMentions(body: string): string[] {
  return [...body.matchAll(/@([A-Za-z0-9_]{3,32})/g)].map((match) => match[1]!);
}

function telegramIdempotencyKey(providerRefs: {
  externalChannelId: string;
  externalThreadId?: string;
  externalMessageId?: string;
}): string {
  return [
    "telegram",
    providerRefs.externalChannelId,
    providerRefs.externalThreadId ?? "main",
    providerRefs.externalMessageId ?? "unknown",
  ].join(":");
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

function telegramErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
