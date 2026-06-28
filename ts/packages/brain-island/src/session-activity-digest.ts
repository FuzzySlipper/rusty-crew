import { createHash } from "node:crypto";
import type {
  CoreEvent,
  MemorySpaceId,
  ProfileId,
  SessionActivityDigest,
  SessionId,
} from "@rusty-crew/contracts";
import { assertValidSessionActivityDigest } from "@rusty-crew/contracts";

const DEFAULT_MAX_SUMMARY_CHARS = 4_000;
const DEFAULT_MAX_TEXT_CHARS = 1_200;
const DEFAULT_MAX_TOOL_CALLS = 24;
const DEFAULT_MAX_SIGNALS = 24;
const CORRECTION_HINTS = [
  "actually",
  "correction",
  "correct",
  "rather",
  "instead",
  "not ",
  "should be",
  "wrong",
];

export interface BuildSessionActivityDigestInput {
  profileId: ProfileId | string;
  sessionId: SessionId | string;
  wakeId: string;
  source: string;
  events: readonly CoreEvent[];
  completionSummary?: string;
  now: string;
  retentionUntil?: string;
  allowedCaptureSpaces?: readonly (MemorySpaceId | string)[];
  maxSummaryChars?: number;
  maxTextChars?: number;
  maxToolCalls?: number;
  maxSignals?: number;
}

export interface SessionActivityToolCallDigest {
  tool_name: string;
  status: "started" | "completed" | "failed";
  wake_id?: string;
  metadata?: unknown;
}

export interface SessionActivitySignalDigest {
  signal_type:
    | "tool_failure"
    | "provider_status"
    | "user_correction"
    | "completion";
  severity: "info" | "warning" | "error";
  summary: string;
  wake_id?: string;
  ref_id?: string;
}

export function buildSessionActivityDigest(
  input: BuildSessionActivityDigestInput,
): SessionActivityDigest {
  const profileId = String(input.profileId) as ProfileId;
  const sessionId = String(input.sessionId) as SessionId;
  const wakeId = input.wakeId.trim();
  const maxTextChars = positiveLimit(
    input.maxTextChars,
    DEFAULT_MAX_TEXT_CHARS,
  );
  const maxSummaryChars = positiveLimit(
    input.maxSummaryChars,
    DEFAULT_MAX_SUMMARY_CHARS,
  );
  const maxToolCalls = positiveLimit(
    input.maxToolCalls,
    DEFAULT_MAX_TOOL_CALLS,
  );
  const maxSignals = positiveLimit(input.maxSignals, DEFAULT_MAX_SIGNALS);

  const wakeEvents = input.events.filter((event) =>
    eventMatchesWake(event, wakeId),
  );
  const eventCounts = countEvents(wakeEvents);
  const text = mergeTextDeltas(wakeEvents).trim();
  const toolCalls = collectToolCalls(wakeEvents).slice(0, maxToolCalls);
  const signals = collectSignals(wakeEvents, input.completionSummary).slice(
    0,
    maxSignals,
  );
  const summaryText = buildSummaryText({
    source: input.source,
    wakeId,
    text: truncate(text, maxTextChars),
    toolCalls,
    signals,
    completionSummary: input.completionSummary,
    maxSummaryChars,
  });

  const digest: SessionActivityDigest = {
    digest_id: sessionActivityDigestId({ profileId, sessionId, wakeId }),
    profile_id: profileId,
    session_id: sessionId,
    wake_id: wakeId,
    source: input.source,
    summary_text: summaryText,
    event_counts_json: eventCounts,
    tool_calls_json: toolCalls,
    signals_json: signals,
    completion_summary: optionalTrimmed(input.completionSummary),
    allowed_capture_spaces:
      input.allowedCaptureSpaces?.map(
        (space) => String(space) as MemorySpaceId,
      ) ?? (["profile_dense"] as MemorySpaceId[]),
    created_at: input.now,
    retention_until: input.retentionUntil,
  };
  assertValidSessionActivityDigest(digest);
  return digest;
}

export function sessionActivityDigestId(input: {
  profileId: ProfileId | string;
  sessionId: SessionId | string;
  wakeId: string;
}): string {
  const raw = `${input.profileId}:${input.sessionId}:${input.wakeId}`;
  return `sad_${createHash("sha256").update(raw).digest("hex").slice(0, 24)}`;
}

function eventMatchesWake(event: CoreEvent, wakeId: string): boolean {
  if (event.type === "brain_event_observed") {
    return event.wakeId === undefined || event.wakeId === wakeId;
  }
  return event.type !== "brain_wake_requested";
}

function countEvents(events: readonly CoreEvent[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const event of events) {
    const key =
      event.type === "brain_event_observed"
        ? `brain_event_observed.${event.event.type}`
        : event.type;
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}

function mergeTextDeltas(events: readonly CoreEvent[]): string {
  return events
    .flatMap((event) =>
      event.type === "brain_event_observed" && event.event.type === "text_delta"
        ? [event.event.text]
        : [],
    )
    .reduce((merged, part) => {
      if (!merged) return part;
      if (part.startsWith(merged)) return part;
      if (merged.endsWith(part)) return merged;
      return `${merged}${part}`;
    }, "");
}

function collectToolCalls(
  events: readonly CoreEvent[],
): SessionActivityToolCallDigest[] {
  const calls: SessionActivityToolCallDigest[] = [];
  for (const event of events) {
    if (event.type !== "brain_event_observed") continue;
    const brainEvent = event.event;
    if (brainEvent.type === "tool_call_started") {
      calls.push({
        tool_name: brainEvent.toolName,
        status: "started",
        wake_id: event.wakeId,
        metadata: brainEvent.metadata,
      });
      continue;
    }
    if (brainEvent.type === "tool_call_finished") {
      calls.push({
        tool_name: brainEvent.toolName,
        status: brainEvent.isError ? "failed" : "completed",
        wake_id: event.wakeId,
        metadata: brainEvent.metadata,
      });
    }
  }
  return calls;
}

function collectSignals(
  events: readonly CoreEvent[],
  completionSummary: string | undefined,
): SessionActivitySignalDigest[] {
  const signals: SessionActivitySignalDigest[] = [];
  for (const event of events) {
    if (event.type === "brain_event_observed") {
      const brainEvent = event.event;
      if (brainEvent.type === "tool_call_finished" && brainEvent.isError) {
        signals.push({
          signal_type: "tool_failure",
          severity: "warning",
          summary: `Tool ${brainEvent.toolName} failed during wake.`,
          wake_id: event.wakeId,
        });
      } else if (
        brainEvent.type === "provider_status" &&
        brainEvent.level !== "info"
      ) {
        signals.push({
          signal_type: "provider_status",
          severity: brainEvent.level === "error" ? "error" : "warning",
          summary: brainEvent.message,
          wake_id: event.wakeId,
        });
      }
    } else if (event.type === "external_event_injected") {
      const correction = correctionText(event.event.payload);
      if (correction !== undefined) {
        signals.push({
          signal_type: "user_correction",
          severity: "info",
          summary: correction,
          ref_id: event.event.source,
        });
      }
    }
  }
  const completion = optionalTrimmed(completionSummary);
  if (completion !== undefined) {
    signals.push({
      signal_type: "completion",
      severity: "info",
      summary: truncate(completion, 320),
    });
  }
  return signals;
}

function correctionText(
  payload: Extract<
    CoreEvent,
    { type: "external_event_injected" }
  >["event"]["payload"],
): string | undefined {
  const text =
    payload.type === "human_message" || payload.type === "channel_message"
      ? payload.text
      : undefined;
  if (!text) return undefined;
  const normalized = text.toLowerCase();
  if (!CORRECTION_HINTS.some((hint) => normalized.includes(hint))) {
    return undefined;
  }
  return truncate(text.trim(), 320);
}

function buildSummaryText(input: {
  source: string;
  wakeId: string;
  text: string;
  toolCalls: readonly SessionActivityToolCallDigest[];
  signals: readonly SessionActivitySignalDigest[];
  completionSummary: string | undefined;
  maxSummaryChars: number;
}): string {
  const completionSummary = optionalTrimmed(input.completionSummary);
  const lines = [
    `Wake ${input.wakeId} from ${input.source}.`,
    input.text ? `Assistant text: ${input.text}` : undefined,
    input.toolCalls.length > 0
      ? `Tool calls: ${input.toolCalls
          .map((call) => `${call.tool_name}:${call.status}`)
          .join(", ")}.`
      : undefined,
    input.signals.length > 0
      ? `Signals: ${input.signals
          .map((signal) => `${signal.signal_type}:${signal.summary}`)
          .join(" | ")}`
      : undefined,
    completionSummary === undefined
      ? undefined
      : `Completion: ${completionSummary}`,
  ].filter((line): line is string => line !== undefined && line.trim() !== "");
  return truncate(lines.join("\n"), input.maxSummaryChars);
}

function optionalTrimmed(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function positiveLimit(input: number | undefined, fallback: number): number {
  return typeof input === "number" && Number.isInteger(input) && input > 0
    ? input
    : fallback;
}

function truncate(value: string, maxChars: number): string {
  if (value.length <= maxChars) return value;
  return `${value.slice(0, Math.max(0, maxChars - 3))}...`;
}
