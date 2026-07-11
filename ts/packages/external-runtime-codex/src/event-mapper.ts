import type { ServerNotification } from "../protocol/0.144.1/ts/ServerNotification.js";
import { captureBoundedRawDetail } from "./raw-detail.js";
import type {
  JsonRpcId,
  NeutralExternalEventKind,
  NeutralExternalRuntimeEvent,
} from "./types.js";

export function mapNotification(
  notification: ServerNotification | { method: string; params: unknown },
  transportSequence: number,
  maxRawDetailBytes: number,
  known: boolean,
): NeutralExternalRuntimeEvent {
  const params = asRecord(notification.params);
  const item = asRecord(params.item);
  const turn = asRecord(params.turn);
  const threadId = stringValue(params.threadId);
  const turnId = stringValue(params.turnId) ?? stringValue(turn.id);
  const itemId = stringValue(params.itemId) ?? stringValue(item.id);
  return {
    transportSequence,
    method: notification.method,
    kind: known
      ? classifyNotification(notification.method, item)
      : "unknown_native_notification",
    ...(threadId === undefined ? {} : { threadId }),
    ...(turnId === undefined ? {} : { turnId }),
    ...(itemId === undefined ? {} : { itemId }),
    payload: params,
    rawDetail: captureBoundedRawDetail(notification, maxRawDetailBytes),
  };
}

export function mapUnsupportedServerRequest(
  request: { id: JsonRpcId; method: string; params: unknown },
  transportSequence: number,
  maxRawDetailBytes: number,
): NeutralExternalRuntimeEvent {
  const params = asRecord(request.params);
  const threadId = stringValue(params.threadId);
  const turnId = stringValue(params.turnId);
  return {
    transportSequence,
    method: request.method,
    kind: "unsupported_server_request" as const,
    ...(threadId === undefined ? {} : { threadId }),
    ...(turnId === undefined ? {} : { turnId }),
    nativeRequestId: request.id,
    payload: params,
    rawDetail: captureBoundedRawDetail(request, maxRawDetailBytes),
  };
}

function classifyNotification(
  method: string,
  item: Record<string, unknown>,
): NeutralExternalEventKind {
  const itemType = stringValue(item.type);
  if (method === "item/agentMessage/delta") return "assistant_text_delta";
  if (method.startsWith("item/reasoning/")) return "reasoning_delta";
  if (method === "item/plan/delta" || method === "turn/plan/updated") {
    return "plan_delta";
  }
  if (method.startsWith("thread/tokenUsage")) return "usage";
  if (method === "thread/compacted") return "compaction";
  if (method.startsWith("thread/")) return "thread_lifecycle";
  if (method.startsWith("turn/")) return "turn_lifecycle";
  if (
    itemType === "commandExecution" ||
    itemType === "command_execution" ||
    method.includes("commandExecution") ||
    method.startsWith("command/")
  ) {
    return "command_activity";
  }
  if (
    itemType === "fileChange" ||
    itemType === "file_change" ||
    method.includes("fileChange")
  ) {
    return "file_activity";
  }
  if (
    itemType === "mcpToolCall" ||
    itemType === "mcp_tool_call" ||
    method.startsWith("mcpServer/") ||
    method.includes("mcpToolCall")
  ) {
    return "mcp_activity";
  }
  if (
    method.startsWith("item/") &&
    (itemType === "dynamicToolCall" || itemType === "dynamic_tool_call")
  ) {
    return "dynamic_tool_activity";
  }
  if (method.startsWith("item/")) return "item_lifecycle";
  if (
    method === "warning" ||
    method === "guardianWarning" ||
    method === "configWarning" ||
    method === "deprecationNotice" ||
    method === "error"
  ) {
    return "runtime_warning";
  }
  return "runtime_status";
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : { value };
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
