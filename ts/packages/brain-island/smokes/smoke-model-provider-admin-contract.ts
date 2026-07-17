import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type {
  NativeModelProviderQuery,
  NativeModelProviderRecord,
  NativeModelProviderWrite,
  NativeOpenAiOauthCodeExchangeInput,
  NativeOpenAiOauthCodeExchangeResult,
} from "@rusty-crew/native-bridge";
import {
  MODEL_PROVIDER_ADMIN_OPENAPI_PATH,
  MODEL_PROVIDER_ADMIN_PATHS,
  MODEL_PROVIDER_ADMIN_REASON_CODES,
  MODEL_PROVIDER_API_RECORD_REQUIRED_FIELDS,
  MODEL_PROVIDER_CREDENTIAL_KIND_VALUES,
  MODEL_PROVIDER_PROTOCOL_VALUES,
  MODEL_PROVIDER_REFRESH_MODE_VALUES,
  MODEL_PROVIDER_REVISION_CONFLICT_DATA_FIELDS,
  MODEL_PROVIDER_STATUS_VALUES,
  OPENAI_OAUTH_LOGIN_CONFIG_REQUIRED_FIELDS,
  OPENAI_OAUTH_PENDING_LOGIN_PUBLIC_FIELDS,
} from "../src/model-provider-admin-contract.js";
import {
  handleModelProviderAdminRequest,
  type ModelProviderAdminRouteContext,
} from "../src/service-model-provider-routes.js";

const contractPath = resolve(
  process.cwd(),
  "../../../",
  MODEL_PROVIDER_ADMIN_OPENAPI_PATH,
);
const contract = JSON.parse(readFileSync(contractPath, "utf8")) as OpenApiDoc;

assert.equal(contract.openapi, "3.1.0");
assert.equal(contract.info.title, "Rusty Crew Model Provider Admin API");

for (const path of Object.values(MODEL_PROVIDER_ADMIN_PATHS)) {
  assert.ok(contract.paths[path], `missing path ${path}`);
}

assert.deepEqual(schema("ModelProviderStatus").enum, [
  ...MODEL_PROVIDER_STATUS_VALUES,
]);
assert.deepEqual(schema("ModelProviderProtocol").enum, [
  ...MODEL_PROVIDER_PROTOCOL_VALUES,
]);
assert.deepEqual(schema("ModelProviderRefreshMode").enum, [
  ...MODEL_PROVIDER_REFRESH_MODE_VALUES,
]);
assert.deepEqual(schema("ModelProviderCredentialKind").enum, [
  ...MODEL_PROVIDER_CREDENTIAL_KIND_VALUES,
]);
assert.deepEqual(
  schema("ModelProviderReasonCode").enum,
  Object.values(MODEL_PROVIDER_ADMIN_REASON_CODES),
);
assert.deepEqual(
  schema("ModelProviderRecord").required,
  MODEL_PROVIDER_API_RECORD_REQUIRED_FIELDS,
);
assert.ok(schema("ModelProviderRecord").properties?.temperature);
assert.ok(schema("ModelProviderRecord").properties?.temperatureMilli);
assert.ok(schema("ModelProviderWrite").properties?.temperature);
assert.ok(schema("ModelProviderWrite").properties?.temperatureMilli);
assert.deepEqual(
  Object.keys(schema("ModelProviderRevisionConflict").properties ?? {}),
  MODEL_PROVIDER_REVISION_CONFLICT_DATA_FIELDS,
);
assert.deepEqual(
  schema("OpenAiOauthLoginConfig").required,
  OPENAI_OAUTH_LOGIN_CONFIG_REQUIRED_FIELDS,
);
assert.deepEqual(
  schema("OpenAiOauthPendingLogin").required,
  OPENAI_OAUTH_PENDING_LOGIN_PUBLIC_FIELDS,
);
assert.equal(
  schema("OpenAiOauthPendingLogin").properties?.codeVerifier,
  undefined,
);

const context = modelProviderRouteContext([
  modelProviderRecord({
    alias: "deepseek-v3.1-flash",
    temperatureMilli: 500,
    revision: 2,
  }),
  modelProviderRecord({
    alias: "gpt",
    providerKind: "openai",
    protocol: "responses",
    modelId: "gpt-5",
    credential: { hasSecret: true, kind: "openai_oauth" },
  }),
]);

const listed = await handleModelProviderAdminRequest(
  {
    method: "GET",
    url: "http://local/v1/admin/model-providers?status=active&limit=5&offset=0",
    requestId: "req-model-provider-contract",
  },
  context,
);
const listData = okData<{
  items: Array<NativeModelProviderRecord & { temperature?: number }>;
}>(listed);
assert.equal(listData.items[0]?.temperature, 0.5);

const conflict = await handleModelProviderAdminRequest(
  {
    method: "PATCH",
    url: "http://local/v1/admin/model-providers/deepseek-v3.1-flash",
    requestId: "req-model-provider-contract",
    body: {
      modelId: "deepseek/deepseek-chat",
      expectedRevision: 1,
    },
  },
  context,
);
assert.equal(conflict.status, 409);
assert.equal(errorReason(conflict), "model_provider_revision_mismatch");
const conflictData = (
  conflict.body as {
    data: {
      provider?: NativeModelProviderRecord & { temperature?: number };
      expectedRevision: number;
      currentRevision: number;
    };
  }
).data;
assert.equal(conflictData.provider?.temperature, 0.5);
assert.equal(conflictData.expectedRevision, 1);
assert.equal(conflictData.currentRevision, 2);

const status = await handleModelProviderAdminRequest(
  {
    method: "GET",
    url: "http://local/v1/admin/model-providers/gpt/oauth/openai/status",
    requestId: "req-model-provider-contract",
  },
  context,
);
const statusData = okData<{
  provider: NativeModelProviderRecord;
  loginConfig: Record<string, unknown>;
  pendingLogins: unknown[];
}>(status);
assert.equal(statusData.provider.alias, "gpt");
for (const field of OPENAI_OAUTH_LOGIN_CONFIG_REQUIRED_FIELDS) {
  assert.ok(field in statusData.loginConfig, `missing loginConfig.${field}`);
}
assert.deepEqual(statusData.pendingLogins, []);

const start = await handleModelProviderAdminRequest(
  {
    method: "POST",
    url: "http://local/v1/admin/model-providers/gpt/oauth/openai/start",
    requestId: "req-model-provider-contract",
    body: { allowedWorkspaceIds: ["workspace-a"] },
  },
  context,
);
const startData = okData<{
  pendingLogin: Record<string, unknown>;
}>(start);
for (const field of OPENAI_OAUTH_PENDING_LOGIN_PUBLIC_FIELDS) {
  assert.ok(field in startData.pendingLogin, `missing pendingLogin.${field}`);
}
assert.equal("codeVerifier" in startData.pendingLogin, false);

console.log(
  JSON.stringify(
    {
      title: contract.info.title,
      paths: Object.values(MODEL_PROVIDER_ADMIN_PATHS).length,
      reasonCodes: Object.values(MODEL_PROVIDER_ADMIN_REASON_CODES).length,
    },
    null,
    2,
  ),
);

function schema(name: string): JsonSchema {
  const value = contract.components.schemas[name];
  assert.ok(value, `missing schema ${name}`);
  return value;
}

function okData<T>(result: AdminResult): T {
  assert.equal(result.status, 200);
  assert.equal(typeof result.body, "object");
  const body = result.body as { ok: boolean; data: T };
  assert.equal(body.ok, true);
  return body.data;
}

function errorReason(result: AdminResult): string | undefined {
  if (typeof result.body !== "object" || result.body === null) {
    return undefined;
  }
  const body = result.body as { error?: { reason_code?: string } };
  return body.error?.reason_code;
}

function modelProviderRouteContext(
  records: NativeModelProviderRecord[],
): ModelProviderAdminRouteContext {
  const providers = new Map(records.map((record) => [record.alias, record]));
  return {
    async listModelProviders(query: NativeModelProviderQuery) {
      return [...providers.values()]
        .filter((provider) =>
          query.status === undefined ? true : provider.status === query.status,
        )
        .slice(query.offset ?? 0, (query.offset ?? 0) + (query.limit ?? 100));
    },
    async getModelProvider(alias: string) {
      return providers.get(alias);
    },
    async upsertModelProvider(write: NativeModelProviderWrite) {
      const existing = providers.get(write.alias);
      if (
        write.expectedRevision !== undefined &&
        existing !== undefined &&
        write.expectedRevision !== existing.revision
      ) {
        throw new Error(
          `model provider ${write.alias} revision mismatch: expected ${write.expectedRevision}, found ${existing.revision}`,
        );
      }
      const record = modelProviderRecord({
        ...(existing ?? {}),
        alias: write.alias,
        status: write.status,
        protocol: write.protocol,
        providerKind: write.providerKind,
        modelId: write.modelId,
        temperatureMilli: write.temperatureMilli,
        revision: (existing?.revision ?? 0) + 1,
      });
      providers.set(write.alias, record);
      return record;
    },
    async exchangeOpenAiOauthCode(
      _input: NativeOpenAiOauthCodeExchangeInput,
    ): Promise<NativeOpenAiOauthCodeExchangeResult> {
      throw new Error("not used by contract smoke");
    },
    openAiOauth: {
      issuer: "https://auth.openai.com",
      clientId: "rusty-crew-test-client",
      redirectUri: "http://localhost:1455/auth/callback",
      allowRedirectUriOverride: false,
      originator: "rusty-crew",
    },
    pendingLogins: new Map(),
    now: () => "2026-07-06T00:00:00.000Z",
    async refreshAfterWrite(_input) {
      return {
        refresh: {
          mode: "none",
          affectedProfiles: [],
          outcomes: [],
        },
      };
    },
  };
}

function modelProviderRecord(
  overrides: Partial<NativeModelProviderRecord> & { alias: string },
): NativeModelProviderRecord {
  return {
    alias: overrides.alias,
    status: overrides.status ?? "active",
    protocol: overrides.protocol ?? "chat_completions",
    providerKind: overrides.providerKind ?? "custom",
    modelId: overrides.modelId ?? "test-model",
    contextWindowTokens: overrides.contextWindowTokens,
    maxOutputTokens: overrides.maxOutputTokens,
    temperatureMilli: overrides.temperatureMilli,
    reasoningEffort: overrides.reasoningEffort,
    reasoningFormat: overrides.reasoningFormat,
    credential: overrides.credential ?? { hasSecret: false },
    metadataJson: overrides.metadataJson ?? {},
    revision: overrides.revision ?? 1,
    createdAt: overrides.createdAt ?? "2026-07-06T00:00:00.000Z",
    updatedAt: overrides.updatedAt ?? "2026-07-06T00:00:00.000Z",
  };
}

interface AdminResult {
  status: number;
  body: unknown;
}

interface OpenApiDoc {
  openapi: string;
  info: { title: string; version: string };
  paths: Record<string, unknown>;
  components: {
    schemas: Record<string, JsonSchema>;
  };
}

interface JsonSchema {
  type?: string;
  enum?: string[];
  const?: string | number | boolean;
  required?: string[] | readonly string[];
  oneOf?: JsonSchema[];
  properties?: Record<string, JsonSchema>;
}
