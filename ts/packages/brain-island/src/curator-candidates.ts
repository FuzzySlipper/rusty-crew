import { createHash } from "node:crypto";
import type { ProfileId } from "@rusty-crew/contracts";
import type {
  BackgroundReviewDenseMemoryRecord,
  BackgroundReviewSeverity,
} from "./background-memory-skill-review.js";
import type { LoadedSkill } from "./profile-loading.js";

export type CuratorCandidateKind =
  | "skill_patch"
  | "skill_archive"
  | "skill_create"
  | "sidecar_write"
  | "dense_memory_prune"
  | "dense_memory_merge"
  | "diagnostics_only";

export type CuratorCandidateStatus = "proposed";

export interface CuratorCandidateSourceRef {
  kind: "skill" | "dense_profile_memory" | "profile" | "diagnostics";
  ref: string;
  hash?: string;
}

export interface CuratorCandidate {
  candidateId: string;
  batchId: string;
  kind: CuratorCandidateKind;
  sourceRefs: readonly CuratorCandidateSourceRef[];
  targetRef: string;
  summary: string;
  severity: BackgroundReviewSeverity;
  confidence: number;
  proposedAction: string;
  previewSummary: string;
  fingerprint: string;
  status: CuratorCandidateStatus;
  rollbackSupported: boolean;
}

export interface CuratorCandidateBatch {
  batchId: string;
  scopeType: "profile" | "skills_root" | "project" | "session" | "runtime";
  scopeId: string;
  profileId?: ProfileId | string;
  generatedAt: string;
  dryRun: boolean;
  candidateCount: number;
  reportId: string;
  candidates: readonly CuratorCandidate[];
  skippedReasons: readonly string[];
}

export interface CuratorCandidateDiscoveryInput {
  batchId: string;
  now: string;
  scopeType: CuratorCandidateBatch["scopeType"];
  scopeId: string;
  profileId?: ProfileId | string;
  skills?: readonly LoadedSkill[];
  expectedSkillSlugs?: readonly string[];
  denseProfileMemory?: readonly BackgroundReviewDenseMemoryRecord[];
  maxCandidates?: number;
  dryRun?: boolean;
}

const DEFAULT_MAX_CANDIDATES = 100;
const MAX_SKILL_BODY_CHARS = 32_000;
const MAX_MEMORY_CONTENT_CHARS = 8_000;

export function discoverCuratorCandidates(
  input: CuratorCandidateDiscoveryInput,
): CuratorCandidateBatch {
  const maxCandidates = clampPositive(
    input.maxCandidates,
    DEFAULT_MAX_CANDIDATES,
    500,
  );
  const skippedReasons: string[] = [];
  const candidates = [
    ...skillCandidates(input),
    ...skillCoverageCandidates(input),
    ...denseMemoryCandidates(input),
  ].slice(0, maxCandidates);
  if (candidates.length === maxCandidates) {
    skippedReasons.push("candidate_limit_reached");
  }
  const reportId = `curator-report:${input.batchId}:${fingerprint(
    input.batchId,
    candidates.length.toString(),
  ).slice(0, 12)}`;
  return {
    batchId: input.batchId,
    scopeType: input.scopeType,
    scopeId: input.scopeId,
    profileId: input.profileId,
    generatedAt: input.now,
    dryRun: input.dryRun ?? true,
    candidateCount: candidates.length,
    reportId,
    candidates,
    skippedReasons,
  };
}

export function renderCuratorCandidateReport(
  batch: CuratorCandidateBatch,
): string {
  const bySeverity = countBy(batch.candidates, (candidate) =>
    candidate.severity.toString(),
  );
  const byKind = countBy(batch.candidates, (candidate) => candidate.kind);
  const lines = [
    "# Curator Candidate Report",
    "",
    `Batch: ${batch.batchId}`,
    `Scope: ${batch.scopeType}:${batch.scopeId}`,
    batch.profileId ? `Profile: ${batch.profileId}` : undefined,
    `Generated: ${batch.generatedAt}`,
    `Dry run: ${batch.dryRun ? "yes" : "no"}`,
    `Candidates: ${batch.candidateCount}`,
    "",
    "## Counts",
    "",
    ...Object.entries(bySeverity).map(
      ([severity, count]) => `- ${severity}: ${count}`,
    ),
    ...Object.entries(byKind).map(([kind, count]) => `- ${kind}: ${count}`),
    "",
    "## Candidates",
    "",
    ...batch.candidates.map(
      (candidate) =>
        `- ${candidate.candidateId} [${candidate.severity}/${candidate.kind}] ${candidate.summary}`,
    ),
    ...(batch.skippedReasons.length > 0
      ? [
          "",
          "## Skipped",
          "",
          ...batch.skippedReasons.map((item) => `- ${item}`),
        ]
      : []),
  ].filter((line): line is string => line !== undefined);
  return lines.join("\n");
}

function skillCandidates(
  input: CuratorCandidateDiscoveryInput,
): CuratorCandidate[] {
  const skills = input.skills ?? [];
  const duplicateTitles = duplicateValues(
    skills.map((skill) => normalizeText(skill.title ?? "")),
  );
  return skills.flatMap((skill) => {
    const refs = [skillRef(skill)];
    return [
      !skill.summary
        ? candidate(input, {
            kind: "skill_patch",
            targetRef: `skill:${skill.slug}`,
            sourceRefs: refs,
            severity: "warning",
            confidence: 0.85,
            summary: `Skill '${skill.slug}' has no summary.`,
            proposedAction: "Add a concise skill summary.",
            previewSummary: "Would patch skill frontmatter with a summary.",
            rollbackSupported: true,
          })
        : undefined,
      skill.bodyMarkdown.length > MAX_SKILL_BODY_CHARS
        ? candidate(input, {
            kind: "skill_patch",
            targetRef: `skill:${skill.slug}`,
            sourceRefs: refs,
            severity: "warning",
            confidence: 0.8,
            summary: `Skill '${skill.slug}' is ${skill.bodyMarkdown.length} chars.`,
            proposedAction:
              "Split oversized content into references or a narrower skill.",
            previewSummary: "Would produce a split/condense proposal.",
            rollbackSupported: true,
          })
        : undefined,
      todoLike(skill.bodyMarkdown)
        ? candidate(input, {
            kind: "diagnostics_only",
            targetRef: `skill:${skill.slug}`,
            sourceRefs: refs,
            severity: "info",
            confidence: 0.7,
            summary: `Skill '${skill.slug}' appears to contain temporary project progress.`,
            proposedAction:
              "Move temporary progress into Den tasks/docs before it becomes stale.",
            previewSummary: "Would report stale-progress language.",
            rollbackSupported: false,
          })
        : undefined,
      skill.title && duplicateTitles.has(normalizeText(skill.title))
        ? candidate(input, {
            kind: "skill_patch",
            targetRef: `skill:${skill.slug}`,
            sourceRefs: refs,
            severity: "info",
            confidence: 0.7,
            summary: `Skill title '${skill.title}' is duplicated.`,
            proposedAction:
              "Clarify duplicate skill titles so profile assembly stays explainable.",
            previewSummary: "Would propose a clearer title.",
            rollbackSupported: true,
          })
        : undefined,
    ].filter((item): item is CuratorCandidate => Boolean(item));
  });
}

function skillCoverageCandidates(
  input: CuratorCandidateDiscoveryInput,
): CuratorCandidate[] {
  const expected = input.expectedSkillSlugs ?? [];
  if (expected.length === 0) return [];
  const existing = new Set((input.skills ?? []).map((skill) => skill.slug));
  return expected
    .filter((slug) => !existing.has(slug))
    .map((slug) =>
      candidate(input, {
        kind: "skill_create",
        targetRef: `skill:${slug}`,
        sourceRefs: [
          { kind: "profile", ref: `${input.profileId ?? input.scopeId}` },
        ],
        severity: "warning",
        confidence: 0.9,
        summary: `Profile expects missing skill '${slug}'.`,
        proposedAction:
          "Create the missing skill or remove it from the profile selection.",
        previewSummary: "Would propose creating a placeholder skill draft.",
        rollbackSupported: true,
      }),
    );
}

function denseMemoryCandidates(
  input: CuratorCandidateDiscoveryInput,
): CuratorCandidate[] {
  const records = input.denseProfileMemory ?? [];
  const duplicateKeys = duplicateValues(
    records.map((record) => normalizeText(record.key)),
  );
  return records.flatMap((record) => {
    const refs = [memoryRef(record)];
    const content = record.content ?? "";
    return [
      content.length > MAX_MEMORY_CONTENT_CHARS
        ? candidate(input, {
            kind: "dense_memory_prune",
            targetRef: `dense_profile_memory:${record.key}`,
            sourceRefs: refs,
            severity: "warning",
            confidence: 0.8,
            summary: `Dense profile memory '${record.key}' is ${content.length} chars.`,
            proposedAction:
              "Condense stable memory and move long references to source docs.",
            previewSummary: "Would propose a shorter durable memory.",
            rollbackSupported: false,
          })
        : undefined,
      todoLike(content)
        ? candidate(input, {
            kind: "dense_memory_prune",
            targetRef: `dense_profile_memory:${record.key}`,
            sourceRefs: refs,
            severity: "warning",
            confidence: 0.75,
            summary: `Dense profile memory '${record.key}' appears to contain task progress.`,
            proposedAction:
              "Move task progress or temporary todos out of dense profile memory.",
            previewSummary: "Would propose pruning temporary content.",
            rollbackSupported: false,
          })
        : undefined,
      duplicateKeys.has(normalizeText(record.key))
        ? candidate(input, {
            kind: "dense_memory_merge",
            targetRef: `dense_profile_memory:${record.key}`,
            sourceRefs: refs,
            severity: "info",
            confidence: 0.7,
            summary: `Dense profile memory key '${record.key}' is duplicated.`,
            proposedAction: "Merge duplicate stable memories or rename keys.",
            previewSummary: "Would propose a merge plan.",
            rollbackSupported: false,
          })
        : undefined,
    ].filter((item): item is CuratorCandidate => Boolean(item));
  });
}

function candidate(
  input: CuratorCandidateDiscoveryInput,
  value: Omit<
    CuratorCandidate,
    "candidateId" | "batchId" | "fingerprint" | "status"
  >,
): CuratorCandidate {
  const hash = fingerprint(
    input.batchId,
    value.kind,
    value.targetRef,
    value.summary,
    value.sourceRefs
      .map((ref) => `${ref.kind}:${ref.ref}:${ref.hash ?? ""}`)
      .join("|"),
  );
  return {
    ...value,
    batchId: input.batchId,
    candidateId: `curator:${input.batchId}:${hash.slice(0, 12)}`,
    fingerprint: hash,
    status: "proposed",
  };
}

function skillRef(skill: LoadedSkill): CuratorCandidateSourceRef {
  return {
    kind: "skill",
    ref: skill.slug,
    hash: fingerprint(skill.sourcePath, skill.bodyMarkdown).slice(0, 16),
  };
}

function memoryRef(
  record: BackgroundReviewDenseMemoryRecord,
): CuratorCandidateSourceRef {
  return {
    kind: "dense_profile_memory",
    ref: `${record.profileId}:${record.key}:${record.revision ?? "unknown"}`,
    hash: fingerprint(record.key, record.content ?? "").slice(0, 16),
  };
}

function duplicateValues(values: readonly string[]): Set<string> {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const value of values) {
    if (!value) continue;
    if (seen.has(value)) duplicates.add(value);
    seen.add(value);
  }
  return duplicates;
}

function todoLike(value: string): boolean {
  return /\b(todo|blocked|in progress|follow[- ]?up|temporary|stub)\b/i.test(
    value,
  );
}

function normalizeText(value: string): string {
  return value.trim().toLowerCase();
}

function countBy<T>(
  items: readonly T[],
  key: (item: T) => string,
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const item of items) {
    const value = key(item);
    counts[value] = (counts[value] ?? 0) + 1;
  }
  return counts;
}

function fingerprint(...parts: readonly string[]): string {
  const hash = createHash("sha256");
  for (const part of parts) {
    hash.update(part);
    hash.update("\0");
  }
  return hash.digest("hex");
}

function clampPositive(
  value: number | undefined,
  fallback: number,
  max: number,
): number {
  if (value === undefined || !Number.isFinite(value) || value < 1) {
    return fallback;
  }
  return Math.min(Math.floor(value), max);
}
