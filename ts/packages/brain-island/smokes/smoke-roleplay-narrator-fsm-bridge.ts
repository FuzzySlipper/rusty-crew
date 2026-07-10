import assert from "node:assert/strict";

import { loadNativeBridge } from "@rusty-crew/native-bridge";

import { createRoleplayNarratorFsmBridge } from "../src/roleplay-narrator-fsm.js";

const native = await loadNativeBridge();
const fsm = createRoleplayNarratorFsmBridge(native);

const start = await fsm.startTurn({
  wakeId: "narrator-fsm-wake",
  sessionId: "narrator-fsm-session",
  profileId: "narrator-fsm-profile",
  pendingText:
    "The player finds a silver locket engraved with a serpent-and-rose crest.",
  reviewEnabled: true,
  maxReviewCycles: 2,
});
assert.equal(start.phase, "prelude_explore");
assert.equal(start.activity?.phase, "exploring");
assert.equal(start.directive.kind, "tool_batch");
assert.deepEqual(
  start.directive.kind === "tool_batch"
    ? start.directive.requests.map((request) => request.toolName)
    : [],
  ["get_scene_state", "recall_lore", "list_lore_layers"],
);

const capture = await fsm.advanceTurn({
  receipt: start,
  outcome: {
    kind: "tool_batch_completed",
    observations: [
      {
        toolName: "list_lore_layers",
        ok: true,
        summary: "one writable layer",
        detailsJson: {
          result: [
            {
              layer_id: "story-auto",
              write_policy: "auto_capture",
              purpose: "story",
            },
          ],
        },
      },
    ],
  },
});
assert.equal(capture.phase, "prelude_capture");
assert.equal(capture.directive.kind, "tool_batch");
assert.equal(
  capture.directive.kind === "tool_batch"
    ? capture.directive.requests[0]?.toolName
    : undefined,
  "capture_lore_fact",
);

const explore = await fsm.advanceTurn({
  receipt: capture,
  outcome: {
    kind: "tool_batch_completed",
    observations: [
      {
        toolName: "capture_lore_fact",
        ok: true,
        summary: "captured",
      },
    ],
  },
});
assert.equal(explore.phase, "explore");
assert.equal(explore.directive.kind, "provider_phase");
assert.equal(
  explore.directive.kind === "provider_phase"
    ? explore.directive.outputMode
    : undefined,
  "internal",
);

const draft = await fsm.advanceTurn({
  receipt: explore,
  outcome: {
    kind: "provider_phase_completed",
    outputText: '{"location":"archive","charactersPresent":["Ada"]}',
  },
});
assert.equal(draft.phase, "compose_draft");
assert.equal(draft.activity?.phase, "composing");

const review = await fsm.advanceTurn({
  receipt: draft,
  outcome: {
    kind: "provider_phase_completed",
    outputText: "First draft",
  },
});
assert.equal(review.phase, "review");
assert.equal(review.activity?.phase, "reviewing");

const revisedDraft = await fsm.advanceTurn({
  receipt: review,
  outcome: {
    kind: "provider_phase_completed",
    outputText: "revise for continuity",
  },
});
assert.equal(revisedDraft.phase, "compose_draft");

const secondReview = await fsm.advanceTurn({
  receipt: revisedDraft,
  outcome: {
    kind: "provider_phase_completed",
    outputText: "Second draft",
  },
});
const finalCompose = await fsm.advanceTurn({
  receipt: secondReview,
  outcome: {
    kind: "provider_phase_completed",
    outputText: "revise again",
  },
});
assert.equal(finalCompose.phase, "compose");
assert.equal(finalCompose.state.reviewCycle, 2);
assert.equal(finalCompose.directive.kind, "provider_phase");
assert.equal(
  finalCompose.directive.kind === "provider_phase"
    ? finalCompose.directive.outputMode
    : undefined,
  "final",
);

const done = await fsm.advanceTurn({
  receipt: finalCompose,
  outcome: {
    kind: "provider_phase_completed",
    outputText: "Ada opened the archive door.",
  },
});
assert.equal(done.phase, "done");
assert.equal(done.terminal, true);
assert.equal(done.activity?.phase, "idle");

console.log(
  JSON.stringify(
    {
      receiptSequences: [
        start.sequence,
        capture.sequence,
        explore.sequence,
        draft.sequence,
        review.sequence,
        revisedDraft.sequence,
        secondReview.sequence,
        finalCompose.sequence,
        done.sequence,
      ],
      completedPhases: done.state.completedPhases,
      terminalActivity: done.activity,
    },
    null,
    2,
  ),
);
