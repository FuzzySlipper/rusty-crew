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
const narratorProfileId = `mechanic-proposal-narrator-${suffix}`;
const roleplaySessionId = `mechanic-proposal-rp-${suffix}`;
const mechanicProfileId = `mechanic-proposal-agent-${suffix}`;
const mechanicSessionId = `${mechanicProfileId}-session`;
const marker = `MECHANIC_PROPOSAL_${suffix.toUpperCase()}`;
const acceptedExemplar = `Rain counted a patient rhythm against the observatory glass. ${marker}`;
const rejectedExemplar = `This rejected exemplar must never become active. ${marker}`;

if (new URL(baseUrl).port !== "9348") {
  throw new Error("mechanic proposal live smoke requires debug port 9348");
}

try {
  await createProfile({
    profileId: narratorProfileId,
    displayName: "Mechanic proposal target narrator",
    providerAlias,
    localToolProfileId: "roleplay_lore",
    brain: { module: "pi-agent", strategy: "roleplay_narrator" },
  });
  await apiData("POST", "/v1/admin/roleplay/sessions", {
    sessionId: roleplaySessionId,
    profileId: narratorProfileId,
    displayName: "Mechanic proposal live target",
  });
  await createProfile({
    profileId: mechanicProfileId,
    sessionId: mechanicSessionId,
    displayName: "Mechanic proposal live agent",
    providerAlias,
    localToolProfileId: "basic_chat",
  });
  await apiData(
    "PATCH",
    `/v1/admin/roleplay/profiles/${mechanicProfileId}/mechanic-config`,
    { name: "Maren", providerAlias, autoMonitor: false },
  );

  const before = await narratorConfig();
  assert.notEqual(before.exemplar, acceptedExemplar);

  const events = await sendAndWait(
    mechanicSessionId,
    [
      "Call propose_roleplay_change exactly once and do not merely describe the proposal.",
      "Pass the following complete Markdown as its single proposal argument:",
      "---",
      `roleplay_session_id: ${roleplaySessionId}`,
      "change_kind: exemplar",
      `rationale: ${marker}`,
      "evidence:",
      "  - live-provider-certification",
      "---",
      acceptedExemplar,
      "After the tool returns, briefly report the proposal status.",
    ].join("\n"),
  );
  assertToolCompleted(events, "propose_roleplay_change");

  const proposals = await listProposals();
  const proposed = proposals.find((proposal) => proposal.rationale === marker);
  assert.ok(proposed, JSON.stringify(proposals));
  assert.equal(proposed.status, "proposed");
  assert.equal(proposed.proposedValue, acceptedExemplar);
  assert.notEqual((await narratorConfig()).exemplar, acceptedExemplar);

  await restartDebugService();
  await waitForHealth();
  const hydrated = await proposal(proposed.proposalId);
  assert.equal(hydrated.status, "proposed");
  assert.equal(hydrated.history.length, 1);

  const approved = await apiData<ProposalRecord>(
    "POST",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(proposed.proposalId)}/approve`,
    {
      reviewerId: "task-5690-operator",
      note: "Live certification approval.",
      expectedRevision: hydrated.revision,
    },
  );
  assert.equal(approved.status, "approved");
  assert.notEqual((await narratorConfig()).exemplar, acceptedExemplar);

  const applied = await apiData<{ proposal: ProposalRecord }>(
    "POST",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(proposed.proposalId)}/apply`,
    { actorId: "task-5690-operator" },
  );
  assert.equal(applied.proposal.status, "applied");
  assert.equal((await narratorConfig()).exemplar, acceptedExemplar);

  const reapplied = await apiData<{ proposal: ProposalRecord }>(
    "POST",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(proposed.proposalId)}/apply`,
    { actorId: "task-5690-operator" },
  );
  assert.equal(reapplied.proposal.revision, applied.proposal.revision);

  const rejected = await apiData<ProposalRecord>(
    "POST",
    "/v1/admin/roleplay/mechanic-proposals",
    {
      proposalId: `mechanic-proposal-rejected-${suffix}`,
      mechanicSessionId,
      roleplaySessionId,
      kind: "exemplar",
      proposedValue: rejectedExemplar,
      rationale: `Rejected ${marker}`,
      diagnosticContext: { source: "live-certification" },
    },
  );
  const rejectedDecision = await apiData<ProposalRecord>(
    "POST",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(rejected.proposalId)}/reject`,
    {
      reviewerId: "task-5690-operator",
      note: "Exercise the rejection path.",
      expectedRevision: rejected.revision,
    },
  );
  assert.equal(rejectedDecision.status, "rejected");
  const rejectedApply = await api(
    "POST",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(rejected.proposalId)}/apply`,
    { actorId: "task-5690-operator" },
  );
  assert.equal(rejectedApply.status, 409, rejectedApply.text);
  assert.equal((await narratorConfig()).exemplar, acceptedExemplar);

  const history = await apiData<{ history: unknown[] }>(
    "GET",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(proposed.proposalId)}/history`,
  );
  assert.equal(history.history.length, 3);

  console.log(
    JSON.stringify({
      baseUrl,
      providerAlias,
      mechanicSessionId,
      roleplaySessionId,
      liveToolCall: "propose_roleplay_change",
      proposalId: proposed.proposalId,
      persistedAcrossRestart: true,
      approvedBeforeApply: true,
      appliedIdempotently: true,
      rejectionStayedInert: true,
    }),
  );
} finally {
  for (const profileId of [mechanicProfileId, narratorProfileId]) {
    await api("POST", `/v1/admin/control/profiles/${profileId}/delete`, {
      confirmProfileId: profileId,
      reason: "task-5690 live mechanic proposal cleanup",
    }).catch(() => undefined);
  }
}

interface EventRecord {
  kind: string;
  payload?: Record<string, unknown>;
}

interface ProposalRecord {
  proposalId: string;
  rationale: string;
  proposedValue: unknown;
  status: "proposed" | "approved" | "rejected" | "applied";
  revision: number;
  history: unknown[];
}

async function createProfile(input: Record<string, unknown>): Promise<void> {
  const response = await apiData<{ outcome?: { status?: string } }>(
    "POST",
    "/v1/admin/control/profiles",
    {
      kind: "full",
      reason: "task-5690 live mechanic proposals",
      ...input,
    },
  );
  assert.equal(response.outcome?.status, "completed");
}

async function narratorConfig(): Promise<Record<string, unknown>> {
  const response = await apiData<{ config: Record<string, unknown> }>(
    "GET",
    `/v1/admin/roleplay/profiles/${narratorProfileId}/narrator-config`,
  );
  return response.config;
}

async function listProposals(): Promise<ProposalRecord[]> {
  return apiData<ProposalRecord[]>(
    "GET",
    `/v1/admin/roleplay/mechanic-proposals?roleplay_session_id=${encodeURIComponent(roleplaySessionId)}`,
  );
}

async function proposal(proposalId: string): Promise<ProposalRecord> {
  return apiData<ProposalRecord>(
    "GET",
    `/v1/admin/roleplay/mechanic-proposals/${encodeURIComponent(proposalId)}`,
  );
}

async function sendAndWait(
  sessionId: string,
  body: string,
): Promise<EventRecord[]> {
  const before = await apiData<Record<string, any>>(
    "GET",
    `/v1/chat/sessions/${sessionId}`,
  );
  const cursor = String(
    before.session?.latest_cursor ?? before.latest_cursor ?? "0",
  );
  const key = `task-5690:${sessionId}:${Date.now()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${sessionId}/messages`,
    {
      actor: { id: "task-5690-operator", kind: "human" },
      body,
      client_message_id: key,
      reason: "task-5690 live mechanic proposal",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);
  const events: EventRecord[] = [];
  let nextCursor = cursor;
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, any>>(
      "GET",
      `/v1/chat/sessions/${sessionId}/events?cursor=${encodeURIComponent(nextCursor)}&limit=500`,
    );
    const items = Array.isArray(page.items)
      ? (page.items as EventRecord[])
      : [];
    events.push(...items);
    nextCursor = String(page.latest_cursor ?? nextCursor);
    if (events.some((event) => event.kind === "assistant_turn_finished")) {
      const failed = events.find(
        (event) =>
          event.kind === "assistant_turn_finished" &&
          event.payload?.status === "failed",
      );
      assert.equal(failed, undefined, JSON.stringify(failed));
      return events;
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for ${sessionId}`);
}

function assertToolCompleted(events: EventRecord[], name: string): void {
  const completed = events
    .filter(
      (event) =>
        event.kind === "tool_call_completed" &&
        event.payload?.is_error !== true,
    )
    .map((event) => String(event.payload?.tool_name ?? ""));
  assert.ok(completed.includes(name), JSON.stringify(events));
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
    const response = await api("GET", "/v1/admin/healthz").catch(
      () => undefined,
    );
    if (response?.status === 200) return;
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
