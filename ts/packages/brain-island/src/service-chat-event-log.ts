import type { SessionId, SessionState } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import type { ChatStreamSubscriber } from "./service-chat-stream-routes.js";

export interface ChatEventLogContext {
  bridge: Pick<NativeBridgeModule, "appendChatEvent" | "queryChatEvents">;
  chatSubscribersBySession: Map<SessionId, Set<ChatStreamSubscriber>>;
  now(): string;
}

export async function appendChatEvent(
  context: ChatEventLogContext,
  sessionId: SessionId,
  event: Pick<ChatEvent, "kind" | "payload">,
): Promise<ChatEvent> {
  const saved = nativeChatEventToChatEvent(
    await context.bridge.appendChatEvent({
      session_id: sessionId,
      created_at: context.now(),
      kind: event.kind,
      payload: event.payload,
    }),
  );
  const subscribers = context.chatSubscribersBySession.get(sessionId);
  if (subscribers !== undefined) {
    for (const subscriber of subscribers) {
      subscriber.write(saved);
    }
  }
  return saved;
}

export function nativeChatEventToChatEvent(value: unknown): ChatEvent {
  const record = isRecord(value) ? value : {};
  return {
    event_id: String(record.event_id),
    session_id: String(record.session_id),
    sequence_id: Number(record.sequence_id),
    created_at: String(record.created_at),
    kind: (typeof record.kind === "string"
      ? record.kind
      : "unknown") as ChatEvent["kind"],
    payload: isRecord(record.payload) ? record.payload : {},
  };
}

export async function listChatEventsAfterCursor(
  context: ChatEventLogContext,
  session: SessionState,
  cursor: string | undefined,
  limit: number,
): Promise<readonly ChatEvent[]> {
  if (limit <= 0) return [];
  const page = await context.bridge.queryChatEvents({
    session_id: session.sessionId,
    cursor: cursor ?? null,
    limit,
  });
  return ((page as { items?: unknown[] }).items ?? []).map(
    nativeChatEventToChatEvent,
  );
}

export function chatSubscribers(
  context: ChatEventLogContext,
  sessionId: SessionId,
): Set<ChatStreamSubscriber> {
  const existing = context.chatSubscribersBySession.get(sessionId);
  if (existing !== undefined) return existing;
  const subscribers = new Set<ChatStreamSubscriber>();
  context.chatSubscribersBySession.set(sessionId, subscribers);
  return subscribers;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
