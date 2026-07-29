import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD,
  DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS,
} from "../src/chat-completions-continuation-policy.js";
import {
  DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD,
  DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS,
} from "../src/responses-continuation-policy.js";

const serviceEnvExample = new URL(
  "../../../../ops/systemd/service.env.example",
  import.meta.url,
);

test("deployment template exposes scheduling quanta without lifetime ceilings", async () => {
  const contents = await readFile(serviceEnvExample, "utf8");

  assert.match(
    contents,
    new RegExp(
      `^RUSTY_CREW_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS=${DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS}$`,
      "m",
    ),
  );
  assert.doesNotMatch(contents, /RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS/);
  assert.match(
    contents,
    new RegExp(
      `^RUSTY_CREW_OPENAI_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS=${DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS}$`,
      "m",
    ),
  );
  assert.doesNotMatch(
    contents,
    /RUSTY_CREW_OPENAI_RESPONSES_MAX_CONTINUATION_ROUNDS/,
  );
  assert.match(
    contents,
    new RegExp(
      `^RUSTY_CREW_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD=${DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD}$`,
      "m",
    ),
  );
  assert.match(
    contents,
    new RegExp(
      `^RUSTY_CREW_OPENAI_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD=${DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD}$`,
      "m",
    ),
  );
});
