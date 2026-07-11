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
  const kind = known
    ? classifyNotification(notification.method, item)
    : "unknown_native_notification";
  return {
    transportSequence,
    method: notification.method,
    kind,
    ...(threadId === undefined ? {} : { threadId }),
    ...(turnId === undefined ? {} : { turnId }),
    ...(itemId === undefined ? {} : { itemId }),
    payload: projectPayload(notification.method, kind, params, item, turn),
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
    payload: {
      nativeMethod: request.method,
      message: "unsupported external runtime server request",
    },
    rawDetail: captureBoundedRawDetail(request, maxRawDetailBytes),
  };
}

function projectPayload(
  method: string,
  kind: NeutralExternalEventKind,
  params: Record<string, unknown>,
  item: Record<string, unknown>,
  turn: Record<string, unknown>,
): NeutralExternalRuntimeEvent["payload"] {
  const source = Object.keys(item).length === 0 ? params : item;
  const status = statusValue(source.status) ?? statusValue(turn.status);
  const text =
    stringValue(params.delta) ??
    stringValue(params.text) ??
    stringValue(source.text);
  const durationMs = numberValue(source.durationMs);
  const base = {
    nativeMethod: method,
    ...(status === undefined ? {} : { status }),
    ...(text === undefined ? {} : { text }),
    ...(durationMs === undefined ? {} : { durationMs }),
  };
  switch (kind) {
    case "command_activity": {
      const command = stringValue(source.command);
      const cwd = stringValue(source.cwd);
      const output = stringValue(source.aggregatedOutput);
      const exitCode = numberValue(source.exitCode);
      return {
        ...base,
        ...(command === undefined ? {} : { command }),
        ...(cwd === undefined ? {} : { cwd }),
        ...(output === undefined ? {} : { output }),
        ...(exitCode === undefined ? {} : { exitCode }),
      };
    }
    case "file_activity":
      return {
        ...base,
        fileChanges: projectFileChanges(source.changes),
      };
    case "mcp_activity": {
      const server = stringValue(source.server);
      const tool = stringValue(source.tool);
      return {
        ...base,
        ...(server === undefined ? {} : { server }),
        ...(tool === undefined ? {} : { tool }),
      };
    }
    case "dynamic_tool_activity": {
      const tool = stringValue(source.tool);
      return {
        ...base,
        ...(tool === undefined ? {} : { tool }),
        ...(typeof source.success === "boolean"
          ? { success: source.success }
          : {}),
      };
    }
    case "reasoning_delta":
      return {
        ...base,
        ...(!Array.isArray(source.summary)
          ? {}
          : {
              summary: source.summary.filter(
                (entry): entry is string => typeof entry === "string",
              ),
            }),
      };
    case "usage":
      return { ...base, usage: numericRecord(params) };
    case "runtime_warning": {
      const message = stringValue(params.message);
      return {
        ...base,
        ...(message === undefined ? {} : { message }),
      };
    }
    default:
      return base;
  }
}

function projectFileChanges(value: unknown) {
  if (!Array.isArray(value)) return [];
  return value.map((entry) => {
    const change = asRecord(entry);
    const path = stringValue(change.path);
    const kind = stringValue(change.kind);
    const status = statusValue(change.status);
    return {
      ...(path === undefined ? {} : { path }),
      ...(kind === undefined ? {} : { kind }),
      ...(status === undefined ? {} : { status }),
    };
  });
}

function numericRecord(value: Record<string, unknown>): Record<string, number> {
  return Object.fromEntries(
    Object.entries(value).filter(
      (entry): entry is [string, number] => typeof entry[1] === "number",
    ),
  );
}

function statusValue(value: unknown): string | undefined {
  return stringValue(value) ?? stringValue(asRecord(value).type);
}

function numberValue(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
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
