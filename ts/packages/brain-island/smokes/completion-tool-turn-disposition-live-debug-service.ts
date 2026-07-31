import assert from "node:assert/strict";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_COMPLETION_CERT_PROVIDER_ALIAS ?? "deepseek-flash";
const suffix = Date.now().toString(36);
const profileId = `completion-disposition-cert-${suffix}`;
const marker = `COMPLETION_DISPOSITION_CERT_${suffix.toUpperCase()}`;
let profileCreated = false;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `completion disposition certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  await waitForHealth();
  const created = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Completion disposition certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_coding_agent",
    reason: "task-6438 completion disposition live certification",
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
  let cursor = requiredString(before, ["latest_cursor"]);
  const key = `task-6438-${suffix}`;
  const sentPromise = request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6438-cert-operator", kind: "human" },
      body: `You must call the deliver_completion_md tool exactly once and call no other tool. Pass this markdown exactly:\n---\nstatus: completed\n---\n\n## Summary\n\n${marker}\n\nDo not answer without making that tool call.`,
      client_message_id: key,
      reason: "task-6438 provider-backed completion disposition proof",
    },
    { "Idempotency-Key": key },
  );

  const events: ChatEvent[] = [];
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const page = await request(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    assert.equal(page.status, 200, page.text);
    const pageItems = nested(page.json, ["data", "items"]);
    if (Array.isArray(pageItems) && pageItems.length > 0) {
      events.push(...(pageItems.filter(isRecord) as ChatEvent[]));
      cursor = requiredString(page.json, ["data", "latest_cursor"]);
    }
    if (events.some((event) => event.kind === "assistant_turn_finished")) {
      break;
    }
    await delay(25);
  }

  const sent = await sentPromise;
  assert.equal(sent.status, 202, sent.text);

  const completionToolCalls = events.filter(
    (event) =>
      event.kind === "tool_call_completed" &&
      nested(event, ["payload", "tool_name"]) === "deliver_completion_md",
  );
  assert.equal(
    completionToolCalls.length,
    1,
    `expected one successful completion tool call: ${JSON.stringify(
      events.map((event) => ({ kind: event.kind, payload: event.payload })),
    )}`,
  );
  assert.notEqual(
    nested(completionToolCalls[0], ["payload", "is_error"]),
    true,
  );

  const completedMessages = events.filter(
    (event) => event.kind === "assistant_message_completed",
  );
  assert.equal(completedMessages.length, 1, JSON.stringify(completedMessages));
  assert.equal(
    nested(completedMessages[0], ["payload", "status"]),
    "completed",
  );
  assert.match(
    String(nested(completedMessages[0], ["payload", "summary"]) ?? ""),
    new RegExp(marker),
  );

  assert.equal(
    events.filter((event) => event.kind === "assistant_turn_finished").length,
    1,
  );
  assert.equal(
    events.filter((event) => event.kind === "logical_turn_completed").length,
    1,
  );
  assert.equal(
    events.filter((event) => event.kind === "logical_turn_failed").length,
    0,
  );
  assert.equal(
    events.filter(
      (event) =>
        nested(event, ["payload", "reason_code"]) === "external_gate_wait" ||
        nested(event, ["payload", "reason_code"]) ===
          "tool_requested_completion",
    ).length,
    0,
    "intentional native stop reason leaked into terminal chat failure",
  );

  const after = await readSession(sessionId);
  assert.equal(nested(after, ["session", "execution", "phase"]), "idle");
  assert.equal(
    nested(after, ["session", "execution", "lastOutcome"]),
    "completed",
  );

  console.log(
    JSON.stringify(
      {
        baseUrl,
        providerAlias,
        profileId,
        sessionId,
        completionToolCalls: completionToolCalls.length,
        assistantMessageCompleted: completedMessages.length,
        logicalTurnCompleted: 1,
        logicalTurnFailed: 0,
        marker,
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
        reason: "task-6438 live certification cleanup",
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
