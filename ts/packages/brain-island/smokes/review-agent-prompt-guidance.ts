import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import type {
  AgentMessageCommand,
  AgentMessageDeliveryReceipt,
  SessionId,
} from "@rusty-crew/contracts";
import { CODEX_COORDINATION_DYNAMIC_TOOLS } from "@rusty-crew/external-runtime-codex";

import type { BrainToolContext } from "../src/brain-tool.js";
import {
  agentRoundTool,
  listAgentsTool,
  replyAgentMessageTool,
  sendAgentMessageTool,
  type CoordinationToolDetails,
  type CoordinationToolRuntime,
} from "../src/coordination-tools.js";
import {
  resolveCodexCoordinationToolCall,
  type CodexCoordinationPort,
} from "../src/external-runtime-coordination.js";

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

const codexToolSpecs = CODEX_COORDINATION_DYNAMIC_TOOLS.flatMap((entry) =>
  entry.type === "namespace" ? entry.tools : [],
);
const codexTools = codexToolSpecs.map((tool) => tool.name);
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

assertSchema(codexSchema("list_agents"), [], [], false);
assertSchema(
  codexSchema("send_agent_message"),
  ["recipient", "body", "correlationId", "ttlSeconds"],
  ["recipient", "body"],
  false,
);
assertSchema(
  codexSchema("reply_agent_message"),
  ["messageId", "body", "ttlSeconds"],
  ["messageId", "body"],
  false,
);
assertSchema(
  codexSchema("agent_round"),
  ["recipient", "body", "correlationId", "timeoutMs"],
  ["recipient", "body"],
  false,
);
assertSchema(
  codexSchema("complete_routed_review"),
  [
    "verdict",
    "taskId",
    "commitSha",
    "notes",
    "evidence",
    "priorFindingResolutions",
    "newFindings",
  ],
  ["verdict"],
  false,
);

assertSchema(schemaOf(listAgentsTool({})), [], [], false);
assertSchema(
  schemaOf(sendAgentMessageTool({})),
  ["toAddress", "body", "correlationId", "requireWake", "ttlSeconds"],
  ["toAddress", "body"],
  undefined,
);
assertSchema(
  schemaOf(replyAgentMessageTool({})),
  ["messageId", "body", "ttlSeconds"],
  ["messageId", "body"],
  undefined,
);
assertSchema(
  schemaOf(agentRoundTool({})),
  ["toAddress", "body", "correlationId", "timeoutMs"],
  ["toAddress", "body"],
  undefined,
);

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
  "/v1/coordination/routes",
  "/v1/debug/coordination/routes",
]) {
  assert.ok(openApi.paths[path], `operator API path missing: ${path}`);
  assert.match(guidance, new RegExp(path.replaceAll("/", "\\/")));
}

assert.match(
  guidance,
  /accept integer `ttlSeconds`[\s\S]*?values from 1 through 86,400/,
);
assert.match(guidance, /agent_message_recipient_session_changed/);
assert.match(guidance, /<reviewer-route>/);
assert.match(guidance, /@reviewer/);
assert.match(guidance, /does not return\n?durable message or delivery IDs/);
assert.match(guidance, /taskId[\s\S]*commitSha[\s\S]*explicitly/);

const codexSendResult = await resolveCodexCoordinationToolCall({
  params: {
    threadId: "guidance-thread",
    turnId: "guidance-turn",
    callId: "guidance-call",
    namespace: "rusty_crew",
    tool: "send_agent_message",
    arguments: {
      recipient: "@reviewer",
      body: "review exact commit",
      correlationId: "review-correlation",
      ttlSeconds: 600,
    },
  },
  binding: {
    runtimeId: "guidance-runtime",
    bindingId: "guidance-binding",
    controllerInstanceId: "guidance-controller",
    controllerGeneration: 1,
  },
  port: acceptedCodexPort(),
  now: () => new Date("2026-07-15T00:00:00Z"),
});
assert.equal(codexSendResult?.success, true);
assert.equal(codexSendResult?.contentItems[0]?.type, "inputText");
assert.equal(
  codexSendResult?.contentItems[0]?.text,
  "message accepted; address=@reviewer; addressKind=raw_agent; agent=@reviewer; session=reviewer-session; runtime=direct_brain; activation=none",
);
assert.doesNotMatch(codexSendResult?.contentItems[0]?.text ?? "", /Id=/);

const builtInResult = await sendAgentMessageTool({
  runtime: acceptedBuiltInRuntime(),
}).executeWithContext?.(
  {
    toAddress: "@reviewer",
    body: "review exact commit",
    correlationId: "review-correlation",
    ttlSeconds: 600,
  },
  {
    wake: { state: { session: { agentId: "requester" } } },
    wakeId: "guidance-wake",
    sessionId: "guidance-session",
    callId: "guidance-call",
    signal: new AbortController().signal,
  } as unknown as BrainToolContext<CoordinationToolDetails>,
);
assert.equal(builtInResult?.details.ok, true);
assert.deepEqual(builtInResult?.details.routed, {
  accepted: true,
  sequence: 42,
});
assert.equal(builtInResult?.content[0]?.type, "text");
assert.equal(
  builtInResult?.content[0]?.type === "text"
    ? builtInResult.content[0].text
    : undefined,
  "message routed; address=@reviewer; concrete_target=unavailable",
);

console.log(
  JSON.stringify({
    codexTools: codexTools.length,
    builtInTools: builtInTools.length,
    statuses: 8,
    operatorRoutes: 4,
  }),
);

interface JsonSchema {
  readonly properties?: Record<string, unknown>;
  readonly required?: readonly string[];
  readonly additionalProperties?: boolean;
}

function codexSchema(name: string): JsonSchema {
  const tool = codexToolSpecs.find((candidate) => candidate.name === name);
  assert.ok(tool, `Codex tool schema missing: ${name}`);
  return tool.inputSchema as JsonSchema;
}

function schemaOf(input: { readonly parameters: unknown }): JsonSchema {
  return input.parameters as JsonSchema;
}

function assertSchema(
  schema: JsonSchema,
  properties: readonly string[],
  required: readonly string[],
  additionalProperties: boolean | undefined,
): void {
  assert.deepEqual(Object.keys(schema.properties ?? {}), properties);
  assert.deepEqual(schema.required ?? [], required);
  assert.equal(schema.additionalProperties, additionalProperties);
}

function acceptedCodexPort(): CodexCoordinationPort {
  return {
    listAgentDirectory: async () => [
      {
        agentId: "@reviewer",
        sessionId: "reviewer-session" as SessionId,
        profileId: "reviewer-profile",
        displayLabel: "Reviewer",
        sessionKind: "full",
        sessionStatus: "idle",
        runtimeKind: "direct_brain",
        routable: true,
      },
    ],
    listAgentRouteResolutions: async () => [],
    deliverAgentMessage: async (command) => acceptedDelivery(command),
    replyAgentMessage: async () => {
      throw new Error("unexpected reply");
    },
    beginAgentRound: async () => {
      throw new Error("unexpected round");
    },
    getAgentRound: async () => undefined,
  };
}

function acceptedDelivery(
  command: AgentMessageCommand,
): AgentMessageDeliveryReceipt {
  return {
    request: {
      deliveryId: command.deliveryId,
      idempotencyKey: command.idempotencyKey,
      messageId: command.messageId,
      fromAgentId: "requester",
      fromSessionId: "requester-session" as SessionId,
      requestedAddress: command.toAddress,
      toAgentId: command.toAddress,
      toSessionId: "reviewer-session" as SessionId,
      replyToMessageId: null,
      body: command.body,
      correlationId: command.correlationId,
      requireWake: command.requireWake,
      createdAt: command.createdAt,
      expiresAt: command.expiresAt,
    },
    status: "accepted",
    sequence: 42,
    terminalAt: command.createdAt,
    revision: 2,
  };
}

function acceptedBuiltInRuntime(): CoordinationToolRuntime {
  return {
    listAgents: async () => [],
    listRoutes: async () => [],
    routeMessage: async () => ({ accepted: true, sequence: 42 }),
    replyMessage: async () => ({ accepted: true, sequence: 43 }),
    roundTrip: async () => ({ accepted: true, sequence: 44 }),
  };
}
