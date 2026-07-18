import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_CHAT_CERT_PROVIDER_ALIAS ?? "tester-chat";
const workdir =
  process.env.RUSTY_CREW_RESOURCE_LIMITS_CERT_WORKDIR ??
  "/home/dev/rusty-crew/ts/packages/brain-island";
const suffix = Date.now().toString(36);
const profileId = `resource-limits-cert-${suffix}`;
const sessionId = `${profileId}-scoped-session`;
const agentId = `${profileId}-scoped-agent`;
const omittedSessionId = `${profileId}-default-session`;
const evidence: Record<string, unknown> = {};

try {
  assert.ok(workdir.startsWith("/"), "certification workdir must be absolute");
  const createdProfile = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Resource limits certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_agent",
    reason: "task 5846 live debug certification",
  });
  assert.equal(createdProfile.status, 200, createdProfile.text);
  assert.equal(
    nested(createdProfile.json, ["data", "outcome", "status"]),
    "completed",
  );

  const createdSession = await createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
    resourceLimits: {
      workdir,
      maxDurationMs: 120_000,
      maxDelegationDepth: 0,
    },
    reason: "task 5846 explicit resource limits",
  });
  assert.equal(createdSession.status, 200, createdSession.text);
  assert.equal(
    nested(createdSession.json, ["data", "outcome", "status"]),
    "completed",
  );
  assert.equal(
    nested(createdSession.json, [
      "data",
      "outcome",
      "result",
      "resourceLimits",
      "workdir",
    ]),
    workdir,
  );
  const createdTools = nested(createdSession.json, [
    "data",
    "outcome",
    "result",
    "toolProfile",
    "tools",
  ]);
  assert.ok(
    Array.isArray(createdTools),
    `created session must report its tool profile; response ${createdSession.text}`,
  );
  assert.equal(
    createdTools.some((tool) => nested(tool, ["name"]) === "terminal"),
    true,
    `created session tool profile must include terminal; observed ${JSON.stringify(createdTools)}`,
  );

  const omitted = await createSession({
    sessionId: omittedSessionId,
    agentId: `${profileId}-default-agent`,
    profileId,
    kind: "full",
    reason: "task 5846 omitted resource limits",
  });
  assert.equal(
    nested(omitted.json, ["data", "outcome", "status"]),
    "completed",
  );
  assert.equal(
    nested(omitted.json, [
      "data",
      "outcome",
      "result",
      "resourceLimits",
      "workdir",
    ]),
    undefined,
  );

  for (const [label, invalidWorkdir] of [
    ["blank", "   "],
    ["relative", "relative/benchmark"],
  ] as const) {
    const invalid = await createSession({
      sessionId: `${profileId}-${label}-session`,
      agentId: `${profileId}-${label}-agent`,
      profileId,
      kind: "full",
      resourceLimits: { workdir: invalidWorkdir },
      reason: `task 5846 rejects ${label} workdir`,
    });
    assert.equal(invalid.status, 200, invalid.text);
    assert.equal(nested(invalid.json, ["data", "outcome", "status"]), "failed");
    assert.match(
      String(nested(invalid.json, ["data", "outcome", "summary"])),
      label === "blank" ? /must not be blank/ : /must be an absolute path/,
    );
  }

  assertSessionLimits(await readSessionDiagnostics(), workdir);
  assertZeroDelegationToolContext(await readToolContext());
  await restartDebugService();
  assertSessionLimits(await readSessionDiagnostics(), workdir);
  assertZeroDelegationToolContext(await readToolContext());

  const messageId = `resource-limits-message-${suffix}`;
  const sent = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "resource-limits-cert-operator", kind: "human" },
      body: [
        "Call the terminal tool exactly once with command pwd and do not pass a cwd override.",
        "Then reply with exactly WORKDIR=<the trimmed command output>.",
      ].join("\n"),
      client_message_id: messageId,
      reason: "task 5846 local-code workdir certification",
    },
    { "Idempotency-Key": messageId },
  );
  assert.equal(sent.status, 202, sent.text);
  const events = await waitForTerminalEvents(sessionId, 180_000);
  const terminalCompleted = events.some(
    (event) =>
      event.kind === "tool_call_completed" &&
      event.payload.tool_name === "terminal",
  );
  assert.equal(
    terminalCompleted,
    true,
    `live turn must complete the terminal tool; observed ${JSON.stringify(
      events.map((event) => ({ kind: event.kind, payload: event.payload })),
    )}`,
  );
  const assistantText = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => String(event.payload.text ?? ""))
    .join("");
  assert.match(assistantText, new RegExp(`WORKDIR=${escapeRegExp(workdir)}`));

  evidence.profileId = profileId;
  evidence.sessionId = sessionId;
  evidence.workdir = workdir;
  evidence.persistedAcrossRestart = true;
  evidence.zeroDelegationCatalogEnforced = true;
  evidence.omissionPreserved = true;
  evidence.invalidWorkdirsRejected = ["blank", "relative"];
  evidence.terminalToolCompleted = true;
  evidence.assistantText = assistantText;
  console.log(JSON.stringify(evidence, null, 2));
} finally {
  await waitForHealth().catch(() => undefined);
  const cleanup = await request(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
    {
      reason: "task 5846 certification cleanup",
      confirmProfileId: profileId,
    },
  ).catch(() => undefined);
  if (
    cleanup !== undefined &&
    nested(cleanup.json, ["data", "outcome", "status"]) !== "completed"
  ) {
    console.error(
      `resource limits certification cleanup failed: ${cleanup.text}`,
    );
  }
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

interface ChatEvent {
  kind: string;
  payload: Record<string, unknown>;
}

async function createSession(
  body: Record<string, unknown>,
): Promise<ApiResponse> {
  return request("POST", "/v1/admin/control/sessions", body);
}

async function readSessionDiagnostics(): Promise<Record<string, unknown>> {
  const response = await request(
    "GET",
    `/v1/admin/diagnostics/sessions?profile_id=${encodeURIComponent(profileId)}&limit=100`,
  );
  assert.equal(response.status, 200, response.text);
  const items = nested(response.json, ["data", "items"]);
  assert.ok(Array.isArray(items), "session diagnostics must return items");
  const session = items.find(
    (item) => nested(item, ["sessionId"]) === sessionId,
  );
  assert.ok(session && typeof session === "object" && !Array.isArray(session));
  return session as Record<string, unknown>;
}

async function readToolContext(): Promise<Record<string, unknown>> {
  const response = await request(
    "GET",
    `/v1/debug/sessions/${encodeURIComponent(sessionId)}/context`,
  );
  assert.equal(response.status, 200, response.text);
  return response.json;
}

function assertZeroDelegationToolContext(
  response: Record<string, unknown>,
): void {
  const selectedTools = nested(response, ["data", "selectedTools"]);
  assert.ok(Array.isArray(selectedTools), "debug context must report tools");
  const selectedNames = selectedTools.map((tool) =>
    String(nested(tool, ["name"])),
  );
  for (const expected of ["read_file", "patch", "terminal"]) {
    assert.equal(
      selectedNames.includes(expected),
      true,
      `zero-delegation context must retain ${expected}`,
    );
  }
  for (const denied of [
    "spawn_subagent",
    "fan_out_subagents",
    "scout_codebase",
  ]) {
    assert.equal(
      selectedNames.includes(denied),
      false,
      `zero-delegation context must omit ${denied}`,
    );
  }
}

function assertSessionLimits(
  session: Record<string, unknown>,
  expectedWorkdir: string,
): void {
  assert.equal(nested(session, ["resourceLimits", "workdir"]), expectedWorkdir);
  assert.equal(nested(session, ["resourceLimits", "maxDurationMs"]), 120_000);
  assert.equal(nested(session, ["resourceLimits", "maxDelegationDepth"]), 0);
}

async function restartDebugService(): Promise<void> {
  await execFileAsync("systemctl", [
    "--user",
    "restart",
    "rusty-crew-debug.service",
  ]);
  await waitForHealth();
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const response = await fetch(`${baseUrl}/v1/admin/healthz`).catch(
      () => undefined,
    );
    if (response?.ok) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("debug service did not become healthy within 60 seconds");
}

async function waitForTerminalEvents(
  targetSessionId: string,
  timeoutMs: number,
): Promise<ChatEvent[]> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const response = await request(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(targetSessionId)}/events?limit=500`,
    );
    assert.equal(response.status, 200, response.text);
    const items = nested(response.json, ["data", "items"]);
    const events = Array.isArray(items) ? (items as ChatEvent[]) : [];
    if (events.some((event) => event.kind === "assistant_message_completed")) {
      return events;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("live resource-limits turn did not finish within timeout");
}

async function request(
  method: string,
  path: string,
  body?: unknown,
  requestHeaders: Record<string, string> = {},
): Promise<ApiResponse> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...requestHeaders,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(30_000),
  });
  const text = await response.text();
  return {
    status: response.status,
    text,
    json: text.trim() ? (JSON.parse(text) as Record<string, unknown>) : {},
  };
}

function nested(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const key of path) {
    if (
      typeof current !== "object" ||
      current === null ||
      Array.isArray(current)
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
