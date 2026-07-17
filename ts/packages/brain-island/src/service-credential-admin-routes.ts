import type {
  NativeModelProviderRecord,
  NativeServiceCredentialQuery,
  NativeServiceCredentialRecord,
  NativeServiceCredentialWrite,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { MODEL_PROVIDER_ADMIN_REASON_CODES } from "./model-provider-admin-contract.js";
import {
  handleOpenAiOauthCredentialAdminRequest,
  type OpenAiOauthRouteContext,
} from "./service-openai-oauth-routes.js";
import { failure, successRoute } from "./service-route-results.js";

export interface ServiceCredentialAdminRouteRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId: string;
}

export interface ServiceCredentialAdminRouteContext extends OpenAiOauthRouteContext {
  listServiceCredentials(
    query: NativeServiceCredentialQuery,
  ): Promise<NativeServiceCredentialRecord[]>;
  deleteServiceCredential(input: {
    credentialId: string;
    expectedRevision?: number;
  }): Promise<NativeServiceCredentialRecord>;
  listModelProviders(input: {
    limit?: number;
    offset?: number;
  }): Promise<NativeModelProviderRecord[]>;
}

export async function handleServiceCredentialAdminRequest(
  request: ServiceCredentialAdminRouteRequest,
  context: ServiceCredentialAdminRouteContext,
): Promise<AdminRouteResult> {
  try {
    return await handleServiceCredentialAdminRequestInner(request, context);
  } catch (error) {
    return serviceCredentialErrorRoute(request.requestId, error);
  }
}

async function handleServiceCredentialAdminRequestInner(
  request: ServiceCredentialAdminRouteRequest,
  context: ServiceCredentialAdminRouteContext,
): Promise<AdminRouteResult> {
  const url = new URL(request.url);
  const method = request.method.toUpperCase();
  const segments = url.pathname
    .replace(/^\/v1\/admin\/service-credentials\/?/, "")
    .split("/")
    .filter(Boolean)
    .map((segment) => decodeURIComponent(segment));
  const credentialId = segments[0];

  if (credentialId && segments[1] === "oauth" && segments[2] === "openai") {
    return handleOpenAiOauthCredentialAdminRequest(
      {
        method,
        credentialId,
        action: segments[3] ?? "status",
        body: request.body,
        requestId: request.requestId,
      },
      context,
    );
  }

  if (method === "GET" && !credentialId) {
    const query: NativeServiceCredentialQuery = {
      providerKind: stringParam(url, "providerKind"),
      limit: numberParam(url, "limit"),
      offset: numberParam(url, "offset"),
    };
    const items = await context.listServiceCredentials(query);
    return successRoute(request.requestId, {
      items,
      total: items.length,
      limit: query.limit ?? 100,
      offset: query.offset ?? 0,
    });
  }

  if (method === "POST" && !credentialId) {
    const write = serviceCredentialWriteFromBody(
      request.body,
      undefined,
      undefined,
      context.now(),
    );
    const credential = await context.upsertServiceCredential(write);
    return successRoute(request.requestId, { credential });
  }

  if (!credentialId) {
    return credentialMethodNotAllowed(request.requestId);
  }

  const existing = await context.getServiceCredential(credentialId);
  if (!existing) {
    return credentialNotFound(request.requestId, credentialId);
  }

  if (method === "GET" && segments.length === 1) {
    return successRoute(request.requestId, existing);
  }

  if (method === "PATCH" && segments.length === 1) {
    const write = serviceCredentialWriteFromBody(
      request.body,
      credentialId,
      existing,
      context.now(),
    );
    const credential = await context.upsertServiceCredential(write);
    return successRoute(request.requestId, { credential });
  }

  if (method === "DELETE" && segments.length === 1) {
    const deleted = await context.deleteServiceCredential({
      credentialId,
      expectedRevision:
        numberParam(url, "expectedRevision") ?? existing.revision,
    });
    discardCredentialPendingLogins(context, credentialId);
    return successRoute(request.requestId, {
      deleted: true,
      credential: deleted,
    });
  }

  if (method === "POST" && segments[1] === "clear") {
    const body = optionalRecord(request.body) ?? {};
    const credential = await context.upsertServiceCredential({
      credentialId,
      displayName: existing.displayName,
      providerKind: existing.providerKind,
      credentialKind: existing.credentialKind,
      clearSecret: true,
      expectedRevision:
        optionalNumber(body.expectedRevision ?? body.expected_revision) ??
        existing.revision,
      now: context.now(),
    });
    discardCredentialPendingLogins(context, credentialId);
    return successRoute(request.requestId, { credential });
  }

  if (method === "GET" && segments[1] === "impact") {
    const providers = await context.listModelProviders({ limit: 1_000 });
    const linkedProviders = providers.filter((provider) =>
      existing.linkedProviderAliases.includes(provider.alias),
    );
    return successRoute(request.requestId, {
      credential: existing,
      linkedProviderAliases: existing.linkedProviderAliases,
      linkedProviders,
      canClear: linkedProviders.length === 0,
      canDelete: linkedProviders.length === 0,
    });
  }

  if (
    method === "POST" &&
    segments[1] === "providers" &&
    segments[2] &&
    (segments[3] === "link" || segments[3] === "unlink")
  ) {
    const alias = segments[2];
    const provider = await context.getModelProvider(alias);
    if (!provider) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.notFound,
        message: `model provider ${alias} was not found`,
        retryable: false,
      });
    }
    const body = optionalRecord(request.body) ?? {};
    if (segments[3] === "link") {
      const linked = await context.linkModelProviderCredential({
        providerAlias: alias,
        credentialId,
        expectedProviderRevision:
          optionalNumber(
            body.expectedProviderRevision ?? body.expected_provider_revision,
          ) ?? provider.revision,
        expectedCredentialRevision:
          optionalNumber(
            body.expectedCredentialRevision ??
              body.expected_credential_revision,
          ) ?? existing.revision,
        now: context.now(),
      });
      return successRoute(request.requestId, linked);
    }
    if (provider.credentialId !== credentialId) {
      return failure(409, request.requestId, {
        code: "conflict",
        reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialLinkMismatch,
        message: `model provider ${alias} is not linked to service credential ${credentialId}`,
        retryable: false,
      });
    }
    const unlinked = await context.unlinkModelProviderCredential({
      providerAlias: alias,
      expectedProviderRevision:
        optionalNumber(
          body.expectedProviderRevision ?? body.expected_provider_revision,
        ) ?? provider.revision,
      now: context.now(),
    });
    return successRoute(request.requestId, {
      provider: unlinked,
      credential: await context.getServiceCredential(credentialId),
    });
  }

  return credentialMethodNotAllowed(request.requestId);
}

function serviceCredentialWriteFromBody(
  bodyValue: unknown,
  pathCredentialId: string | undefined,
  existing: NativeServiceCredentialRecord | undefined,
  now: string,
): NativeServiceCredentialWrite {
  const body = requiredRecord(bodyValue, "service credential request body");
  const credentialId =
    pathCredentialId ??
    requiredString(
      body.credentialId ?? body.credential_id,
      "service credential credentialId",
    );
  return {
    credentialId,
    displayName:
      optionalString(body.displayName ?? body.display_name) ??
      existing?.displayName ??
      credentialId,
    providerKind:
      optionalString(body.providerKind ?? body.provider_kind) ??
      existing?.providerKind ??
      "custom",
    credentialKind:
      credentialKind(body.credentialKind ?? body.credential_kind) ??
      existing?.credentialKind ??
      "api_key",
    secret: serviceCredentialSecretFromBody(body),
    clearSecret: optionalBoolean(body.clearSecret ?? body.clear_secret),
    expectedRevision:
      optionalNumber(body.expectedRevision ?? body.expected_revision) ??
      existing?.revision,
    now,
  };
}

function serviceCredentialSecretFromBody(
  body: Record<string, unknown>,
): string | undefined {
  const envelope = body.credentialSecret ?? body.credential_secret;
  if (envelope !== undefined) {
    return JSON.stringify(serviceCredentialSecretEnvelope(envelope));
  }
  const value = optionalString(body.secret ?? body.apiKey ?? body.api_key);
  if (value === undefined) return undefined;
  return JSON.stringify({ kind: "api_key", version: 1, value });
}

function serviceCredentialSecretEnvelope(
  value: unknown,
): Record<string, unknown> {
  const envelope = requiredRecord(value, "service credential credentialSecret");
  const kind = requiredString(
    envelope.kind,
    "service credential credentialSecret.kind",
  );
  const version = optionalNumber(envelope.version) ?? 1;
  if (version !== 1) {
    throw new Error("service credential credentialSecret.version must be 1");
  }
  if (kind === "api_key") {
    return {
      kind,
      version,
      value: requiredString(
        envelope.value ?? envelope.apiKey ?? envelope.api_key,
        "service credential credentialSecret.value",
      ),
    };
  }
  if (kind === "openai_oauth") {
    return {
      kind,
      version,
      issuer: requiredString(
        envelope.issuer,
        "service credential credentialSecret.issuer",
      ),
      client_id: requiredString(
        envelope.clientId ?? envelope.client_id,
        "service credential credentialSecret.clientId",
      ),
      id_token: requiredString(
        envelope.idToken ?? envelope.id_token,
        "service credential credentialSecret.idToken",
      ),
      access_token: requiredString(
        envelope.accessToken ?? envelope.access_token,
        "service credential credentialSecret.accessToken",
      ),
      refresh_token: requiredString(
        envelope.refreshToken ?? envelope.refresh_token,
        "service credential credentialSecret.refreshToken",
      ),
      exchanged_api_token: optionalString(
        envelope.exchangedApiToken ?? envelope.exchanged_api_token,
      ),
      last_refresh_at: optionalString(
        envelope.lastRefreshAt ?? envelope.last_refresh_at,
      ),
      account_id: optionalString(envelope.accountId ?? envelope.account_id),
      email: optionalString(envelope.email),
      plan_type: optionalString(envelope.planType ?? envelope.plan_type),
      is_fedramp_account:
        optionalBoolean(
          envelope.isFedrampAccount ?? envelope.is_fedramp_account,
        ) ?? false,
      access_token_expires_at: optionalString(
        envelope.accessTokenExpiresAt ?? envelope.access_token_expires_at,
      ),
    };
  }
  throw new Error(
    "service credential credentialSecret.kind must be api_key or openai_oauth",
  );
}

function credentialKind(value: unknown) {
  const kind = optionalString(value);
  if (kind === undefined) return undefined;
  if (kind === "api_key" || kind === "openai_oauth") return kind;
  throw new Error(
    "service credential credentialKind must be api_key or openai_oauth",
  );
}

function serviceCredentialErrorRoute(
  requestId: string,
  error: unknown,
): AdminRouteResult {
  const message = error instanceof Error ? error.message : String(error);
  const revision =
    /service credential ([^ ]+) revision mismatch: expected (\d+), found (\d+)/.exec(
      message,
    );
  if (revision) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialRevisionMismatch,
      message,
      retryable: false,
    });
  }
  if (
    /cannot (?:clear|delete) service credential .* while linked/u.test(message)
  ) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialLinked,
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
  if (/changed concurrently/u.test(message)) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialRevisionMismatch,
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

function credentialNotFound(
  requestId: string,
  credentialId: string,
): AdminRouteResult {
  return failure(404, requestId, {
    code: "not_found",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialNotFound,
    message: `service credential ${credentialId} was not found`,
    retryable: false,
  });
}

function credentialMethodNotAllowed(requestId: string): AdminRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: MODEL_PROVIDER_ADMIN_REASON_CODES.credentialMethodNotAllowed,
    message: "service credential route method or action is not supported",
    retryable: false,
  });
}

function discardCredentialPendingLogins(
  context: ServiceCredentialAdminRouteContext,
  credentialId: string,
): void {
  for (const [id, pending] of context.pendingLogins) {
    if (pending.credentialId === credentialId) context.pendingLogins.delete(id);
  }
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

function stringParam(url: URL, key: string): string | undefined {
  return optionalString(url.searchParams.get(key));
}

function numberParam(url: URL, key: string): number | undefined {
  const value = stringParam(url, key);
  if (!value) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}
