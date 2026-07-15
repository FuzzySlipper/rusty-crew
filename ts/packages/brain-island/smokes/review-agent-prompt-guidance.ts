import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { CODEX_COORDINATION_DYNAMIC_TOOLS } from "@rusty-crew/external-runtime-codex";

const root = resolve(process.cwd(), "../../..");
const guidance = readFileSync(
  resolve(root, "docs/review-agent-inbox-and-prompt-guidance.md"),
  "utf8",
);
const registry = JSON.parse(
  readFileSync(
    resolve(root, "fixtures/tool-registry/default-tool-registry-metadata.json"),
    "utf8",
  ),
) as { tools: Array<{ name: string }> };
const openApi = JSON.parse(
  readFileSync(
    resolve(root, "docs/rusty-crew-api-capabilities.openapi.json"),
    "utf8",
  ),
) as { paths: Record<string, unknown> };

for (const heading of [
  "## Reviewer Profile Prompt",
  "## Review Requester Prompt",
  "## Identity and Delivery Policy",
  "## Tool Contracts",
  "## Status and Failure Handling",
  "## Operator Readback",
]) {
  assert.match(guidance, new RegExp(`^${heading}$`, "m"));
}

const codexTools = CODEX_COORDINATION_DYNAMIC_TOOLS.flatMap((entry) =>
  entry.type === "namespace" ? entry.tools.map((tool) => tool.name) : [],
);
const builtInTools = registry.tools.map((tool) => tool.name);
for (const tool of [
  "list_agents",
  "send_agent_message",
  "reply_agent_message",
  "agent_round",
]) {
  assert.ok(codexTools.includes(tool), `Codex tool missing: ${tool}`);
  assert.ok(builtInTools.includes(tool), `built-in tool missing: ${tool}`);
  assert.match(guidance, new RegExp(`(?:rusty_crew\\.)?${tool}`));
}

for (const status of [
  "queued",
  "in_progress",
  "awaiting_reply",
  "replied",
  "no_reply",
  "failed",
  "expired",
  "rejected",
]) {
  assert.match(guidance, new RegExp(`\\b${status}\\b`));
}

for (const path of [
  "/v1/coordination/messages",
  "/v1/debug/coordination/messages",
]) {
  assert.ok(openApi.paths[path], `operator API path missing: ${path}`);
  assert.match(guidance, new RegExp(path.replaceAll("/", "\\/")));
}

assert.match(guidance, /ttlSeconds.*1 through\n86,400/s);
assert.match(guidance, /agent_message_recipient_session_changed/);
assert.match(guidance, /<reviewer-agent-id>/);

console.log(
  JSON.stringify({
    codexTools: codexTools.length,
    builtInTools: builtInTools.length,
    statuses: 8,
    operatorRoutes: 2,
  }),
);
