import type { SessionId } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import { nativeChatEventToChatEvent } from "./service-chat-event-log.js";

const PAGE_LIMIT = 500;
const EVENT_LIMIT = 1_000;
const INTERRUPTION_REASON_CODE = "service_restart_interrupted";
const INTERRUPTION_SOURCE = "service_restart_reconciliation";
const INTERRUPTION_SUMMARY =
  "Assistant turn was interrupted by a Rusty Crew service restart before completion.";

type RestartReconciliationBridge = Pick<
  NativeBridgeModule,
  | "appendChatEvent"
  | "logicalTurnDiagnostics"
  | "queryChatSessionSummaries"
  | "readChatSession"
>;

export interface ChatRestartReconciliationReport {
  sessionsScanned: number;
  sessionsReconciled: string[];
  eventsAppended: number;
}

export async function reconcileInterruptedChatTurns(input: {
  bridge: RestartReconciliationBridge;
  now(): string;
}): Promise<ChatRestartReconciliationReport> {
  const report: ChatRestartReconciliationReport = {
    sessionsScanned: 0,
    sessionsReconciled: [],
    eventsAppended: 0,
  };
  let offset = 0;
  for (;;) {
    const result = await input.bridge.queryChatSessionSummaries({
      page: { limit: PAGE_LIMIT, offset },
    });
    const page = result.page;
    for (const facts of page.items) {
      if (facts.session.status === "archived") continue;
      report.sessionsScanned += 1;
      const read = await input.bridge.readChatSession({
        session_id: facts.session.sessionId,
        cursor: undefined,
        limit: EVENT_LIMIT,
        include_alternates: false,
      });
      if (read.source !== "event_log") continue;
      const events = read.events.map(nativeChatEventToChatEvent);
      const repair = interruptedTurnRepair(events, facts.session.sessionId);
      if (repair === undefined) continue;
      const activeLogicalTurns = await input.bridge.logicalTurnDiagnostics({
        sessionId: facts.session.sessionId,
        includeTerminal: false,
        limit: 1,
      });
      if (activeLogicalTurns.items.length > 0) continue;
      for (const event of repair.events) {
        await input.bridge.appendChatEvent({
          session_id: facts.session.sessionId,
          created_at: input.now(),
          kind: event.kind,
          payload: event.payload,
        });
        report.eventsAppended += 1;
      }
      report.sessionsReconciled.push(facts.session.sessionId);
    }
    if (page.next_offset === undefined || page.next_offset === null) break;
    offset = page.next_offset;
  }
  return report;
}

export function interruptedTurnRepair(
  events: readonly ChatEvent[],
  sessionId: SessionId,
):
  | { wakeId: string; events: Pick<ChatEvent, "kind" | "payload">[] }
  | undefined {
  const anchorIndex = findLastIndex(events, isTurnAnchor);
  if (anchorIndex < 0) return undefined;

  const trailing = events.slice(anchorIndex);
  const anchor = events[anchorIndex];
  const wakeId =
    [...trailing]
      .reverse()
      .map((event) => stringField(event.payload, "wake_id"))
      .find((value) => value !== undefined) ??
    `restart-interrupted:${sessionId}:${anchor.sequence_id}`;
  const hasCompletion = trailing.some(
    (event) =>
      event.kind === "assistant_message_completed" &&
      eventBelongsToWake(event, wakeId),
  );
  const hasFinished = trailing.some(
    (event) =>
      event.kind === "assistant_turn_finished" &&
      eventBelongsToWake(event, wakeId),
  );
  if (hasCompletion && hasFinished) return undefined;

  const repairs: Pick<ChatEvent, "kind" | "payload">[] = [];
  if (!hasCompletion) {
    repairs.push({
      kind: "assistant_message_completed",
      payload: {
        status: "failed",
        summary: INTERRUPTION_SUMMARY,
        wake_id: wakeId,
        source: INTERRUPTION_SOURCE,
        reason_code: INTERRUPTION_REASON_CODE,
      },
    });
  }
  if (!hasFinished) {
    repairs.push({
      kind: "assistant_turn_finished",
      payload: {
        status: "failed",
        wake_id: wakeId,
        source: INTERRUPTION_SOURCE,
        reason_code: INTERRUPTION_REASON_CODE,
      },
    });
  }
  return { wakeId, events: repairs };
}

function isTurnAnchor(event: ChatEvent): boolean {
  if (event.kind === "message_created") {
    return stringField(event.payload, "role") === "user";
  }
  return [
    "assistant_turn_started",
    "assistant_text_delta",
    "assistant_reasoning_delta",
    "phase_change",
    "provider_status",
    "tool_call_started",
    "tool_call_completed",
    "tool_call_failed",
  ].includes(event.kind);
}

function eventBelongsToWake(event: ChatEvent, wakeId: string): boolean {
  const eventWakeId = stringField(event.payload, "wake_id");
  return eventWakeId === undefined || eventWakeId === wakeId;
}

function findLastIndex<T>(
  values: readonly T[],
  predicate: (value: T) => boolean,
): number {
  for (let index = values.length - 1; index >= 0; index -= 1) {
    if (predicate(values[index])) return index;
  }
  return -1;
}

function stringField(
  value: Record<string, unknown>,
  key: string,
): string | undefined {
  const field = value[key];
  return typeof field === "string" && field.length > 0 ? field : undefined;
}
