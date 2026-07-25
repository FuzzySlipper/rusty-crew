import type { BrainWakeStreamItem } from "@rusty-crew/contracts";
import type { BrainWakeInput } from "./index.js";
import {
  localToolCallMetadata,
  withToolCallDebugReference,
  type ToolCallDebugStore,
} from "./tool-call-debug-store.js";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type {
  BrainToolMediaReference,
  BrainToolMediaSink,
} from "./brain-tool-media.js";

export interface BrainHostToolRequest {
  wakeId: string;
  callId: string;
  providerItemId?: string;
  name: string;
  argumentsJson: string;
}

export interface PreparedBrainHostToolRequest {
  request: BrainHostToolRequest;
  tool?: BrainTool;
  params?: unknown;
  debugDetailId?: string;
  preparationError?: string;
}

interface BrainHostToolFailure {
  toolName: string;
  reasonCode: string;
  retryable: boolean;
  action?: string;
  detail: string;
}

export interface BrainHostToolExecutionResult {
  output: string;
  failure?: BrainHostToolFailure;
  suspend?: boolean;
}

export function prepareBrainHostToolRequest(
  wake: BrainWakeInput,
  request: BrainHostToolRequest,
  toolsByName: ReadonlyMap<string, BrainTool>,
  toolCallDebugStore: ToolCallDebugStore | undefined,
): PreparedBrainHostToolRequest {
  const tool = toolsByName.get(request.name);
  if (tool === undefined) {
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: { argumentsJson: request.argumentsJson },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      debugDetailId: debugRecord?.debug_detail_id,
      preparationError: `Tool ${request.name} is not available in this brain session.`,
    };
  }
  let rawArguments: unknown;
  try {
    rawArguments =
      request.argumentsJson.trim().length === 0
        ? {}
        : JSON.parse(request.argumentsJson);
  } catch (error) {
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: { argumentsJson: request.argumentsJson },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      tool,
      debugDetailId: debugRecord?.debug_detail_id,
      preparationError: `Tool ${request.name} arguments were not valid JSON: ${
        error instanceof Error ? error.message : String(error)
      }`,
    };
  }
  try {
    const params = tool.prepareArguments
      ? tool.prepareArguments(rawArguments)
      : (rawArguments as never);
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: {
        rawArguments,
        preparedArguments: params,
      },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      tool,
      params,
      debugDetailId: debugRecord?.debug_detail_id,
    };
  } catch (error) {
    const debugRecord = toolCallDebugStore?.start({
      toolCallId: request.callId,
      sessionId: wake.sessionId,
      wakeId: wake.wakeId,
      toolName: request.name,
      arguments: { rawArguments },
      sourceMetadata: localToolCallMetadata(request.name),
    });
    return {
      request,
      tool,
      debugDetailId: debugRecord?.debug_detail_id,
      preparationError: `Tool ${request.name} argument preparation failed: ${
        error instanceof Error ? error.message : String(error)
      }`,
    };
  }
}

export async function executePreparedBrainHostToolRequest(
  wake: BrainWakeInput,
  prepared: PreparedBrainHostToolRequest,
  toolCallDebugStore: ToolCallDebugStore | undefined,
  toolMediaSink?: BrainToolMediaSink,
  onUpdate?: (partialResult: BrainToolResult) => void,
  signal?: AbortSignal,
): Promise<BrainHostToolExecutionResult> {
  const failDebugRecord = (error: unknown) => {
    if (prepared.debugDetailId) {
      toolCallDebugStore?.fail({
        debugDetailId: prepared.debugDetailId,
        error,
      });
    }
  };
  if (prepared.preparationError) {
    failDebugRecord(prepared.preparationError);
    return {
      output: prepared.preparationError,
      failure: {
        toolName: prepared.request.name,
        reasonCode: "tool_preparation_failed",
        retryable: false,
        action: "failed",
        detail: prepared.preparationError,
      },
    };
  }
  if (!prepared.tool) {
    const output = `Tool ${prepared.request.name} is not available in this brain session.`;
    failDebugRecord(output);
    return {
      output,
      failure: {
        toolName: prepared.request.name,
        reasonCode: "tool_unavailable",
        retryable: false,
        action: "failed",
        detail: output,
      },
    };
  }
  try {
    const controller = new AbortController();
    const executionSignal = signal
      ? AbortSignal.any([controller.signal, signal])
      : controller.signal;
    const recordAndForwardUpdate = (partialResult: BrainToolResult) => {
      if (prepared.debugDetailId) {
        toolCallDebugStore?.recordUpdate({
          debugDetailId: prepared.debugDetailId,
          partialResult: brainToolResultToDebugValue(partialResult),
        });
      }
      onUpdate?.(partialResult);
    };
    const result = prepared.tool.executeWithContext
      ? await prepared.tool.executeWithContext(prepared.params as never, {
          wake,
          wakeId: wake.wakeId,
          sessionId: wake.sessionId,
          callId: prepared.request.callId,
          signal: executionSignal,
          onUpdate: recordAndForwardUpdate,
        })
      : await prepared.tool.execute(
          prepared.request.callId,
          prepared.params as never,
          executionSignal,
          recordAndForwardUpdate,
        );
    const failure = brainHostToolFailureFromResult(
      prepared.request.name,
      result,
    );
    let mediaReferences: readonly BrainToolMediaReference[] = [];
    if (
      failure === undefined &&
      toolMediaSink !== undefined &&
      result.content.some((item) => item.type === "image")
    ) {
      try {
        mediaReferences = await toolMediaSink.persistImages({
          sessionId: wake.sessionId,
          wakeId: wake.wakeId,
          callId: prepared.request.callId,
          toolName: prepared.request.name,
          result,
        });
      } catch (error) {
        const detail = `Tool ${prepared.request.name} media persistence failed: ${
          error instanceof Error ? error.message : String(error)
        }`;
        failDebugRecord(detail);
        return {
          output: detail,
          failure: {
            toolName: prepared.request.name,
            reasonCode: "tool_media_persistence_failed",
            retryable: false,
            action: "failed",
            detail,
          },
        };
      }
    }
    if (prepared.debugDetailId) {
      toolCallDebugStore?.finish({
        debugDetailId: prepared.debugDetailId,
        finalResult: brainToolResultToDebugValue(result, mediaReferences),
      });
      if (failure) {
        toolCallDebugStore?.fail({
          debugDetailId: prepared.debugDetailId,
          error: failure.detail,
        });
      }
    }
    return {
      output: brainToolResultToHostOutput(result, mediaReferences),
      ...(result.terminate === true ? { suspend: true } : {}),
      ...(failure === undefined ? {} : { failure }),
    };
  } catch (error) {
    if (prepared.debugDetailId) {
      toolCallDebugStore?.fail({
        debugDetailId: prepared.debugDetailId,
        error,
      });
    }
    const detail = `Tool ${prepared.request.name} failed: ${
      error instanceof Error ? error.message : String(error)
    }`;
    return {
      output: detail,
      failure: {
        toolName: prepared.request.name,
        reasonCode: "tool_exception",
        retryable: true,
        action: "failed",
        detail,
      },
    };
  }
}

export function brainToolResultToDebugValue(
  result: BrainToolResult,
  mediaReferences: readonly BrainToolMediaReference[] = [],
): BrainToolResult {
  let imageIndex = 0;
  return {
    ...result,
    content: result.content.map((item) => {
      if (item.type === "text") return item;
      const reference = mediaReferences[imageIndex++];
      return {
        type: "image" as const,
        mimeType: item.mimeType,
        data: "[redacted media bytes]",
        ...(reference === undefined
          ? {}
          : {
              attachment: {
                attachment_id: reference.attachmentId,
                filename: reference.filename,
                mime_type: reference.mimeType,
                byte_size: reference.byteSize,
                width: reference.width,
                height: reference.height,
                download_url: reference.downloadUrl,
              },
            }),
      };
    }),
  };
}

function brainHostToolFailureFromResult(
  toolName: string,
  result: BrainToolResult,
): BrainHostToolFailure | undefined {
  const details = result.details;
  if (!isRecord(details)) return undefined;
  if (details.ok !== false && details.action !== "failed") return undefined;
  const reasonCode =
    stringField(details, "reasonCode") ??
    stringField(details, "reason_code") ??
    stringField(details, "code") ??
    stringField(details, "action") ??
    "tool_reported_unsuccessful_result";
  const operation = stringField(details, "operation");
  const backend = stringField(details, "backend");
  const message = stringField(details, "message");
  const statusCode = numberField(details, "statusCode");
  const action = stringField(details, "action");
  const retryable =
    typeof details.retryable === "boolean" ? details.retryable : true;
  return {
    toolName,
    reasonCode,
    retryable,
    ...(action === undefined ? {} : { action }),
    detail: [
      message
        ? `${toolName} failed: ${message}`
        : `${toolName} returned ok=false`,
      operation ? `operation=${operation}` : undefined,
      backend ? `backend=${backend}` : undefined,
      `reason=${reasonCode}`,
      statusCode === undefined ? undefined : `status=${statusCode}`,
      `retryable=${retryable}`,
    ]
      .filter(Boolean)
      .join(" "),
  };
}

export function brainToolResultIsUnsuccessful(
  result: BrainToolResult,
): boolean {
  return brainHostToolFailureFromResult("tool", result) !== undefined;
}

export interface BrainHostToolDebugReferences {
  startByToolName: Map<string, string[]>;
  finishByToolName: Map<string, string[]>;
}

export function createBrainHostToolDebugReferences(): BrainHostToolDebugReferences {
  return {
    startByToolName: new Map(),
    finishByToolName: new Map(),
  };
}

export function addPreparedBrainHostToolDebugReferences(
  references: BrainHostToolDebugReferences,
  preparedRequests: readonly PreparedBrainHostToolRequest[],
): void {
  for (const prepared of preparedRequests) {
    if (!prepared.debugDetailId) continue;
    pushDebugReference(
      references.startByToolName,
      prepared.request.name,
      prepared.debugDetailId,
    );
    pushDebugReference(
      references.finishByToolName,
      prepared.request.name,
      prepared.debugDetailId,
    );
  }
}

function pushDebugReference(
  references: Map<string, string[]>,
  toolName: string,
  debugDetailId: string,
): void {
  const refs = references.get(toolName) ?? [];
  refs.push(debugDetailId);
  references.set(toolName, refs);
}

export function withBrainHostToolDebugReference(
  item: BrainWakeStreamItem,
  debugReferences: BrainHostToolDebugReferences,
): BrainWakeStreamItem {
  if (item.type !== "event") return item;
  const event = item.event.event;
  if (
    event.type !== "tool_call_started" &&
    event.type !== "tool_call_finished"
  ) {
    return item;
  }
  const referencesByToolName =
    event.type === "tool_call_started"
      ? debugReferences.startByToolName
      : debugReferences.finishByToolName;
  const refs = referencesByToolName.get(event.toolName);
  const debugDetailId = refs?.shift();
  if (!debugDetailId) return item;
  if (refs && refs.length === 0) {
    referencesByToolName.delete(event.toolName);
  }
  return {
    ...item,
    event: {
      ...item.event,
      event: {
        ...event,
        metadata: withToolCallDebugReference(
          event.metadata ?? undefined,
          debugDetailId,
        ),
      },
    },
  };
}

export function brainToolResultToHostOutput(
  result: BrainToolResult,
  mediaReferences: readonly BrainToolMediaReference[] = [],
): string {
  let imageIndex = 0;
  const content = result.content
    .map((item) => {
      if (item.type === "text") return item.text;
      const reference = mediaReferences[imageIndex++];
      return reference === undefined
        ? `[image:${item.mimeType};unavailable]`
        : `[image attachment_id=${reference.attachmentId} filename=${reference.filename} mime_type=${reference.mimeType} byte_size=${reference.byteSize} width=${reference.width} height=${reference.height} download_url=${reference.downloadUrl}]`;
    })
    .filter((text) => text.length > 0)
    .join("\n");
  const details =
    result.details === undefined
      ? undefined
      : safeJsonStringify(result.details);
  const output =
    details === undefined || details === "{}"
      ? content
      : [content, `Details:\n${details}`].filter(Boolean).join("\n\n");
  return output || "(tool returned no content)";
}

function safeJsonStringify(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(
  record: Record<string, unknown>,
  field: string,
): string | undefined {
  const value = record[field];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberField(
  record: Record<string, unknown>,
  field: string,
): number | undefined {
  const value = record[field];
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}
