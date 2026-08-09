import type { IncomingMessage, ServerResponse } from "node:http";
import type { SessionId, SessionState } from "@rusty-crew/contracts";
import {
  handleRustyViewChatRequest,
  type ChatEvent,
  type RustyViewChatContext,
} from "./rusty-view-chat-api.js";
import type { ServiceRouteResult } from "./service-route-results.js";
import {
  failure,
  isRawServiceRouteResult,
  successRoute,
} from "./service-route-results.js";
import type { ToolMediaAttachmentContent } from "./tool-media-attachments.js";
import {
  MAX_CHAT_IMAGE_BYTES,
  ToolMediaAttachmentError,
} from "./tool-media-attachments.js";

export interface ChatStreamSubscriber {
  write(event: ChatEvent): void;
}

export interface RustyViewChatStreamRouteContext {
  listSessions(): Promise<SessionState[]>;
  streamReplayEvents(
    session: SessionState,
    cursor: string | undefined,
    url: URL,
  ): Promise<readonly ChatEvent[]>;
  subscribersForSession(sessionId: SessionId): Set<ChatStreamSubscriber>;
  deleteSubscribersForSession(sessionId: SessionId): void;
  timers: Set<NodeJS.Timeout>;
  corsHeaders(request: IncomingMessage): Record<string, string>;
  readAttachmentContent(
    sessionId: string,
    attachmentId: string,
  ): Promise<ToolMediaAttachmentContent>;
  uploadAttachmentContent(input: {
    sessionId: string;
    idempotencyKey: string;
    filename: string;
    mimeType: string;
    bytes: Buffer;
  }): Promise<ToolMediaAttachmentContent>;
}

export interface RustyViewChatRouteContext {
  stream: RustyViewChatStreamRouteContext;
  chat: RustyViewChatContext;
  readJsonBody(request: IncomingMessage): Promise<unknown>;
  requestId(request: IncomingMessage): string;
  headers(request: IncomingMessage): Record<string, string | undefined>;
}

export async function handleRustyViewChatRouteRequest(
  request: IncomingMessage,
  url: URL,
  context: RustyViewChatRouteContext,
): Promise<ServiceRouteResult> {
  const streamResult = await handleRustyViewChatStreamRequest(
    request,
    url,
    context.stream,
  );
  if (streamResult !== undefined) {
    return withChatCors(streamResult, request, context.stream.corsHeaders);
  }
  const body =
    (request.method ?? "GET").toUpperCase() === "POST"
      ? await context.readJsonBody(request)
      : undefined;
  const result = await handleRustyViewChatRequest(
    {
      method: request.method ?? "GET",
      url: url.toString(),
      headers: context.headers(request),
      body,
      requestId: context.requestId(request),
    },
    context.chat,
  );
  return withChatCors(result, request, context.stream.corsHeaders);
}

export async function handleRustyViewChatStreamRequest(
  request: IncomingMessage,
  url: URL,
  context: RustyViewChatStreamRouteContext,
): Promise<ServiceRouteResult | undefined> {
  const parts = url.pathname.split("/").filter(Boolean);
  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "chat" &&
    parts[2] === "sessions" &&
    parts[4] === "attachments" &&
    parts[5] === "upload"
  ) {
    return handleAttachmentUploadRequest(request, url, context, parts);
  }
  if (
    parts.length === 7 &&
    parts[0] === "v1" &&
    parts[1] === "chat" &&
    parts[2] === "sessions" &&
    parts[4] === "attachments" &&
    parts[6] === "content"
  ) {
    return handleAttachmentContentRequest(request, context, parts);
  }
  if (
    parts.length !== 5 ||
    parts[0] !== "v1" ||
    parts[1] !== "chat" ||
    parts[2] !== "sessions" ||
    parts[4] !== "stream"
  ) {
    return undefined;
  }

  const requestIdValue = requestId(request);
  if ((request.method ?? "GET").toUpperCase() !== "GET") {
    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "chat_stream_requires_get",
      message: "Rusty View chat stream routes only support GET",
      retryable: false,
    });
  }

  const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
  const sessions = await context.listSessions();
  const session = sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "chat_session_not_found",
      message: `chat session ${sessionId} was not found`,
      retryable: false,
    });
  }

  const cursor =
    stringHeader(request, "last-event-id") ?? stringParam(url, "cursor");
  const replay = await context.streamReplayEvents(session, cursor, url);
  const closeAfterReplay =
    url.searchParams.get("once") === "true" ||
    url.searchParams.get("close_after_replay") === "true";
  return {
    kind: "raw",
    write(response) {
      writeRustyViewChatSseStream({
        context,
        session,
        replay,
        closeAfterReplay,
        request,
        response,
      });
    },
  };
}

async function handleAttachmentUploadRequest(
  request: IncomingMessage,
  url: URL,
  context: RustyViewChatStreamRouteContext,
  parts: string[],
): Promise<ServiceRouteResult> {
  const requestIdValue = requestId(request);
  if ((request.method ?? "GET").toUpperCase() !== "POST") {
    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "attachment_upload_requires_post",
      message: "attachment upload routes only support POST",
      retryable: false,
    });
  }
  const sessionId = decodeURIComponent(parts[3] ?? "");
  const session = (await context.listSessions()).find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "chat_session_not_found",
      message: `chat session ${sessionId} was not found`,
      retryable: false,
    });
  }
  if (session.status === "archived") {
    return failure(412, requestIdValue, {
      code: "failed_precondition",
      reason_code: "chat_session_archived",
      message: `chat session ${sessionId} is archived`,
      retryable: false,
    });
  }
  const filename = stringParam(url, "filename");
  if (filename === undefined) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "attachment_filename_required",
      message: "attachment upload requires a filename query parameter",
      retryable: false,
    });
  }
  const mimeType = (stringHeader(request, "content-type") ?? "")
    .split(";", 1)[0]!
    .trim()
    .toLowerCase();
  const idempotencyKey =
    stringHeader(request, "idempotency-key") ?? requestIdValue;
  try {
    const bytes = await readBoundedAttachmentBody(request);
    const content = await context.uploadAttachmentContent({
      sessionId,
      idempotencyKey,
      filename,
      mimeType,
      bytes,
    });
    return {
      ...successRoute(requestIdValue, {
        status: "created",
        attachment: content.attachment,
      }),
      status: 201,
    };
  } catch (error) {
    const reasonCode =
      error instanceof ToolMediaAttachmentError
        ? error.reasonCode
        : "attachment_upload_failed";
    const invalid = [
      "attachment_upload_empty",
      "attachment_upload_oversized",
      "unsupported_image_mime_type",
      "invalid_image_byte_size",
      "invalid_image_dimensions",
      "invalid_image_content",
    ].includes(reasonCode);
    return failure(invalid ? 400 : 500, requestIdValue, {
      code: invalid ? "invalid_input" : "internal_error",
      reason_code: reasonCode,
      message: error instanceof Error ? error.message : String(error),
      retryable: false,
    });
  }
}

async function readBoundedAttachmentBody(
  request: IncomingMessage,
): Promise<Buffer> {
  const chunks: Buffer[] = [];
  let byteSize = 0;
  for await (const chunk of request) {
    const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    byteSize += bytes.length;
    if (byteSize > MAX_CHAT_IMAGE_BYTES) {
      throw new ToolMediaAttachmentError(
        "attachment_upload_oversized",
        `attachment upload exceeds ${MAX_CHAT_IMAGE_BYTES} bytes`,
      );
    }
    chunks.push(bytes);
  }
  if (byteSize === 0) {
    throw new ToolMediaAttachmentError(
      "attachment_upload_empty",
      "attachment upload body is empty",
    );
  }
  return Buffer.concat(chunks, byteSize);
}

async function handleAttachmentContentRequest(
  request: IncomingMessage,
  context: RustyViewChatStreamRouteContext,
  parts: string[],
): Promise<ServiceRouteResult> {
  const requestIdValue = requestId(request);
  if ((request.method ?? "GET").toUpperCase() !== "GET") {
    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "attachment_content_requires_get",
      message: "attachment content routes only support GET",
      retryable: false,
    });
  }
  const sessionId = decodeURIComponent(parts[3] ?? "");
  const attachmentId = decodeURIComponent(parts[5] ?? "");
  try {
    const content = await context.readAttachmentContent(
      sessionId,
      attachmentId,
    );
    const filename = content.attachment.filename.replace(/[\r\n"\\]/g, "_");
    const metadata = recordValue(content.attachment.metadata_json);
    const sha256 = stringValue(metadata.content_sha256);
    const width = positiveInteger(metadata.width);
    const height = positiveInteger(metadata.height);
    const etag = sha256 === undefined ? undefined : `"sha256:${sha256}"`;
    return {
      kind: "raw",
      write(response) {
        if (etag !== undefined && request.headers["if-none-match"] === etag) {
          response.writeHead(304, {
            etag,
            "cache-control": "private, max-age=60",
            ...context.corsHeaders(request),
          });
          response.end();
          return;
        }
        response.writeHead(200, {
          "content-type": content.attachment.mime_type,
          "content-length": String(content.bytes.length),
          "content-disposition": `inline; filename="${filename}"`,
          "cache-control": "private, max-age=60",
          ...(etag === undefined ? {} : { etag }),
          ...(sha256 === undefined ? {} : { "x-content-sha256": sha256 }),
          ...(width === undefined ? {} : { "x-image-width": String(width) }),
          ...(height === undefined ? {} : { "x-image-height": String(height) }),
          "x-content-type-options": "nosniff",
          ...context.corsHeaders(request),
        });
        response.end(content.bytes);
      },
    };
  } catch (error) {
    const reasonCode =
      error instanceof ToolMediaAttachmentError
        ? error.reasonCode
        : "attachment_content_read_failed";
    return failure(
      reasonCode === "attachment_not_found" ? 404 : 410,
      requestIdValue,
      {
        code:
          reasonCode === "attachment_not_found"
            ? "not_found"
            : "failed_precondition",
        reason_code: reasonCode,
        message: error instanceof Error ? error.message : String(error),
        retryable: false,
      },
    );
  }
}

function recordValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function positiveInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? value
    : undefined;
}

export function writeRustyViewChatSseStream(input: {
  context: RustyViewChatStreamRouteContext;
  session: SessionState;
  replay: readonly ChatEvent[];
  closeAfterReplay: boolean;
  request: IncomingMessage;
  response: ServerResponse;
}): void {
  const { context, session, response } = input;
  response.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache, no-transform",
    connection: "keep-alive",
    "x-accel-buffering": "no",
    ...context.corsHeaders(input.request),
  });
  response.write(": connected\n\n");
  for (const event of input.replay) {
    writeSseEvent(response, event);
  }
  if (input.closeAfterReplay) {
    response.end();
    return;
  }

  const subscriber: ChatStreamSubscriber = {
    write(event) {
      writeSseEvent(response, event);
    },
  };
  const subscribers = context.subscribersForSession(session.sessionId);
  subscribers.add(subscriber);
  const heartbeat = setInterval(() => {
    if (!response.destroyed) response.write(": keep-alive\n\n");
  }, 15_000);
  context.timers.add(heartbeat);

  const cleanup = () => {
    clearInterval(heartbeat);
    context.timers.delete(heartbeat);
    subscribers.delete(subscriber);
    if (subscribers.size === 0) {
      context.deleteSubscribersForSession(session.sessionId);
    }
  };
  response.on("close", cleanup);
  response.on("error", cleanup);
}

export function writeSseEvent(
  response: Pick<ServerResponse, "destroyed" | "write">,
  event: ChatEvent,
): void {
  if (response.destroyed) return;
  response.write(`id: ${event.event_id}\n`);
  response.write(`event: ${event.kind}\n`);
  response.write(`data: ${JSON.stringify(event)}\n\n`);
}

export function isChatRoute(pathname: string): boolean {
  return pathname === "/v1/chat" || pathname.startsWith("/v1/chat/");
}

function withChatCors<T extends ServiceRouteResult>(
  result: T,
  request: IncomingMessage,
  corsHeaders: (request: IncomingMessage) => Record<string, string>,
): T {
  if (isRawServiceRouteResult(result)) return result;
  return {
    ...result,
    headers: {
      ...result.headers,
      ...corsHeaders(request),
    },
  };
}

function stringParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value;
}

function stringHeader(
  request: IncomingMessage,
  name: string,
): string | undefined {
  const value = request.headers[name.toLowerCase()];
  if (Array.isArray(value)) return value.find((candidate) => candidate.trim());
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function requestId(request: IncomingMessage): string {
  const value = request.headers["x-request-id"];
  return typeof value === "string" && value.trim()
    ? value.trim()
    : `req_${Date.now()}`;
}
