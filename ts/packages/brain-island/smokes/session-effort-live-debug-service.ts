import assert from "node:assert/strict";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_SESSION_EFFORT_CERT_PROVIDER_ALIAS ?? "tester-chat";
const requestedEffort =
  process.env.RUSTY_CREW_SESSION_EFFORT_CERT_VALUE ?? "low";
const suffix = Date.now().toString(36);
const profileId = `session-effort-cert-${suffix}`;
const firstMarker = `EFFORT_FIRST_${suffix.toUpperCase()}`;
const secondMarker = `EFFORT_SECOND_${suffix.toUpperCase()}`;
let profileCreated = false;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `session effort certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  await waitForHealth();
  const created = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Session effort certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "basic_chat",
    reason: "task-6583 live session effort certification",
  });
  assert.equal(created.status, 200, created.text);
  profileCreated = true;
  const sessionId = requiredString(created.json, [
    "data",
    "outcome",
    "result",
    "sessionId",
  ]);

  const before = await readSession(sessionId);
  const first = await sendAndWait(
    sessionId,
    `Reply with the exact marker ${firstMarker} and one short sentence.`,
    requiredString(before, ["latest_cursor"]),
  );
  assert.match(assistantText(first.events), new RegExp(firstMarker));
  const firstProviderRequests = await exactProviderRequests(
    sessionId,
    first.events,
  );
  assert.ok(
    firstProviderRequests.length > 0,
    "first turn must expose a captured provider request",
  );

  const command = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/commands`,
    {
      command: `/effort ${requestedEffort}`,
      actor: { id: "task-6583-cert-operator", kind: "human" },
    },
    { "Idempotency-Key": `effort-${suffix}` },
  );
  assert.equal(command.status, 200, command.text);
  assert.equal(nested(command.json, ["data", "status"]), "completed");
  assert.equal(nested(command.json, ["data", "command_name"]), "effort");
  assert.equal(
    nested(command.json, [
      "data",
      "response",
      "outcome",
      "result",
      "reasoningEffort",
    ]),
    requestedEffort,
  );
  assert.equal(
    nested(command.json, [
      "data",
      "response",
      "outcome",
      "result",
      "reasoningEffortSource",
    ]),
    "session_override",
  );

  const context = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/context`,
  );
  assert.equal(context.status, 200, context.text);
  assert.equal(
    nested(context.json, ["data", "provider", "reasoning_effort"]),
    requestedEffort,
  );
  assert.equal(
    nested(context.json, ["data", "provider", "reasoning_effort_source"]),
    "session_override",
  );

  const second = await sendAndWait(
    sessionId,
    `Recall the exact marker from your previous answer, then include it and ${secondMarker}. Keep the response short.`,
    first.cursor,
  );
  const secondText = assistantText(second.events);
  assert.match(secondText, new RegExp(firstMarker));
  assert.match(secondText, new RegExp(secondMarker));
  const secondProviderRequests = await exactProviderRequests(
    sessionId,
    second.events,
  );
  assert.ok(
    secondProviderRequests.some(
      (requestBody) => requestBody.reasoning_effort === requestedEffort,
    ),
    `second turn provider requests did not carry reasoning_effort=${requestedEffort}: ${JSON.stringify(secondProviderRequests)}`,
  );

  console.log(
    JSON.stringify(
      {
        baseUrl,
        providerAlias,
        profileId,
        sessionId,
        requestedEffort,
        commandReasoningEffort: nested(command.json, [
          "data",
          "response",
          "outcome",
          "result",
          "reasoningEffort",
        ]),
        firstProviderRequestCount: firstProviderRequests.length,
        secondProviderRequestCount: secondProviderRequests.length,
        continuity: true,
      },
      null,
      2,
    ),
  );
} finally {
  if (profileCreated) {
    const cleanup = await request(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task-6583 live session effort certification cleanup",
      },
    ).catch(() => undefined);
    if (cleanup !== undefined && cleanup.status >= 400) {
      console.error(`profile cleanup failed: ${cleanup.text}`);
    }
  }
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

interface ChatEvent extends Record<string, unknown> {
  kind: string;
  payload?: Record<string, unknown>;
}

async function sendAndWait(
  sessionId: string,
  body: string,
  initialCursor: string,
): Promise<{ cursor: string; events: ChatEvent[] }> {
  const key = `task-6583:${sessionId}:${Date.now()}`;
  const sent = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6583-cert-operator", kind: "human" },
      body,
      client_message_id: key,
      reason: "task-6583 live session effort certification",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);

  let cursor = initialCursor;
  const events: ChatEvent[] = [];
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const page = await request(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    assert.equal(page.status, 200, page.text);
    const items = nested(page.json, ["data", "items"]);
    if (Array.isArray(items))
      events.push(...(items.filter(isRecord) as ChatEvent[]));
    const latest = nested(page.json, ["data", "latest_cursor"]);
    if (typeof latest === "string") cursor = latest;
    const terminal = events.find(
      (event) => event.kind === "assistant_turn_finished",
    );
    if (terminal !== undefined) {
      assert.notEqual(
        terminal.payload?.status,
        "failed",
        JSON.stringify(events),
      );
      return { cursor, events };
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for session effort turn ${sessionId}`);
}

async function exactProviderRequests(
  sessionId: string,
  events: ChatEvent[],
): Promise<Record<string, unknown>[]> {
  const details: Record<string, unknown>[] = [];
  for (const debugDetailId of providerDebugIds(events)) {
    const response = await request(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/provider-requests/${encodeURIComponent(debugDetailId)}`,
    );
    if (response.status >= 400) continue;
    const detail = nested(response.json, ["data"]);
    if (!isRecord(detail)) continue;
    const requests = nested(detail, ["request", "value", "requests"]);
    if (Array.isArray(requests)) {
      details.push(...(requests.filter(isRecord) as Record<string, unknown>[]));
    }
  }
  return details;
}

function providerDebugIds(events: ChatEvent[]): string[] {
  const ids = new Set<string>();
  for (const event of events) {
    for (const candidate of [
      event.payload?.metadata,
      event.payload?.metadataJson,
      event.payload?.metadata_json,
    ]) {
      const metadata = parseObject(candidate);
      const id = metadata?.provider_request_debug_detail_id;
      if (typeof id === "string") ids.add(id);
    }
  }
  return [...ids];
}

function assistantText(events: ChatEvent[]): string {
  const deltas = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => String(event.payload?.text ?? ""))
    .join("");
  if (deltas.length > 0) return deltas;
  const completed = events.find(
    (event) => event.kind === "assistant_message_completed",
  );
  return String(completed?.payload?.body ?? completed?.payload?.text ?? "");
}

async function readSession(
  sessionId: string,
): Promise<Record<string, unknown>> {
  const response = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}`,
  );
  assert.equal(response.status, 200, response.text);
  const data = nested(response.json, ["data"]);
  assert.ok(isRecord(data));
  return data;
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const response = await request("GET", "/v1/admin/healthz").catch(
      () => undefined,
    );
    if (
      response?.status === 200 &&
      nested(response.json, ["data", "ok"]) === true
    ) {
      return;
    }
    await delay(250);
  }
  throw new Error("debug service did not become ready within 60 seconds");
}

async function request(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
): Promise<ApiResponse> {
  const token = process.env.RUSTY_CREW_ADMIN_TOKEN?.trim();
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...(token === undefined || token === ""
        ? {}
        : { authorization: `Bearer ${token}` }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(300_000),
  });
  const text = await response.text();
  const parsed: unknown = text.trim() === "" ? {} : JSON.parse(text);
  return {
    status: response.status,
    text,
    json: isRecord(parsed) ? parsed : {},
  };
}

function requiredString(value: unknown, path: readonly string[]): string {
  const found = nested(value, path);
  assert.equal(typeof found, "string", `${path.join(".")} must be a string`);
  assert.notEqual(found, "", `${path.join(".")} must not be empty`);
  return found as string;
}

function nested(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const key of path) {
    if (!isRecord(current)) return undefined;
    current = current[key];
  }
  return current;
}

function parseObject(value: unknown): Record<string, unknown> | undefined {
  if (isRecord(value)) return value;
  if (typeof value !== "string" || value.trim() === "") return undefined;
  try {
    const parsed: unknown = JSON.parse(value);
    return isRecord(parsed) ? parsed : undefined;
  } catch {
    return undefined;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
