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

test("review CLI exposes revision-guarded reconciliation without SQL", async () => {
  let observed;
  const server = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    observed = {
      method: request.method,
      url: request.url,
      body: JSON.parse(Buffer.concat(chunks).toString("utf8")),
    };
    response.writeHead(200, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        ok: true,
        data: {
          submissionId: `review-submission:${"d".repeat(64)}`,
          phase: "superseded",
          terminalReason: "automatic_den_task_already_done",
        },
      }),
    );
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.equal(typeof address, "object");
  const submissionId = `review-submission:${"d".repeat(64)}`;
  const result = await run([
    "reconcile",
    "--service-url",
    `http://127.0.0.1:${address.port}`,
    "--deployment-role",
    "debug",
    "--submission-id",
    submissionId,
    "--expected-revision",
    "12",
    "--json",
  ]);
  server.close();

  assert.equal(result.code, 5, result.stderr);
  assert.deepEqual(observed, {
    method: "POST",
    url: `/v1/admin/review-submissions/${encodeURIComponent(submissionId)}/reconcile`,
    body: { expectedRevision: 12, expectedDeploymentRole: "debug" },
  });
});

test("review CLI reads bounded recovery diagnostics", async () => {
  let observedUrl;
  const server = createServer((request, response) => {
    observedUrl = request.url;
    response.writeHead(200, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        ok: true,
        data: {
          pendingSubmissionCount: 0,
          oldestEligibleAgeMs: null,
          retryBackoff: { eligible: 0, waiting: 0, exhausted: 0 },
          terminalReconciliations: 12,
          suppressedStaleDispatches: 0,
          submissionsTruncated: false,
          submissions: [],
        },
      }),
    );
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.equal(typeof address, "object");
  const result = await run([
    "diagnostics",
    "--service-url",
    `http://127.0.0.1:${address.port}`,
    "--deployment-role",
    "debug",
    "--json",
  ]);
  server.close();

  assert.equal(result.code, 0, result.stderr);
  assert.equal(
    observedUrl,
    "/v1/admin/diagnostics/review-submission-recovery?expectedDeploymentRole=debug",
  );
});

test("review CLI prints only deterministic stale task handles", async () => {
  let observedUrl;
  const server = createServer((request, response) => {
    observedUrl = request.url;
    response.writeHead(200, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        ok: true,
        data: [
          { projectId: "alpha", taskId: 12 },
          { projectId: "beta", taskId: 34 },
        ],
      }),
    );
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.equal(typeof address, "object");
  const result = await run([
    "stale",
    "--service-url",
    `http://127.0.0.1:${address.port}`,
    "--deployment-role",
    "production",
    "--project",
    "beta",
    "--project",
    "alpha",
    "--stale-ms",
    "60000",
  ]);
  server.close();

  assert.equal(result.code, 0, result.stderr);
  assert.equal(result.stdout, "alpha#12\nbeta#34\n");
  assert.equal(
    observedUrl,
    "/v1/admin/review-operator/stale-review-tasks?expectedDeploymentRole=production&staleMs=60000&projectId=beta&projectId=alpha",
  );
});

test("review CLI prints no default output for an empty stale task list", async () => {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ ok: true, data: [] }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.equal(typeof address, "object");
  const serviceUrl = `http://127.0.0.1:${address.port}`;
  const plain = await run([
    "stale",
    "--service-url",
    serviceUrl,
    "--deployment-role",
    "debug",
  ]);
  const json = await run([
    "stale",
    "--service-url",
    serviceUrl,
    "--deployment-role",
    "debug",
    "--json",
  ]);
  server.close();

  assert.equal(plain.code, 0, plain.stderr);
  assert.equal(plain.stdout, "");
  assert.equal(json.code, 0, json.stderr);
  assert.equal(json.stdout, "[]\n");
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
