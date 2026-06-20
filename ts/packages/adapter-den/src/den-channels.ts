import type {
  AdapterId,
  AgentId,
  AgentInstanceId,
  ChannelDeliveryPolicy,
  ChannelProviderRefs,
  ChannelSeverity,
  ChannelVisibility,
  ExternalEvent,
  NormalizedChannelActivityProjection,
  NormalizedChannelInboundMessage,
  NormalizedChannelOutboundMessage,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";

export interface DenChannelsBindingContext {
  adapterId: AdapterId;
  bindingId: string;
  agentId?: AgentId;
  instanceId?: AgentInstanceId;
  sessionId?: SessionId;
  profileId?: ProfileId;
  ttlMs: number;
  visibility?: ChannelVisibility;
  receivedAt?: string;
}

export interface LegacyDenChannelsMessageEvent {
  type: "message" | "den_channel_message";
  channelId: string;
  threadId?: string;
  messageId?: string;
  userId: string;
  userLabel?: string;
  text: string;
  receivedAt?: string;
  cursor?: string;
  mentions?: string[];
  attachments?: DenChannelsAttachmentFixture[];
}

export interface CurrentDenChannelsMessageEvent {
  kind: "channel.message.created" | "message.created";
  channel: { id: string };
  thread?: { id: string };
  message: { id: string; text?: string; body?: string; createdAt?: string };
  author: { id: string; displayName?: string };
  cursor?: string;
  mentions?: Array<string | { id: string }>;
  attachments?: DenChannelsAttachmentFixture[];
}

export interface DenChannelsAttachmentFixture {
  id?: string;
  ref?: string;
  url?: string;
  mediaType?: string;
  label?: string;
}

export type DenChannelsInboundFixture =
  | LegacyDenChannelsMessageEvent
  | CurrentDenChannelsMessageEvent;

export interface DenChannelsPostMessageRequest {
  channelId: string;
  threadId?: string;
  body: string;
  replyToMessageId?: string;
  idempotencyKey: string;
  deliveryPolicy: ChannelDeliveryPolicy;
  metadata: {
    bindingId: string;
    adapterId: AdapterId;
    correlationId?: string;
    visibility: ChannelVisibility;
    workRef?: string;
    resultRef?: string;
  };
}

export interface DenChannelsActivityRequest {
  channelId: string;
  threadId?: string;
  summary: string;
  severity: ChannelSeverity;
  eventType: string;
  createdAt: string;
  metadata: {
    bindingId: string;
    adapterId: AdapterId;
    workRef?: string;
    resultRef?: string;
  };
}

interface ExtractedDenMessage {
  channelId: string;
  threadId?: string;
  messageId?: string;
  userId: string;
  userLabel?: string;
  text: string;
  receivedAt?: string;
  cursor?: string;
  mentions: string[];
  attachments: DenChannelsAttachmentFixture[];
}

export function normalizeDenChannelsInboundEvent(
  event: DenChannelsInboundFixture,
  context: DenChannelsBindingContext,
): NormalizedChannelInboundMessage {
  const extracted = extractDenChannelsMessage(event);
  const receivedAt = extracted.receivedAt ?? context.receivedAt ?? nowIso();
  const expiresAt = addMsIso(receivedAt, context.ttlMs);
  const providerRefs: ChannelProviderRefs = {
    provider: "den_channels",
    externalChannelId: extracted.channelId,
    externalThreadId: extracted.threadId,
    externalMessageId: extracted.messageId,
    externalUserId: extracted.userId,
  };

  return {
    kind: "channel_inbound_message.v1",
    adapterId: context.adapterId,
    bindingId: context.bindingId,
    runtime: {
      agentId: context.agentId,
      instanceId: context.instanceId,
      sessionId: context.sessionId,
      profileId: context.profileId,
    },
    providerRefs,
    author: {
      externalUserId: extracted.userId,
      displayLabel: extracted.userLabel,
    },
    body: extracted.text,
    summary: summarizeText(extracted.text),
    attachments: extracted.attachments.map(normalizeAttachment),
    mentions: extracted.mentions,
    receivedAt,
    ttlMs: context.ttlMs,
    expiresAt,
    cursor: extracted.cursor,
    idempotencyKey: denChannelsIdempotencyKey(providerRefs),
    visibility: context.visibility ?? "conversation",
    provenance: {
      sourceShape: isCurrentDenChannelsMessageEvent(event)
        ? "current"
        : "legacy",
    },
  };
}

export function isExpiredChannelInboundMessage(
  message: NormalizedChannelInboundMessage,
  now: string = nowIso(),
): boolean {
  return Date.parse(message.expiresAt) <= Date.parse(now);
}

export function denChannelsInboundToExternalEvent(
  message: NormalizedChannelInboundMessage,
  now?: string,
): ExternalEvent {
  if (isExpiredChannelInboundMessage(message, now)) {
    return {
      adapterId: message.adapterId,
      source: `den_channels:${message.bindingId}`,
      payload: {
        type: "raw_json",
        json: JSON.stringify({
          expired: true,
          kind: message.kind,
          bindingId: message.bindingId,
          idempotencyKey: message.idempotencyKey,
          expiresAt: message.expiresAt,
        }),
      },
    };
  }

  return {
    adapterId: message.adapterId,
    source: `den_channels:${message.bindingId}`,
    payload: {
      type: "human_message",
      from: message.author.externalUserId,
      text: message.body,
    },
  };
}

export function denChannelsInboundToChannelExternalEvent(
  message: NormalizedChannelInboundMessage,
  correlationId: string,
): ExternalEvent {
  return {
    adapterId: message.adapterId,
    source: `${message.providerRefs.provider}:${message.bindingId}`,
    payload: {
      type: "channel_message",
      bindingId: message.bindingId,
      correlationId,
      idempotencyKey: message.idempotencyKey,
      provider: message.providerRefs.provider,
      externalChannelId: message.providerRefs.externalChannelId,
      externalThreadId: message.providerRefs.externalThreadId,
      externalMessageId: message.providerRefs.externalMessageId,
      from: message.author.externalUserId,
      text: message.body,
      receivedAt: message.receivedAt,
      expiresAt: message.expiresAt,
    },
  };
}

export function toDenChannelsPostMessageRequest(
  message: NormalizedChannelOutboundMessage,
): DenChannelsPostMessageRequest {
  return {
    channelId: message.providerRefs.externalChannelId,
    threadId: message.providerRefs.externalThreadId,
    body: message.body,
    replyToMessageId: message.replyToExternalMessageId,
    idempotencyKey: message.idempotencyKey,
    deliveryPolicy: message.deliveryPolicy,
    metadata: {
      bindingId: message.bindingId,
      adapterId: message.adapterId,
      correlationId: message.correlationId,
      visibility: message.visibility,
      workRef: message.workRef,
      resultRef: message.resultRef,
    },
  };
}

export function toDenChannelsActivityRequest(
  projection: NormalizedChannelActivityProjection,
): DenChannelsActivityRequest {
  return {
    channelId: projection.providerRefs.externalChannelId,
    threadId: projection.providerRefs.externalThreadId,
    summary: projection.summary,
    severity: projection.severity,
    eventType: projection.eventType,
    createdAt: projection.createdAt,
    metadata: {
      bindingId: projection.bindingId,
      adapterId: projection.adapterId,
      workRef: projection.workRef,
      resultRef: projection.resultRef,
    },
  };
}

function extractDenChannelsMessage(
  event: DenChannelsInboundFixture,
): ExtractedDenMessage {
  if (isCurrentDenChannelsMessageEvent(event)) {
    return {
      channelId: event.channel.id,
      threadId: event.thread?.id,
      messageId: event.message.id,
      userId: event.author.id,
      userLabel: event.author.displayName,
      text: event.message.text ?? event.message.body ?? "",
      receivedAt: event.message.createdAt,
      cursor: event.cursor,
      mentions: normalizeMentions(event.mentions),
      attachments: event.attachments ?? [],
    };
  }

  return {
    channelId: event.channelId,
    threadId: event.threadId,
    messageId: event.messageId,
    userId: event.userId,
    userLabel: event.userLabel,
    text: event.text,
    receivedAt: event.receivedAt,
    cursor: event.cursor,
    mentions: event.mentions ?? [],
    attachments: event.attachments ?? [],
  };
}

function isCurrentDenChannelsMessageEvent(
  event: DenChannelsInboundFixture,
): event is CurrentDenChannelsMessageEvent {
  return "kind" in event;
}

function normalizeAttachment(attachment: DenChannelsAttachmentFixture) {
  return {
    ref:
      attachment.ref ??
      attachment.url ??
      attachment.id ??
      "den-channels-attachment:unknown",
    mediaType: attachment.mediaType,
    label: attachment.label,
  };
}

function normalizeMentions(
  mentions: CurrentDenChannelsMessageEvent["mentions"],
): string[] {
  return (
    mentions?.map((mention) =>
      typeof mention === "string" ? mention : mention.id,
    ) ?? []
  );
}

function denChannelsIdempotencyKey(refs: ChannelProviderRefs): string {
  return [
    refs.provider,
    refs.externalChannelId,
    refs.externalThreadId ?? "_",
    refs.externalMessageId ?? refs.externalUserId ?? "_",
  ].join(":");
}

function summarizeText(text: string): string {
  return text.length <= 120 ? text : `${text.slice(0, 117)}...`;
}

function addMsIso(iso: string, ttlMs: number): string {
  const parsed = Date.parse(iso);
  const base = Number.isNaN(parsed) ? Date.now() : parsed;
  return new Date(base + ttlMs).toISOString();
}

function nowIso(): string {
  return new Date().toISOString();
}
