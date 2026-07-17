import { createHash, randomBytes } from "node:crypto";
import type {
  NativeModelProviderRecord,
  NativeOpenAiOauthCodeExchangeInput,
  NativeOpenAiOauthCodeExchangeResult,
  NativeServiceCredentialRecord,
  NativeServiceCredentialWrite,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { MODEL_PROVIDER_ADMIN_REASON_CODES } from "./model-provider-admin-contract.js";
import type { RustyCrewOpenAiOauthConfig } from "./service-config.js";
import { failure, successRoute } from "./service-route-results.js";

export interface OpenAiOauthPendingLogin {
  pendingLoginId: string;
  credentialId: string;
  providerAlias?: string;
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

export interface OpenAiOauthRouteContext {
  getModelProvider(
    alias: string,
  ): Promise<NativeModelProviderRecord | undefined>;
  getServiceCredential(
    credentialId: string,
  ): Promise<NativeServiceCredentialRecord | undefined>;
  upsertServiceCredential(
    write: NativeServiceCredentialWrite,
  ): Promise<NativeServiceCredentialRecord>;
  linkModelProviderCredential(input: {
    providerAlias: string;
    credentialId: string;
    expectedProviderRevision?: number;
    expectedCredentialRevision?: number;
    now: string;
  }): Promise<{
    provider: NativeModelProviderRecord;
    credential: NativeServiceCredentialRecord;
  }>;
  unlinkModelProviderCredential(input: {
    providerAlias: string;
    expectedProviderRevision?: number;
    now: string;
  }): Promise<NativeModelProviderRecord>;
  exchangeOpenAiOauthCode(
    input: NativeOpenAiOauthCodeExchangeInput,
  ): Promise<NativeOpenAiOauthCodeExchangeResult>;
  openAiOauth: RustyCrewOpenAiOauthConfig;
  pendingLogins: Map<string, OpenAiOauthPendingLogin>;
  now(): string;
}

interface OpenAiOauthActionRequest {
  method: string;
  action: string;
  body?: unknown;
  requestId: string;
}

export async function handleOpenAiOauthProviderAdminRequest(
  request: OpenAiOauthActionRequest & { alias: string },
  context: OpenAiOauthRouteContext,
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
  if (provider.providerKind !== "openai" || provider.protocol !== "responses") {
    return failure(409, request.requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthIncompatibleTarget,
      message: "OpenAI OAuth requires an openai Responses provider alias",
      retryable: false,
    });
  }

  const credentialId = provider.credentialId ?? `provider:${provider.alias}`;
  const credential = await context.getServiceCredential(credentialId);

  if (request.action === "clear" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    const updated = await context.unlinkModelProviderCredential({
      providerAlias: provider.alias,
      expectedProviderRevision:
        optionalNumber(body.expectedRevision ?? body.expected_revision) ??
        provider.revision,
      now: context.now(),
    });
    discardPendingLogins(
      context,
      (pending) => pending.providerAlias === provider.alias,
    );
    return successRoute(request.requestId, {
      provider: updated,
      credential: updated.credential,
      serviceCredential: credential,
      compatibilityFacade: true,
      action: "unlinked",
    });
  }

  const result = await handleOpenAiOauthCredentialAction(request, context, {
    credentialId,
    credential,
    providerAlias: provider.alias,
    displayName: provider.displayName ?? provider.alias,
  });
  if (result.status !== 200 || request.action !== "complete") {
    return withProviderCompatibilityData(result, provider);
  }

  const body = requiredRecord(
    request.body,
    "OpenAI OAuth complete request body",
  );
  const completedCredential = dataField<NativeServiceCredentialRecord>(
    result,
    "credential",
  );
  const linked = await context.linkModelProviderCredential({
    providerAlias: provider.alias,
    credentialId,
    expectedProviderRevision:
      optionalNumber(
        body.expectedProviderRevision ??
          body.expected_provider_revision ??
          body.expectedRevision ??
          body.expected_revision,
      ) ?? provider.revision,
    expectedCredentialRevision: completedCredential.revision,
    now: context.now(),
  });
  return successRoute(request.requestId, {
    ...dataRecord(result),
    provider: linked.provider,
    credential: linked.provider.credential,
    serviceCredential: linked.credential,
    compatibilityFacade: true,
  });
}

export async function handleOpenAiOauthCredentialAdminRequest(
  request: OpenAiOauthActionRequest & { credentialId: string },
  context: OpenAiOauthRouteContext,
): Promise<AdminRouteResult> {
  const credential = await context.getServiceCredential(request.credentialId);
  if (!credential) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialNotFound,
      message: `service credential ${request.credentialId} was not found`,
      retryable: false,
    });
  }
  if (
    credential.providerKind !== "openai" ||
    credential.credentialKind !== "openai_oauth"
  ) {
    return failure(409, request.requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthIncompatibleTarget,
      message:
        "OpenAI OAuth requires an openai/openai_oauth service credential",
      retryable: false,
    });
  }
  return handleOpenAiOauthCredentialAction(request, context, {
    credentialId: credential.credentialId,
    credential,
    displayName: credential.displayName,
  });
}

async function handleOpenAiOauthCredentialAction(
  request: OpenAiOauthActionRequest,
  context: OpenAiOauthRouteContext,
  target: {
    credentialId: string;
    credential?: NativeServiceCredentialRecord;
    providerAlias?: string;
    displayName: string;
  },
): Promise<AdminRouteResult> {
  if (request.action === "status" && request.method === "GET") {
    return successRoute(request.requestId, {
      credential: target.credential,
      loginConfig: openAiOauthLoginConfig(context.openAiOauth),
      pendingLogins: pendingLoginsForCredential(context, target.credentialId),
    });
  }

  if (request.action === "start" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    const redirectFailure = validateRedirectUri(
      body,
      context,
      request.requestId,
    );
    if (redirectFailure) return redirectFailure;
    const pending = startOpenAiOauthLogin(target, context, body);
    context.pendingLogins.set(pending.pendingLoginId, pending);
    return successRoute(request.requestId, {
      credential: target.credential,
      loginConfig: openAiOauthLoginConfig(context.openAiOauth),
      pendingLogin: redactedOpenAiOauthPendingLogin(pending),
    });
  }

  if (request.action === "complete" && request.method === "POST") {
    const body = requiredRecord(
      request.body,
      "OpenAI OAuth complete request body",
    );
    const pendingResult = resolvePendingLogin(
      body,
      target,
      context,
      request.requestId,
    );
    if ("failure" in pendingResult) return pendingResult.failure;
    const { pending } = pendingResult;
    const exchangeResult = await completeOpenAiOauthExchange(
      body,
      pending,
      context,
      request.requestId,
    );
    if ("failure" in exchangeResult) return exchangeResult.failure;
    const credential = await context.upsertServiceCredential({
      credentialId: target.credentialId,
      displayName: target.credential?.displayName ?? target.displayName,
      providerKind: "openai",
      credentialKind: "openai_oauth",
      secret:
        typeof exchangeResult.credentialSecret === "string"
          ? exchangeResult.credentialSecret
          : JSON.stringify(exchangeResult.credentialSecret),
      clearSecret: false,
      expectedRevision:
        optionalNumber(
          body.expectedCredentialRevision ??
            body.expected_credential_revision ??
            (target.providerAlias === undefined
              ? (body.expectedRevision ?? body.expected_revision)
              : undefined),
        ) ?? target.credential?.revision,
      now: context.now(),
    });
    context.pendingLogins.delete(pending.pendingLoginId);
    return successRoute(request.requestId, {
      credential,
      completionMode: exchangeResult.completionMode,
      oauthSummary: exchangeResult.oauthSummary,
      pendingLoginId: pending.pendingLoginId,
    });
  }

  if (request.action === "clear" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    if (!target.credential) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialNotFound,
        message: `service credential ${target.credentialId} was not found`,
        retryable: false,
      });
    }
    const credential = await context.upsertServiceCredential({
      credentialId: target.credential.credentialId,
      displayName: target.credential.displayName,
      providerKind: target.credential.providerKind,
      credentialKind: target.credential.credentialKind,
      clearSecret: true,
      expectedRevision:
        optionalNumber(body.expectedRevision ?? body.expected_revision) ??
        target.credential.revision,
      now: context.now(),
    });
    discardPendingLogins(
      context,
      (pending) => pending.credentialId === target.credentialId,
    );
    return successRoute(request.requestId, { credential });
  }

  return failure(405, request.requestId, {
    code: "method_not_allowed",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthMethodNotAllowed,
    message:
      "OpenAI OAuth routes support GET status and POST start/complete/clear",
    retryable: false,
  });
}

function validateRedirectUri(
  body: Record<string, unknown>,
  context: OpenAiOauthRouteContext,
  requestId: string,
): AdminRouteResult | undefined {
  const requested = optionalString(body.redirectUri ?? body.redirect_uri);
  if (
    requested !== undefined &&
    requested !== context.openAiOauth.redirectUri &&
    !context.openAiOauth.allowRedirectUriOverride
  ) {
    return failure(400, requestId, {
      code: "invalid_input",
      reason_code:
        MODEL_PROVIDER_ADMIN_REASON_CODES.oauthUnregisteredRedirectUri,
      message:
        "OpenAI OAuth redirectUri override is disabled; use the configured registered redirectUri from status/start response",
      retryable: false,
    });
  }
  return undefined;
}

function resolvePendingLogin(
  body: Record<string, unknown>,
  target: { credentialId: string },
  context: OpenAiOauthRouteContext,
  requestId: string,
): { pending: OpenAiOauthPendingLogin } | { failure: AdminRouteResult } {
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
    return {
      failure: failure(400, requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthInvalidCallbackUrl,
        message:
          "OpenAI OAuth callbackUrl must be a full callback URL or query string containing code and state",
        retryable: false,
      }),
    };
  }
  if (callback?.error !== undefined) {
    return {
      failure: failure(400, requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthCallbackError,
        message: `OpenAI OAuth callback returned error ${callback.error}`,
        retryable: false,
      }),
    };
  }
  const stateValue = requiredString(
    body.state ?? callback?.state,
    "OpenAI OAuth state",
  );
  const pendingLoginId = optionalString(
    body.pendingLoginId ?? body.pending_login_id,
  );
  const pending = pendingLoginId
    ? context.pendingLogins.get(pendingLoginId)
    : [...context.pendingLogins.values()].find(
        (item) =>
          item.credentialId === target.credentialId &&
          item.state === stateValue,
      );
  if (!pending || pending.credentialId !== target.credentialId) {
    return {
      failure: failure(404, requestId, {
        code: "not_found",
        reason_code:
          MODEL_PROVIDER_ADMIN_REASON_CODES.oauthPendingLoginNotFound,
        message: "OpenAI OAuth pending login was not found",
        retryable: false,
      }),
    };
  }
  if (stateValue !== pending.state) {
    return {
      failure: failure(400, requestId, {
        code: "invalid_input",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthStateMismatch,
        message: "OpenAI OAuth callback state did not match the pending login",
        retryable: false,
      }),
    };
  }
  return { pending };
}

async function completeOpenAiOauthExchange(
  body: Record<string, unknown>,
  pending: OpenAiOauthPendingLogin,
  context: OpenAiOauthRouteContext,
  requestId: string,
): Promise<
  | {
      credentialSecret: Record<string, unknown> | string;
      completionMode: "real" | "test";
      oauthSummary?: unknown;
    }
  | { failure: AdminRouteResult }
> {
  const fake = body.fakeTokenResponse ?? body.fake_token_response;
  if (fake !== undefined) {
    if (
      optionalBoolean(body.testMode ?? body.test_mode) !== true &&
      optionalBoolean(body.allowFakeTokenResponse) !== true
    ) {
      return {
        failure: failure(400, requestId, {
          code: "invalid_input",
          reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.oauthTestModeRequired,
          message:
            "OpenAI OAuth fakeTokenResponse completion requires explicit testMode=true",
          retryable: false,
        }),
      };
    }
    return {
      credentialSecret: openAiOauthCredentialSecretFromFakeCompletion(
        pending,
        body,
        context.now(),
      ),
      completionMode: "test",
    };
  }
  const callbackUrl = optionalString(
    body.callbackUrl ??
      body.callback_url ??
      body.authorizationResponseUrl ??
      body.authorization_response_url,
  );
  const callback = callbackUrl
    ? parseOpenAiOauthCallbackUrl(callbackUrl)
    : undefined;
  const exchange = await context.exchangeOpenAiOauthCode({
    issuer: pending.issuer,
    clientId: pending.clientId,
    redirectUri: pending.redirectUri,
    code: requiredString(body.code ?? callback?.code, "OpenAI OAuth code"),
    codeVerifier: pending.codeVerifier,
    now: context.now(),
  });
  if (!exchange.ok) {
    return {
      failure: failure(exchange.error.retryable ? 502 : 400, requestId, {
        code: exchange.error.retryable ? "internal_error" : "invalid_input",
        reason_code: exchange.error.reasonCode,
        message: exchange.error.message,
        retryable: exchange.error.retryable,
      }),
    };
  }
  return {
    credentialSecret: exchange.secret,
    completionMode: "real",
    oauthSummary: exchange.summary,
  };
}

function startOpenAiOauthLogin(
  target: { credentialId: string; providerAlias?: string },
  context: OpenAiOauthRouteContext,
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
  const state = randomBase64Url(32);
  const codeVerifier = randomBase64Url(64);
  const codeChallenge = base64Url(
    createHash("sha256").update(codeVerifier).digest(),
  );
  const createdAt = context.now();
  const pendingLoginId = `openai-oauth:${target.credentialId}:${randomBase64Url(18)}`;
  return {
    pendingLoginId,
    credentialId: target.credentialId,
    providerAlias: target.providerAlias,
    issuer,
    clientId,
    redirectUri,
    scopes,
    state,
    codeVerifier,
    codeChallenge,
    authorizationUrl: openAiOauthAuthorizationUrl({
      issuer,
      clientId,
      redirectUri,
      scopes,
      state,
      codeChallenge,
      allowedWorkspaceIds: optionalStringArray(
        body.allowedWorkspaceIds ?? body.allowed_workspace_ids,
        [],
        "OpenAI OAuth allowedWorkspaceIds",
      ),
      originator:
        optionalString(body.originator) ?? context.openAiOauth.originator,
    }),
    createdAt,
    expiresAt: addMilliseconds(createdAt, 10 * 60 * 1000),
  };
}

export function openAiOauthLoginConfig(config: RustyCrewOpenAiOauthConfig) {
  return {
    issuer: config.issuer,
    clientId: config.clientId,
    redirectUri: config.redirectUri,
    redirectUriOverrideAllowed: config.allowRedirectUriOverride,
    redirectUriMode: config.allowRedirectUriOverride
      ? ("operator_configured" as const)
      : ("fixed_registered" as const),
    callbackUrlCompletionAccepted: true,
    callbackUrlCompletionField: "callbackUrl" as const,
    pendingLoginIdRequiredForCallbackUrl: false,
    remoteOperatorFlow: "paste_callback_url" as const,
  };
}

export function redactedOpenAiOauthPendingLogin(
  pending: OpenAiOauthPendingLogin,
) {
  return {
    pendingLoginId: pending.pendingLoginId,
    credentialId: pending.credentialId,
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

function pendingLoginsForCredential(
  context: OpenAiOauthRouteContext,
  credentialId: string,
) {
  return [...context.pendingLogins.values()]
    .filter((pending) => pending.credentialId === credentialId)
    .map(redactedOpenAiOauthPendingLogin);
}

function discardPendingLogins(
  context: OpenAiOauthRouteContext,
  predicate: (pending: OpenAiOauthPendingLogin) => boolean,
): void {
  for (const [id, pending] of context.pendingLogins) {
    if (predicate(pending)) context.pendingLogins.delete(id);
  }
}

function withProviderCompatibilityData(
  result: AdminRouteResult,
  provider: NativeModelProviderRecord,
): AdminRouteResult {
  if (result.status !== 200) return result;
  return successRoute(provider.alias ? requestIdFromResult(result) : "", {
    provider,
    ...dataRecord(result),
    credential: provider.credential,
    serviceCredential: dataRecord(result).credential,
    compatibilityFacade: true,
  });
}

function requestIdFromResult(result: AdminRouteResult): string {
  const body = result.body as { meta?: { request_id?: string } };
  return body.meta?.request_id ?? "";
}

function dataRecord(result: AdminRouteResult): Record<string, unknown> {
  const body = result.body as { data?: unknown };
  return typeof body.data === "object" && body.data !== null
    ? (body.data as Record<string, unknown>)
    : {};
}

function dataField<T>(result: AdminRouteResult, field: string): T {
  return dataRecord(result)[field] as T;
}

function parseOpenAiOauthCallbackUrl(value: string) {
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

function optionalStringArray(
  value: unknown,
  fallback: string[],
  fieldName: string,
): string[] {
  if (value === undefined) return fallback;
  if (!Array.isArray(value)) throw new Error(`${fieldName} must be an array`);
  return value.map((item, index) =>
    requiredString(item, `${fieldName}[${index}]`),
  );
}

function requiredRecord(
  value: unknown,
  fieldName: string,
): Record<string, unknown> {
  const record = optionalRecord(value);
  if (!record) throw new Error(`${fieldName} must be an object`);
  return record;
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function requiredString(value: unknown, fieldName: string): string {
  const text = optionalString(value);
  if (!text) throw new Error(`${fieldName} is required`);
  return text;
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

function randomBase64Url(bytes: number): string {
  return base64Url(randomBytes(bytes));
}

function base64Url(value: Buffer): string {
  return value
    .toString("base64")
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");
}

function addMilliseconds(value: string, milliseconds: number): string {
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) throw new Error("OpenAI OAuth now is invalid");
  return new Date(parsed + milliseconds).toISOString();
}
