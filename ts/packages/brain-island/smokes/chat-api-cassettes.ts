import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
  "..",
);
const cassettePath = join(
  repoRoot,
  "fixtures",
  "external-cassettes",
  "rusty-view-chat-api",
  "roleplay-turn-readback.redacted.json",
);

const cassette = readJson(cassettePath);
assertNoSecretMaterial(cassette, "$");
assertObject(cassette, "$");
assert.equal(cassette.schemaVersion, 1);
assert.equal(cassette.source, "rusty-view-chat-api");
assert.equal(cassette.scenario, "roleplay-turn-readback");
assertIsoTimestamp(cassette.capturedAt, "$.capturedAt");
assertString(cassette.redaction, "$.redaction");
assertArray(cassette.interactions, "$.interactions");

const interactions = new Map<string, Record<string, unknown>>();
for (const interaction of cassette.interactions) {
  assertObject(interaction, "$.interactions[]");
  assertString(interaction.name, "$.interactions[].name");
  assertRequest(interaction.request, `$.interactions.${interaction.name}`);
  assertObject(
    interaction.response,
    `$.interactions.${interaction.name}.response`,
  );
  assertNumber(
    interaction.response.status,
    `$.interactions.${interaction.name}.response.status`,
  );
  assert.equal(interaction.response.status, 200);
  interactions.set(interaction.name, interaction);
}

assertSessionsPage(envelopeData("sessions-page"));
assertSessionOpen(envelopeData("session-open"));
assertEventsPage(envelopeData("events-page"));
assertSessionContext(envelopeData("session-context"));
assertToolCallDebugDetail(envelopeData("tool-call-debug-detail"));

console.log(
  JSON.stringify(
    {
      cassette: "rusty-view-chat-api/roleplay-turn-readback",
      interactions: interactions.size,
      lane: "offline",
    },
    null,
    2,
  ),
);

function envelopeData(name: string): unknown {
  const interaction = interactions.get(name);
  assertObject(interaction, `interaction ${name}`);
  assertObject(interaction.response, `interaction ${name}.response`);
  assertObject(interaction.response.body, `interaction ${name}.response.body`);
  assert.equal(interaction.response.body.ok, true);
  assertObject(interaction.response.body.meta, `${name}.meta`);
  assertNumber(
    interaction.response.body.meta.schema_version,
    `${name}.meta.schema_version`,
  );
  assert.equal(interaction.response.body.meta.schema_version, 1);
  return interaction.response.body.data;
}

function assertSessionsPage(value: unknown): void {
  assertObject(value, "$.sessionsPage");
  assertArray(value.items, "$.sessionsPage.items");
  assert.ok(value.items.length > 0, "sessions page should include a session");
  assertNumber(value.total, "$.sessionsPage.total");
  assertNumber(value.limit, "$.sessionsPage.limit");
  assertNumber(value.offset, "$.sessionsPage.offset");
  assertChatSessionSummary(value.items[0], "$.sessionsPage.items[0]");
}

function assertSessionOpen(value: unknown): void {
  assertObject(value, "$.sessionOpen");
  assertChatSessionSummary(value.session, "$.sessionOpen.session");
  assertArray(value.events, "$.sessionOpen.events");
  assertArray(value.message_slots, "$.sessionOpen.message_slots");
  assertString(value.latest_cursor, "$.sessionOpen.latest_cursor");
  assertBoolean(value.has_more_before, "$.sessionOpen.has_more_before");
  assertEventKinds(value.events, [
    "session_snapshot",
    "assistant_turn_finished",
    "phase_change",
    "assistant_message_completed",
    "unknown",
  ]);
  const completed = findEvent(value.events, "assistant_message_completed");
  assert.equal(completed.payload.status, "completed");
  assertString(completed.payload.summary, "$.sessionOpen.completed.summary");

  const slot = value.message_slots[0];
  assertObject(slot, "$.sessionOpen.message_slots[0]");
  assertString(slot.slot_id, "$.sessionOpen.message_slots[0].slot_id");
  assertString(
    slot.primary_variant_id,
    "$.sessionOpen.message_slots[0].primary_variant_id",
  );
  assertObject(slot.primary, "$.sessionOpen.message_slots[0].primary");
  assertObject(
    slot.primary.message,
    "$.sessionOpen.message_slots[0].primary.message",
  );
  assertArray(
    slot.primary.message.blocks,
    "$.sessionOpen.message_slots[0].primary.message.blocks",
  );
}

function assertEventsPage(value: unknown): void {
  assertObject(value, "$.eventsPage");
  assertArray(value.items, "$.eventsPage.items");
  assertString(value.latest_cursor, "$.eventsPage.latest_cursor");
  assertBoolean(value.has_more, "$.eventsPage.has_more");
  assertEventKinds(value.items, [
    "message_created",
    "phase_change",
    "tool_call_started",
    "tool_call_completed",
    "provider_status",
    "assistant_turn_started",
    "assistant_reasoning_delta",
  ]);
  const phases = value.items
    .filter((event) => isRecord(event) && event.kind === "phase_change")
    .map((event) => recordBody(event.payload).phase);
  assert.deepEqual(phases, ["exploring", "composing"]);

  const toolStarted = findEvent(value.items, "tool_call_started");
  assertString(
    toolStarted.payload.tool_call_id,
    "$.eventsPage.tool.started.tool_call_id",
  );
  assertString(
    toolStarted.payload.tool_name,
    "$.eventsPage.tool.started.tool_name",
  );
  assert.equal(toolStarted.payload.tool_name, "get_scene_state");

  const toolCompleted = findEvent(value.items, "tool_call_completed");
  assert.equal(toolCompleted.payload.is_error, false);

  const reasoning = findEvent(value.items, "assistant_reasoning_delta");
  assert.equal(reasoning.payload.visibility, "reasoning");
  assertString(reasoning.payload.format, "$.eventsPage.reasoning.format");

  const provider = findEvent(value.items, "provider_status");
  assert.equal(provider.payload.level, "info");
  assertString(
    provider.payload.metadata_json,
    "$.eventsPage.provider.metadata_json",
  );
  const providerMetadata = JSON.parse(
    provider.payload.metadata_json,
  ) as unknown;
  assertObject(providerMetadata, "$.eventsPage.provider.metadata");
  assertString(
    providerMetadata.provider_request_debug_detail_id,
    "$.eventsPage.provider.metadata.provider_request_debug_detail_id",
  );
  assertString(
    providerMetadata.provider_request_debug_url,
    "$.eventsPage.provider.metadata.provider_request_debug_url",
  );
  assertNumber(
    providerMetadata.request_json_chars,
    "$.eventsPage.provider.metadata.request_json_chars",
  );
}

function assertSessionContext(value: unknown): void {
  assertObject(value, "$.sessionContext");
  assertString(value.session_id, "$.sessionContext.session_id");
  assertObject(value.provider, "$.sessionContext.provider");
  assert.equal(value.provider.protocol, "chat_completions");
  assert.equal(value.provider.status, "active");
  assertNumber(
    value.provider.temperature,
    "$.sessionContext.provider.temperature",
  );
  assertObject(value.brain, "$.sessionContext.brain");
  assert.equal(value.brain.strategy, "roleplay_narrator");
  assertObject(value.tools, "$.sessionContext.tools");
  assert.equal(value.tools.local_tool_profile_id, "roleplay_lore");
  assertNumber(value.tools.tool_count, "$.sessionContext.tools.tool_count");
  assertArray(
    value.tools.requested_toolsets,
    "$.sessionContext.tools.requested_toolsets",
  );
  assert.ok(
    value.tools.requested_toolsets.includes("roleplay_scene_state"),
    "session context should expose roleplay scene-state toolset",
  );
  assertObject(value.context, "$.sessionContext.context");
  assertNumber(
    value.context.estimated_prompt_tokens,
    "$.sessionContext.context.estimated_prompt_tokens",
  );
  assert.equal(value.degraded, false);
  assertArray(value.diagnostics, "$.sessionContext.diagnostics");
}

function assertToolCallDebugDetail(value: unknown): void {
  assertObject(value, "$.toolCallDebugDetail");
  assertString(value.debug_detail_id, "$.toolCallDebugDetail.debug_detail_id");
  assertString(value.tool_call_id, "$.toolCallDebugDetail.tool_call_id");
  assertString(value.session_id, "$.toolCallDebugDetail.session_id");
  assertString(value.wake_id, "$.toolCallDebugDetail.wake_id");
  assert.equal(value.tool_name, "get_scene_state");
  assert.equal(value.status, "completed");
  assertObject(value.arguments, "$.toolCallDebugDetail.arguments");
  assertObject(value.final_result, "$.toolCallDebugDetail.final_result");
  assertObject(
    value.final_result.value,
    "$.toolCallDebugDetail.final_result.value",
  );
  assertArray(
    value.final_result.value.content,
    "$.toolCallDebugDetail.final_result.value.content",
  );
  assertObject(
    value.final_result.value.details,
    "$.toolCallDebugDetail.final_result.value.details",
  );
  assert.equal(value.final_result.value.details.operation, "get_scene_state");
  assertObject(value.source_metadata, "$.toolCallDebugDetail.source_metadata");
  assertObject(value.limits, "$.toolCallDebugDetail.limits");
  assertNumber(
    value.limits.retentionMs,
    "$.toolCallDebugDetail.limits.retentionMs",
  );
  assertIsoTimestamp(value.started_at, "$.toolCallDebugDetail.started_at");
  assertIsoTimestamp(value.expires_at, "$.toolCallDebugDetail.expires_at");
}

function assertChatSessionSummary(value: unknown, path: string): void {
  assertObject(value, path);
  assertString(value.session_id, `${path}.session_id`);
  assertString(value.agent_id, `${path}.agent_id`);
  assertString(value.profile_id, `${path}.profile_id`);
  assertString(value.kind, `${path}.kind`);
  assertString(value.status, `${path}.status`);
  assertString(value.latest_cursor, `${path}.latest_cursor`);
  assertIsoTimestamp(value.created_at, `${path}.created_at`);
  assertIsoTimestamp(value.updated_at, `${path}.updated_at`);
  assertNumber(value.message_count, `${path}.message_count`);
  assertNumber(value.tool_event_count, `${path}.tool_event_count`);
}

function assertEventKinds(events: unknown[], kinds: readonly string[]): void {
  const present = new Set(
    events.map((event) => {
      assertObject(event, "$.event");
      assertChatEvent(event, "$.event");
      return event.kind;
    }),
  );
  for (const kind of kinds) {
    assert.ok(present.has(kind), `expected event kind ${kind}`);
  }
}

function findEvent(
  events: unknown[],
  kind: string,
): { payload: Record<string, unknown> } {
  const event = events.find(
    (candidate) => isRecord(candidate) && candidate.kind === kind,
  );
  assertObject(event, `event ${kind}`);
  assertChatEvent(event, `event ${kind}`);
  return { payload: event.payload };
}

function assertChatEvent(
  value: unknown,
  path: string,
): asserts value is {
  event_id: string;
  session_id: string;
  sequence_id: number;
  created_at: string;
  kind: string;
  payload: Record<string, unknown>;
} {
  assertObject(value, path);
  assertString(value.event_id, `${path}.event_id`);
  assertString(value.session_id, `${path}.session_id`);
  assertNumber(value.sequence_id, `${path}.sequence_id`);
  assertIsoTimestamp(value.created_at, `${path}.created_at`);
  assertString(value.kind, `${path}.kind`);
  assertObject(value.payload, `${path}.payload`);
}

function assertRequest(value: unknown, path: string): void {
  assertObject(value, `${path}.request`);
  assertString(value.method, `${path}.request.method`);
  assert.match(value.method, /^(GET|POST|PUT|PATCH|DELETE)$/);
  assertString(value.path, `${path}.request.path`);
  assert.ok(
    value.path.startsWith("/"),
    `${path}.request.path must be relative`,
  );
}

function assertNoSecretMaterial(value: unknown, path: string): void {
  if (typeof value === "string") {
    assert.equal(
      /\bbearer\s+[a-z0-9._~+/=-]{8,}/i.test(value),
      false,
      `${path} appears to contain bearer material`,
    );
    assert.equal(
      /sk-[a-z0-9_-]{12,}/i.test(value),
      false,
      `${path} appears to contain an API key`,
    );
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) =>
      assertNoSecretMaterial(item, `${path}[${index}]`),
    );
    return;
  }
  if (isRecord(value)) {
    for (const [key, nested] of Object.entries(value)) {
      assert.equal(
        /^(authorization|api[_-]?key|access[_-]?token|refresh[_-]?token|cookie)$/i.test(
          key,
        ),
        false,
        `${path}.${key} is a secret-like key`,
      );
      assertNoSecretMaterial(nested, `${path}.${key}`);
    }
  }
}

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, "utf8")) as unknown;
}

function recordBody(value: unknown): Record<string, unknown> {
  assertObject(value, "$.record");
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function assertObject(
  value: unknown,
  path: string,
): asserts value is Record<string, unknown> {
  assert.equal(typeof value, "object", `${path} must be an object`);
  assert.notEqual(value, null, `${path} must not be null`);
  assert.equal(Array.isArray(value), false, `${path} must not be an array`);
}

function assertArray(value: unknown, path: string): asserts value is unknown[] {
  assert.ok(Array.isArray(value), `${path} must be an array`);
}

function assertString(value: unknown, path: string): asserts value is string {
  assert.equal(typeof value, "string", `${path} must be a string`);
}

function assertNumber(value: unknown, path: string): asserts value is number {
  assert.equal(typeof value, "number", `${path} must be a number`);
}

function assertBoolean(value: unknown, path: string): asserts value is boolean {
  assert.equal(typeof value, "boolean", `${path} must be a boolean`);
}

function assertIsoTimestamp(value: unknown, path: string): void {
  assertString(value, path);
  assert.match(
    value,
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/,
    `${path} must be ISO-like`,
  );
}
