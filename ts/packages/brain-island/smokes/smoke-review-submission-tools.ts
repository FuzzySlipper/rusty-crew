import assert from "node:assert/strict";
import { createServer } from "node:http";
import { submitTaskForReviewTool } from "../src/review-submission-tools.js";
import { createDefaultMcpDiscoveryClient } from "../src/service-mcp-tools.js";

const calls: unknown[] = [];
const tool = submitTaskForReviewTool({
  async submit(input) {
    calls.push(input);
    return {
      ok: true,
      submissionId: "review-submission:test",
      phase: "gate_pending",
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
    taskId: 6574,
    repository: "earendil-works/rusty-crew",
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
    taskId: 6574,
    repository: "earendil-works/rusty-crew",
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
