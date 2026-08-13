export const MODEL_ENDPOINT_ADMIN_CONTRACT_VERSION = "0.1.0";

export const MODEL_ENDPOINT_ADMIN_OPENAPI_PATH =
  "docs/model-provider-admin-api-v0.openapi.json";

export const MODEL_ENDPOINT_ADMIN_PATHS = {
  endpoints: "/v1/admin/model-endpoints",
  endpoint: "/v1/admin/model-endpoints/{endpointId}",
  configurations: "/v1/admin/model-configurations",
  configuration: "/v1/admin/model-configurations/{modelConfigId}",
} as const;

export const MODEL_ENDPOINT_STATUS_VALUES = [
  "active",
  "disabled",
  "archived",
] as const;

export type ModelEndpointStatus = (typeof MODEL_ENDPOINT_STATUS_VALUES)[number];

export const MODEL_ENDPOINT_PROTOCOL_VALUES = [
  "responses",
  "chat_completions",
] as const;

export type ModelEndpointProtocol =
  (typeof MODEL_ENDPOINT_PROTOCOL_VALUES)[number];

export const MODEL_ENDPOINT_WIRE_DIALECT_VALUES = [
  "openai_stateful",
  "openai_stateless",
  "generic_stateless",
  "deepseek",
  "meta",
  "standard",
  "kimi",
  "glm",
  "qwen",
] as const;

export type ModelEndpointWireDialect =
  (typeof MODEL_ENDPOINT_WIRE_DIALECT_VALUES)[number];

export const MODEL_ENDPOINT_RESPONSES_WIRE_DIALECT_VALUES = [
  "openai_stateful",
  "openai_stateless",
  "generic_stateless",
  "deepseek",
  "meta",
] as const;

export const MODEL_ENDPOINT_CHAT_COMPLETIONS_WIRE_DIALECT_VALUES = [
  "standard",
  "kimi",
  "glm",
  "qwen",
  "deepseek",
] as const;

export const MODEL_ENDPOINT_AUTH_SCHEME_VALUES = [
  "none",
  "bearer_api_key",
  "openai_codex_oauth",
] as const;

export type ModelEndpointAuthScheme =
  (typeof MODEL_ENDPOINT_AUTH_SCHEME_VALUES)[number];

export const MODEL_ENDPOINT_PROMPT_CACHE_TRANSPORT_VALUES = [
  "none",
  "openrouter_anthropic",
] as const;

export type ModelEndpointPromptCacheTransport =
  (typeof MODEL_ENDPOINT_PROMPT_CACHE_TRANSPORT_VALUES)[number];

export const MODEL_CONFIGURATION_REASONING_HISTORY_VALUES = [
  "provider_default",
  "discard",
  "preserve_all",
  "tool_calls_only",
] as const;

export type ModelConfigurationReasoningHistory =
  (typeof MODEL_CONFIGURATION_REASONING_HISTORY_VALUES)[number];

export const MODEL_CONFIGURATION_THINKING_MODE_VALUES = [
  "provider_default",
  "enabled",
  "disabled",
] as const;

export type ModelConfigurationThinkingMode =
  (typeof MODEL_CONFIGURATION_THINKING_MODE_VALUES)[number];

export const MODEL_CONFIGURATION_PROMPT_CACHING_POLICY_VALUES = [
  "disabled",
  "automatic_5m",
  "automatic_1h",
] as const;

export type ModelConfigurationPromptCachingPolicy =
  (typeof MODEL_CONFIGURATION_PROMPT_CACHING_POLICY_VALUES)[number];

export const MODEL_CAPABILITIES_VERSION = 1;

export interface ModelCapabilities {
  version: number;
  imageInput: boolean;
}

export const MODEL_ENDPOINT_ADMIN_REASON_CODES = {
  invalidEndpoint: "invalid_model_endpoint",
  invalidConfiguration: "invalid_model_configuration",
  invalidStatus: "invalid_model_endpoint_status",
  endpointNotFound: "model_endpoint_not_found",
  configurationNotFound: "model_configuration_not_found",
  endpointRevisionMismatch: "model_endpoint_revision_mismatch",
  configurationRevisionMismatch: "model_configuration_revision_mismatch",
  methodNotAllowed: "model_endpoint_admin_method_not_allowed",
} as const;

export const MODEL_ENDPOINT_API_RECORD_REQUIRED_FIELDS = [
  "endpointId",
  "status",
  "baseUrl",
  "protocol",
  "wireDialect",
  "authScheme",
  "promptCacheTransport",
  "metadataJson",
  "revision",
  "createdAt",
  "updatedAt",
] as const;

export const MODEL_CONFIGURATION_API_RECORD_REQUIRED_FIELDS = [
  "modelConfigId",
  "endpointId",
  "status",
  "modelId",
  "reasoningHistory",
  "thinkingMode",
  "promptCachingPolicy",
  "capabilities",
  "metadataJson",
  "revision",
  "createdAt",
  "updatedAt",
] as const;

export interface NativeModelEndpointRecord {
  endpointId: string;
  status: ModelEndpointStatus;
  displayName?: string;
  description?: string;
  baseUrl: string;
  protocol: ModelEndpointProtocol;
  wireDialect: ModelEndpointWireDialect;
  authScheme: ModelEndpointAuthScheme;
  credentialId?: string;
  promptCacheTransport: ModelEndpointPromptCacheTransport;
  metadataJson: unknown;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeModelEndpointWrite {
  endpointId: string;
  status: ModelEndpointStatus;
  displayName?: string;
  description?: string;
  baseUrl: string;
  protocol: ModelEndpointProtocol;
  wireDialect: ModelEndpointWireDialect;
  authScheme?: ModelEndpointAuthScheme;
  credentialId?: string;
  promptCacheTransport?: ModelEndpointPromptCacheTransport;
  metadataJson?: unknown;
  expectedRevision?: number;
  now: string;
}

export interface NativeModelEndpointQuery {
  endpointId?: string;
  status?: ModelEndpointStatus;
  limit?: number;
  offset?: number;
}

export interface NativeModelConfigurationRecord {
  modelConfigId: string;
  endpointId: string;
  status: ModelEndpointStatus;
  displayName?: string;
  description?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  reasoningHistory: ModelConfigurationReasoningHistory;
  reasoningBudgetTokens?: number;
  thinkingMode: ModelConfigurationThinkingMode;
  promptCachingPolicy: ModelConfigurationPromptCachingPolicy;
  capabilities: ModelCapabilities;
  metadataJson: unknown;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeModelConfigurationWrite {
  modelConfigId: string;
  endpointId: string;
  status: ModelEndpointStatus;
  displayName?: string;
  description?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  reasoningHistory?: ModelConfigurationReasoningHistory;
  reasoningBudgetTokens?: number;
  thinkingMode?: ModelConfigurationThinkingMode;
  promptCachingPolicy?: ModelConfigurationPromptCachingPolicy;
  capabilities?: ModelCapabilities;
  metadataJson?: unknown;
  expectedRevision?: number;
  now: string;
}

export interface NativeModelConfigurationQuery {
  modelConfigId?: string;
  endpointId?: string;
  status?: ModelEndpointStatus;
  limit?: number;
  offset?: number;
}

export function isModelEndpointStatusContractValue(
  value: string,
): value is ModelEndpointStatus {
  return MODEL_ENDPOINT_STATUS_VALUES.includes(value as ModelEndpointStatus);
}

export function isModelEndpointProtocolContractValue(
  value: string,
): value is ModelEndpointProtocol {
  return MODEL_ENDPOINT_PROTOCOL_VALUES.includes(
    value as ModelEndpointProtocol,
  );
}

export function isModelEndpointWireDialectContractValue(
  value: string,
): value is ModelEndpointWireDialect {
  return MODEL_ENDPOINT_WIRE_DIALECT_VALUES.includes(
    value as ModelEndpointWireDialect,
  );
}

export function isModelEndpointAuthSchemeContractValue(
  value: string,
): value is ModelEndpointAuthScheme {
  return MODEL_ENDPOINT_AUTH_SCHEME_VALUES.includes(
    value as ModelEndpointAuthScheme,
  );
}

export function isModelEndpointPromptCacheTransportContractValue(
  value: string,
): value is ModelEndpointPromptCacheTransport {
  return MODEL_ENDPOINT_PROMPT_CACHE_TRANSPORT_VALUES.includes(
    value as ModelEndpointPromptCacheTransport,
  );
}

export function isModelConfigurationReasoningHistoryContractValue(
  value: string,
): value is ModelConfigurationReasoningHistory {
  return MODEL_CONFIGURATION_REASONING_HISTORY_VALUES.includes(
    value as ModelConfigurationReasoningHistory,
  );
}

export function isModelConfigurationThinkingModeContractValue(
  value: string,
): value is ModelConfigurationThinkingMode {
  return MODEL_CONFIGURATION_THINKING_MODE_VALUES.includes(
    value as ModelConfigurationThinkingMode,
  );
}

export function isModelConfigurationPromptCachingPolicyContractValue(
  value: string,
): value is ModelConfigurationPromptCachingPolicy {
  return MODEL_CONFIGURATION_PROMPT_CACHING_POLICY_VALUES.includes(
    value as ModelConfigurationPromptCachingPolicy,
  );
}
