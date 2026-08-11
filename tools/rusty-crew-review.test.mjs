import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { fileURLToPath } from "node:url";
import test from "node:test";

const cli = fileURLToPath(new URL("./rusty-crew-review.mjs", import.meta.url));
const common = [
  "submit",
  "--deployment-role",
  "debug",
  "--project-id",
  "den-services",
  "--task",
  "6797",
  "--repository",
  "FuzzySlipper/den-services",
  "--sha",
  "a".repeat(40),
  "--ref",
  "main",
  "--base-sha",
  "0".repeat(40),
  "--summary",
  "Managed checkless review.",
  "--client-id",
  "test-cli",
  "--idempotency-key",
  "6797-a",
];

test("review CLI sends an explicit checkless submission", async () => {
  let submitted;
  const server = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    submitted = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    response.writeHead(200, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        ok: true,
        data: {
          submissionId: "review-submission:test",
          deploymentRole: "debug",
          projectId: "den-services",
          taskId: 6797,
          commitSha: "a".repeat(40),
          phase: "reviewer_dispatch_pending",
          terminalReason: "no_required_checks",
        },
        meta: {},
      }),
    );
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.notEqual(address, null);
  assert.equal(typeof address, "object");
  const result = await run([
    ...common,
    "--service-url",
    `http://127.0.0.1:${address.port}`,
    "--no-checks",
    "--json",
  ]);
  server.close();

  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(submitted.requiredChecks, []);
});

test("review CLI requires an explicit check mode", async () => {
  const omitted = await run([...common, "--service-url", "http://127.0.0.1:1"]);
  assert.equal(omitted.code, 64);
  assert.match(omitted.stderr, /--check or explicit --no-checks/);

  const conflicting = await run([
    ...common,
    "--service-url",
    "http://127.0.0.1:1",
    "--check",
    "Verify",
    "--no-checks",
  ]);
  assert.equal(conflicting.code, 64);
  assert.match(conflicting.stderr, /either --check or --no-checks/);
});

function run(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [cli, ...args], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}
