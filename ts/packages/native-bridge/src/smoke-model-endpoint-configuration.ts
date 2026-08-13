import assert from "node:assert/strict";
import {
  createNativeBridgeNormalizedModelMethods,
  type NativeBridgeBindingWithNormalizedModelMethods,
  type NormalizedModelNativeBridgeBinding,
} from "./model-endpoint-wrappers.js";

const endpointRecord = {
  endpoint_id: "endpoint-openai",
  status: "active" as const,
  display_name: "OpenAI",
  description: null,
  base_url: "https://api.openai.com/v1",
  protocol: "responses" as const,
  wire_dialect: "openai_stateless" as const,
  auth_scheme: "bearer_api_key" as const,
  credential_id: "credential-openai",
  prompt_cache_transport: "none" as const,
  metadata_json: { owner: "crew" },
  revision: 3,
  created_at: "2026-08-12T00:00:00Z",
  updated_at: "2026-08-12T01:00:00Z",
};

const configurationRecord = {
  model_config_id: "config-gpt-5",
  endpoint_id: "endpoint-openai",
  status: "active" as const,
  display_name: "GPT-5",
  description: null,
  model_id: "gpt-5",
  context_window_tokens: 128000,
  max_output_tokens: 16000,
  temperature_milli: 700,
  reasoning_effort: "high",
  reasoning_format: null,
  reasoning_history: "preserve_all" as const,
  reasoning_budget_tokens: 20000,
  thinking_mode: "enabled" as const,
  prompt_caching_policy: "automatic_5m" as const,
  capabilities: { version: 1, image_input: true },
  metadata_json: { owner: "crew" },
  revision: 2,
  created_at: "2026-08-12T00:00:00Z",
  updated_at: "2026-08-12T01:00:00Z",
};

function parsedInput(inputJson: string): Record<string, unknown> {
  return JSON.parse(inputJson) as Record<string, unknown>;
}

const binding: NormalizedModelNativeBridgeBinding = {
  upsertModelEndpointJson(inputJson) {
    const input = parsedInput(inputJson);
    assert.equal(input.endpoint_id, "endpoint-openai");
    assert.equal(input.auth_scheme, "none");
    assert.equal(input.prompt_cache_transport, "none");
    assert.deepEqual(input.metadata_json, {});
    assert.equal("secret" in input, false);
    return JSON.stringify(endpointRecord);
  },
  listModelEndpointsJson(inputJson) {
    assert.deepEqual(parsedInput(inputJson), {
      endpoint_id: "endpoint-openai",
      limit: 10,
    });
    return JSON.stringify([endpointRecord]);
  },
  getModelEndpointJson(endpointId) {
    return endpointId === "missing" ? "null" : JSON.stringify(endpointRecord);
  },
  upsertModelConfigurationJson(inputJson) {
    const input = parsedInput(inputJson);
    assert.equal(input.model_config_id, "config-gpt-5");
    assert.equal(input.reasoning_history, "provider_default");
    assert.equal(input.thinking_mode, "provider_default");
    assert.equal(input.prompt_caching_policy, "disabled");
    assert.deepEqual(input.capabilities, { version: 1, image_input: false });
    return JSON.stringify(configurationRecord);
  },
  listModelConfigurationsJson(inputJson) {
    assert.deepEqual(parsedInput(inputJson), {
      model_config_id: "config-gpt-5",
      endpoint_id: "endpoint-openai",
      status: "active",
    });
    return JSON.stringify([configurationRecord]);
  },
  getModelConfigurationJson(modelConfigId) {
    return modelConfigId === "missing"
      ? "null"
      : JSON.stringify(configurationRecord);
  },
};

const methods = createNativeBridgeNormalizedModelMethods(
  binding as NativeBridgeBindingWithNormalizedModelMethods,
);

const endpoint = await methods.upsertModelEndpoint({
  endpointId: "endpoint-openai",
  status: "active",
  baseUrl: "https://api.openai.com/v1",
  protocol: "responses",
  wireDialect: "openai_stateless",
  credentialId: "credential-openai",
  now: "2026-08-12T02:00:00Z",
});
assert.equal(endpoint.endpointId, "endpoint-openai");
assert.equal(endpoint.credentialId, "credential-openai");
assert.equal("secret" in endpoint, false);

const endpoints = await methods.listModelEndpoints({
  endpointId: "endpoint-openai",
  limit: 10,
});
assert.equal(endpoints.length, 1);
assert.equal(await methods.getModelEndpoint("missing"), undefined);

const configuration = await methods.upsertModelConfiguration({
  modelConfigId: "config-gpt-5",
  endpointId: "endpoint-openai",
  status: "active",
  modelId: "gpt-5",
  now: "2026-08-12T02:00:00Z",
});
assert.equal(configuration.modelConfigId, "config-gpt-5");
assert.equal(configuration.capabilities.imageInput, true);

const configurations = await methods.listModelConfigurations({
  modelConfigId: "config-gpt-5",
  endpointId: "endpoint-openai",
  status: "active",
});
assert.equal(configurations.length, 1);
assert.equal(await methods.getModelConfiguration("missing"), undefined);

console.log("normalized model endpoint/configuration bridge smoke passed");
