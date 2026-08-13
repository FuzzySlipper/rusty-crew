import type {
  NativeModelCapabilities,
  NativeModelConfigurationQuery,
  NativeModelConfigurationDelete,
  NativeModelConfigurationRecord,
  NativeModelConfigurationWrite,
  NativeModelEndpointQuery,
  NativeModelEndpointDelete,
  NativeModelEndpointRecord,
  NativeModelEndpointWrite,
} from "./model-endpoint-public-api.js";

export interface RawModelCapabilities {
  version: number;
  image_input: boolean;
}

export interface RawModelEndpointRecord {
  endpoint_id: string;
  status: NativeModelEndpointRecord["status"];
  display_name?: string | null;
  description?: string | null;
  base_url: string;
  protocol: NativeModelEndpointRecord["protocol"];
  wire_dialect: NativeModelEndpointRecord["wireDialect"];
  auth_scheme: NativeModelEndpointRecord["authScheme"];
  credential_id?: string | null;
  prompt_cache_transport: NativeModelEndpointRecord["promptCacheTransport"];
  metadata_json: unknown;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface RawModelEndpointWrite {
  endpoint_id: string;
  status: NativeModelEndpointWrite["status"];
  display_name?: string;
  description?: string;
  base_url: string;
  protocol: NativeModelEndpointWrite["protocol"];
  wire_dialect: NativeModelEndpointWrite["wireDialect"];
  auth_scheme: NonNullable<NativeModelEndpointWrite["authScheme"]>;
  credential_id?: string;
  prompt_cache_transport: NonNullable<
    NativeModelEndpointWrite["promptCacheTransport"]
  >;
  metadata_json: unknown;
  expected_revision?: number;
  now: string;
}

export interface RawModelEndpointQuery {
  endpoint_id?: string;
  status?: NativeModelEndpointQuery["status"];
  limit?: number;
  offset?: number;
}

export interface RawModelEndpointDelete {
  endpoint_id: string;
  expected_revision: number;
}

export interface RawModelConfigurationRecord {
  model_config_id: string;
  endpoint_id: string;
  status: NativeModelConfigurationRecord["status"];
  display_name?: string | null;
  description?: string | null;
  model_id: string;
  context_window_tokens?: number | null;
  max_output_tokens?: number | null;
  temperature_milli?: number | null;
  reasoning_effort?: string | null;
  reasoning_format?: string | null;
  reasoning_history: NativeModelConfigurationRecord["reasoningHistory"];
  reasoning_budget_tokens?: number | null;
  thinking_mode: NativeModelConfigurationRecord["thinkingMode"];
  prompt_caching_policy: NativeModelConfigurationRecord["promptCachingPolicy"];
  capabilities: RawModelCapabilities;
  metadata_json: unknown;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface RawModelConfigurationWrite {
  model_config_id: string;
  endpoint_id: string;
  status: NativeModelConfigurationWrite["status"];
  display_name?: string;
  description?: string;
  model_id: string;
  context_window_tokens?: number;
  max_output_tokens?: number;
  temperature_milli?: number;
  reasoning_effort?: string;
  reasoning_format?: string;
  reasoning_history: NativeModelConfigurationWrite["reasoningHistory"];
  reasoning_budget_tokens?: number;
  thinking_mode: NonNullable<NativeModelConfigurationWrite["thinkingMode"]>;
  prompt_caching_policy: NonNullable<
    NativeModelConfigurationWrite["promptCachingPolicy"]
  >;
  capabilities: RawModelCapabilities;
  metadata_json: unknown;
  expected_revision?: number;
  now: string;
}

export interface RawModelConfigurationQuery {
  model_config_id?: string;
  endpoint_id?: string;
  status?: NativeModelConfigurationQuery["status"];
  limit?: number;
  offset?: number;
}

export interface RawModelConfigurationDelete {
  model_config_id: string;
  expected_revision: number;
}

export function toRawModelEndpointDelete(
  deleteRequest: NativeModelEndpointDelete,
): RawModelEndpointDelete {
  return {
    endpoint_id: deleteRequest.endpointId,
    expected_revision: deleteRequest.expectedRevision,
  };
}

export function toRawModelConfigurationDelete(
  deleteRequest: NativeModelConfigurationDelete,
): RawModelConfigurationDelete {
  return {
    model_config_id: deleteRequest.modelConfigId,
    expected_revision: deleteRequest.expectedRevision,
  };
}

function toNativeModelCapabilities(
  capabilities: RawModelCapabilities,
): NativeModelCapabilities {
  return {
    version: capabilities.version,
    imageInput: capabilities.image_input,
  };
}

function toRawModelCapabilities(
  capabilities: NativeModelCapabilities,
): RawModelCapabilities {
  return {
    version: capabilities.version,
    image_input: capabilities.imageInput,
  };
}

export function toNativeModelEndpointRecord(
  record: RawModelEndpointRecord,
): NativeModelEndpointRecord {
  return {
    endpointId: record.endpoint_id,
    status: record.status,
    displayName: record.display_name ?? undefined,
    description: record.description ?? undefined,
    baseUrl: record.base_url,
    protocol: record.protocol,
    wireDialect: record.wire_dialect,
    authScheme: record.auth_scheme,
    credentialId: record.credential_id ?? undefined,
    promptCacheTransport: record.prompt_cache_transport,
    metadataJson: record.metadata_json,
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
  };
}

export function toRawModelEndpointRecord(
  record: NativeModelEndpointRecord,
): RawModelEndpointRecord {
  return {
    endpoint_id: record.endpointId,
    status: record.status,
    display_name: record.displayName,
    description: record.description,
    base_url: record.baseUrl,
    protocol: record.protocol,
    wire_dialect: record.wireDialect,
    auth_scheme: record.authScheme,
    credential_id: record.credentialId,
    prompt_cache_transport: record.promptCacheTransport,
    metadata_json: record.metadataJson,
    revision: record.revision,
    created_at: record.createdAt,
    updated_at: record.updatedAt,
  };
}

export function toRawModelEndpointWrite(
  write: NativeModelEndpointWrite,
): RawModelEndpointWrite {
  return {
    endpoint_id: write.endpointId,
    status: write.status,
    display_name: write.displayName,
    description: write.description,
    base_url: write.baseUrl,
    protocol: write.protocol,
    wire_dialect: write.wireDialect,
    auth_scheme: write.authScheme ?? "none",
    credential_id: write.credentialId,
    prompt_cache_transport: write.promptCacheTransport ?? "none",
    metadata_json: write.metadataJson ?? {},
    expected_revision: write.expectedRevision,
    now: write.now,
  };
}

export function toRawModelEndpointQuery(
  query: NativeModelEndpointQuery,
): RawModelEndpointQuery {
  return {
    endpoint_id: query.endpointId,
    status: query.status,
    limit: query.limit,
    offset: query.offset,
  };
}

export function toNativeModelConfigurationRecord(
  record: RawModelConfigurationRecord,
): NativeModelConfigurationRecord {
  return {
    modelConfigId: record.model_config_id,
    endpointId: record.endpoint_id,
    status: record.status,
    displayName: record.display_name ?? undefined,
    description: record.description ?? undefined,
    modelId: record.model_id,
    contextWindowTokens: record.context_window_tokens ?? undefined,
    maxOutputTokens: record.max_output_tokens ?? undefined,
    temperatureMilli: record.temperature_milli ?? undefined,
    reasoningEffort: record.reasoning_effort ?? undefined,
    reasoningFormat: record.reasoning_format ?? undefined,
    reasoningHistory: record.reasoning_history,
    reasoningBudgetTokens: record.reasoning_budget_tokens ?? undefined,
    thinkingMode: record.thinking_mode,
    promptCachingPolicy: record.prompt_caching_policy,
    capabilities: toNativeModelCapabilities(record.capabilities),
    metadataJson: record.metadata_json,
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
  };
}

export function toRawModelConfigurationRecord(
  record: NativeModelConfigurationRecord,
): RawModelConfigurationRecord {
  return {
    model_config_id: record.modelConfigId,
    endpoint_id: record.endpointId,
    status: record.status,
    display_name: record.displayName,
    description: record.description,
    model_id: record.modelId,
    context_window_tokens: record.contextWindowTokens,
    max_output_tokens: record.maxOutputTokens,
    temperature_milli: record.temperatureMilli,
    reasoning_effort: record.reasoningEffort,
    reasoning_format: record.reasoningFormat,
    reasoning_history: record.reasoningHistory,
    reasoning_budget_tokens: record.reasoningBudgetTokens,
    thinking_mode: record.thinkingMode,
    prompt_caching_policy: record.promptCachingPolicy,
    capabilities: toRawModelCapabilities(record.capabilities),
    metadata_json: record.metadataJson,
    revision: record.revision,
    created_at: record.createdAt,
    updated_at: record.updatedAt,
  };
}

export function toRawModelConfigurationWrite(
  write: NativeModelConfigurationWrite,
): RawModelConfigurationWrite {
  return {
    model_config_id: write.modelConfigId,
    endpoint_id: write.endpointId,
    status: write.status,
    display_name: write.displayName,
    description: write.description,
    model_id: write.modelId,
    context_window_tokens: write.contextWindowTokens,
    max_output_tokens: write.maxOutputTokens,
    temperature_milli: write.temperatureMilli,
    reasoning_effort: write.reasoningEffort,
    reasoning_format: write.reasoningFormat,
    reasoning_history: write.reasoningHistory ?? "provider_default",
    reasoning_budget_tokens: write.reasoningBudgetTokens,
    thinking_mode: write.thinkingMode ?? "provider_default",
    prompt_caching_policy: write.promptCachingPolicy ?? "disabled",
    capabilities: toRawModelCapabilities(
      write.capabilities ?? { version: 1, imageInput: false },
    ),
    metadata_json: write.metadataJson ?? {},
    expected_revision: write.expectedRevision,
    now: write.now,
  };
}

export function toRawModelConfigurationQuery(
  query: NativeModelConfigurationQuery,
): RawModelConfigurationQuery {
  return {
    model_config_id: query.modelConfigId,
    endpoint_id: query.endpointId,
    status: query.status,
    limit: query.limit,
    offset: query.offset,
  };
}
