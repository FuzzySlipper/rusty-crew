import assert from "node:assert/strict";
import test from "node:test";

import {
  CODEX_COORDINATION_DYNAMIC_TOOLS,
  CODEX_MANAGED_REVIEWER_DYNAMIC_TOOLS,
  codexCoordinationDynamicToolsForProfile,
} from "../src/index.js";

test("coordination catalog keeps model arguments small", () => {
  const namespace = CODEX_COORDINATION_DYNAMIC_TOOLS[0];
  assert.equal(namespace?.type, "namespace");
  if (namespace?.type !== "namespace") return;
  assert.equal(namespace.name, "rusty_crew");
  assert.deepEqual(
    namespace.tools.map((tool) => tool.name),
    [
      "list_agents",
      "send_agent_message",
      "reply_agent_message",
      "agent_round",
      "submit_task_for_review",
      "complete_routed_review",
    ],
  );
  const directorySchema = namespace.tools[0]?.inputSchema as {
    properties?: Record<string, unknown>;
  };
  assert.deepEqual(Object.keys(directorySchema.properties ?? {}), []);
  const schema = namespace.tools[1]?.inputSchema as {
    properties?: Record<string, unknown>;
  };
  assert.deepEqual(Object.keys(schema.properties ?? {}), [
    "recipient",
    "body",
    "correlationId",
    "ttlSeconds",
  ]);
  const replySchema = namespace.tools[2]?.inputSchema as {
    properties?: Record<string, unknown>;
  };
  assert.deepEqual(Object.keys(replySchema.properties ?? {}), [
    "messageId",
    "body",
    "ttlSeconds",
  ]);
  assert.match(
    namespace.tools.find((tool) => tool.name === "complete_routed_review")
      ?.description ?? "",
    /emit the returned contentItems text/,
  );
});

test("managed reviewer catalog removes the raw reply primitive by profile", () => {
  const managedNamespace = CODEX_MANAGED_REVIEWER_DYNAMIC_TOOLS[0];
  assert.equal(managedNamespace?.type, "namespace");
  if (managedNamespace?.type !== "namespace") return;
  assert.deepEqual(
    managedNamespace.tools.map((tool) => tool.name),
    [
      "list_agents",
      "send_agent_message",
      "agent_round",
      "submit_task_for_review",
      "complete_routed_review",
    ],
  );
  assert.equal(
    codexCoordinationDynamicToolsForProfile({ profileId: "reviewer" }),
    CODEX_MANAGED_REVIEWER_DYNAMIC_TOOLS,
  );
  assert.equal(
    codexCoordinationDynamicToolsForProfile({
      agentId: "reviewer-cert-5806",
    }),
    CODEX_MANAGED_REVIEWER_DYNAMIC_TOOLS,
  );
  assert.equal(
    codexCoordinationDynamicToolsForProfile({ profileId: "software-engineer" }),
    CODEX_COORDINATION_DYNAMIC_TOOLS,
  );
});
