import type {
  ChannelReadbackVisibilityFilter,
  NormalizedChannelInboundMessage,
  NormalizedChannelReadbackMessageSummary,
  NormalizedChannelReadbackRequest,
  NormalizedChannelReadbackResponse,
} from "@rusty-crew/contracts";
import { isExpiredChannelInboundMessage } from "./den-channels.js";

export interface ChannelReadbackService {
  readback(
    request: NormalizedChannelReadbackRequest,
  ):
    | Promise<NormalizedChannelReadbackResponse>
    | NormalizedChannelReadbackResponse;
}

export interface ChannelReadbackRecorder {
  record(message: NormalizedChannelInboundMessage): void;
}

export interface InMemoryChannelReadbackStoreOptions {
  maxStoredMessages?: number;
  defaultLimit?: number;
  maxLimit?: number;
  defaultMaxBodyChars?: number;
  maxBodyChars?: number;
  now?: () => string;
}

export class InMemoryChannelReadbackStore
  implements ChannelReadbackService, ChannelReadbackRecorder
{
  readonly #messages: NormalizedChannelInboundMessage[] = [];
  readonly #maxStoredMessages: number;
  readonly #defaultLimit: number;
  readonly #maxLimit: number;
  readonly #defaultMaxBodyChars: number;
  readonly #maxBodyChars: number;
  readonly #now: () => string;

  constructor(options: InMemoryChannelReadbackStoreOptions = {}) {
    this.#maxStoredMessages = options.maxStoredMessages ?? 1_000;
    this.#defaultLimit = options.defaultLimit ?? 10;
    this.#maxLimit = options.maxLimit ?? 50;
    this.#defaultMaxBodyChars = options.defaultMaxBodyChars ?? 600;
    this.#maxBodyChars = options.maxBodyChars ?? 2_000;
    this.#now = options.now ?? (() => new Date().toISOString());
  }

  record(message: NormalizedChannelInboundMessage): void {
    this.#messages.push(message);
    if (this.#messages.length > this.#maxStoredMessages) {
      this.#messages.splice(0, this.#messages.length - this.#maxStoredMessages);
    }
  }

  readback(
    request: NormalizedChannelReadbackRequest,
  ): NormalizedChannelReadbackResponse {
    const errors = validateRequest(request);
    if (errors.length > 0) {
      return response(request, [], {
        errors,
        degradedReason: "channel_readback_request_denied",
        truncated: false,
      });
    }

    const limit = clamp(request.limit ?? this.#defaultLimit, 1, this.#maxLimit);
    const maxBodyChars = clamp(
      request.maxBodyChars ?? this.#defaultMaxBodyChars,
      1,
      this.#maxBodyChars,
    );
    const now = this.#now();
    const matching = this.#messages
      .filter((message) => matchesRequest(message, request, now))
      .sort(compareMessages);
    const bounded = applyBeforeBoundary(matching, request);
    const page = bounded.slice(Math.max(0, bounded.length - limit));

    return response(
      request,
      page.map((message) => summarizeMessage(message, maxBodyChars)),
      {
        truncated: bounded.length > page.length,
      },
    );
  }
}

export function createInMemoryChannelReadbackStore(
  options?: InMemoryChannelReadbackStoreOptions,
): InMemoryChannelReadbackStore {
  return new InMemoryChannelReadbackStore(options);
}

function validateRequest(request: NormalizedChannelReadbackRequest): string[] {
  const errors: string[] = [];
  if (!request.bindingId.trim()) {
    errors.push("binding_id_required");
  }
  if (!request.requester.agentId && !request.requester.sessionId) {
    errors.push("requester_runtime_identity_required");
  }
  return errors;
}

function matchesRequest(
  message: NormalizedChannelInboundMessage,
  request: NormalizedChannelReadbackRequest,
  now: string,
): boolean {
  if (message.bindingId !== request.bindingId) return false;
  if (request.adapterId && message.adapterId !== request.adapterId) {
    return false;
  }
  if (!matchesProviderRefs(message, request)) return false;
  if (!matchesVisibility(message.visibility, request.visibility)) return false;
  if (!matchesRequester(message, request)) return false;
  if (!request.includeExpired && isExpiredChannelInboundMessage(message, now)) {
    return false;
  }
  return true;
}

function matchesProviderRefs(
  message: NormalizedChannelInboundMessage,
  request: NormalizedChannelReadbackRequest,
): boolean {
  const refs = request.providerRefs;
  if (!refs) return true;
  if (refs.provider && message.providerRefs.provider !== refs.provider) {
    return false;
  }
  if (
    refs.externalChannelId &&
    message.providerRefs.externalChannelId !== refs.externalChannelId
  ) {
    return false;
  }
  if (
    refs.externalThreadId &&
    message.providerRefs.externalThreadId !== refs.externalThreadId
  ) {
    return false;
  }
  if (
    refs.externalUserId &&
    message.providerRefs.externalUserId !== refs.externalUserId
  ) {
    return false;
  }
  return true;
}

function matchesVisibility(
  messageVisibility: NormalizedChannelInboundMessage["visibility"],
  filter: ChannelReadbackVisibilityFilter | undefined,
): boolean {
  return (
    filter === undefined || filter === "any" || messageVisibility === filter
  );
}

function matchesRequester(
  message: NormalizedChannelInboundMessage,
  request: NormalizedChannelReadbackRequest,
): boolean {
  const requester = request.requester;
  return (
    matchesOptional(message.runtime.agentId, requester.agentId) &&
    matchesOptional(message.runtime.instanceId, requester.instanceId) &&
    matchesOptional(message.runtime.sessionId, requester.sessionId) &&
    matchesOptional(message.runtime.profileId, requester.profileId)
  );
}

function matchesOptional<T>(
  messageValue: T | undefined,
  requesterValue: T | undefined,
): boolean {
  return requesterValue === undefined || messageValue === requesterValue;
}

function applyBeforeBoundary(
  messages: readonly NormalizedChannelInboundMessage[],
  request: NormalizedChannelReadbackRequest,
): NormalizedChannelInboundMessage[] {
  if (!request.beforeExternalMessageId && !request.beforeCursor) {
    return [...messages];
  }
  return messages.filter((message) => {
    if (
      request.beforeExternalMessageId &&
      message.providerRefs.externalMessageId &&
      message.providerRefs.externalMessageId >= request.beforeExternalMessageId
    ) {
      return false;
    }
    if (
      request.beforeCursor &&
      message.cursor &&
      compareCursor(message.cursor, request.beforeCursor) >= 0
    ) {
      return false;
    }
    return true;
  });
}

function summarizeMessage(
  message: NormalizedChannelInboundMessage,
  maxBodyChars: number,
): NormalizedChannelReadbackMessageSummary {
  return {
    providerRefs: message.providerRefs,
    author: message.author,
    bodySnippet: message.body.slice(0, maxBodyChars),
    summary: message.summary,
    receivedAt: message.receivedAt,
    expiresAt: message.expiresAt,
    cursor: message.cursor,
    visibility: message.visibility,
    attachmentCount: message.attachments.length,
    truncated: message.body.length > maxBodyChars,
  };
}

function response(
  request: NormalizedChannelReadbackRequest,
  messages: NormalizedChannelReadbackMessageSummary[],
  options: {
    truncated: boolean;
    errors?: string[];
    degradedReason?: string;
  },
): NormalizedChannelReadbackResponse {
  return {
    kind: "channel_readback_response.v1",
    adapterId: request.adapterId,
    bindingId: request.bindingId,
    providerRefs: request.providerRefs,
    messages,
    cursorBoundaries: {
      oldestCursor: messages[0]?.cursor,
      newestCursor: messages.at(-1)?.cursor,
      beforeCursor: request.beforeCursor,
      beforeExternalMessageId: request.beforeExternalMessageId,
    },
    truncated: options.truncated,
    provenance: {
      service: "adapter_den.in_memory_channel_readback",
      reasonCode: request.reasonCode,
      bounded: true,
    },
    errors: options.errors,
    degradedReason: options.degradedReason,
  };
}

function compareMessages(
  left: NormalizedChannelInboundMessage,
  right: NormalizedChannelInboundMessage,
): number {
  const received = left.receivedAt.localeCompare(right.receivedAt);
  if (received !== 0) return received;
  return (left.cursor ?? "").localeCompare(right.cursor ?? "");
}

function compareCursor(left: string, right: string): number {
  const leftNumber = Number(left);
  const rightNumber = Number(right);
  if (Number.isSafeInteger(leftNumber) && Number.isSafeInteger(rightNumber)) {
    return leftNumber - rightNumber;
  }
  return left.localeCompare(right);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
