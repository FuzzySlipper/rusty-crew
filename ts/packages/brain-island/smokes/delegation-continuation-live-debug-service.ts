import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/$/, "");
const providerAlias =
  process.env.RUSTY_CREW_DELEGATION_CERT_PROVIDER ?? "deepseek-flash";
const evidenceRoot =
  process.env.RUSTY_CREW_DELEGATION_CERT_EVIDENCE_ROOT ??
  "/home/system/rusty-crew-debug/evidence/task-6514";
const suffix = Date.now().toString(36);
const parentProfileId = `task-6514-parent-${suffix}`;
const childAProfileId = `task-6514-child-a-${suffix}`;
const childBProfileId = `task-6514-child-b-${suffix}`;
const childAMarker = `TASK_6514_CHILD_A_${suffix.toUpperCase()}`;
const childBMarker = `TASK_6514_CHILD_B_${suffix.toUpperCase()}`;
const createdProfiles: string[] = [];

assert.equal(new URL(baseUrl).port, "9348");

try {
  await waitForService();
  const beforeCounts = await tableCounts();
  await createProfile(childAProfileId, "Task 6514 child A");
  await createProfile(childBProfileId, "Task 6514 child B");
  const parentSessionId = await createProfile(
    parentProfileId,
    "Task 6514 parent",
  );
  const beforeCursor = await latestCursor(parentSessionId);
  const messageKey = `task-6514-${suffix}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(parentSessionId)}/messages`,
    {
      actor: { id: "task-6514-certifier", kind: "human" },
      body: parentPrompt(),
      client_message_id: `message:${messageKey}`,
      reason: "task 6514 yielded delegation live certification",
    },
    { "Idempotency-Key": messageKey },
  );
  assert.equal(sent.status, 202, sent.text);

  const events = await waitForTerminalEvents(parentSessionId, beforeCursor);
  const serializedEvents = JSON.stringify(events);
  assert.ok(
    serializedEvents.includes(childAMarker),
    "parent must consume child A",
  );
  assert.ok(
    serializedEvents.includes(childBMarker),
    "parent must consume child B",
  );
  assert.ok(
    events.some((event) => event.kind === "logical_turn_yielding"),
    "parent must cross a work-quantum boundary",
  );
  assert.ok(
    events.some(
      (event) =>
        event.kind === "tool_call_completed" &&
        event.payload.tool_name === "fan_out_subagents_md" &&
        event.payload.is_error !== true,
    ),
    "fan-out tool must complete successfully",
  );

  const inventory = await chatInventory();
  const delegatedSessions = inventory.filter((item) =>
    String(item.session_id ?? "").startsWith(`${parentSessionId}:delegated:`),
  );
  assert.equal(delegatedSessions.length, 2);
  assert.ok(
    delegatedSessions.every(
      (item) =>
        String(item.latest_cursor ?? "") !== `${item.session_id ?? ""}:0`,
    ),
    "both delegated sessions must execute provider-backed turns",
  );

  const afterCounts = await tableCounts();
  assert.equal(afterCounts.worker_runs - beforeCounts.worker_runs, 2);
  assert.ok(
    afterCounts.completion_packets - beforeCounts.completion_packets >= 2,
  );
  const diagnostic = await logicalTurnDiagnostic(parentSessionId);
  assert.equal(diagnostic.operatorState, "completed");
  assert.ok(Number(diagnostic.continuationCount) >= 2);

  const evidence = {
    schemaVersion: "task-6514-live-v1",
    generatedAt: new Date().toISOString(),
    baseUrl,
    providerAlias,
    parentProfileId,
    parentSessionId,
    delegatedSessionIds: delegatedSessions.map((item) => item.session_id),
    workerRunsCreated: afterCounts.worker_runs - beforeCounts.worker_runs,
    completionPacketsCreated:
      afterCounts.completion_packets - beforeCounts.completion_packets,
    continuationCount: diagnostic.continuationCount,
    childMarkers: [childAMarker, childBMarker],
    eventKinds: events.map((event) => event.kind),
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
  for (const profileId of createdProfiles.reverse()) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task 6514 live certification cleanup",
      },
    ).catch((error: unknown) => {
      console.error(`profile cleanup failed for ${profileId}`, error);
    });
  }
}

interface ChatEvent {
  event_id: string;
  kind: string;
  payload: Record<string, unknown>;
}

function parentPrompt(): string {
  return [
    "Complete this as one logical turn.",
    "First call fan_out_subagents_md exactly once with these two sections:",
    `## ${childAProfileId}`,
    `Call deliver_completion_md with a completed summary containing exactly ${childAMarker}.`,
    `## ${childBProfileId}`,
    `Call deliver_completion_md with a completed summary containing exactly ${childBMarker}.`,
    "Use group_id task-6514-live, max_concurrency 2, failure_policy fail_soft, and parent_consumption await_completion.",
    "After fan-out succeeds, call git_status on /home/dev/rusty-crew, then read_file on package.json, then read_file on README.md, then search_files for Rusty Crew under docs with max_results 3. Call tools one at a time.",
    "Rusty Crew will inject each finished child as a message beginning `[Rusty Crew delegated completion]`. Those injected messages are authoritative completion results.",
    "After the four required local tool calls, if both delegated completion messages are not visible yet, call terminal with `sleep 5` once. Do not call session_search, list_agents, or any other inspection tool to look for children.",
    "As soon as both injected completion messages are visible, call no more tools and answer immediately.",
    `Your final answer must contain both ${childAMarker} and ${childBMarker}.`,
  ].join("\n");
}

async function createProfile(
  profileId: string,
  displayName: string,
): Promise<string> {
  const response = await api("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_coding_agent",
    reason: "task 6514 live certification",
  });
  assert.equal(response.status, 200, response.text);
  createdProfiles.push(profileId);
  const sessionId = nestedString(response.json, [
    "data",
    "outcome",
    "result",
    "sessionId",
  ]);
  assert.ok(sessionId, `profile ${profileId} must report its session`);
  return sessionId;
}

async function latestCursor(sessionId: string): Promise<string> {
  const session = (await chatInventory()).find(
    (item) => item.session_id === sessionId,
  );
  assert.ok(session, `chat inventory must contain ${sessionId}`);
  return String(session.latest_cursor ?? `${sessionId}:0`);
}

async function chatInventory(): Promise<Array<Record<string, unknown>>> {
  const response = await api("GET", "/v1/chat/sessions?limit=500");
  assert.equal(response.status, 200, response.text);
  return nestedArray(response.json, ["data", "items"]);
}

async function waitForTerminalEvents(
  sessionId: string,
  initialCursor: string,
): Promise<ChatEvent[]> {
  const events: ChatEvent[] = [];
  let cursor = initialCursor;
  const deadline = Date.now() + 600_000;
  while (Date.now() < deadline) {
    const response = await api(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?limit=500&cursor=${encodeURIComponent(cursor)}`,
    );
    assert.equal(response.status, 200, response.text);
    for (const item of nestedArray(response.json, ["data", "items"])) {
      const event = item as unknown as ChatEvent;
      if (!events.some((seen) => seen.event_id === event.event_id)) {
        events.push(event);
      }
      cursor = event.event_id;
    }
    if (
      events.some(
        (event) =>
          event.kind === "assistant_message_completed" &&
          event.payload.status !== "continuing",
      )
    ) {
      return events;
    }
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${sessionId}`);
}

async function logicalTurnDiagnostic(
  sessionId: string,
): Promise<Record<string, unknown>> {
  const response = await api(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/logical-turns?include_terminal=true&limit=10`,
  );
  assert.equal(response.status, 200, response.text);
  const items = nestedArray(response.json, ["data", "items"]);
  const completed = items
    .filter((item) => item.operatorState === "completed")
    .sort(
      (left, right) =>
        Number(right.continuationCount ?? 0) -
        Number(left.continuationCount ?? 0),
    );
  assert.ok(completed.length > 0, "parent must have a completed logical turn");
  return completed[0] ?? {};
}

async function tableCounts(): Promise<{
  worker_runs: number;
  completion_packets: number;
}> {
  const response = await api("GET", "/v1/admin/diagnostics");
  assert.equal(response.status, 200, response.text);
  const counts = nestedRecord(response.json, [
    "data",
    "overview",
    "persistence",
    "tableCounts",
  ]);
  return {
    worker_runs: Number(counts.worker_runs ?? 0),
    completion_packets: Number(counts.completion_packets ?? 0),
  };
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
      // Expected while the debug service restarts.
    }
    await sleep(250);
  }
  throw new Error("debug service did not become healthy");
}

async function api(
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
  timeoutMs = 300_000,
) {
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
  const json = text ? (JSON.parse(text) as Record<string, unknown>) : {};
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

function nestedString(value: unknown, path: readonly string[]) {
  const result = nestedValue(value, path);
  return typeof result === "string" ? result : undefined;
}

function nestedRecord(value: unknown, path: readonly string[]) {
  const result = nestedValue(value, path);
  return result !== null && typeof result === "object" && !Array.isArray(result)
    ? (result as Record<string, unknown>)
    : {};
}

function nestedArray(value: unknown, path: readonly string[]) {
  const result = nestedValue(value, path);
  return Array.isArray(result)
    ? result.filter(
        (item): item is Record<string, unknown> =>
          item !== null && typeof item === "object" && !Array.isArray(item),
      )
    : [];
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
