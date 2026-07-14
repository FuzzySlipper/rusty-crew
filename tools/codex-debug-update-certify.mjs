#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const DEBUG_BASE_URL = "http://127.0.0.1:9348";
const DEBUG_ROOT = "/home/system/rusty-crew-debug";
const DEBUG_SERVICE = "rusty-crew-debug.service";
const DEBUG_APP_SERVER_SERVICE = "codex-app-server.service";
const LIVE_APP_SERVER_SERVICE = "codex-app-server-live.service";
const DEBUG_SOCKET = "/run/user/1001/codex-app-server/app-server.sock";
const SERVICE_ENV = join(DEBUG_ROOT, "config/service.env");
const EVIDENCE_ROOT = join(DEBUG_ROOT, "evidence/codex-compatibility");
const WORKFLOW_REVISION = "codex-debug-update-certify-v1";

export function parseOperatorArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      update: { type: "boolean", default: false },
      "skip-update": { type: "boolean", default: false },
      help: { type: "boolean", short: "h", default: false },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  if (values.update === values["skip-update"]) {
    throw new Error("choose exactly one of --update or --skip-update");
  }
  return {
    help: false,
    updateMode: values.update ? "update" : "explicit_skip",
  };
}

export function certificationIdentity(runtime) {
  const fields = [
    WORKFLOW_REVISION,
    runtime.runtimeId,
    runtime.observedCliVersion,
    runtime.consumedContractRevision,
    runtime.probeSuiteRevision,
  ];
  if (fields.some((value) => typeof value !== "string" || value.length === 0)) {
    throw new Error("runtime identity is incomplete after compatibility probe");
  }
  const digest = createHash("sha256").update(fields.join("\0")).digest("hex");
  return {
    certificationId: `codex-debug-${digest.slice(0, 24)}`,
    idempotencyKey: `codex-debug-certify:${digest}`,
  };
}

export function newThreadIds(before, after) {
  const existing = new Set(before);
  return after.filter((threadId) => !existing.has(threadId)).sort();
}

export function assertDebugRuntimeTarget(baseUrl, service, socketPath) {
  if (
    baseUrl !== DEBUG_BASE_URL ||
    service !== DEBUG_SERVICE ||
    socketPath !== DEBUG_SOCKET
  ) {
    throw new Error("Codex update certification is restricted to debug Crew");
  }
}

async function main() {
  const options = parseOperatorArgs(process.argv.slice(2));
  if (options.help) {
    console.log(`Usage: npm run codex:debug:update-certify -- (--update|--skip-update)

Stages a Codex CLI update through the isolated Rusty Crew debug deployment.
The command cannot target the live service.`);
    return;
  }

  assertDebugRuntimeTarget(DEBUG_BASE_URL, DEBUG_SERVICE, DEBUG_SOCKET);
  const startedAt = new Date().toISOString();
  const token = readEnvValue(SERVICE_ENV, "RUSTY_CREW_ADMIN_TOKEN");
  const evidence = {
    schemaVersion: 1,
    workflowRevision: WORKFLOW_REVISION,
    startedAt,
    completedAt: null,
    status: "running",
    updateMode: options.updateMode,
    target: {
      baseUrl: DEBUG_BASE_URL,
      root: DEBUG_ROOT,
      crewService: DEBUG_SERVICE,
      appServerService: DEBUG_APP_SERVER_SERVICE,
      socket: DEBUG_SOCKET,
    },
    liveAppServer: {},
    debugAppServer: {},
    installedVersion: {},
    runtime: {},
    smoke: { status: "not_run" },
    cleanup: { status: "not_run", threadIds: [] },
    certification: { status: "not_written" },
  };
  let evidencePath;

  try {
    const before = await inspectState(token);
    evidence.installedVersion.before = before.installedVersion;
    evidence.runtime.before = projectRuntime(before.runtime);
    evidence.liveAppServer.beforePid = before.livePid;
    evidence.debugAppServer.beforePid = before.debugPid;
    printState("before mutation", before);

    if (options.updateMode === "update") {
      run("/home/agent/.npm-global/bin/codex", ["update"], {
        stdio: "inherit",
      });
    } else {
      console.log("Codex self-update: explicitly skipped");
    }
    evidence.installedVersion.afterUpdate = installedCodexVersion();
    assertServicePid(LIVE_APP_SERVER_SERVICE, before.livePid);

    run("systemctl", [
      "--user",
      "restart",
      DEBUG_APP_SERVER_SERVICE,
      DEBUG_SERVICE,
    ]);
    const ready = await waitForReady(token);
    assertServicePid(LIVE_APP_SERVER_SERVICE, before.livePid);
    evidence.liveAppServer.afterRestartPid = servicePid(
      LIVE_APP_SERVER_SERVICE,
    );
    evidence.debugAppServer.afterRestartPid = ready.debugPid;
    evidence.installedVersion.afterRestart = ready.installedVersion;
    evidence.runtime.afterRestart = projectRuntime(ready.runtime);
    printState("debug services ready", ready);

    const threadsBefore = await listAllActiveThreadIds(
      token,
      ready.runtime.runtimeId,
    );
    let smoke;
    try {
      smoke = runExternalRuntimeSmoke();
    } catch (error) {
      evidence.smoke = {
        status: "failed",
        command:
          "npm run smoke:external-runtime-service-live -w @rusty-crew/brain-island",
        reason: error instanceof Error ? error.message : String(error),
      };
      evidence.cleanup = await cleanupIntroducedThreads(
        token,
        ready.runtime.runtimeId,
        threadsBefore,
      );
      throw error;
    }
    evidence.smoke = {
      status: "passed",
      command:
        "npm run smoke:external-runtime-service-live -w @rusty-crew/brain-island",
      outputSha256: createHash("sha256").update(smoke.stdout).digest("hex"),
    };

    evidence.cleanup = await cleanupIntroducedThreads(
      token,
      ready.runtime.runtimeId,
      threadsBefore,
    );

    assertServicePid(LIVE_APP_SERVER_SERVICE, before.livePid);
    const finalReady = await waitForReady(token);
    const identity = certificationIdentity(finalReady.runtime);
    const evidenceSummary = [
      WORKFLOW_REVISION,
      "real Codex turn streaming",
      "local tool execution",
      "interrupt/control",
      "history readback",
      "exact-ID resume",
      "disposable thread cleanup",
      "live app-server unchanged",
    ].join("; ");
    const certification = await api(
      token,
      "/v1/admin/external-runtime-certifications",
      {
        method: "POST",
        body: JSON.stringify({
          ...identity,
          runtimeId: finalReady.runtime.runtimeId,
          evidenceSummary,
        }),
      },
    );
    evidence.certification = {
      status: "written",
      certificationId: certification.certificationId,
      recordStatus: certification.status,
      revision: certification.revision,
    };
    const certified = await inspectState(token);
    assertServicePid(LIVE_APP_SERVER_SERVICE, before.livePid);
    evidence.runtime.certified = projectRuntime(certified.runtime);
    evidence.status = "passed";
    evidence.completedAt = new Date().toISOString();
    evidencePath = join(EVIDENCE_ROOT, `${identity.certificationId}.json`);
    writeEvidence(evidencePath, evidence);
    console.log(`Certification passed: ${certification.certificationId}`);
    console.log(`Evidence: ${evidencePath}`);
  } catch (error) {
    evidence.status = "failed";
    evidence.completedAt = new Date().toISOString();
    evidence.failure = error instanceof Error ? error.message : String(error);
    evidencePath = join(
      EVIDENCE_ROOT,
      `failed-${startedAt.replaceAll(/[:.]/g, "-")}.json`,
    );
    writeEvidence(evidencePath, evidence);
    console.error(`Codex debug certification failed: ${evidence.failure}`);
    console.error(`Evidence: ${evidencePath}`);
    process.exitCode = 1;
  }
}

async function inspectState(token) {
  const fleet = await api(token, "/v1/external-runtimes");
  const ready = selectDebugRuntime(fleet);
  return {
    installedVersion: installedCodexVersion(),
    livePid: servicePid(LIVE_APP_SERVER_SERVICE),
    debugPid: servicePid(DEBUG_APP_SERVER_SERVICE),
    runtime: ready,
  };
}

async function waitForReady(token) {
  const deadline = Date.now() + 120_000;
  let lastReason = "debug service did not answer";
  while (Date.now() < deadline) {
    try {
      const health = await api(token, "/v1/admin/healthz");
      const fleet = await api(token, "/v1/external-runtimes");
      const runtime = selectDebugRuntime(fleet);
      if (
        health.ok === true &&
        runtime.observedState === "ready" &&
        runtime.driverState === "ready" &&
        runtime.probeOutcome === "passed" &&
        typeof runtime.observedCliVersion === "string" &&
        typeof runtime.consumedContractRevision === "string"
      ) {
        return {
          installedVersion: installedCodexVersion(),
          livePid: servicePid(LIVE_APP_SERVER_SERVICE),
          debugPid: servicePid(DEBUG_APP_SERVER_SERVICE),
          runtime,
        };
      }
      lastReason = JSON.stringify(projectRuntime(runtime));
    } catch (error) {
      lastReason = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`debug readiness/probe timeout: ${lastReason}`);
}

function selectDebugRuntime(fleet) {
  const registrations = fleet.runtimes ?? [];
  const controllers = fleet.controllers ?? [];
  const candidates = registrations
    .filter((runtime) => runtime.kind === "codex_app_server")
    .map((runtime) => ({
      ...runtime,
      ...(controllers.find(
        (controller) => controller.runtimeId === runtime.runtimeId,
      ) ?? {}),
    }));
  if (candidates.length !== 1) {
    throw new Error(
      `debug service must expose exactly one Codex runtime, found ${candidates.length}`,
    );
  }
  const runtime = candidates[0];
  return {
    ...runtime,
    probeOutcome: runtime.lastCompatibilityProbe?.outcome ?? null,
    probeSuiteRevision: runtime.lastCompatibilityProbe?.suiteRevision ?? null,
  };
}

async function listAllActiveThreadIds(token, runtimeId) {
  const ids = [];
  let cursor;
  do {
    const query = new URLSearchParams({ limit: "100" });
    if (cursor) query.set("cursor", cursor);
    const page = await api(
      token,
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads?${query}`,
    );
    ids.push(...page.items.map((thread) => thread.threadId));
    cursor = page.nextCursor ?? undefined;
  } while (cursor);
  return ids;
}

async function cleanupIntroducedThreads(token, runtimeId, threadsBefore) {
  const threadsAfter = await listAllActiveThreadIds(token, runtimeId);
  const disposableThreads = newThreadIds(threadsBefore, threadsAfter);
  for (const threadId of disposableThreads) {
    await api(
      token,
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/${encodeURIComponent(threadId)}/delete`,
      { method: "POST" },
    );
  }
  return { status: "passed", threadIds: disposableThreads };
}

function runExternalRuntimeSmoke() {
  const result = spawnSync(
    "npm",
    [
      "run",
      "smoke:external-runtime-service-live",
      "-w",
      "@rusty-crew/brain-island",
    ],
    {
      cwd: process.cwd(),
      env: {
        ...process.env,
        CODEX_APP_SERVER_SOCKET: DEBUG_SOCKET,
        CODEX_APP_SERVER_HOME_REF: join(DEBUG_ROOT, "codex-home"),
        CODEX_APP_SERVER_SERVICE_LIVE_ROOT: join(
          DEBUG_ROOT,
          "tmp/codex-certification",
        ),
        CODEX_APP_SERVER_RESTART_SERVICE: "0",
      },
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
      timeout: 20 * 60 * 1_000,
    },
  );
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.status !== 0) {
    throw new Error(
      `external runtime smoke failed with status ${result.status ?? "signal"}`,
    );
  }
  return { stdout: result.stdout ?? "" };
}

async function api(token, path, init = {}) {
  const deadline = Date.now() + 30_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const headers = new Headers(init.headers);
      headers.set("accept", "application/json");
      if (init.body !== undefined)
        headers.set("content-type", "application/json");
      if (token) headers.set("authorization", `Bearer ${token}`);
      const response = await fetch(new URL(path, DEBUG_BASE_URL), {
        ...init,
        headers,
      });
      const payload = await response.json().catch(() => undefined);
      if (response.ok && payload?.ok === true) return payload.data;
      const error = new Error(
        `debug Crew ${init.method ?? "GET"} ${path} failed (${response.status}): ${JSON.stringify(payload)}`,
      );
      if (response.status < 500) throw error;
      lastError = error;
    } catch (error) {
      lastError = error;
      if (
        error instanceof Error &&
        error.message.startsWith("debug Crew") &&
        !/failed \(5\d\d\)/.test(error.message)
      ) {
        throw error;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw lastError ?? new Error(`debug Crew request timed out: ${path}`);
}

function installedCodexVersion() {
  return execFileSync("/home/agent/.npm-global/bin/codex", ["--version"], {
    encoding: "utf8",
  }).trim();
}

function servicePid(service) {
  const value = execFileSync(
    "systemctl",
    ["--user", "show", service, "--property=MainPID", "--value"],
    { encoding: "utf8" },
  ).trim();
  const pid = Number(value);
  if (!Number.isSafeInteger(pid) || pid <= 0) {
    throw new Error(`${service} has no running MainPID`);
  }
  return pid;
}

function assertServicePid(service, expectedPid) {
  const actualPid = servicePid(service);
  if (actualPid !== expectedPid) {
    throw new Error(
      `${service} changed from PID ${expectedPid} to ${actualPid}; live service must remain untouched`,
    );
  }
}

function run(command, args, options = {}) {
  execFileSync(command, args, { ...options, encoding: "utf8" });
}

function readEnvValue(path, key) {
  const line = readFileSync(path, "utf8")
    .split(/\r?\n/)
    .find((candidate) =>
      new RegExp(`^(?:export\\s+)?${key}=`).test(candidate.trim()),
    );
  if (line === undefined) return undefined;
  const raw = line
    .trim()
    .replace(/^export\s+/, "")
    .slice(key.length + 1);
  if (
    (raw.startsWith('"') && raw.endsWith('"')) ||
    (raw.startsWith("'") && raw.endsWith("'"))
  ) {
    return raw.slice(1, -1);
  }
  return raw;
}

function projectRuntime(runtime) {
  return {
    runtimeId: runtime.runtimeId,
    observedState: runtime.observedState,
    driverState: runtime.driverState,
    observedCliVersion: runtime.observedCliVersion,
    consumedContractRevision: runtime.consumedContractRevision,
    compatibilityState: runtime.compatibilityState,
    compatibilityDiagnostic: runtime.compatibilityDiagnostic,
    probeOutcome: runtime.probeOutcome,
    probeSuiteRevision: runtime.probeSuiteRevision,
  };
}

function printState(label, state) {
  console.log(
    `${label}: installed=${state.installedVersion}; running=${state.runtime.observedCliVersion ?? "unknown"}; debug_pid=${state.debugPid}; live_pid=${state.livePid}`,
  );
}

function writeEvidence(path, evidence) {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = `${path}.tmp-${process.pid}`;
  writeFileSync(temporary, `${JSON.stringify(evidence, null, 2)}\n`, {
    mode: 0o600,
  });
  chmodSync(temporary, 0o600);
  renameSync(temporary, path);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
