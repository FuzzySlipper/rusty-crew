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
const narratorProfileId = `mechanic-read-narrator-${suffix}`;
const narratorSessionId = `mechanic-read-rp-${suffix}`;
const mechanicProfileId = `mechanic-read-agent-${suffix}`;
const mechanicSessionId = `${mechanicProfileId}-session`;
const layerId = `mechanic-read-layer-${suffix}`;
const loreId = `mechanic-read-lore-${suffix}`;
const marker = `MECHANIC_READ_${suffix.toUpperCase()}`;

if (new URL(baseUrl).port !== "9348") {
  throw new Error(`mechanic diagnostics live smoke requires debug port 9348`);
}

try {
  await createProfile({
    profileId: narratorProfileId,
    displayName: "Mechanic read target narrator",
    providerAlias,
    localToolProfileId: "roleplay_lore",
    brain: { module: "pi-agent", strategy: "roleplay_narrator" },
  });
  const metadata = await api("POST", "/v1/admin/roleplay/sessions", {
    sessionId: narratorSessionId,
    profileId: narratorProfileId,
    displayName: "Mechanic diagnostic target",
  });
  assert.equal(metadata.status, 200, metadata.text);
  await apiOk("POST", "/v1/admin/roleplay/lore/layers", {
    layer_id: layerId,
    profile_id: narratorProfileId,
    name: "Mechanic diagnostic world",
    description: "Live diagnostic fixture.",
    purpose: "world",
    write_policy: "auto_capture",
    now: new Date().toISOString(),
  });
  await apiOk("POST", "/v1/admin/roleplay/lore/chat-layers", {
    chat_id: narratorSessionId,
    layers: [{ layer_id: layerId, priority: 0, enabled: true }],
    now: new Date().toISOString(),
  });
  await apiOk("POST", "/v1/admin/roleplay/lore/facts/capture", {
    layer_id: layerId,
    write: {
      record_id: loreId,
      world_id: `world-${suffix}`,
      entity_id: "brass-observatory",
      session_id: narratorSessionId,
      shape: { shape_id: "lore_entry", version: 1 },
      canon_status: "canon",
      visibility: "public",
      title: "Brass Observatory",
      body: "The observatory door opens only when the blue bell rings.",
      content: {
        world_id: `world-${suffix}`,
        entity_id: "brass-observatory",
        title: "Brass Observatory",
        body: "The observatory door opens only when the blue bell rings.",
        canon_status: "canon",
        visibility: "public",
        metadata_json: { tags: ["observatory", "blue-bell"] },
      },
      evidence_refs: [{ evidence_type: "ui", ref_id: marker }],
      source: "ui",
      confidence: 1,
      durability_rationale: "Task 5689 live diagnostic fixture.",
      now: new Date().toISOString(),
    },
    is_constant: true,
    priority: 0,
    capture_reason: "task-5689-live",
  });

  const narratorEvents = await sendAndWait(
    narratorSessionId,
    "At the brass observatory, Mira hears the blue bell and reaches for the door. Continue the scene in a short paragraph.",
  );
  assertToolCompleted(narratorEvents, "recall_lore");
  const sessionRead = await apiOk(
    "GET",
    `/v1/admin/roleplay/sessions/${narratorSessionId}`,
  );
  assert.match(JSON.stringify(sessionRead), /narratorDiagnostic/);
  assert.match(JSON.stringify(sessionRead), /sceneBrief/);

  await restartDebugService();
  await waitForHealth();

  await createProfile({
    profileId: mechanicProfileId,
    sessionId: mechanicSessionId,
    displayName: "Mechanic read live certification",
    providerAlias,
    localToolProfileId: "basic_chat",
  });
  await apiOk(
    "PATCH",
    `/v1/admin/roleplay/profiles/${mechanicProfileId}/mechanic-config`,
    { name: "Maren", providerAlias, autoMonitor: false },
  );
  const mechanicEvents = await sendAndWait(
    mechanicSessionId,
    [
      `Use sessionId ${narratorSessionId} for every call.`,
      "Call inspect_roleplay_transcript, inspect_roleplay_scene, and inspect_lore_retrieval before answering.",
      `Then write ${marker} and briefly identify the observed scene, lore decision evidence, and selected transcript state.`,
    ].join("\n"),
  );
  for (const toolName of [
    "inspect_roleplay_transcript",
    "inspect_roleplay_scene",
    "inspect_lore_retrieval",
  ]) {
    assertToolCompleted(mechanicEvents, toolName);
  }
  const assistantText = mechanicEvents
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => String(event.payload?.text ?? ""))
    .join("");
  assert.match(assistantText, new RegExp(marker));
  assert.match(assistantText, /observatory/i);

  console.log(
    JSON.stringify({
      baseUrl,
      narratorSessionId,
      mechanicSessionId,
      providerAlias,
      persistedAcrossRestart: true,
      completedTools: completedTools(mechanicEvents),
      assistantPreview: assistantText.slice(0, 500),
    }),
  );
} finally {
  for (const profileId of [mechanicProfileId, narratorProfileId]) {
    await api("POST", `/v1/admin/control/profiles/${profileId}/delete`, {
      confirmProfileId: profileId,
      reason: "task-5689 live mechanic diagnostics cleanup",
    }).catch(() => undefined);
  }
}

interface EventRecord {
  kind: string;
  payload?: Record<string, unknown>;
}

async function createProfile(input: Record<string, unknown>): Promise<void> {
  const response = await apiOk("POST", "/v1/admin/control/profiles", {
    kind: "full",
    reason: "task-5689 live mechanic diagnostics",
    ...input,
  });
  assert.equal(response.outcome?.status, "completed");
}

async function sendAndWait(
  sessionId: string,
  body: string,
): Promise<EventRecord[]> {
  const before = await apiOk("GET", `/v1/chat/sessions/${sessionId}`);
  const cursor = String(
    before.session?.latest_cursor ?? before.latest_cursor ?? "0",
  );
  const key = `task-5689:${sessionId}:${Date.now()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${sessionId}/messages`,
    {
      actor: { id: "task-5689-operator", kind: "human" },
      body,
      client_message_id: key,
      reason: "task-5689 live mechanic diagnostics",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);
  const events: EventRecord[] = [];
  let nextCursor = cursor;
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    const page = await apiOk(
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
  assert.ok(completedTools(events).includes(name), JSON.stringify(events));
}

function completedTools(events: EventRecord[]): string[] {
  return events
    .filter(
      (event) =>
        event.kind === "tool_call_completed" &&
        event.payload?.is_error !== true,
    )
    .map((event) => String(event.payload?.tool_name ?? ""))
    .filter(Boolean);
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

async function apiOk(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
): Promise<Record<string, any>> {
  const response = await api(method, path, body);
  assert.ok(response.status < 400, response.text);
  assert.equal(response.json.ok, true, response.text);
  return (response.json.data ?? {}) as Record<string, any>;
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
