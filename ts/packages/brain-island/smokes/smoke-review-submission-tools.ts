import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
  completeRoutedReviewTool,
  submitTaskForReviewTool,
} from "../src/review-submission-tools.js";
import { createDefaultMcpDiscoveryClient } from "../src/service-mcp-tools.js";

const calls: unknown[] = [];
const tool = submitTaskForReviewTool({
  async submit(input) {
    calls.push(input);
    return {
      ok: true,
      submissionId: "review-submission:test",
      phase: "gate_pending",
      projectId: input.projectId,
      taskId: input.taskId,
      commitSha: input.commitSha,
      summary: "accepted without waiting",
    };
  },
});

assert.match(tool.description, /normal Den task review submission/);
assert.match(tool.description, /lower-level Den review and GitHub-gate tools/i);
assert.ok(tool.executeWithContext);
const result = await tool.executeWithContext!(
  {
    projectId: "rusty-crew",
    taskId: 6574,
    repository: "FuzzySlipper/rusty-crew",
    commitSha: "a".repeat(40),
    ref: "main",
    requiredChecks: ["Verify Offline", "Verify Postgres Backend"],
    baseCommit: "b".repeat(40),
    reviewSummaryMd: "Implemented and verified.",
  },
  {
    wake: {
      state: {
        session: { agentId: "runner" },
      },
    } as never,
    wakeId: "wake-1",
    sessionId: "session-1",
    callId: "call-1",
    signal: new AbortController().signal,
  },
);
assert.equal(result.turnDisposition, "complete_turn");
assert.equal(result.details.phase, "gate_pending");
assert.deepEqual(calls, [
  {
    projectId: "rusty-crew",
    taskId: 6574,
    repository: "FuzzySlipper/rusty-crew",
    commitSha: "a".repeat(40),
    ref: "main",
    requiredChecks: ["Verify Offline", "Verify Postgres Backend"],
    baseCommit: "b".repeat(40),
    reviewSummaryMd: "Implemented and verified.",
    reviewer: "@reviewer",
    caller: {
      type: "direct_brain",
      sessionId: "session-1",
      wakeId: "wake-1",
      toolCallId: "call-1",
    },
  },
]);

const completionCalls: unknown[] = [];
const completionTool = completeRoutedReviewTool({
  async submit() {
    throw new Error("not used");
  },
  async complete(input) {
    completionCalls.push(input);
    return {
      ok: true,
      submissionId: "review-submission:one",
      taskId: 6574,
      commitSha: "a".repeat(40),
      finalizationId: 12,
      packetId: 13,
      packetMessageId: 14,
      exactHeadCommit: "a".repeat(40),
      verdict: input.verdict,
      findingStatuses: [],
      taskStatus: "done",
      replyMessageId: "review-reply-message:one",
      replyStatus: "accepted",
      summary: "completed",
    };
  },
});
assert.ok(completionTool.executeWithContext);
const completionResult = await completionTool.executeWithContext!(
  {
    verdict: "looks_good",
    taskId: 6574,
    commitSha: "a".repeat(40),
    notes: "Focused checks passed.",
    evidence: ["npm test", "git diff --check"],
  },
  {
    wake: {
      state: {
        session: { agentId: "reviewer" },
        pendingMessages: [
          {
            from: "runner",
            to: "reviewer",
            body: "review request",
            correlationId: "review:6574:" + "a".repeat(40),
          },
        ],
      },
    } as never,
    wakeId: "wake-review",
    sessionId: "reviewer-session",
    callId: "call-review",
    signal: new AbortController().signal,
  },
);
assert.equal(completionResult.turnDisposition, "complete_turn");
assert.equal(completionResult.details.replyStatus, "accepted");
assert.deepEqual(completionCalls, [
  {
    verdict: "looks_good",
    taskId: 6574,
    commitSha: "a".repeat(40),
    notes: "Focused checks passed.",
    evidence: ["npm test", "git diff --check"],
    caller: {
      type: "review_submission",
      submissionId: "context-resolved",
    },
    reviewerSessionId: "reviewer-session",
    correlationId: "review:6574:" + "a".repeat(40),
  },
]);

const discoveryRequests: unknown[] = [];
const server = createServer((request, response) => {
  const chunks: Buffer[] = [];
  request.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
  request.on("end", () => {
    const rpc = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    discoveryRequests.push(rpc);
    response.setHeader("content-type", "application/json");
    response.end(
      JSON.stringify({
        jsonrpc: "2.0",
        id: rpc.id,
        result: {
          tools: [
            {
              name: "request_review",
              description: "green path",
              inputSchema: { type: "object" },
            },
          ],
        },
      }),
    );
  });
});
await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
try {
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const client = createDefaultMcpDiscoveryClient({
    bindingId: "den-binding",
    adapterId: "den-adapter",
    profileId: "runner",
    agentId: "runner",
    implementationId: "den-mcp",
    toolProfileKey: "planner",
    serverNames: ["den"],
    endpointRef: `http://127.0.0.1:${address.port}/mcp`,
    status: "active",
  } as never);
  assert.ok(client);
  const discovered = await client.listTools();
  assert.equal(discovered.length, 1);
  assert.deepEqual((discoveryRequests[0] as { params: unknown }).params, {
    toolProfile: "managed-runtime",
  });
} finally {
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
}

console.log("review submission tool smoke passed");
