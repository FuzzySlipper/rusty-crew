import assert from "node:assert/strict";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  buildBackgroundServiceDiagnosticsProjection,
  buildRuntimeDiagnosticsProjection,
  buildToolContextDiagnosticsReport,
  buildToolRegistryDiagnostics,
  createCuratorGovernanceExecutor,
  curatorSkillSourceRef,
  discoverCuratorCandidates,
  handleAdminDiagnosticsRequest,
  MemoryCuratorGovernanceStore,
  publishBackgroundGovernanceObservation,
  renderCuratorCandidateReport,
  runBackgroundMemorySkillReview,
  type AdminRouteResult,
  type BackgroundServiceDiagnosticsProjection,
  type CuratorMutationCandidate,
  type CuratorObservedBehaviorEvidence,
} from "./index.js";
import type { LoadedSkill } from "./profile-loading.js";
import { createMemoryAgentActivityObservationSink } from "./test-support.js";

const now = "2026-06-21T16:00:00.000Z";
const root = mkdtempSync(join(tmpdir(), "rusty-crew-curator-review-e2e-"));
const skillsDir = join(root, "skills");
mkdirSync(skillsDir, { recursive: true });
writeFileSync(
  join(skillsDir, "managed.md"),
  `---
title: Managed Skill
summary: Existing managed skill.
tags:
  - smoke
---

Original body.
`,
);

const profileId = "prime" as ProfileId;
const sessionId = "curator-review-session" as SessionId;
const skills: LoadedSkill[] = [
  {
    slug: "managed",
    title: "Managed Skill",
    summary: "Existing managed skill.",
    tags: ["smoke"],
    bodyMarkdown: readFileSync(join(skillsDir, "managed.md"), "utf8"),
    sourcePath: join(skillsDir, "managed.md"),
  },
  {
    slug: "temporary-progress",
    title: "Managed Skill",
    tags: ["smoke"],
    bodyMarkdown: "TODO: move temporary project progress to Den tasks.",
    sourcePath: join(skillsDir, "temporary-progress.md"),
  },
];
const denseProfileMemory = [
  {
    profileId,
    key: "handoff",
    content: "Temporary handoff note from an old task.",
    revision: 1,
    updatedAt: now,
  },
  {
    profileId,
    key: "handoff",
    content: "Duplicate handoff note.",
    revision: 2,
    updatedAt: now,
  },
];
const autoSkillSlug = "review-loop-checklist";
const observedWorkflow =
  "Check unresolved review feedback, apply the smallest useful fix, and re-run the focused smoke before reporting back.";
const observedBehavior: CuratorObservedBehaviorEvidence[] = [
  {
    evidenceId: "observed:review-loop:curator-e2e",
    summary: "Repeatedly run the review loop before final handoff",
    suggestedSkillSlug: autoSkillSlug,
    suggestedTitle: "Review Loop Checklist",
    suggestedSummary:
      "Use when repeated review-loop behavior should become profile skill guidance.",
    workflowMarkdown: observedWorkflow,
    occurrences: 5,
    confidence: 0.91,
    tags: ["review", "curator", "auto-skill"],
  },
];
const diagnostics = buildToolContextDiagnosticsReport({
  now,
  session: {
    sessionId,
    agentId: "agent-prime",
    profileId,
    kind: "full",
  },
  toolDiagnostics: buildToolRegistryDiagnostics({ catalogId: "curator-e2e" }),
  memorySkillsPlanning: {
    denMemory: { configured: true, clientAvailable: true },
    skills: {
      rootConfigured: true,
      rootReadable: true,
      profileSkillCount: 2,
      loadedSkillCount: 2,
      missingSkillCount: 1,
      invalidSkillCount: 0,
    },
    denseProfileMemory: {
      clientAvailable: true,
      recordCount: 63,
      maxRecordsPerProfile: 64,
      capReached: true,
    },
    sessionSearch: { available: true },
    todo: { available: true },
    counters: { available: true, resetAllowed: false },
  },
  roleAssembly: {
    instructions: "Use stable memory only.",
    initialMessages: [],
  },
});

const batch = discoverCuratorCandidates({
  batchId: "curator-e2e-batch",
  now,
  scopeType: "profile",
  scopeId: profileId,
  profileId,
  skills,
  expectedSkillSlugs: ["managed", "missing-skill"],
  denseProfileMemory,
  observedBehavior,
});
const report = renderCuratorCandidateReport(batch);
assert.equal(batch.dryRun, true);
assert.equal(batch.candidateCount > 0, true);
assert.match(report, /Curator Candidate Report/);
assert.match(report, /missing-skill/);
assert.match(report, /review-loop-checklist/);

const autoSkillCandidate = batch.candidates.find(
  (candidate) => candidate.targetRef === `skill:${autoSkillSlug}`,
);
assert.equal(autoSkillCandidate?.kind, "skill_create");
assert.ok(
  autoSkillCandidate?.sourceRefs.some(
    (ref) => ref.kind === "observed_behavior",
  ),
);
const sourceRef = await curatorSkillSourceRef(skillsDir, "managed");
const mutationCandidate: CuratorMutationCandidate = {
  candidateId: "curator:e2e:patch-managed",
  batchId: batch.batchId,
  kind: "skill_patch",
  sourceRefs: [sourceRef],
  targetRef: "skill:managed",
  summary: "Patch managed skill body.",
  severity: "warning",
  confidence: 0.9,
  proposedAction: "Replace original body with curated body.",
  previewSummary: "Would patch managed skill body.",
  fingerprint: "curator-e2e-fingerprint",
  status: "proposed",
  rollbackSupported: true,
  mutation: {
    type: "skill_patch",
    slug: "managed",
    oldString: "Original body.",
    newString: "Curated body.",
  },
};
const autoSkillMutationCandidate: CuratorMutationCandidate = {
  ...autoSkillCandidate!,
  mutation: {
    type: "skill_create",
    slug: autoSkillSlug,
    content: [
      "---",
      "title: Review Loop Checklist",
      "summary: Use when repeated review-loop behavior should become profile skill guidance.",
      "tags:",
      "  - review",
      "  - curator",
      "  - auto-skill",
      "---",
      "",
      observedWorkflow,
      "",
    ].join("\n"),
  },
};
const store = new MemoryCuratorGovernanceStore();
store.upsertBatch(batch, [mutationCandidate, autoSkillMutationCandidate]);
const curatorExecutor = createCuratorGovernanceExecutor({
  skillsDir,
  store,
  now: () => new Date(now),
});
const preview = await curatorExecutor({
  action: "apply_candidate",
  candidateId: mutationCandidate.candidateId,
  dryRun: true,
  reason: "e2e dry-run preview",
});
assert.equal(preview.status, "previewed");
assert.match(preview.summary, /changed=false/);
assert.match(readFileSync(join(skillsDir, "managed.md"), "utf8"), /Original/);
assert.equal(store.mutations.size, 0);

const autoSkillPath = join(skillsDir, `${autoSkillSlug}.md`);
const autoSkillPreview = await curatorExecutor({
  action: "preview_candidate",
  candidateId: autoSkillMutationCandidate.candidateId,
  dryRun: true,
  reason: "e2e observed-behavior auto-skill preview",
});
assert.equal(autoSkillPreview.status, "previewed");
assert.match(autoSkillPreview.summary, /changed=false/);
assert.equal(existsSync(autoSkillPath), false);

const autoSkillApproval = await curatorExecutor({
  action: "approve_candidate",
  candidateId: autoSkillMutationCandidate.candidateId,
  reason: "observed behavior should become reusable skill guidance",
  dryRun: false,
});
assert.equal(autoSkillApproval.status, "approved");

const autoSkillApplied = await curatorExecutor({
  action: "apply_candidate",
  candidateId: autoSkillMutationCandidate.candidateId,
  reason: "create observed-behavior auto-skill",
  dryRun: false,
});
assert.equal(autoSkillApplied.status, "applied");
assert.match(readFileSync(autoSkillPath, "utf8"), /Review Loop Checklist/);
assert.match(readFileSync(autoSkillPath, "utf8"), /focused smoke/);
assert.equal(store.mutations.size, 1);

const autoSkillSourceRef = await curatorSkillSourceRef(
  skillsDir,
  autoSkillSlug,
);
const retireAutoSkillCandidate: CuratorMutationCandidate = {
  candidateId: "curator:e2e:archive-review-loop-checklist",
  batchId: batch.batchId,
  kind: "skill_archive",
  sourceRefs: [autoSkillSourceRef],
  targetRef: `skill:${autoSkillSlug}`,
  summary: "Retire observed-behavior auto-skill after governance review.",
  severity: "info",
  confidence: 0.9,
  proposedAction: "Archive the auto-created skill after it is absorbed.",
  previewSummary: "Would archive the auto-created observed-behavior skill.",
  fingerprint: "curator-e2e-archive-fingerprint",
  status: "proposed",
  rollbackSupported: true,
  mutation: {
    type: "skill_archive",
    slug: autoSkillSlug,
    absorbedInto: "managed",
  },
};
store.upsertCandidate(retireAutoSkillCandidate);

const retirePreview = await curatorExecutor({
  action: "preview_candidate",
  candidateId: retireAutoSkillCandidate.candidateId,
  dryRun: true,
  reason: "e2e retire preview",
});
assert.equal(retirePreview.status, "previewed");
assert.equal(existsSync(autoSkillPath), true);

const retireApproval = await curatorExecutor({
  action: "approve_candidate",
  candidateId: retireAutoSkillCandidate.candidateId,
  reason: "auto-skill was absorbed into managed workflow",
  dryRun: false,
});
assert.equal(retireApproval.status, "approved");

const retired = await curatorExecutor({
  action: "apply_candidate",
  candidateId: retireAutoSkillCandidate.candidateId,
  reason: "retire observed-behavior auto-skill",
  dryRun: false,
});
assert.equal(retired.status, "applied");
assert.equal(existsSync(autoSkillPath), false);
assert.equal(store.mutations.size, 2);
const retireMutation = [...store.mutations.values()].at(-1)!;
assert.equal(retireMutation.action, "skill_archive");
assert.equal(existsSync(retireMutation.management?.archivePath ?? ""), true);

const observationSink = createMemoryAgentActivityObservationSink();
const producer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const review = await runBackgroundMemorySkillReview({
  runId: "review:e2e:1",
  now,
  payload: {
    schemaVersion: 1,
    reviewType: "combined",
    profileId,
    triggerSource: "scheduler",
    maxCandidates: 20,
    maxFindings: 20,
    llmReviewEnabled: true,
    dryRun: true,
    reason: "curator review e2e",
  },
  diagnostics,
  skills,
  denseProfileMemory,
  observation: {
    identity: {
      profile: profileId,
      instance_id: "background-review" as AgentId,
      session_key: sessionId,
    },
    sink: observationSink,
    required: true,
  },
});
assert.equal(review.findingCount > 0, true);
assert.equal(
  review.skippedReasons.includes("llm_review_requires_provider_path"),
  true,
);

const curatorObservation = await publishBackgroundGovernanceObservation({
  producer,
  identity: {
    profile: profileId,
    instance_id: "curator-loop",
    session_key: sessionId,
  },
  loopKind: "curator",
  phase: "completed",
  summary: `Curator produced ${batch.candidateCount} candidate(s) and dry-run preview ${preview.receiptId}.`,
  workRef: { run_id: batch.batchId, task_id: "2984" },
  resultRef: { document_slug: batch.reportId },
});
assert.equal(curatorObservation?.status, "published");

const background = buildBackgroundServiceDiagnosticsProjection({
  now,
  scheduler: {
    jobCount: 2,
    activeJobs: 2,
    pausedJobs: 0,
    staleRuns: 0,
    lastRunAt: now,
  },
  curator: {
    status: "available",
    candidateCount: batch.candidateCount,
    mutationCount: store.mutations.size,
    lastRunAt: now,
  },
  backgroundReview: {
    enabled: true,
    recentFindings: review.findingCount,
    lastRunAt: review.finishedAt,
  },
  cleanup: {
    lastRunAt: now,
    terminalArchived: 0,
    orphanedArchived: 0,
    expiredArchived: 0,
    adapterReleased: 0,
    adapterDegraded: 0,
  },
});
assert.equal(background.health, "ok");

const adminDiagnostics = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/diagnostics/background",
    requestId: "curator-review-e2e",
  },
  {
    diagnostics: buildRuntimeDiagnosticsProjection({
      now,
      observation: { enabled: true, writerAvailable: true },
    }),
    background,
  },
);
assert.equal(adminDiagnostics.status, 200);
assert.equal(
  okData<BackgroundServiceDiagnosticsProjection>(adminDiagnostics).health,
  "ok",
);
assert.equal(
  okData<BackgroundServiceDiagnosticsProjection>(adminDiagnostics).summary
    .curatorCandidates,
  batch.candidateCount,
);
assert.equal(observationSink.events.length, 2);
assert.deepEqual(
  observationSink.events.map((event) => event.event_type),
  ["work_checkpoint", "work_completed"],
);

console.log(
  JSON.stringify(
    {
      curatorCandidates: batch.candidateCount,
      previewStatus: preview.status,
      autoSkillStatus: autoSkillApplied.status,
      retireStatus: retired.status,
      mutationsWritten: store.mutations.size,
      reviewFindings: review.findingCount,
      diagnostics: background.summary,
      observations: observationSink.events.map((event) => event.event_type),
    },
    null,
    2,
  ),
);

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  return result.body.data as T;
}
