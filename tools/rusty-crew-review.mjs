#!/usr/bin/env node

import { readFileSync } from "node:fs";

const EXIT = Object.freeze({
  OK: 0,
  PENDING: 2,
  GATE_FAILED: 3,
  CHANGES_REQUESTED: 4,
  SUPERSEDED: 5,
  USAGE: 64,
  SERVICE: 70,
});

class CliError extends Error {
  constructor(message, code = EXIT.SERVICE, details) {
    super(message);
    this.code = code;
    this.details = details;
  }
}

const main = async () => {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printUsage();
    return;
  }
  const command = options.positionals[0];
  if (
    command !== "submit" &&
    command !== "status" &&
    command !== "recover" &&
    command !== "reconcile" &&
    command !== "diagnostics"
  ) {
    throw new CliError(
      "command must be submit, status, recover, reconcile, or diagnostics",
      EXIT.USAGE,
    );
  }
  const serviceUrl = requiredOption(options, "service-url");
  const deploymentRole = requiredOption(options, "deployment-role");
  if (deploymentRole !== "production" && deploymentRole !== "debug") {
    throw new CliError(
      "--deployment-role must be production or debug",
      EXIT.USAGE,
    );
  }
  const client = createApiClient(serviceUrl, options);
  const data =
    command === "submit"
      ? await submit(client, options, deploymentRole)
      : command === "recover"
        ? await recover(client, options, deploymentRole)
        : command === "reconcile"
          ? await reconcile(client, options, deploymentRole)
          : command === "diagnostics"
            ? await diagnostics(client, deploymentRole)
            : await readStatus(client, options, deploymentRole);
  printData(data, options.json);
  if (options.wait && command !== "diagnostics") {
    const final = await waitForTerminal(
      client,
      options,
      deploymentRole,
      data.submissionId,
    );
    if (final !== data) printData(final, options.json);
    process.exitCode = terminalExitCode(final);
    return;
  }
  process.exitCode =
    command === "status" || command === "reconcile"
      ? terminalExitCode(data)
      : command === "diagnostics"
        ? EXIT.OK
        : acceptedExitCode(data);
};

async function submit(client, options, deploymentRole) {
  const summary = readSummary(options);
  const checks = options.values.check;
  if (checks.length > 0 && options.noChecks) {
    throw new CliError(
      "submit accepts either --check or --no-checks, not both",
      EXIT.USAGE,
    );
  }
  if (checks.length === 0 && !options.noChecks) {
    throw new CliError(
      "submit requires at least one --check or explicit --no-checks",
      EXIT.USAGE,
    );
  }
  const data = await client.request("POST", "/v1/admin/review-submissions", {
    projectId: requiredOption(options, "project-id"),
    taskId: positiveInteger(options, "task"),
    repository: requiredOption(options, "repository"),
    commitSha: requiredOption(options, "sha"),
    ref: requiredOption(options, "ref"),
    requiredChecks: checks,
    ...(options.values["base-sha"] === undefined
      ? {}
      : { baseCommit: options.values["base-sha"] }),
    reviewSummaryMd: summary,
    clientId: requiredOption(options, "client-id"),
    idempotencyKey: requiredOption(options, "idempotency-key"),
    expectedDeploymentRole: deploymentRole,
  });
  return data;
}

async function readStatus(client, options, deploymentRole) {
  const submissionId = requiredOption(options, "submission-id");
  const query = new URLSearchParams({
    expectedDeploymentRole: deploymentRole,
  });
  return client.request(
    "GET",
    `/v1/admin/review-submissions/${encodeURIComponent(submissionId)}?${query}`,
  );
}

async function recover(client, options, deploymentRole) {
  const submissionId = requiredOption(options, "submission-id");
  const expectedRevision = integerOption(
    options,
    "expected-revision",
    undefined,
    0,
  );
  if (expectedRevision === undefined) {
    throw new CliError("recover requires --expected-revision", EXIT.USAGE);
  }
  return client.request(
    "POST",
    `/v1/admin/review-submissions/${encodeURIComponent(submissionId)}/recover`,
    { expectedRevision, expectedDeploymentRole: deploymentRole },
  );
}

async function reconcile(client, options, deploymentRole) {
  const submissionId = requiredOption(options, "submission-id");
  const expectedRevision = integerOption(
    options,
    "expected-revision",
    undefined,
    0,
  );
  if (expectedRevision === undefined) {
    throw new CliError("reconcile requires --expected-revision", EXIT.USAGE);
  }
  return client.request(
    "POST",
    `/v1/admin/review-submissions/${encodeURIComponent(submissionId)}/reconcile`,
    { expectedRevision, expectedDeploymentRole: deploymentRole },
  );
}

async function diagnostics(client, deploymentRole) {
  const query = new URLSearchParams({ expectedDeploymentRole: deploymentRole });
  return client.request(
    "GET",
    `/v1/admin/diagnostics/review-submission-recovery?${query}`,
  );
}

async function waitForTerminal(client, options, deploymentRole, submissionId) {
  const pollMs = integerOption(options, "poll-ms", 5_000, 250);
  const timeoutMs = integerOption(options, "timeout-ms", 0, 0);
  const startedAt = Date.now();
  let current = { submissionId };
  while (!isTerminal(current)) {
    if (timeoutMs > 0 && Date.now() - startedAt >= timeoutMs) {
      throw new CliError(
        `review submission ${submissionId} is still pending after ${timeoutMs}ms`,
        EXIT.PENDING,
        current,
      );
    }
    await sleep(pollMs);
    const query = new URLSearchParams({
      expectedDeploymentRole: deploymentRole,
    });
    current = await client.request(
      "GET",
      `/v1/admin/review-submissions/${encodeURIComponent(submissionId)}?${query}`,
    );
  }
  return current;
}

function isTerminal(data) {
  return [
    "replied",
    "reply_terminal",
    "review_terminal",
    "superseded",
    "gate_failed",
  ].includes(data?.phase);
}

function acceptedExitCode(data) {
  return isTerminal(data) ? terminalExitCode(data) : EXIT.OK;
}

function terminalExitCode(data) {
  if (data?.gateStatus && data.gateStatus !== "passed") {
    return EXIT.GATE_FAILED;
  }
  switch (data?.phase) {
    case "gate_failed":
      return EXIT.GATE_FAILED;
    case "superseded":
      return EXIT.SUPERSEDED;
    case "review_terminal":
    case "replied":
      return data.reviewVerdict === "looks_good"
        ? EXIT.OK
        : EXIT.CHANGES_REQUESTED;
    case "reply_terminal":
      return EXIT.CHANGES_REQUESTED;
    default:
      return EXIT.PENDING;
  }
}

function createApiClient(serviceUrl, options) {
  let baseUrl;
  try {
    baseUrl = new URL(serviceUrl.endsWith("/") ? serviceUrl : `${serviceUrl}/`);
  } catch {
    throw new CliError(`invalid --service-url: ${serviceUrl}`, EXIT.USAGE);
  }
  const token = process.env.RUSTY_CREW_ADMIN_TOKEN?.trim();
  return {
    async request(method, path, body) {
      const headers = { accept: "application/json" };
      if (body !== undefined) headers["content-type"] = "application/json";
      if (token) headers.authorization = `Bearer ${token}`;
      let response;
      try {
        response = await fetch(new URL(path.slice(1), baseUrl), {
          method,
          headers,
          body: body === undefined ? undefined : JSON.stringify(body),
        });
      } catch (error) {
        throw new CliError(
          `request to ${baseUrl.origin} failed: ${errorMessage(error)}`,
          EXIT.SERVICE,
        );
      }
      const raw = await response.text();
      let envelope;
      try {
        envelope = raw ? JSON.parse(raw) : {};
      } catch {
        throw new CliError(
          `service returned non-JSON HTTP ${response.status}`,
          EXIT.SERVICE,
        );
      }
      if (!response.ok || envelope.ok !== true) {
        const error = envelope.error ?? {};
        throw new CliError(
          `${error.reason_code ?? "service_error"}: ${error.message ?? `HTTP ${response.status}`}`,
          response.status === 409 ? EXIT.USAGE : EXIT.SERVICE,
          envelope,
        );
      }
      return envelope.data;
    },
  };
}

function parseArgs(argv) {
  const options = {
    positionals: [],
    values: { check: [] },
    json: false,
    help: false,
    wait: false,
    noChecks: false,
  };
  const valueOptions = new Set([
    "service-url",
    "deployment-role",
    "project-id",
    "service-role",
    "check",
    "task",
    "repository",
    "sha",
    "ref",
    "base-sha",
    "summary",
    "summary-file",
    "client-id",
    "idempotency-key",
    "submission-id",
    "expected-revision",
    "poll-ms",
    "timeout-ms",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!arg.startsWith("--")) {
      options.positionals.push(arg);
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      options.help = true;
      continue;
    }
    if (arg === "--json") {
      options.json = true;
      continue;
    }
    if (arg === "--wait") {
      options.wait = true;
      continue;
    }
    if (arg === "--no-checks") {
      options.noChecks = true;
      continue;
    }
    const name = arg.slice(2);
    if (!valueOptions.has(name)) {
      throw new CliError(`unknown option --${name}`, EXIT.USAGE);
    }
    const value = argv[++index];
    if (value === undefined || value.startsWith("--")) {
      throw new CliError(`option --${name} requires a value`, EXIT.USAGE);
    }
    if (name === "check") options.values.check.push(value);
    else options.values[name] = value;
  }
  if (options.values["service-role"] !== undefined) {
    throw new CliError(
      "use --deployment-role; --service-role is intentionally not accepted",
      EXIT.USAGE,
    );
  }
  return options;
}

function requiredOption(options, name) {
  const value = options.values[name];
  if (typeof value !== "string" || value.trim() === "") {
    throw new CliError(`missing --${name}`, EXIT.USAGE);
  }
  return value;
}

function positiveInteger(options, name) {
  const value = Number(requiredOption(options, name));
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new CliError(`--${name} must be a positive integer`, EXIT.USAGE);
  }
  return value;
}

function integerOption(options, name, defaultValue, minimum) {
  const raw = options.values[name];
  if (raw === undefined) return defaultValue;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < minimum) {
    throw new CliError(
      `--${name} must be an integer >= ${minimum}`,
      EXIT.USAGE,
    );
  }
  return value;
}

function readSummary(options) {
  const direct = options.values.summary;
  const file = options.values["summary-file"];
  if (direct !== undefined && file !== undefined) {
    throw new CliError(
      "use only one of --summary and --summary-file",
      EXIT.USAGE,
    );
  }
  if (file !== undefined) {
    try {
      return readFileSync(file, "utf8");
    } catch (error) {
      throw new CliError(
        `could not read --summary-file: ${errorMessage(error)}`,
        EXIT.USAGE,
      );
    }
  }
  if (direct === undefined || direct.trim() === "") {
    throw new CliError(
      "submit requires --summary or --summary-file",
      EXIT.USAGE,
    );
  }
  return direct;
}

function printData(data, json) {
  if (json) {
    process.stdout.write(`${JSON.stringify(data)}\n`);
    return;
  }
  if (data.submissionId === undefined) {
    process.stdout.write(
      [
        `pending: ${data.pendingSubmissionCount}`,
        `oldest_eligible_age_ms: ${data.oldestEligibleAgeMs ?? "none"}`,
        `retry_eligible: ${data.retryBackoff?.eligible ?? 0}`,
        `retry_waiting: ${data.retryBackoff?.waiting ?? 0}`,
        `retry_exhausted: ${data.retryBackoff?.exhausted ?? 0}`,
        `terminal_reconciliations: ${data.terminalReconciliations ?? 0}`,
        `suppressed_stale_dispatches: ${data.suppressedStaleDispatches ?? 0}`,
      ].join("\n") + "\n",
    );
    return;
  }
  const lines = [
    `submission: ${data.submissionId}`,
    `service: ${data.deploymentRole}`,
    `project: ${data.projectId}`,
    `task: #${data.taskId}`,
    `commit: ${data.commitSha}`,
    `phase: ${data.phase}`,
  ];
  if (data.reviewVerdict) lines.push(`verdict: ${data.reviewVerdict}`);
  if (data.gateStatus) lines.push(`gate: ${data.gateStatus}`);
  if (data.lastAdapterError)
    lines.push(`adapter_error: ${data.lastAdapterError}`);
  if (data.terminalReason)
    lines.push(`terminal_reason: ${data.terminalReason}`);
  process.stdout.write(`${lines.join("\n")}\n`);
}

function printUsage() {
  process.stdout.write(
    "Submit requires one or more --check NAME arguments, or explicit --no-checks.\n\n",
  );
  process.stdout.write(
    `Usage:\n  rusty-crew-review submit --service-url URL --deployment-role production|debug \\\n    --project-id PROJECT --task ID --repository OWNER/REPO --sha SHA --ref REF \\\n    --check NAME --base-sha SHA --summary TEXT|--summary-file PATH \\\n    --client-id ID --idempotency-key KEY [--wait] [--json]\n\n  rusty-crew-review status --service-url URL --deployment-role production|debug \\\n    --submission-id ID [--wait] [--json]\n\n  rusty-crew-review recover --service-url URL --deployment-role production|debug \\\n    --submission-id ID --expected-revision REVISION [--wait] [--json]\n\n  rusty-crew-review reconcile --service-url URL --deployment-role production|debug \\\n    --submission-id ID --expected-revision REVISION [--wait] [--json]\n\n  rusty-crew-review diagnostics --service-url URL --deployment-role production|debug [--json]\n\nEnvironment:\n  RUSTY_CREW_ADMIN_TOKEN  Bearer token for the selected service (when enabled).\n\nExit codes:\n  0 success/accepted, 2 pending, 3 GitHub gate failed, 4 changes requested,\n  5 superseded, 64 usage error, 70 service error.\n`,
  );
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

main().catch((error) => {
  const code = error instanceof CliError ? error.code : EXIT.SERVICE;
  const message = errorMessage(error);
  if (process.argv.includes("--json")) {
    process.stderr.write(
      `${JSON.stringify({ ok: false, error: message, details: error.details })}\n`,
    );
  } else {
    process.stderr.write(`rusty-crew-review: ${message}\n`);
  }
  process.exitCode = code;
});
