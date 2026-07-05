import type { ChatEvent } from "./rusty-view-chat-api.js";

export interface ChatWakeFailureSummaryInput {
  failureSummary: string;
  events: readonly ChatEvent[];
  sessionId: string;
  toolDebugLookup: ToolDebugLookup;
}

export interface ToolDebugLookup {
  get(input: { sessionId: string; debugDetailId: string }):
    | {
        tool_name?: string;
        final_result?: { value: unknown };
      }
    | undefined;
}

export function buildChatWakeFailureSummaryFromEvents(
  input: ChatWakeFailureSummaryInput,
): string {
  const base = input.failureSummary.trim() || "assistant turn failed";
  if (input.events.length === 0) return base;

  const text = mergeTextParts(
    input.events.flatMap((event) => {
      if (event.kind !== "assistant_text_delta") return [];
      const payload = event.payload;
      return typeof payload.text === "string" ? [payload.text] : [];
    }),
  ).trim();
  const reasoningDeltaCount = input.events.filter(
    (event) => event.kind === "assistant_reasoning_delta",
  ).length;
  const startedTools = toolCallsForChatEvents(
    input.events,
    "tool_call_started",
  );
  const completedTools = toolCallsForChatEvents(
    input.events,
    "tool_call_completed",
  );
  const failedTools = toolCallsForChatEvents(input.events, "tool_call_failed");
  const unsuccessfulCompletedTools = unsuccessfulCompletedToolSummaries(input);
  const inFlightTools = [...startedTools].filter(
    ([toolCallId]) =>
      !completedTools.has(toolCallId) && !failedTools.has(toolCallId),
  );
  const providerStatuses = input.events.flatMap((event) => {
    if (event.kind !== "provider_status") return [];
    const payload = event.payload;
    if (typeof payload.message !== "string") return [];
    return [`${String(payload.level ?? "info")}: ${payload.message}`];
  });

  const lines = [`Assistant turn failed before it could finish: ${base}`];
  if (text) {
    lines.push(`Partial response before failure: ${truncate(text, 360)}`);
  }
  if (failedTools.size > 0) {
    lines.push(`Failed tool calls: ${formatToolCallMap(failedTools)}.`);
  }
  if (unsuccessfulCompletedTools.length > 0) {
    lines.push(
      `Tool calls reporting unsuccessful results: ${unsuccessfulCompletedTools
        .slice(0, 5)
        .join("; ")}.`,
    );
  }
  if (completedTools.size > 0) {
    lines.push(`Completed tool calls before failure: ${completedTools.size}.`);
  }
  if (inFlightTools.length > 0) {
    lines.push(
      `Tool calls still in flight: ${inFlightTools
        .slice(0, 5)
        .map(([, toolName]) => toolName)
        .join(", ")}.`,
    );
  }
  if (reasoningDeltaCount > 0) {
    lines.push(`Reasoning updates before failure: ${reasoningDeltaCount}.`);
  }
  if (providerStatuses.length > 0) {
    lines.push(
      `Recent provider status: ${providerStatuses.slice(-3).join("; ")}.`,
    );
  }
  return truncate(lines.join("\n"), 1_500);
}

function toolCallsForChatEvents(
  events: readonly ChatEvent[],
  kind: ChatEvent["kind"],
): Map<string, string> {
  const calls = new Map<string, string>();
  for (const event of events) {
    if (event.kind !== kind) continue;
    const payload = event.payload;
    if (typeof payload.tool_name !== "string") {
      continue;
    }
    const toolCallId =
      typeof payload.tool_call_id === "string"
        ? payload.tool_call_id
        : `${kind}:${calls.size}:${payload.tool_name}`;
    calls.set(toolCallId, payload.tool_name);
  }
  return calls;
}

function unsuccessfulCompletedToolSummaries(
  input: ChatWakeFailureSummaryInput,
): string[] {
  return input.events.flatMap((event) => {
    if (event.kind !== "tool_call_completed") return [];
    const payload = event.payload;
    const debugDetailId =
      typeof payload.debug_detail_id === "string"
        ? payload.debug_detail_id
        : undefined;
    if (!debugDetailId) return [];
    const record = input.toolDebugLookup.get({
      sessionId: input.sessionId,
      debugDetailId,
    });
    const detail = unsuccessfulToolDetail(record?.final_result?.value);
    if (!detail) return [];
    const toolName =
      typeof payload.tool_name === "string"
        ? payload.tool_name
        : record?.tool_name;
    return [toolName ? `${toolName} (${detail})` : detail];
  });
}

function unsuccessfulToolDetail(value: unknown): string | undefined {
  if (!isRecord(value)) return undefined;
  const details = isRecord(value.details) ? value.details : undefined;
  if (!details) return undefined;
  if (details.ok !== false && details.action !== "failed") return undefined;
  if (typeof details.reasonCode === "string") return details.reasonCode;
  if (typeof details.action === "string") return details.action;
  return "ok=false";
}

function formatToolCallMap(calls: Map<string, string>): string {
  return [...calls.values()].slice(0, 5).join(", ");
}

function mergeTextParts(parts: readonly string[]): string {
  return parts
    .filter((part) => part.length > 0)
    .reduce((merged, part) => {
      if (!merged) return part;
      if (part.startsWith(merged)) return part;
      if (merged.endsWith(part)) return merged;
      return `${merged}${part}`;
    }, "");
}

function truncate(value: string, maxChars: number): string {
  return value.length <= maxChars ? value : `${value.slice(0, maxChars)}...`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
