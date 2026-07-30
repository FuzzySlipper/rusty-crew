import assert from "node:assert/strict";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_SESSION_EXECUTION_CERT_PROVIDER_ALIAS ?? "tester-chat";
const suffix = Date.now().toString(36);
const profileId = `session-execution-cert-${suffix}`;
const marker = `SESSION_EXECUTION_CERT_${suffix.toUpperCase()}`;
let profileCreated = false;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `session execution certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  await waitForHealth();
  const created = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Session execution certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_coding_agent",
    reason: "task-6420 canonical session execution live certification",
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
  assert.equal(nested(before, ["session", "status"]), "idle");
  assert.equal(nested(before, ["session", "execution", "phase"]), "idle");
  const beforeCursor = requiredString(before, ["latest_cursor"]);

  const key = `task-6420-${suffix}`;
  const sentPromise = request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6420-cert-operator", kind: "human" },
      body: `Use the terminal tool exactly once to run sleep 6. After it finishes, reply with exactly ${marker}.`,
      client_message_id: key,
      reason: "task-6420 provider-backed execution transition proof",
    },
    { "Idempotency-Key": key },
  );

  const observedSnapshotPhases = new Set<string>();
  const events: ChatEvent[] = [];
  let cursor = beforeCursor;
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const snapshot = await readSession(sessionId);
    const phase = nested(snapshot, ["session", "execution", "phase"]);
    if (typeof phase === "string") observedSnapshotPhases.add(phase);

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
    const terminalSeen = events.some(
      (event) =>
        event.kind === "assistant_turn_finished" &&
        event.payload?.status !== "failed",
    );
    const workingSeen = [...observedSnapshotPhases].some(
      (observed) => observed !== "idle",
    );
    if (terminalSeen && workingSeen && phase === "idle") {
      break;
    }
    await delay(25);
  }

  const sent = await sentPromise;
  assert.equal(sent.status, 202, sent.text);

  const terminal = events.find(
    (event) => event.kind === "assistant_turn_finished",
  );
  assert.ok(terminal, "real provider turn did not reach a terminal event");
  assert.notEqual(terminal.payload?.status, "failed", JSON.stringify(terminal));
  assert.ok(
    [...observedSnapshotPhases].some((phase) => phase !== "idle"),
    `snapshot polling never observed active work: ${JSON.stringify([...observedSnapshotPhases])}`,
  );

  const executionEvents = events.filter(
    (event) => event.kind === "session_execution_changed",
  );
  const eventPhases = executionEvents
    .map((event) => nested(event, ["payload", "execution", "phase"]))
    .filter((phase): phase is string => typeof phase === "string");
  assert.ok(
    eventPhases.some((phase) => phase !== "idle"),
    `event replay did not include a working phase: ${JSON.stringify(eventPhases)}`,
  );
  assert.equal(eventPhases.at(-1), "idle");

  const after = await readSession(sessionId);
  assert.equal(nested(after, ["session", "status"]), "idle");
  assert.equal(nested(after, ["session", "execution", "phase"]), "idle");
  assert.equal(
    nested(after, ["session", "execution", "lastOutcome"]),
    "completed",
  );

  const directory = await request("GET", "/v1/debug/coordination/agents");
  assert.equal(directory.status, 200, directory.text);
  const agents = nested(directory.json, ["data", "agents"]);
  assert.ok(Array.isArray(agents));
  const directoryEntry = agents
    .filter(isRecord)
    .find((entry) => entry.sessionId === sessionId);
  assert.ok(directoryEntry, "coordination inventory omitted certified session");
  assert.equal(directoryEntry.sessionStatus, "idle");
  assert.equal(nested(directoryEntry, ["execution", "phase"]), "idle");

  console.log(
    JSON.stringify(
      {
        baseUrl,
        providerAlias,
        profileId,
        sessionId,
        before: "idle",
        observedSnapshotPhases: [...observedSnapshotPhases],
        eventPhases,
        settledOutcome: "completed",
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
        reason: "task-6420 live certification cleanup",
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
