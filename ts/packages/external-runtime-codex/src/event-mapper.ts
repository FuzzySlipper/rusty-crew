import type { ServerNotification } from "../protocol/0.144.1/ts/ServerNotification.js";
import { captureBoundedRawDetail } from "./raw-detail.js";
import { replaceAgentMessageFileLinks } from "./agent-message-file-links.js";
import type {
  JsonRpcId,
  NeutralExternalEventKind,
  ExternalRuntimeMediaCaptureCandidate,
  ExternalRuntimeDocumentCaptureCandidate,
  NeutralExternalRuntimeEvent,
  NeutralExternalRuntimeEventPayload,
} from "./types.js";
import { projectCodexErrorDiagnostic } from "./error-diagnostics.js";

export function mapNotification(
  notification: ServerNotification | { method: string; params: unknown },
  transportSequence: number,
  maxRawDetailBytes: number,
  known: boolean,
): NeutralExternalRuntimeEvent {
  const params = asRecord(notification.params);
  const item = asRecord(params.item);
  const turn = asRecord(params.turn);
  const thread = asRecord(params.thread);
  const threadId = stringValue(params.threadId) ?? stringValue(thread.id);
  const turnId = stringValue(params.turnId) ?? stringValue(turn.id);
  const itemId = stringValue(params.itemId) ?? stringValue(item.id);
  const kind = known
    ? classifyNotification(notification.method, item)
    : "unknown_native_notification";
  const documents = projectDocumentCandidates(
    item,
    notification.method === "item/completed",
  );
  return {
    transportSequence,
    method: notification.method,
    kind,
    ...(threadId === undefined ? {} : { threadId }),
    ...(turnId === undefined ? {} : { turnId }),
    ...(itemId === undefined ? {} : { itemId }),
    payload: projectPayload(
      notification.method,
      kind,
      params,
      documents.sanitizedItem,
      turn,
    ),
    ...projectMediaCandidates(item),
    ...(documents.candidates.length === 0
      ? {}
      : { documentCandidates: documents.candidates }),
    rawDetail: captureBoundedRawDetail(notification, maxRawDetailBytes),
  };
}

function projectDocumentCandidates(
  item: Record<string, unknown>,
  captureCompletedMessage: boolean,
): {
  candidates: readonly ExternalRuntimeDocumentCaptureCandidate[];
  sanitizedItem: Record<string, unknown>;
} {
  if (stringValue(item.type) !== "agentMessage") {
    return { candidates: [], sanitizedItem: item };
  }
  const text = stringValue(item.text);
  if (text === undefined) return { candidates: [], sanitizedItem: item };
  const candidates: ExternalRuntimeDocumentCaptureCandidate[] = [];
  const sanitized = replaceAgentMessageFileLinks(
    text,
    ({ label, pathWithLine }) => {
      const path = pathWithLine.replace(/:\d+$/, "");
      const decodedPath = decodeURIComponentSafe(path);
      const displayName = label.trim() || basenameFromPath(decodedPath);
      if (captureCompletedMessage) {
        candidates.push({
          source: "agent_message_file_link",
          documentIndex: candidates.length,
          path: decodedPath,
          displayName,
        });
      }
      return displayName;
    },
  );
  return {
    candidates,
    sanitizedItem: sanitized === text ? item : { ...item, text: sanitized },
  };
}

function basenameFromPath(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1) || "document";
}

function decodeURIComponentSafe(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function projectMediaCandidates(item: Record<string, unknown>): {
  mediaCandidates?: readonly ExternalRuntimeMediaCaptureCandidate[];
} {
  const itemType = stringValue(item.type);
  if (itemType === "dynamicToolCall" || itemType === "dynamic_tool_call") {
    const candidates = arrayValue(item.contentItems).flatMap((entry, index) => {
      const content = asRecord(entry);
      const imageUrl = stringValue(content.imageUrl);
      return stringValue(content.type) === "inputImage" &&
        imageUrl !== undefined
        ? [
            {
              source: "dynamic_tool_input_image" as const,
              mediaIndex: index,
              imageUrl,
            },
          ]
        : [];
    });
    return candidates.length === 0 ? {} : { mediaCandidates: candidates };
  }
  if (itemType === "mcpToolCall" || itemType === "mcp_tool_call") {
    const result = asRecord(item.result);
    const candidates = arrayValue(result.content).flatMap((entry, index) => {
      const content = asRecord(entry);
      const data = stringValue(content.data);
      const mimeType =
        stringValue(content.mimeType) ?? stringValue(content.mime_type);
      return stringValue(content.type) === "image" &&
        data !== undefined &&
        mimeType !== undefined
        ? [
            {
              source: "mcp_image_content" as const,
              mediaIndex: index,
              data,
              mimeType,
            },
          ]
        : [];
    });
    return candidates.length === 0 ? {} : { mediaCandidates: candidates };
  }
  if (itemType === "imageView" || itemType === "image_view") {
    const path = stringValue(item.path);
    return path === undefined
      ? {}
      : {
          mediaCandidates: [{ source: "image_view_path", mediaIndex: 0, path }],
        };
  }
  return {};
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
  const itemType = stringValue(item.type);
  const status = statusValue(source.status) ?? statusValue(turn.status);
  const text =
    stringValue(params.delta) ??
    stringValue(params.text) ??
    stringValue(source.text);
  const durationMs = numberValue(source.durationMs);
  const messagePhase =
    itemType === "agentMessage" || method.includes("agentMessage")
      ? messagePhaseValue(source.phase ?? params.phase)
      : undefined;
  const summary = Array.isArray(source.summary)
    ? source.summary.filter(
        (entry): entry is string => typeof entry === "string",
      )
    : undefined;
  const base = {
    nativeMethod: method,
    ...(itemType === undefined ? {} : { itemType }),
    ...(status === undefined ? {} : { status }),
    ...(text === undefined ? {} : { text }),
    ...(messagePhase === undefined ? {} : { messagePhase }),
    ...(durationMs === undefined ? {} : { durationMs }),
    ...(summary === undefined || summary.length === 0 ? {} : { summary }),
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
      const resultText = dynamicToolResultText(source.contentItems);
      return {
        ...base,
        ...(tool === undefined ? {} : { tool }),
        ...(resultText === undefined ? {} : { text: resultText }),
        ...(typeof source.success === "boolean"
          ? { success: source.success }
          : {}),
      };
    }
    case "reasoning_delta":
      return base;
    case "usage":
      return { ...base, usage: projectTokenUsage(params.tokenUsage) };
    case "thread_lifecycle": {
      if (method !== "thread/settings/updated") return base;
      const settings = asRecord(params.threadSettings);
      const model = stringValue(settings.model);
      const modelProvider = stringValue(settings.modelProvider);
      if (model === undefined || modelProvider === undefined) return base;
      return {
        ...base,
        settings: {
          model,
          modelProvider,
          effort: stringValue(settings.effort) ?? null,
        },
      };
    }
    case "turn_lifecycle":
      return { ...base, ...projectErrorPayload(params, turn) };
    case "runtime_warning": {
      const errorPayload = projectErrorPayload(params, turn);
      const message = errorPayload.error?.message;
      return {
        ...base,
        ...(message === undefined ? {} : { message }),
        ...errorPayload,
      };
    }
    default:
      return base;
  }
}

const DYNAMIC_TOOL_RESULT_LIMIT = 4_096;
const DYNAMIC_TOOL_RESULT_TRUNCATION_MARKER = "...[truncated]";
const UNSAFE_RESULT_CONTROL_CHARACTERS =
  /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g;

function dynamicToolResultText(value: unknown): string | undefined {
  if (!Array.isArray(value)) return undefined;
  const result = value
    .map(asRecord)
    .filter((item) => stringValue(item.type) === "inputText")
    .map((item) => stringValue(item.text))
    .filter((item): item is string => item !== undefined)
    .map(projectDynamicToolResultDisplayText)
    .join("\n")
    .replace(UNSAFE_RESULT_CONTROL_CHARACTERS, " ");
  if (result.length === 0) return undefined;
  if (result.length <= DYNAMIC_TOOL_RESULT_LIMIT) return result;
  const contentLength =
    DYNAMIC_TOOL_RESULT_LIMIT - DYNAMIC_TOOL_RESULT_TRUNCATION_MARKER.length;
  let end = contentLength;
  if (end > 0 && isHighSurrogate(result.charCodeAt(end - 1))) end -= 1;
  return `${result.slice(0, end)}${DYNAMIC_TOOL_RESULT_TRUNCATION_MARKER}`;
}

/**
 * Codex dynamic tools expose their result through an inputText item. Newer
 * app-server revisions serialize the complete MCP CallToolResult envelope into
 * that item. Keep transport bookkeeping in raw detail and expose only the
 * envelope's primary text blocks as the neutral event's readable result.
 */
export function projectDynamicToolResultDisplayText(value: string): string {
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    return value;
  }
  if (!isPlainRecord(parsed) || !Array.isArray(parsed.content)) return value;
  const detailRef = callToolResultDetailRef(parsed.structuredContent);
  const text = parsed.content
    .filter(isPlainRecord)
    .filter((item) => stringValue(item.type) === "text")
    .map((item) => stringValue(item.text))
    .filter((item): item is string => item !== undefined)
    .filter((item) => !isDetailReferenceText(item, detailRef))
    .map(readableJsonText)
    .join("\n");
  if (text.length > 0) return text;
  return parsed.isError === true
    ? "Tool call failed without text output."
    : "Tool call completed without text output.";
}

function callToolResultDetailRef(value: unknown): string | undefined {
  if (!isPlainRecord(value)) return undefined;
  return stringValue(value.detail_ref) ?? stringValue(value.detailRef);
}

function isDetailReferenceText(
  value: string,
  detailRef: string | undefined,
): boolean {
  if (detailRef === undefined) return false;
  if (value === detailRef) return true;
  try {
    return JSON.parse(value) === detailRef;
  } catch {
    return false;
  }
}

function readableJsonText(value: string): string {
  try {
    const parsed = JSON.parse(value);
    if (typeof parsed === "string") return parsed;
    return JSON.stringify(parsed, null, 2);
  } catch {
    return value;
  }
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isHighSurrogate(codeUnit: number): boolean {
  return codeUnit >= 0xd800 && codeUnit <= 0xdbff;
}

function projectErrorPayload(
  params: Record<string, unknown>,
  turn: Record<string, unknown>,
): Pick<NeutralExternalRuntimeEventPayload, "error"> {
  const paramsError = asRecord(params.error);
  const error = asRecord(
    stringValue(paramsError.message) === undefined ? turn.error : params.error,
  );
  const diagnostic = projectCodexErrorDiagnostic(error);
  if (diagnostic === undefined) return {};
  return {
    error: {
      ...diagnostic,
      willRetry: params.willRetry === true,
    },
  };
}

function messagePhaseValue(
  value: unknown,
): "commentary" | "final_answer" | "unknown" | undefined {
  if (value === undefined || value === null) return undefined;
  if (value === "commentary" || value === "final_answer") return value;
  return "unknown";
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

function projectTokenUsage(
  value: unknown,
): NonNullable<NeutralExternalRuntimeEventPayload["usage"]> {
  const usage = asRecord(value);
  return {
    total: numericRecord(asRecord(usage.total)),
    last: numericRecord(asRecord(usage.last)),
    modelContextWindow: numberValue(usage.modelContextWindow) ?? null,
  };
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
  if (
    itemType === "contextCompaction" ||
    itemType === "context_compaction" ||
    method.includes("contextCompaction")
  ) {
    return "compaction";
  }
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

function arrayValue(value: unknown): readonly unknown[] {
  return Array.isArray(value) ? value : [];
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
