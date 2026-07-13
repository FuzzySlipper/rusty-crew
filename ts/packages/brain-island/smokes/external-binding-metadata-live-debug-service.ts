import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const profileId =
  process.env.RUSTY_CREW_EXTERNAL_METADATA_PROFILE_ID ?? "rv-codex-5516-a";
const cwd =
  process.env.RUSTY_CREW_EXTERNAL_METADATA_CWD ?? "/home/dev/rusty-crew";
const suffix = `${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
const initialLabel = `Metadata cert ${suffix}`;
const changedLabel = `Metadata cert renamed ${suffix}`;
const restoredLabel = `Metadata cert restored ${suffix}`;
let runtimeId: string | undefined;
let nativeThreadId: string | undefined;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `external binding metadata certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  await waitForReady();
  runtimeId = await readyRuntimeId();

  const created = await request("POST", "/v1/external-agent-sessions", {
    idempotencyKey: `task-5765:${suffix}`,
    runtimeId,
    profileId,
    cwd,
    taskRef: { project_id: "rusty-crew", task_id: "5765" },
    label: initialLabel,
  });
  assert.equal(created.status, 200, created.text);
  const bindingId = requiredString(created.json, [
    "data",
    "creation",
    "binding",
    "bindingId",
  ]);
  nativeThreadId = requiredString(created.json, [
    "data",
    "creation",
    "nativeThreadId",
  ]);
  const initialRevision = requiredNumber(created.json, [
    "data",
    "creation",
    "binding",
    "revision",
  ]);
  assert.equal(
    nested(created.json, ["data", "creation", "binding", "label"]),
    initialLabel,
  );
  assert.equal(nested(created.json, ["data", "thread", "name"]), initialLabel);
  await materializeThread(bindingId);
  await assertThreadName(runtimeId, nativeThreadId, initialLabel);

  const changed = await writeMetadata(bindingId, initialRevision, {
    label: changedLabel,
    taskRef: { project_id: "rusty-crew", task_id: "5764" },
  });
  assert.equal(nested(changed.json, ["data", "label"]), changedLabel);
  assert.deepEqual(nested(changed.json, ["data", "taskRef"]), {
    project_id: "rusty-crew",
    task_id: "5764",
  });
  const changedRevision = requiredNumber(changed.json, ["data", "revision"]);
  await assertThreadName(runtimeId, nativeThreadId, changedLabel);

  const stale = await writeMetadata(bindingId, initialRevision, {
    label: "stale metadata write",
    taskRef: null,
  });
  assert.equal(stale.status, 409, stale.text);
  assert.equal(
    nested(stale.json, ["error", "reason_code"]),
    "external_binding_metadata_revision_conflict",
  );

  const cleared = await writeMetadata(bindingId, changedRevision, {
    label: null,
    taskRef: null,
  });
  assert.equal(nested(cleared.json, ["data", "label"]), null);
  assert.equal(nested(cleared.json, ["data", "taskRef"]), null);
  const clearedRevision = requiredNumber(cleared.json, ["data", "revision"]);
  await assertThreadName(runtimeId, nativeThreadId, null);

  await restartDebugService();
  await waitForReady();
  await assertBinding(bindingId, clearedRevision, null, null);
  await assertThreadName(runtimeId, nativeThreadId, null);

  const restored = await writeMetadata(bindingId, clearedRevision, {
    label: restoredLabel,
    taskRef: { project_id: "rusty-crew", task_id: "5765" },
  });
  const restoredRevision = requiredNumber(restored.json, ["data", "revision"]);
  await assertThreadName(runtimeId, nativeThreadId, restoredLabel);

  await restartDebugService();
  await waitForReady();
  await assertBinding(bindingId, restoredRevision, restoredLabel, {
    project_id: "rusty-crew",
    task_id: "5765",
  });
  await assertThreadName(runtimeId, nativeThreadId, restoredLabel);

  console.log(
    JSON.stringify(
      {
        baseUrl,
        runtimeId,
        profileId,
        bindingId,
        nativeThreadId,
        revisions: {
          initial: initialRevision,
          changed: changedRevision,
          cleared: clearedRevision,
          restored: restoredRevision,
        },
        setChangedClearRestarted: true,
        staleWriteRejected: true,
        nativeNameProjectionVerified: true,
      },
      null,
      2,
    ),
  );
} finally {
  if (runtimeId !== undefined && nativeThreadId !== undefined) {
    const cleanup = await request(
      "POST",
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/${encodeURIComponent(nativeThreadId)}/delete`,
    ).catch(() => undefined);
    if (cleanup !== undefined && cleanup.status >= 400) {
      console.error(
        `external metadata certification cleanup failed: ${cleanup.text}`,
      );
    }
  }
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

async function materializeThread(bindingId: string): Promise<void> {
  const deliveryId = `task-5765-materialize:${suffix}`;
  const delivered = await request(
    "POST",
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/messages`,
    {
      deliveryId,
      idempotencyKey: deliveryId,
      messageId: `${deliveryId}:message`,
      body: "Reply with exactly EXTERNAL_METADATA_CERT_READY and nothing else.",
      ttlMs: 60_000,
    },
  );
  assert.equal(delivered.status, 200, delivered.text);
  assert.equal(
    nested(delivered.json, ["data", "activation", "type"]),
    "external_turn_requested",
  );
  const requestId = requiredString(delivered.json, [
    "data",
    "activation",
    "requestId",
  ]);
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const receipt = await request(
      "GET",
      `/v1/external-turns/${encodeURIComponent(requestId)}`,
    );
    assert.equal(receipt.status, 200, receipt.text);
    const phase = nested(receipt.json, ["data", "phase"]);
    if (phase === "completed") return;
    if (phase === "failed" || phase === "outcome_unknown") {
      throw new Error(
        `materialization turn ended in ${String(phase)}: ${receipt.text}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("materialization turn did not complete within 180 seconds");
}

async function readyRuntimeId(): Promise<string> {
  const response = await request("GET", "/v1/external-runtimes");
  assert.equal(response.status, 200, response.text);
  const runtimes = nested(response.json, ["data", "runtimes"]);
  assert.ok(Array.isArray(runtimes), "runtime list must be an array");
  const runtime = runtimes.find(
    (candidate) =>
      isRecord(candidate) &&
      candidate.kind === "codex_app_server" &&
      candidate.desiredState === "enabled" &&
      candidate.observedState === "ready",
  );
  assert.ok(
    isRecord(runtime),
    "debug service must expose a ready Codex runtime",
  );
  assert.equal(typeof runtime.runtimeId, "string");
  return runtime.runtimeId;
}

async function writeMetadata(
  bindingId: string,
  expectedRevision: number,
  metadata: { label: string | null; taskRef: unknown },
): Promise<ApiResponse> {
  const response = await request(
    "POST",
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/metadata`,
    { expectedRevision, ...metadata },
  );
  if (metadata.label !== "stale metadata write") {
    assert.equal(response.status, 200, response.text);
  }
  return response;
}

async function assertBinding(
  bindingId: string,
  revision: number,
  label: string | null,
  taskRef: unknown,
): Promise<void> {
  const response = await request("GET", "/v1/external-bindings");
  assert.equal(response.status, 200, response.text);
  const bindings = nested(response.json, ["data", "bindings"]);
  assert.ok(Array.isArray(bindings), "binding list must be an array");
  const binding = bindings.find(
    (candidate) => isRecord(candidate) && candidate.bindingId === bindingId,
  );
  assert.ok(isRecord(binding), `binding ${bindingId} must survive restart`);
  assert.equal(binding.revision, revision);
  assert.equal(binding.label ?? null, label);
  assert.deepEqual(binding.taskRef ?? null, taskRef);
}

async function assertThreadName(
  currentRuntimeId: string,
  threadId: string,
  expectedName: string | null,
): Promise<void> {
  let cursor: string | undefined;
  let listed: Record<string, unknown> | undefined;
  for (let page = 0; page < 50 && listed === undefined; page += 1) {
    const list = await request(
      "GET",
      `/v1/external-runtimes/${encodeURIComponent(currentRuntimeId)}/threads?limit=100${cursor === undefined ? "" : `&cursor=${encodeURIComponent(cursor)}`}`,
    );
    assert.equal(list.status, 200, list.text);
    const items = nested(list.json, ["data", "items"]);
    assert.ok(Array.isArray(items), "thread list items must be an array");
    listed = items.find(
      (candidate) => isRecord(candidate) && candidate.threadId === threadId,
    ) as Record<string, unknown> | undefined;
    const nextCursor = nested(list.json, ["data", "nextCursor"]);
    cursor = typeof nextCursor === "string" ? nextCursor : undefined;
    if (cursor === undefined) break;
  }
  assert.ok(isRecord(listed), `thread ${threadId} must be listed`);
  assert.equal(listed.name ?? null, expectedName);

  const read = await request(
    "POST",
    `/v1/external-runtimes/${encodeURIComponent(currentRuntimeId)}/threads/read`,
    { threadId, includeTurns: false },
  );
  assert.equal(read.status, 200, read.text);
  assert.equal(nested(read.json, ["data", "thread", "name"]), expectedName);
}

async function restartDebugService(): Promise<void> {
  await execFileAsync("systemctl", [
    "--user",
    "restart",
    "rusty-crew-debug.service",
  ]);
}

async function waitForReady(): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const response = await request("GET", "/v1/admin/readyz").catch(
      () => undefined,
    );
    if (
      response?.status === 200 &&
      nested(response.json, ["data", "ready"]) === true
    ) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("debug service did not become ready within 60 seconds");
}

async function request(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
): Promise<ApiResponse> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let json: Record<string, unknown> = {};
  if (text !== "") {
    const parsed: unknown = JSON.parse(text);
    if (isRecord(parsed)) json = parsed;
  }
  return { status: response.status, text, json };
}

function nested(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const key of path) {
    if (!isRecord(current)) return undefined;
    current = current[key];
  }
  return current;
}

function requiredString(value: unknown, path: readonly string[]): string {
  const result = nested(value, path);
  assert.equal(typeof result, "string", `${path.join(".")} must be a string`);
  return result;
}

function requiredNumber(value: unknown, path: readonly string[]): number {
  const result = nested(value, path);
  assert.equal(typeof result, "number", `${path.join(".")} must be a number`);
  return result;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
