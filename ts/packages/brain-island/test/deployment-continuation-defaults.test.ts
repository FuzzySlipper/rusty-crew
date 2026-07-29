import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { MAX_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS } from "../src/chat-completions-continuation-policy.js";
import { MAX_RESPONSES_MAX_CONTINUATION_ROUNDS } from "../src/responses-continuation-policy.js";

const serviceEnvExample = new URL(
  "../../../../ops/systemd/service.env.example",
  import.meta.url,
);

test("deployment template keeps temporary continuation ceilings at implementation maxima", async () => {
  const contents = await readFile(serviceEnvExample, "utf8");

  assert.match(
    contents,
    new RegExp(
      `^RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS=${MAX_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS}$`,
      "m",
    ),
  );
  assert.match(
    contents,
    new RegExp(
      `^RUSTY_CREW_OPENAI_RESPONSES_MAX_CONTINUATION_ROUNDS=${MAX_RESPONSES_MAX_CONTINUATION_ROUNDS}$`,
      "m",
    ),
  );
});
