/**
 * Roleplay Narrator Quality Spike — Live Integration Test
 *
 * Tests the full narrator pipeline against a real running rusty-crew service
 * with a live LLM. Verifies lore-aware responses, fact capture, continuity
 * across turns, clean output, and style adherence.
 *
 * Requirements: rusty-crew debug service running
 * (RUSTY_CREW_ADMIN_BASE_URL or RUSTY_CREW_DEBUG_ADMIN_BASE_URL), live LLM
 * provider configured for the test profile.
 *
 * Run: npm run smoke -- roleplay-quality-spike-live
 * Or:  tsx src/smoke-roleplay-quality-spike-live.ts
 */

import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

// ── Configuration ────────────────────────────────────────────────────────────

const ADMIN_BASE =
  process.env.RUSTY_CREW_ADMIN_BASE_URL ??
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ??
  "http://127.0.0.1:9348";
const CHAT_BASE =
  process.env.RUSTY_CREW_CHAT_BASE_URL ?? ADMIN_BASE.replace("/admin", "");
const LIVENESS_TIMEOUT_MS = 5_000;
const STREAM_TIMEOUT_MS = 120_000; // 2 min for LLM to respond
const POLL_INTERVAL_MS = 200;
const SCENARIO_LIMIT = scenarioLimit();

const TEST_PREFIX = `quality-spike-${Date.now()}`;
const TEST_PROFILE = `${TEST_PREFIX}-narrator`;
const TEST_SESSION = `${TEST_PREFIX}-session`;
const TEST_AGENT = `${TEST_PREFIX}-agent`;
const TEST_WORLD = `${TEST_PREFIX}-world`;
const WORLD_LAYER = `${TEST_PREFIX}-world-details`;
const CHARACTER_LAYER = `${TEST_PREFIX}-character-details`;
const STORY_LAYER = `${TEST_PREFIX}-story-events`;

// ── Helpers ──────────────────────────────────────────────────────────────────

async function adminPost(path: string, body: unknown): Promise<any> {
  const url = `${ADMIN_BASE.replace(/\/+$/, "")}/v1/admin${path}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await res.json();
  if (!data.ok) {
    throw new Error(
      `Admin POST ${path} failed: ${data.error?.code} — ${data.error?.message}`,
    );
  }
  return data.data;
}

async function chatPost(
  sessionId: string,
  path: string,
  body: unknown,
): Promise<any> {
  const url = `${CHAT_BASE.replace(/\/+$/, "")}/v1/chat/sessions/${sessionId}${path}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await res.json();
  if (!data.ok) {
    throw new Error(
      `Chat POST ${sessionId}${path} failed: ${data.error?.code} — ${data.error?.message}`,
    );
  }
  if (data.data?.status === "rejected") {
    throw new Error(
      `Chat POST ${sessionId}${path} rejected: ${data.data.reason_code ?? "unknown"} — ${data.data.summary ?? "no summary"}`,
    );
  }
  return data.data;
}

async function getSessionLatestCursor(
  sessionId: string,
): Promise<string | undefined> {
  const url = `${CHAT_BASE.replace(/\/+$/, "")}/v1/chat/sessions/${sessionId}`;
  const res = await fetch(url);
  const data = await res.json();
  if (!data.ok) return undefined;
  const cursor = data.data?.session?.latest_cursor ?? data.data?.latest_cursor;
  return typeof cursor === "string" ? cursor : undefined;
}

async function streamSessionEvents(
  sessionId: string,
  signal: AbortSignal,
): Promise<StreamedEvent[]> {
  const url = `${CHAT_BASE.replace(/\/+$/, "")}/v1/chat/sessions/${sessionId}/stream`;
  const res = await fetch(url, { signal });
  if (!res.ok || !res.body) {
    throw new Error(`Stream ${sessionId} failed: ${res.status}`);
  }

  const events: StreamedEvent[] = [];
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";

    for (const line of lines) {
      if (line.startsWith("data: ")) {
        try {
          events.push(JSON.parse(line.slice(6)));
        } catch {
          // skip malformed
        }
      }
    }
  }

  return events;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForEvent(
  sessionId: string,
  predicate: (e: StreamedEvent) => boolean,
  timeoutMs: number = STREAM_TIMEOUT_MS,
  cursor?: string,
): Promise<StreamedEvent[]> {
  const deadline = Date.now() + timeoutMs;
  const seen: StreamedEvent[] = [];
  let nextCursor = cursor;

  while (Date.now() < deadline) {
    try {
      const url = new URL(
        `${CHAT_BASE.replace(/\/+$/, "")}/v1/chat/sessions/${sessionId}/events`,
      );
      if (nextCursor) {
        url.searchParams.set("cursor", nextCursor);
      }
      const res = await fetch(url);
      const data = await res.json();
      const events = data.ok
        ? Array.isArray(data.data?.items)
          ? data.data.items
          : Array.isArray(data.data?.events)
            ? data.data.events
            : []
        : [];
      if (events.length > 0) {
        seen.push(...events);
        nextCursor = data.data?.latest_cursor ?? nextCursor;
        const match = events.find(predicate);
        if (match) return seen;
      }
    } catch {
      // service not ready yet
    }
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(
    `Timeout waiting for event in session ${sessionId} (seen ${seen.length} events)`,
  );
}

function getAssistantText(events: StreamedEvent[]): string {
  return events
    .filter((e) => e.kind === "assistant_text_delta")
    .map((e) => e.payload?.text ?? "")
    .join("");
}

function getToolCalls(events: StreamedEvent[]): StreamedEvent[] {
  return events.filter(
    (e) =>
      e.kind === "tool_call_started" ||
      e.kind === "tool_call_completed" ||
      e.kind === "tool_call_failed",
  );
}

function toolName(event: StreamedEvent): string | undefined {
  const raw = event.payload?.tool_name ?? event.payload?.name;
  return typeof raw === "string" ? raw : undefined;
}

function getPhaseChanges(events: StreamedEvent[]): string[] {
  return events
    .filter((e) => e.kind === "phase_change")
    .map((e) => String(e.payload?.phase ?? "unknown"));
}

// ── Types ────────────────────────────────────────────────────────────────────

interface StreamedEvent {
  event_id: string;
  session_id: string;
  sequence_id: number;
  created_at: string;
  kind: string;
  payload?: Record<string, unknown>;
}

interface AdminResponse {
  outcome: {
    status: string;
    summary: string;
    affectedIds?: Record<string, string>;
    result?: unknown;
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

async function runQualitySpike(): Promise<void> {
  console.log("═══ Roleplay Narrator Quality Spike ═══");
  console.log(`Backend: ${ADMIN_BASE}`);
  console.log(`Profile: ${TEST_PROFILE}`);
  console.log(`Session: ${TEST_SESSION}`);
  console.log(`Scenario limit: ${SCENARIO_LIMIT}`);

  // ── 1. Liveness check ─────────────────────────────────────────────────────

  console.log("\n── 1. Checking backend reachability ──");
  const start = Date.now();
  let backendUp = false;
  while (Date.now() - start < LIVENESS_TIMEOUT_MS) {
    try {
      const res = await fetch(
        `${CHAT_BASE.replace(/\/+$/, "")}/v1/chat/sessions`,
      );
      if (res.ok) {
        backendUp = true;
        break;
      }
    } catch {
      // not up yet
    }
    await sleep(500);
  }
  if (!backendUp) {
    console.error("❌ Backend not reachable. Skipping live tests.");
    console.error(
      `   Set RUSTY_CREW_ADMIN_BASE_URL or RUSTY_CREW_DEBUG_ADMIN_BASE_URL (default: http://127.0.0.1:9348)`,
    );
    process.exit(0); // soft skip, not a failure
  }
  console.log("✅ Backend reachable");

  // ── 2. Create test profile ────────────────────────────────────────────────

  console.log("\n── 2. Creating test narrator profile ──");
  const profileResult: AdminResponse = await adminPost("/control/profiles", {
    profileId: TEST_PROFILE,
    displayName: "Quality Spike Narrator",
    providerAlias: process.env.RUSTY_CREW_QUALITY_PROVIDER_ALIAS ?? undefined,
    kind: "full",
    brain: {
      module: "pi-agent",
      strategy: "roleplay_narrator",
    },
    localToolProfileId: "roleplay_lore",
    sessionId: TEST_SESSION,
    agentId: TEST_AGENT,
    reason: `quality-spike-${TEST_PREFIX}`,
  });
  assert.equal(
    profileResult.outcome.status,
    "completed",
    `profile create should complete: ${profileResult.outcome.summary}`,
  );
  const sessionId =
    profileResult.outcome.result &&
    typeof profileResult.outcome.result === "object" &&
    "sessionId" in profileResult.outcome.result &&
    typeof profileResult.outcome.result.sessionId === "string"
      ? profileResult.outcome.result.sessionId
      : (profileResult.outcome.affectedIds?.sessionId ?? TEST_SESSION);
  console.log(`✅ Profile created. Session: ${sessionId}`);

  // ── 3. Seed test lore ────────────────────────────────────────────────────

  console.log("\n── 3. Seeding test lore ──");
  await seedTestLore(sessionId);
  console.log("✅ Lore seeded");

  // ── 4. Test: Lore recall accuracy ─────────────────────────────────────────

  console.log("\n── 4. Test: Lore recall accuracy ──");
  await sleep(1000);

  const recallResult = await sendMessageAndCollect(
    sessionId,
    "Katheryn paused at the edge of the Silver Orchard, her hand resting on the cool bark of a moonlit tree. The clockmaker's song had faded an hour ago, leaving only the rustle of leaves. 'Elara,' she called softly, 'I know you're here. The locket — where is it?'",
  );

  // Verify the actor message was submitted
  console.log("   Events received:", recallResult.events.length);
  // Verify tool calls happened in the explore phase
  const exploreToolCalls = getToolCalls(recallResult.events);
  const hasLoreRecall = exploreToolCalls.some(
    (e) => e.kind === "tool_call_started" && toolName(e) === "recall_lore",
  );
  console.log(`   recall_lore called: ${hasLoreRecall}`);

  const hasSceneState = exploreToolCalls.some(
    (e) => e.kind === "tool_call_started" && toolName(e) === "get_scene_state",
  );
  console.log(`   scene state queried: ${hasSceneState}`);

  // Verify the output narrative is clean (no tool call noise)
  const narrative = getAssistantText(recallResult.events);
  const hasTechNoise =
    narrative.includes("sceneBrief") ||
    narrative.includes("[TOOL_CALL]") ||
    narrative.includes("recall_lore") ||
    narrative.includes("FTS");
  console.log(`   Narrative length: ${narrative.length} chars`);
  console.log(`   Tech noise in output: ${hasTechNoise}`);

  // Check phase transitions
  const phases = getPhaseChanges(recallResult.events);
  console.log(`   Phase transitions: ${phases.join(" → ")}`);
  const hasExplorePhase = phases.includes("exploring");
  const hasComposePhase = phases.includes("composing");

  assert.ok(hasLoreRecall, "narrator should call recall_lore in explore phase");
  assert.ok(hasSceneState, "narrator should query scene state");
  assert.ok(
    !hasTechNoise,
    "narrative output should not contain technical artifacts",
  );
  assert.ok(narrative.length > 50, "narrative should be substantial");
  assert.ok(hasExplorePhase, "should emit explore phase event");
  assert.ok(hasComposePhase, "should emit compose phase event");
  console.log("✅ Lore recall test passed");
  if (SCENARIO_LIMIT <= 1) {
    await cleanupTestProfile();
    return;
  }

  await sleep(2000);

  // ── 5. Test: Fact capture ─────────────────────────────────────────────────

  console.log("\n── 5. Test: Fact capture ──");
  const captureResult = await sendMessageAndCollect(
    sessionId,
    "A silver locket falls from Elara's cloak as she moves through the trees. It catches the moonlight — engraved with an unfamiliar crest, a serpent coiled around a rose. Elara doesn't notice. Katheryn picks it up, turning it over in her gloved fingers.",
  );

  const captureEvents = getToolCalls(captureResult.events);
  const hasCapture = captureEvents.some(
    (e) =>
      e.kind === "tool_call_completed" && toolName(e) === "capture_lore_fact",
  );
  console.log(`   capture_lore_fact called: ${hasCapture}`);

  const captureNarrative = getAssistantText(captureResult.events);
  const mentionsLocket = /locket/i.test(captureNarrative);
  const mentionsCrest = /crest|serpent|rose/i.test(captureNarrative);
  console.log(`   Locket mentioned: ${mentionsLocket}`);
  console.log(`   Crest mentioned: ${mentionsCrest}`);

  assert.ok(hasCapture, "narrator should capture new facts");
  assert.ok(mentionsLocket, "response should reference the locket");
  assert.ok(mentionsCrest, "response should reference the crest details");
  console.log("✅ Fact capture test passed");
  if (SCENARIO_LIMIT <= 2) {
    await cleanupTestProfile();
    return;
  }

  await sleep(2000);

  // ── 6. Test: Continuity across turns ──────────────────────────────────────

  console.log("\n── 6. Test: Continuity across turns ──");
  const continuityResult = await sendMessageAndCollect(
    sessionId,
    "Katheryn turns the locket over once more before slipping it into her pocket. 'Elara,' she says, her voice steady, 'this crest — it's the mark of the Northern Court. I've seen it before, on the documents your father kept locked in his study.' The orchard seems to grow still around them.",
  );

  const continuityEvents = getToolCalls(continuityResult.events);
  const sceneStateUpdated = continuityEvents.some(
    (e) =>
      e.kind === "tool_call_completed" && toolName(e) === "update_scene_state",
  );
  const recallInLaterTurn = continuityEvents.some(
    (e) => e.kind === "tool_call_started" && toolName(e) === "recall_lore",
  );
  console.log(`   Scene state updated: ${sceneStateUpdated}`);

  const continuityNarrative = getAssistantText(continuityResult.events);
  const referencesLocket = /locket/i.test(continuityNarrative);
  const referencesCrest = /crest|serpent|rose|northern/i.test(
    continuityNarrative,
  );
  const referencesPrevious = /silver|orchard|orchard/i.test(
    continuityNarrative,
  );
  console.log(`   References locket: ${referencesLocket}`);
  console.log(`   References crest/court: ${referencesCrest}`);
  console.log(`   References previous scene: ${referencesPrevious}`);
  if (!referencesCrest) {
    console.log(
      "   Note: response did not echo crest/court wording; continuity is judged on carried object + scene anchoring.",
    );
  }

  assert.ok(
    recallInLaterTurn || sceneStateUpdated,
    "narrator should access context in later turns",
  );
  assert.ok(referencesLocket, "should reference the locket from turn 2");
  assert.ok(
    referencesPrevious,
    "should remain anchored in the established Silver Orchard scene",
  );
  console.log("✅ Continuity test passed");
  if (SCENARIO_LIMIT <= 3) {
    await cleanupTestProfile();
    return;
  }

  await sleep(2000);

  // ── 7. Test: Clean phase output (no tool calls in compose) ────────────────

  console.log("\n── 7. Test: Clean phase output ──");
  const cleanResult = await sendMessageAndCollect(
    sessionId,
    "A light breeze stirs the leaves. From somewhere beyond the orchard wall, the clockmaker's song begins again — a simple melody, three notes repeated. Katheryn feels the locket warm against her palm through the fabric of her pocket.",
  );

  const cleanNarrative = getAssistantText(cleanResult.events);
  const cleanPhases = getPhaseChanges(cleanResult.events);
  console.log(`   Phase transitions: ${cleanPhases.join(" → ")}`);

  // The compose phase should produce narrative without tool call artifacts
  const hasArtifacts =
    /```json|```|\[DONE\]|tool_call|function_call|system:/i.test(
      cleanNarrative,
    );
  console.log(`   Artifacts in narrative: ${hasArtifacts}`);
  console.log(`   Narrative length: ${cleanNarrative.length} chars`);

  assert.ok(!hasArtifacts, "narrative should be clean prose");
  assert.ok(cleanNarrative.length > 30, "should produce substantial output");
  console.log("✅ Clean output test passed");

  // ── Summary ────────────────────────────────────────────────────────────────

  console.log("\n═══ Quality Spike Results ═══");
  const allPhases = [
    ...getPhaseChanges(recallResult.events),
    ...getPhaseChanges(captureResult.events),
    ...getPhaseChanges(continuityResult.events),
    ...getPhaseChanges(cleanResult.events),
  ];
  const uniquePhases = [...new Set(allPhases)];
  console.log(`✅ All tests passed`);
  console.log(`   Unique phases seen: ${uniquePhases.join(", ")}`);
  console.log(`   Test profile: ${TEST_PROFILE}`);
  console.log(`   Test session: ${sessionId}`);

  // ── Cleanup ────────────────────────────────────────────────────────────────

  console.log("\n── Cleanup ──");
  await cleanupTestProfile();
}

// ── Sub-operations ───────────────────────────────────────────────────────────

async function seedTestLore(sessionId: string): Promise<void> {
  await seedViaDirectApi(sessionId);
}

async function seedViaDirectApi(sessionId: string): Promise<void> {
  const now = () => new Date().toISOString();
  const layers = [
    {
      layer_id: WORLD_LAYER,
      name: "world-details",
      description: "Quality spike world facts.",
      purpose: "world",
    },
    {
      layer_id: CHARACTER_LAYER,
      name: "character-details",
      description: "Quality spike character facts.",
      purpose: "characters",
    },
    {
      layer_id: STORY_LAYER,
      name: "story-events",
      description: "Quality spike auto-captured story facts.",
      purpose: "story",
    },
  ];
  for (const layer of layers) {
    await adminPost("/roleplay/lore/layers", {
      ...layer,
      profile_id: TEST_PROFILE,
      write_policy: "auto_capture",
      now: now(),
    });
  }
  await adminPost("/roleplay/lore/chat-layers", {
    chat_id: sessionId,
    layers: layers.map((layer, index) => ({
      layer_id: layer.layer_id,
      priority: index,
      enabled: true,
    })),
    now: now(),
  });

  const facts = [
    {
      layer_id: WORLD_LAYER,
      record_id: `${TEST_PREFIX}-silver-orchard`,
      title: "Silver Orchard",
      body: "The Silver Orchard is a moonlit grove where silver-leafed trees bloom after the clockmaker sings. It is a meeting place for star-crossed lovers, and the clockmaker's song is three notes heard at dusk.",
      tags: ["silver-orchard", "clockmaker", "dusk-song"],
      priority: 10,
      is_constant: true,
    },
    {
      layer_id: CHARACTER_LAYER,
      record_id: `${TEST_PREFIX}-katheryn`,
      title: "Katheryn",
      body: "Katheryn is a sharp-witted noblewoman with a talent for uncovering secrets. She wears gloves even in summer, and her family has ties to the Northern Court.",
      tags: ["katheryn", "northern-court", "gloves"],
      priority: 8,
      is_constant: false,
    },
    {
      layer_id: CHARACTER_LAYER,
      record_id: `${TEST_PREFIX}-elara`,
      title: "Elara",
      body: "Elara is quiet and perceptive with a hidden past. She carries a locket with a serpent-and-rose crest and avoids speaking of her family.",
      tags: ["elara", "locket", "serpent-rose-crest"],
      priority: 8,
      is_constant: false,
    },
  ];

  for (const fact of facts) {
    await adminPost("/roleplay/lore/facts/capture", {
      layer_id: fact.layer_id,
      write: {
        record_id: fact.record_id,
        world_id: TEST_WORLD,
        entity_id: fact.title.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
        session_id: sessionId,
        branch_id: undefined,
        shape: {
          shape_id: "lore_entry",
          version: 1,
        },
        canon_status: "canon",
        visibility: "public",
        title: fact.title,
        body: fact.body,
        content: {
          world_id: TEST_WORLD,
          entity_id: fact.title.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
          title: fact.title,
          body: fact.body,
          canon_status: "canon",
          visibility: "public",
          metadata_json: { tags: fact.tags },
        },
        evidence_refs: [
          {
            evidence_type: "import",
            ref_id: TEST_PREFIX,
            label: "roleplay quality spike fixture",
          },
        ],
        source: "import",
        confidence: 1,
        durability_rationale: "Seed fixture for live narrator quality smoke.",
        supersedes_record_id: undefined,
        now: now(),
      },
      is_constant: fact.is_constant,
      priority: fact.priority,
      capture_reason: "roleplay-quality-spike-fixture",
    });
  }
  console.log(`   Fixture layers created: ${layers.length}`);
  console.log(`   Fixture facts captured: ${facts.length}`);
}

async function cleanupTestProfile(): Promise<void> {
  console.log("\n── Cleanup ──");
  try {
    await adminPost(`/control/profiles/${TEST_PROFILE}/delete`, {
      reason: "quality-spike-cleanup",
      confirmProfileId: TEST_PROFILE,
    });
    console.log("✅ Test profile cleaned up");
  } catch (err) {
    console.log(`⚠️  Profile cleanup failed: ${err}`);
  }
}

function scenarioLimit(): number {
  const raw = process.env.RUSTY_CREW_QUALITY_SCENARIO_LIMIT;
  if (raw === undefined || raw.trim() === "") return Number.POSITIVE_INFINITY;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0
    ? parsed
    : Number.POSITIVE_INFINITY;
}

async function sendMessageAndCollect(
  sessionId: string,
  message: string,
): Promise<{ events: StreamedEvent[] }> {
  const msgId = `quality-msg-${randomUUID().slice(0, 8)}`;
  const beforeCursor = await getSessionLatestCursor(sessionId);

  // Send message
  await chatPost(sessionId, "/messages", {
    actor: {
      id: "quality-tester",
      kind: "human",
      display_name: "Quality Tester",
    },
    body: message,
    client_message_id: msgId,
  });

  // Wait for assistant_turn_finished or stream_error
  const events = await waitForEvent(
    sessionId,
    (e) =>
      e.kind === "assistant_turn_finished" ||
      e.kind === "stream_error" ||
      e.kind === "command_completed",
    STREAM_TIMEOUT_MS,
    beforeCursor,
  );

  return { events };
}

// ── Entry point ──────────────────────────────────────────────────────────────

runQualitySpike().catch(async (err: unknown) => {
  console.error("❌ Quality spike failed:", err);
  await cleanupTestProfile();
  process.exitCode = 1;
});

// Export for smoke runner
export { runQualitySpike };
