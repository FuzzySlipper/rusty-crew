import type { BrainModelConfig } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeModelConfigurationRecord,
  NativeModelEndpointRecord,
  NativeModelProviderRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import { narratorImageInputCapability } from "./narrator-image-context.js";

/**
 * Resolve the independently revisioned registry records into the immutable,
 * secret-free model snapshot consumed by one registered brain runtime.
 */
export async function resolveModelConfigurationForBrain(
  bridge: Pick<
    NativeBridgeModule,
    | "getModelConfiguration"
    | "getModelEndpoint"
    | "getServiceCredential"
    | "getServiceCredentialSecret"
  >,
  modelConfigId: string,
): Promise<Readonly<BrainModelConfig>> {
  const configuration = await bridge.getModelConfiguration(modelConfigId);
  if (configuration === undefined) {
    throw new Error(`model configuration ${modelConfigId} was not found`);
  }
  if (configuration.status !== "active") {
    throw new Error(
      `model configuration ${modelConfigId} is ${configuration.status}; active configuration required`,
    );
  }
  const endpoint = await bridge.getModelEndpoint(configuration.endpointId);
  if (endpoint === undefined) {
    throw new Error(
      `model configuration ${modelConfigId} references missing endpoint ${configuration.endpointId}`,
    );
  }
  if (endpoint.status !== "active") {
    throw new Error(
      `model endpoint ${endpoint.endpointId} is ${endpoint.status}; active endpoint required`,
    );
  }
  const credential = await resolvedCredential(bridge, endpoint);
  const secret =
    credential?.credential.hasSecret === true
      ? await bridge.getServiceCredentialSecret(credential.credentialId)
      : undefined;
  return Object.freeze(
    normalizedRecordsToBrainModelConfig(
      configuration,
      endpoint,
      credential,
      secret,
    ),
  );
}

/** Resolve the compatibility provider registry during an explicit rollback. */
export async function resolveModelProviderForBrain(
  bridge: Pick<
    NativeBridgeModule,
    "getModelProvider" | "getModelProviderSecret"
  >,
  alias: string,
): Promise<BrainModelConfig> {
  const provider = await bridge.getModelProvider(alias);
  if (provider === undefined) {
    throw new Error(`model provider alias ${alias} was not found`);
  }
  if (provider.status !== "active") {
    throw new Error(
      `model provider alias ${alias} is ${provider.status}; active provider required`,
    );
  }
  const secret = provider.credential.hasSecret
    ? await bridge.getModelProviderSecret(alias)
    : undefined;
  return modelProviderToBrainModelConfig(provider, secret);
}

function modelProviderToBrainModelConfig(
  provider: NativeModelProviderRecord,
  secret: string | undefined,
): BrainModelConfig {
  const apiKey = modelProviderApiKeySecret(provider, secret);
  const credentialKind =
    provider.credential.kind ??
    (apiKey === undefined ? undefined : "legacy_raw_api_key");
  const apiKeyEnv =
    apiKey === undefined
      ? undefined
      : modelProviderSecretEnvName(provider.alias);
  if (apiKeyEnv !== undefined) {
    process.env[apiKeyEnv] = apiKey;
  }
  return {
    provider: provider.providerKind,
    modelName: provider.modelId,
    baseUrl: provider.baseUrl,
    api:
      provider.protocol === "responses"
        ? "openai-responses"
        : "openai-completions",
    apiKeyEnv,
    credentialKind,
    contextWindowTokens: provider.contextWindowTokens,
    temperatureMilli: provider.temperatureMilli,
    maxOutputTokens: provider.maxOutputTokens,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    responsesDialect: provider.responsesDialect,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    promptCaching: provider.promptCaching,
    narratorImageInput: narratorImageInputCapability(provider.metadataJson),
  };
}

function modelProviderApiKeySecret(
  provider: NativeModelProviderRecord,
  secret: string | undefined,
): string | undefined {
  if (secret === undefined) {
    return undefined;
  }
  const trimmed = secret.trim();
  if (!trimmed.startsWith("{")) {
    return secret;
  }
  const envelope = JSON.parse(trimmed) as unknown;
  if (!isRecord(envelope)) {
    throw new Error(
      `model provider ${provider.alias} secret envelope is invalid`,
    );
  }
  if (envelope.kind === "api_key" && typeof envelope.value === "string") {
    return envelope.value;
  }
  if (envelope.kind === "openai_oauth") {
    return undefined;
  }
  throw new Error(
    `model provider ${provider.alias} secret envelope kind is unsupported`,
  );
}

function modelProviderSecretEnvName(alias: string): string {
  return `RUSTY_CREW_MODEL_PROVIDER_SECRET_${alias
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")}`;
}

function normalizedRecordsToBrainModelConfig(
  configuration: NativeModelConfigurationRecord,
  endpoint: NativeModelEndpointRecord,
  credential: NativeServiceCredentialRecord | undefined,
  secret: string | undefined,
): BrainModelConfig {
  const apiKey = credentialApiKeySecret(credential, secret);
  const apiKeyEnv =
    apiKey === undefined || credential === undefined
      ? undefined
      : credentialSecretEnvName(credential.credentialId);
  if (apiKeyEnv !== undefined) {
    process.env[apiKeyEnv] = apiKey;
  }
  return {
    modelConfigId: configuration.modelConfigId,
    modelConfigRevision: configuration.revision,
    endpointId: endpoint.endpointId,
    endpointRevision: endpoint.revision,
    authScheme: endpoint.authScheme,
    promptCacheTransport: endpoint.promptCacheTransport,
    ...(credential === undefined
      ? {}
      : {
          credentialId: credential.credentialId,
          credentialRevision: credential.revision,
          credentialKind: credential.credentialKind,
        }),
    // Retained as a compatibility display field. Runtime behavior is selected
    // by explicit protocol/dialect/auth fields below, never by a vendor label.
    provider: endpoint.endpointId,
    modelName: configuration.modelId,
    baseUrl: endpoint.baseUrl,
    api:
      endpoint.protocol === "responses"
        ? "openai-responses"
        : "openai-completions",
    apiKeyEnv,
    contextWindowTokens: configuration.contextWindowTokens,
    temperatureMilli: configuration.temperatureMilli,
    maxOutputTokens: configuration.maxOutputTokens,
    reasoningEffort: configuration.reasoningEffort,
    reasoningFormat: configuration.reasoningFormat,
    responsesDialect:
      endpoint.protocol === "responses"
        ? responsesDialect(endpoint)
        : undefined,
    chatCompletionsDialect:
      endpoint.protocol === "chat_completions"
        ? chatCompletionsDialect(endpoint)
        : undefined,
    thinkingMode: configuration.thinkingMode,
    reasoningHistory: configuration.reasoningHistory,
    reasoningBudgetTokens: configuration.reasoningBudgetTokens,
    promptCaching: configuration.promptCachingPolicy,
    narratorImageInput: narratorImageInputCapability({
      narratorImageInput: {
        supported: configuration.capabilities.imageInput,
      },
    }),
  };
}

async function resolvedCredential(
  bridge: Pick<NativeBridgeModule, "getServiceCredential">,
  endpoint: NativeModelEndpointRecord,
): Promise<NativeServiceCredentialRecord | undefined> {
  if (endpoint.credentialId === undefined) {
    if (endpoint.authScheme !== "none") {
      throw new Error(
        `model endpoint ${endpoint.endpointId} requires ${endpoint.authScheme} but has no credential`,
      );
    }
    return undefined;
  }
  const credential = await bridge.getServiceCredential(endpoint.credentialId);
  if (credential === undefined) {
    throw new Error(
      `model endpoint ${endpoint.endpointId} references missing credential ${endpoint.credentialId}`,
    );
  }
  return credential;
}

function credentialApiKeySecret(
  credential: NativeServiceCredentialRecord | undefined,
  secret: string | undefined,
): string | undefined {
  if (credential === undefined || secret === undefined) {
    return undefined;
  }
  if (credential.credentialKind === "openai_oauth") {
    return undefined;
  }
  const trimmed = secret.trim();
  if (!trimmed.startsWith("{")) {
    return secret;
  }
  const envelope = JSON.parse(trimmed) as unknown;
  if (!isRecord(envelope)) {
    throw new Error(
      `service credential ${credential.credentialId} secret envelope is invalid`,
    );
  }
  if (envelope.kind === "api_key" && typeof envelope.value === "string") {
    return envelope.value;
  }
  throw new Error(
    `service credential ${credential.credentialId} secret envelope kind is incompatible with ${credential.credentialKind}`,
  );
}

function responsesDialect(
  endpoint: NativeModelEndpointRecord,
): BrainModelConfig["responsesDialect"] {
  switch (endpoint.wireDialect) {
    case "openai_stateful":
    case "openai_stateless":
    case "generic_stateless":
    case "deepseek":
    case "meta":
      return endpoint.wireDialect;
    default:
      throw new Error(
        `model endpoint ${endpoint.endpointId} has invalid Responses wire dialect ${endpoint.wireDialect}`,
      );
  }
}

function chatCompletionsDialect(
  endpoint: NativeModelEndpointRecord,
): BrainModelConfig["chatCompletionsDialect"] {
  switch (endpoint.wireDialect) {
    case "standard":
    case "kimi":
    case "glm":
    case "qwen":
    case "deepseek":
      return endpoint.wireDialect;
    default:
      throw new Error(
        `model endpoint ${endpoint.endpointId} has invalid Chat Completions wire dialect ${endpoint.wireDialect}`,
      );
  }
}

function credentialSecretEnvName(credentialId: string): string {
  return `RUSTY_CREW_SERVICE_CREDENTIAL_SECRET_${credentialId
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
