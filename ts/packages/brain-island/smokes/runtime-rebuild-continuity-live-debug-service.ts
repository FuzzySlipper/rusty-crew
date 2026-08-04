import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const serviceUnit =
  process.env.RUSTY_CREW_DEBUG_SERVICE_UNIT ?? "rusty-crew-debug.service";
const providerAlias =
  process.env.RUSTY_CREW_REBUILD_CERT_PROVIDER_ALIAS ?? "deepseek-flash";
const evidenceRoot =
  process.env.RUSTY_CREW_REBUILD_CERT_EVIDENCE_ROOT ??
  "/home/system/rusty-crew-debug/evidence/task-6582";
const suffix = Date.now().toString(36);
const profileId = `task-6582-rebuild-${suffix}`;
let sessionId: string | undefined;
let originalProvider: Record<string, unknown> | undefined;
let providerUpdated = false;

assert.equal(new URL(baseUrl).port, "9348", "certification is debug-only");
assert.equal(serviceUnit, "rusty-crew-debug.service");

try {
  await apiData("GET", "/v1/admin/healthz");
  originalProvider = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}`,
  );
  assert.equal(originalProvider.protocol, "chat_completions");
  const originalEffort = originalProvider.reasoningEffort;
  const changedEffort = originalEffort === "medium" ? "high" : "medium";

  const created = await apiData<Record<string, unknown>>(
    "POST",
    "/v1/admin/control/profiles",
    {
      profileId,
      displayName: `Task 6582 rebuild continuity ${suffix}`,
      providerAlias,
      kind: "full",
      localToolProfileId: "full_agent",
      reason: "task 6582 live session rebuild certification",
    },
  );
  sessionId = nestedString(created, "outcome", "result", "sessionId");
  assert.ok(sessionId, "profile creation must return a session id");

  const first = await sendAndWait(
    sessionId,
    `Remember this exact fact for the next turn: REBUILD_FACT_${suffix}. Reply with the marker and a short confirmation.`,
    `task-6582:${suffix}:fact`,
  );
  assert.match(
    assistantSummary(first.events),
    new RegExp(`REBUILD_FACT_${suffix}`),
  );

  const currentProvider = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}`,
  );
  const updated = await apiData<Record<string, unknown>>(
    "PATCH",
    `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}?refresh=apply`,
    providerWriteBody(currentProvider, {
      reasoningEffort: changedEffort,
      expectedRevision: currentProvider.revision,
    }),
  );
  providerUpdated = true;
  assert.equal(nested(updated, "provider", "reasoningEffort"), changedEffort);

  const transition = await waitForTransition(sessionId, first.cursor);
  assert.equal(transition.payload.outcome, "reconstructed");
  assert.equal(transition.payload.action, "reconstruct");
  assert.equal(transition.payload.transition, "reconstructed");
  assert.equal(transition.payload.clearedSessions, 0);

  const second = await sendAndWait(
    sessionId,
    `Without tools, answer with one line containing both exact markers: REBUILD_FACT_${suffix} REBUILD_CONTINUITY_${suffix}. Do not omit either marker.`,
    `task-6582:${suffix}:continuity`,
    transition.cursor,
  );
  const secondSummary = assistantSummary(second.events);
  assert.match(secondSummary, new RegExp(`REBUILD_FACT_${suffix}`));
  assert.match(secondSummary, new RegExp(`REBUILD_CONTINUITY_${suffix}`));

  execFileSync("systemctl", ["--user", "restart", serviceUnit], {
    stdio: "inherit",
  });
  await waitForService();

  const third = await sendAndWait(
    sessionId,
    `After the service restart, answer with one line containing both exact markers: REBUILD_FACT_${suffix} RESTART_CONTINUITY_${suffix}. Do not omit either marker.`,
    `task-6582:${suffix}:restart`,
    second.cursor,
  );
  const thirdSummary = assistantSummary(third.events);
  assert.match(thirdSummary, new RegExp(`REBUILD_FACT_${suffix}`));
  assert.match(thirdSummary, new RegExp(`RESTART_CONTINUITY_${suffix}`));

  const diagnostics = await apiData<unknown[]>(
    "GET",
    "/v1/admin/diagnostics/provider-state",
  );
  const profileDiagnostic = diagnostics.find(
    (candidate) => nested(candidate, "profileId") === profileId,
  );
  assert.ok(profileDiagnostic, `missing provider diagnostics for ${profileId}`);
  assert.equal(
    nested(profileDiagnostic, "providerStateRebuild", "action"),
    "reconstruct",
  );

  const evidenceDirectory = `${evidenceRoot}/${suffix}`;
  await mkdir(evidenceDirectory, { recursive: true });
  await writeFile(
    `${evidenceDirectory}/live-results.json`,
    `${JSON.stringify(
      {
        baseUrl,
        serviceUnit,
        providerAlias,
        profileId,
        sessionId,
        originalEffort,
        changedEffort,
        transition: transition.payload,
        firstCursor: first.cursor,
        secondCursor: second.cursor,
        thirdCursor: third.cursor,
      },
      null,
      2,
    )}\n`,
    "utf8",
  );
  console.log(
    JSON.stringify(
      {
        baseUrl,
        serviceUnit,
        providerAlias,
        profileId,
        sessionId,
        originalEffort,
        changedEffort,
        transition: transition.payload,
        evidenceDirectory,
      },
      null,
      2,
    ),
  );
} finally {
  if (sessionId !== undefined) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task 6582 live certification cleanup",
      },
    ).catch(() => undefined);
  }
  if (providerUpdated && originalProvider !== undefined) {
    const current = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}`,
    );
    await apiData(
      "PATCH",
      `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}?refresh=apply`,
      providerWriteBody(originalProvider, {
        expectedRevision: current.revision,
      }),
    );
  }
}

interface ChatEvent {
  event_id: string;
  kind: string;
  payload: Record<string, unknown>;
}

interface ScenarioResult {
  cursor: string;
  events: ChatEvent[];
}

async function sendAndWait(
  currentSessionId: string,
  body: string,
  clientMessageId: string,
  initialCursor?: string,
): Promise<ScenarioResult> {
  const cursor = initialCursor ?? `${currentSessionId}:0`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/messages`,
    {
      actor: { id: "task-6582-certifier", kind: "human" },
      body,
      client_message_id: clientMessageId,
      reason: "task 6582 session rebuild continuity certification",
    },
    { "Idempotency-Key": clientMessageId },
  );
  assert.equal(sent.status, 202, sent.text);
  return waitForTurn(currentSessionId, cursor);
}

async function waitForTurn(
  currentSessionId: string,
  initialCursor: string,
): Promise<ScenarioResult> {
  let cursor = initialCursor;
  const events: ChatEvent[] = [];
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    for (const event of nestedArray(page, "items") as unknown as ChatEvent[]) {
      if (!events.some((seen) => seen.event_id === event.event_id)) {
        events.push(event);
      }
    }
    cursor =
      typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    const terminal = events.find(
      (event) => event.kind === "assistant_turn_finished",
    );
    if (terminal) {
      assert.equal(
        events.some(
          (event) =>
            event.kind === "logical_turn_failed" ||
            (event.kind === "assistant_message_completed" &&
              event.payload.status === "failed"),
        ),
        false,
        JSON.stringify(events, null, 2),
      );
      assert.ok(
        events.some(
          (event) =>
            event.kind === "assistant_message_completed" &&
            event.payload.status === "completed",
        ),
        JSON.stringify(events, null, 2),
      );
      return { cursor, events };
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${currentSessionId}`);
}

async function waitForTransition(
  currentSessionId: string,
  initialCursor: string,
): Promise<{ cursor: string; payload: Record<string, unknown> }> {
  let cursor = initialCursor;
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    const events = nestedArray(page, "items") as unknown as ChatEvent[];
    const transition = events.find(
      (event) => event.kind === "runtime_rebuild_transition",
    );
    cursor =
      typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    if (transition) return { cursor, payload: transition.payload };
    await delay(100);
  }
  throw new Error("timed out waiting for runtime rebuild transition");
}

function assistantSummary(events: ChatEvent[]): string {
  const completed = events.find(
    (event) => event.kind === "assistant_message_completed",
  );
  return String(completed?.payload.summary ?? "");
}

async function waitForService(): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      if ((await api("GET", "/v1/admin/healthz")).status === 200) return;
    } catch {
      // Expected while systemd is restarting the debug service.
    }
    await delay(250);
  }
  throw new Error(`${serviceUnit} did not become healthy`);
}

function providerWriteBody(
  provider: Record<string, unknown>,
  overrides: Record<string, unknown>,
): Record<string, unknown> {
  return {
    status: provider.status,
    protocol: provider.protocol,
    providerKind: provider.providerKind,
    displayName: provider.displayName,
    description: provider.description,
    baseUrl: provider.baseUrl,
    modelId: provider.modelId,
    contextWindowTokens: provider.contextWindowTokens,
    maxOutputTokens: provider.maxOutputTokens,
    temperatureMilli: provider.temperatureMilli,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    responsesDialect: provider.responsesDialect,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    metadataJson: provider.metadataJson,
    expectedRevision: provider.revision,
    ...overrides,
  };
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
    // Keep raw text for the assertion.
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

function nestedString(value: unknown, ...path: string[]): string | undefined {
  const result = nested(value, ...path);
  return typeof result === "string" ? result : undefined;
}

function nestedArray(value: unknown, ...path: string[]): unknown[] {
  const result = nested(value, ...path);
  return Array.isArray(result) ? result : [];
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
