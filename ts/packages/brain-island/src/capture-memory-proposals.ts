import { createHash } from "node:crypto";
import type {
  MemoryEvidenceKind,
  MemoryEvidenceRef,
  MemoryGovernanceMode,
  MemoryOperation,
  MemoryProposalEnvelope,
  MemoryRecordShapeRef,
  MemoryScope,
  MemorySpaceId,
  ProfileId,
} from "@rusty-crew/contracts";

export type CaptureTargetSpaceId =
  | "profile_dense"
  | "session_memory"
  | "roleplay_lore";

export interface CaptureProducerEvidenceRef {
  eventType?: string;
  wakeId?: string;
  refId?: string;
  summary?: string;
  evidenceType?: MemoryEvidenceKind;
}

export interface TypedCaptureMemoryProposal {
  id?: string;
  summary: string;
  space_id: CaptureTargetSpaceId;
  operation: Extract<
    MemoryOperation,
    "add" | "replace" | "merge" | "supersede" | "remove" | "archive"
  >;
  scope: MemoryScope;
  shape: MemoryRecordShapeRef;
  content: unknown;
  evidence_refs: readonly CaptureProducerEvidenceRef[];
  confidence: number;
  durability_rationale: string;
  governance_policy?: MemoryGovernanceMode;
  dedupe_key?: string;
}

export type LegacyDenseMemoryCaptureKind =
  | "dense_memory_add"
  | "dense_memory_replace"
  | "dense_memory_remove";

export interface LegacyDenseMemoryCaptureProposal {
  id?: string;
  kind: LegacyDenseMemoryCaptureKind;
  summary: string;
  confidence: number;
  durabilityRationale: string;
  memoryKey?: string;
  memoryContent?: string;
  replacesKey?: string;
  expectedRevision?: number;
  evidenceRefs: readonly CaptureProducerEvidenceRef[];
}

export interface CaptureProducerOutput {
  runId: string;
  profileId: ProfileId | string;
  proposals: readonly (
    | TypedCaptureMemoryProposal
    | LegacyDenseMemoryCaptureProposal
  )[];
  skippedReasons: readonly string[];
}

export function isLegacyDenseMemoryCaptureProposal(
  proposal: TypedCaptureMemoryProposal | LegacyDenseMemoryCaptureProposal,
): proposal is LegacyDenseMemoryCaptureProposal {
  return "kind" in proposal && proposal.kind.startsWith("dense_memory_");
}

export function captureProposalToMemoryProposal(input: {
  runId: string;
  profileId: ProfileId | string;
  proposal: TypedCaptureMemoryProposal | LegacyDenseMemoryCaptureProposal;
}): MemoryProposalEnvelope {
  const { runId, profileId, proposal } = input;
  if (isLegacyDenseMemoryCaptureProposal(proposal)) {
    return legacyDenseCaptureProposalToMemoryProposal({
      runId,
      profileId,
      proposal,
    });
  }
  return typedCaptureProposalToMemoryProposal({ runId, profileId, proposal });
}

export function typedCaptureProposalToMemoryProposal(input: {
  runId: string;
  profileId: ProfileId | string;
  proposal: TypedCaptureMemoryProposal;
}): MemoryProposalEnvelope {
  const { proposal } = input;
  return {
    proposal_id: proposalId(input.runId, proposal.id, proposal.summary),
    space_id: proposal.space_id as MemorySpaceId,
    operation: proposal.operation,
    scope: proposal.scope,
    shape: proposal.shape,
    content: proposal.content,
    evidence_refs: evidenceRefs(input.runId, proposal.evidence_refs),
    confidence: boundedConfidence(proposal.confidence),
    durability_rationale: proposal.durability_rationale,
    governance_mode: proposal.governance_policy ?? "curator_route",
    source: "capture_producer",
    dedupe_key:
      proposal.dedupe_key ??
      dedupeKey(
        proposal.space_id,
        proposal.operation,
        proposal.scope.scope_type,
        proposal.scope.scope_id,
        stableJson(proposal.content),
      ),
  };
}

export function legacyDenseCaptureProposalToMemoryProposal(input: {
  runId: string;
  profileId: ProfileId | string;
  proposal: LegacyDenseMemoryCaptureProposal;
}): MemoryProposalEnvelope {
  const { proposal } = input;
  const operation = legacyDenseOperation(proposal.kind);
  const key = requireValue(
    proposal.memoryKey ?? proposal.replacesKey,
    `${proposal.kind} requires memoryKey or replacesKey`,
  );
  return {
    proposal_id: proposalId(input.runId, proposal.id, proposal.summary),
    space_id: "profile_dense" as MemorySpaceId,
    operation,
    scope: {
      scope_type: "profile",
      scope_id: input.profileId.toString(),
    },
    shape: {
      shape_id: "profile_dense_item" as never,
      version: 1,
    },
    content: {
      key,
      ...(operation !== "remove"
        ? {
            content: requireValue(
              proposal.memoryContent,
              `${proposal.kind} requires memoryContent`,
            ),
          }
        : {}),
      ...(proposal.replacesKey ? { replaces_key: proposal.replacesKey } : {}),
      ...(proposal.expectedRevision !== undefined
        ? { expected_revision: proposal.expectedRevision }
        : {}),
      metadata_json: {
        capture_summary: proposal.summary,
        legacy_capture_kind: proposal.kind,
      },
    },
    evidence_refs: evidenceRefs(input.runId, proposal.evidenceRefs),
    confidence: boundedConfidence(proposal.confidence),
    durability_rationale: proposal.durabilityRationale,
    governance_mode: "curator_route",
    source: "capture_producer",
    dedupe_key: dedupeKey(
      "profile_dense",
      operation,
      "profile",
      input.profileId.toString(),
      key,
    ),
  };
}

function legacyDenseOperation(
  kind: LegacyDenseMemoryCaptureKind,
): Extract<MemoryOperation, "add" | "replace" | "remove"> {
  switch (kind) {
    case "dense_memory_add":
      return "add";
    case "dense_memory_replace":
      return "replace";
    case "dense_memory_remove":
      return "remove";
  }
}

function evidenceRefs(
  runId: string,
  refs: readonly CaptureProducerEvidenceRef[],
): MemoryEvidenceRef[] {
  const mapped = refs.map((ref) => ({
    evidence_type:
      ref.evidenceType ?? evidenceTypeFromEvent(ref.eventType) ?? "event",
    ref_id: ref.refId ?? ref.wakeId ?? `${runId}:capture_producer`,
    ...(ref.summary ? { label: ref.summary } : {}),
  }));
  if (!mapped.some((ref) => ref.evidence_type === "wake")) {
    mapped.unshift({
      evidence_type: "wake",
      ref_id: refs.find((ref) => ref.wakeId)?.wakeId ?? `${runId}:wake`,
      label: "capture producer wake evidence",
    });
  }
  return mapped;
}

function evidenceTypeFromEvent(
  eventType: string | undefined,
): MemoryEvidenceKind | undefined {
  if (!eventType) return undefined;
  if (eventType.includes("correction")) return "user_correction";
  if (eventType.includes("tool")) return "tool_call";
  if (eventType.includes("transcript")) return "transcript";
  if (eventType.includes("wake")) return "wake";
  return "event";
}

function proposalId(
  runId: string,
  explicitId: string | undefined,
  summary: string,
): string {
  const raw = explicitId ?? `${runId}:${summary}`;
  const normalized = raw
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_+/g, "_");
  if (/^[a-z][a-z0-9_]*[a-z0-9]$/.test(normalized) && normalized.length <= 64) {
    return normalized;
  }
  return `cap_${hash(raw).slice(0, 24)}`;
}

function dedupeKey(...parts: readonly string[]): string {
  return parts
    .map((part) => part.trim().toLowerCase().replace(/\s+/g, " "))
    .join(":");
}

function boundedConfidence(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(1, value));
}

function requireValue(value: string | undefined, message: string): string {
  if (!value || value.trim().length === 0) {
    throw new Error(message);
  }
  return value;
}

function stableJson(value: unknown): string {
  return JSON.stringify(sortJson(value));
}

function sortJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortJson);
  if (typeof value !== "object" || value === null) return value;
  return Object.fromEntries(
    Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, nested]) => [key, sortJson(nested)]),
  );
}

function hash(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
