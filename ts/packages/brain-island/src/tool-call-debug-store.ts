import { createHash, randomBytes } from "node:crypto";
import type { ToolCallMetadata } from "@rusty-crew/contracts";

export type ToolCallDebugStatus = "running" | "completed" | "failed";

export interface ToolCallDebugLimits {
  maxJsonChars: number;
  maxPartialUpdates: number;
  retentionMs: number;
  maxRecords: number;
}

export interface ToolCallDebugValue {
  value: unknown;
  truncated: boolean;
  redacted: boolean;
  sha256?: string;
  originalJsonChars?: number;
}

export interface ToolCallDebugUpdate {
  recorded_at: string;
  partial_result: ToolCallDebugValue;
}

export interface ToolCallDebugError {
  name?: string;
  message: string;
}

export interface ToolCallDebugRecord {
  debug_detail_id: string;
  tool_call_id: string;
  session_id: string;
  wake_id: string;
  tool_name: string;
  status: ToolCallDebugStatus;
  arguments: ToolCallDebugValue;
  partial_updates: ToolCallDebugUpdate[];
  final_result?: ToolCallDebugValue;
  error?: ToolCallDebugError;
  source_metadata: ToolCallMetadata;
  started_at: string;
  updated_at: string;
  expires_at: string;
  limits: ToolCallDebugLimits;
}

export interface StartToolCallDebugInput {
  toolCallId: string;
  sessionId: string;
  wakeId: string;
  toolName: string;
  arguments: unknown;
  sourceMetadata?: ToolCallMetadata;
}

export interface ToolCallDebugStore {
  readonly limits: ToolCallDebugLimits;
  start(input: StartToolCallDebugInput): ToolCallDebugRecord;
  recordUpdate(input: {
    debugDetailId: string;
    partialResult: unknown;
  }): ToolCallDebugRecord | undefined;
  finish(input: {
    debugDetailId: string;
    finalResult: unknown;
  }): ToolCallDebugRecord | undefined;
  fail(input: {
    debugDetailId: string;
    error: unknown;
  }): ToolCallDebugRecord | undefined;
  get(input: {
    sessionId: string;
    debugDetailId: string;
  }): ToolCallDebugRecord | undefined;
  referenceForToolCall(input: {
    sessionId: string;
    wakeId: string;
    toolCallId: string;
  }): ToolCallDebugRecord | undefined;
}

export interface MemoryToolCallDebugStoreOptions {
  maxJsonChars?: number;
  maxPartialUpdates?: number;
  retentionMs?: number;
  maxRecords?: number;
  now?: () => string;
}

const DEFAULT_LIMITS: ToolCallDebugLimits = {
  maxJsonChars: 24_000,
  maxPartialUpdates: 8,
  retentionMs: 60 * 60 * 1000,
  maxRecords: 1_000,
};

const REDACTED_KEY_PATTERN =
  /(?:api[_-]?key|authorization|bearer|credential|password|secret|token)/i;

export class MemoryToolCallDebugStore implements ToolCallDebugStore {
  readonly limits: ToolCallDebugLimits;
  private readonly now: () => string;
  private readonly records = new Map<string, ToolCallDebugRecord>();
  private readonly recordsByToolCall = new Map<string, string>();

  constructor(options: MemoryToolCallDebugStoreOptions = {}) {
    this.limits = {
      maxJsonChars: options.maxJsonChars ?? DEFAULT_LIMITS.maxJsonChars,
      maxPartialUpdates:
        options.maxPartialUpdates ?? DEFAULT_LIMITS.maxPartialUpdates,
      retentionMs: options.retentionMs ?? DEFAULT_LIMITS.retentionMs,
      maxRecords: options.maxRecords ?? DEFAULT_LIMITS.maxRecords,
    };
    this.now = options.now ?? (() => new Date().toISOString());
  }

  start(input: StartToolCallDebugInput): ToolCallDebugRecord {
    this.cleanup();
    const existingDebugDetailId = this.recordsByToolCall.get(
      toolCallKey(input),
    );
    const existing =
      existingDebugDetailId === undefined
        ? undefined
        : this.records.get(existingDebugDetailId);
    if (existing) return cloneRecord(existing);
    const startedAt = this.now();
    const debugDetailId = debugDetailIdFor(input, startedAt);
    const record: ToolCallDebugRecord = {
      debug_detail_id: debugDetailId,
      tool_call_id: input.toolCallId,
      session_id: input.sessionId,
      wake_id: input.wakeId,
      tool_name: input.toolName,
      status: "running",
      arguments: boundedValue(input.arguments, this.limits.maxJsonChars),
      partial_updates: [],
      source_metadata:
        input.sourceMetadata ??
        localToolCallMetadata(input.toolName, debugDetailId),
      started_at: startedAt,
      updated_at: startedAt,
      expires_at: expiresAt(startedAt, this.limits.retentionMs),
      limits: { ...this.limits },
    };
    this.records.set(debugDetailId, record);
    this.recordsByToolCall.set(toolCallKey(input), debugDetailId);
    this.trimToLimit();
    return cloneRecord(record);
  }

  recordUpdate(input: {
    debugDetailId: string;
    partialResult: unknown;
  }): ToolCallDebugRecord | undefined {
    return this.mutate(input.debugDetailId, (record) => {
      if (record.partial_updates.length >= this.limits.maxPartialUpdates) {
        return;
      }
      record.partial_updates.push({
        recorded_at: this.now(),
        partial_result: boundedValue(
          input.partialResult,
          this.limits.maxJsonChars,
        ),
      });
    });
  }

  finish(input: {
    debugDetailId: string;
    finalResult: unknown;
  }): ToolCallDebugRecord | undefined {
    return this.mutate(input.debugDetailId, (record) => {
      record.status = "completed";
      record.final_result = boundedValue(
        input.finalResult,
        this.limits.maxJsonChars,
      );
      delete record.error;
    });
  }

  fail(input: {
    debugDetailId: string;
    error: unknown;
  }): ToolCallDebugRecord | undefined {
    return this.mutate(input.debugDetailId, (record) => {
      record.status = "failed";
      record.error = errorValue(input.error);
    });
  }

  get(input: {
    sessionId: string;
    debugDetailId: string;
  }): ToolCallDebugRecord | undefined {
    this.cleanup();
    const record = this.records.get(input.debugDetailId);
    if (!record || record.session_id !== input.sessionId) return undefined;
    return cloneRecord(record);
  }

  referenceForToolCall(input: {
    sessionId: string;
    wakeId: string;
    toolCallId: string;
  }): ToolCallDebugRecord | undefined {
    this.cleanup();
    const debugDetailId = this.recordsByToolCall.get(toolCallKey(input));
    if (!debugDetailId) return undefined;
    const record = this.records.get(debugDetailId);
    return record ? cloneRecord(record) : undefined;
  }

  private mutate(
    debugDetailId: string,
    update: (record: ToolCallDebugRecord) => void,
  ): ToolCallDebugRecord | undefined {
    this.cleanup();
    const record = this.records.get(debugDetailId);
    if (!record) return undefined;
    update(record);
    record.updated_at = this.now();
    return cloneRecord(record);
  }

  private cleanup(): void {
    const nowMs = Date.parse(this.now());
    for (const [debugDetailId, record] of this.records) {
      if (Date.parse(record.expires_at) <= nowMs) {
        this.records.delete(debugDetailId);
        this.recordsByToolCall.delete(
          toolCallKey({
            sessionId: record.session_id,
            wakeId: record.wake_id,
            toolCallId: record.tool_call_id,
          }),
        );
      }
    }
  }

  private trimToLimit(): void {
    while (this.records.size > this.limits.maxRecords) {
      const first = this.records.keys().next().value;
      if (typeof first !== "string") return;
      const record = this.records.get(first);
      this.records.delete(first);
      if (record) {
        this.recordsByToolCall.delete(
          toolCallKey({
            sessionId: record.session_id,
            wakeId: record.wake_id,
            toolCallId: record.tool_call_id,
          }),
        );
      }
    }
  }
}

export function localToolCallMetadata(
  toolName: string,
  debugDetailId?: string,
): ToolCallMetadata {
  return {
    source: "local",
    serverNames: [],
    sourceToolName: toolName,
    catalogRevision: "brain-tool-adapter",
    debugDetailId,
    policy: {
      allowed: true,
    },
  };
}

export function withToolCallDebugReference(
  metadata: ToolCallMetadata | undefined,
  debugDetailId: string | undefined,
): ToolCallMetadata | undefined {
  if (!debugDetailId) return metadata;
  return {
    ...(metadata ?? localToolCallMetadata("unknown", debugDetailId)),
    serverNames: metadata?.serverNames ?? [],
    debugDetailId,
  };
}

function debugDetailIdFor(
  input: StartToolCallDebugInput,
  startedAt: string,
): string {
  const digest = createHash("sha256")
    .update(input.sessionId)
    .update("\0")
    .update(input.wakeId)
    .update("\0")
    .update(input.toolCallId)
    .update("\0")
    .update(startedAt)
    .update("\0")
    .update(randomBytes(8))
    .digest("hex")
    .slice(0, 24);
  return `tooldbg_${digest}`;
}

function toolCallKey(input: {
  sessionId: string;
  wakeId: string;
  toolCallId: string;
}): string {
  return `${input.sessionId}\0${input.wakeId}\0${input.toolCallId}`;
}

function expiresAt(startedAt: string, retentionMs: number): string {
  return new Date(Date.parse(startedAt) + retentionMs).toISOString();
}

function boundedValue(
  value: unknown,
  maxJsonChars: number,
): ToolCallDebugValue {
  const redaction = redactValue(value);
  const json = safeJson(redaction.value);
  if (json.length <= maxJsonChars) {
    return {
      value: redaction.value,
      truncated: false,
      redacted: redaction.redacted,
      originalJsonChars: json.length,
    };
  }
  return {
    value: {
      truncated: true,
      preview: json.slice(0, Math.max(0, maxJsonChars)),
    },
    truncated: true,
    redacted: redaction.redacted,
    sha256: createHash("sha256").update(json).digest("hex"),
    originalJsonChars: json.length,
  };
}

function redactValue(value: unknown): { value: unknown; redacted: boolean } {
  if (Array.isArray(value)) {
    let redacted = false;
    const mapped = value.map((item) => {
      const next = redactValue(item);
      redacted ||= next.redacted;
      return next.value;
    });
    return { value: mapped, redacted };
  }
  if (value && typeof value === "object") {
    let redacted = false;
    const output: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(value)) {
      if (REDACTED_KEY_PATTERN.test(key)) {
        output[key] = "[redacted]";
        redacted = true;
      } else {
        const next = redactValue(nested);
        output[key] = next.value;
        redacted ||= next.redacted;
      }
    }
    return { value: output, redacted };
  }
  return { value, redacted: false };
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return JSON.stringify(String(value));
  }
}

function errorValue(error: unknown): ToolCallDebugError {
  if (error instanceof Error) {
    return { name: error.name, message: error.message };
  }
  return { message: String(error) };
}

function cloneRecord(record: ToolCallDebugRecord): ToolCallDebugRecord {
  return JSON.parse(JSON.stringify(record)) as ToolCallDebugRecord;
}
