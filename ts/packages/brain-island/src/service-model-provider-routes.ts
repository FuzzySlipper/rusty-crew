import type {
  NativeModelProviderProtocol,
  NativeModelProviderQuery,
  NativeModelProviderRecord,
  NativeModelProviderStatus,
  NativeModelProviderWrite,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import {
  MODEL_PROVIDER_ADMIN_REASON_CODES,
  MODEL_PROVIDER_TEMPERATURE_MILLI_SCALE,
  isModelProviderProtocolContractValue,
  isModelProviderRefreshModeContractValue,
  isModelProviderStatusContractValue,
} from "./model-provider-admin-contract.js";
import {
  handleOpenAiOauthProviderAdminRequest,
  type OpenAiOauthRouteContext,
} from "./service-openai-oauth-routes.js";
import { failure, successRoute } from "./service-route-results.js";

export type ModelProviderRefreshMode = "none" | "plan" | "apply";

export interface ModelProviderWriteRefreshResult {
  refresh: {
    mode: ModelProviderRefreshMode;
    affectedProfiles: Array<{
      profileId: string;
      sessionIds: string[];
      configuredSessionIds: string[];
      activeSessionIds: string[];
    }>;
    outcomes: Array<{
      profileId: string;
      status: "planned" | "applied" | "blocked" | "failed";
      summary: string;
      reasonCode?: string;
      result?: unknown;
    }>;
  };
}

export interface ModelProviderAdminRouteRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId: string;
}

export interface ModelProviderAdminRouteContext extends OpenAiOauthRouteContext {
  listModelProviders(
    query: NativeModelProviderQuery,
  ): Promise<NativeModelProviderRecord[]>;
  getModelProvider(
    alias: string,
  ): Promise<NativeModelProviderRecord | undefined>;
  upsertModelProvider(
    write: NativeModelProviderWrite,
  ): Promise<NativeModelProviderRecord>;
  refreshAfterWrite(input: {
    requestId: string;
    provider: NativeModelProviderRecord;
    refreshMode: ModelProviderRefreshMode;
  }): Promise<ModelProviderWriteRefreshResult>;
}

export async function handleModelProviderAdminRequest(
  request: ModelProviderAdminRouteRequest,
  context: ModelProviderAdminRouteContext,
): Promise<AdminRouteResult> {
  const url = new URL(request.url);
  const method = request.method.toUpperCase();
  const relativePath = url.pathname.replace(
    /^\/v1\/admin\/model-providers\/?/,
    "",
  );
  const segments = relativePath
    .split("/")
    .filter((segment) => segment.length > 0)
    .map((segment) => decodeURIComponent(segment));
  const alias = segments[0] ?? "";

  if (alias && segments[1] === "oauth" && segments[2] === "openai") {
    try {
      return await handleOpenAiOauthProviderAdminRequest(
        {
          method,
          alias,
          action: segments[3] ?? "status",
          body: request.body,
          requestId: request.requestId,
        },
        context,
      );
    } catch (error) {
      return modelProviderCredentialConflictRoute(request.requestId, error);
    }
  }

  if (
    alias &&
    segments[1] === "credential" &&
    (segments[2] === "link" || segments[2] === "unlink") &&
    method === "POST"
  ) {
    const provider = await context.getModelProvider(alias);
    if (!provider) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.notFound,
        message: `model provider ${alias} was not found`,
        retryable: false,
      });
    }
    const body = isRecord(request.body) ? request.body : {};
    try {
      if (segments[2] === "link") {
        const credentialId = optionalString(
          body.credentialId ?? body.credential_id,
        );
        if (credentialId === undefined) {
          return failure(400, request.requestId, {
            code: "invalid_input",
            reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialInvalid,
            message: "service credential credentialId is required",
            retryable: false,
          });
        }
        const result = await context.linkModelProviderCredential({
          providerAlias: alias,
          credentialId,
          expectedProviderRevision:
            optionalNumber(
              body.expectedProviderRevision ?? body.expected_provider_revision,
            ) ?? provider.revision,
          expectedCredentialRevision: optionalNumber(
            body.expectedCredentialRevision ??
              body.expected_credential_revision,
          ),
          now: context.now(),
        });
        return successRoute(request.requestId, result);
      }
      const updated = await context.unlinkModelProviderCredential({
        providerAlias: alias,
        expectedProviderRevision:
          optionalNumber(
            body.expectedProviderRevision ?? body.expected_provider_revision,
          ) ?? provider.revision,
        now: context.now(),
      });
      return successRoute(request.requestId, { provider: updated });
    } catch (error) {
      return modelProviderCredentialConflictRoute(request.requestId, error);
    }
  }

  if (method === "GET" && !alias) {
    const status = modelProviderStatusParam(url.searchParams.get("status"));
    if (status === "invalid") {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.invalidStatus,
        message: "invalid model provider status filter",
        retryable: false,
      });
    }
    const query: NativeModelProviderQuery = {
      status,
      aliasPrefix: stringParam(url, "aliasPrefix"),
      limit: numberParam(url, "limit"),
      offset: numberParam(url, "offset"),
    };
    const items = await context.listModelProviders(query);
    return successRoute(request.requestId, {
      items: items.map(modelProviderApiRecord),
      total: items.length,
      limit: query.limit ?? 100,
      offset: query.offset ?? 0,
    });
  }

  if (method === "GET" && alias) {
    const provider = await context.getModelProvider(alias);
    if (!provider) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.notFound,
        message: `model provider ${alias} was not found`,
        retryable: false,
      });
    }
    return successRoute(request.requestId, modelProviderApiRecord(provider));
  }

  if ((method === "POST" && !alias) || (method === "PATCH" && alias)) {
    let write: NativeModelProviderWrite;
    let refreshMode: ModelProviderRefreshMode;
    try {
      write = modelProviderWriteFromBody(
        request.body,
        alias || undefined,
        context.now(),
      );
      refreshMode = modelProviderRefreshMode(url, request.body);
    } catch (error) {
      return modelProviderValidationFailureRoute(
        request.requestId,
        errorMessage(error, "invalid model provider write"),
      );
    }
    let provider: NativeModelProviderRecord;
    try {
      provider = await context.upsertModelProvider(write);
    } catch (error) {
      const mismatch = modelProviderRevisionMismatch(error);
      if (mismatch !== undefined && mismatch.alias === write.alias) {
        const currentProvider = await context.getModelProvider(write.alias);
        return modelProviderRevisionConflictRoute(
          request.requestId,
          mismatch,
          currentProvider,
        );
      }
      const validationMessage = modelProviderInvalidInputMessage(error);
      if (validationMessage !== undefined) {
        return modelProviderValidationFailureRoute(
          request.requestId,
          validationMessage,
        );
      }
      throw error;
    }
    const refresh = await context.refreshAfterWrite({
      requestId: request.requestId,
      provider,
      refreshMode,
    });
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(provider),
      ...refresh,
    });
  }

  return failure(405, request.requestId, {
    code: "method_not_allowed",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.methodNotAllowed,
    message:
      "model provider routes support GET list/get, POST create/upsert, and PATCH update",
    retryable: false,
  });
}

export function modelProviderApiRecord(
  provider: NativeModelProviderRecord,
): NativeModelProviderRecord & { temperature?: number } {
  if (provider.temperatureMilli === undefined) {
    return provider;
  }
  return {
    ...provider,
    temperature:
      provider.temperatureMilli / MODEL_PROVIDER_TEMPERATURE_MILLI_SCALE,
  };
}

interface ModelProviderRevisionMismatch {
  alias: string;
  expected: number;
  found: number;
}

function modelProviderRevisionMismatch(
  error: unknown,
): ModelProviderRevisionMismatch | undefined {
  const message = errorMessage(error, "");
  const match =
    /model provider ([^ ]+) revision mismatch: expected (\d+), found (\d+)/.exec(
      message,
    );
  if (match === null) {
    return undefined;
  }
  return {
    alias: match[1] ?? "",
    expected: Number(match[2]),
    found: Number(match[3]),
  };
}

function modelProviderRevisionConflictRoute(
  requestIdValue: string,
  mismatch: ModelProviderRevisionMismatch,
  currentProvider: NativeModelProviderRecord | undefined,
): AdminRouteResult {
  return {
    status: 409,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error: {
        code: "conflict",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.revisionMismatch,
        message: `model provider ${mismatch.alias} revision mismatch: expected ${mismatch.expected}, found ${mismatch.found}`,
        retryable: false,
      },
      data: {
        provider:
          currentProvider === undefined
            ? undefined
            : modelProviderApiRecord(currentProvider),
        expectedRevision: mismatch.expected,
        currentRevision: mismatch.found,
      },
      meta: { request_id: requestIdValue, schema_version: 1 },
    } as AdminRouteResult["body"],
  };
}

function modelProviderInvalidInputMessage(error: unknown): string | undefined {
  const message = errorMessage(error, "");
  const match = /^InvalidInput:\s*(.+)$/su.exec(message);
  return match?.[1]?.trim() || undefined;
}

function modelProviderValidationFailureRoute(
  requestId: string,
  message: string,
): AdminRouteResult {
  return failure(400, requestId, {
    code: "invalid_input",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.invalidProvider,
    message,
    retryable: false,
  });
}

function modelProviderCredentialConflictRoute(
  requestId: string,
  error: unknown,
): AdminRouteResult {
  const message = errorMessage(error, "model provider credential write failed");
  const providerMismatch = modelProviderRevisionMismatch(error);
  if (providerMismatch !== undefined) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.revisionMismatch,
      message,
      retryable: false,
    });
  }
  if (/service credential .* revision mismatch:/u.test(message)) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialRevisionMismatch,
      message,
      retryable: false,
    });
  }
  if (/not found/u.test(message)) {
    return failure(404, requestId, {
      code: "not_found",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialNotFound,
      message,
      retryable: false,
    });
  }
  return failure(400, requestId, {
    code: "invalid_input",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialInvalid,
    message,
    retryable: false,
  });
}

function modelProviderRefreshMode(
  url: URL,
  body: unknown,
): ModelProviderRefreshMode {
  const raw =
    url.searchParams.get("refresh") ??
    (isRecord(body) ? optionalString(body.refresh) : undefined) ??
    "none";
  if (isModelProviderRefreshModeContractValue(raw)) return raw;
  throw new Error("model provider refresh must be none, plan, or apply");
}

function modelProviderStatusParam(
  value: string | null,
): NativeModelProviderStatus | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return isModelProviderStatusContractValue(value) ? value : "invalid";
}

function modelProviderProtocolFromBody(
  value: unknown,
): NativeModelProviderProtocol {
  const protocol = optionalString(value) ?? "chat_completions";
  if (isModelProviderProtocolContractValue(protocol)) {
    return protocol;
  }
  throw new Error(
    "model provider protocol must be responses or chat_completions",
  );
}

function modelProviderSecretFromBody(
  body: Record<string, unknown>,
): string | undefined {
  const credentialSecret = body.credentialSecret ?? body.credential_secret;
  if (credentialSecret !== undefined) {
    return JSON.stringify(
      modelProviderCredentialSecretEnvelope(credentialSecret),
    );
  }
  return optionalString(body.secret ?? body.apiKey);
}

function modelProviderCredentialSecretEnvelope(
  value: unknown,
): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error("model provider credentialSecret must be an object");
  }
  const kind = requiredString(
    value.kind,
    "model provider credentialSecret.kind",
  );
  const version = optionalNumber(value.version) ?? 1;
  if (version !== 1) {
    throw new Error("model provider credentialSecret.version must be 1");
  }
  if (kind === "api_key") {
    return {
      kind,
      version,
      value: requiredString(
        value.value ?? value.apiKey ?? value.api_key,
        "model provider credentialSecret.value",
      ),
    };
  }
  if (kind === "openai_oauth") {
    return {
      kind,
      version,
      issuer: requiredString(
        value.issuer,
        "model provider credentialSecret.issuer",
      ),
      client_id: requiredString(
        value.clientId ?? value.client_id,
        "model provider credentialSecret.clientId",
      ),
      id_token: requiredString(
        value.idToken ?? value.id_token,
        "model provider credentialSecret.idToken",
      ),
      access_token: requiredString(
        value.accessToken ?? value.access_token,
        "model provider credentialSecret.accessToken",
      ),
      refresh_token: requiredString(
        value.refreshToken ?? value.refresh_token,
        "model provider credentialSecret.refreshToken",
      ),
      exchanged_api_token: optionalString(
        value.exchangedApiToken ?? value.exchanged_api_token,
      ),
      last_refresh_at: optionalString(
        value.lastRefreshAt ?? value.last_refresh_at,
      ),
      account_id: optionalString(value.accountId ?? value.account_id),
      email: optionalString(value.email),
      plan_type: optionalString(value.planType ?? value.plan_type),
      is_fedramp_account:
        optionalBoolean(value.isFedrampAccount ?? value.is_fedramp_account) ??
        false,
      access_token_expires_at: optionalString(
        value.accessTokenExpiresAt ?? value.access_token_expires_at,
      ),
    };
  }
  throw new Error(
    "model provider credentialSecret.kind must be api_key or openai_oauth",
  );
}

function modelProviderWriteFromBody(
  body: unknown,
  pathAlias: string | undefined,
  now: string,
): NativeModelProviderWrite {
  if (!isRecord(body)) {
    throw new Error("model provider request body must be an object");
  }
  const alias = pathAlias ?? requiredString(body.alias, "model provider alias");
  const status = modelProviderStatusParam(optionalString(body.status) ?? null);
  if (status === "invalid") {
    throw new Error(
      "model provider status must be active, disabled, or archived",
    );
  }
  return {
    alias,
    status: status ?? "active",
    protocol: modelProviderProtocolFromBody(body.protocol),
    providerKind: optionalString(body.providerKind) ?? "custom",
    displayName: optionalString(body.displayName),
    description: optionalString(body.description),
    baseUrl: optionalString(body.baseUrl),
    modelId: requiredString(
      body.modelId ?? body.model,
      "model provider modelId",
    ),
    contextWindowTokens: optionalNumber(body.contextWindowTokens),
    maxOutputTokens: optionalNumber(body.maxOutputTokens),
    temperatureMilli: optionalTemperatureMilli(body),
    reasoningEffort: optionalString(body.reasoningEffort),
    reasoningFormat: optionalString(body.reasoningFormat),
    responsesDialect: responsesProviderDialect(body.responsesDialect),
    chatCompletionsDialect: chatCompletionsDialect(body.chatCompletionsDialect),
    thinkingMode: chatCompletionsThinkingMode(body.thinkingMode),
    reasoningHistory: chatCompletionsReasoningHistory(body.reasoningHistory),
    reasoningBudgetTokens: optionalNumber(body.reasoningBudgetTokens),
    promptCaching: chatCompletionsPromptCaching(body.promptCaching),
    secret: modelProviderSecretFromBody(body),
    clearSecret: optionalBoolean(body.clearSecret),
    expectedCredentialRevision: optionalNumber(
      body.expectedCredentialRevision ?? body.expected_credential_revision,
    ),
    metadataJson: isRecord(body.metadataJson) ? body.metadataJson : {},
    expectedRevision: optionalNumber(body.expectedRevision),
    now,
  };
}

function responsesProviderDialect(
  value: unknown,
): NativeModelProviderWrite["responsesDialect"] {
  const dialect = optionalString(value);
  if (dialect === undefined) return undefined;
  if (
    dialect === "openai_stateful" ||
    dialect === "openai_stateless" ||
    dialect === "generic_stateless" ||
    dialect === "deepseek"
  ) {
    return dialect;
  }
  throw new Error(
    "model provider responsesDialect must be openai_stateful, openai_stateless, generic_stateless, or deepseek",
  );
}

function chatCompletionsDialect(
  value: unknown,
): NonNullable<NativeModelProviderWrite["chatCompletionsDialect"]> {
  const dialect = optionalString(value) ?? "standard";
  if (
    dialect === "standard" ||
    dialect === "kimi" ||
    dialect === "glm" ||
    dialect === "qwen" ||
    dialect === "deepseek"
  ) {
    return dialect;
  }
  throw new Error(
    "model provider chatCompletionsDialect must be standard, kimi, glm, qwen, or deepseek",
  );
}

function chatCompletionsThinkingMode(
  value: unknown,
): NonNullable<NativeModelProviderWrite["thinkingMode"]> {
  const mode = optionalString(value) ?? "provider_default";
  if (
    mode === "provider_default" ||
    mode === "enabled" ||
    mode === "disabled"
  ) {
    return mode;
  }
  throw new Error(
    "model provider thinkingMode must be provider_default, enabled, or disabled",
  );
}

function chatCompletionsReasoningHistory(
  value: unknown,
): NonNullable<NativeModelProviderWrite["reasoningHistory"]> {
  const history = optionalString(value) ?? "provider_default";
  if (
    history === "provider_default" ||
    history === "discard" ||
    history === "preserve_all" ||
    history === "tool_calls_only"
  ) {
    return history;
  }
  throw new Error(
    "model provider reasoningHistory must be provider_default, discard, preserve_all, or tool_calls_only",
  );
}

function chatCompletionsPromptCaching(
  value: unknown,
): NonNullable<NativeModelProviderWrite["promptCaching"]> {
  const policy = optionalString(value) ?? "disabled";
  if (
    policy === "disabled" ||
    policy === "automatic_5m" ||
    policy === "automatic_1h"
  ) {
    return policy;
  }
  throw new Error(
    "model provider promptCaching must be disabled, automatic_5m, or automatic_1h",
  );
}

function stringParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value;
}

function numberParam(url: URL, key: string): number | undefined {
  const value = stringParam(url, key);
  if (value === undefined) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

function requiredString(value: unknown, fieldName: string): string {
  const text = optionalString(value);
  if (!text) throw new Error(`${fieldName} is required`);
  return text;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function optionalBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function optionalTemperatureMilli(
  body: Record<string, unknown>,
): number | undefined {
  const temperatureMilli = optionalNumber(body.temperatureMilli);
  if (temperatureMilli !== undefined) {
    if (Number.isInteger(temperatureMilli)) {
      return temperatureMilli;
    }
    if (temperatureMilli >= 0 && temperatureMilli <= 10) {
      return Math.round(
        temperatureMilli * MODEL_PROVIDER_TEMPERATURE_MILLI_SCALE,
      );
    }
    throw new Error(
      "model provider temperatureMilli must be an integer millivalue; use temperature for decimal temperatures",
    );
  }

  const temperature = optionalNumber(body.temperature);
  if (temperature === undefined) {
    return undefined;
  }
  if (temperature < 0) {
    throw new Error("model provider temperature must be non-negative");
  }
  return Math.round(temperature * MODEL_PROVIDER_TEMPERATURE_MILLI_SCALE);
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  return fallback;
}
