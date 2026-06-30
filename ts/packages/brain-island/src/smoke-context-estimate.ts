import assert from "node:assert/strict";
import {
  contextTokenBudget,
  estimateApproximateTokens,
  estimateContextUsage,
  textFragmentsFromPayload,
} from "./index.js";

assert.equal(estimateApproximateTokens("one two three four"), 6);

const budget = contextTokenBudget({
  contextWindowTokens: 128_000,
  maxOutputTokens: 4_096,
});
assert.equal(budget.contextWindowTokens, 128_000);
assert.equal(budget.maxOutputTokens, 4_096);
assert.equal(budget.reservedResponseTokens, 4_096);
assert.equal(budget.safetyMarginTokens, 2_560);
assert.equal(budget.usableInputTokens, 121_344);

assert.deepEqual(textFragmentsFromPayload({ body: "hello", text: "world" }), [
  "hello",
  "world",
]);

const usage = estimateContextUsage({
  provider: {
    contextWindowTokens: 128_000,
    maxOutputTokens: 4_096,
  },
  textFragments: ["hello world"],
  sampledEventCount: 3,
  sampledMessageCount: 1,
});
assert.equal(usage.estimateQuality, "approximate");
assert.equal(usage.estimatorId, "fallback_chars_words_v1");
assert.equal(usage.estimatedPromptTokens, 3);
assert.equal(usage.estimatedRemainingTokens, 127_997);
assert.equal(usage.sampledEventCount, 3);
assert.equal(usage.sampledMessageCount, 1);

const unavailable = estimateContextUsage({
  textFragments: ["hello world"],
  sampledEventCount: 1,
  sampledMessageCount: 1,
});
assert.equal(unavailable.estimateQuality, "unavailable");
assert.equal(unavailable.estimatedRemainingTokens, undefined);

console.log(
  JSON.stringify(
    {
      estimatorId: usage.estimatorId,
      estimatedPromptTokens: usage.estimatedPromptTokens,
      usableInputTokens: usage.budget.usableInputTokens,
      unavailable: unavailable.estimateQuality,
    },
    null,
    2,
  ),
);
