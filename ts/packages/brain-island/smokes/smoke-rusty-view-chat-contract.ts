import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { API_CAPABILITY_OPENAPI_PATH } from "../src/api-capability-openapi.js";
import { apiCapabilityRegistry } from "../src/api-command-registry.js";
import {
  CHAT_COMPLETIONS_DIALECT_VALUES,
  CHAT_COMPLETIONS_REASONING_HISTORY_VALUES,
  CHAT_COMPLETIONS_THINKING_MODE_VALUES,
} from "../src/model-provider-admin-contract.js";
import {
  RUSTY_VIEW_CHAT_EVENT_KIND_VALUES,
  RUSTY_VIEW_CHAT_EVENT_REQUIRED_FIELDS,
  RUSTY_VIEW_CHAT_OPENAPI_PATH,
  RUSTY_VIEW_CHAT_PATHS,
  RUSTY_VIEW_MESSAGE_SLOT_REQUIRED_FIELDS,
} from "../src/rusty-view-chat-contract.js";

const contractPath = resolve(
  process.cwd(),
  "../../../",
  RUSTY_VIEW_CHAT_OPENAPI_PATH,
);
const contract = JSON.parse(readFileSync(contractPath, "utf8")) as OpenApiDoc;
const capabilityContract = JSON.parse(
  readFileSync(
    resolve(process.cwd(), "../../../", API_CAPABILITY_OPENAPI_PATH),
    "utf8",
  ),
) as OpenApiDoc;

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

assert.deepEqual(schema("MemorySurfaceOwner").enum, [
  "crew",
  "den",
  "filesystem",
]);
assert.deepEqual(schema("MemorySurfaceAvailability").enum, [
  "available",
  "degraded",
  "unavailable",
  "profile_scoped",
]);
assert.deepEqual(schema("MemorySurfaceCatalogProjection").required, [
  "generatedAt",
  "items",
]);
assert.deepEqual(schema("MemorySurfaceCatalogItem").required, [
  "surfaceId",
  "displayName",
  "owner",
  "storageHome",
  "promptPolicy",
  "modelFacingToolNames",
  "backendProvenance",
  "availability",
  "availabilityReasonCode",
  "notes",
]);
assert.equal(
  schema("MemorySurfaceCatalogItem").properties?.lastSafeError?.type,
  "string",
);

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
assert.deepEqual(schema("ChatSessionOpenResult").required, [
  "session",
  "events",
  "latest_cursor",
  "has_more_before",
]);
assert.deepEqual(schema("ChatEventPage").required, [
  "items",
  "latest_cursor",
  "has_more",
]);
assert.equal(schema("MessageSlotMutationResult").oneOf?.length, 2);
assert.deepEqual(schema("MessageSlotMutationResult").oneOf?.[0]?.required, [
  "status",
  "slot",
  "latest_cursor",
]);
assert.deepEqual(schema("MessageSlotMutationResult").oneOf?.[1]?.required, [
  "status",
  "branch",
  "conflict",
]);
assert.ok(
  schema("MessageSlotMutationResult").oneOf?.[1]?.properties?.latest_cursor,
);
assert.deepEqual(schema("ToolCallDebugDetail").required, [
  "debug_detail_id",
  "tool_call_id",
  "session_id",
  "wake_id",
  "tool_name",
  "status",
  "arguments",
  "partial_updates",
  "source_metadata",
  "started_at",
  "updated_at",
  "expires_at",
  "limits",
]);
assert.deepEqual(schema("ProviderRequestDebugDetail").required, [
  "debug_detail_id",
  "session_id",
  "wake_id",
  "provider",
  "request",
  "request_sha256",
  "request_json_chars",
  "recorded_at",
  "expires_at",
  "limits",
]);

const contextProvider = schema("SessionContextUsageResult").properties
  ?.provider;
assert.ok(contextProvider?.properties, "missing context provider schema");
assert.deepEqual(contextProvider.required, ["alias", "status"]);
assert.deepEqual(contextProvider.properties.chat_completions_dialect?.enum, [
  ...CHAT_COMPLETIONS_DIALECT_VALUES,
]);
assert.deepEqual(contextProvider.properties.thinking_mode?.enum, [
  ...CHAT_COMPLETIONS_THINKING_MODE_VALUES,
]);
assert.deepEqual(contextProvider.properties.reasoning_history?.enum, [
  ...CHAT_COMPLETIONS_REASONING_HISTORY_VALUES,
]);
assert.deepEqual(
  contextProvider.properties.reasoning_budget_tokens?.type,
  "integer",
);
for (const field of [
  "thinking_settings_applied",
  "thinking_mode_applied",
  "reasoning_history_applied",
  "reasoning_budget_applied",
]) {
  assert.deepEqual(
    contextProvider.properties[field]?.type,
    "boolean",
    `context provider ${field} must remain boolean`,
  );
}

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
  if (
    value.$ref?.startsWith(
      "./rusty-crew-api-capabilities.openapi.json#/components/schemas/",
    )
  ) {
    const generatedName = value.$ref.split("/").at(-1);
    assert.ok(generatedName, `invalid generated schema ref for ${name}`);
    const generated = capabilityContract.components.schemas[generatedName];
    assert.ok(generated, `missing generated schema ${generatedName}`);
    return generated;
  }
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
  $ref?: string;
  type?: string;
  enum?: string[];
  required?: string[];
  oneOf?: JsonSchema[];
  properties?: Record<string, JsonSchema>;
}
