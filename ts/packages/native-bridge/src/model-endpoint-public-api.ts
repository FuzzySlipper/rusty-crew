import type {
  NativeChatCompletionsPromptCachingPolicy,
  NativeChatCompletionsReasoningHistory,
  NativeChatCompletionsThinkingMode,
} from "./model-provider-public-api.js";

export type NativeModelEndpointStatus = "active" | "disabled" | "archived";
export type NativeModelEndpointProtocol = "responses" | "chat_completions";
export type NativeModelEndpointWireDialect =
  | "openai_stateful"
  | "openai_stateless"
  | "generic_stateless"
  | "deepseek"
  | "meta"
  | "standard"
  | "kimi"
  | "glm"
  | "qwen";
export type NativeModelEndpointAuthScheme =
  | "none"
  | "bearer_api_key"
  | "openai_codex_oauth";
export type NativePromptCacheTransport = "none" | "openrouter_anthropic";

export type NativeModelReasoningHistory = NativeChatCompletionsReasoningHistory;
export type NativeModelThinkingMode = NativeChatCompletionsThinkingMode;
export type NativeModelPromptCachingPolicy =
  NativeChatCompletionsPromptCachingPolicy;

export interface NativeModelCapabilities {
  version: number;
  imageInput: boolean;
}

export interface NativeModelEndpointRecord {
  endpointId: string;
  status: NativeModelEndpointStatus;
  displayName?: string;
  description?: string;
  baseUrl: string;
  protocol: NativeModelEndpointProtocol;
  wireDialect: NativeModelEndpointWireDialect;
  authScheme: NativeModelEndpointAuthScheme;
  credentialId?: string;
  promptCacheTransport: NativePromptCacheTransport;
  metadataJson: unknown;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeModelEndpointWrite {
  endpointId: string;
  status: NativeModelEndpointStatus;
  displayName?: string;
  description?: string;
  baseUrl: string;
  protocol: NativeModelEndpointProtocol;
  wireDialect: NativeModelEndpointWireDialect;
  authScheme?: NativeModelEndpointAuthScheme;
  credentialId?: string;
  promptCacheTransport?: NativePromptCacheTransport;
  metadataJson?: unknown;
  expectedRevision?: number;
  now: string;
}

export interface NativeModelEndpointQuery {
  endpointId?: string;
  status?: NativeModelEndpointStatus;
  limit?: number;
  offset?: number;
}

export interface NativeModelEndpointDelete {
  endpointId: string;
  expectedRevision: number;
}

export interface NativeModelConfigurationRecord {
  modelConfigId: string;
  endpointId: string;
  status: NativeModelEndpointStatus;
  displayName?: string;
  description?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  reasoningHistory: NativeModelReasoningHistory;
  reasoningBudgetTokens?: number;
  thinkingMode: NativeModelThinkingMode;
  promptCachingPolicy: NativeModelPromptCachingPolicy;
  capabilities: NativeModelCapabilities;
  metadataJson: unknown;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeModelConfigurationWrite {
  modelConfigId: string;
  endpointId: string;
  status: NativeModelEndpointStatus;
  displayName?: string;
  description?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  reasoningHistory?: NativeModelReasoningHistory;
  reasoningBudgetTokens?: number;
  thinkingMode?: NativeModelThinkingMode;
  promptCachingPolicy?: NativeModelPromptCachingPolicy;
  capabilities?: NativeModelCapabilities;
  metadataJson?: unknown;
  expectedRevision?: number;
  now: string;
}

export interface NativeModelConfigurationQuery {
  modelConfigId?: string;
  endpointId?: string;
  status?: NativeModelEndpointStatus;
  limit?: number;
  offset?: number;
}

export interface NativeModelConfigurationDelete {
  modelConfigId: string;
  expectedRevision: number;
}

export interface NativeModelEndpointBridgeMethods {
  upsertModelEndpoint(
    write: NativeModelEndpointWrite,
  ): Promise<NativeModelEndpointRecord>;
  listModelEndpoints(
    query?: NativeModelEndpointQuery,
  ): Promise<NativeModelEndpointRecord[]>;
  getModelEndpoint(
    endpointId: string,
  ): Promise<NativeModelEndpointRecord | undefined>;
  deleteModelEndpoint(
    deleteRequest: NativeModelEndpointDelete,
  ): Promise<NativeModelEndpointRecord>;
  upsertModelConfiguration(
    write: NativeModelConfigurationWrite,
  ): Promise<NativeModelConfigurationRecord>;
  listModelConfigurations(
    query?: NativeModelConfigurationQuery,
  ): Promise<NativeModelConfigurationRecord[]>;
  getModelConfiguration(
    modelConfigId: string,
  ): Promise<NativeModelConfigurationRecord | undefined>;
  deleteModelConfiguration(
    deleteRequest: NativeModelConfigurationDelete,
  ): Promise<NativeModelConfigurationRecord>;
}
