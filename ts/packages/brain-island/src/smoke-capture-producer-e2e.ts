import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AdapterId,
  CoreEvent,
  EngineHandle,
  MemoryEvidenceKind,
  MemorySpaceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import {
  buildSessionActivityDigest,
  runBackgroundMemorySkillReview,
  type TypedCaptureMemoryProposal,
} from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-capture-producer-e2e-"));
const profileId = "capture-runner" as ProfileId;
const sessionId = "capture-session" as SessionId;
const wakeId = "wake-capture-1";
let bridge: NativeBridgeModule | undefined;
let engine: EngineHandle | undefined;

const profileDenseProposal: TypedCaptureMemoryProposal = {
  id: "profile_dense_compact_review_style",
  summary: "Remember compact review summary preference.",
  space_id: "profile_dense",
  operation: "add",
  scope: {
    scope_type: "profile",
    scope_id: profileId,
  },
  shape: {
    shape_id: "profile_dense_item" as never,
    version: 1,
  },
  content: {
    key: "review_style",
    content: "Prefers compact review summaries.",
  },
  evidence_refs: [
    {
      eventType: "tool_call_failed",
      wakeId,
      summary: "den_memory_recall failed during wake.",
    },
    {
      eventType: "user_correction",
      wakeId,
      summary: "Operator corrected profile review style preference.",
    },
    {
      eventType: "wake",
      wakeId,
      summary: "Capture source wake.",
    },
  ],
  confidence: 0.88,
  durability_rationale:
    "The preference is stable profile guidance for future review output.",
  governance_policy: "curator_route",
  dedupe_key: "profile_dense:capture-runner:review_style",
};

try {
  bridge = await loadNativeBridge();
  engine = await bridge.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-28T01:00:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });

  const digest = buildSessionActivityDigest({
    profileId,
    sessionId,
    wakeId,
    source: "channel",
    now: "2026-06-28T01:00:00.000Z",
    events: [
      observed({ type: "tool_call_started", toolName: "den_memory_recall" }),
      observed({
        type: "tool_call_finished",
        toolName: "den_memory_recall",
        isError: true,
      }),
      {
        type: "external_event_injected",
        event: {
          adapterId: "adapter-den" as AdapterId,
          source: "message-operator-correction",
          payload: {
            type: "human_message",
            from: "operator",
            text: "Actually the profile should remember compact review summaries.",
          },
        },
      },
      observed({
        type: "text_delta",
        text: "I will remember compact review summaries after review.",
      }),
      observed({ type: "finished" }),
    ],
    completionSummary: "Acknowledged compact review summary preference.",
  });
  await bridge.saveSessionActivityDigest(digest);

  const loadedDigests = await bridge.listSessionActivityDigests({
    profile_id: profileId,
    session_id: sessionId,
    include_reviewed: false,
    limit: 10,
    offset: 0,
  });
  assert.equal(loadedDigests.length, 1);
  assert.equal(loadedDigests[0]?.digest_id, digest.digest_id);
  assert.equal(
    loadedDigests[0]?.signals_json.some(
      (signal) => signalType(signal) === "tool_failure",
    ),
    true,
  );
  assert.equal(
    loadedDigests[0]?.signals_json.some(
      (signal) => signalType(signal) === "user_correction",
    ),
    true,
  );

  const review = await runBackgroundMemorySkillReview({
    runId: "capture-producer-e2e",
    now: "2026-06-28T01:01:00.000Z",
    payload: {
      reviewType: "memory",
      profileId,
      llmReviewEnabled: true,
      captureProviderAlias: "fake-capture",
      captureMaxProposals: 3,
      dryRun: false,
      maxFindings: 10,
    },
    sessionActivityDigests: loadedDigests,
    captureProvider: async (input) => {
      assert.equal(input.providerAlias, "fake-capture");
      assert.equal(input.maxProposals, 3);
      assert.equal(input.sessionActivityDigests.length, 1);
      assert.match(
        input.sessionActivityDigests[0]?.summary_text ?? "",
        /compact review/,
      );
      return { proposals: [profileDenseProposal], skippedReasons: [] };
    },
  });

  assert.equal(review.dryRun, false);
  assert.equal(review.skippedReasons.length, 0);
  const captureFinding = review.findings.find(
    (finding) => finding.memoryProposal?.source === "capture_producer",
  );
  assert.ok(captureFinding);
  assert.equal(captureFinding.memoryProposal?.space_id, "profile_dense");
  assert.equal(captureFinding.memoryProposal?.operation, "add");
  assert.equal(captureFinding.memoryProposal?.governance_mode, "curator_route");

  for (const finding of review.findings) {
    if (finding.memoryProposal) {
      await bridge.saveMemoryProposal(finding.memoryProposal);
    }
  }

  const pending = await bridge.listMemoryProposals({
    space_id: "profile_dense" as MemorySpaceId,
    status: "pending_review",
    dedupe_key: profileDenseProposal.dedupe_key,
    limit: 10,
    offset: 0,
  });
  assert.equal(pending.length, 1);
  const storedProposal = pending[0]?.proposal;
  assert.equal(storedProposal?.source, "capture_producer");
  assert.equal(storedProposal?.space_id, "profile_dense");
  assert.equal(pending[0]?.status, "pending_review");
  assert.deepEqual(evidenceTypes(storedProposal?.evidence_refs ?? []), [
    "tool_call",
    "user_correction",
    "wake",
  ]);
  assert.equal(
    await bridge.getProfileMemory({
      profileId,
      targetType: "profile",
      key: "review_style",
    }),
    undefined,
  );

  console.log("smoke-capture-producer-e2e ok");
} finally {
  if (bridge && engine !== undefined) {
    await bridge.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  }
  rmSync(root, { recursive: true, force: true });
}

function observed(
  event: Extract<CoreEvent, { type: "brain_event_observed" }>["event"],
): CoreEvent {
  return {
    type: "brain_event_observed",
    sessionId,
    wakeId,
    event,
  };
}

function signalType(value: unknown): string | undefined {
  if (
    typeof value !== "object" ||
    value === null ||
    !("signal_type" in value)
  ) {
    return undefined;
  }
  return String(value.signal_type);
}

function evidenceTypes(
  refs: readonly { evidence_type: MemoryEvidenceKind }[],
): string[] {
  return [...new Set(refs.map((ref) => ref.evidence_type.toString()))].sort();
}
