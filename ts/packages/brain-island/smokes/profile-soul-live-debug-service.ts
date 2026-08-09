import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

const baseUrl = "http://127.0.0.1:9348";
const serviceUnit = "rusty-crew-debug.service";
const suffix = Date.now().toString(36);
const profileId = `task-6556-soul-${suffix}`;
const soulMarker = `TASK_6556_SOUL_${suffix.toUpperCase()}`;
const memoryMarker = `TASK_6556_MEMORY_${suffix.toUpperCase()}`;
let created = false;

try {
  await waitForService();
  const result = await apiData("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Task 6556 soul ${suffix}`,
    providerAlias: "deepseek-flash",
    kind: "full",
    workspaceCwd: "/home/dev/rusty-crew",
    localToolProfileId: "basic_chat",
    soulMarkdown: `# Certified soul\n\nAlways retain ${soulMarker}.`,
    memoryMarkdown: `# Certified memory\n\nRetain ${memoryMarker}.`,
    reason: "task 6556 registry-backed soul certification",
  });
  assert.equal(
    nested(result, "outcome", "status"),
    "completed",
    JSON.stringify(result, null, 2),
  );
  created = true;
  const sessionId = requiredString(result, "outcome", "result", "sessionId");

  const readback = await apiData(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/read`,
    { reason: "task 6556 exact prompt readback" },
  );
  assert.equal(nested(readback, "outcome", "status"), "completed");
  assert.equal(
    nested(readback, "outcome", "result", "profileConfig", "prompt"),
    undefined,
    "the file-backed profile must not duplicate registry prompt assets",
  );
  await assertRegistryReadback();

  const first = await sendAndWait(
    sessionId,
    `Reply with exactly FIRST_${suffix}.`,
    `${sessionId}:0`,
  );
  await assertProviderPrompt(sessionId, first.events);

  execFileSync("systemctl", ["--user", "restart", serviceUnit], {
    stdio: "inherit",
  });
  await waitForService();
  await assertRegistryReadback();

  const second = await sendAndWait(
    sessionId,
    `Reply with exactly SECOND_${suffix}.`,
    first.cursor,
  );
  await assertProviderPrompt(sessionId, second.events);

  const journal = execFileSync(
    "journalctl",
    ["--user-unit", serviceUnit, "--since", "10 minutes ago", "--no-pager"],
    { encoding: "utf8" },
  );
  assert.equal(journal.includes(soulMarker), false, "soul leaked to journal");
  assert.equal(
    journal.includes(memoryMarker),
    false,
    "memory leaked to journal",
  );

  console.log(
    JSON.stringify(
      {
        profileId,
        sessionId,
        registryReadback: true,
        providerPromptBeforeRestart: true,
        providerPromptAfterRestart: true,
        ordinaryJournalPromptLeak: false,
      },
      null,
      2,
    ),
  );
} finally {
  if (created) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task 6556 live certification cleanup",
      },
    ).catch(() => undefined);
  }
}

interface ChatEvent {
  event_id: string;
  kind: string;
  payload: Record<string, unknown>;
}

async function assertRegistryReadback(): Promise<void> {
  const record = await apiData(
    "GET",
    `/v1/admin/profiles/registry/${encodeURIComponent(profileId)}`,
  );
  assert.match(String(record.promptSoulMarkdown ?? ""), new RegExp(soulMarker));
  assert.match(
    String(record.promptMemoryMarkdown ?? ""),
    new RegExp(memoryMarker),
  );
}

async function assertProviderPrompt(
  sessionId: string,
  events: ChatEvent[],
): Promise<void> {
  const ids = providerDebugIds(events);
  assert.ok(ids.length > 0, "provider request debug id missing");
  let serialized = "";
  for (const id of ids) {
    const detail = await apiData(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/provider-requests/${encodeURIComponent(id)}`,
    );
    serialized += JSON.stringify(detail);
  }
  assert.match(serialized, new RegExp(soulMarker));
  assert.match(serialized, new RegExp(memoryMarker));
}

function providerDebugIds(events: ChatEvent[]): string[] {
  const ids = new Set<string>();
  for (const event of events) {
    for (const candidate of [
      event.payload?.metadata,
      event.payload?.metadataJson,
      event.payload?.metadata_json,
    ]) {
      const metadata = parseRecord(candidate);
      const id = metadata?.provider_request_debug_detail_id;
      if (typeof id === "string") ids.add(id);
    }
  }
  return [...ids];
}

async function sendAndWait(
  sessionId: string,
  body: string,
  initialCursor: string,
): Promise<{ cursor: string; events: ChatEvent[] }> {
  const key = `task-6556:${sessionId}:${Date.now()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6556-certifier", kind: "human" },
      body,
      client_message_id: key,
      reason: "task 6556 registry-backed soul certification",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);
  let cursor = initialCursor;
  const events: ChatEvent[] = [];
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const page = await apiData(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    for (const event of nestedArray(page, "items") as ChatEvent[]) {
      if (!events.some((seen) => seen.event_id === event.event_id))
        events.push(event);
    }
    if (typeof page.latest_cursor === "string") cursor = page.latest_cursor;
    const terminal = events.find(
      (event) => event.kind === "assistant_turn_finished",
    );
    if (terminal !== undefined) {
      assert.notEqual(
        terminal.payload.status,
        "failed",
        JSON.stringify(events),
      );
      return { cursor, events };
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${sessionId}`);
}

async function waitForService(): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const response = await api("GET", "/v1/admin/healthz").catch(
      () => undefined,
    );
    if (response?.status === 200) return;
    await delay(250);
  }
  throw new Error("debug service did not become healthy");
}

async function apiData(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
): Promise<Record<string, unknown>> {
  const response = await api(method, path, body);
  assert.ok(response.status < 400, response.text);
  const data = nested(response.json, "data");
  assert.ok(data && typeof data === "object" && !Array.isArray(data));
  return data as Record<string, unknown>;
}

async function api(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
  extraHeaders: Record<string, string> = {},
) {
  const token = process.env.RUSTY_CREW_ADMIN_TOKEN?.trim();
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...extraHeaders,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(300_000),
  });
  const text = await response.text();
  return {
    status: response.status,
    text,
    json: (text.trim() === "" ? {} : JSON.parse(text)) as Record<
      string,
      unknown
    >,
  };
}

function nested(value: unknown, ...path: string[]): unknown {
  let current = value;
  for (const part of path) {
    if (!current || typeof current !== "object" || Array.isArray(current))
      return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function nestedArray(value: unknown, ...path: string[]): unknown[] {
  const result = nested(value, ...path);
  return Array.isArray(result) ? result : [];
}

function requiredString(value: unknown, ...path: string[]): string {
  const result = nested(value, ...path);
  assert.equal(typeof result, "string", path.join("."));
  return result as string;
}

function parseRecord(value: unknown): Record<string, unknown> | undefined {
  if (typeof value === "string") {
    try {
      return parseRecord(JSON.parse(value));
    } catch {
      return undefined;
    }
  }
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
