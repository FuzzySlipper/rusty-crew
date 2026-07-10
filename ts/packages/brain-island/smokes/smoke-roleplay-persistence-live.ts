import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile, rm, writeFile } from "node:fs/promises";

const BASE_URL = (
  process.env.RUSTY_CREW_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const MODE = process.env.RUSTY_CREW_ROLEPLAY_CERT_MODE ?? "prepare";
const PROVIDER_ALIAS =
  process.env.RUSTY_CREW_ROLEPLAY_CERT_PROVIDER_ALIAS ?? "deepseek-flash";
const STATE_PATH =
  process.env.RUSTY_CREW_ROLEPLAY_CERT_STATE_PATH ??
  `/tmp/rusty-crew-roleplay-cert-${new URL(BASE_URL).port}.json`;
const TURN_TIMEOUT_MS = 180_000;

interface CertificationState {
  baseUrl: string;
  backend: string;
  profileId: string;
  runtimeSessionId: string;
  sessionId: string;
  characterId: string;
  personaId: string;
  sourceLayerId: string;
  durableLayerId: string;
  sourceRecordId: string;
  promotedRecordId: string;
  terminalSlotId: string;
  primaryVariantId: string;
  generatedVariantId: string;
  manualVariantId: string;
  generatedWakeId?: string;
  preparedAt: string;
}

interface ChatEvent {
  kind: string;
  payload?: Record<string, unknown>;
}

if (MODE === "prepare") {
  await prepare();
} else if (MODE === "verify") {
  await verify();
} else if (MODE === "cleanup") {
  await cleanup();
} else {
  throw new Error(
    `unsupported RUSTY_CREW_ROLEPLAY_CERT_MODE ${MODE}; expected prepare, verify, or cleanup`,
  );
}

async function prepare(): Promise<void> {
  const storage = await api("GET", "/v1/admin/diagnostics/storage");
  const backend = requiredString(storage.backend, "storage backend");
  const prefix = `rp-cert-${backend}-${Date.now()}`;
  const state: CertificationState = {
    baseUrl: BASE_URL,
    backend,
    profileId: `${prefix}-profile`,
    runtimeSessionId: `${prefix}-agent-session`,
    sessionId: `${prefix}-rp-session`,
    characterId: `${prefix}-character`,
    personaId: `${prefix}-persona`,
    sourceLayerId: `${prefix}-capture`,
    durableLayerId: `${prefix}-durable`,
    sourceRecordId: `${prefix}-clockmaker-source`,
    promotedRecordId: `${prefix}-clockmaker-canon`,
    terminalSlotId: "",
    primaryVariantId: "",
    generatedVariantId: `${prefix}-generated-alt`,
    manualVariantId: `${prefix}-manual-alt`,
    preparedAt: new Date().toISOString(),
  };

  await api("POST", "/v1/admin/control/profiles", {
    profileId: state.profileId,
    displayName: `Roleplay live certification (${backend})`,
    providerAlias: PROVIDER_ALIAS,
    kind: "full",
    brain: { module: "pi-agent", strategy: "roleplay_narrator" },
    localToolProfileId: "roleplay_lore",
    sessionId: state.runtimeSessionId,
    agentId: `${prefix}-agent`,
    reason: `task-5390-${backend}`,
  });

  await createLayer(state, state.sourceLayerId, "story", "auto_capture");
  await createLayer(state, state.durableLayerId, "world", "manual");

  await api("POST", "/v1/admin/roleplay/lore/facts/capture", {
    layer_id: state.sourceLayerId,
    write: loreWrite(state, state.sourceRecordId),
    is_constant: false,
    priority: 4,
    capture_reason: "task-5390-live-certification",
  });
  const promotion = await api(
    "POST",
    `/v1/admin/roleplay/lore/entries/${encodeURIComponent(state.sourceRecordId)}/promote`,
    {
      sourceLayerId: state.sourceLayerId,
      targetLayerId: state.durableLayerId,
      newRecordId: state.promotedRecordId,
      isConstant: true,
      priority: 9,
    },
  );
  assert.equal(promotion.promoted, true);

  await api(
    "POST",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(state.profileId)}/characters`,
    {
      id: state.characterId,
      name: "Elara Voss",
      description: "A guarded clockmaker with an obsidian locket.",
      personality: "observant, dryly funny, and slow to trust",
      scenario:
        "{{char}} meets {{user}} beneath the silver orchard after the clockmaker's three-note song.",
      firstMessage: "You heard the song too, then.",
      alternateGreetings: ["The orchard remembers you."],
      exampleMessages: ["Keep your voice down. The silver leaves listen."],
      tags: ["task-5390", backend],
    },
  );
  await api(
    "POST",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(state.profileId)}/personas`,
    {
      id: state.personaId,
      displayName: "Rowan",
      description: "A patient investigator who notices small physical details.",
      notes: "Uses first person and asks direct questions.",
    },
  );
  await api(
    "PATCH",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(state.profileId)}/narrator-config`,
    {
      tone: "wry",
      pacing: "balanced",
      explicitness: "implied",
      memoryDepth: "deep",
      stylePrompt:
        "Write clean in-world prose with concrete sensory detail and no technical narration.",
      exemplar: "The three notes faded; the silver leaves kept trembling.",
      review: { enabled: true, maxReviewCycles: 1 },
    },
  );
  await api("POST", "/v1/admin/roleplay/sessions", {
    sessionId: state.sessionId,
    profileId: state.profileId,
    displayName: `Task 5390 ${backend} scene`,
    playerPersonaId: state.personaId,
    characterId: state.characterId,
    activeLayerIds: [state.durableLayerId, state.sourceLayerId],
  });

  const firstTurn = await sendTurn(
    state.sessionId,
    "I hold up the obsidian locket as the clockmaker's three-note song fades. 'Elara, why does the serpent-and-rose crest match the orchard gate?'",
  );
  assertNarratorTurn(firstTurn.events, firstTurn.text);

  const alternatives = await api(
    "GET",
    `/v1/admin/roleplay/sessions/${encodeURIComponent(state.sessionId)}/alternatives`,
  );
  state.terminalSlotId = requiredString(
    alternatives.slot?.slot_id,
    "terminal slot id",
  );
  state.primaryVariantId = requiredString(
    alternatives.slot?.primary_variant_id,
    "primary variant id",
  );

  const generated = await api(
    "POST",
    `/v1/admin/roleplay/sessions/${encodeURIComponent(state.sessionId)}/alternatives/generate`,
    {
      slotId: state.terminalSlotId,
      variantId: state.generatedVariantId,
      messageId: `${prefix}-generated-message`,
      instructions:
        "Write a distinct concise alternative that still mentions the locket or the three-note song.",
    },
  );
  assert.equal(generated.status, "generated");
  assert.equal(generated.slot.active_variant_id, state.generatedVariantId);
  assert.ok(
    requiredString(generated.variant?.message?.body, "generated body").length >
      40,
  );
  state.generatedWakeId = optionalString(
    generated.variant?.message?.metadata_json?.wake_id,
  );

  const manual = await api(
    "POST",
    `/v1/admin/roleplay/sessions/${encodeURIComponent(state.sessionId)}/alternatives`,
    {
      slotId: state.terminalSlotId,
      variantId: state.manualVariantId,
      messageId: `${prefix}-manual-message`,
      body: "Elara closes her hand around the locket. The third note dies beneath the silver leaves.",
    },
  );
  assert.equal(manual.status, "created");
  await selectVariant(state, state.manualVariantId);
  const selected = await selectVariant(state, state.generatedVariantId);
  assert.equal(selected.slot.active_variant_id, state.generatedVariantId);

  if (state.generatedWakeId) {
    const events = await chatEvents(state.sessionId);
    assert.deepEqual(
      events.filter(
        (event) =>
          event.payload?.wake_id === state.generatedWakeId &&
          [
            "assistant_turn_started",
            "assistant_text_delta",
            "assistant_reasoning_delta",
            "assistant_turn_finished",
            "assistant_message_completed",
          ].includes(event.kind),
      ),
      [],
      "generated alternatives must not append a normal assistant wake turn",
    );
  }

  await writeFile(STATE_PATH, `${JSON.stringify(state, null, 2)}\n`, "utf8");
  console.log(
    JSON.stringify(
      {
        phase: "prepared",
        backend,
        statePath: STATE_PATH,
        profileId: state.profileId,
        sessionId: state.sessionId,
        narrativeChars: firstTurn.text.length,
        generatedVariantId: state.generatedVariantId,
      },
      null,
      2,
    ),
  );
}

async function verify(): Promise<void> {
  const state = await loadState();
  assert.equal(state.baseUrl, BASE_URL);
  const storage = await api("GET", "/v1/admin/diagnostics/storage");
  assert.equal(storage.backend, state.backend);

  const characters = await api(
    "GET",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(state.profileId)}/characters`,
  );
  assert.ok(
    characters.items.some(
      (item: Record<string, unknown>) => item.id === state.characterId,
    ),
  );
  const personas = await api(
    "GET",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(state.profileId)}/personas`,
  );
  assert.ok(
    personas.items.some(
      (item: Record<string, unknown>) => item.id === state.personaId,
    ),
  );
  const narrator = await api(
    "GET",
    `/v1/admin/roleplay/profiles/${encodeURIComponent(state.profileId)}/narrator-config`,
  );
  assert.equal(narrator.config.tone, "wry");
  assert.equal(narrator.config.memoryDepth, "deep");

  const session = await api(
    "GET",
    `/v1/admin/roleplay/sessions/${encodeURIComponent(state.sessionId)}`,
  );
  assert.equal(session.session.character_id, state.characterId);
  assert.equal(session.session.player_persona_id, state.personaId);
  assert.deepEqual(session.session.active_layer_ids, [
    state.durableLayerId,
    state.sourceLayerId,
  ]);

  const promoted = await api(
    "GET",
    `/v1/admin/roleplay/lore/entries/${encodeURIComponent(state.promotedRecordId)}?layer_id=${encodeURIComponent(state.durableLayerId)}`,
  );
  assert.equal(promoted.entry.record_id, state.promotedRecordId);
  assert.equal(promoted.supersession.supersedesRecordId, state.sourceRecordId);
  assert.equal(promoted.layerEntries[0]?.is_constant, true);

  const slots = await api(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(state.sessionId)}/slots?include_alternates=true`,
  );
  const terminal = slots.items.find(
    (slot: Record<string, unknown>) => slot.slot_id === state.terminalSlotId,
  );
  assert.ok(terminal, "terminal roleplay slot should survive restart");
  assert.equal(terminal.active_variant_id, state.generatedVariantId);
  assert.ok(
    terminal.alternates.some(
      (variant: Record<string, unknown>) =>
        variant.variant_id === state.generatedVariantId,
    ),
  );
  assert.ok(
    terminal.alternates.some(
      (variant: Record<string, unknown>) =>
        variant.variant_id === state.manualVariantId,
    ),
  );

  if (state.generatedWakeId) {
    const events = await chatEvents(state.sessionId);
    assert.equal(
      events.some(
        (event) =>
          event.payload?.wake_id === state.generatedWakeId &&
          event.kind === "assistant_text_delta",
      ),
      false,
      "generated-alternative provider text must remain outside normal chat replay",
    );
  }

  const followup = await sendTurn(
    state.sessionId,
    "Continue from the selected alternative. I ask Elara what the three-note song unlocks, keeping the obsidian locket in view.",
  );
  assertNarratorTurn(followup.events, followup.text);
  assert.match(followup.text, /locket|song|note|orchard/i);

  console.log(
    JSON.stringify(
      {
        phase: "verified_after_restart",
        backend: state.backend,
        profileId: state.profileId,
        sessionId: state.sessionId,
        selectedVariantId: state.generatedVariantId,
        followupNarrativeChars: followup.text.length,
      },
      null,
      2,
    ),
  );
}

async function cleanup(): Promise<void> {
  const state = await loadState();
  const result = await api(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(state.profileId)}/delete`,
    {
      reason: "task-5390-live-certification-cleanup",
      confirmProfileId: state.profileId,
    },
  );
  assert.equal(result.outcome.status, "completed");
  await rm(STATE_PATH, { force: true });
  console.log(
    JSON.stringify(
      { phase: "cleaned", backend: state.backend, profileId: state.profileId },
      null,
      2,
    ),
  );
}

async function createLayer(
  state: CertificationState,
  layerId: string,
  purpose: "story" | "world",
  writePolicy: "auto_capture" | "manual",
): Promise<void> {
  await api("POST", "/v1/admin/roleplay/lore/layers", {
    layer_id: layerId,
    profile_id: state.profileId,
    name: purpose === "story" ? "Captured Story" : "Durable World",
    description: `Task 5390 ${state.backend} certification layer.`,
    purpose,
    write_policy: writePolicy,
  });
}

function loreWrite(
  state: CertificationState,
  recordId: string,
): Record<string, unknown> {
  const now = new Date().toISOString();
  const body =
    "The obsidian locket bears a serpent-and-rose crest. The clockmaker's three-note song opens the silver orchard gate.";
  return {
    record_id: recordId,
    world_id: state.profileId,
    entity_id: state.characterId,
    session_id: state.sessionId,
    shape: { shape_id: "lore_entry", version: 1 },
    canon_status: "draft",
    visibility: "public",
    title: "Obsidian locket and the three-note song",
    body,
    content: {
      world_id: state.profileId,
      entity_id: state.characterId,
      title: "Obsidian locket and the three-note song",
      body,
      canon_status: "draft",
      visibility: "public",
      metadata_json: { tags: ["locket", "clockmaker", "silver-orchard"] },
    },
    evidence_refs: [
      {
        evidence_type: "other",
        ref_id: `task-5390-${state.backend}`,
        label: "live certification fixture",
      },
    ],
    source: "import",
    confidence: 1,
    durability_rationale: "Task 5390 cross-backend live persistence proof.",
    now,
  };
}

async function selectVariant(
  state: CertificationState,
  variantId: string,
): Promise<any> {
  return api(
    "POST",
    `/v1/admin/roleplay/sessions/${encodeURIComponent(state.sessionId)}/alternatives/${encodeURIComponent(state.terminalSlotId)}/select`,
    { variantId },
  );
}

async function sendTurn(
  sessionId: string,
  body: string,
): Promise<{ events: ChatEvent[]; text: string }> {
  const sessionBefore = await api(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}`,
  );
  const cursor = optionalString(
    sessionBefore.session?.latest_cursor ?? sessionBefore.latest_cursor,
  );
  await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-5390-user", kind: "human", display_name: "Rowan" },
      body,
      client_message_id: `task-5390-${randomUUID()}`,
    },
  );
  const events = await waitForTurn(sessionId, cursor);
  const failed = events.find(
    (event) =>
      event.kind === "assistant_turn_finished" &&
      event.payload?.status === "failed",
  );
  assert.equal(failed, undefined, JSON.stringify(failed));
  return {
    events,
    text: events
      .filter((event) => event.kind === "assistant_text_delta")
      .map((event) => String(event.payload?.text ?? ""))
      .join(""),
  };
}

async function waitForTurn(
  sessionId: string,
  cursor: string | undefined,
): Promise<ChatEvent[]> {
  const deadline = Date.now() + TURN_TIMEOUT_MS;
  let nextCursor = cursor;
  const events: ChatEvent[] = [];
  while (Date.now() < deadline) {
    const url = new URL(
      `${BASE_URL}/v1/chat/sessions/${encodeURIComponent(sessionId)}/events`,
    );
    if (nextCursor) url.searchParams.set("cursor", nextCursor);
    url.searchParams.set("limit", "1000");
    const page = await fetchJson(url.toString());
    const items = Array.isArray(page.items) ? (page.items as ChatEvent[]) : [];
    if (items.length > 0) {
      events.push(...items);
      nextCursor = optionalString(page.latest_cursor) ?? nextCursor;
      if (
        items.some(
          (event) =>
            event.kind === "assistant_turn_finished" ||
            event.kind === "stream_error",
        )
      ) {
        return events;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`timed out waiting for roleplay turn in ${sessionId}`);
}

function assertNarratorTurn(events: ChatEvent[], text: string): void {
  const phases = events
    .filter((event) => event.kind === "phase_change")
    .map((event) => String(event.payload?.phase ?? ""));
  const tools = events
    .filter((event) => event.kind === "tool_call_started")
    .map((event) => String(event.payload?.tool_name ?? ""));
  assert.ok(phases.includes("exploring"), JSON.stringify(phases));
  assert.ok(phases.includes("composing"), JSON.stringify(phases));
  assert.ok(tools.includes("recall_lore"), JSON.stringify(tools));
  assert.ok(tools.includes("get_scene_state"), JSON.stringify(tools));
  assert.ok(text.length > 40, `narrative was only ${text.length} characters`);
  assert.doesNotMatch(
    text,
    /```json|\[TOOL_CALL\]|function_call|system:|recall_lore|sceneBrief/i,
  );
}

async function chatEvents(sessionId: string): Promise<ChatEvent[]> {
  const result = await api(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?limit=1000`,
  );
  return Array.isArray(result.items) ? result.items : [];
}

async function loadState(): Promise<CertificationState> {
  return JSON.parse(await readFile(STATE_PATH, "utf8")) as CertificationState;
}

async function api(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
): Promise<any> {
  const response = await fetch(`${BASE_URL}${path}`, {
    method,
    headers:
      body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const envelope = (await response.json()) as any;
  if (!response.ok || envelope.ok !== true) {
    throw new Error(
      `${method} ${path} failed (${response.status}): ${JSON.stringify(envelope)}`,
    );
  }
  return envelope.data;
}

async function fetchJson(url: string): Promise<any> {
  const response = await fetch(url);
  const envelope = (await response.json()) as any;
  if (!response.ok || envelope.ok !== true) {
    throw new Error(`GET ${url} failed (${response.status})`);
  }
  return envelope.data;
}

function requiredString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} was missing`);
  }
  return value;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}
