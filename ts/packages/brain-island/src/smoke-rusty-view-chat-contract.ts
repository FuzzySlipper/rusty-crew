import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const contractPath = resolve(
  process.cwd(),
  "../../../docs/rusty-view-chat-api-v0.openapi.json",
);
const contract = JSON.parse(readFileSync(contractPath, "utf8")) as OpenApiDoc;

const requiredPaths = [
  "/v1/chat/sessions",
  "/v1/chat/sessions/{session_id}",
  "/v1/chat/sessions/{session_id}/events",
  "/v1/chat/sessions/{session_id}/stream",
  "/v1/chat/sessions/{session_id}/messages",
  "/v1/chat/sessions/{session_id}/slots",
  "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants",
  "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants/{variant_id}",
  "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants/reorder",
  "/v1/chat/sessions/{session_id}/slots/{slot_id}/active-variant",
  "/v1/chat/sessions/{session_id}/tree",
  "/v1/chat/sessions/{session_id}/jump",
  "/v1/chat/sessions/{session_id}/branches",
  "/v1/chat/sessions/{session_id}/branches/active",
  "/v1/chat/sessions/{session_id}/branches/{branch_id}/head",
  "/v1/chat/sessions/{session_id}/snapshots",
  "/v1/chat/sessions/{session_id}/attachments",
  "/v1/chat/sessions/{session_id}/attachments/{attachment_id}",
  "/v1/chat/sessions/{session_id}/data-bank/scopes",
  "/v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}",
  "/v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}/attachments",
  "/v1/chat/commands",
  "/v1/chat/commands/{command_name}/autocomplete",
  "/v1/chat/sessions/{session_id}/commands",
];

for (const path of requiredPaths) {
  assert.ok(contract.paths[path], `missing path ${path}`);
}

assert.equal(contract.openapi, "3.1.0");
assert.equal(
  contract.paths["/v1/chat/sessions/{session_id}/stream"]?.get?.responses["200"]
    ?.content?.["text/event-stream"]?.schema?.type,
  "string",
);

const chatEvent = schema("ChatEvent");
assert.deepEqual(chatEvent.required, [
  "event_id",
  "session_id",
  "sequence_id",
  "created_at",
  "kind",
  "payload",
]);

const eventKinds = schema("ChatEventKind").enum ?? [];
for (const kind of [
  "message_created",
  "assistant_turn_started",
  "assistant_text_delta",
  "assistant_message_completed",
  "assistant_turn_finished",
  "tool_call_started",
  "tool_call_completed",
  "tool_call_failed",
  "command_started",
  "command_completed",
  "command_failed",
  "message_slot_created",
  "message_variant_created",
  "message_variant_deleted",
  "message_variants_reordered",
  "message_active_variant_selected",
  "conversation_branch_created",
  "conversation_active_branch_selected",
  "conversation_branch_head_updated",
  "conversation_snapshot_created",
  "attachment_uploaded",
  "attachment_linked",
  "attachment_removed",
  "attachment_updated",
  "data_bank_scope_created",
  "data_bank_scope_removed",
  "unknown",
]) {
  assert.ok(eventKinds.includes(kind), `missing event kind ${kind}`);
}

assert.ok(schema("ChatSessionOpenResult").properties?.message_slots);
assert.ok(schema("SendChatMessageResult").properties?.slot_id);
assert.ok(schema("SendChatMessageResult").properties?.primary_variant_id);
assert.deepEqual(schema("MessageSlotRecord").required, [
  "slot_id",
  "session_id",
  "primary_variant_id",
  "metadata_json",
  "created_at",
  "updated_at",
  "version",
  "primary",
  "alternates",
]);
assert.ok(schema("MessageVariantRecord").properties?.message);
assert.ok(schema("ActiveVariantExpectation").oneOf?.length);
assert.ok(schema("DurableMessageRecord").properties?.branch_id);
assert.ok(schema("DurableMessageRecord").properties?.parent_message_id);
assert.ok(schema("ConversationTreeProjection").properties?.branches);
assert.ok(schema("ConversationBranchRecord").properties?.head_message_id);
assert.ok(schema("ConversationSnapshotRecord").properties?.cursor);
assert.ok(schema("ConversationJumpResult").properties?.target);
assert.ok(schema("AttachmentRecord").properties?.links);
assert.ok(schema("AttachmentMutationResult").properties?.attachment);
assert.ok(schema("DataBankScopeRecord").properties?.scope_id);
assert.ok(schema("DataBankScopeMutationResult").properties?.scope);
assert.ok(schema("ActiveBranchExpectation").oneOf?.length);
assert.ok(schema("BranchHeadExpectation").oneOf?.length);

const commandDescriptor = schema("ChatCommandDescriptor");
assert.ok(commandDescriptor.required?.includes("positional_args"));
assert.ok(commandDescriptor.required?.includes("named_args"));
assert.ok(commandDescriptor.required?.includes("surfaces"));
assert.ok(commandDescriptor.required?.includes("source"));
assert.ok(commandDescriptor.required?.includes("read_only"));
assert.ok(commandDescriptor.required?.includes("mutating"));
assert.ok(commandDescriptor.required?.includes("scope"));
assert.ok(commandDescriptor.properties?.backing_control_command);
assert.deepEqual(schema("ChatCommandArgumentDescriptor").required, [
  "name",
  "type",
  "required",
]);
assert.ok(schema("ChatCommandAutocompleteResult").properties?.items);
assert.ok(schema("ChatCommandSurface").enum?.includes("chat-input"));
assert.ok(schema("ChatCommandSource").enum?.includes("frontend-local"));

console.log(
  JSON.stringify(
    {
      title: contract.info.title,
      paths: requiredPaths.length,
      eventKinds: eventKinds.length,
    },
    null,
    2,
  ),
);

function schema(name: string): JsonSchema {
  const value = contract.components.schemas[name];
  assert.ok(value, `missing schema ${name}`);
  return value;
}

interface OpenApiDoc {
  openapi: string;
  info: { title: string; version: string };
  paths: Record<
    string,
    {
      get?: Operation;
      post?: Operation;
      delete?: Operation;
    }
  >;
  components: {
    schemas: Record<string, JsonSchema>;
  };
}

interface Operation {
  responses: Record<
    string,
    {
      content?: Record<string, { schema?: JsonSchema }>;
    }
  >;
}

interface JsonSchema {
  type?: string;
  enum?: string[];
  required?: string[];
  oneOf?: JsonSchema[];
  properties?: Record<string, JsonSchema>;
}
