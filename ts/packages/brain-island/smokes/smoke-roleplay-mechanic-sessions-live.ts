import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_MECHANIC_PROVIDER_ALIAS ?? "deepseek-flash";
const suffix = Date.now().toString(36);
const narratorOne = `mechanic-session-narrator-a-${suffix}`;
const narratorTwo = `mechanic-session-narrator-b-${suffix}`;
const roleplayOne = `mechanic-session-rp-a-${suffix}`;
const roleplayTwo = `mechanic-session-rp-b-${suffix}`;
const mechanicProfile = `mechanic-session-agent-${suffix}`;

if (new URL(baseUrl).port !== "9348") {
  throw new Error("mechanic session live smoke requires debug port 9348");
}

try {
  await createNarrator(narratorOne, roleplayOne);
  await createNarrator(narratorTwo, roleplayTwo);
  await createProfile({
    profileId: mechanicProfile,
    displayName: "Mechanic session live agent",
    providerAlias,
    localToolProfileId: "basic_chat",
  });
  await apiData(
    "PATCH",
    `/v1/admin/roleplay/profiles/${mechanicProfile}/mechanic-config`,
    { name: "Maren", providerAlias, autoMonitor: false },
  );

  const first = await createMechanic(roleplayOne);
  const second = await createMechanic(roleplayOne);
  assert.notEqual(first.mechanicSessionId, second.mechanicSessionId);

  const moved = await apiData<{ association: Association }>(
    "POST",
    `/v1/admin/roleplay/mechanic-sessions/${encodeURIComponent(second.mechanicSessionId)}/attach`,
    {
      roleplaySessionId: roleplayTwo,
      expectedRevision: second.revision,
    },
  );
  assert.equal(moved.association.roleplaySessionId, roleplayTwo);
  assert.equal(moved.association.revision, 2);

  const firstList = await apiData<{ items: unknown[] }>(
    "GET",
    `/v1/admin/roleplay/mechanic-sessions?roleplay_session_id=${encodeURIComponent(roleplayOne)}`,
  );
  assert.equal(firstList.items.length, 1);
  const secondList = await apiData<{ items: unknown[] }>(
    "GET",
    `/v1/admin/roleplay/mechanic-sessions?roleplay_session_id=${encodeURIComponent(roleplayTwo)}`,
  );
  assert.equal(secondList.items.length, 1);

  await apiData(
    "POST",
    `/v1/admin/roleplay/mechanic-sessions/${encodeURIComponent(first.mechanicSessionId)}/archive`,
  );
  const roleplayAfterMechanicArchive = await apiData<{
    session: { archived: boolean };
  }>("GET", `/v1/admin/roleplay/sessions/${encodeURIComponent(roleplayOne)}`);
  assert.equal(roleplayAfterMechanicArchive.session.archived, false);
  await apiData(
    "POST",
    `/v1/admin/roleplay/mechanic-sessions/${encodeURIComponent(first.mechanicSessionId)}/restore`,
  );

  const proposal = await apiData<Proposal>(
    "POST",
    "/v1/admin/roleplay/mechanic-proposals",
    {
      mechanicSessionId: second.mechanicSessionId,
      roleplaySessionId: roleplayTwo,
      kind: "exemplar",
      proposedValue: "A patient test exemplar.",
      rationale: "Link a live diagnostic to a real proposal.",
      diagnosticContext: { source: "task-5691-live" },
    },
  );
  const events = await sendAndWait(
    second.mechanicSessionId,
    [
      "Call record_roleplay_diagnostic exactly once.",
      "Pass this complete Markdown as the single report argument:",
      "---",
      "symptom: Scene transitions skip established beats.",
      "hypothesis: The active exemplar rewards abrupt pacing.",
      "proposal_ids:",
      `  - ${proposal.proposalId}`,
      "---",
      "Observe the next three assistant turns before deciding the outcome.",
      "After the tool returns, briefly report the diagnostic status.",
    ].join("\n"),
  );
  assertToolCompleted(events, "record_roleplay_diagnostic");
  const diagnostics = await apiData<{ items: Diagnostic[] }>(
    "GET",
    `/v1/admin/roleplay/mechanic-diagnostics?mechanic_session_id=${encodeURIComponent(second.mechanicSessionId)}`,
  );
  assert.equal(diagnostics.items.length, 1);
  const diagnostic = diagnostics.items[0]!;
  assert.equal(diagnostic.outcome, "pending");
  assert.deepEqual(diagnostic.proposalIds, [proposal.proposalId]);

  await restartDebugService();
  await waitForHealth();
  const hydrated = await apiData<{ diagnostic: Diagnostic }>(
    "GET",
    `/v1/admin/roleplay/mechanic-diagnostics/${encodeURIComponent(diagnostic.diagnosticId)}`,
  );
  assert.equal(hydrated.diagnostic.revision, 1);
  const updated = await apiData<{ diagnostic: Diagnostic }>(
    "POST",
    `/v1/admin/roleplay/mechanic-diagnostics/${encodeURIComponent(diagnostic.diagnosticId)}/outcome`,
    {
      outcome: "improved",
      notes: "Follow-up turns preserved the scene beats.",
      expectedRevision: 1,
    },
  );
  assert.equal(updated.diagnostic.outcome, "improved");
  const conflict = await api(
    "POST",
    `/v1/admin/roleplay/mechanic-diagnostics/${encodeURIComponent(diagnostic.diagnosticId)}/outcome`,
    { outcome: "worse", expectedRevision: 1 },
  );
  assert.equal(conflict.status, 409, conflict.text);

  console.log(
    JSON.stringify({
      baseUrl,
      providerAlias,
      continueRoleplayNewMechanic: true,
      newRoleplayContinueMechanic: true,
      independentArchiveRestore: true,
      diagnosticId: diagnostic.diagnosticId,
      diagnosticPersistedAcrossRestart: true,
      liveToolCall: "record_roleplay_diagnostic",
      revisionConflictProtected: true,
    }),
  );
} finally {
  for (const profileId of [mechanicProfile, narratorTwo, narratorOne]) {
    await api("POST", `/v1/admin/control/profiles/${profileId}/delete`, {
      confirmProfileId: profileId,
      reason: "task-5691 live mechanic session cleanup",
    }).catch(() => undefined);
  }
}

interface Association {
  mechanicSessionId: string;
  roleplaySessionId?: string;
  revision: number;
}

interface Proposal {
  proposalId: string;
}

interface Diagnostic {
  diagnosticId: string;
  proposalIds: string[];
  outcome: "pending" | "improved" | "no_change" | "worse";
  revision: number;
}

interface EventRecord {
  kind: string;
  payload?: Record<string, unknown>;
}

async function createNarrator(profileId: string, sessionId: string) {
  await createProfile({
    profileId,
    displayName: `Narrator ${profileId}`,
    providerAlias,
    localToolProfileId: "roleplay_lore",
    brain: { module: "chat-completions", strategy: "roleplay_narrator" },
  });
  await apiData("POST", "/v1/admin/roleplay/sessions", {
    sessionId,
    profileId,
    displayName: `Roleplay ${sessionId}`,
  });
}

async function createMechanic(roleplaySessionId: string): Promise<Association> {
  const created = await apiData<{ association: Association }>(
    "POST",
    "/v1/admin/roleplay/mechanic-sessions",
    { profileId: mechanicProfile, roleplaySessionId },
  );
  return created.association;
}

async function createProfile(input: Record<string, unknown>): Promise<void> {
  const response = await apiData<{ outcome?: { status?: string } }>(
    "POST",
    "/v1/admin/control/profiles",
    { kind: "full", reason: "task-5691 live mechanic sessions", ...input },
  );
  assert.equal(response.outcome?.status, "completed");
}

async function sendAndWait(
  sessionId: string,
  body: string,
): Promise<EventRecord[]> {
  const before = await apiData<Record<string, any>>(
    "GET",
    `/v1/chat/sessions/${sessionId}`,
  );
  let cursor = String(
    before.session?.latest_cursor ?? before.latest_cursor ?? "0",
  );
  const key = `task-5691:${sessionId}:${Date.now()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${sessionId}/messages`,
    {
      actor: { id: "task-5691-operator", kind: "human" },
      body,
      client_message_id: key,
      reason: "task-5691 live mechanic diagnostic",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);
  const events: EventRecord[] = [];
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, any>>(
      "GET",
      `/v1/chat/sessions/${sessionId}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    events.push(...(Array.isArray(page.items) ? page.items : []));
    cursor = String(page.latest_cursor ?? cursor);
    if (events.some((event) => event.kind === "assistant_turn_finished")) {
      assert.equal(
        events.find(
          (event) =>
            event.kind === "assistant_turn_finished" &&
            event.payload?.status === "failed",
        ),
        undefined,
      );
      return events;
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for ${sessionId}`);
}

function assertToolCompleted(events: EventRecord[], name: string): void {
  assert.ok(
    events.some(
      (event) =>
        event.kind === "tool_call_completed" &&
        event.payload?.tool_name === name &&
        event.payload?.is_error !== true,
    ),
    JSON.stringify(events),
  );
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
    if (
      (await api("GET", "/v1/admin/healthz").catch(() => undefined))?.status ===
      200
    )
      return;
    await delay(250);
  }
  throw new Error("debug service did not recover after restart");
}

async function apiData<T>(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await api(method, path, body);
  assert.ok(response.status < 400, response.text);
  assert.equal(response.json.ok, true, response.text);
  return response.json.data as T;
}

async function api(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
) {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let json: Record<string, any> = {};
  try {
    json = JSON.parse(text) as Record<string, any>;
  } catch {
    // Assertions retain raw response text.
  }
  return { status: response.status, text, json };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
