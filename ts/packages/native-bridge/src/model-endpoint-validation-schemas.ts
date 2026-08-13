import { Type, type TSchema } from "typebox";

import { generatedBridgeOutputSchemas } from "./generated/bridge-wire-schemas.js";

const generatedSchemas = generatedBridgeOutputSchemas as unknown as Record<
  string,
  TSchema | undefined
>;
const nullableString = Type.Union([Type.String(), Type.Null()]);
const nullableNumber = Type.Union([Type.Number(), Type.Null()]);

const modelEndpointStatusSchema = Type.Union([
  Type.Literal("active"),
  Type.Literal("disabled"),
  Type.Literal("archived"),
]);
const modelEndpointProtocolSchema = Type.Union([
  Type.Literal("responses"),
  Type.Literal("chat_completions"),
]);
const modelEndpointWireDialectSchema = Type.Union([
  Type.Literal("openai_stateful"),
  Type.Literal("openai_stateless"),
  Type.Literal("generic_stateless"),
  Type.Literal("deepseek"),
  Type.Literal("meta"),
  Type.Literal("standard"),
  Type.Literal("kimi"),
  Type.Literal("glm"),
  Type.Literal("qwen"),
]);
const modelEndpointAuthSchemeSchema = Type.Union([
  Type.Literal("none"),
  Type.Literal("bearer_api_key"),
  Type.Literal("openai_codex_oauth"),
]);
const promptCacheTransportSchema = Type.Union([
  Type.Literal("none"),
  Type.Literal("openrouter_anthropic"),
]);
const modelReasoningHistorySchema = Type.Union([
  Type.Literal("provider_default"),
  Type.Literal("discard"),
  Type.Literal("preserve_all"),
  Type.Literal("tool_calls_only"),
]);
const modelThinkingModeSchema = Type.Union([
  Type.Literal("provider_default"),
  Type.Literal("enabled"),
  Type.Literal("disabled"),
]);
const modelPromptCachingPolicySchema = Type.Union([
  Type.Literal("disabled"),
  Type.Literal("automatic_5m"),
  Type.Literal("automatic_1h"),
]);
const modelCapabilitiesSchema = Type.Object(
  {
    version: Type.Number(),
    image_input: Type.Boolean(),
  },
  { additionalProperties: false },
);

const fallbackModelEndpointRecordSchema = Type.Object(
  {
    endpoint_id: Type.String(),
    status: modelEndpointStatusSchema,
    display_name: Type.Optional(nullableString),
    description: Type.Optional(nullableString),
    base_url: Type.String(),
    protocol: modelEndpointProtocolSchema,
    wire_dialect: modelEndpointWireDialectSchema,
    auth_scheme: modelEndpointAuthSchemeSchema,
    credential_id: Type.Optional(nullableString),
    prompt_cache_transport: promptCacheTransportSchema,
    metadata_json: Type.Unknown(),
    revision: Type.Number(),
    created_at: Type.String(),
    updated_at: Type.String(),
  },
  { additionalProperties: false },
);

const fallbackModelConfigurationRecordSchema = Type.Object(
  {
    model_config_id: Type.String(),
    endpoint_id: Type.String(),
    status: modelEndpointStatusSchema,
    display_name: Type.Optional(nullableString),
    description: Type.Optional(nullableString),
    model_id: Type.String(),
    context_window_tokens: Type.Optional(nullableNumber),
    max_output_tokens: Type.Optional(nullableNumber),
    temperature_milli: Type.Optional(nullableNumber),
    reasoning_effort: Type.Optional(nullableString),
    reasoning_format: Type.Optional(nullableString),
    reasoning_history: modelReasoningHistorySchema,
    reasoning_budget_tokens: Type.Optional(nullableNumber),
    thinking_mode: modelThinkingModeSchema,
    prompt_caching_policy: modelPromptCachingPolicySchema,
    capabilities: modelCapabilitiesSchema,
    metadata_json: Type.Unknown(),
    revision: Type.Number(),
    created_at: Type.String(),
    updated_at: Type.String(),
  },
  { additionalProperties: false },
);

// The six operation entries are supplied by bridge codegen once the normalized
// manifest is regenerated. Until then, these local schemas keep this adapter
// validated without editing generated artifacts.
export const rawModelEndpointRecordSchema =
  generatedSchemas.upsert_model_endpoint ?? fallbackModelEndpointRecordSchema;
export const rawModelEndpointRecordArraySchema =
  generatedSchemas.list_model_endpoints ??
  Type.Array(fallbackModelEndpointRecordSchema);
export const rawModelConfigurationRecordSchema =
  generatedSchemas.upsert_model_configuration ??
  fallbackModelConfigurationRecordSchema;
export const rawModelConfigurationRecordArraySchema =
  generatedSchemas.list_model_configurations ??
  Type.Array(fallbackModelConfigurationRecordSchema);
