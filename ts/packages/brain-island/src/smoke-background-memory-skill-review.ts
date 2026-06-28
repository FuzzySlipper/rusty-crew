import assert from "node:assert/strict";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import {
  buildToolContextDiagnosticsReport,
  buildToolRegistryDiagnostics,
  runBackgroundMemorySkillReview,
} from "./index.js";
import type { LoadedSkill } from "./profile-loading.js";
import { createMemoryAgentActivityObservationSink } from "./test-support.js";

const now = "2026-06-20T14:00:00.000Z";
const profileId = "prime" as ProfileId;
const sessionId = "session-prime" as SessionId;
const diagnostics = buildToolContextDiagnosticsReport({
  now,
  session: {
    sessionId,
    agentId: "agent-prime",
    profileId,
    kind: "full",
  },
  toolDiagnostics: buildToolRegistryDiagnostics({ catalogId: "review-smoke" }),
  memorySkillsPlanning: {
    denMemory: {
      configured: true,
      clientAvailable: false,
      lastError: "den memory service unavailable",
    },
    skills: {
      rootConfigured: true,
      rootReadable: true,
      profileSkillCount: 3,
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
    instructions: "Use compact stable memory only.",
    initialMessages: [],
  },
});
const skills: LoadedSkill[] = [
  {
    slug: "review-style",
    title: "Review Style",
    tags: ["review"],
    bodyMarkdown: "TODO: remove temporary project fact after migration.",
    sourcePath: "/skills/review-style.md",
  },
  {
    slug: "review-style-copy",
    title: "Review Style",
    summary: "Duplicate title for smoke coverage.",
    tags: ["review"],
    bodyMarkdown: "Stable review guidance.",
    sourcePath: "/skills/review-style-copy.md",
  },
];
const observationSink = createMemoryAgentActivityObservationSink();

const result = await runBackgroundMemorySkillReview({
  runId: "scheduled:review:1",
  now,
  payload: {
    schemaVersion: 1,
    reviewType: "combined",
    profileId,
    triggerSource: "cron",
    maxCandidates: 20,
    maxFindings: 20,
    llmReviewEnabled: true,
    dryRun: true,
    reason: "smoke",
  },
  diagnostics,
  skills,
  denseProfileMemory: [
    {
      profileId,
      key: "task-progress",
      content: "Blocked on temporary TODO from yesterday.",
      revision: 2,
      updatedAt: now,
    },
    {
      profileId,
      key: "task-progress",
      content: "Duplicate memory key.",
      revision: 3,
      updatedAt: now,
    },
  ],
  observation: {
    identity: {
      profile: profileId,
      instance_id: "agent-prime" as AgentId,
      session_key: sessionId,
    },
    sink: observationSink,
  },
});

assert.equal(result.reviewType, "combined");
assert.equal(result.triggerSource, "cron");
assert.equal(result.findingCount >= 7, true);
assert.equal(result.findings.length, result.findingCount);
assert.equal(result.resultRef.kind, "review_finding_batch");
assert.equal(result.resultRef.findingIds.length, result.findingCount);
assert.equal(result.findingFingerprints.length, result.findingCount);
assert.equal(
  result.skippedReasons.includes("llm_review_no_session_activity_digests"),
  true,
);
assert.equal(observationSink.events.length, 1);
assert.equal(result.observation?.payload.reason_code, "background_review");
assert.equal(
  result.findings.some(
    (finding) => finding.candidateKind === "dense_profile_memory",
  ),
  true,
);
assert.equal(
  result.findings.some((finding) => finding.candidateKind === "skill"),
  true,
);
assert.equal(
  result.findings.every((finding) => finding.sourceRefs.length > 0),
  true,
);

const quiet = await runBackgroundMemorySkillReview({
  runId: "scheduled:review:2",
  now,
  payload: {
    reviewType: "memory",
    profileId,
    maxFindings: 10,
    dryRun: true,
  },
  denseProfileMemory: [
    {
      profileId,
      key: "stable",
      content: "Prefers concise summaries.",
      revision: 1,
    },
  ],
});
assert.equal(quiet.findingCount, 0);
assert.equal(quiet.resultRef.findingIds.length, 0);
