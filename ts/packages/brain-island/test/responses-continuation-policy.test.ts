import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_RESPONSES_MAX_CONTINUATION_ROUNDS,
  responsesMaxContinuationRounds,
} from "../src/responses-continuation-policy.js";

const variable = "RUSTY_CREW_OPENAI_RESPONSES_MAX_CONTINUATION_ROUNDS";

test("Responses continuation policy uses the durable default", () => {
  assert.equal(
    responsesMaxContinuationRounds({}),
    DEFAULT_RESPONSES_MAX_CONTINUATION_ROUNDS,
  );
});

test("Responses continuation policy accepts an explicit bounded limit", () => {
  assert.equal(responsesMaxContinuationRounds({ [variable]: " 96 " }), 96);
});

test("Responses continuation policy rejects zero and unreasonable limits", () => {
  for (const value of ["0", "-1", "1.5", "513", "many"]) {
    assert.throws(
      () => responsesMaxContinuationRounds({ [variable]: value }),
      new RegExp(variable),
    );
  }
});
