import type { IncomingMessage, ServerResponse } from "node:http";
import type { SessionId, SessionState } from "@rusty-crew/contracts";
import {
  handleRustyViewChatRequest,
  type ChatEvent,
  type RustyViewChatContext,
} from "./rusty-view-chat-api.js";
import type { ServiceRouteResult } from "./service-route-results.js";
import { failure, isRawServiceRouteResult } from "./service-route-results.js";

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
