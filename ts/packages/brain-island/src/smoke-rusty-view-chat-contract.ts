import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { apiCapabilityRegistry } from "./api-command-registry.js";
import {
  RUSTY_VIEW_CHAT_EVENT_KIND_VALUES,
  RUSTY_VIEW_CHAT_EVENT_REQUIRED_FIELDS,
  RUSTY_VIEW_CHAT_OPENAPI_PATH,
  RUSTY_VIEW_CHAT_PATHS,
  RUSTY_VIEW_MESSAGE_SLOT_REQUIRED_FIELDS,
} from "./rusty-view-chat-contract.js";

const contractPath = resolve(
  process.cwd(),
  "../../../",
  RUSTY_VIEW_CHAT_OPENAPI_PATH,
);
const contract = JSON.parse(readFileSync(contractPath, "utf8")) as OpenApiDoc;

for (const path of Object.values(RUSTY_VIEW_CHAT_PATHS)) {
  assert.ok(contract.paths[path], `missing path ${path}`);
}

assert.equal(contract.openapi, "3.1.0");
assert.equal(
  contract.paths[RUSTY_VIEW_CHAT_PATHS.stream]?.get?.responses["200"]
    ?.content?.["text/event-stream"]?.schema?.type,
  "string",
);

const chatEvent = schema("ChatEvent");
assert.deepEqual(chatEvent.required, [
  ...RUSTY_VIEW_CHAT_EVENT_REQUIRED_FIELDS,
]);

assert.deepEqual(schema("ChatEventKind").enum, [
  ...RUSTY_VIEW_CHAT_EVENT_KIND_VALUES,
]);

assert.ok(schema("ChatSessionOpenResult").properties?.message_slots);
assert.ok(schema("SendChatMessageResult").properties?.slot_id);
assert.ok(schema("SendChatMessageResult").properties?.primary_variant_id);
assert.deepEqual(schema("MessageSlotRecord").required, [
  ...RUSTY_VIEW_MESSAGE_SLOT_REQUIRED_FIELDS,
]);
assert.ok(schema("MessageVariantRecord").properties?.message);
assert.ok(schema("ActiveVariantExpectation").oneOf?.length);
assert.ok(schema("DurableMessageRecord").properties?.branch_id);
assert.ok(schema("DurableMessageRecord").properties?.parent_message_id);
assert.ok(schema("ConversationTreeProjection").properties?.branches);
assert.ok(schema("ConversationBranchRecord").properties?.head_message_id);
assert.ok(schema("ConversationSnapshotRecord").properties?.cursor);
assert.ok(schema("ConversationJumpResult").properties?.target);
assert.ok(schema("TranscriptSearchResult").properties?.highlights);
assert.ok(schema("TranscriptSearchResult").properties?.jump);
assert.ok(schema("TranscriptSearchResultPage").properties?.source);
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

const capabilityPaths = new Set(
  apiCapabilityRegistry().capabilities.map(
    (capability) => capability.path_template,
  ),
);
for (const path of Object.values(RUSTY_VIEW_CHAT_PATHS)) {
  assert.ok(capabilityPaths.has(path), `capability registry missing ${path}`);
}

console.log(
  JSON.stringify(
    {
      title: contract.info.title,
      paths: Object.values(RUSTY_VIEW_CHAT_PATHS).length,
      eventKinds: RUSTY_VIEW_CHAT_EVENT_KIND_VALUES.length,
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
