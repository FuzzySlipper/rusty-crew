import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile, rename, writeFile } from "node:fs/promises";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const serviceUnit = "rusty-crew-debug.service";
const providerAlias =
  process.env.RUSTY_CREW_COMPATIBILITY_CERT_PROVIDER_ALIAS ?? "deepseek-flash";
const serviceConfigPath =
  process.env.RUSTY_CREW_COMPATIBILITY_CERT_CONFIG ??
  "/home/system/rusty-crew-debug/config/service.json";
const firstWorkdir = "/home/dev/rusty-crew";
const secondWorkdir = "/home/dev/rusty-crew/ts/packages/brain-island";
const suffix = Date.now().toString(36);
const profileId = `task-6596-compatibility-${suffix}`;
let sessionId: string | undefined;
let configBeforeWorkspaceMutation: string | undefined;

assert.equal(new URL(baseUrl).port, "9348", "certification is debug-only");

try {
  await apiData("GET", "/v1/admin/healthz");
  const created = await apiData<Record<string, unknown>>(
    "POST",
    "/v1/admin/control/profiles",
    {
      profileId,
      displayName: `Task 6596 compatibility ${suffix}`,
      providerAlias,
      kind: "full",
      localToolProfileId: "full_agent",
      reason: "task 6596 live compatibility certification",
    },
  );
  sessionId = nestedString(created, "outcome", "result", "sessionId");
  assert.ok(sessionId, "profile creation must return a session id");
  assert.equal(nested(created, "outcome", "status"), "completed");
  await assertSessionWorkdir(sessionId, "/home");

  await setSessionWorkdir(sessionId, firstWorkdir);
  const first = await sendAndWait(
    sessionId,
    `Remember this exact fact: COMPATIBILITY_FACT_${suffix}. Reply with the marker.`,
    `task-6596:${suffix}:fact`,
  );
  assert.match(
    assistantSummary(first.events),
    new RegExp(`COMPATIBILITY_FACT_${suffix}`),
  );

  const profileRead = await apiData<Record<string, unknown>>(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/read`,
    {
      reason: "task 6596 read disposable profile before prompt refresh",
    },
  );
  const profileConfig = nestedRecord(
    profileRead,
    "outcome",
    "result",
    "profileConfig",
  );
  const profileUpdate = await apiData<Record<string, unknown>>(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/update/apply`,
    {
      profileConfig,
      soulMarkdown:
        "Preserve conversation context across compatible profile refreshes.",
      reason: "task 6596 benign profile prompt refresh",
    },
  );
  assert.equal(
    nested(profileUpdate, "outcome", "status"),
    "completed",
    JSON.stringify(profileUpdate, null, 2),
  );
  const rebuilt = await apiData<Record<string, unknown>>(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/rebuild-brain/apply`,
    { reason: "task 6596 apply refreshed profile facts" },
  );
  assert.equal(nested(rebuilt, "outcome", "status"), "completed");

  configBeforeWorkspaceMutation = await readFile(serviceConfigPath, "utf8");
  await setSessionWorkdir(sessionId, secondWorkdir);

  const second = await sendAndWait(
    sessionId,
    `Without tools, reply with both exact markers: COMPATIBILITY_FACT_${suffix} WORKSPACE_SWITCH_${suffix}.`,
    `task-6596:${suffix}:workspace`,
    first.cursor,
  );
  const secondSummary = assistantSummary(second.events);
  assert.match(secondSummary, new RegExp(`COMPATIBILITY_FACT_${suffix}`));
  assert.match(secondSummary, new RegExp(`WORKSPACE_SWITCH_${suffix}`));

  const beforeRestart = await providerDiagnostic(profileId, sessionId);
  const plannedTransitions = [
    beforeRestart,
    ...nestedArray(beforeRestart, "history"),
  ].filter((candidate) => nested(candidate, "compatibilityPlan") !== undefined);
  assert.ok(
    plannedTransitions.length > 0,
    JSON.stringify(beforeRestart, null, 2),
  );
  const plans = plannedTransitions.map((candidate) =>
    nestedRecord(candidate, "compatibilityPlan"),
  );
  for (const plan of plans) {
    assert.equal(plan.class, "compatible");
    assert.equal(plan.action, "preserve_lineage");
  }
  const changes = plans.flatMap((plan) => nestedArray(plan, "changes"));
  assert.ok(
    changes.some((change) => nested(change, "dimension") === "prompt"),
    JSON.stringify(plans, null, 2),
  );
  assert.ok(
    changes.some(
      (change) => nested(change, "dimension") === "session_workspace",
    ),
    JSON.stringify(plans, null, 2),
  );

  execFileSync("systemctl", ["--user", "restart", serviceUnit], {
    stdio: "inherit",
  });
  await waitForService();
  const third = await sendAndWait(
    sessionId,
    `After restart, reply with both exact markers: COMPATIBILITY_FACT_${suffix} RESTART_CONTINUITY_${suffix}.`,
    `task-6596:${suffix}:restart`,
    second.cursor,
  );
  const thirdSummary = assistantSummary(third.events);
  assert.match(thirdSummary, new RegExp(`COMPATIBILITY_FACT_${suffix}`));
  assert.match(thirdSummary, new RegExp(`RESTART_CONTINUITY_${suffix}`));

  console.log(
    JSON.stringify(
      {
        profileId,
        sessionId,
        providerAlias,
        workdirs: [firstWorkdir, secondWorkdir],
        compatibilityPlans: plans,
        restartContinuity: true,
      },
      null,
      2,
    ),
  );
} finally {
  if (configBeforeWorkspaceMutation !== undefined) {
    await writeConfigAtomically(configBeforeWorkspaceMutation);
    await reloadConfig().catch(() => undefined);
  }
  if (sessionId !== undefined) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task 6596 live certification cleanup",
      },
    ).catch(() => undefined);
  }
}

interface ChatEvent {
  event_id: string;
  kind: string;
  payload: Record<string, unknown>;
}

async function setSessionWorkdir(
  targetSessionId: string,
  workdir: string,
): Promise<void> {
  const text = await readFile(serviceConfigPath, "utf8");
  const config = JSON.parse(text) as Record<string, unknown>;
  const sessions = nestedArray(config, "sessions");
  const session = sessions.find(
    (candidate) => nested(candidate, "sessionId") === targetSessionId,
  );
  assert.ok(session && typeof session === "object" && !Array.isArray(session));
  const record = session as Record<string, unknown>;
  record.resourceLimits = {
    ...nestedRecord(record, "resourceLimits"),
    workdir,
  };
  await writeConfigAtomically(`${JSON.stringify(config, null, 2)}\n`);
  await reloadConfig();
  await assertSessionWorkdir(targetSessionId, workdir);
}

async function assertSessionWorkdir(
  targetSessionId: string,
  workdir: string,
): Promise<void> {
  const deadline = Date.now() + 10_000;
  let diagnostics: Record<string, unknown> = {};
  while (Date.now() < deadline) {
    diagnostics = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/admin/diagnostics/sessions?profile_id=${encodeURIComponent(profileId)}&limit=20`,
    );
    const current = nestedArray(diagnostics, "items").find(
      (candidate) => nested(candidate, "sessionId") === targetSessionId,
    );
    if (nested(current, "resourceLimits", "workdir") === workdir) return;
    await delay(100);
  }
  assert.fail(
    JSON.stringify({ targetSessionId, workdir, diagnostics }, null, 2),
  );
}

async function writeConfigAtomically(contents: string): Promise<void> {
  const temporaryPath = `${serviceConfigPath}.task-6596-${process.pid}.tmp`;
  await writeFile(temporaryPath, contents, "utf8");
  await rename(temporaryPath, serviceConfigPath);
}

async function reloadConfig(): Promise<void> {
  const result = await apiData<Record<string, unknown>>(
    "POST",
    "/v1/admin/control/config/reload",
    { reason: "task 6596 workspace-context certification" },
  );
  assert.equal(nested(result, "outcome", "status"), "completed");
}

async function sendAndWait(
  targetSessionId: string,
  body: string,
  clientMessageId: string,
  initialCursor = `${targetSessionId}:0`,
): Promise<{ cursor: string; events: ChatEvent[] }> {
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(targetSessionId)}/messages`,
    {
      actor: { id: "task-6596-certifier", kind: "human" },
      body,
      client_message_id: clientMessageId,
      reason: "task 6596 provider-state compatibility certification",
    },
    { "Idempotency-Key": clientMessageId },
  );
  assert.equal(sent.status, 202, sent.text);
  let cursor = initialCursor;
  const events: ChatEvent[] = [];
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(targetSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    for (const event of nestedArray(page, "items") as ChatEvent[]) {
      if (!events.some((seen) => seen.event_id === event.event_id))
        events.push(event);
    }
    cursor =
      typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    if (events.some((event) => event.kind === "assistant_turn_finished")) {
      assert.equal(
        events.some((event) => event.kind === "logical_turn_failed"),
        false,
        JSON.stringify(events, null, 2),
      );
      return { cursor, events };
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${targetSessionId}`);
}

async function providerDiagnostic(
  profile: string,
  targetSessionId: string,
): Promise<Record<string, unknown>> {
  const diagnostics = await apiData<unknown[]>(
    "GET",
    "/v1/admin/diagnostics/provider-state",
  );
  const result = diagnostics.find(
    (candidate) => nested(candidate, "profileId") === profile,
  );
  assert.ok(result && typeof result === "object" && !Array.isArray(result));
  const session = nestedArray(result, "providerState", "sessions").find(
    (candidate) => nested(candidate, "sessionId") === targetSessionId,
  );
  assert.ok(session && typeof session === "object" && !Array.isArray(session));
  return session as Record<string, unknown>;
}

function assistantSummary(events: ChatEvent[]): string {
  const completed = events.find(
    (event) => event.kind === "assistant_message_completed",
  );
  assert.equal(
    completed?.payload.status,
    "completed",
    JSON.stringify(events, null, 2),
  );
  return String(completed.payload.summary ?? "");
}

async function waitForService(): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      if ((await api("GET", "/v1/admin/healthz")).status === 200) return;
    } catch {
      // Expected while systemd restarts the debug service.
    }
    await delay(250);
  }
  throw new Error(`${serviceUnit} did not become healthy`);
}

async function apiData<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await api(method, path, body);
  assert.ok(response.status < 400, response.text);
  assert.equal(response.json.ok, true, response.text);
  return response.json.data as T;
}

async function api(
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
): Promise<{ status: number; text: string; json: Record<string, unknown> }> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(300_000),
  });
  const text = await response.text();
  let json: Record<string, unknown> = {};
  try {
    json = JSON.parse(text) as Record<string, unknown>;
  } catch {
    // Preserve raw text for assertions.
  }
  return { status: response.status, text, json };
}

function nested(value: unknown, ...path: string[]): unknown {
  let current = value;
  for (const key of path) {
    if (
      typeof current !== "object" ||
      current === null ||
      Array.isArray(current)
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function nestedArray(value: unknown, ...path: string[]): unknown[] {
  const result = nested(value, ...path);
  return Array.isArray(result) ? result : [];
}

function nestedRecord(
  value: unknown,
  ...path: string[]
): Record<string, unknown> {
  const result = nested(value, ...path);
  return typeof result === "object" && result !== null && !Array.isArray(result)
    ? (result as Record<string, unknown>)
    : {};
}

function nestedString(value: unknown, ...path: string[]): string | undefined {
  const result = nested(value, ...path);
  return typeof result === "string" ? result : undefined;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
