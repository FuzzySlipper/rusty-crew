import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_CREW_SESSION_CERT_PROVIDER_ALIAS ?? "tester-chat";
const suffix = Date.now().toString(36);
const profileId = `crew-session-cert-${suffix}`;
const firstMarker = `CREW_ARCHIVE_BEFORE_${suffix.toUpperCase()}`;
const secondMarker = `CREW_ARCHIVE_AFTER_${suffix.toUpperCase()}`;
let profileCreated = false;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(
    `Crew session lifecycle certification is debug-only and requires port 9348, got ${baseUrl}`,
  );
}

try {
  await waitForHealth();
  const createdProfile = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Crew session lifecycle certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "basic_chat",
    mcpBindings: [
      {
        serverId: "den",
        toolProfileKey: "planner",
      },
    ],
    reason: "task-6326 live Crew session lifecycle certification",
  });
  assert.equal(createdProfile.status, 200, createdProfile.text);
  profileCreated = true;
  const originalSessionId = requiredString(createdProfile.json, [
    "data",
    "outcome",
    "result",
    "sessionId",
  ]);

  await runProviderTurn(originalSessionId, firstMarker);
  const archive = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(originalSessionId)}/commands`,
    {
      command: "/archive task-6326 lifecycle certification",
      actor: { id: "task-6326-cert-operator", kind: "human" },
    },
    { "Idempotency-Key": `archive-${suffix}` },
  );
  assert.equal(archive.status, 200, archive.text);
  assert.equal(nested(archive.json, ["data", "status"]), "completed");
  assert.equal(nested(archive.json, ["data", "command_name"]), "archive");

  await assertArchiveReadback(originalSessionId);
  await assertDirectorySession(profileId, originalSessionId, false);
  await assertNoRuntimeMcpBinding(profileId, originalSessionId);

  await restartDebugService();
  await waitForHealth();
  await assertArchiveReadback(originalSessionId);
  await assertDirectorySession(profileId, originalSessionId, false);
  await assertNoRuntimeMcpBinding(profileId, originalSessionId);

  const profile = await request(
    "GET",
    `/v1/admin/profiles/registry/${encodeURIComponent(profileId)}`,
  );
  assert.equal(profile.status, 200, profile.text);
  const profileRevision = requiredNumber(profile.json, ["data", "revision"]);
  const creationKey = `fresh-${suffix}`;
  const creationBody = {
    profile_id: profileId,
    expected_profile_revision: profileRevision,
    workspace_cwd: "/home/dev/rusty-crew",
  };
  const fresh = await request("POST", "/v1/chat/sessions", creationBody, {
    "Idempotency-Key": creationKey,
  });
  assert.equal(fresh.status, 200, fresh.text);
  assert.equal(nested(fresh.json, ["data", "creation", "outcome"]), "created");
  const freshSessionId = requiredString(fresh.json, [
    "data",
    "creation",
    "session",
    "sessionId",
  ]);
  assert.notEqual(freshSessionId, originalSessionId);

  const replay = await request("POST", "/v1/chat/sessions", creationBody, {
    "Idempotency-Key": creationKey,
  });
  assert.equal(replay.status, 200, replay.text);
  assert.equal(
    nested(replay.json, ["data", "creation", "outcome"]),
    "replayed",
  );
  assert.equal(
    nested(replay.json, ["data", "creation", "session", "sessionId"]),
    freshSessionId,
  );

  const changedIntent = await request(
    "POST",
    "/v1/chat/sessions",
    {
      ...creationBody,
      expected_profile_revision: profileRevision + 1,
      workspace_cwd: "/home/dev/rusty-crew",
    },
    { "Idempotency-Key": creationKey },
  );
  assert.equal(changedIntent.status, 409, changedIntent.text);
  assert.equal(
    nested(changedIntent.json, ["error", "reason_code"]),
    "crew_agent_session_creation_idempotency_conflict",
  );

  const staleRevision = await request(
    "POST",
    "/v1/chat/sessions",
    {
      profile_id: profileId,
      expected_profile_revision: profileRevision,
      workspace_cwd: "/home/dev/rusty-crew",
    },
    { "Idempotency-Key": `stale-${suffix}` },
  );
  assert.equal(staleRevision.status, 409, staleRevision.text);
  assert.equal(
    nested(staleRevision.json, ["error", "reason_code"]),
    "crew_agent_session_creation_profile_revision_conflict",
  );

  await assertDirectorySession(profileId, freshSessionId, true);
  await runProviderTurn(freshSessionId, secondMarker);

  console.log(
    JSON.stringify(
      {
        baseUrl,
        providerAlias,
        profileId,
        originalSessionId,
        freshSessionId,
        archiveCommandCursor: nested(archive.json, ["data", "latest_cursor"]),
        profileRevision,
        idempotencyReplay: "replayed",
        changedIntentReason: nested(changedIntent.json, [
          "error",
          "reason_code",
        ]),
        staleRevisionReason: nested(staleRevision.json, [
          "error",
          "reason_code",
        ]),
        providerMarkers: [firstMarker, secondMarker],
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
        reason: "task-6326 live certification cleanup",
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

async function runProviderTurn(
  sessionId: string,
  marker: string,
): Promise<void> {
  const before = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}`,
  );
  assert.equal(before.status, 200, before.text);
  const cursor = nested(before.json, ["data", "latest_cursor"]);
  const messageKey = `task-6326:${marker}`;
  const sent = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6326-cert-operator", kind: "human" },
      body: `Reply with exactly ${marker} and nothing else.`,
      client_message_id: messageKey,
      reason: "task-6326 provider-backed lifecycle proof",
    },
    { "Idempotency-Key": messageKey },
  );
  assert.equal(sent.status, 202, sent.text);
  const events = await waitForTurn(
    sessionId,
    typeof cursor === "string" ? cursor : undefined,
  );
  const terminal = events.find(
    (event) => event.kind === "assistant_turn_finished",
  );
  assert.ok(terminal, "provider turn must emit assistant_turn_finished");
  assert.notEqual(terminal.payload?.status, "failed", JSON.stringify(terminal));
  const text = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => String(event.payload?.text ?? ""))
    .join("");
  assert.match(text, new RegExp(marker));
}

async function assertArchiveReadback(sessionId: string): Promise<void> {
  const active = await request(
    "GET",
    `/v1/chat/sessions?profile_id=${encodeURIComponent(profileId)}`,
  );
  assert.equal(active.status, 200, active.text);
  assert.ok(
    !items(active.json).some((item) => item.session_id === sessionId),
    "archived session must be absent from default active inventory",
  );
  const archived = await request(
    "GET",
    `/v1/chat/sessions?profile_id=${encodeURIComponent(profileId)}&status=archived`,
  );
  assert.equal(archived.status, 200, archived.text);
  assert.ok(
    items(archived.json).some((item) => item.session_id === sessionId),
    "archived session must remain explicitly queryable",
  );
  const history = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}`,
  );
  assert.equal(history.status, 200, history.text);
  const events = nested(history.json, ["data", "events"]);
  assert.ok(Array.isArray(events));
  assert.ok(
    events.some(
      (event) =>
        isRecord(event) &&
        event.kind === "command_completed" &&
        nested(event, ["payload", "command_name"]) === "archive",
    ),
    "archive command outcome must remain in durable history",
  );
  assert.ok(
    events.some(
      (event) =>
        isRecord(event) && event.kind === "assistant_message_completed",
    ),
    "pre-archive transcript must remain available",
  );
}

async function assertDirectorySession(
  expectedProfileId: string,
  sessionId: string,
  present: boolean,
): Promise<void> {
  const directory = await request("GET", "/v1/debug/coordination/agents");
  assert.equal(directory.status, 200, directory.text);
  const agents = nested(directory.json, ["data", "agents"]);
  assert.ok(Array.isArray(agents));
  const found = agents.some(
    (agent) =>
      isRecord(agent) &&
      agent.profileId === expectedProfileId &&
      agent.sessionId === sessionId &&
      agent.routable === true,
  );
  assert.equal(found, present);
}

async function assertNoRuntimeMcpBinding(
  expectedProfileId: string,
  sessionId: string,
): Promise<void> {
  const catalog = await request("GET", "/v1/admin/mcp/servers");
  assert.equal(catalog.status, 200, catalog.text);
  const bindings = nested(catalog.json, ["data", "bindings"]);
  assert.ok(Array.isArray(bindings));
  assert.ok(
    !bindings.some(
      (binding) =>
        isRecord(binding) &&
        (binding.profileId === expectedProfileId ||
          binding.sessionId === sessionId),
    ),
    "archive must remove the session-targeted MCP binding instead of leaving an orphan",
  );
}

async function waitForTurn(
  sessionId: string,
  cursor: string | undefined,
): Promise<ChatEvent[]> {
  const deadline = Date.now() + 180_000;
  const events: ChatEvent[] = [];
  let nextCursor = cursor;
  while (Date.now() < deadline) {
    const url = new URL(
      `${baseUrl}/v1/chat/sessions/${encodeURIComponent(sessionId)}/events`,
    );
    url.searchParams.set("limit", "500");
    if (nextCursor !== undefined) url.searchParams.set("cursor", nextCursor);
    const page = await request("GET", `${url.pathname}${url.search}`);
    assert.equal(page.status, 200, page.text);
    const pageItems = nested(page.json, ["data", "items"]);
    if (Array.isArray(pageItems) && pageItems.length > 0) {
      events.push(...(pageItems as ChatEvent[]));
      const latest = nested(page.json, ["data", "latest_cursor"]);
      if (typeof latest === "string") nextCursor = latest;
      if (
        events.some((event) => event.kind === "assistant_turn_finished") &&
        events.some((event) => event.kind === "assistant_message_completed")
      ) {
        return events;
      }
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for provider turn in ${sessionId}`);
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
  if (text.trim() !== "") {
    const parsed: unknown = JSON.parse(text);
    if (isRecord(parsed)) json = parsed;
  }
  return { status: response.status, text, json };
}

function items(value: unknown): Array<Record<string, unknown>> {
  const found = nested(value, ["data", "items"]);
  return Array.isArray(found)
    ? found.filter((item): item is Record<string, unknown> => isRecord(item))
    : [];
}

function nested(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const key of path) {
    if (!isRecord(current)) return undefined;
    current = current[key];
  }
  return current;
}

function requiredString(value: unknown, path: readonly string[]): string {
  const result = nested(value, path);
  assert.equal(typeof result, "string", `${path.join(".")} must be a string`);
  return result;
}

function requiredNumber(value: unknown, path: readonly string[]): number {
  const result = nested(value, path);
  assert.equal(typeof result, "number", `${path.join(".")} must be a number`);
  return result;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
