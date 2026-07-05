import { createHash, randomBytes } from "node:crypto";
import type { SessionId } from "@rusty-crew/contracts";

export interface ProviderRequestDebugLimits {
  maxJsonChars: number;
  retentionMs: number;
  maxRecords: number;
}

export interface ProviderRequestDebugValue {
  value: unknown;
  truncated: boolean;
  redacted: boolean;
  sha256?: string;
  originalJsonChars?: number;
}

export interface ProviderRequestDebugRecord {
  debug_detail_id: string;
  session_id: string;
  wake_id: string;
  provider: {
    brain_module: string;
    provider_alias?: string;
    model?: string;
    protocol?: string;
    provider_kind?: string;
  };
  request: ProviderRequestDebugValue;
  request_sha256: string;
  request_json_chars: number;
  recorded_at: string;
  expires_at: string;
  limits: ProviderRequestDebugLimits;
}

export interface RecordProviderRequestDebugInput {
  sessionId: SessionId | string;
  wakeId: string;
  brainModule: string;
  providerAlias?: string;
  model?: string;
  protocol?: string;
  providerKind?: string;
  request: unknown;
}

export interface ProviderRequestDebugStore {
  readonly limits: ProviderRequestDebugLimits;
  record(input: RecordProviderRequestDebugInput): ProviderRequestDebugRecord;
  get(input: {
    sessionId: SessionId | string;
    debugDetailId: string;
  }): ProviderRequestDebugRecord | undefined;
  latestForWake(input: {
    sessionId: SessionId | string;
    wakeId: string;
  }): ProviderRequestDebugRecord | undefined;
}

export interface MemoryProviderRequestDebugStoreOptions {
  maxJsonChars?: number;
  retentionMs?: number;
  maxRecords?: number;
  now?: () => string;
}

const DEFAULT_LIMITS: ProviderRequestDebugLimits = {
  maxJsonChars: 80_000,
  retentionMs: 60 * 60 * 1000,
  maxRecords: 200,
};

const REDACTED_KEY_PATTERN =
  /(?:api[_-]?key|authorization|bearer|credential|password|secret|token)/i;

export class MemoryProviderRequestDebugStore
  implements ProviderRequestDebugStore
{
  readonly limits: ProviderRequestDebugLimits;
  private readonly now: () => string;
  private readonly records = new Map<string, ProviderRequestDebugRecord>();
  private readonly latestByWake = new Map<string, string>();

  constructor(options: MemoryProviderRequestDebugStoreOptions = {}) {
    this.limits = {
      maxJsonChars: options.maxJsonChars ?? DEFAULT_LIMITS.maxJsonChars,
      retentionMs: options.retentionMs ?? DEFAULT_LIMITS.retentionMs,
      maxRecords: options.maxRecords ?? DEFAULT_LIMITS.maxRecords,
    };
    this.now = options.now ?? (() => new Date().toISOString());
  }

  record(
    input: RecordProviderRequestDebugInput,
  ): ProviderRequestDebugRecord {
    this.cleanup();
    const recordedAt = this.now();
    const request = boundedValue(input.request, this.limits.maxJsonChars);
    const requestJson = safeJson(request.value);
    const record: ProviderRequestDebugRecord = {
      debug_detail_id: debugDetailIdFor(input, recordedAt),
      session_id: String(input.sessionId),
      wake_id: input.wakeId,
      provider: {
        brain_module: input.brainModule,
        ...(input.providerAlias === undefined
          ? {}
          : { provider_alias: input.providerAlias }),
        ...(input.model === undefined ? {} : { model: input.model }),
        ...(input.protocol === undefined ? {} : { protocol: input.protocol }),
        ...(input.providerKind === undefined
          ? {}
          : { provider_kind: input.providerKind }),
      },
      request,
      request_sha256: createHash("sha256").update(requestJson).digest("hex"),
      request_json_chars: request.originalJsonChars ?? requestJson.length,
      recorded_at: recordedAt,
      expires_at: expiresAt(recordedAt, this.limits.retentionMs),
      limits: { ...this.limits },
    };
    this.records.set(record.debug_detail_id, record);
    this.latestByWake.set(
      wakeKey({ sessionId: input.sessionId, wakeId: input.wakeId }),
      record.debug_detail_id,
    );
    this.trimToLimit();
    return cloneRecord(record);
  }

  get(input: {
    sessionId: SessionId | string;
    debugDetailId: string;
  }): ProviderRequestDebugRecord | undefined {
    this.cleanup();
    const record = this.records.get(input.debugDetailId);
    if (!record || record.session_id !== String(input.sessionId)) {
      return undefined;
    }
    return cloneRecord(record);
  }

  latestForWake(input: {
    sessionId: SessionId | string;
    wakeId: string;
  }): ProviderRequestDebugRecord | undefined {
    this.cleanup();
    const debugDetailId = this.latestByWake.get(wakeKey(input));
    if (!debugDetailId) return undefined;
    const record = this.records.get(debugDetailId);
    return record ? cloneRecord(record) : undefined;
  }

  private cleanup(): void {
    const nowMs = Date.parse(this.now());
    for (const [debugDetailId, record] of this.records) {
      if (Date.parse(record.expires_at) <= nowMs) {
        this.records.delete(debugDetailId);
        this.latestByWake.delete(
          wakeKey({ sessionId: record.session_id, wakeId: record.wake_id }),
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
        this.latestByWake.delete(
          wakeKey({ sessionId: record.session_id, wakeId: record.wake_id }),
        );
      }
    }
  }
}

function debugDetailIdFor(
  input: RecordProviderRequestDebugInput,
  recordedAt: string,
): string {
  const digest = createHash("sha256")
    .update(String(input.sessionId))
    .update("\0")
    .update(input.wakeId)
    .update("\0")
    .update(input.brainModule)
    .update("\0")
    .update(recordedAt)
    .update("\0")
    .update(randomBytes(8))
    .digest("hex")
    .slice(0, 24);
  return `providerdbg_${digest}`;
}

function wakeKey(input: {
  sessionId: SessionId | string;
  wakeId: string;
}): string {
  return `${String(input.sessionId)}\0${input.wakeId}`;
}

function expiresAt(recordedAt: string, retentionMs: number): string {
  return new Date(Date.parse(recordedAt) + retentionMs).toISOString();
}

function boundedValue(
  value: unknown,
  maxJsonChars: number,
): ProviderRequestDebugValue {
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
        output[key] = {
          redacted: true,
          reason: "secret_key_name",
          sha256: createHash("sha256").update(safeJson(nested)).digest("hex"),
        };
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

function cloneRecord(
  record: ProviderRequestDebugRecord,
): ProviderRequestDebugRecord {
  return JSON.parse(JSON.stringify(record)) as ProviderRequestDebugRecord;
}
