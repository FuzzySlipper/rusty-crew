import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const initialProviderAlias =
  process.env.RUSTY_CREW_MECHANIC_INITIAL_PROVIDER_ALIAS ?? "tester-chat";
const overrideProviderAlias =
  process.env.RUSTY_CREW_MECHANIC_PROVIDER_ALIAS ?? "deepseek-flash";
const suffix = Date.now().toString(36);
const profileId = `mechanic-cert-${suffix}`;
const marker = `MECHANIC_CERT_${suffix.toUpperCase()}`;
const restartMarker = `${marker}_RESTARTED`;
const turnTimeoutMs = 180_000;
let sessionId: string | undefined;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `mechanic live certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  const health = await request("GET", "/v1/admin/healthz");
  assert.equal(health.status, 200, health.text);

  const created = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Mechanic certification ${suffix}`,
    providerAlias: initialProviderAlias,
    kind: "full",
    localToolProfileId: "basic_chat",
    reason: "task-5688 live mechanic profile certification",
  });
  assert.equal(created.status, 200, created.text);
  sessionId = nestedString(created.json, [
    "data",
    "outcome",
    "result",
    "sessionId",
  ]);
  assert.ok(sessionId, "profile creation must return its derived session id");

  const configured = await request(
    "PATCH",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(profileId)}/mechanic-config`,
    {
      name: "Maren",
      providerAlias: overrideProviderAlias,
      autoMonitor: false,
    },
  );
  assert.equal(configured.status, 200, configured.text);
  assert.equal(nested(configured.json, ["data", "config", "name"]), "Maren");
  assert.equal(
    nested(configured.json, ["data", "config", "providerAlias"]),
    overrideProviderAlias,
  );
  assert.equal(
    nested(configured.json, ["data", "localToolProfileId"]),
    "roleplay_mechanic",
  );
  assert.equal(
    nested(configured.json, ["data", "config", "autoMonitor", "available"]),
    false,
  );

  const firstTurn = await runMechanicTurn(sessionId, marker);
  assertMechanicTurn(firstTurn, marker);

  await restartDebugService();
  await waitForHealth();

  const readback = await request(
    "GET",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(profileId)}/mechanic-config`,
  );
  assert.equal(readback.status, 200, readback.text);
  assert.equal(nested(readback.json, ["data", "configured"]), true);
  assert.equal(nested(readback.json, ["data", "config", "name"]), "Maren");
  assert.equal(
    nested(readback.json, ["data", "config", "providerAlias"]),
    overrideProviderAlias,
  );
  assert.equal(nested(readback.json, ["data", "toolPolicyIsolated"]), true);

  const profileReadback = await request(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/read`,
    {},
  );
  assert.equal(profileReadback.status, 200, profileReadback.text);
  assert.match(profileReadback.text, /Maren/);
  assert.match(profileReadback.text, new RegExp(overrideProviderAlias));
  assert.match(profileReadback.text, /roleplay_mechanic/);

  const restartedTurn = await runMechanicTurn(sessionId, restartMarker);
  assertMechanicTurn(restartedTurn, restartMarker);

  console.log(
    JSON.stringify(
      {
        baseUrl,
        profileId,
        sessionId,
        initialProviderAlias,
        overrideProviderAlias,
        firstWake: firstTurn.summary,
        restartedWake: restartedTurn.summary,
        restartReadback: {
          name: nested(readback.json, ["data", "config", "name"]),
          providerAlias: nested(readback.json, [
            "data",
            "config",
            "providerAlias",
          ]),
          localToolProfileId: nested(readback.json, [
            "data",
            "localToolProfileId",
          ]),
        },
      },
      null,
      2,
    ),
  );
} finally {
  if (sessionId !== undefined) {
    const cleanup = await request(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task-5688 live mechanic profile certification cleanup",
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

interface ChatEvent {
  kind: string;
  event_id?: string;
  payload?: Record<string, unknown>;
}

interface TurnResult {
  events: ChatEvent[];
  text: string;
  summary: {
    eventCount: number;
    completedTools: string[];
    toolEvents: Array<{ kind: string; payload?: Record<string, unknown> }>;
    assistantCharacters: number;
    assistantPreview: string;
  };
}

async function runMechanicTurn(
  currentSessionId: string,
  expectedMarker: string,
): Promise<TurnResult> {
  const session = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}`,
  );
  assert.equal(session.status, 200, session.text);
  const cursor =
    nestedString(session.json, ["data", "session", "latest_cursor"]) ??
    nestedString(session.json, ["data", "latest_cursor"]);
  const messageKey = `mechanic-cert:${expectedMarker}`;
  const sent = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/messages`,
    {
      actor: { id: "mechanic-cert-operator", kind: "human" },
      body: [
        "Call get_mechanic_capabilities before answering.",
        `Then include the exact marker ${expectedMarker}.`,
        "In one short paragraph say whether you are a narrator or an environmental diagnostician and how durable changes are made.",
      ].join("\n"),
      client_message_id: messageKey,
      reason: "task-5688 live mechanic profile certification",
    },
    { "Idempotency-Key": messageKey },
  );
  assert.equal(sent.status, 202, sent.text);
  const events = await waitForTurn(currentSessionId, cursor);
  const text = assistantText(events);
  const completedTools = events
    .filter(
      (event) =>
        event.kind === "tool_call_completed" &&
        event.payload?.is_error !== true,
    )
    .map((event) =>
      String(event.payload?.tool_name ?? event.payload?.toolName ?? ""),
    )
    .filter(Boolean);
  const toolEvents = events
    .filter((event) => event.kind.startsWith("tool_call_"))
    .map((event) => ({ kind: event.kind, payload: event.payload }));
  return {
    events,
    text,
    summary: {
      eventCount: events.length,
      completedTools,
      toolEvents,
      assistantCharacters: text.length,
      assistantPreview: text.slice(0, 500),
    },
  };
}

function assertMechanicTurn(turn: TurnResult, expectedMarker: string): void {
  const failed = turn.events.find(
    (event) =>
      event.kind === "assistant_turn_finished" &&
      event.payload?.status === "failed",
  );
  assert.equal(failed, undefined, JSON.stringify(failed));
  assert.ok(
    turn.summary.completedTools.includes("get_mechanic_capabilities"),
    JSON.stringify(turn.summary),
  );
  assert.match(turn.text, new RegExp(expectedMarker));
  assert.match(turn.text, /diagnostician/i);
  assert.match(turn.text, /proposal|approval|review/i);
}

async function waitForTurn(
  currentSessionId: string,
  cursor: string | undefined,
): Promise<ChatEvent[]> {
  const deadline = Date.now() + turnTimeoutMs;
  const events: ChatEvent[] = [];
  let nextCursor = cursor;
  while (Date.now() < deadline) {
    const url = new URL(
      `${baseUrl}/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events`,
    );
    url.searchParams.set("limit", "500");
    if (nextCursor !== undefined) url.searchParams.set("cursor", nextCursor);
    const page = await request("GET", `${url.pathname}${url.search}`);
    assert.equal(page.status, 200, page.text);
    const items = nested(page.json, ["data", "items"]);
    if (Array.isArray(items) && items.length > 0) {
      events.push(...(items as ChatEvent[]));
      nextCursor = nestedString(page.json, ["data", "latest_cursor"]);
      if (
        events.some((event) => event.kind === "assistant_turn_finished") &&
        events.some((event) => event.kind === "assistant_message_completed")
      ) {
        return events;
      }
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for mechanic wake in ${currentSessionId}`);
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

async function restartDebugService(): Promise<void> {
  await execFileAsync("systemctl", [
    "--user",
    "restart",
    "rusty-crew-debug.service",
  ]);
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const response = await request("GET", "/v1/admin/healthz").catch(
      () => undefined,
    );
    if (response?.status === 200) return;
    await delay(250);
  }
  throw new Error("debug service did not become healthy after restart");
}

async function request(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
): Promise<ApiResponse> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let json: Record<string, unknown> = {};
  try {
    json = JSON.parse(text) as Record<string, unknown>;
  } catch {
    // Preserve the raw body in assertion messages.
  }
  return { status: response.status, text, json };
}

function nested(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const part of path) {
    if (
      current === null ||
      typeof current !== "object" ||
      Array.isArray(current)
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function nestedString(
  value: unknown,
  path: readonly string[],
): string | undefined {
  const found = nested(value, path);
  return typeof found === "string" ? found : undefined;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
