import assert from "node:assert/strict";
import { deliveryIntentWakeDecision } from "./channel-wake-policy.js";

const now = "2026-06-21T12:00:00.000Z";
const future = "2026-06-21T12:01:00.000Z";
const past = "2026-06-21T11:59:00.000Z";

assert.deepEqual(
  deliveryIntentWakeDecision({
    wakePolicy: "subscription",
    expiresAt: future,
    now,
  }),
  { action: "claim_and_wake", wakePolicy: "subscription" },
);
assert.deepEqual(
  deliveryIntentWakeDecision({ wakePolicy: "manual", expiresAt: future, now }),
  {
    action: "manual_wait",
    wakePolicy: "manual",
    reasonCode: "wake_policy_manual",
    summary:
      "profile channel wake policy is manual; waiting for an explicit pull/claim path",
  },
);
assert.deepEqual(
  deliveryIntentWakeDecision({
    wakePolicy: "disabled",
    expiresAt: future,
    now,
  }),
  {
    action: "reject",
    wakePolicy: "disabled",
    reasonCode: "wake_policy_disabled",
    summary: "profile channel wake policy disables automatic delivery",
  },
);
assert.deepEqual(
  deliveryIntentWakeDecision({
    wakePolicy: "subscription",
    expiresAt: past,
    now,
  }),
  {
    action: "skip_expired",
    wakePolicy: "subscription",
    reasonCode: "delivery_intent_expired",
    summary: "delivery intent expired before Rusty Crew claimed it",
  },
);
assert.equal(
  deliveryIntentWakeDecision({
    wakePolicy: undefined,
    expiresAt: future,
    now,
  }).wakePolicy,
  "subscription",
);

console.log(
  JSON.stringify(
    {
      subscription: "claim_and_wake",
      manual: "manual_wait",
      disabled: "reject",
      expired: "skip_expired",
    },
    null,
    2,
  ),
);
