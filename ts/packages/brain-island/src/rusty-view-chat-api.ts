import type {
  AgentMessage,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type {
  AdminApiEnvelope,
  AdminErrorCode,
  AdminRouteResult,
} from "./admin-diagnostics-api.js";
import { chatCommandRegistry } from "./api-command-registry.js";
import type { SlashCommandResponse } from "./slash-command-router.js";

export type {
  ChatCommandDescriptor,
  ChatCommandRegistry,
} from "./api-command-registry.js";

export interface RustyViewChatRouteRequest {
  method: string;
  url: string;
  headers?: Record<string, string | undefined>;
  body?: unknown;
  requestId?: string;
}

export interface RustyViewChatContext {
  listSessions(): Promise<SessionState[]>;
  projectBodyStateJson(sessionId: SessionId): Promise<Uint8Array>;
  listChatEvents?(
    session: SessionState,
    cursor: string | undefined,
    limit: number,
  ): readonly ChatEvent[];
  executeCommand?(
    input: ExecuteChatCommandInput,
  ): Promise<ExecuteChatCommandResult>;
  sendMessage?(input: ChatSendMessageInput): Promise<SendChatMessageResult>;
  now?: () => string;
}

const CHAT_SUMMARY_EVENT_LIMIT = 1_000;

export interface ChatSessionSummary {
  session_id: string;
  agent_id: string;
  profile_id: string;
  kind: string;
  status: string;
  latest_cursor: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  tool_event_count: number;
  effective_defaults?: Record<string, unknown>;
}

export interface ChatSessionPage {
  items: ChatSessionSummary[];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number;
}

export interface ChatSessionOpenResult {
  session: ChatSessionSummary;
  events: ChatEvent[];
  latest_cursor: string;
  has_more_before: boolean;
}

export interface ChatEvent {
  event_id: string;
  session_id: string;
  sequence_id: number;
  created_at: string;
  kind:
    | "session_snapshot"
    | "message_created"
    | "assistant_turn_started"
    | "assistant_text_delta"
    | "assistant_message_completed"
    | "assistant_turn_finished"
    | "tool_call_started"
    | "tool_call_completed"
    | "tool_call_failed"
    | "command_started"
    | "command_completed"
    | "command_failed"
    | "stream_error"
    | "unknown";
  payload: Record<string, unknown>;
}

export interface ChatActor {
  id: string;
  kind: "human" | "agent" | "system";
  display_name?: string;
}

export interface SendChatMessageRequest {
  actor: ChatActor;
  body: string;
  client_message_id?: string;
  reason?: string;
}

export interface ChatSendMessageInput {
  session: SessionState;
  actor: ChatActor;
  body: string;
  clientMessageId?: string;
  idempotencyKey: string;
  reason?: string;
  requestId: string;
}

export interface ExecuteChatCommandRequest {
  command: string;
  actor?: ChatActor;
}

export interface ExecuteChatCommandInput {
  session: SessionState;
  command: string;
  actor: ChatActor;
  requestId: string;
}

export interface ExecuteChatCommandResult {
  status: "completed" | "failed" | "rejected";
  command_name: string;
  summary: string;
  latest_cursor: string;
  old_session_id?: string;
  new_session_id?: string;
  reason_code?: string;
  response?: SlashCommandResponse | Record<string, unknown>;
}

export interface SendChatMessageResult {
  status: "accepted" | "duplicate" | "rejected";
  message_id: string;
  wake_id?: string;
  correlation_id?: string;
  latest_cursor: string;
  reason_code?: string;
}

interface RawBodyStateJson {
  pending_messages?: AgentMessage[];
}

export async function handleRustyViewChatRequest(
  request: RustyViewChatRouteRequest,
  context: RustyViewChatContext,
): Promise<AdminRouteResult> {
  const requestId = request.requestId ?? "rusty-view-chat";
  const url = new URL(request.url, "http://rusty-crew.local");
  const method = request.method.toUpperCase();
  if (method !== "GET") {
    if (
      method === "POST" &&
      partsMatch(url.pathname, ["v1", "chat", "sessions", "*", "messages"])
    ) {
      return handleSendMessage(request, context, requestId, url);
    }
    if (
      method === "POST" &&
      partsMatch(url.pathname, ["v1", "chat", "sessions", "*", "commands"])
    ) {
      return handleExecuteCommand(request, context, requestId, url);
    }
    return failure(405, requestId, {
      code: "method_not_allowed",
      reason_code: "chat_read_requires_get",
      message:
        "this Rusty View chat route does not support the requested method",
      retryable: false,
    });
  }

  const parts = url.pathname.split("/").filter(Boolean);
  if (url.pathname === "/v1/chat/sessions") {
    const sessions = await context.listSessions();
    return success(requestId, sessionPage(sessions, context, url));
  }

  if (url.pathname === "/v1/chat/commands") {
    return success(requestId, chatCommandRegistry());
  }

  if (
    parts.length === 4 &&
    parts[0] === "v1" &&
    parts[1] === "chat" &&
    parts[2] === "sessions"
  ) {
    const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
    const sessions = await context.listSessions();
    const session = sessions.find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return failure(404, requestId, {
        code: "not_found",
        reason_code: "chat_session_not_found",
        message: `chat session ${sessionId} was not found`,
        retryable: false,
      });
    }
    return success(
      requestId,
      await openSessionResult(
        session,
        context,
        pageLimit(url, 100, 500),
        cursorParam(request, url),
      ),
    );
  }

  if (
    parts.length === 5 &&
    parts[0] === "v1" &&
    parts[1] === "chat" &&
    parts[2] === "sessions" &&
    parts[4] === "events"
  ) {
    const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
    const sessions = await context.listSessions();
    const session = sessions.find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return failure(404, requestId, {
        code: "not_found",
        reason_code: "chat_session_not_found",
        message: `chat session ${sessionId} was not found`,
        retryable: false,
      });
    }
    return success(
      requestId,
      await eventPageResult(
        session,
        context,
        pageLimit(url, 100, 500),
        cursorParam(request, url),
      ),
    );
  }

  return failure(404, requestId, {
    code: "not_found",
    reason_code: "unknown_chat_route",
    message: `unknown Rusty View chat route ${url.pathname}`,
    retryable: false,
  });
}

async function handleSendMessage(
  request: RustyViewChatRouteRequest,
  context: RustyViewChatContext,
  requestId: string,
  url: URL,
): Promise<AdminRouteResult> {
  if (!context.sendMessage) {
    return failure(412, requestId, {
      code: "failed_precondition",
      reason_code: "chat_send_not_configured",
      message: "chat send-message execution is not configured",
      retryable: true,
    });
  }
  const parts = url.pathname.split("/").filter(Boolean);
  const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
  const sessions = await context.listSessions();
  const session = sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    return failure(404, requestId, {
      code: "not_found",
      reason_code: "chat_session_not_found",
      message: `chat session ${sessionId} was not found`,
      retryable: false,
    });
  }
  if (session.status === "archived") {
    return failure(412, requestId, {
      code: "failed_precondition",
      reason_code: "chat_session_archived",
      message: `chat session ${sessionId} is archived`,
      retryable: false,
    });
  }
  const parsed = parseSendMessageRequest(request.body);
  if (!parsed.ok) {
    return failure(400, requestId, {
      code: "invalid_input",
      reason_code: parsed.reasonCode,
      message: parsed.message,
      retryable: false,
    });
  }
  const idempotencyKey =
    request.headers?.["idempotency-key"] ??
    request.headers?.["Idempotency-Key"] ??
    parsed.value.client_message_id ??
    `${sessionId}:${requestId}`;
  const result = await context.sendMessage({
    session,
    actor: parsed.value.actor,
    body: parsed.value.body.trim(),
    clientMessageId: parsed.value.client_message_id,
    idempotencyKey,
    reason: parsed.value.reason,
    requestId,
  });
  return {
    status: result.status === "rejected" ? 409 : 202,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data: result,
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

async function handleExecuteCommand(
  request: RustyViewChatRouteRequest,
  context: RustyViewChatContext,
  requestId: string,
  url: URL,
): Promise<AdminRouteResult> {
  if (!context.executeCommand) {
    return failure(412, requestId, {
      code: "failed_precondition",
      reason_code: "chat_command_execution_not_configured",
      message: "chat command execution is not configured",
      retryable: true,
    });
  }
  const parts = url.pathname.split("/").filter(Boolean);
  const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
  const sessions = await context.listSessions();
  const session = sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    return failure(404, requestId, {
      code: "not_found",
      reason_code: "chat_session_not_found",
      message: `chat session ${sessionId} was not found`,
      retryable: false,
    });
  }

  const parsed = parseExecuteCommandRequest(request.body);
  if (!parsed.ok) {
    return failure(400, requestId, {
      code: "invalid_input",
      reason_code: parsed.reasonCode,
      message: parsed.message,
      retryable: false,
    });
  }

  const result = await context.executeCommand({
    session,
    command: parsed.value.command,
    actor: parsed.value.actor ?? { id: "rusty-view", kind: "human" },
    requestId,
  });
  return {
    status: result.status === "completed" ? 200 : 409,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data: result,
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

function sessionPage(
  sessions: SessionState[],
  context: RustyViewChatContext,
  url: URL,
): ChatSessionPage {
  const limit = pageLimit(url, 100, 500);
  const offset = pageOffset(url);
  const profileId = trimmedParam(url, "profile_id");
  const status = trimmedParam(url, "status");
  const filtered = sessions
    .filter(
      (session) => profileId === undefined || session.profileId === profileId,
    )
    .filter((session) => status === undefined || session.status === status)
    .sort((left, right) => left.sessionId.localeCompare(right.sessionId));
  const items = filtered.slice(offset, offset + limit).map((session) => {
    const stats = chatEventStats(session, context);
    return sessionSummary(session, {
      messageCount: stats.messageCount,
      latestCursor: stats.latestCursor,
    });
  });
  return {
    items,
    total: filtered.length,
    limit,
    offset,
    ...(offset + items.length < filtered.length
      ? { nextOffset: offset + items.length }
      : {}),
  };
}

async function openSessionResult(
  session: SessionState,
  context: RustyViewChatContext,
  limit: number,
  cursor: string | undefined,
): Promise<ChatSessionOpenResult> {
  const now = context.now?.() ?? new Date().toISOString();
  const pendingMessages = await pendingMessagesForSession(session, context);
  const stats = chatEventStats(session, context);
  const loggedEvents = context.listChatEvents?.(session, cursor, limit) ?? [];
  const snapshot: ChatEvent = {
    event_id: eventId(session.sessionId, 0),
    session_id: session.sessionId,
    sequence_id: 0,
    created_at: session.lastActiveAt,
    kind: "session_snapshot",
    payload: {
      session: sessionSummary(session, {
        messageCount: stats.hasLoggedEvents
          ? stats.messageCount
          : pendingMessages.length,
        latestCursor: stats.latestCursor,
      }),
    },
  };
  const events: ChatEvent[] = [
    snapshot,
    ...(loggedEvents.length > 0
      ? loggedEvents
      : pendingMessages.map((message, index) =>
          messageCreatedEvent(session, message, index + 1, now),
        )),
  ].slice(0, limit);
  const latestSequence = events.at(-1)?.sequence_id ?? 0;
  return {
    session: sessionSummary(session, {
      messageCount: stats.hasLoggedEvents
        ? stats.messageCount
        : pendingMessages.length,
      latestCursor: stats.latestCursor,
    }),
    events,
    latest_cursor: cursorFor(session.sessionId, latestSequence),
    has_more_before: false,
  };
}

async function eventPageResult(
  session: SessionState,
  context: RustyViewChatContext,
  limit: number,
  cursor: string | undefined,
): Promise<{ items: ChatEvent[]; latest_cursor: string; has_more: boolean }> {
  const events =
    context.listChatEvents?.(session, cursor, limit) ??
    (await pendingMessagesForSession(session, context)).map((message, index) =>
      messageCreatedEvent(
        session,
        message,
        cursorSequence(cursor, session.sessionId) + index + 1,
        context.now?.() ?? new Date().toISOString(),
      ),
    );
  const latestSequence =
    events.at(-1)?.sequence_id ?? cursorSequence(cursor, session.sessionId);
  return {
    items: [...events],
    latest_cursor: cursorFor(session.sessionId, latestSequence),
    has_more: events.length >= limit,
  };
}

async function pendingMessagesForSession(
  session: SessionState,
  context: RustyViewChatContext,
): Promise<AgentMessage[]> {
  try {
    const raw = context.projectBodyStateJson(session.sessionId);
    const bytes = await raw;
    const parsed = JSON.parse(
      new TextDecoder().decode(bytes),
    ) as RawBodyStateJson;
    return parsed.pending_messages ?? [];
  } catch {
    return [];
  }
}

function sessionSummary(
  session: SessionState,
  options: {
    messageCount: number;
    latestCursor?: string;
  },
): ChatSessionSummary {
  return {
    session_id: session.sessionId,
    agent_id: session.agentId,
    profile_id: session.profileId,
    kind: session.kind,
    status: session.status,
    latest_cursor:
      options.latestCursor ??
      cursorFor(session.sessionId, session.brainTurnCount),
    created_at: session.createdAt,
    updated_at: session.lastActiveAt,
    message_count: options.messageCount,
    tool_event_count: session.toolProfile.tools.length,
    effective_defaults: {
      historyWindow: session.historyWindow,
      resourceLimits: session.resourceLimits,
    },
  };
}

function chatEventStats(
  session: SessionState,
  context: RustyViewChatContext,
): {
  hasLoggedEvents: boolean;
  latestCursor?: string;
  messageCount: number;
} {
  const events =
    context.listChatEvents?.(session, undefined, CHAT_SUMMARY_EVENT_LIMIT) ??
    [];
  return {
    hasLoggedEvents: events.length > 0,
    latestCursor: events.at(-1)?.event_id,
    messageCount: countChatMessages(events),
  };
}

function countChatMessages(events: readonly ChatEvent[]): number {
  return events.filter(
    (event) =>
      event.kind === "message_created" ||
      event.kind === "assistant_message_completed",
  ).length;
}

function messageCreatedEvent(
  session: SessionState,
  message: AgentMessage,
  sequence: number,
  now: string,
): ChatEvent {
  const role = message.from === session.agentId ? "assistant" : "user";
  return {
    event_id: eventId(session.sessionId, sequence),
    session_id: session.sessionId,
    sequence_id: sequence,
    created_at: now,
    kind: "message_created",
    payload: {
      message_id: `pending:${message.correlationId ?? sequence}`,
      role,
      body: message.body,
      correlation_id: message.correlationId,
    },
  };
}

function eventId(sessionId: string, sequence: number): string {
  return `${sessionId}:${sequence}`;
}

function cursorFor(sessionId: string, sequence: number): string {
  return eventId(sessionId, sequence);
}

export function cursorSequence(
  cursor: string | undefined,
  sessionId: string,
): number {
  if (!cursor) return 0;
  const prefix = `${sessionId}:`;
  if (!cursor.startsWith(prefix)) return 0;
  const sequence = Number(cursor.slice(prefix.length));
  return Number.isSafeInteger(sequence) && sequence >= 0 ? sequence : 0;
}

function cursorParam(
  request: Pick<RustyViewChatRouteRequest, "headers">,
  url: URL,
): string | undefined {
  return (
    trimmedParam(url, "cursor") ??
    request.headers?.["last-event-id"] ??
    request.headers?.["Last-Event-ID"]
  );
}

function pageLimit(url: URL, fallback: number, max: number): number {
  const value = Number(url.searchParams.get("limit") ?? fallback);
  return Number.isSafeInteger(value) && value > 0
    ? Math.min(value, max)
    : fallback;
}

function pageOffset(url: URL): number {
  const value = Number(url.searchParams.get("offset") ?? 0);
  return Number.isSafeInteger(value) && value >= 0 ? value : 0;
}

function trimmedParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value.trim();
}

function parseSendMessageRequest(
  value: unknown,
):
  | { ok: true; value: SendChatMessageRequest }
  | { ok: false; reasonCode: string; message: string } {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return {
      ok: false,
      reasonCode: "invalid_chat_message_body",
      message: "chat message body must be a JSON object",
    };
  }
  const record = value as Record<string, unknown>;
  const actor = record.actor;
  if (typeof actor !== "object" || actor === null || Array.isArray(actor)) {
    return {
      ok: false,
      reasonCode: "invalid_chat_actor",
      message: "chat message actor is required",
    };
  }
  const actorRecord = actor as Record<string, unknown>;
  const actorId = stringValue(actorRecord.id);
  const actorKind = stringValue(actorRecord.kind);
  if (
    actorId === undefined ||
    (actorKind !== "human" && actorKind !== "agent" && actorKind !== "system")
  ) {
    return {
      ok: false,
      reasonCode: "invalid_chat_actor",
      message: "chat actor requires id and kind",
    };
  }
  const body = stringValue(record.body);
  if (body === undefined || body.trim() === "") {
    return {
      ok: false,
      reasonCode: "empty_chat_message",
      message: "chat message body is empty",
    };
  }
  return {
    ok: true,
    value: {
      actor: {
        id: actorId,
        kind: actorKind,
        display_name: stringValue(actorRecord.display_name),
      },
      body,
      client_message_id: stringValue(record.client_message_id),
      reason: stringValue(record.reason),
    },
  };
}

function parseExecuteCommandRequest(
  value: unknown,
):
  | { ok: true; value: ExecuteChatCommandRequest }
  | { ok: false; reasonCode: string; message: string } {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return {
      ok: false,
      reasonCode: "invalid_chat_command_body",
      message: "chat command body must be a JSON object",
    };
  }
  const record = value as Record<string, unknown>;
  const command = stringValue(record.command);
  if (command === undefined || !command.startsWith("/")) {
    return {
      ok: false,
      reasonCode: "invalid_chat_command",
      message: "chat command must be a slash command string",
    };
  }
  const actor = parseOptionalActor(record.actor);
  if (!actor.ok) return actor;
  return {
    ok: true,
    value: {
      command,
      actor: actor.value,
    },
  };
}

function parseOptionalActor(
  value: unknown,
):
  | { ok: true; value?: ChatActor }
  | { ok: false; reasonCode: string; message: string } {
  if (value === undefined) return { ok: true };
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return {
      ok: false,
      reasonCode: "invalid_chat_actor",
      message: "chat command actor must be an object",
    };
  }
  const record = value as Record<string, unknown>;
  const id = stringValue(record.id);
  const kind = stringValue(record.kind);
  if (
    id === undefined ||
    (kind !== "human" && kind !== "agent" && kind !== "system")
  ) {
    return {
      ok: false,
      reasonCode: "invalid_chat_actor",
      message: "chat command actor requires id and kind",
    };
  }
  return {
    ok: true,
    value: {
      id,
      kind,
      display_name: stringValue(record.display_name),
    },
  };
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function partsMatch(pathname: string, pattern: readonly string[]): boolean {
  const parts = pathname.split("/").filter(Boolean);
  return (
    parts.length === pattern.length &&
    pattern.every((part, index) => part === "*" || part === parts[index])
  );
}

function success<T>(requestId: string, data: T): AdminRouteResult<T> {
  return {
    status: 200,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data,
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

function failure(
  status: number,
  requestId: string,
  error: {
    code: AdminErrorCode;
    reason_code: string;
    message: string;
    retryable: boolean;
  },
): AdminRouteResult {
  const body: AdminApiEnvelope<never> = {
    ok: false,
    error,
    meta: { request_id: requestId, schema_version: 1 },
  };
  return {
    status,
    headers: { "content-type": "application/json" },
    body,
  };
}
