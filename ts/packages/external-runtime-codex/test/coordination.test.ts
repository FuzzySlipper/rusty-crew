import assert from "node:assert/strict";
import test from "node:test";

import { CODEX_COORDINATION_DYNAMIC_TOOLS } from "../src/index.js";

test("coordination catalog keeps model arguments small", () => {
  const namespace = CODEX_COORDINATION_DYNAMIC_TOOLS[0];
  assert.equal(namespace?.type, "namespace");
  if (namespace?.type !== "namespace") return;
  assert.equal(namespace.name, "rusty_crew");
  assert.deepEqual(
    namespace.tools.map((tool) => tool.name),
    ["send_agent_message", "agent_round"],
  );
  const schema = namespace.tools[0]?.inputSchema as {
    properties?: Record<string, unknown>;
  };
  assert.deepEqual(Object.keys(schema.properties ?? {}), [
    "recipient",
    "body",
    "correlationId",
  ]);
});
