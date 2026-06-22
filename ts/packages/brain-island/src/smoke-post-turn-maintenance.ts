import assert from "node:assert/strict";
import type { CoreEvent, SessionId } from "@rusty-crew/contracts";
import { postTurnMaintenanceDecision } from "./post-turn-maintenance.js";

const sessionId = "runner-session" as SessionId;
const events: CoreEvent[] = ["read_file", "patch", "terminal"].map(
  (toolName) => ({
    type: "brain_event_observed",
    wakeId: "wake-complex",
    sessionId,
    event: { type: "tool_call_started", toolName },
  }),
);

const disabled = postTurnMaintenanceDecision({
  profileId: "rusty-crew-runner",
  wakeId: "wake-disabled",
  source: "delivery",
  backgroundReviewEnabled: false,
  events: [],
  completionSummary: "done",
});
assert.deepEqual(disabled, {
  action: "noop",
  reasonCode: "background_review_disabled",
  summary: "post-turn maintenance skipped because background review is off",
});

const simple = postTurnMaintenanceDecision({
  profileId: "rusty-crew-runner",
  wakeId: "wake-simple",
  source: "delivery",
  backgroundReviewEnabled: true,
  events: [],
  completionSummary: "short answer",
});
assert.equal(simple.action, "noop");
assert.equal(simple.reasonCode, "turn_not_complex");

const complex = postTurnMaintenanceDecision({
  profileId: "rusty-crew-runner",
  wakeId: "wake-complex",
  source: "delivery",
  backgroundReviewEnabled: true,
  events,
  completionSummary: "Implemented a repeatable workflow.",
});
assert.equal(complex.action, "propose_skill_candidate");
if (complex.action !== "propose_skill_candidate") {
  throw new Error("expected skill candidate");
}
assert.equal(
  complex.evidence.suggestedSkillSlug,
  "rusty-crew-runner-observed-workflow",
);
assert.match(complex.evidence.workflowMarkdown ?? "", /read_file, patch/);

console.log(
  JSON.stringify(
    {
      disabled: disabled.reasonCode,
      simple: simple.reasonCode,
      complex: complex.action,
    },
    null,
    2,
  ),
);
