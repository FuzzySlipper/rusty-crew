#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
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

const LIVE_BASE_URL = "http://127.0.0.1:9347";
const DEBUG_BASE_URL = "http://127.0.0.1:9348";
const LIVE_ROOT = "/home/system/rusty-crew";
const LIVE_SERVICE = "rusty-crew.service";
const LIVE_APP_SERVER_SERVICE = "codex-app-server-live.service";
const LIVE_SOCKET = "/run/user/1001/codex-app-server-live/app-server.sock";
const LIVE_ENV = join(LIVE_ROOT, "config/service.env");
const DEBUG_ENV = "/home/system/rusty-crew-debug/config/service.env";
const EVIDENCE_ROOT = join(LIVE_ROOT, "evidence/codex-promotions");
const WORKFLOW_REVISION = "codex-live-promotion-v1";

export function parsePromotionArgs(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      promote: { type: "boolean", default: false },
      "override-active": { type: "boolean", default: false },
      help: { type: "boolean", short: "h", default: false },
    },
    strict: true,
  });
  if (values.help) return { help: true };
  if (!values.promote) throw new Error("live promotion requires --promote");
  return {
    help: false,
    overrideActive: values["override-active"],
  };
}

export function selectCurrentCertification(certifications, runtime) {
  return certifications.find(
    (record) =>
      record.status === "active" &&
      record.runtimeKind === "codex_app_server" &&
      record.observedCliVersion === runtime.observedCliVersion &&
      record.consumedContractRevision === runtime.consumedContractRevision &&
      record.probeSuiteRevision === runtime.probeSuiteRevision,
  );
}

export function promotionBlockers(readiness) {
  return {
    activeTurnIds: readiness.activeTurns.map((turn) => turn.request.requestId),
    interactionIds: readiness.pendingInteractions.map(
      (interaction) => interaction.interactionId,
    ),
  };
}

export function compareBindingSnapshots(before, after) {
  const afterById = new Map(after.map((entry) => [entry.bindingId, entry]));
  const changes = [];
  for (const entry of before) {
    const current = afterById.get(entry.bindingId);
    if (current === undefined) {
      changes.push({ bindingId: entry.bindingId, reason: "binding_missing" });
    } else if (current.nativeThreadId !== entry.nativeThreadId) {
      changes.push({
        bindingId: entry.bindingId,
        reason: "native_thread_replaced",
        before: entry.nativeThreadId,
        after: current.nativeThreadId,
      });
    }
  }
  for (const entry of after) {
    if (!before.some((candidate) => candidate.bindingId === entry.bindingId)) {
      changes.push({ bindingId: entry.bindingId, reason: "binding_created" });
    }
  }
  return changes;
}

export function compareTurnSnapshots(before, after) {
  const changes = [];
  for (const [bindingId, beforeIds] of Object.entries(before)) {
    const afterIds = after[bindingId];
    if (afterIds === undefined) {
      changes.push({ bindingId, beforeTurnIds: beforeIds, afterTurnIds: null });
      continue;
    }
    if (JSON.stringify(beforeIds) !== JSON.stringify(afterIds)) {
      changes.push({
        bindingId,
        beforeTurnIds: beforeIds,
        afterTurnIds: afterIds,
      });
    }
  }
  return changes;
}

async function main() {
  const options = parsePromotionArgs(process.argv.slice(2));
  if (options.help) {
    console.log(`Usage: npm run codex:live:promote -- --promote [--override-active]

Promotes only the exact Codex identity certified on the debug deployment.`);
    return;
  }

  const startedAt = new Date().toISOString();
  const liveToken = readEnvValue(LIVE_ENV, "RUSTY_CREW_ADMIN_TOKEN");
  const debugToken = readEnvValue(DEBUG_ENV, "RUSTY_CREW_ADMIN_TOKEN");
  const evidence = {
    schemaVersion: 1,
    workflowRevision: WORKFLOW_REVISION,
    startedAt,
    completedAt: null,
    status: "running",
    overrideActive: options.overrideActive,
    certification: {},
    before: {},
    after: {},
    staleBindings: [],
    recoveryGuidance: null,
  };
  let evidencePath = join(
    EVIDENCE_ROOT,
    `failed-${startedAt.replaceAll(/[:.]/g, "-")}.json`,
  );

  try {
    const [debugRuntime, debugCertifications, before] = await Promise.all([
      readSingleRuntime(DEBUG_BASE_URL, debugToken),
      api(
        DEBUG_BASE_URL,
        debugToken,
        "/v1/admin/external-runtime-certifications",
      ),
      readLiveReadiness(liveToken),
    ]);
    const installedVersion = normalizeCodexVersion(installedCodexVersion());
    if (
      debugRuntime.observedState !== "ready" ||
      debugRuntime.driverState !== "ready" ||
      debugRuntime.probeOutcome !== "passed" ||
      debugRuntime.observedCliVersion !== installedVersion
    ) {
      throw new Error(
        `debug runtime does not certify installed Codex ${installedVersion}: ${JSON.stringify(debugRuntime)}`,
      );
    }
    const certification = selectCurrentCertification(
      debugCertifications.certifications,
      debugRuntime,
    );
    if (certification === undefined) {
      throw new Error(
        "no active debug certification matches installed CLI, consumed contract, and probe suite",
      );
    }
    evidence.certification = {
      sourceService: DEBUG_BASE_URL,
      certificationId: certification.certificationId,
      observedCliVersion: certification.observedCliVersion,
      consumedContractRevision: certification.consumedContractRevision,
      probeSuiteRevision: certification.probeSuiteRevision,
    };

    const blockers = promotionBlockers(before);
    if (
      !options.overrideActive &&
      (blockers.activeTurnIds.length > 0 || blockers.interactionIds.length > 0)
    ) {
      throw new Error(
        `live promotion blocked by active work: ${JSON.stringify(blockers)}; rerun only with explicit --override-active after operator review`,
      );
    }

    const bindingSnapshot = before.activeBindings
      .filter((binding) => typeof binding.nativeThreadId === "string")
      .map((binding) => ({
        bindingId: binding.bindingId,
        nativeThreadId: binding.nativeThreadId,
      }))
      .sort((left, right) => left.bindingId.localeCompare(right.bindingId));
    const beforeThreads = await snapshotBindingTurns(
      liveToken,
      before.registration.runtimeId,
      bindingSnapshot,
    );
    const beforeAppServerPid = servicePid(LIVE_APP_SERVER_SERVICE);
    const beforeCrewPid = servicePid(LIVE_SERVICE);
    evidence.before = {
      installedVersion,
      appServerPid: beforeAppServerPid,
      crewPid: beforeCrewPid,
      controllerInstanceId: before.controller?.controllerInstanceId ?? null,
      controllerGeneration: before.controller?.controllerGeneration ?? null,
      leaseExpiresAt: before.controller?.leaseExpiresAt ?? null,
      blockers,
      bindings: bindingSnapshot,
      turns: beforeThreads.healthy,
      staleBindings: beforeThreads.stale,
    };

    execFileSync("systemctl", ["--user", "restart", LIVE_APP_SERVER_SERVICE]);
    await waitForServicePidChange(LIVE_APP_SERVER_SERVICE, beforeAppServerPid);
    execFileSync("systemctl", ["--user", "restart", LIVE_SERVICE]);
    const after = await waitForLiveReady(
      liveToken,
      beforeCrewPid,
      certification,
    );

    const afterBindings = after.activeBindings
      .filter((binding) => typeof binding.nativeThreadId === "string")
      .map((binding) => ({
        bindingId: binding.bindingId,
        nativeThreadId: binding.nativeThreadId,
      }))
      .sort((left, right) => left.bindingId.localeCompare(right.bindingId));
    const bindingChanges = compareBindingSnapshots(
      bindingSnapshot,
      afterBindings,
    );
    if (bindingChanges.length > 0) {
      throw new Error(
        `binding/thread identity changed during promotion: ${JSON.stringify(bindingChanges)}`,
      );
    }
    const afterThreads = await snapshotBindingTurns(
      liveToken,
      after.registration.runtimeId,
      afterBindings,
    );
    const turnChanges = compareTurnSnapshots(
      beforeThreads.healthy,
      afterThreads.healthy,
    );
    if (turnChanges.length > 0) {
      throw new Error(
        `native turn history changed during promotion: ${JSON.stringify(turnChanges)}`,
      );
    }
    const previouslyStale = new Set(
      beforeThreads.stale.map((entry) => entry.bindingId),
    );
    const newlyStale = afterThreads.stale.filter(
      (entry) => !previouslyStale.has(entry.bindingId),
    );
    if (newlyStale.length > 0) {
      throw new Error(
        `healthy bindings became stale during promotion: ${JSON.stringify(newlyStale)}`,
      );
    }
    if (
      after.controller?.controllerInstanceId ===
        before.controller?.controllerInstanceId ||
      after.controller?.leaseExpiresAt === before.controller?.leaseExpiresAt
    ) {
      throw new Error("live controller did not acquire a fresh lease");
    }
    const afterBlockers = promotionBlockers(after);
    if (
      !options.overrideActive &&
      (afterBlockers.activeTurnIds.length > 0 ||
        afterBlockers.interactionIds.length > 0)
    ) {
      throw new Error(
        `promotion created active work unexpectedly: ${JSON.stringify(afterBlockers)}`,
      );
    }

    const promotionDigest = createHash("sha256")
      .update(
        [
          WORKFLOW_REVISION,
          certification.certificationId,
          after.registration.runtimeId,
          certification.observedCliVersion,
          certification.consumedContractRevision,
        ].join("\0"),
      )
      .digest("hex");
    const liveCertification = await api(
      LIVE_BASE_URL,
      liveToken,
      "/v1/admin/external-runtime-certifications",
      {
        method: "POST",
        body: JSON.stringify({
          certificationId: `codex-live-${promotionDigest.slice(0, 24)}`,
          idempotencyKey: `codex-live-promotion:${promotionDigest}`,
          runtimeId: after.registration.runtimeId,
          evidenceSummary: `${WORKFLOW_REVISION}; promoted from debug certification ${certification.certificationId}; exact bindings and turn histories resumed; no replayed work`,
        }),
      },
    );

    evidence.after = {
      appServerPid: servicePid(LIVE_APP_SERVER_SERVICE),
      crewPid: servicePid(LIVE_SERVICE),
      controllerInstanceId: after.controller.controllerInstanceId,
      controllerGeneration: after.controller.controllerGeneration,
      leaseExpiresAt: after.controller.leaseExpiresAt,
      bindings: afterBindings,
      turns: afterThreads.healthy,
      blockers: afterBlockers,
      liveCertificationId: liveCertification.certificationId,
    };
    evidence.staleBindings = afterThreads.stale;
    evidence.status = "passed";
    evidence.completedAt = new Date().toISOString();
    evidencePath = join(
      EVIDENCE_ROOT,
      `promotion-${promotionDigest.slice(0, 24)}.json`,
    );
    writeEvidence(evidencePath, evidence);
    console.log(
      `Live Codex promotion passed: ${certification.observedCliVersion}`,
    );
    console.log(`Evidence: ${evidencePath}`);
    if (afterThreads.stale.length > 0) {
      console.log(
        `Stale bindings isolated: ${afterThreads.stale.map((entry) => entry.bindingId).join(", ")}`,
      );
    }
  } catch (error) {
    evidence.status = "failed";
    evidence.completedAt = new Date().toISOString();
    evidence.failure = error instanceof Error ? error.message : String(error);
    evidence.recoveryGuidance = [
      "Do not downgrade Codex or replay turns automatically.",
      `Inspect: systemctl --user status ${LIVE_APP_SERVER_SERVICE} ${LIVE_SERVICE}`,
      `Inspect: journalctl --user -u ${LIVE_SERVICE} -u ${LIVE_APP_SERVER_SERVICE} --since -30m`,
      "Resolve the exact failure, recertify on debug if identity changed, then rerun promotion.",
    ];
    writeEvidence(evidencePath, evidence);
    console.error(`Live Codex promotion failed: ${evidence.failure}`);
    console.error(`Evidence: ${evidencePath}`);
    process.exitCode = 1;
  }
}

async function readSingleRuntime(baseUrl, token) {
  const fleet = await api(baseUrl, token, "/v1/external-runtimes");
  const registrations = fleet.runtimes.filter(
    (runtime) => runtime.kind === "codex_app_server",
  );
  if (registrations.length !== 1) {
    throw new Error(
      `${baseUrl} must expose exactly one Codex runtime, found ${registrations.length}`,
    );
  }
  const registration = registrations[0];
  const controller = fleet.controllers.find(
    (candidate) => candidate.runtimeId === registration.runtimeId,
  );
  return {
    ...registration,
    ...(controller ?? {}),
    probeOutcome: controller?.lastCompatibilityProbe?.outcome ?? null,
    probeSuiteRevision:
      controller?.lastCompatibilityProbe?.suiteRevision ?? null,
  };
}

async function readLiveReadiness(token) {
  const runtime = await readSingleRuntime(LIVE_BASE_URL, token);
  return api(
    LIVE_BASE_URL,
    token,
    `/v1/admin/external-runtime-promotion-readiness?runtimeId=${encodeURIComponent(runtime.runtimeId)}`,
  );
}

async function waitForLiveReady(token, beforeCrewPid, certification) {
  const deadline = Date.now() + 120_000;
  let lastReason = "live service unavailable";
  while (Date.now() < deadline) {
    try {
      const readiness = await readLiveReadiness(token);
      const currentPid = servicePid(LIVE_SERVICE);
      if (
        currentPid !== beforeCrewPid &&
        readiness.registration.observedState === "ready" &&
        readiness.controller?.driverState === "ready" &&
        readiness.controller?.lastCompatibilityProbe?.outcome === "passed" &&
        readiness.registration.observedCliVersion ===
          certification.observedCliVersion &&
        readiness.registration.consumedContractRevision ===
          certification.consumedContractRevision
      ) {
        return readiness;
      }
      lastReason = JSON.stringify({
        currentPid,
        registration: readiness.registration,
        controller: readiness.controller,
      });
    } catch (error) {
      lastReason = error instanceof Error ? error.message : String(error);
    }
    await delay(500);
  }
  throw new Error(`live readiness timeout: ${lastReason}`);
}

async function snapshotBindingTurns(token, runtimeId, bindings) {
  const healthy = {};
  const stale = [];
  for (const binding of bindings) {
    try {
      const result = await api(
        LIVE_BASE_URL,
        token,
        `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/read`,
        {
          method: "POST",
          body: JSON.stringify({
            threadId: binding.nativeThreadId,
            includeTurns: true,
          }),
        },
        { retryServerErrors: false },
      );
      healthy[binding.bindingId] = result.thread.turns.map(
        (turn) => turn.turnId,
      );
    } catch (error) {
      stale.push({
        bindingId: binding.bindingId,
        nativeThreadId: binding.nativeThreadId,
        reason: error instanceof Error ? error.message : String(error),
      });
    }
  }
  return { healthy, stale };
}

async function api(baseUrl, token, path, init = {}, options = {}) {
  const deadline = Date.now() + 30_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const headers = new Headers(init.headers);
      headers.set("accept", "application/json");
      if (init.body !== undefined)
        headers.set("content-type", "application/json");
      if (token) headers.set("authorization", `Bearer ${token}`);
      const response = await fetch(new URL(path, baseUrl), {
        ...init,
        headers,
      });
      const payload = await response.json().catch(() => undefined);
      if (response.ok && payload?.ok === true) return payload.data;
      const error = new Error(
        `${baseUrl} ${init.method ?? "GET"} ${path} failed (${response.status}): ${JSON.stringify(payload)}`,
      );
      if (response.status < 500 || options.retryServerErrors === false)
        throw error;
      lastError = error;
    } catch (error) {
      lastError = error;
      if (
        error instanceof Error &&
        error.message.startsWith(baseUrl) &&
        (!/failed \(5\d\d\)/.test(error.message) ||
          options.retryServerErrors === false)
      ) {
        throw error;
      }
    }
    await delay(250);
  }
  throw lastError ?? new Error(`${baseUrl} request timed out: ${path}`);
}

async function waitForServicePidChange(service, beforePid) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      if (servicePid(service) !== beforePid) return;
    } catch {
      // systemd may briefly report MainPID=0 during restart.
    }
    await delay(200);
  }
  throw new Error(`${service} did not acquire a new MainPID`);
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

function installedCodexVersion() {
  return execFileSync("/home/agent/.npm-global/bin/codex", ["--version"], {
    encoding: "utf8",
  }).trim();
}

function normalizeCodexVersion(value) {
  const match = value.match(/(\d+\.\d+\.\d+)/);
  if (!match) throw new Error(`could not parse Codex version from ${value}`);
  return match[1];
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

function writeEvidence(path, evidence) {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = `${path}.tmp-${process.pid}`;
  writeFileSync(temporary, `${JSON.stringify(evidence, null, 2)}\n`, {
    mode: 0o600,
  });
  chmodSync(temporary, 0o600);
  renameSync(temporary, path);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
