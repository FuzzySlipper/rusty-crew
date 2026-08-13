import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import {
  MODEL_CAPABILITIES_VERSION,
  MODEL_ENDPOINT_ADMIN_REASON_CODES,
  MODEL_ENDPOINT_CHAT_COMPLETIONS_WIRE_DIALECT_VALUES,
  MODEL_ENDPOINT_PROMPT_CACHE_TRANSPORT_VALUES,
  MODEL_ENDPOINT_RESPONSES_WIRE_DIALECT_VALUES,
  MODEL_ENDPOINT_STATUS_VALUES,
  MODEL_ENDPOINT_WIRE_DIALECT_VALUES,
  MODEL_CONFIGURATION_PROMPT_CACHING_POLICY_VALUES,
  MODEL_CONFIGURATION_REASONING_HISTORY_VALUES,
  MODEL_CONFIGURATION_THINKING_MODE_VALUES,
  isModelConfigurationPromptCachingPolicyContractValue,
  isModelConfigurationReasoningHistoryContractValue,
  isModelConfigurationThinkingModeContractValue,
  isModelEndpointAuthSchemeContractValue,
  isModelEndpointProtocolContractValue,
  isModelEndpointPromptCacheTransportContractValue,
  isModelEndpointStatusContractValue,
  isModelEndpointWireDialectContractValue,
  type ModelCapabilities,
  type ModelConfigurationPromptCachingPolicy,
  type ModelConfigurationReasoningHistory,
  type ModelConfigurationThinkingMode,
  type ModelEndpointAuthScheme,
  type ModelEndpointProtocol,
  type ModelEndpointPromptCacheTransport,
  type ModelEndpointStatus,
  type ModelEndpointWireDialect,
  type NativeModelConfigurationQuery,
  type NativeModelConfigurationRecord,
  type NativeModelConfigurationWrite,
  type NativeModelEndpointQuery,
  type NativeModelEndpointRecord,
  type NativeModelEndpointWrite,
} from "./model-endpoint-admin-contract.js";
import { failure, successRoute } from "./service-route-results.js";

export interface ModelEndpointAdminRouteRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId: string;
}

/**
 * This is deliberately the six-method native bridge seam. The generated
 * native package owns the concrete declarations; keeping the route context
 * narrow lets the HTTP surface compile while that generated surface changes
 * in lockstep with the Rust bridge manifest.
 */
export interface ModelEndpointAdminRouteContext {
  upsertModelEndpoint(
    write: NativeModelEndpointWrite,
  ): Promise<NativeModelEndpointRecord>;
  listModelEndpoints(
    query: NativeModelEndpointQuery,
  ): Promise<NativeModelEndpointRecord[]>;
  getModelEndpoint(
    endpointId: string,
  ): Promise<NativeModelEndpointRecord | undefined>;
  upsertModelConfiguration(
    write: NativeModelConfigurationWrite,
  ): Promise<NativeModelConfigurationRecord>;
  listModelConfigurations(
    query: NativeModelConfigurationQuery,
  ): Promise<NativeModelConfigurationRecord[]>;
  getModelConfiguration(
    modelConfigId: string,
  ): Promise<NativeModelConfigurationRecord | undefined>;
  refreshAfterWrite?(input: {
    kind: "endpoint" | "configuration";
    id: string;
  }): Promise<{ profileIds: string[] }>;
  now(): string;
}

export function normalizedModelRefreshProfileIds(input: {
  kind: "endpoint" | "configuration";
  id: string;
  configurations: readonly Pick<
    NativeModelConfigurationRecord,
    "modelConfigId" | "endpointId"
  >[];
  profiles: readonly {
    profileId: string;
    activeRuntimeSettingsJson?: unknown;
  }[];
}): string[] {
  const modelConfigIds =
    input.kind === "configuration"
      ? new Set([input.id])
      : new Set(
          input.configurations
            .filter((configuration) => configuration.endpointId === input.id)
            .map((configuration) => configuration.modelConfigId),
        );
  return input.profiles
    .filter((profile) => {
      const settings = refreshRecord(profile.activeRuntimeSettingsJson);
      const nested = refreshRecord(settings.profile);
      const modelConfigId =
        refreshString(settings.modelConfigId) ??
        refreshString(nested.modelConfigId);
      return modelConfigId !== undefined && modelConfigIds.has(modelConfigId);
    })
    .map((profile) => profile.profileId)
    .sort();
}

export function normalizedCredentialRefreshProfileIds(input: {
  credentialId: string;
  endpoints: readonly Pick<
    NativeModelEndpointRecord,
    "endpointId" | "credentialId"
  >[];
  configurations: readonly Pick<
    NativeModelConfigurationRecord,
    "modelConfigId" | "endpointId"
  >[];
  profiles: readonly {
    profileId: string;
    activeRuntimeSettingsJson?: unknown;
  }[];
}): string[] {
  const endpointIds = new Set(
    input.endpoints
      .filter((endpoint) => endpoint.credentialId === input.credentialId)
      .map((endpoint) => endpoint.endpointId),
  );
  const modelConfigIds = new Set(
    input.configurations
      .filter((configuration) => endpointIds.has(configuration.endpointId))
      .map((configuration) => configuration.modelConfigId),
  );
  return input.profiles
    .filter((profile) => {
      const settings = refreshRecord(profile.activeRuntimeSettingsJson);
      const nested = refreshRecord(settings.profile);
      const modelConfigId =
        refreshString(settings.modelConfigId) ??
        refreshString(nested.modelConfigId);
      return modelConfigId !== undefined && modelConfigIds.has(modelConfigId);
    })
    .map((profile) => profile.profileId)
    .sort();
}

function refreshRecord(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

function refreshString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

export async function handleModelEndpointAdminRequest(
  request: ModelEndpointAdminRouteRequest,
  context: ModelEndpointAdminRouteContext,
): Promise<AdminRouteResult> {
  const url = new URL(request.url);
  const segments = routeSegments(url.pathname, "/v1/admin/model-endpoints");
  if (segments === undefined) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: MODEL_ENDPOINT_ADMIN_REASON_CODES.endpointNotFound,
      message: "unknown model endpoint admin route",
      retryable: false,
    });
  }
  if (segments.length > 1) {
    return methodNotAllowed(request.requestId);
  }

  const method = request.method.toUpperCase();
  const endpointId = segments[0];
  if (endpointId !== undefined) {
    try {
      validateNormalizedId(endpointId, "model endpoint endpointId");
    } catch (error) {
      return validationFailure(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidEndpoint,
        errorMessage(error, "invalid model endpoint id"),
      );
    }
  }

  if (method === "GET") {
    return endpointGet(request, url, endpointId, context);
  }
  if (method === "POST" && endpointId === undefined) {
    return endpointWrite(request, context);
  }
  if (method === "PATCH" && endpointId !== undefined) {
    const existing = await context.getModelEndpoint(endpointId);
    if (existing === undefined) {
      return notFound(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.endpointNotFound,
        `model endpoint ${endpointId} was not found`,
      );
    }
    return endpointWrite(request, context, endpointId, existing);
  }
  return methodNotAllowed(request.requestId);
}

export async function handleModelConfigurationAdminRequest(
  request: ModelEndpointAdminRouteRequest,
  context: ModelEndpointAdminRouteContext,
): Promise<AdminRouteResult> {
  const url = new URL(request.url);
  const segments = routeSegments(
    url.pathname,
    "/v1/admin/model-configurations",
  );
  if (segments === undefined) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: MODEL_ENDPOINT_ADMIN_REASON_CODES.configurationNotFound,
      message: "unknown model configuration admin route",
      retryable: false,
    });
  }
  if (segments.length > 1) {
    return methodNotAllowed(request.requestId);
  }

  const method = request.method.toUpperCase();
  const modelConfigId = segments[0];
  if (modelConfigId !== undefined) {
    try {
      validateNormalizedId(modelConfigId, "model configuration modelConfigId");
    } catch (error) {
      return validationFailure(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidConfiguration,
        errorMessage(error, "invalid model configuration id"),
      );
    }
  }

  if (method === "GET") {
    return configurationGet(request, url, modelConfigId, context);
  }
  if (method === "POST" && modelConfigId === undefined) {
    return configurationWrite(request, context);
  }
  if (method === "PATCH" && modelConfigId !== undefined) {
    const existing = await context.getModelConfiguration(modelConfigId);
    if (existing === undefined) {
      return notFound(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.configurationNotFound,
        `model configuration ${modelConfigId} was not found`,
      );
    }
    return configurationWrite(request, context, modelConfigId, existing);
  }
  return methodNotAllowed(request.requestId);
}

export async function handleModelRegistryAdminRequest(
  request: ModelEndpointAdminRouteRequest,
  context: ModelEndpointAdminRouteContext,
): Promise<AdminRouteResult> {
  if (
    routeSegments(new URL(request.url).pathname, "/v1/admin/model-endpoints")
  ) {
    return handleModelEndpointAdminRequest(request, context);
  }
  if (
    routeSegments(
      new URL(request.url).pathname,
      "/v1/admin/model-configurations",
    )
  ) {
    return handleModelConfigurationAdminRequest(request, context);
  }
  return failure(404, request.requestId, {
    code: "not_found",
    reason_code: MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidEndpoint,
    message: "unknown normalized model admin route",
    retryable: false,
  });
}

async function endpointGet(
  request: ModelEndpointAdminRouteRequest,
  url: URL,
  endpointId: string | undefined,
  context: ModelEndpointAdminRouteContext,
): Promise<AdminRouteResult> {
  try {
    if (endpointId !== undefined) {
      const endpoint = await context.getModelEndpoint(endpointId);
      if (endpoint === undefined) {
        return notFound(
          request.requestId,
          MODEL_ENDPOINT_ADMIN_REASON_CODES.endpointNotFound,
          `model endpoint ${endpointId} was not found`,
        );
      }
      return successRoute(request.requestId, modelEndpointApiRecord(endpoint));
    }
    const query = endpointQuery(url);
    const items = await context.listModelEndpoints(query);
    return successRoute(request.requestId, {
      items: items.map(modelEndpointApiRecord),
      total: items.length,
      limit: query.limit ?? 100,
      offset: query.offset ?? 0,
    });
  } catch (error) {
    return bridgeFailure(
      request.requestId,
      MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidEndpoint,
      error,
    );
  }
}

async function configurationGet(
  request: ModelEndpointAdminRouteRequest,
  url: URL,
  modelConfigId: string | undefined,
  context: ModelEndpointAdminRouteContext,
): Promise<AdminRouteResult> {
  try {
    if (modelConfigId !== undefined) {
      const configuration = await context.getModelConfiguration(modelConfigId);
      if (configuration === undefined) {
        return notFound(
          request.requestId,
          MODEL_ENDPOINT_ADMIN_REASON_CODES.configurationNotFound,
          `model configuration ${modelConfigId} was not found`,
        );
      }
      return successRoute(
        request.requestId,
        modelConfigurationApiRecord(configuration),
      );
    }
    const query = configurationQuery(url);
    const items = await context.listModelConfigurations(query);
    return successRoute(request.requestId, {
      items: items.map(modelConfigurationApiRecord),
      total: items.length,
      limit: query.limit ?? 100,
      offset: query.offset ?? 0,
    });
  } catch (error) {
    return bridgeFailure(
      request.requestId,
      MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidConfiguration,
      error,
    );
  }
}

async function endpointWrite(
  request: ModelEndpointAdminRouteRequest,
  context: ModelEndpointAdminRouteContext,
  endpointId?: string,
  existing?: NativeModelEndpointRecord,
): Promise<AdminRouteResult> {
  let write: NativeModelEndpointWrite;
  try {
    write = endpointWriteFromBody(
      request.body,
      endpointId,
      existing,
      context.now(),
    );
  } catch (error) {
    return validationFailure(
      request.requestId,
      MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidEndpoint,
      errorMessage(error, "invalid model endpoint write"),
    );
  }

  try {
    const endpoint = await context.upsertModelEndpoint(write);
    const refresh = await context.refreshAfterWrite?.({
      kind: "endpoint",
      id: endpoint.endpointId,
    });
    return successRoute(request.requestId, {
      endpoint: modelEndpointApiRecord(endpoint),
      ...(refresh === undefined ? {} : { refresh }),
    });
  } catch (error) {
    const mismatch = revisionMismatch(
      error,
      "model endpoint",
      write.endpointId,
    );
    if (mismatch !== undefined) {
      let current: NativeModelEndpointRecord | undefined;
      try {
        current = await context.getModelEndpoint(write.endpointId);
      } catch {
        current = undefined;
      }
      return revisionConflict(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.endpointRevisionMismatch,
        `model endpoint ${write.endpointId} revision mismatch: expected ${mismatch.expected}, found ${mismatch.found}`,
        mismatch.expected,
        mismatch.found,
        current === undefined ? undefined : modelEndpointApiRecord(current),
        "endpoint",
      );
    }
    return bridgeFailure(
      request.requestId,
      MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidEndpoint,
      error,
    );
  }
}

async function configurationWrite(
  request: ModelEndpointAdminRouteRequest,
  context: ModelEndpointAdminRouteContext,
  modelConfigId?: string,
  existing?: NativeModelConfigurationRecord,
): Promise<AdminRouteResult> {
  let write: NativeModelConfigurationWrite;
  try {
    write = await configurationWriteFromBody(
      request.body,
      modelConfigId,
      existing,
      context,
    );
  } catch (error) {
    if (error instanceof MissingModelEndpointError) {
      return notFound(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.endpointNotFound,
        error.message,
      );
    }
    return validationFailure(
      request.requestId,
      MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidConfiguration,
      errorMessage(error, "invalid model configuration write"),
    );
  }

  try {
    const configuration = await context.upsertModelConfiguration(write);
    const refresh = await context.refreshAfterWrite?.({
      kind: "configuration",
      id: configuration.modelConfigId,
    });
    return successRoute(request.requestId, {
      configuration: modelConfigurationApiRecord(configuration),
      ...(refresh === undefined ? {} : { refresh }),
    });
  } catch (error) {
    const mismatch = revisionMismatch(
      error,
      "model configuration",
      write.modelConfigId,
    );
    if (mismatch !== undefined) {
      let current: NativeModelConfigurationRecord | undefined;
      try {
        current = await context.getModelConfiguration(write.modelConfigId);
      } catch {
        current = undefined;
      }
      return revisionConflict(
        request.requestId,
        MODEL_ENDPOINT_ADMIN_REASON_CODES.configurationRevisionMismatch,
        `model configuration ${write.modelConfigId} revision mismatch: expected ${mismatch.expected}, found ${mismatch.found}`,
        mismatch.expected,
        mismatch.found,
        current === undefined
          ? undefined
          : modelConfigurationApiRecord(current),
        "configuration",
      );
    }
    return bridgeFailure(
      request.requestId,
      MODEL_ENDPOINT_ADMIN_REASON_CODES.invalidConfiguration,
      error,
    );
  }
}

function endpointWriteFromBody(
  bodyValue: unknown,
  pathEndpointId: string | undefined,
  existing: NativeModelEndpointRecord | undefined,
  now: string,
): NativeModelEndpointWrite {
  const body = requiredRecord(bodyValue, "model endpoint request body");
  rejectLegacyModelFields(body);
  const endpointId =
    pathEndpointId ??
    requiredString(body.endpointId, "model endpoint endpointId");
  if (pathEndpointId !== undefined && body.endpointId !== undefined) {
    const bodyEndpointId = requiredString(
      body.endpointId,
      "model endpoint endpointId",
    );
    if (bodyEndpointId !== pathEndpointId) {
      throw new Error("model endpoint endpointId must match the path id");
    }
  }
  validateNormalizedId(endpointId, "model endpoint endpointId");

  const status = enumValue(
    valueForWrite(body, "status", existing?.status ?? "active"),
    "model endpoint status",
    MODEL_ENDPOINT_STATUS_VALUES,
    isModelEndpointStatusContractValue,
  );
  const protocol = enumValue(
    valueForWrite(body, "protocol", existing?.protocol),
    "model endpoint protocol",
    ["responses", "chat_completions"] as const,
    isModelEndpointProtocolContractValue,
  );
  const wireDialect = enumValue(
    valueForWrite(body, "wireDialect", existing?.wireDialect),
    "model endpoint wireDialect",
    MODEL_ENDPOINT_WIRE_DIALECT_VALUES,
    isModelEndpointWireDialectContractValue,
  );
  const authScheme = enumValue(
    valueForWrite(body, "authScheme", existing?.authScheme ?? "none"),
    "model endpoint authScheme",
    ["none", "bearer_api_key", "openai_codex_oauth"] as const,
    isModelEndpointAuthSchemeContractValue,
  );
  const promptCacheTransport = enumValue(
    valueForWrite(
      body,
      "promptCacheTransport",
      existing?.promptCacheTransport ?? "none",
    ),
    "model endpoint promptCacheTransport",
    MODEL_ENDPOINT_PROMPT_CACHE_TRANSPORT_VALUES,
    isModelEndpointPromptCacheTransportContractValue,
  );
  const displayName = optionalText(
    valueForWrite(body, "displayName", existing?.displayName),
    "model endpoint displayName",
  );
  const description = optionalText(
    valueForWrite(body, "description", existing?.description),
    "model endpoint description",
  );
  const baseUrl = requiredString(
    valueForWrite(body, "baseUrl", existing?.baseUrl),
    "model endpoint baseUrl",
  );
  const credentialId = optionalId(
    valueForWrite(body, "credentialId", existing?.credentialId),
    "model endpoint credentialId",
  );
  const metadataJson = objectValue(
    valueForWrite(body, "metadataJson", existing?.metadataJson ?? {}),
    "model endpoint metadataJson",
  );
  const expectedRevision = optionalRevision(
    valueForWrite(body, "expectedRevision", existing?.revision),
    "model endpoint expectedRevision",
  );

  validateEndpointFields({
    endpointId,
    baseUrl,
    protocol,
    wireDialect,
    authScheme,
    credentialId,
    promptCacheTransport,
  });

  return {
    endpointId,
    status,
    displayName,
    description,
    baseUrl,
    protocol,
    wireDialect,
    authScheme,
    credentialId,
    promptCacheTransport,
    metadataJson,
    expectedRevision,
    now,
  };
}

async function configurationWriteFromBody(
  bodyValue: unknown,
  pathModelConfigId: string | undefined,
  existing: NativeModelConfigurationRecord | undefined,
  context: ModelEndpointAdminRouteContext,
): Promise<NativeModelConfigurationWrite> {
  const body = requiredRecord(bodyValue, "model configuration request body");
  rejectLegacyModelFields(body);
  const modelConfigId =
    pathModelConfigId ??
    requiredString(body.modelConfigId, "model configuration modelConfigId");
  if (pathModelConfigId !== undefined && body.modelConfigId !== undefined) {
    const bodyModelConfigId = requiredString(
      body.modelConfigId,
      "model configuration modelConfigId",
    );
    if (bodyModelConfigId !== pathModelConfigId) {
      throw new Error(
        "model configuration modelConfigId must match the path id",
      );
    }
  }
  validateNormalizedId(modelConfigId, "model configuration modelConfigId");

  const endpointId = requiredString(
    valueForWrite(body, "endpointId", existing?.endpointId),
    "model configuration endpointId",
  );
  validateNormalizedId(endpointId, "model configuration endpointId");
  const endpoint = await context.getModelEndpoint(endpointId);
  if (endpoint === undefined) {
    throw new MissingModelEndpointError(
      `model endpoint ${endpointId} was not found for model configuration ${modelConfigId}`,
    );
  }

  const status = enumValue(
    valueForWrite(body, "status", existing?.status ?? "active"),
    "model configuration status",
    MODEL_ENDPOINT_STATUS_VALUES,
    isModelEndpointStatusContractValue,
  );
  const displayName = optionalText(
    valueForWrite(body, "displayName", existing?.displayName),
    "model configuration displayName",
  );
  const description = optionalText(
    valueForWrite(body, "description", existing?.description),
    "model configuration description",
  );
  const modelId = requiredString(
    valueForWrite(body, "modelId", existing?.modelId),
    "model configuration modelId",
  );
  if (modelId.length > 512) {
    throw new Error("model configuration modelId must be at most 512 bytes");
  }
  const contextWindowTokens = optionalU32(
    valueForWrite(body, "contextWindowTokens", existing?.contextWindowTokens),
    "model configuration contextWindowTokens",
    false,
  );
  const maxOutputTokens = optionalU32(
    valueForWrite(body, "maxOutputTokens", existing?.maxOutputTokens),
    "model configuration maxOutputTokens",
    false,
  );
  const temperatureMilli = optionalU32(
    valueForWrite(body, "temperatureMilli", existing?.temperatureMilli),
    "model configuration temperatureMilli",
    true,
  );
  if (temperatureMilli !== undefined && temperatureMilli > 10_000) {
    throw new Error(
      "model configuration temperatureMilli must be at most 10000",
    );
  }
  const reasoningEffort = optionalText(
    valueForWrite(body, "reasoningEffort", existing?.reasoningEffort),
    "model configuration reasoningEffort",
  );
  const reasoningFormat = optionalText(
    valueForWrite(body, "reasoningFormat", existing?.reasoningFormat),
    "model configuration reasoningFormat",
  );
  const reasoningHistory = enumValue(
    valueForWrite(
      body,
      "reasoningHistory",
      existing?.reasoningHistory ?? "provider_default",
    ),
    "model configuration reasoningHistory",
    MODEL_CONFIGURATION_REASONING_HISTORY_VALUES,
    isModelConfigurationReasoningHistoryContractValue,
  );
  const reasoningBudgetTokens = optionalU32(
    valueForWrite(
      body,
      "reasoningBudgetTokens",
      existing?.reasoningBudgetTokens,
    ),
    "model configuration reasoningBudgetTokens",
    false,
  );
  const thinkingMode = enumValue(
    valueForWrite(
      body,
      "thinkingMode",
      existing?.thinkingMode ?? "provider_default",
    ),
    "model configuration thinkingMode",
    MODEL_CONFIGURATION_THINKING_MODE_VALUES,
    isModelConfigurationThinkingModeContractValue,
  );
  const promptCachingPolicy = enumValue(
    valueForWrite(
      body,
      "promptCachingPolicy",
      existing?.promptCachingPolicy ?? "disabled",
    ),
    "model configuration promptCachingPolicy",
    MODEL_CONFIGURATION_PROMPT_CACHING_POLICY_VALUES,
    isModelConfigurationPromptCachingPolicyContractValue,
  );
  const capabilities = modelCapabilities(
    valueForWrite(body, "capabilities", existing?.capabilities),
  );
  const metadataJson = objectValue(
    valueForWrite(body, "metadataJson", existing?.metadataJson ?? {}),
    "model configuration metadataJson",
  );
  const expectedRevision = optionalRevision(
    valueForWrite(body, "expectedRevision", existing?.revision),
    "model configuration expectedRevision",
  );

  validateConfigurationFields({
    endpoint,
    modelId,
    maxOutputTokens,
    temperatureMilli,
    reasoningHistory,
    reasoningBudgetTokens,
    thinkingMode,
    promptCachingPolicy,
    capabilities,
  });

  return {
    modelConfigId,
    endpointId,
    status,
    displayName,
    description,
    modelId,
    contextWindowTokens,
    maxOutputTokens,
    temperatureMilli,
    reasoningEffort,
    reasoningFormat,
    reasoningHistory,
    reasoningBudgetTokens,
    thinkingMode,
    promptCachingPolicy,
    capabilities,
    metadataJson,
    expectedRevision,
    now: context.now(),
  };
}

function endpointQuery(url: URL): NativeModelEndpointQuery {
  return {
    endpointId: optionalQueryId(
      url.searchParams.get("endpointId"),
      "endpointId",
    ),
    status: queryEnum(
      url.searchParams.get("status"),
      "model endpoint status",
      isModelEndpointStatusContractValue,
    ),
    ...pageQuery(url),
  };
}

function configurationQuery(url: URL): NativeModelConfigurationQuery {
  return {
    modelConfigId: optionalQueryId(
      url.searchParams.get("modelConfigId"),
      "modelConfigId",
    ),
    endpointId: optionalQueryId(
      url.searchParams.get("endpointId"),
      "endpointId",
    ),
    status: queryEnum(
      url.searchParams.get("status"),
      "model configuration status",
      isModelEndpointStatusContractValue,
    ),
    ...pageQuery(url),
  };
}

function pageQuery(url: URL): { limit?: number; offset?: number } {
  return {
    limit: queryNumber(url.searchParams.get("limit"), "limit"),
    offset: queryNumber(url.searchParams.get("offset"), "offset"),
  };
}

export function modelEndpointApiRecord(
  record: NativeModelEndpointRecord,
): NativeModelEndpointRecord {
  return {
    endpointId: record.endpointId,
    status: record.status,
    ...(record.displayName === undefined
      ? {}
      : { displayName: record.displayName }),
    ...(record.description === undefined
      ? {}
      : { description: record.description }),
    baseUrl: record.baseUrl,
    protocol: record.protocol,
    wireDialect: record.wireDialect,
    authScheme: record.authScheme,
    ...(record.credentialId === undefined
      ? {}
      : { credentialId: record.credentialId }),
    promptCacheTransport: record.promptCacheTransport,
    metadataJson: record.metadataJson,
    revision: record.revision,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
  };
}

export function modelConfigurationApiRecord(
  record: NativeModelConfigurationRecord,
): NativeModelConfigurationRecord {
  return {
    modelConfigId: record.modelConfigId,
    endpointId: record.endpointId,
    status: record.status,
    ...(record.displayName === undefined
      ? {}
      : { displayName: record.displayName }),
    ...(record.description === undefined
      ? {}
      : { description: record.description }),
    modelId: record.modelId,
    ...(record.contextWindowTokens === undefined
      ? {}
      : { contextWindowTokens: record.contextWindowTokens }),
    ...(record.maxOutputTokens === undefined
      ? {}
      : { maxOutputTokens: record.maxOutputTokens }),
    ...(record.temperatureMilli === undefined
      ? {}
      : { temperatureMilli: record.temperatureMilli }),
    ...(record.reasoningEffort === undefined
      ? {}
      : { reasoningEffort: record.reasoningEffort }),
    ...(record.reasoningFormat === undefined
      ? {}
      : { reasoningFormat: record.reasoningFormat }),
    reasoningHistory: record.reasoningHistory,
    ...(record.reasoningBudgetTokens === undefined
      ? {}
      : { reasoningBudgetTokens: record.reasoningBudgetTokens }),
    thinkingMode: record.thinkingMode,
    promptCachingPolicy: record.promptCachingPolicy,
    capabilities: record.capabilities,
    metadataJson: record.metadataJson,
    revision: record.revision,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
  };
}

function validateEndpointFields(input: {
  endpointId: string;
  baseUrl: string;
  protocol: ModelEndpointProtocol;
  wireDialect: ModelEndpointWireDialect;
  authScheme: ModelEndpointAuthScheme;
  credentialId?: string;
  promptCacheTransport: ModelEndpointPromptCacheTransport;
}): void {
  if (input.baseUrl.length > 2048 || /\s/u.test(input.baseUrl)) {
    throw new Error(
      "model endpoint baseUrl must be an http(s) URL without whitespace and at most 2048 bytes",
    );
  }
  if (
    !input.baseUrl.startsWith("http://") &&
    !input.baseUrl.startsWith("https://")
  ) {
    throw new Error("model endpoint baseUrl must use http:// or https://");
  }
  const validDialect =
    input.protocol === "responses"
      ? MODEL_ENDPOINT_RESPONSES_WIRE_DIALECT_VALUES.includes(
          input.wireDialect as (typeof MODEL_ENDPOINT_RESPONSES_WIRE_DIALECT_VALUES)[number],
        )
      : MODEL_ENDPOINT_CHAT_COMPLETIONS_WIRE_DIALECT_VALUES.includes(
          input.wireDialect as (typeof MODEL_ENDPOINT_CHAT_COMPLETIONS_WIRE_DIALECT_VALUES)[number],
        );
  if (!validDialect) {
    throw new Error(
      `model endpoint wireDialect ${input.wireDialect} is not valid for protocol ${input.protocol}`,
    );
  }
  if (
    input.authScheme === "openai_codex_oauth" &&
    input.protocol !== "responses"
  ) {
    throw new Error(
      "model endpoint authScheme openai_codex_oauth requires the responses protocol",
    );
  }
  if (input.authScheme === "none" && input.credentialId !== undefined) {
    throw new Error(
      "model endpoint authScheme none cannot reference credentialId",
    );
  }
  if (input.authScheme !== "none" && input.credentialId === undefined) {
    throw new Error("an authenticated model endpoint requires credentialId");
  }
  if (
    input.promptCacheTransport === "openrouter_anthropic" &&
    input.protocol !== "chat_completions"
  ) {
    throw new Error(
      "model endpoint promptCacheTransport openrouter_anthropic requires the chat_completions protocol",
    );
  }
}

function validateConfigurationFields(input: {
  endpoint: NativeModelEndpointRecord;
  modelId: string;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningHistory: ModelConfigurationReasoningHistory;
  reasoningBudgetTokens?: number;
  thinkingMode: ModelConfigurationThinkingMode;
  promptCachingPolicy: ModelConfigurationPromptCachingPolicy;
  capabilities: ModelCapabilities;
}): void {
  if (input.capabilities.version !== MODEL_CAPABILITIES_VERSION) {
    throw new Error(
      `unsupported model configuration capabilities version ${input.capabilities.version}; expected ${MODEL_CAPABILITIES_VERSION}`,
    );
  }
  if (
    input.thinkingMode === "disabled" &&
    input.reasoningHistory !== "provider_default"
  ) {
    throw new Error(
      "disabled thinking cannot configure reasoning history preservation",
    );
  }
  if (
    input.promptCachingPolicy !== "disabled" &&
    (input.endpoint.promptCacheTransport !== "openrouter_anthropic" ||
      !input.modelId.startsWith("anthropic/"))
  ) {
    throw new Error(
      "model configuration prompt caching requires the openrouter_anthropic endpoint transport and an anthropic/ model id",
    );
  }
  if (input.endpoint.protocol === "responses") {
    if (
      input.reasoningHistory !== "provider_default" ||
      input.reasoningBudgetTokens !== undefined ||
      input.thinkingMode !== "provider_default"
    ) {
      throw new Error(
        "chat completions reasoning settings require the chat_completions protocol",
      );
    }
    return;
  }
  if (
    input.endpoint.wireDialect === "standard" &&
    (input.reasoningHistory !== "provider_default" ||
      input.reasoningBudgetTokens !== undefined ||
      input.thinkingMode !== "provider_default")
  ) {
    throw new Error(
      "standard chat completions dialect does not accept vendor thinking settings",
    );
  }
  if (
    input.reasoningHistory === "tool_calls_only" &&
    input.endpoint.wireDialect !== "deepseek"
  ) {
    throw new Error(
      "tool_calls_only reasoning history requires the deepseek chat completions dialect",
    );
  }
  if (input.reasoningBudgetTokens !== undefined) {
    if (input.endpoint.wireDialect !== "qwen") {
      throw new Error(
        "reasoning budget tokens are supported only by the qwen chat completions dialect",
      );
    }
    if (input.thinkingMode !== "enabled") {
      throw new Error(
        "qwen reasoning budget tokens require thinking mode enabled",
      );
    }
  }
  if (
    input.endpoint.wireDialect === "kimi" &&
    input.thinkingMode !== "disabled" &&
    (input.temperatureMilli !== undefined ||
      input.maxOutputTokens === undefined ||
      input.maxOutputTokens < 16_000)
  ) {
    throw new Error(
      "kimi thinking models require no temperature override and maxOutputTokens of at least 16000",
    );
  }
}

function routeSegments(pathname: string, prefix: string): string[] | undefined {
  if (pathname !== prefix && !pathname.startsWith(`${prefix}/`)) {
    return undefined;
  }
  const suffix = pathname.slice(prefix.length).replace(/^\/+/, "");
  if (!suffix) return [];
  try {
    return suffix
      .split("/")
      .filter(Boolean)
      .map((value) => decodeURIComponent(value));
  } catch {
    return undefined;
  }
}

function requiredRecord(
  value: unknown,
  fieldName: string,
): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${fieldName} must be an object`);
  return value;
}

function rejectLegacyModelFields(body: Record<string, unknown>): void {
  const forbidden = [
    "providerKind",
    "provider_kind",
    "secret",
    "apiKey",
    "api_key",
    "credentialSecret",
    "credential_secret",
  ];
  const field = forbidden.find((candidate) => candidate in body);
  if (field !== undefined) {
    throw new Error(`normalized model admin writes do not accept ${field}`);
  }
}

function valueForWrite(
  body: Record<string, unknown>,
  key: string,
  current: unknown,
): unknown {
  return Object.prototype.hasOwnProperty.call(body, key) ? body[key] : current;
}

function requiredString(value: unknown, fieldName: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`${fieldName} is required`);
  }
  return value.trim();
}

function optionalText(value: unknown, fieldName: string): string | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string")
    throw new Error(`${fieldName} must be a string`);
  if (value.length > 512) {
    throw new Error(`${fieldName} must be at most 512 bytes`);
  }
  return value;
}

function optionalId(value: unknown, fieldName: string): string | undefined {
  if (value === undefined || value === null) return undefined;
  const id = requiredString(value, fieldName);
  validateNormalizedId(id, fieldName);
  return id;
}

function validateNormalizedId(value: string, fieldName: string): void {
  if (
    value.length < 1 ||
    value.length > 128 ||
    !/^[a-z0-9._:-]+$/u.test(value)
  ) {
    throw new Error(
      `${fieldName} must use 1-128 lowercase ASCII id characters (a-z, 0-9, ., _, :, -)`,
    );
  }
}

function objectValue(
  value: unknown,
  fieldName: string,
): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${fieldName} must be a JSON object`);
  return value;
}

function enumValue<T extends string>(
  value: unknown,
  fieldName: string,
  values: readonly T[],
  predicate: (value: string) => value is T,
): T {
  if (
    typeof value !== "string" ||
    !predicate(value) ||
    !values.includes(value)
  ) {
    throw new Error(`${fieldName} must be one of: ${values.join(", ")}`);
  }
  return value;
}

function optionalU32(
  value: unknown,
  fieldName: string,
  allowZero: boolean,
): number | undefined {
  if (value === undefined || value === null) return undefined;
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < (allowZero ? 0 : 1) ||
    value > 4_294_967_295
  ) {
    throw new Error(
      `${fieldName} must be an integer between ${allowZero ? 0 : 1} and 4294967295`,
    );
  }
  return value;
}

function optionalRevision(
  value: unknown,
  fieldName: string,
): number | undefined {
  return optionalU32(value, fieldName, false);
}

function optionalQueryId(
  value: string | null,
  fieldName: string,
): string | undefined {
  if (value === null || value.trim() === "") return undefined;
  const id = value.trim();
  validateNormalizedId(id, `model ${fieldName}`);
  return id;
}

function queryEnum<T extends string>(
  value: string | null,
  fieldName: string,
  predicate: (value: string) => value is T,
): T | undefined {
  if (value === null || value.trim() === "") return undefined;
  if (!predicate(value)) {
    throw new Error(`${fieldName} must be a closed normalized enum value`);
  }
  return value;
}

function queryNumber(
  value: string | null,
  fieldName: string,
): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0 || parsed > 4_294_967_295) {
    throw new Error(`${fieldName} must be a non-negative integer`);
  }
  return parsed;
}

function modelCapabilities(value: unknown): ModelCapabilities {
  if (value === undefined || value === null) {
    return { version: MODEL_CAPABILITIES_VERSION, imageInput: false };
  }
  const record = objectValue(value, "model configuration capabilities");
  const version =
    record.version === undefined
      ? MODEL_CAPABILITIES_VERSION
      : optionalU32(
          record.version,
          "model configuration capabilities.version",
          true,
        );
  const imageInput = record.imageInput ?? false;
  if (typeof imageInput !== "boolean") {
    throw new Error(
      "model configuration capabilities.imageInput must be boolean",
    );
  }
  return { version: version ?? MODEL_CAPABILITIES_VERSION, imageInput };
}

function endpointStatusForQuery(
  value: string | null,
): ModelEndpointStatus | undefined {
  return queryEnum(
    value,
    "model endpoint status",
    isModelEndpointStatusContractValue,
  );
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  return fallback;
}

function revisionMismatch(
  error: unknown,
  label: "model endpoint" | "model configuration",
  id: string,
): { expected: number; found: number } | undefined {
  const message = errorMessage(error, "");
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const match = new RegExp(
    `${label} ${escapedId} revision mismatch: expected (\\d+), found (\\d+)`,
    "u",
  ).exec(message);
  if (match === null) return undefined;
  return { expected: Number(match[1]), found: Number(match[2]) };
}

function bridgeFailure(
  requestId: string,
  reasonCode: string,
  error: unknown,
): AdminRouteResult {
  const message = errorMessage(
    error,
    "normalized model registry operation failed",
  );
  return failure(400, requestId, {
    code: "invalid_input",
    reason_code: reasonCode,
    message: message.replace(/^(?:InvalidInput|ActionRejected):\s*/u, ""),
    retryable: false,
  });
}

function validationFailure(
  requestId: string,
  reasonCode: string,
  message: string,
): AdminRouteResult {
  return failure(400, requestId, {
    code: "invalid_input",
    reason_code: reasonCode,
    message,
    retryable: false,
  });
}

function notFound(
  requestId: string,
  reasonCode: string,
  message: string,
): AdminRouteResult {
  return failure(404, requestId, {
    code: "not_found",
    reason_code: reasonCode,
    message,
    retryable: false,
  });
}

function methodNotAllowed(requestId: string): AdminRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: MODEL_ENDPOINT_ADMIN_REASON_CODES.methodNotAllowed,
    message:
      "normalized model routes support GET list/get, POST create, and PATCH update",
    retryable: false,
  });
}

function revisionConflict(
  requestId: string,
  reasonCode: string,
  message: string,
  expectedRevision: number,
  currentRevision: number,
  current:
    | NativeModelEndpointRecord
    | NativeModelConfigurationRecord
    | undefined,
  currentKey: "endpoint" | "configuration",
): AdminRouteResult {
  return {
    status: 409,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error: {
        code: "conflict",
        reason_code: reasonCode,
        message,
        retryable: false,
      },
      data: {
        [currentKey]: current,
        expectedRevision,
        currentRevision,
      },
      meta: { request_id: requestId, schema_version: 1 },
    },
  } as AdminRouteResult;
}

class MissingModelEndpointError extends Error {}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
