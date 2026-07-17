import { createHash, randomBytes } from "node:crypto";
import type {
  NativeModelProviderProtocol,
  NativeModelProviderQuery,
  NativeModelProviderRecord,
  NativeModelProviderStatus,
  NativeModelProviderWrite,
  NativeOpenAiOauthCodeExchangeInput,
  NativeOpenAiOauthCodeExchangeResult,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import {
  MODEL_PROVIDER_ADMIN_REASON_CODES,
  MODEL_PROVIDER_TEMPERATURE_MILLI_SCALE,
  isModelProviderProtocolContractValue,
  isModelProviderRefreshModeContractValue,
  isModelProviderStatusContractValue,
} from "./model-provider-admin-contract.js";
import type { RustyCrewOpenAiOauthConfig } from "./service-config.js";
import { failure, successRoute } from "./service-route-results.js";

export interface OpenAiOauthPendingLogin {
  pendingLoginId: string;
  providerAlias: string;
  issuer: string;
  clientId: string;
  redirectUri: string;
  scopes: string[];
  state: string;
  codeVerifier: string;
  codeChallenge: string;
  authorizationUrl: string;
  createdAt: string;
  expiresAt: string;
}

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

export interface ModelProviderAdminRouteContext {
  listModelProviders(
    query: NativeModelProviderQuery,
  ): Promise<NativeModelProviderRecord[]>;
  getModelProvider(
    alias: string,
  ): Promise<NativeModelProviderRecord | undefined>;
  upsertModelProvider(
    write: NativeModelProviderWrite,
  ): Promise<NativeModelProviderRecord>;
  exchangeOpenAiOauthCode(
    input: NativeOpenAiOauthCodeExchangeInput,
  ): Promise<NativeOpenAiOauthCodeExchangeResult>;
  openAiOauth: RustyCrewOpenAiOauthConfig;
  pendingLogins: Map<string, OpenAiOauthPendingLogin>;
  now(): string;
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
    return handleOpenAiOauthProviderAdminRequest(
      {
        method,
        alias,
        action: segments[3] ?? "status",
        body: request.body,
        requestId: request.requestId,
      },
      context,
    );
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
    const write = modelProviderWriteFromBody(
      request.body,
      alias || undefined,
      context.now(),
    );
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
      throw error;
    }
    const refresh = await context.refreshAfterWrite({
      requestId: request.requestId,
      provider,
      refreshMode: modelProviderRefreshMode(url, request.body),
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

async function handleOpenAiOauthProviderAdminRequest(
  request: {
    method: string;
    alias: string;
    action: string;
    body?: unknown;
    requestId: string;
  },
  context: ModelProviderAdminRouteContext,
): Promise<AdminRouteResult> {
  const provider = await context.getModelProvider(request.alias);
  if (!provider) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.notFound,
      message: `model provider ${request.alias} was not found`,
      retryable: false,
    });
  }

  if (request.action === "status" && request.method === "GET") {
    return successRoute(
      request.requestId,
      openAiOauthProviderStatus(provider, context),
    );
  }

  if (request.action === "start" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    const requestedRedirectUri = optionalString(
      body.redirectUri ?? body.redirect_uri,
    );
    if (
      requestedRedirectUri !== undefined &&
      requestedRedirectUri !== context.openAiOauth.redirectUri &&
      !context.openAiOauth.allowRedirectUriOverride
    ) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code:
          MODEL_PROVIDER_ADMIN_REASON_CODES.oauthUnregisteredRedirectUri,
        message:
          "OpenAI OAuth redirectUri override is disabled; use the configured registered redirectUri from status/start response",
        retryable: false,
      });
    }
    const pending = startOpenAiOauthLogin(provider, context, body);
    context.pendingLogins.set(pending.pendingLoginId, pending);
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(provider),
      loginConfig: openAiOauthLoginConfig(context.openAiOauth),
      pendingLogin: redactedOpenAiOauthPendingLogin(pending),
    });
  }

  if (request.action === "complete" && request.method === "POST") {
    const body = requiredRecord(
      request.body,
      "OpenAI OAuth complete request body",
    );
    const callbackUrl = optionalString(
      body.callbackUrl ??
        body.callback_url ??
        body.authorizationResponseUrl ??
        body.authorization_response_url,
    );
    let callback: ReturnType<typeof parseOpenAiOauthCallbackUrl> | undefined;
    try {
      callback = callbackUrl
        ? parseOpenAiOauthCallbackUrl(callbackUrl)
        : undefined;
    } catch {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthInvalidCallbackUrl,
        message:
          "OpenAI OAuth callbackUrl must be a full callback URL or query string containing code and state",
        retryable: false,
      });
    }
    if (callback?.error !== undefined) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthCallbackError,
        message: `OpenAI OAuth callback returned error ${callback.error}`,
        retryable: false,
      });
    }
    const stateValue = requiredString(
      body.state ?? callback?.state,
      "OpenAI OAuth state",
    );
    const pendingLoginId = optionalString(
      body.pendingLoginId ?? body.pending_login_id,
    );
    const pending =
      pendingLoginId !== undefined
        ? context.pendingLogins.get(pendingLoginId)
        : findOpenAiOauthPendingLoginByState(
            provider.alias,
            stateValue,
            context,
          );
    if (!pending || pending.providerAlias !== provider.alias) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code:
          MODEL_PROVIDER_ADMIN_REASON_CODES.oauthPendingLoginNotFound,
        message: "OpenAI OAuth pending login was not found",
        retryable: false,
      });
    }
    if (stateValue !== pending.state) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthStateMismatch,
        message: "OpenAI OAuth callback state did not match the pending login",
        retryable: false,
      });
    }
    const fakeTokenResponse =
      body.fakeTokenResponse ?? body.fake_token_response;
    let completionMode: "real" | "test";
    let credentialSecret: Record<string, unknown> | string;
    let oauthSummary: unknown;
    if (fakeTokenResponse !== undefined) {
      if (
        optionalBoolean(body.testMode ?? body.test_mode) !== true &&
        optionalBoolean(body.allowFakeTokenResponse) !== true
      ) {
        return failure(400, request.requestId, {
          code: "invalid_input",
          reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthTestModeRequired,
          message:
            "OpenAI OAuth fakeTokenResponse completion requires explicit testMode=true",
          retryable: false,
        });
      }
      completionMode = "test";
      credentialSecret = openAiOauthCredentialSecretFromFakeCompletion(
        pending,
        body,
        context.now(),
      );
    } else {
      completionMode = "real";
      const code = requiredString(
        body.code ?? callback?.code,
        "OpenAI OAuth code",
      );
      const exchange = await context.exchangeOpenAiOauthCode({
        issuer: pending.issuer,
        clientId: pending.clientId,
        redirectUri: pending.redirectUri,
        code,
        codeVerifier: pending.codeVerifier,
        now: context.now(),
      });
      if (!exchange.ok) {
        return failure(
          exchange.error.retryable ? 502 : 400,
          request.requestId,
          {
            code: exchange.error.retryable ? "internal_error" : "invalid_input",
            reason_code: exchange.error.reasonCode,
            message: exchange.error.message,
            retryable: exchange.error.retryable,
          },
        );
      }
      credentialSecret = exchange.secret;
      oauthSummary = exchange.summary;
    }
    const updated = await upsertModelProviderCredentialSecret({
      context,
      provider,
      credentialSecret,
      expectedRevision: optionalNumber(
        body.expectedRevision ?? body.expected_revision,
      ),
      now: context.now(),
    });
    context.pendingLogins.delete(pending.pendingLoginId);
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(updated),
      credential: updated.credential,
      completionMode,
      oauthSummary,
      pendingLoginId: pending.pendingLoginId,
    });
  }

  if (request.action === "clear" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    const updated = await clearModelProviderCredential({
      context,
      provider,
      expectedRevision: optionalNumber(
        body.expectedRevision ?? body.expected_revision,
      ),
      now: context.now(),
    });
    for (const [pendingLoginId, pending] of context.pendingLogins) {
      if (pending.providerAlias === provider.alias) {
        context.pendingLogins.delete(pendingLoginId);
      }
    }
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(updated),
      credential: updated.credential,
    });
  }

  return failure(405, request.requestId, {
    code: "method_not_allowed",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthMethodNotAllowed,
    message:
      "OpenAI OAuth provider routes support GET status and POST start/complete/clear",
    retryable: false,
  });
}

function openAiOauthProviderStatus(
  provider: NativeModelProviderRecord,
  context: ModelProviderAdminRouteContext,
): {
  provider: NativeModelProviderRecord & { temperature?: number };
  credential: NativeModelProviderRecord["credential"];
  pendingLogins: Array<ReturnType<typeof redactedOpenAiOauthPendingLogin>>;
  loginConfig: ReturnType<typeof openAiOauthLoginConfig>;
} {
  return {
    provider: modelProviderApiRecord(provider),
    credential: provider.credential,
    loginConfig: openAiOauthLoginConfig(context.openAiOauth),
    pendingLogins: [...context.pendingLogins.values()]
      .filter((pending) => pending.providerAlias === provider.alias)
      .map(redactedOpenAiOauthPendingLogin),
  };
}

function startOpenAiOauthLogin(
  provider: NativeModelProviderRecord,
  context: ModelProviderAdminRouteContext,
  body: Record<string, unknown>,
): OpenAiOauthPendingLogin {
  const issuer = optionalString(body.issuer) ?? context.openAiOauth.issuer;
  const clientId =
    optionalString(body.clientId ?? body.client_id) ??
    context.openAiOauth.clientId;
  const redirectUri =
    optionalString(body.redirectUri ?? body.redirect_uri) ??
    context.openAiOauth.redirectUri;
  const scopes = optionalStringArray(
    body.scopes,
    [
      "openid",
      "profile",
      "email",
      "offline_access",
      "api.connectors.read",
      "api.connectors.invoke",
    ],
    "OpenAI OAuth scopes",
  );
  const stateValue = randomBase64Url(32);
  const codeVerifier = randomBase64Url(64);
  const codeChallenge = base64Url(
    createHash("sha256").update(codeVerifier).digest(),
  );
  const createdAt = context.now();
  const expiresAt = addMilliseconds(createdAt, 10 * 60 * 1000);
  const pendingLoginId = `openai-oauth:${provider.alias}:${randomBase64Url(18)}`;
  const authorizationUrl = openAiOauthAuthorizationUrl({
    issuer,
    clientId,
    redirectUri,
    scopes,
    state: stateValue,
    codeChallenge,
    allowedWorkspaceIds: optionalStringArray(
      body.allowedWorkspaceIds ?? body.allowed_workspace_ids,
      [],
      "OpenAI OAuth allowedWorkspaceIds",
    ),
    originator:
      optionalString(body.originator) ?? context.openAiOauth.originator,
  });
  return {
    pendingLoginId,
    providerAlias: provider.alias,
    issuer,
    clientId,
    redirectUri,
    scopes,
    state: stateValue,
    codeVerifier,
    codeChallenge,
    authorizationUrl,
    createdAt,
    expiresAt,
  };
}

function openAiOauthLoginConfig(config: RustyCrewOpenAiOauthConfig): {
  issuer: string;
  clientId: string;
  redirectUri: string;
  redirectUriOverrideAllowed: boolean;
  redirectUriMode: "fixed_registered" | "operator_configured";
  callbackUrlCompletionAccepted: boolean;
  callbackUrlCompletionField: "callbackUrl";
  pendingLoginIdRequiredForCallbackUrl: boolean;
  remoteOperatorFlow: "paste_callback_url";
} {
  return {
    issuer: config.issuer,
    clientId: config.clientId,
    redirectUri: config.redirectUri,
    redirectUriOverrideAllowed: config.allowRedirectUriOverride,
    redirectUriMode: config.allowRedirectUriOverride
      ? "operator_configured"
      : "fixed_registered",
    callbackUrlCompletionAccepted: true,
    callbackUrlCompletionField: "callbackUrl",
    pendingLoginIdRequiredForCallbackUrl: false,
    remoteOperatorFlow: "paste_callback_url",
  };
}

function findOpenAiOauthPendingLoginByState(
  providerAlias: string,
  stateValue: string,
  context: ModelProviderAdminRouteContext,
): OpenAiOauthPendingLogin | undefined {
  for (const pending of context.pendingLogins.values()) {
    if (
      pending.providerAlias === providerAlias &&
      pending.state === stateValue
    ) {
      return pending;
    }
  }
  return undefined;
}

function parseOpenAiOauthCallbackUrl(value: string): {
  code?: string;
  state?: string;
  error?: string;
  errorDescription?: string;
} {
  const trimmed = value.trim();
  const url = new URL(
    trimmed.startsWith("?")
      ? `http://localhost:1455/auth/callback${trimmed}`
      : trimmed,
  );
  return {
    code: optionalString(url.searchParams.get("code")),
    state: optionalString(url.searchParams.get("state")),
    error: optionalString(url.searchParams.get("error")),
    errorDescription: optionalString(url.searchParams.get("error_description")),
  };
}

function redactedOpenAiOauthPendingLogin(pending: OpenAiOauthPendingLogin): {
  pendingLoginId: string;
  providerAlias: string;
  issuer: string;
  clientId: string;
  redirectUri: string;
  scopes: string[];
  codeChallenge: string;
  authorizationUrl: string;
  createdAt: string;
  expiresAt: string;
} {
  return {
    pendingLoginId: pending.pendingLoginId,
    providerAlias: pending.providerAlias,
    issuer: pending.issuer,
    clientId: pending.clientId,
    redirectUri: pending.redirectUri,
    scopes: pending.scopes,
    codeChallenge: pending.codeChallenge,
    authorizationUrl: pending.authorizationUrl,
    createdAt: pending.createdAt,
    expiresAt: pending.expiresAt,
  };
}

function openAiOauthAuthorizationUrl(input: {
  issuer: string;
  clientId: string;
  redirectUri: string;
  scopes: string[];
  state: string;
  codeChallenge: string;
  allowedWorkspaceIds: string[];
  originator: string;
}): string {
  const url = new URL("/oauth/authorize", input.issuer);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("client_id", input.clientId);
  url.searchParams.set("redirect_uri", input.redirectUri);
  url.searchParams.set("scope", input.scopes.join(" "));
  url.searchParams.set("code_challenge", input.codeChallenge);
  url.searchParams.set("code_challenge_method", "S256");
  url.searchParams.set("id_token_add_organizations", "true");
  url.searchParams.set("codex_cli_simplified_flow", "true");
  url.searchParams.set("state", input.state);
  url.searchParams.set("originator", input.originator);
  if (input.allowedWorkspaceIds.length > 0) {
    url.searchParams.set(
      "allowed_workspace_id",
      input.allowedWorkspaceIds.join(","),
    );
  }
  return url.toString();
}

function openAiOauthCredentialSecretFromFakeCompletion(
  pending: OpenAiOauthPendingLogin,
  body: Record<string, unknown>,
  now: string,
): Record<string, unknown> {
  const fake = requiredRecord(
    body.fakeTokenResponse ?? body.fake_token_response,
    "OpenAI OAuth fakeTokenResponse",
  );
  return {
    kind: "openai_oauth",
    version: 1,
    issuer: pending.issuer,
    client_id: pending.clientId,
    id_token: requiredString(fake.idToken ?? fake.id_token, "fake idToken"),
    access_token: requiredString(
      fake.accessToken ?? fake.access_token,
      "fake accessToken",
    ),
    refresh_token: requiredString(
      fake.refreshToken ?? fake.refresh_token,
      "fake refreshToken",
    ),
    exchanged_api_token: optionalString(
      fake.exchangedApiToken ?? fake.exchanged_api_token,
    ),
    last_refresh_at:
      optionalString(fake.lastRefreshAt ?? fake.last_refresh_at) ?? now,
    account_id: optionalString(fake.accountId ?? fake.account_id),
    email: optionalString(fake.email),
    plan_type: optionalString(fake.planType ?? fake.plan_type),
    is_fedramp_account:
      optionalBoolean(fake.isFedrampAccount ?? fake.is_fedramp_account) ??
      false,
    access_token_expires_at: optionalString(
      fake.accessTokenExpiresAt ?? fake.access_token_expires_at,
    ),
  };
}

async function upsertModelProviderCredentialSecret(input: {
  context: ModelProviderAdminRouteContext;
  provider: NativeModelProviderRecord;
  credentialSecret: Record<string, unknown> | string;
  expectedRevision: number | undefined;
  now: string;
}): Promise<NativeModelProviderRecord> {
  return input.context.upsertModelProvider({
    ...modelProviderWriteFromRecord(input.provider, input.now),
    secret:
      typeof input.credentialSecret === "string"
        ? input.credentialSecret
        : JSON.stringify(input.credentialSecret),
    expectedRevision: input.expectedRevision ?? input.provider.revision,
    expectedCredentialRevision: input.provider.credential.revision,
  });
}

async function clearModelProviderCredential(input: {
  context: ModelProviderAdminRouteContext;
  provider: NativeModelProviderRecord;
  expectedRevision: number | undefined;
  now: string;
}): Promise<NativeModelProviderRecord> {
  return input.context.upsertModelProvider({
    ...modelProviderWriteFromRecord(input.provider, input.now),
    clearSecret: true,
    expectedRevision: input.expectedRevision ?? input.provider.revision,
    expectedCredentialRevision: input.provider.credential.revision,
  });
}

function modelProviderWriteFromRecord(
  provider: NativeModelProviderRecord,
  now: string,
): NativeModelProviderWrite {
  return {
    alias: provider.alias,
    status: provider.status,
    protocol: provider.protocol,
    providerKind: provider.providerKind,
    displayName: provider.displayName,
    description: provider.description,
    baseUrl: provider.baseUrl,
    modelId: provider.modelId,
    contextWindowTokens: provider.contextWindowTokens,
    maxOutputTokens: provider.maxOutputTokens,
    temperatureMilli: provider.temperatureMilli,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    clearSecret: false,
    expectedCredentialRevision: provider.credential.revision,
    metadataJson: provider.metadataJson,
    now,
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
    secret: modelProviderSecretFromBody(body),
    clearSecret: optionalBoolean(body.clearSecret),
    metadataJson: isRecord(body.metadataJson) ? body.metadataJson : {},
    expectedRevision: optionalNumber(body.expectedRevision),
    now,
  };
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

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`${fieldName} must be an array`);
  }
  return value.map((item, index) => {
    const text = optionalString(item);
    if (text === undefined) {
      throw new Error(`${fieldName}[${index}] must be a non-empty string`);
    }
    return text;
  });
}

function optionalStringArray(
  value: unknown,
  fallback: string[],
  fieldName: string,
): string[] {
  return value === undefined ? fallback : stringArray(value, fieldName);
}

function requiredString(value: unknown, fieldName: string): string {
  const text = optionalString(value);
  if (!text) throw new Error(`${fieldName} is required`);
  return text;
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function requiredRecord(
  value: unknown,
  fieldName: string,
): Record<string, unknown> {
  const record = optionalRecord(value);
  if (record === undefined) {
    throw new Error(`${fieldName} must be an object`);
  }
  return record;
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

function randomBase64Url(byteLength: number): string {
  return base64Url(randomBytes(byteLength));
}

function base64Url(bytes: Buffer): string {
  return bytes
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function addMilliseconds(isoTimestamp: string, milliseconds: number): string {
  const parsed = Date.parse(isoTimestamp);
  const base = Number.isFinite(parsed) ? parsed : Date.now();
  return new Date(base + milliseconds).toISOString();
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  return fallback;
}
