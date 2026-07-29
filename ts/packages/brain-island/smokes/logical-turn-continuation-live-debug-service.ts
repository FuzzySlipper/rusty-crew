import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/$/, "");
const serviceUnit =
  process.env.RUSTY_CREW_DEBUG_SERVICE_UNIT ?? "rusty-crew-debug.service";
const chatProvider =
  process.env.RUSTY_CREW_LONG_TURN_CHAT_PROVIDER ?? "tester-chat";
const responsesProvider =
  process.env.RUSTY_CREW_LONG_TURN_RESPONSES_PROVIDER ??
  "responses-proxy-cert-5389";
const evidenceRoot =
  process.env.RUSTY_CREW_LONG_TURN_EVIDENCE_ROOT ??
  "/home/system/rusty-crew-debug/evidence/task-6371";
const suffix = Date.now().toString(36);
const forbiddenReasonCodes = [
  "chat_completions_continuation_limit_exceeded",
  "responses_continuation_limit_exceeded",
];
const profiles: string[] = [];

assert.match(baseUrl, /(?:127\.0\.0\.1|localhost):9348$/);
assert.equal(serviceUnit, "rusty-crew-debug.service");

try {
  await waitForService();
  const responses = await runCompletionScenario({
    label: "responses-restart-resume",
    providerAlias: responsesProvider,
    restartAfterFirstYield: true,
  });
  const chat = await runCompletionScenario({
    label: "chat-completions-multi-yield",
    providerAlias: chatProvider,
    restartAfterFirstYield: false,
  });
  const cancellation = await runCancellationScenario();
  const evidence = {
    schemaVersion: "task-6371-live-v1",
    generatedAt: new Date().toISOString(),
    baseUrl,
    serviceUnit,
    configuredSchedulingQuantum: 1,
    scenarios: [responses, chat, cancellation],
    forbiddenReasonCodes,
  };
  const evidenceDirectory = `${evidenceRoot}/${suffix}`;
  await mkdir(evidenceDirectory, { recursive: true });
  await writeFile(
    `${evidenceDirectory}/live-provider-results.json`,
    `${JSON.stringify(evidence, null, 2)}\n`,
    "utf8",
  );
  console.log(JSON.stringify({ ...evidence, evidenceDirectory }, null, 2));
} finally {
  for (const profileId of profiles.reverse()) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task 6371 live certification cleanup",
      },
    ).catch((error: unknown) => {
      console.error(`profile cleanup failed for ${profileId}`, error);
    });
  }
}

interface ChatEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
  payload: Record<string, unknown>;
}

interface CompletionScenario {
  label: string;
  providerAlias: string;
  profileId: string;
  sessionId: string;
  logicalTurnId: string;
  continuationCount: number;
  yieldingEvents: number;
  queuedEvents: number;
  completedTools: string[];
  providerRequestTotal: number;
  toolRoundTotal: number;
  terminalEvents: number;
  serviceRestarted: boolean;
  eventCount: number;
  latestCursor: string;
}

async function runCompletionScenario(input: {
  label: string;
  providerAlias: string;
  restartAfterFirstYield: boolean;
}): Promise<CompletionScenario> {
  const { profileId, sessionId } = await createProfile(
    input.label,
    input.providerAlias,
  );
  const beforeCursor = await latestCursor(sessionId);
  const messageKey = `task-6371-${input.label}-${suffix}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: {
        id: "task-6371-certifier",
        kind: "human",
        display_name: "Long-turn certifier",
      },
      body: sequentialToolPrompt(input.label),
      client_message_id: `message:${messageKey}`,
      reason: "task 6371 live continuation certification",
    },
    { "Idempotency-Key": messageKey },
  );
  assert.equal(sent.status, 202, sent.text);

  let serviceRestarted = false;
  const completed = await waitForEvents(
    sessionId,
    beforeCursor,
    async (events) => {
      if (
        input.restartAfterFirstYield &&
        !serviceRestarted &&
        events.some((event) => event.kind === "logical_turn_queued_to_continue")
      ) {
        serviceRestarted = true;
        restartDebugService();
        await waitForService();
      }
      return events.some(
        (event) => event.kind === "assistant_message_completed",
      );
    },
  );
  const events = completed.events;
  const yielding = events.filter(
    (event) => event.kind === "logical_turn_yielding",
  );
  const queued = events.filter(
    (event) => event.kind === "logical_turn_queued_to_continue",
  );
  const terminal = events.filter(
    (event) => event.kind === "assistant_message_completed",
  );
  const completedTools = events.filter(
    (event) =>
      event.kind === "tool_call_completed" && event.payload.is_error !== true,
  );

  assert.ok(yielding.length >= 2, `${input.label} must cross two yields`);
  assert.equal(queued.length, yielding.length);
  assert.ok(
    completedTools.length >= 3,
    `${input.label} must complete at least three real tool calls`,
  );
  assert.equal(terminal.length, 1, `${input.label} must terminate once`);
  assert.equal(terminal[0]?.payload.status, "completed");
  assert.equal(
    new Set(events.map((event) => event.event_id)).size,
    events.length,
  );
  assert.equal(
    new Set(
      completedTools.map((event) =>
        String(event.payload.tool_call_id ?? event.payload.call_id ?? ""),
      ),
    ).size,
    completedTools.length,
    `${input.label} must not complete one tool call twice`,
  );
  assertNoLegacyCeiling(events);

  const diagnostic = await logicalTurnDiagnostic(sessionId, true);
  assert.equal(diagnostic.operatorState, "completed");
  assert.ok(Number(diagnostic.continuationCount) >= yielding.length + 1);
  assert.ok(Number(diagnostic.toolRoundTotal) >= 3);
  assert.ok(Number(diagnostic.providerRequestTotal) >= 4);

  return {
    label: input.label,
    providerAlias: input.providerAlias,
    profileId,
    sessionId,
    logicalTurnId: String(diagnostic.logicalTurnId),
    continuationCount: Number(diagnostic.continuationCount),
    yieldingEvents: yielding.length,
    queuedEvents: queued.length,
    completedTools: completedTools.map((event) =>
      String(event.payload.tool_name ?? "unknown"),
    ),
    providerRequestTotal: Number(diagnostic.providerRequestTotal),
    toolRoundTotal: Number(diagnostic.toolRoundTotal),
    terminalEvents: terminal.length,
    serviceRestarted,
    eventCount: events.length,
    latestCursor: completed.latestCursor,
  };
}

async function runCancellationScenario(): Promise<Record<string, unknown>> {
  const label = "chat-completions-operator-cancel";
  const { profileId, sessionId } = await createProfile(label, chatProvider);
  const beforeCursor = await latestCursor(sessionId);
  const messageKey = `task-6371-${label}-${suffix}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6371-certifier", kind: "human" },
      body: [
        "Work slowly and call one tool at a time.",
        "Run terminal with `sleep 5; git status --short` in /home/dev/rusty-crew.",
        "After that completes, call read_file on /home/dev/rusty-crew/README.md.",
        "Then call git_status on /home/dev/rusty-crew and summarize.",
      ].join("\n"),
      client_message_id: `message:${messageKey}`,
      reason: "task 6371 live cancellation certification",
    },
    { "Idempotency-Key": messageKey },
  );
  assert.equal(sent.status, 202, sent.text);

  const running = await waitForLogicalTurnState(sessionId, [
    "running",
    "queued_to_continue",
  ]);
  const cancelled = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/logical-turns/${encodeURIComponent(String(running.logicalTurnId))}/cancel`,
    {
      expectedRevision: running.revision,
      reasonCode: "operator_cancelled_live_certification",
      summary: "task 6371 operator cancellation",
    },
    { "Idempotency-Key": `cancel:${running.logicalTurnId}:${suffix}` },
  );
  assert.equal(cancelled.status, 200, cancelled.text);

  const terminalDiagnostic = await waitForLogicalTurnState(sessionId, [
    "cancelled",
  ]);
  const completed = await waitForEvents(
    sessionId,
    beforeCursor,
    async (events) =>
      events.some((event) => event.kind === "logical_turn_cancelled"),
  );
  const terminalMessages = completed.events.filter(
    (event) => event.kind === "assistant_message_completed",
  );
  assert.equal(
    terminalMessages.filter((event) => event.payload.status === "completed")
      .length,
    0,
    "cancelled turn must not later complete",
  );
  assertNoLegacyCeiling(completed.events);

  return {
    label,
    providerAlias: chatProvider,
    profileId,
    sessionId,
    logicalTurnId: terminalDiagnostic.logicalTurnId,
    cancelledFromState: running.operatorState,
    finalOperatorState: terminalDiagnostic.operatorState,
    reasonCode: terminalDiagnostic.reasonCode,
    continuationCount: terminalDiagnostic.continuationCount,
    eventCount: completed.events.length,
    latestCursor: completed.latestCursor,
  };
}

function sequentialToolPrompt(label: string): string {
  return [
    "Complete this as one turn. You MUST call exactly one tool at a time and wait for each result before issuing the next tool call.",
    "First call git_status for /home/dev/rusty-crew.",
    "Only after that result, call read_file for /home/dev/rusty-crew/package.json.",
    "Only after that result, call read_file for /home/dev/rusty-crew/README.md.",
    "Only after that result, call search_files for the literal text `Rusty Crew` under /home/dev/rusty-crew/docs with max_results 3.",
    `Then answer with the exact marker TASK_6371_${label.toUpperCase().replaceAll("-", "_")} and a short summary.`,
  ].join("\n");
}

async function createProfile(
  label: string,
  providerAlias: string,
): Promise<{ profileId: string; sessionId: string }> {
  const profileId = `task-6371-${label}-${suffix}`;
  const created = await api("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Task 6371 ${label}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_coding_agent",
    reason: "task 6371 live continuation certification",
  });
  assert.equal(created.status, 200, created.text);
  profiles.push(profileId);
  const sessionId = nestedString(created.json, [
    "data",
    "outcome",
    "result",
    "sessionId",
  ]);
  assert.ok(sessionId, "created profile must report its derived session id");
  return { profileId, sessionId };
}

async function logicalTurnDiagnostic(
  sessionId: string,
  includeTerminal = false,
): Promise<Record<string, unknown>> {
  const response = await api(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/logical-turns?include_terminal=${includeTerminal}&limit=10`,
  );
  assert.equal(response.status, 200, response.text);
  const items = nestedArray(response.json, ["data", "items"]);
  assert.equal(items.length, 1, `expected one logical turn for ${sessionId}`);
  return items[0] ?? {};
}

async function waitForLogicalTurnState(
  sessionId: string,
  states: readonly string[],
): Promise<Record<string, unknown>> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      const diagnostic = await logicalTurnDiagnostic(sessionId, true);
      if (states.includes(String(diagnostic.operatorState))) return diagnostic;
    } catch {
      // Admission and restart windows can briefly have no diagnostic row.
    }
    await sleep(25);
  }
  throw new Error(
    `timed out waiting for ${sessionId} logical turn state ${states.join(", ")}`,
  );
}

async function latestCursor(sessionId: string): Promise<string> {
  const sessions = await api("GET", "/v1/chat/sessions?limit=500");
  assert.equal(sessions.status, 200, sessions.text);
  const session = nestedArray(sessions.json, ["data", "items"]).find(
    (item) => item.session_id === sessionId,
  );
  assert.ok(session, `chat inventory must contain ${sessionId}`);
  return String(session.latest_cursor ?? `${sessionId}:0`);
}

async function waitForEvents(
  sessionId: string,
  initialCursor: string,
  done: (events: ChatEvent[]) => Promise<boolean>,
): Promise<{ events: ChatEvent[]; latestCursor: string }> {
  const events: ChatEvent[] = [];
  let cursor = initialCursor;
  const deadline = Date.now() + 600_000;
  while (Date.now() < deadline) {
    try {
      const response = await api(
        "GET",
        `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?limit=500&cursor=${encodeURIComponent(cursor)}`,
      );
      if (response.status === 200) {
        const page = nestedArray(response.json, ["data", "items"]).map(
          (item) => item as unknown as ChatEvent,
        );
        for (const event of page) {
          if (!events.some((seen) => seen.event_id === event.event_id)) {
            events.push(event);
          }
          cursor = event.event_id;
        }
        if (await done(events)) return { events, latestCursor: cursor };
      }
    } catch {
      // A planned debug-service restart temporarily makes the API unavailable.
    }
    await sleep(100);
  }
  throw new Error(`timed out waiting for terminal events for ${sessionId}`);
}

function assertNoLegacyCeiling(events: readonly ChatEvent[]): void {
  const serialized = JSON.stringify(events);
  for (const reasonCode of forbiddenReasonCodes) {
    assert.equal(
      serialized.includes(reasonCode),
      false,
      `live event stream must not contain ${reasonCode}`,
    );
  }
}

function restartDebugService(): void {
  execFileSync("systemctl", ["--user", "restart", serviceUnit], {
    stdio: "inherit",
  });
}

async function waitForService(): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      const health = await api(
        "GET",
        "/v1/admin/healthz",
        undefined,
        {},
        5_000,
      );
      if (health.status === 200) return;
    } catch {
      // Expected while systemd is starting the debug service.
    }
    await sleep(250);
  }
  throw new Error(`debug service ${serviceUnit} did not become healthy`);
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

async function api(
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
  timeoutMs = 300_000,
): Promise<ApiResponse> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const text = await response.text();
  const json =
    text.trim() === "" ? {} : (JSON.parse(text) as Record<string, unknown>);
  return { status: response.status, text, json };
}

function nestedValue(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const key of path) {
    if (
      current === null ||
      typeof current !== "object" ||
      Array.isArray(current)
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function nestedString(
  value: unknown,
  path: readonly string[],
): string | undefined {
  const result = nestedValue(value, path);
  return typeof result === "string" ? result : undefined;
}

function nestedArray(
  value: unknown,
  path: readonly string[],
): Array<Record<string, unknown>> {
  const result = nestedValue(value, path);
  return Array.isArray(result)
    ? result.filter(
        (item): item is Record<string, unknown> =>
          item !== null && typeof item === "object" && !Array.isArray(item),
      )
    : [];
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
