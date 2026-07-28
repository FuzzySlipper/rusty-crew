import assert from "node:assert/strict";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_EXTERNAL_RESTORE_PROVIDER_ALIAS ?? "tester-chat";
const cwd =
  process.env.RUSTY_CREW_EXTERNAL_RESTORE_CWD ?? "/home/dev/rusty-crew";
const suffix = `${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
const profileId = `external-restore-cert-${suffix}`;
let runtimeId: string | undefined;
let bindingId: string | undefined;
let nativeThreadId: string | undefined;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `external binding restore certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  await waitForReady();
  runtimeId = await readyRuntimeId();
  const createdProfile = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `External restore certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_agent",
    soulMarkdown:
      "You are a Rusty Crew external binding restore certification agent. Follow exact-output requests literally.",
    reason: "task 6321 live restore certification",
  });
  assert.equal(createdProfile.status, 200, createdProfile.text);
  assert.equal(
    nested(createdProfile.json, ["data", "outcome", "status"]),
    "completed",
  );

  const created = await request("POST", "/v1/external-agent-sessions", {
    idempotencyKey: `task-6321:${suffix}`,
    runtimeId,
    profileId,
    cwd,
    taskRef: { project_id: "rusty-crew", task_id: "6321" },
    label: `Restore cert ${suffix}`,
  });
  assert.equal(created.status, 200, created.text);
  bindingId = requiredString(created.json, [
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
  const sessionId = requiredString(created.json, [
    "data",
    "creation",
    "binding",
    "sessionId",
  ]);
  const agentId = requiredString(created.json, [
    "data",
    "creation",
    "binding",
    "agentId",
  ]);

  await completeTurn(
    bindingId,
    `Reply with exactly RESTORE_BEFORE_${suffix} and nothing else.`,
    "before",
  );

  const profileBefore = await readProfile(profileId);
  const archivedProfile = await applyProfileLifecycle(
    profileId,
    requiredNumber(profileBefore, ["revision"]),
    "archived",
  );
  assert.equal(nested(archivedProfile, ["lifecycleStatus"]), "archived");
  const archivedBinding = await readBinding(bindingId);
  assert.equal(nested(archivedBinding, ["status"]), "archived");
  assert.equal(nested(archivedBinding, ["nativeThreadId"]), nativeThreadId);

  const activatedProfile = await applyProfileLifecycle(
    profileId,
    requiredNumber(archivedProfile, ["revision"]),
    "active",
  );
  assert.equal(nested(activatedProfile, ["lifecycleStatus"]), "active");
  const archivedRevision = requiredNumber(archivedBinding, ["revision"]);

  const restored = await request(
    "POST",
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/restore`,
    {
      expectedBindingRevision: archivedRevision,
      expectedSessionId: sessionId,
      expectedAgentId: agentId,
      expectedProfileId: profileId,
      expectedNativeThreadId: nativeThreadId,
    },
  );
  assert.equal(restored.status, 200, restored.text);
  assert.equal(nested(restored.json, ["data", "outcome"]), "restored");
  assert.equal(
    nested(restored.json, ["data", "binding", "nativeThreadId"]),
    nativeThreadId,
  );
  assert.equal(
    nested(restored.json, ["data", "session", "sessionId"]),
    sessionId,
  );
  assert.equal(nested(restored.json, ["data", "profileRevisionUpdated"]), true);

  const restoredRevision = requiredNumber(restored.json, [
    "data",
    "binding",
    "revision",
  ]);
  const repeated = await request(
    "POST",
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/restore`,
    {
      expectedBindingRevision: restoredRevision,
      expectedSessionId: sessionId,
      expectedAgentId: agentId,
      expectedProfileId: profileId,
      expectedNativeThreadId: nativeThreadId,
    },
  );
  assert.equal(repeated.status, 200, repeated.text);
  assert.equal(nested(repeated.json, ["data", "outcome"]), "already_active");

  const terminal = await completeTurn(
    bindingId,
    `Reply with exactly RESTORE_AFTER_${suffix} and nothing else.`,
    "after",
  );
  assert.equal(nested(terminal, ["nativeThreadId"]), nativeThreadId);

  console.log(
    JSON.stringify(
      {
        baseUrl,
        runtimeId,
        profileId,
        bindingId,
        sessionId,
        agentId,
        nativeThreadId,
        archivedRevision,
        restoredRevision,
        exactIdentityPreserved: true,
        idempotentRetryVerified: true,
        liveTurnBeforeAndAfterRestore: true,
      },
      null,
      2,
    ),
  );
} finally {
  if (runtimeId !== undefined && nativeThreadId !== undefined) {
    await request(
      "POST",
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/${encodeURIComponent(nativeThreadId)}/delete`,
    ).catch(() => undefined);
  }
  await request(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
    { confirmProfileId: profileId, reason: "task 6321 certification cleanup" },
  ).catch(() => undefined);
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

async function completeTurn(
  currentBindingId: string,
  body: string,
  phase: string,
): Promise<Record<string, unknown>> {
  const deliveryId = `task-6321:${suffix}:${phase}`;
  const delivered = await request(
    "POST",
    `/v1/external-bindings/${encodeURIComponent(currentBindingId)}/messages`,
    {
      deliveryId,
      idempotencyKey: deliveryId,
      messageId: `${deliveryId}:message`,
      body,
      ttlMs: 60_000,
    },
  );
  assert.equal(delivered.status, 200, delivered.text);
  const requestId = requiredString(delivered.json, [
    "data",
    "activation",
    "requestId",
  ]);
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const receipt = await request(
      "GET",
      `/v1/external-turns/${encodeURIComponent(requestId)}`,
    );
    assert.equal(receipt.status, 200, receipt.text);
    const turn = nested(receipt.json, ["data"]);
    assert.ok(isRecord(turn));
    if (turn.phase === "completed") return turn;
    if (turn.phase === "failed" || turn.phase === "outcome_unknown") {
      throw new Error(
        `live ${phase} turn ended in ${String(turn.phase)}: ${receipt.text}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`live ${phase} turn did not complete within 300 seconds`);
}

async function readyRuntimeId(): Promise<string> {
  const response = await request("GET", "/v1/external-runtimes");
  assert.equal(response.status, 200, response.text);
  const runtimes = nested(response.json, ["data", "runtimes"]);
  assert.ok(Array.isArray(runtimes));
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

async function readProfile(
  currentProfileId: string,
): Promise<Record<string, unknown>> {
  const response = await request(
    "GET",
    `/v1/admin/profiles/registry/${encodeURIComponent(currentProfileId)}`,
  );
  assert.equal(response.status, 200, response.text);
  const profile = nested(response.json, ["data"]);
  assert.ok(isRecord(profile));
  return profile;
}

async function applyProfileLifecycle(
  currentProfileId: string,
  expectedRevision: number,
  lifecycleStatus: "active" | "archived",
): Promise<Record<string, unknown>> {
  const response = await request(
    "POST",
    `/v1/admin/profiles/registry/${encodeURIComponent(currentProfileId)}/lifecycle/apply`,
    { expectedRevision, lifecycleStatus },
  );
  assert.equal(response.status, 200, response.text);
  assert.equal(nested(response.json, ["data", "applied"]), true, response.text);
  const record = nested(response.json, ["data", "record"]);
  assert.ok(isRecord(record));
  return record;
}

async function readBinding(
  currentBindingId: string,
): Promise<Record<string, unknown>> {
  const response = await request("GET", "/v1/external-bindings");
  assert.equal(response.status, 200, response.text);
  const bindings = nested(response.json, ["data", "bindings"]);
  assert.ok(Array.isArray(bindings));
  const binding = bindings.find(
    (candidate) =>
      isRecord(candidate) && candidate.bindingId === currentBindingId,
  );
  assert.ok(isRecord(binding), `binding ${currentBindingId} must be listed`);
  return binding;
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
