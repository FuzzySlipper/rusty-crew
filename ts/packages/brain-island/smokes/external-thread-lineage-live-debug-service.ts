import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const profileId =
  process.env.RUSTY_CREW_THREAD_LINEAGE_PROFILE_ID ?? "rv-codex-5516-a";
const cwd = process.env.RUSTY_CREW_THREAD_LINEAGE_CWD ?? "/home/dev/rusty-crew";
const suffix = `${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
let runtimeId: string | undefined;
let predecessorThreadId: string | undefined;
let successorThreadId: string | undefined;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(`thread lineage certification is debug-only: ${baseUrl}`);
}

try {
  await waitForReady();
  runtimeId = await readyRuntimeId();
  const created = await post("/v1/external-agent-sessions", {
    idempotencyKey: `task-6668:${suffix}`,
    runtimeId,
    profileId,
    cwd,
    taskRef: { project_id: "rusty-crew", task_id: "6668" },
    label: `Thread lineage cert ${suffix}`,
  });
  const predecessorBinding = recordAt(created, ["creation", "binding"]);
  const predecessorBindingId = requiredString(predecessorBinding, "bindingId");
  const predecessorSessionId = requiredString(predecessorBinding, "sessionId");
  predecessorThreadId = requiredString(predecessorBinding, "nativeThreadId");

  await deliverTurn(
    predecessorBindingId,
    `${suffix}:predecessor:1`,
    "LINEAGE_ALPHA",
  );
  await deliverTurn(
    predecessorBindingId,
    `${suffix}:predecessor:2`,
    "LINEAGE_BETA",
  );
  const populated = await readThread(runtimeId, predecessorThreadId);
  assert.ok(
    populated.turns.length >= 2,
    "predecessor did not materialize two turns",
  );

  const replacementResult = await post(
    `/v1/external-bindings/${encodeURIComponent(predecessorBindingId)}/commands`,
    { input: "/new", idempotencyKey: `task-6668:${suffix}:new` },
  );
  assert.equal(replacementResult.status, "applied");
  const replacement = recordAt(replacementResult, [
    "result",
    "threadReplacement",
  ]);
  const transitionId = requiredString(replacementResult, "commandId");
  const successorBindingId = requiredString(replacement, "bindingId");
  const successorSessionId = requiredString(replacement, "sessionId");
  successorThreadId = requiredString(replacement, "nativeThreadId");
  assert.notEqual(successorBindingId, predecessorBindingId);
  assert.notEqual(successorSessionId, predecessorSessionId);
  assert.notEqual(successorThreadId, predecessorThreadId);
  assert.equal(replacement.previousBindingId, predecessorBindingId);
  assert.equal(replacement.previousSessionId, predecessorSessionId);
  assert.equal(replacement.previousNativeThreadId, predecessorThreadId);
  assert.equal(replacement.previousNativeThreadArchived, false);

  await assertDurableLineage({
    runtimeId,
    predecessorBindingId,
    predecessorSessionId,
    predecessorThreadId,
    successorBindingId,
    successorSessionId,
    successorThreadId,
    transitionId,
    predecessorTurnFloor: populated.turns.length,
    successorTurnCount: 0,
  });

  await restartDebugService();
  await waitForReady();
  await assertDurableLineage({
    runtimeId,
    predecessorBindingId,
    predecessorSessionId,
    predecessorThreadId,
    successorBindingId,
    successorSessionId,
    successorThreadId,
    transitionId,
    predecessorTurnFloor: populated.turns.length,
    successorTurnCount: 0,
  });

  await deliverTurn(
    successorBindingId,
    `${suffix}:successor:1`,
    "LINEAGE_GAMMA",
  );
  const successorAfterTurn = await readThread(runtimeId, successorThreadId);
  assert.equal(successorAfterTurn.turns.length, 1);
  const predecessorAfterSuccessorTurn = await readThread(
    runtimeId,
    predecessorThreadId,
  );
  assert.ok(
    predecessorAfterSuccessorTurn.turns.length >= populated.turns.length,
  );

  console.log(
    JSON.stringify({
      baseUrl,
      runtimeId,
      profileId,
      predecessorBindingId,
      predecessorSessionId,
      predecessorThreadId,
      predecessorTurns: predecessorAfterSuccessorTurn.turns.length,
      successorBindingId,
      successorSessionId,
      successorThreadId,
      successorTurns: successorAfterTurn.turns.length,
      restartHydrationVerified: true,
    }),
  );
} finally {
  if (runtimeId !== undefined) {
    for (const threadId of [successorThreadId, predecessorThreadId]) {
      if (threadId === undefined) continue;
      await post(
        `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/${encodeURIComponent(threadId)}/delete`,
        undefined,
      ).catch((error) =>
        console.error(
          `thread lineage cleanup failed for ${threadId}: ${String(error)}`,
        ),
      );
    }
  }
}

async function assertDurableLineage(input: {
  runtimeId: string;
  predecessorBindingId: string;
  predecessorSessionId: string;
  predecessorThreadId: string;
  successorBindingId: string;
  successorSessionId: string;
  successorThreadId: string;
  transitionId: string;
  predecessorTurnFloor: number;
  successorTurnCount: number;
}): Promise<void> {
  const bindings = await get("/v1/external-bindings");
  const items = arrayAt(bindings, ["bindings"]);
  const predecessor = items.find(
    (candidate) =>
      isRecord(candidate) && candidate.bindingId === input.predecessorBindingId,
  );
  const successor = items.find(
    (candidate) =>
      isRecord(candidate) && candidate.bindingId === input.successorBindingId,
  );
  assert.ok(isRecord(predecessor));
  assert.ok(isRecord(successor));
  assert.equal(predecessor.sessionId, input.predecessorSessionId);
  assert.equal(predecessor.nativeThreadId, input.predecessorThreadId);
  assert.equal(predecessor.status, "active");
  assert.equal(successor.sessionId, input.successorSessionId);
  assert.equal(successor.nativeThreadId, input.successorThreadId);
  assert.deepEqual(successor.lineage, {
    predecessorBindingId: input.predecessorBindingId,
    predecessorSessionId: input.predecessorSessionId,
    predecessorNativeThreadId: input.predecessorThreadId,
    transitionId: input.transitionId,
    reasonCode: "external_command_new_session",
    createdAt: predecessor.updatedAt,
  });

  const listed = await get(
    `/v1/external-runtimes/${encodeURIComponent(input.runtimeId)}/threads?limit=100&archived=false`,
  );
  const threadIds = new Set(
    arrayAt(listed, ["items"])
      .filter(isRecord)
      .map((thread) => thread.threadId),
  );
  assert.ok(threadIds.has(input.predecessorThreadId));
  assert.ok(threadIds.has(input.successorThreadId));
  assert.ok(
    (await readThread(input.runtimeId, input.predecessorThreadId)).turns
      .length >= input.predecessorTurnFloor,
  );
  assert.equal(
    (await readThread(input.runtimeId, input.successorThreadId)).turns.length,
    input.successorTurnCount,
  );
}

async function deliverTurn(
  bindingId: string,
  id: string,
  marker: string,
): Promise<void> {
  const delivered = await post(
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/messages`,
    {
      deliveryId: id,
      idempotencyKey: id,
      messageId: `${id}:message`,
      body: `Reply with exactly ${marker} and nothing else.`,
      ttlMs: 120_000,
    },
  );
  const requestId = requiredString(
    recordAt(delivered, ["activation"]),
    "requestId",
  );
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const turn = await get(
      `/v1/external-turns/${encodeURIComponent(requestId)}`,
    );
    if (turn.phase === "completed") return;
    if (
      ["failed", "interrupted", "outcome_unknown"].includes(String(turn.phase))
    ) {
      throw new Error(`live turn ${requestId} ended in ${String(turn.phase)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`live turn ${requestId} timed out`);
}

async function readThread(runtime: string, threadId: string) {
  const result = await post(
    `/v1/external-runtimes/${encodeURIComponent(runtime)}/threads/read`,
    { threadId, includeTurns: true },
  );
  return recordAt(result, ["thread"]) as { turns: unknown[] } & Record<
    string,
    unknown
  >;
}

async function readyRuntimeId(): Promise<string> {
  const runtimes = arrayAt(await get("/v1/external-runtimes"), ["runtimes"]);
  const runtime = runtimes.find(
    (candidate) =>
      isRecord(candidate) &&
      candidate.kind === "codex_app_server" &&
      candidate.desiredState === "enabled" &&
      candidate.observedState === "ready",
  );
  assert.ok(isRecord(runtime));
  return requiredString(runtime, "runtimeId");
}

async function restartDebugService(): Promise<void> {
  await execFileAsync("systemctl", [
    "--user",
    "restart",
    "rusty-crew-debug.service",
  ]);
}

async function waitForReady(): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${baseUrl}/v1/admin/healthz`);
      if (response.ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("debug service did not become ready");
}

async function get(path: string): Promise<Record<string, unknown>> {
  return request(path, "GET");
}

async function post(
  path: string,
  body: unknown,
): Promise<Record<string, unknown>> {
  return request(path, "POST", body);
}

async function request(
  path: string,
  method: "GET" | "POST",
  body?: unknown,
): Promise<Record<string, unknown>> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers:
      body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const envelope = (await response.json()) as Record<string, unknown>;
  if (!response.ok || envelope.ok !== true || !isRecord(envelope.data)) {
    throw new Error(
      `${method} ${path} failed (${response.status}): ${JSON.stringify(envelope)}`,
    );
  }
  return envelope.data;
}

function recordAt(
  value: Record<string, unknown>,
  path: readonly string[],
): Record<string, unknown> {
  let current: unknown = value;
  for (const key of path)
    current = isRecord(current) ? current[key] : undefined;
  assert.ok(isRecord(current), `expected object at ${path.join(".")}`);
  return current;
}

function arrayAt(
  value: Record<string, unknown>,
  path: readonly string[],
): unknown[] {
  let current: unknown = value;
  for (const key of path)
    current = isRecord(current) ? current[key] : undefined;
  assert.ok(Array.isArray(current), `expected array at ${path.join(".")}`);
  return current;
}

function requiredString(value: Record<string, unknown>, key: string): string {
  assert.equal(typeof value[key], "string", `expected string ${key}`);
  return value[key] as string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
