import { createHash } from "node:crypto";
import type { ProfileId } from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  type AgentActivityObservationEvent,
  type AgentActivityObservationSink,
  type AgentObservationIdentity,
  workActivity,
} from "./agent-activity-observation.js";
import type { LoadedSkill } from "./profile-loading.js";
import type {
  ToolContextDiagnosticsIssue,
  ToolContextDiagnosticsReport,
} from "./tool-context-diagnostics.js";

export type BackgroundReviewType = "memory" | "skills" | "combined";
export type BackgroundReviewSeverity = "info" | "warning" | "error";
export type BackgroundReviewCandidateKind =
  | "diagnostics"
  | "dense_profile_memory"
  | "skill"
  | "role_assembly"
  | "llm_review";

export interface BackgroundReviewDenseMemoryRecord {
  profileId: ProfileId | string;
  key: string;
  content?: string;
  revision?: number;
  updatedAt?: string;
  metadata?: unknown;
}

export interface BackgroundReviewPayload {
  schemaVersion?: 1;
  reviewType: BackgroundReviewType;
  profileId: ProfileId | string;
  triggerSource?: string;
  maxFindings?: number;
  maxCandidates?: number;
  llmReviewEnabled?: boolean;
  dryRun?: boolean;
  reason?: string;
}

export interface BackgroundReviewRunnerInput {
  runId: string;
  now: string;
  payload: BackgroundReviewPayload;
  diagnostics?: ToolContextDiagnosticsReport;
  skills?: readonly LoadedSkill[];
  denseProfileMemory?: readonly BackgroundReviewDenseMemoryRecord[];
  observation?: {
    identity: AgentObservationIdentity;
    sink?: AgentActivityObservationSink;
    required?: boolean;
  };
}

export interface BackgroundReviewSourceRef {
  kind:
    | "diagnostics"
    | "skill"
    | "dense_profile_memory"
    | "role_assembly"
    | "scheduler_run";
  ref: string;
  hash?: string;
}

export interface BackgroundReviewFinding {
  findingId: string;
  fingerprint: string;
  reviewType: BackgroundReviewType;
  sourceRefs: readonly BackgroundReviewSourceRef[];
  severity: BackgroundReviewSeverity;
  confidence: number;
  summary: string;
  proposedAction: string;
  candidateKind: BackgroundReviewCandidateKind;
}

export interface BackgroundReviewResultRef {
  kind: "review_finding_batch";
  runId: string;
  findingIds: readonly string[];
  reportId: string;
}

export interface BackgroundReviewResult {
  runId: string;
  reviewType: BackgroundReviewType;
  triggerSource: string;
  profileId: ProfileId | string;
  candidateCount: number;
  findingCount: number;
  skippedCount: number;
  findingFingerprints: readonly string[];
  findings: readonly BackgroundReviewFinding[];
  resultRef: BackgroundReviewResultRef;
  observation?: AgentActivityObservationEvent;
  skippedReasons: readonly string[];
  dryRun: boolean;
  startedAt: string;
  finishedAt: string;
}

const DEFAULT_MAX_FINDINGS = 25;
const DEFAULT_MAX_CANDIDATES = 100;
const MAX_SKILL_BODY_CHARS = 32_000;
const MAX_MEMORY_CONTENT_CHARS = 8_000;

export async function runBackgroundMemorySkillReview(
  input: BackgroundReviewRunnerInput,
): Promise<BackgroundReviewResult> {
  const maxFindings = clampPositive(
    input.payload.maxFindings,
    DEFAULT_MAX_FINDINGS,
    100,
  );
  const maxCandidates = clampPositive(
    input.payload.maxCandidates,
    DEFAULT_MAX_CANDIDATES,
    500,
  );
  const skippedReasons: string[] = [];
  if (input.payload.llmReviewEnabled) {
    skippedReasons.push("llm_review_requires_provider_path");
  }

  const candidates = [
    ...diagnosticCandidates(input),
    ...skillCandidates(input),
    ...denseMemoryCandidates(input),
    ...roleAssemblyCandidates(input),
  ].slice(0, maxCandidates);
  const findings = candidates
    .map((candidate, index) => toFinding(input, candidate, index))
    .slice(0, maxFindings);
  const skippedCount = candidates.length - findings.length;
  const resultRef: BackgroundReviewResultRef = {
    kind: "review_finding_batch",
    runId: input.runId,
    findingIds: findings.map((finding) => finding.findingId),
    reportId: `review:${input.runId}:${fingerprint(input.runId, findings.length.toString()).slice(0, 12)}`,
  };
  const observation = await publishObservation(input, findings, resultRef);

  return {
    runId: input.runId,
    reviewType: input.payload.reviewType,
    triggerSource: input.payload.triggerSource ?? "manual",
    profileId: input.payload.profileId,
    candidateCount: candidates.length,
    findingCount: findings.length,
    skippedCount,
    findingFingerprints: findings.map((finding) => finding.fingerprint),
    findings,
    resultRef,
    observation,
    skippedReasons,
    dryRun: input.payload.dryRun ?? true,
    startedAt: input.now,
    finishedAt: input.now,
  };
}

interface Candidate {
  severity: BackgroundReviewSeverity;
  confidence: number;
  summary: string;
  proposedAction: string;
  candidateKind: BackgroundReviewCandidateKind;
  sourceRefs: readonly BackgroundReviewSourceRef[];
}

function diagnosticCandidates(input: BackgroundReviewRunnerInput): Candidate[] {
  if (!input.diagnostics) return [];
  return input.diagnostics.issues.map((issue) => ({
    severity: severityFromDiagnostics(issue),
    confidence: 0.9,
    summary: issue.message,
    proposedAction: proposedActionForIssue(issue),
    candidateKind: "diagnostics",
    sourceRefs: [
      {
        kind: "diagnostics",
        ref: `${input.diagnostics?.catalogId ?? "diagnostics"}:${issue.code}`,
      },
    ],
  }));
}

function skillCandidates(input: BackgroundReviewRunnerInput): Candidate[] {
  if (input.payload.reviewType === "memory") return [];
  const skills = input.skills ?? [];
  const duplicateTitles = duplicateValues(
    skills.map((skill) => normalizeText(skill.title ?? "")),
  );
  return skills.flatMap((skill) => {
    const refs = [skillRef(skill)];
    return [
      !skill.summary
        ? candidate(
            "warning",
            "Skill has no summary.",
            "Add a concise summary.",
            "skill",
            refs,
          )
        : undefined,
      skill.bodyMarkdown.length > MAX_SKILL_BODY_CHARS
        ? candidate(
            "warning",
            `Skill body is ${skill.bodyMarkdown.length} chars.`,
            "Split oversized skill content or move reference material elsewhere.",
            "skill",
            refs,
          )
        : undefined,
      todoLike(skill.bodyMarkdown)
        ? candidate(
            "info",
            "Skill body appears to contain task-progress language.",
            "Move temporary project progress into Den tasks/docs before it becomes stale.",
            "skill",
            refs,
          )
        : undefined,
      skill.title && duplicateTitles.has(normalizeText(skill.title))
        ? candidate(
            "info",
            `Skill title '${skill.title}' is duplicated.`,
            "Clarify skill titles so profile assembly stays explainable.",
            "skill",
            refs,
          )
        : undefined,
    ].filter((item): item is Candidate => Boolean(item));
  });
}

function denseMemoryCandidates(
  input: BackgroundReviewRunnerInput,
): Candidate[] {
  if (input.payload.reviewType === "skills") return [];
  const records = input.denseProfileMemory ?? [];
  const duplicateKeys = duplicateValues(
    records.map((record) => normalizeText(record.key)),
  );
  return records.flatMap((record) => {
    const refs = [memoryRef(record)];
    const content = record.content ?? "";
    return [
      content.length > MAX_MEMORY_CONTENT_CHARS
        ? candidate(
            "warning",
            `Dense profile memory '${record.key}' is ${content.length} chars.`,
            "Condense stable memory and move long references to source docs.",
            "dense_profile_memory",
            refs,
          )
        : undefined,
      todoLike(content)
        ? candidate(
            "warning",
            `Dense profile memory '${record.key}' appears to contain task progress.`,
            "Move task progress or temporary todos out of dense profile memory.",
            "dense_profile_memory",
            refs,
          )
        : undefined,
      duplicateKeys.has(normalizeText(record.key))
        ? candidate(
            "info",
            `Dense profile memory key '${record.key}' is duplicated.`,
            "Merge duplicate stable memories or rename keys for clarity.",
            "dense_profile_memory",
            refs,
          )
        : undefined,
    ].filter((item): item is Candidate => Boolean(item));
  });
}

function roleAssemblyCandidates(
  input: BackgroundReviewRunnerInput,
): Candidate[] {
  if (!input.diagnostics || input.payload.reviewType === "memory") return [];
  const skillChars = input.diagnostics.context.skills.reduce(
    (sum, skill) => sum + skill.bodyChars,
    0,
  );
  if (skillChars <= 64_000) return [];
  return [
    candidate(
      "warning",
      `Selected skill bodies total ${skillChars} chars.`,
      "Review profile skill selection or disable full skill bodies for broad profiles.",
      "role_assembly",
      [{ kind: "role_assembly", ref: input.diagnostics.session.sessionId }],
    ),
  ];
}

function toFinding(
  input: BackgroundReviewRunnerInput,
  candidate: Candidate,
  index: number,
): BackgroundReviewFinding {
  const rawFingerprint = fingerprint(
    input.payload.profileId.toString(),
    input.payload.reviewType,
    candidate.candidateKind,
    candidate.summary,
    candidate.sourceRefs.map((ref) => ref.ref).join("|"),
  );
  const findingId = `review:${input.runId}:${index + 1}:${rawFingerprint.slice(0, 12)}`;
  return {
    findingId,
    fingerprint: rawFingerprint,
    reviewType: input.payload.reviewType,
    sourceRefs: candidate.sourceRefs,
    severity: candidate.severity,
    confidence: candidate.confidence,
    summary: candidate.summary,
    proposedAction: candidate.proposedAction,
    candidateKind: candidate.candidateKind,
  };
}

async function publishObservation(
  input: BackgroundReviewRunnerInput,
  findings: readonly BackgroundReviewFinding[],
  resultRef: BackgroundReviewResultRef,
): Promise<AgentActivityObservationEvent | undefined> {
  if (!input.observation) return undefined;
  const producer = new AgentActivityObservationProducer({
    sink: input.observation.sink,
    required: input.observation.required,
  });
  const severity = findings.some((finding) => finding.severity === "error")
    ? "error"
    : findings.some((finding) => finding.severity === "warning")
      ? "warning"
      : "success";
  const result = await producer.publish(
    workActivity({
      eventType: findings.length > 0 ? "work_checkpoint" : "work_completed",
      identity: input.observation.identity,
      summary: `background ${input.payload.reviewType} review produced ${findings.length} findings`,
      workRef: { run_id: input.runId },
      resultRef: { artifact_path: resultRef.reportId },
      reasonCode: "background_review",
      visibility: severity === "success" ? "debug" : "agent",
    }),
  );
  return result.event;
}

function candidate(
  severity: BackgroundReviewSeverity,
  summary: string,
  proposedAction: string,
  candidateKind: BackgroundReviewCandidateKind,
  sourceRefs: readonly BackgroundReviewSourceRef[],
): Candidate {
  return {
    severity,
    confidence: severity === "info" ? 0.7 : 0.85,
    summary,
    proposedAction,
    candidateKind,
    sourceRefs,
  };
}

function skillRef(skill: LoadedSkill): BackgroundReviewSourceRef {
  return {
    kind: "skill",
    ref: skill.slug,
    hash: fingerprint(skill.sourcePath, skill.bodyMarkdown).slice(0, 16),
  };
}

function memoryRef(
  record: BackgroundReviewDenseMemoryRecord,
): BackgroundReviewSourceRef {
  return {
    kind: "dense_profile_memory",
    ref: `${record.profileId}:${record.key}:${record.revision ?? "unknown"}`,
    hash: fingerprint(record.key, record.content ?? "").slice(0, 16),
  };
}

function severityFromDiagnostics(
  issue: ToolContextDiagnosticsIssue,
): BackgroundReviewSeverity {
  return issue.severity === "blocked" ? "error" : issue.severity;
}

function proposedActionForIssue(issue: ToolContextDiagnosticsIssue): string {
  switch (issue.code) {
    case "den_memory_unavailable":
      return "Check Den memory client configuration before enabling memory review writes.";
    case "skill_root_unavailable":
      return "Repair skill root diagnostics or remove missing skills from the profile.";
    case "dense_profile_memory_unavailable":
      return "Verify dense profile memory bridge availability for this profile.";
    default:
      return "Inspect diagnostics and resolve the degraded surface.";
  }
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
