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

export interface TelegramSendMessageRequest {
  chat_id: number | string;
  text: string;
  message_thread_id?: number;
  reply_to_message_id?: number;
  parse_mode?: "MarkdownV2" | "HTML";
  disable_web_page_preview?: boolean;
}

export interface TelegramBotApiClient {
  getUpdates?(): Promise<TelegramUpdate[]> | TelegramUpdate[];
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
