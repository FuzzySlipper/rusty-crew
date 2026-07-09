import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const BASE_URL = (
  process.env.RUSTY_CREW_DEBUG_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const CONFIG_PATH =
  process.env.RUSTY_CREW_DEBUG_SERVICE_CONFIG ??
  "/home/system/rusty-crew-debug/config/service.json";
const PROFILE_ID =
  process.env.RUSTY_CREW_TIMEOUT_LIVE_PROFILE_ID ?? "asha-planner";
const SESSION_ID = process.env.RUSTY_CREW_TIMEOUT_LIVE_SESSION_ID;
const ACTOR_ID = "wake-timeout-live-smoke";
const SERVICE_TIMEOUT_MS = Number(
  process.env.RUSTY_CREW_TIMEOUT_LIVE_SERVICE_MS ?? 25,
);
const OVERRIDE_TIMEOUT_MS = Number(
  process.env.RUSTY_CREW_TIMEOUT_LIVE_OVERRIDE_MS ?? 60_000,
);
const TURN_TIMEOUT_MS = Number(
  process.env.RUSTY_CREW_TIMEOUT_LIVE_WAIT_MS ?? 120_000,
);

interface RuntimeConfig {
  wakeTimeout?: { mode: "disabled" } | { mode: "default"; defaultMs: number };
  sessions?: Array<Record<string, unknown> & { sessionId?: string }>;
  [key: string]: unknown;
}

interface ChatEvent {
  kind: string;
  cursor?: string;
  sequence_id?: number;
  payload?: Record<string, unknown>;
}

interface ScenarioResult {
  label: string;
  sessionId: string;
  beforeCursor: string;
  latestCursor: string;
  eventCount: number;
  wakeIds: string[];
  assistantCompleted?: Record<string, unknown>;
  turnFinished: boolean;
  elapsedMs: number;
}

const originalConfig = JSON.parse(
  await readFile(CONFIG_PATH, "utf8"),
) as RuntimeConfig;
let selectedSessionId = SESSION_ID;

try {
  selectedSessionId ??= await discoverSessionId();
  await applyRuntimeConfig(
    withSessionTurnTimeout(
      { ...originalConfig, wakeTimeout: { mode: "disabled" } },
      selectedSessionId,
      undefined,
    ),
  );
  await assertWakeTimeoutReadback(selectedSessionId, undefined);
  const disabled = await runChatScenario({
    label: "disabled",
    sessionId: selectedSessionId,
    body: "Live wake-timeout smoke disabled case: reply with exactly 'timeout disabled ok' and no tool calls.",
  });
  assert.equal(
    completionReason(disabled.assistantCompleted),
    undefined,
    "disabled timeout policy should not produce wake_timeout completion",
  );

  await applyRuntimeConfig(
    withSessionTurnTimeout(
      {
        ...originalConfig,
        wakeTimeout: { mode: "default", defaultMs: SERVICE_TIMEOUT_MS },
      },
      selectedSessionId,
      undefined,
    ),
  );
  await assertWakeTimeoutReadback(selectedSessionId, SERVICE_TIMEOUT_MS);
  const capped = await runChatScenario({
    label: "service-default",
    sessionId: selectedSessionId,
    body: "Live wake-timeout smoke default-cap case: write a thoughtful paragraph. This should be interrupted by the test timeout.",
  });
  assert.equal(completionStatus(capped.assistantCompleted), "failed");
  assert.equal(completionReason(capped.assistantCompleted), "wake_timeout");
  assert.match(
    String(capped.assistantCompleted?.summary ?? ""),
    new RegExp(`${SERVICE_TIMEOUT_MS}ms`),
  );

  await applyRuntimeConfig(
    withSessionTurnTimeout(
      {
        ...originalConfig,
        wakeTimeout: { mode: "default", defaultMs: SERVICE_TIMEOUT_MS },
      },
      selectedSessionId,
      OVERRIDE_TIMEOUT_MS,
    ),
  );
  await assertWakeTimeoutReadback(selectedSessionId, OVERRIDE_TIMEOUT_MS);
  const override = await runChatScenario({
    label: "profile-session-override",
    sessionId: selectedSessionId,
    body: "Live wake-timeout smoke override case: reply with exactly 'timeout override ok' and no tool calls.",
  });
  assert.notEqual(
    completionReason(override.assistantCompleted),
    "wake_timeout",
  );
  assert.equal(override.turnFinished, true);

  console.log(
    JSON.stringify(
      {
        baseUrl: BASE_URL,
        configPath: CONFIG_PATH,
        serviceTimeoutMs: SERVICE_TIMEOUT_MS,
        overrideTimeoutMs: OVERRIDE_TIMEOUT_MS,
        profileId: PROFILE_ID,
        sessionId: selectedSessionId,
        scenarios: [disabled, capped, override],
      },
      null,
      2,
    ),
  );
} finally {
  await applyRuntimeConfig(
    withSessionTurnTimeout(
      { ...originalConfig, wakeTimeout: { mode: "disabled" } },
      selectedSessionId,
      undefined,
    ),
  ).catch((error: unknown) => {
    console.error("failed to restore disabled wakeTimeout policy", error);
  });
}

async function discoverSessionId(): Promise<string> {
  const page = await getJson("/v1/chat/sessions?limit=100");
  const items = page.data?.items as Array<{
    session_id: string;
    profile_id: string;
    kind: string;
    status: string;
  }>;
  const session = items.find(
    (item) =>
      item.profile_id === PROFILE_ID &&
      item.kind === "full" &&
      item.status !== "archived",
  );
  if (!session) {
    throw new Error(`no active debug chat session found for ${PROFILE_ID}`);
  }
  return session.session_id;
}

async function applyRuntimeConfig(runtimeConfig: RuntimeConfig): Promise<void> {
  const response = await postJson("/v1/admin/control/config/draft/apply", {
    runtimeConfig,
  });
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.equal(
    response.data?.outcome?.result?.ok,
    true,
    JSON.stringify(response),
  );
}

function withSessionTurnTimeout(
  config: RuntimeConfig,
  sessionId: string | undefined,
  turnTimeoutMs: number | undefined,
): RuntimeConfig {
  if (sessionId === undefined) return config;
  const sessions = (config.sessions ?? []).map((session) => {
    if (session.sessionId !== sessionId) return { ...session };
    const next = { ...session };
    if (turnTimeoutMs === undefined) {
      delete next.turnTimeoutMs;
    } else {
      next.turnTimeoutMs = turnTimeoutMs;
    }
    return next;
  });
  if (!sessions.some((session) => session.sessionId === sessionId)) {
    sessions.push({ sessionId, turnTimeoutMs });
  }
  return { ...config, sessions };
}

async function assertWakeTimeoutReadback(
  sessionId: string,
  expected: number | undefined,
): Promise<void> {
  const page = await getJson("/v1/chat/sessions?limit=100");
  const session = (page.data?.items as Array<Record<string, unknown>>).find(
    (item) => item.session_id === sessionId,
  );
  assert.ok(session, `session ${sessionId} should be visible in chat readback`);
  const effective = session.effective_defaults as
    | { wakeTimeoutMs?: number }
    | undefined;
  assert.equal(effective?.wakeTimeoutMs, expected);
}

async function runChatScenario(input: {
  label: string;
  sessionId: string;
  body: string;
}): Promise<ScenarioResult> {
  const before = await getJson(`/v1/chat/sessions?limit=100`);
  const beforeSession = (
    before.data?.items as Array<Record<string, unknown>>
  ).find((item) => item.session_id === input.sessionId);
  const beforeCursor = String(beforeSession?.latest_cursor ?? "");
  const clientMessageId = `wake-timeout-${input.label}-${Date.now()}`;
  const started = Date.now();
  await postJson(
    `/v1/chat/sessions/${encodeURIComponent(input.sessionId)}/messages`,
    {
      actor: { id: ACTOR_ID, kind: "human", display_name: "Timeout Smoke" },
      body: input.body,
      client_message_id: clientMessageId,
      reason: `wake timeout live smoke ${input.label}`,
    },
    { "Idempotency-Key": clientMessageId },
  );
  const terminal = await waitForTerminalEvents(input.sessionId, beforeCursor);
  const events = terminal.events;
  const assistantCompleted = events.find(
    (event) => event.kind === "assistant_message_completed",
  )?.payload;
  return {
    label: input.label,
    sessionId: input.sessionId,
    beforeCursor,
    latestCursor: terminal.latestCursor,
    eventCount: events.length,
    wakeIds: Array.from(
      new Set(
        events
          .map((event) => String(event.payload?.wake_id ?? ""))
          .filter(Boolean),
      ),
    ),
    assistantCompleted,
    turnFinished: events.some(
      (event) => event.kind === "assistant_turn_finished",
    ),
    elapsedMs: Date.now() - started,
  };
}

async function waitForTerminalEvents(
  sessionId: string,
  cursor: string,
): Promise<{ events: ChatEvent[]; latestCursor: string }> {
  const deadline = Date.now() + TURN_TIMEOUT_MS;
  const collected: ChatEvent[] = [];
  let nextCursor = cursor;
  while (Date.now() < deadline) {
    const page = await getJson(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?limit=200`,
      { "Last-Event-ID": nextCursor },
    );
    const events = (page.data?.items ?? []) as ChatEvent[];
    if (events.length > 0) {
      collected.push(...events);
      nextCursor = String(page.data?.latest_cursor ?? nextCursor);
      const wakeIds = new Set(
        collected
          .map((event) => String(event.payload?.wake_id ?? ""))
          .filter(Boolean),
      );
      for (const wakeId of wakeIds) {
        const hasFinished = collected.some(
          (event) =>
            event.kind === "assistant_turn_finished" &&
            event.payload?.wake_id === wakeId,
        );
        const hasCompleted = collected.some(
          (event) =>
            event.kind === "assistant_message_completed" &&
            event.payload?.wake_id === wakeId,
        );
        if (hasFinished && hasCompleted) {
          return { events: collected, latestCursor: nextCursor };
        }
      }
    }
    await delay(250);
  }
  throw new Error(
    `timed out waiting for assistant_turn_finished after ${TURN_TIMEOUT_MS}ms`,
  );
}

function completionStatus(
  payload: Record<string, unknown> | undefined,
): string | undefined {
  return typeof payload?.status === "string" ? payload.status : undefined;
}

function completionReason(
  payload: Record<string, unknown> | undefined,
): string | undefined {
  return typeof payload?.reason_code === "string"
    ? payload.reason_code
    : undefined;
}

async function getJson(
  path: string,
  headers: Record<string, string> = {},
): Promise<Record<string, any>> {
  const response = await fetch(`${BASE_URL}${path}`, { headers });
  const json = (await response.json()) as Record<string, any>;
  assert.equal(response.ok, true, JSON.stringify(json));
  return json;
}

async function postJson(
  path: string,
  body: unknown,
  headers: Record<string, string> = {},
): Promise<Record<string, any>> {
  const response = await fetch(`${BASE_URL}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json", ...headers },
    body: JSON.stringify(body),
  });
  const json = (await response.json()) as Record<string, any>;
  assert.equal(
    response.ok || response.status === 409,
    true,
    JSON.stringify(json),
  );
  return json;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
