import assert from "node:assert/strict";

import { loadNativeBridge } from "@rusty-crew/native-bridge";

import { createRoleplayNarratorFsmBridge } from "./roleplay-narrator-fsm.js";

const native = await loadNativeBridge();
const fsm = createRoleplayNarratorFsmBridge(native);

const mandatoryRequests = await fsm.mandatoryExploreRequests({
  sessionId: "narrator-fsm-session",
  profileId: "narrator-fsm-profile",
  pendingText:
    "The player finds a silver locket engraved with a serpent-and-rose crest.",
});
assert.deepEqual(
  mandatoryRequests.map((request) => request.toolName),
  ["get_scene_state", "recall_lore", "list_lore_layers"],
);
assert.equal(
  mandatoryRequests[1]?.paramsJson &&
    typeof mandatoryRequests[1].paramsJson === "object" &&
    !Array.isArray(mandatoryRequests[1].paramsJson)
    ? mandatoryRequests[1].paramsJson["recordTrace"]
    : undefined,
  true,
);

const autoCapture = await fsm.autoCaptureRequest({
  sessionId: "narrator-fsm-session",
  profileId: "narrator-fsm-profile",
  wakeId: "narrator-fsm-wake",
  pendingText:
    "The player finds a silver locket engraved with a serpent-and-rose crest.",
  layerDetailsJson: {
    result: [
      {
        layer_id: "story-auto",
        write_policy: "auto_capture",
        purpose: "story",
      },
    ],
  },
});
assert.equal(autoCapture?.toolName, "capture_lore_fact");
assert.equal(
  autoCapture?.paramsJson &&
    typeof autoCapture.paramsJson === "object" &&
    !Array.isArray(autoCapture.paramsJson)
    ? autoCapture.paramsJson["layerId"]
    : undefined,
  "story-auto",
);

const start = await fsm.startTurn({
  reviewEnabled: false,
  preludeObservations: [
    {
      toolName: "get_scene_state",
      ok: true,
      summary: "Scene state is present.",
    },
  ],
});
assert.equal(start.phase, "explore");
assert.equal(start.terminal, false);
assert.ok(start.instructions.includes("Mandatory explore tool results"));
assert.ok(start.allowedTools.includes("recall_lore"));

const compose = await fsm.nextPhase({
  state: start.state,
  completedPhase: "explore",
  outputText: '{"location":"archive","charactersPresent":["Ada"]}',
});
assert.equal(compose.phase, "compose");
assert.equal(compose.terminal, false);
assert.ok(compose.instructions.includes("Roleplay narrator phase: compose"));
assert.equal(
  compose.state.sceneBrief,
  '{"location":"archive","charactersPresent":["Ada"]}',
);

const done = await fsm.nextPhase({
  state: compose.state,
  completedPhase: "compose",
  outputText: "Ada opened the archive door.",
});
assert.equal(done.phase, "done");
assert.equal(done.terminal, true);

const reviewedStart = await fsm.startTurn({
  reviewEnabled: true,
  maxReviewCycles: 2,
});
const draft = await fsm.nextPhase({
  state: reviewedStart.state,
  completedPhase: "explore",
  outputText: "Scene brief",
});
assert.equal(draft.phase, "compose_draft");

const review = await fsm.nextPhase({
  state: draft.state,
  completedPhase: "compose_draft",
  outputText: "First draft",
});
assert.equal(review.phase, "review");
assert.ok(await fsm.reviewRequestsRevision("revise for continuity"));
assert.equal(await fsm.reviewRequestsRevision("all clear"), false);

const revisedDraft = await fsm.nextPhase({
  state: review.state,
  completedPhase: "review",
  outputText: "revise for continuity",
});
assert.equal(revisedDraft.phase, "compose_draft");

const secondReview = await fsm.nextPhase({
  state: revisedDraft.state,
  completedPhase: "compose_draft",
  outputText: "Second draft",
});
const finalCompose = await fsm.nextPhase({
  state: secondReview.state,
  completedPhase: "review",
  outputText: "revise again",
});
assert.equal(finalCompose.phase, "compose");
assert.equal(finalCompose.state.reviewCycle, 2);

console.log(
  JSON.stringify(
    {
      mandatoryTools: mandatoryRequests.map((request) => request.toolName),
      autoCaptureTool: autoCapture?.toolName,
      noReviewPath: [start.phase, compose.phase, done.phase],
      reviewPath: [
        reviewedStart.phase,
        draft.phase,
        review.phase,
        revisedDraft.phase,
        secondReview.phase,
        finalCompose.phase,
      ],
    },
    null,
    2,
  ),
);
