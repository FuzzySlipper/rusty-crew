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
  "den-successor-gateway",
  "conversation-readback.redacted.json",
);

const cassette = readJson(cassettePath);
assertNoSecretMaterial(cassette, "$");
assertObject(cassette, "$");
assert.equal(cassette.schemaVersion, 1);
assert.equal(cassette.source, "den-successor-gateway");
assertString(cassette.scenario, "$.scenario");
assertIsoTimestamp(cassette.capturedAt, "$.capturedAt");
assertString(cassette.redaction, "$.redaction");
assertArray(cassette.interactions, "$.interactions");

const interactions = new Map<string, unknown>();
for (const interaction of cassette.interactions) {
  assertObject(interaction, "$.interactions[]");
  assertString(interaction.name, "$.interactions[].name");
  interactions.set(interaction.name, interaction);
  assertRequest(
    interaction.request,
    `$.interactions.${interaction.name}.request`,
  );
  assertObject(
    interaction.response,
    `$.interactions.${interaction.name}.response`,
  );
  assertNumber(
    interaction.response.status,
    `$.interactions.${interaction.name}.response.status`,
  );
  assert.equal(interaction.response.status, 200);
}

assertHealthBody(responseBody("health"));
assertRuntimeInstance(responseBody("runtime-instance"));
assertDeliveryIntents(responseBody("pending-delivery-intents"));
assertConversationChannels(responseBody("conversation-channels"));
assertConversationMemberships(responseBody("conversation-memberships"));
assertConversationMessages(responseBody("conversation-messages"));

console.log(
  JSON.stringify(
    {
      cassette: "den-successor-gateway/conversation-readback",
      interactions: interactions.size,
      lane: "offline",
    },
    null,
    2,
  ),
);

function responseBody(name: string): unknown {
  const interaction = interactions.get(name);
  assertObject(interaction, `interaction ${name}`);
  assertObject(interaction.response, `interaction ${name}.response`);
  return interaction.response.body;
}

function assertHealthBody(value: unknown): void {
  assertObject(value, "$.health.body");
  assertString(value.status, "$.health.body.status");
  assertOptionalString(value.service_name, "$.health.body.service_name");
  assertOptionalString(value.version, "$.health.body.version");
  assertOptionalString(value.commit, "$.health.body.commit");
  assertOptionalString(value.built_at, "$.health.body.built_at");
}

function assertRuntimeInstance(value: unknown): void {
  assertObject(value, "$.runtimeInstance");
  assertString(value.instance_id, "$.runtimeInstance.instance_id");
  assertString(value.profile_identity, "$.runtimeInstance.profile_identity");
  assertString(value.host, "$.runtimeInstance.host");
  assertOptionalNumber(value.pid, "$.runtimeInstance.pid");
  assertString(value.state, "$.runtimeInstance.state");
  assertIsoTimestamp(value.started_at, "$.runtimeInstance.started_at");
  assertOptionalIsoTimestamp(
    value.last_heartbeat_at,
    "$.runtimeInstance.last_heartbeat_at",
  );
  assertOptionalIsoTimestamp(value.stopped_at, "$.runtimeInstance.stopped_at");
  assertOptionalString(
    value.degraded_reason,
    "$.runtimeInstance.degraded_reason",
  );
}

function assertDeliveryIntents(value: unknown): void {
  assertArray(value, "$.deliveryIntents");
  assert.ok(
    value.length > 0,
    "delivery intent cassette should include an item",
  );
  for (const item of value) {
    assertObject(item, "$.deliveryIntents[]");
    assertNumber(item.id, "$.deliveryIntents[].id");
    assertAgentIdentity(
      item.target_identity,
      "$.deliveryIntents[].target_identity",
    );
    assertString(item.state, "$.deliveryIntents[].state");
    assertString(item.idempotency_key, "$.deliveryIntents[].idempotency_key");
    assertIsoTimestamp(item.created_at, "$.deliveryIntents[].created_at");
    assertIsoTimestamp(item.expires_at, "$.deliveryIntents[].expires_at");
    assertOptionalString(item.source_ref, "$.deliveryIntents[].source_ref");
    assertOptionalNumber(
      item.channel_message_id,
      "$.deliveryIntents[].channel_message_id",
    );
  }
}

function assertConversationChannels(value: unknown): void {
  assertArray(value, "$.conversationChannels");
  assert.ok(value.length > 0, "channel cassette should include an item");
  for (const item of value) {
    assertObject(item, "$.conversationChannels[]");
    assertNumber(item.id, "$.conversationChannels[].id");
    assertString(item.slug, "$.conversationChannels[].slug");
    assertString(item.display_name, "$.conversationChannels[].display_name");
    assertString(item.kind, "$.conversationChannels[].kind");
    assertOptionalString(
      item.project_id,
      "$.conversationChannels[].project_id",
    );
    assertOptionalString(item.space_id, "$.conversationChannels[].space_id");
    assertString(item.created_by, "$.conversationChannels[].created_by");
    assertString(item.visibility, "$.conversationChannels[].visibility");
    assertOptionalObject(item.settings, "$.conversationChannels[].settings");
    assertIsoTimestamp(item.created_at, "$.conversationChannels[].created_at");
    assertIsoTimestamp(item.updated_at, "$.conversationChannels[].updated_at");
    assertOptionalIsoTimestamp(
      item.archived_at,
      "$.conversationChannels[].archived_at",
    );
  }
}

function assertConversationMemberships(value: unknown): void {
  assertArray(value, "$.conversationMemberships");
  assert.ok(value.length > 0, "membership cassette should include an item");
  for (const item of value) {
    assertObject(item, "$.conversationMemberships[]");
    assertNumber(item.id, "$.conversationMemberships[].id");
    assertNumber(item.channel_id, "$.conversationMemberships[].channel_id");
    assertString(item.member_type, "$.conversationMemberships[].member_type");
    assertString(
      item.member_identity,
      "$.conversationMemberships[].member_identity",
    );
    assertOptionalString(
      item.profile_identity,
      "$.conversationMemberships[].profile_identity",
    );
    assertString(
      item.membership_status,
      "$.conversationMemberships[].membership_status",
    );
    assertString(item.wake_policy, "$.conversationMemberships[].wake_policy");
    assertBoolean(item.can_send, "$.conversationMemberships[].can_send");
    assertBoolean(item.can_react, "$.conversationMemberships[].can_react");
    assertBoolean(item.can_invite, "$.conversationMemberships[].can_invite");
    assertString(
      item.membership_purpose,
      "$.conversationMemberships[].membership_purpose",
    );
    assertOptionalObject(item.settings, "$.conversationMemberships[].settings");
    assertIsoTimestamp(
      item.created_at,
      "$.conversationMemberships[].created_at",
    );
    assertIsoTimestamp(
      item.updated_at,
      "$.conversationMemberships[].updated_at",
    );
    assertOptionalIsoTimestamp(
      item.left_at,
      "$.conversationMemberships[].left_at",
    );
  }
}

function assertConversationMessages(value: unknown): void {
  assertArray(value, "$.conversationMessages");
  assert.ok(value.length > 0, "message cassette should include an item");
  for (const item of value) {
    assertObject(item, "$.conversationMessages[]");
    assertNumber(item.id, "$.conversationMessages[].id");
    assertNumber(item.channel_id, "$.conversationMessages[].channel_id");
    assertString(item.sender_type, "$.conversationMessages[].sender_type");
    assertString(
      item.sender_identity,
      "$.conversationMessages[].sender_identity",
    );
    assertString(item.body, "$.conversationMessages[].body");
    assertString(item.message_kind, "$.conversationMessages[].message_kind");
    assertString(item.source_kind, "$.conversationMessages[].source_kind");
    assertOptionalString(item.source_id, "$.conversationMessages[].source_id");
    assertOptionalString(
      item.source_project_id,
      "$.conversationMessages[].source_project_id",
    );
    assertOptionalString(
      item.target_project_id,
      "$.conversationMessages[].target_project_id",
    );
    assertOptionalNumber(
      item.target_task_id,
      "$.conversationMessages[].target_task_id",
    );
    assertOptionalString(
      item.assignment_id,
      "$.conversationMessages[].assignment_id",
    );
    assertOptionalString(
      item.worker_run_id,
      "$.conversationMessages[].worker_run_id",
    );
    assertOptionalString(
      item.worker_role,
      "$.conversationMessages[].worker_role",
    );
    assertOptionalString(
      item.profile_identity,
      "$.conversationMessages[].profile_identity",
    );
    assertOptionalString(
      item.agent_instance_id,
      "$.conversationMessages[].agent_instance_id",
    );
    assertOptionalString(
      item.session_id,
      "$.conversationMessages[].session_id",
    );
    assertOptionalString(item.summary, "$.conversationMessages[].summary");
    assertOptionalString(item.deep_link, "$.conversationMessages[].deep_link");
    assertOptionalNumber(
      item.thread_root_message_id,
      "$.conversationMessages[].thread_root_message_id",
    );
    assertOptionalNumber(
      item.reply_to_message_id,
      "$.conversationMessages[].reply_to_message_id",
    );
    assertOptionalObject(item.metadata, "$.conversationMessages[].metadata");
    assertOptionalString(
      item.dedupe_key,
      "$.conversationMessages[].dedupe_key",
    );
    assertIsoTimestamp(item.created_at, "$.conversationMessages[].created_at");
  }
}

function assertRequest(value: unknown, path: string): void {
  assertObject(value, path);
  assertString(value.method, `${path}.method`);
  assert.match(value.method, /^(GET|POST|PUT|PATCH|DELETE)$/);
  assertString(value.path, `${path}.path`);
  assert.ok(value.path.startsWith("/"), `${path}.path must be relative`);
}

function assertAgentIdentity(value: unknown, path: string): void {
  assertObject(value, path);
  assertString(value.profile, `${path}.profile`);
  assertString(value.instance_id, `${path}.instance_id`);
  assertOptionalString(value.session_key, `${path}.session_key`);
}

function assertNoSecretMaterial(value: unknown, path: string): void {
  if (typeof value === "string") {
    assert.equal(
      /\bbearer\s+[a-z0-9._~+/=-]{8,}/i.test(value),
      false,
      `${path} appears to contain secret material`,
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
  if (value !== null && typeof value === "object") {
    for (const [key, nested] of Object.entries(value)) {
      assert.equal(
        /authorization|api[_-]?key|secret|token|cookie|sessionid/i.test(key),
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

function assertObject(
  value: unknown,
  path: string,
): asserts value is Record<string, unknown> {
  assert.equal(typeof value, "object", `${path} must be an object`);
  assert.notEqual(value, null, `${path} must not be null`);
  assert.equal(Array.isArray(value), false, `${path} must not be an array`);
}

function assertOptionalObject(value: unknown, path: string): void {
  if (value !== undefined) assertObject(value, path);
}

function assertArray(value: unknown, path: string): asserts value is unknown[] {
  assert.ok(Array.isArray(value), `${path} must be an array`);
}

function assertString(value: unknown, path: string): asserts value is string {
  assert.equal(typeof value, "string", `${path} must be a string`);
}

function assertOptionalString(value: unknown, path: string): void {
  if (value !== undefined) assertString(value, path);
}

function assertNumber(value: unknown, path: string): asserts value is number {
  assert.equal(typeof value, "number", `${path} must be a number`);
}

function assertOptionalNumber(value: unknown, path: string): void {
  if (value !== undefined) assertNumber(value, path);
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

function assertOptionalIsoTimestamp(value: unknown, path: string): void {
  if (value !== undefined) assertIsoTimestamp(value, path);
}
