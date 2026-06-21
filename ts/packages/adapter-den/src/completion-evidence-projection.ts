import type {
  ChannelBindingRecord,
  ChannelSeverity,
  CompletionPacket,
  CoreEvent,
  NormalizedChannelActivityProjection,
  ResultReference,
  WorkReference,
} from "@rusty-crew/contracts";
import {
  dispatchChannelActivityProjection,
  type ChannelProjectionDispatchResult,
  type ChannelProjectionSink,
  type ChannelProjectionOptions,
} from "./channel-projection.js";
import { providerRefsFromBinding } from "./channel-presence.js";

const DEFAULT_MAX_EVIDENCE_SUMMARY_CHARS = 480;

export interface CompletionEvidenceProjectionInput extends ChannelProjectionOptions {
  binding: ChannelBindingRecord;
  event: CoreEvent;
  workRefs?: readonly WorkReference[];
  resultRefs?: readonly ResultReference[];
  summary?: string;
  severity?: ChannelSeverity;
}

export interface CompletionEvidenceProjectionDispatchResult {
  activity: NormalizedChannelActivityProjection;
  dispatch: ChannelProjectionDispatchResult;
}

export function projectCompletionEvidenceToChannelActivity(
  input: CompletionEvidenceProjectionInput,
): NormalizedChannelActivityProjection {
  const workRefs = normalizeWorkRefs(input);
  const resultRefs = normalizeResultRefs(input);
  return {
    kind: "channel_activity_projection.v1",
    adapterId: input.binding.adapterId,
    bindingId: input.binding.bindingId,
    runtime: {
      agentId: input.binding.agentId,
      instanceId: input.binding.instanceId,
      sessionId: input.binding.sessionId,
      profileId: input.binding.profileId,
    },
    providerRefs: providerRefsFromBinding(input.binding),
    eventType: input.event.type,
    summary: boundedEvidenceSummary(
      input.summary ?? completionEvidenceSummary(input.event),
      input.maxSummaryChars,
    ),
    severity: input.severity ?? completionEvidenceSeverity(input.event),
    workRef: workRefs[0] ? formatWorkReference(workRefs[0]) : undefined,
    resultRef: resultRefs[0] ? formatResultReference(resultRefs[0]) : undefined,
    workRefs,
    resultRefs,
    createdAt: input.now ?? new Date().toISOString(),
  };
}

export async function dispatchCompletionEvidenceProjection(
  sink: ChannelProjectionSink,
  input: CompletionEvidenceProjectionInput,
): Promise<CompletionEvidenceProjectionDispatchResult> {
  const activity = projectCompletionEvidenceToChannelActivity(input);
  const dispatch = await dispatchChannelActivityProjection(sink, activity);
  return { activity, dispatch };
}

export function completionPacketResultRef(
  packet: CompletionPacket,
): ResultReference {
  return {
    kind: "result_ref.v1",
    sourceDomain: "runtime",
    refKind: "completion_packet",
    id: `${packet.sessionId}:${packet.status}`,
    label: `completion ${packet.status} for ${packet.sessionId}`,
  };
}

export function runtimeSessionWorkRef(sessionId: string): WorkReference {
  return {
    kind: "work_ref.v1",
    sourceDomain: "runtime",
    refKind: "session",
    id: sessionId,
  };
}

function normalizeWorkRefs(
  input: CompletionEvidenceProjectionInput,
): WorkReference[] {
  const refs = [...(input.workRefs ?? [])];
  switch (input.event.type) {
    case "completion_packet_delivered":
      refs.push(runtimeSessionWorkRef(input.event.packet.sessionId));
      break;
    case "delegation_lifecycle_observed":
      if (input.event.lifecycle.runId) {
        refs.push({
          kind: "work_ref.v1",
          sourceDomain: "runtime",
          refKind: "delegation_run",
          id: input.event.lifecycle.runId,
        });
      }
      refs.push(
        runtimeSessionWorkRef(input.event.lifecycle.delegatedSessionId),
      );
      break;
    default:
      if ("sessionId" in input.event) {
        refs.push(runtimeSessionWorkRef(String(input.event.sessionId)));
      }
  }
  return dedupeReferences(refs);
}

function normalizeResultRefs(
  input: CompletionEvidenceProjectionInput,
): ResultReference[] {
  const refs = [...(input.resultRefs ?? [])];
  if (input.event.type === "completion_packet_delivered") {
    refs.push(completionPacketResultRef(input.event.packet));
  }
  if (
    input.event.type === "delegation_lifecycle_observed" &&
    input.event.lifecycle.phase !== "created" &&
    input.event.lifecycle.phase !== "wake_requested" &&
    input.event.lifecycle.phase !== "checkpoint_requested"
  ) {
    refs.push({
      kind: "result_ref.v1",
      sourceDomain: "runtime",
      refKind: "runtime_event",
      id: `delegation:${input.event.lifecycle.delegatedSessionId}:${input.event.lifecycle.phase}`,
      label: `delegation ${input.event.lifecycle.phase}`,
    });
  }
  return dedupeReferences(refs);
}

function completionEvidenceSummary(event: CoreEvent): string {
  switch (event.type) {
    case "completion_packet_delivered":
      return `Completion ${event.packet.status} for ${event.packet.sessionId}: ${event.packet.summary}`;
    case "delegation_lifecycle_observed":
      return `Delegation ${event.lifecycle.phase} for ${event.lifecycle.delegatedSessionId}`;
    default:
      return `Runtime evidence for ${event.type}`;
  }
}

function completionEvidenceSeverity(event: CoreEvent): ChannelSeverity {
  if (event.type === "completion_packet_delivered") {
    return event.packet.status === "completed"
      ? "success"
      : event.packet.status === "failed"
        ? "error"
        : "warning";
  }
  if (event.type === "delegation_lifecycle_observed") {
    return event.lifecycle.phase === "failed" ||
      event.lifecycle.phase === "blocked" ||
      event.lifecycle.phase === "timed_out"
      ? "error"
      : event.lifecycle.phase === "completed"
        ? "success"
        : "info";
  }
  return "info";
}

function boundedEvidenceSummary(
  summary: string,
  maxChars = DEFAULT_MAX_EVIDENCE_SUMMARY_CHARS,
): string {
  const trimmed = summary.trim();
  if (trimmed.length <= maxChars) return trimmed;
  const marker = "\n[truncated]";
  return `${trimmed.slice(0, Math.max(0, maxChars - marker.length))}${marker}`;
}

function formatWorkReference(ref: WorkReference): string {
  return `${ref.sourceDomain}:${ref.refKind}:${ref.id}`;
}

function formatResultReference(ref: ResultReference): string {
  return `${ref.sourceDomain}:${ref.refKind}:${ref.id}`;
}

function dedupeReferences<
  T extends { sourceDomain: string; refKind: string; id: string },
>(refs: readonly T[]): T[] {
  const seen = new Set<string>();
  const result: T[] = [];
  for (const ref of refs) {
    const key = `${ref.sourceDomain}:${ref.refKind}:${ref.id}`;
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(ref);
  }
  return result;
}
