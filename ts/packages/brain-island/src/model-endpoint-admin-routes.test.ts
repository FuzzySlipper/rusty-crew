import assert from "node:assert/strict";
import test from "node:test";
import { retryDeferredCredentialRuntimeRefreshWithClaim } from "./service-app.js";
import {
  applyNormalizedCredentialRuntimeRefreshes,
  handleModelRegistryAdminRequest,
  modelConfigurationApiRecord,
  modelEndpointApiRecord,
  normalizedCredentialRefreshProfileIds,
  normalizedModelRefreshProfileIds,
  type ModelEndpointAdminRouteContext,
  type ModelEndpointAdminRouteRequest,
} from "./service-model-endpoint-admin-routes.js";
import type {
  NativeModelConfigurationQuery,
  NativeModelConfigurationDelete,
  NativeModelConfigurationRecord,
  NativeModelConfigurationWrite,
  NativeModelEndpointQuery,
  NativeModelEndpointDelete,
  NativeModelEndpointRecord,
  NativeModelEndpointWrite,
} from "./model-endpoint-admin-contract.js";

const NOW = "2026-08-13T00:00:00.000Z";

test("normalized refresh fans endpoint changes out but keeps model changes selective", () => {
  const configurations = [
    { modelConfigId: "model-a", endpointId: "shared" },
    { modelConfigId: "model-b", endpointId: "shared" },
    { modelConfigId: "model-c", endpointId: "other" },
  ];
  const profiles = configurations.map((configuration) => ({
    profileId: `profile-${configuration.modelConfigId}`,
    activeRuntimeSettingsJson: {
      modelConfigId: configuration.modelConfigId,
    },
  }));
  assert.deepEqual(
    normalizedModelRefreshProfileIds({
      kind: "endpoint",
      id: "shared",
      configurations,
      profiles,
    }),
    ["profile-model-a", "profile-model-b"],
  );
  assert.deepEqual(
    normalizedModelRefreshProfileIds({
      kind: "configuration",
      id: "model-a",
      configurations,
      profiles,
    }),
    ["profile-model-a"],
  );
});

test("normalized credential refresh rebuilds every shared consumer and no unrelated profile", () => {
  assert.deepEqual(
    normalizedCredentialRefreshProfileIds({
      credentialId: "credential-shared",
      endpoints: [
        { endpointId: "endpoint-a", credentialId: "credential-shared" },
        { endpointId: "endpoint-b", credentialId: "credential-shared" },
        { endpointId: "endpoint-other", credentialId: "credential-other" },
      ],
      configurations: [
        { modelConfigId: "model-a", endpointId: "endpoint-a" },
        { modelConfigId: "model-b", endpointId: "endpoint-b" },
        { modelConfigId: "model-other", endpointId: "endpoint-other" },
      ],
      profiles: [
        {
          profileId: "profile-b",
          activeRuntimeSettingsJson: { modelConfigId: "model-b" },
        },
        {
          profileId: "profile-other",
          activeRuntimeSettingsJson: { modelConfigId: "model-other" },
        },
        {
          profileId: "profile-a",
          activeRuntimeSettingsJson: {
            profile: { modelConfigId: "model-a" },
          },
        },
      ],
    }),
    ["profile-a", "profile-b"],
  );
});

test("normalized credential refresh retries an in-flight shared consumer exactly once", async () => {
  const runtimeCredentialRevisions = new Map([
    ["profile-a", 1],
    ["profile-b", 1],
    ["profile-other", 1],
  ]);
  const rebuildCounts = new Map<string, number>();
  const deferred = new Set<string>();
  let profileBInFlight = true;
  const rebuild = async (profileId: string) => {
    if (profileId === "profile-b" && profileBInFlight) {
      return {
        status: "blocked" as const,
        reasonCode: "runtime_rebuild_in_flight",
      };
    }
    rebuildCounts.set(profileId, (rebuildCounts.get(profileId) ?? 0) + 1);
    runtimeCredentialRevisions.set(profileId, 2);
    return { status: "completed" as const };
  };
  const complete = (profileId: string) => deferred.delete(profileId);

  const initial = await applyNormalizedCredentialRuntimeRefreshes({
    profileIds: ["profile-a", "profile-b"],
    rebuild,
    defer: (profileId) => deferred.add(profileId),
    complete,
  });
  assert.deepEqual(initial, {
    refreshedProfileIds: ["profile-a"],
    deferredProfileIds: ["profile-b"],
  });
  assert.deepEqual([...deferred], ["profile-b"]);

  profileBInFlight = false;
  const retry = await applyNormalizedCredentialRuntimeRefreshes({
    profileIds: [...deferred],
    rebuild,
    defer: (profileId) => deferred.add(profileId),
    complete,
  });
  assert.deepEqual(retry, {
    refreshedProfileIds: ["profile-b"],
    deferredProfileIds: [],
  });
  assert.deepEqual(Object.fromEntries(runtimeCredentialRevisions), {
    "profile-a": 2,
    "profile-b": 2,
    "profile-other": 1,
  });
  assert.deepEqual(Object.fromEntries(rebuildCounts), {
    "profile-a": 1,
    "profile-b": 1,
  });
  assert.equal(deferred.size, 0);
});

test("deferred credential retry claims serialize concurrent settlements and retain pending state", async () => {
  const pendingProfileIds = new Set(["profile-shared"]);
  const claimedProfileIds = new Set<string>();
  const credentialRevisions = new Map([
    ["profile-shared", 1],
    ["profile-unrelated", 1],
  ]);
  const completedRebuilds = new Map<string, number>();
  let activeRebuilds = 0;
  let maximumActiveRebuilds = 0;
  let releaseRebuild!: () => void;
  let rebuildEntered!: () => void;
  const rebuildStarted = new Promise<void>((resolve) => {
    rebuildEntered = resolve;
  });
  const rebuildRelease = new Promise<void>((resolve) => {
    releaseRebuild = resolve;
  });
  const rebuild = async (profileId: string) => {
    activeRebuilds += 1;
    maximumActiveRebuilds = Math.max(maximumActiveRebuilds, activeRebuilds);
    rebuildEntered();
    await rebuildRelease;
    activeRebuilds -= 1;
    const nextRevision = (credentialRevisions.get(profileId) ?? 0) + 1;
    credentialRevisions.set(profileId, nextRevision);
    completedRebuilds.set(
      profileId,
      (completedRebuilds.get(profileId) ?? 0) + 1,
    );
    return { status: "completed" as const };
  };

  const firstSettlement = retryDeferredCredentialRuntimeRefreshWithClaim({
    profileId: "profile-shared",
    pendingProfileIds,
    claimedProfileIds,
    rebuild,
  });
  await rebuildStarted;
  const secondSettlement = retryDeferredCredentialRuntimeRefreshWithClaim({
    profileId: "profile-shared",
    pendingProfileIds,
    claimedProfileIds,
    rebuild,
  });

  assert.equal(maximumActiveRebuilds, 1);
  releaseRebuild();
  assert.deepEqual(await Promise.all([firstSettlement, secondSettlement]), [
    "completed",
    "skipped",
  ]);
  assert.equal(credentialRevisions.get("profile-shared"), 2);
  assert.equal(completedRebuilds.get("profile-shared"), 1);
  assert.equal(pendingProfileIds.has("profile-shared"), false);
  assert.equal(claimedProfileIds.size, 0);

  let remainingSessionInFlight = true;
  pendingProfileIds.add("profile-shared");
  const blockedRetry = await retryDeferredCredentialRuntimeRefreshWithClaim({
    profileId: "profile-shared",
    pendingProfileIds,
    claimedProfileIds,
    rebuild: async (profileId) => {
      if (remainingSessionInFlight) {
        return {
          status: "blocked" as const,
          reasonCode: "runtime_rebuild_in_flight",
        };
      }
      const nextRevision = (credentialRevisions.get(profileId) ?? 0) + 1;
      credentialRevisions.set(profileId, nextRevision);
      completedRebuilds.set(
        profileId,
        (completedRebuilds.get(profileId) ?? 0) + 1,
      );
      return { status: "completed" as const };
    },
  });
  assert.equal(blockedRetry, "pending");
  assert.equal(pendingProfileIds.has("profile-shared"), true);

  remainingSessionInFlight = false;
  assert.equal(
    await retryDeferredCredentialRuntimeRefreshWithClaim({
      profileId: "profile-shared",
      pendingProfileIds,
      claimedProfileIds,
      rebuild: async (profileId) => {
        const nextRevision = (credentialRevisions.get(profileId) ?? 0) + 1;
        credentialRevisions.set(profileId, nextRevision);
        completedRebuilds.set(
          profileId,
          (completedRebuilds.get(profileId) ?? 0) + 1,
        );
        return { status: "completed" as const };
      },
    }),
    "completed",
  );
  assert.equal(credentialRevisions.get("profile-shared"), 3);
  assert.equal(completedRebuilds.get("profile-shared"), 2);
  assert.equal(credentialRevisions.get("profile-unrelated"), 1);
  assert.equal(pendingProfileIds.size, 0);
  assert.equal(claimedProfileIds.size, 0);
});

class MemoryModelRegistry implements ModelEndpointAdminRouteContext {
  readonly endpoints = new Map<string, NativeModelEndpointRecord>();
  readonly configurations = new Map<string, NativeModelConfigurationRecord>();

  now(): string {
    return NOW;
  }

  async upsertModelEndpoint(
    write: NativeModelEndpointWrite,
  ): Promise<NativeModelEndpointRecord> {
    const current = this.endpoints.get(write.endpointId);
    const currentRevision = current?.revision ?? 0;
    if (
      write.expectedRevision !== undefined &&
      write.expectedRevision !== currentRevision
    ) {
      throw new Error(
        `model endpoint ${write.endpointId} revision mismatch: expected ${write.expectedRevision}, found ${currentRevision}`,
      );
    }
    const record: NativeModelEndpointRecord = {
      endpointId: write.endpointId,
      status: write.status,
      ...(write.displayName === undefined
        ? {}
        : { displayName: write.displayName }),
      ...(write.description === undefined
        ? {}
        : { description: write.description }),
      baseUrl: write.baseUrl,
      protocol: write.protocol,
      wireDialect: write.wireDialect,
      authScheme: write.authScheme ?? "none",
      ...(write.credentialId === undefined
        ? {}
        : { credentialId: write.credentialId }),
      promptCacheTransport: write.promptCacheTransport ?? "none",
      metadataJson: write.metadataJson ?? {},
      revision: currentRevision + 1,
      createdAt: current?.createdAt ?? write.now,
      updatedAt: write.now,
    };
    this.endpoints.set(record.endpointId, record);
    return record;
  }

  async listModelEndpoints(
    query: NativeModelEndpointQuery,
  ): Promise<NativeModelEndpointRecord[]> {
    return [...this.endpoints.values()]
      .filter(
        (record) =>
          (query.endpointId === undefined ||
            record.endpointId === query.endpointId) &&
          (query.status === undefined || record.status === query.status),
      )
      .slice(query.offset ?? 0, (query.offset ?? 0) + (query.limit ?? 100));
  }

  async getModelEndpoint(
    endpointId: string,
  ): Promise<NativeModelEndpointRecord | undefined> {
    return this.endpoints.get(endpointId);
  }

  async deleteModelEndpoint(
    deleteRequest: NativeModelEndpointDelete,
  ): Promise<NativeModelEndpointRecord> {
    const current = this.endpoints.get(deleteRequest.endpointId);
    if (current === undefined) {
      throw new Error(`model endpoint ${deleteRequest.endpointId} not found`);
    }
    if (current.revision !== deleteRequest.expectedRevision) {
      throw new Error(
        `model endpoint ${deleteRequest.endpointId} revision mismatch: expected ${deleteRequest.expectedRevision}, found ${current.revision}`,
      );
    }
    const references = [...this.configurations.values()].filter(
      (configuration) => configuration.endpointId === deleteRequest.endpointId,
    );
    if (references.length > 0) {
      throw new Error(
        `model endpoint ${deleteRequest.endpointId} is still referenced by ${references.length} model configuration(s)`,
      );
    }
    this.endpoints.delete(deleteRequest.endpointId);
    return current;
  }

  async upsertModelConfiguration(
    write: NativeModelConfigurationWrite,
  ): Promise<NativeModelConfigurationRecord> {
    const current = this.configurations.get(write.modelConfigId);
    const currentRevision = current?.revision ?? 0;
    if (
      write.expectedRevision !== undefined &&
      write.expectedRevision !== currentRevision
    ) {
      throw new Error(
        `model configuration ${write.modelConfigId} revision mismatch: expected ${write.expectedRevision}, found ${currentRevision}`,
      );
    }
    const record: NativeModelConfigurationRecord = {
      modelConfigId: write.modelConfigId,
      endpointId: write.endpointId,
      status: write.status,
      ...(write.displayName === undefined
        ? {}
        : { displayName: write.displayName }),
      ...(write.description === undefined
        ? {}
        : { description: write.description }),
      modelId: write.modelId,
      ...(write.contextWindowTokens === undefined
        ? {}
        : { contextWindowTokens: write.contextWindowTokens }),
      ...(write.maxOutputTokens === undefined
        ? {}
        : { maxOutputTokens: write.maxOutputTokens }),
      ...(write.temperatureMilli === undefined
        ? {}
        : { temperatureMilli: write.temperatureMilli }),
      ...(write.reasoningEffort === undefined
        ? {}
        : { reasoningEffort: write.reasoningEffort }),
      ...(write.reasoningFormat === undefined
        ? {}
        : { reasoningFormat: write.reasoningFormat }),
      reasoningHistory: write.reasoningHistory ?? "provider_default",
      ...(write.reasoningBudgetTokens === undefined
        ? {}
        : { reasoningBudgetTokens: write.reasoningBudgetTokens }),
      thinkingMode: write.thinkingMode ?? "provider_default",
      promptCachingPolicy: write.promptCachingPolicy ?? "disabled",
      capabilities: write.capabilities ?? { version: 1, imageInput: false },
      metadataJson: write.metadataJson ?? {},
      revision: currentRevision + 1,
      createdAt: current?.createdAt ?? write.now,
      updatedAt: write.now,
    };
    this.configurations.set(record.modelConfigId, record);
    return record;
  }

  async listModelConfigurations(
    query: NativeModelConfigurationQuery,
  ): Promise<NativeModelConfigurationRecord[]> {
    return [...this.configurations.values()]
      .filter(
        (record) =>
          (query.modelConfigId === undefined ||
            record.modelConfigId === query.modelConfigId) &&
          (query.endpointId === undefined ||
            record.endpointId === query.endpointId) &&
          (query.status === undefined || record.status === query.status),
      )
      .slice(query.offset ?? 0, (query.offset ?? 0) + (query.limit ?? 100));
  }

  async getModelConfiguration(
    modelConfigId: string,
  ): Promise<NativeModelConfigurationRecord | undefined> {
    return this.configurations.get(modelConfigId);
  }

  async deleteModelConfiguration(
    deleteRequest: NativeModelConfigurationDelete,
  ): Promise<NativeModelConfigurationRecord> {
    const current = this.configurations.get(deleteRequest.modelConfigId);
    if (current === undefined) {
      throw new Error(
        `model configuration ${deleteRequest.modelConfigId} not found`,
      );
    }
    if (current.revision !== deleteRequest.expectedRevision) {
      throw new Error(
        `model configuration ${deleteRequest.modelConfigId} revision mismatch: expected ${deleteRequest.expectedRevision}, found ${current.revision}`,
      );
    }
    this.configurations.delete(deleteRequest.modelConfigId);
    return current;
  }

  forceEndpointRevision(endpointId: string, revision: number): void {
    const current = this.endpoints.get(endpointId);
    if (current === undefined)
      throw new Error(`missing endpoint ${endpointId}`);
    this.endpoints.set(endpointId, { ...current, revision });
  }

  forceConfigurationRevision(modelConfigId: string, revision: number): void {
    const current = this.configurations.get(modelConfigId);
    if (current === undefined) {
      throw new Error(`missing configuration ${modelConfigId}`);
    }
    this.configurations.set(modelConfigId, { ...current, revision });
  }
}

function request(
  method: string,
  path: string,
  body?: unknown,
): ModelEndpointAdminRouteRequest {
  return {
    method,
    url: `https://crew.test${path}`,
    body,
    requestId: `${method.toLowerCase()}-${path.replaceAll("/", "-")}`,
  };
}

function responseData<T>(
  result: Awaited<ReturnType<typeof handleModelRegistryAdminRequest>>,
): T {
  assert.equal(result.status, 200);
  if (!result.body.ok) throw new Error(result.body.error.message);
  return result.body.data as T;
}

function responseError(
  result: Awaited<ReturnType<typeof handleModelRegistryAdminRequest>>,
): { code: string; reason_code: string; message: string } {
  assert.notEqual(result.status, 200);
  if (result.body.ok) throw new Error("expected a failed route response");
  return result.body.error;
}

test("normalized model endpoint and configuration CRUD keeps the public shape closed", async () => {
  const context = new MemoryModelRegistry();
  const endpointCreate = await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-endpoints", {
      endpointId: "openai-main",
      status: "active",
      displayName: "OpenAI main",
      baseUrl: "https://api.openai.com/v1",
      protocol: "responses",
      wireDialect: "openai_stateful",
      authScheme: "bearer_api_key",
      credentialId: "credential-openai",
      promptCacheTransport: "none",
      metadataJson: { region: "primary" },
    }),
    context,
  );
  const endpoint = responseData<{ endpoint: NativeModelEndpointRecord }>(
    endpointCreate,
  ).endpoint;
  assert.equal(endpoint.revision, 1);
  assert.equal(endpoint.credentialId, "credential-openai");
  assert.equal("providerKind" in endpoint, false);
  assert.equal("secret" in endpoint, false);

  const endpointList = await handleModelRegistryAdminRequest(
    request("GET", "/v1/admin/model-endpoints?status=active"),
    context,
  );
  assert.deepEqual(
    responseData<{ items: NativeModelEndpointRecord[] }>(endpointList).items,
    [endpoint],
  );

  const endpointPatch = await handleModelRegistryAdminRequest(
    request("PATCH", "/v1/admin/model-endpoints/openai-main", {
      displayName: "OpenAI primary",
      expectedRevision: 1,
    }),
    context,
  );
  assert.equal(
    responseData<{ endpoint: NativeModelEndpointRecord }>(endpointPatch)
      .endpoint.displayName,
    "OpenAI primary",
  );

  const configurationCreate = await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-configurations", {
      modelConfigId: "gpt-main",
      endpointId: "openai-main",
      status: "active",
      modelId: "gpt-4.1",
      capabilities: { version: 1, imageInput: true },
      metadataJson: { purpose: "chat" },
    }),
    context,
  );
  const configuration = responseData<{
    configuration: NativeModelConfigurationRecord;
  }>(configurationCreate).configuration;
  assert.equal(configuration.revision, 1);
  assert.equal(configuration.capabilities.imageInput, true);
  assert.equal("providerKind" in configuration, false);
  assert.equal("secret" in configuration, false);

  const configurationPatch = await handleModelRegistryAdminRequest(
    request("PATCH", "/v1/admin/model-configurations/gpt-main", {
      displayName: "GPT main",
      expectedRevision: 1,
    }),
    context,
  );
  assert.equal(
    responseData<{ configuration: NativeModelConfigurationRecord }>(
      configurationPatch,
    ).configuration.displayName,
    "GPT main",
  );

  const configurationList = await handleModelRegistryAdminRequest(
    request("GET", "/v1/admin/model-configurations?endpointId=openai-main"),
    context,
  );
  assert.equal(
    responseData<{ items: NativeModelConfigurationRecord[] }>(configurationList)
      .items.length,
    1,
  );

  assert.deepEqual(
    modelEndpointApiRecord({
      ...endpoint,
      providerKind: "legacy-provider",
      secret: "must-not-leak",
    } as NativeModelEndpointRecord & Record<string, unknown>),
    endpoint,
  );
  assert.deepEqual(
    modelConfigurationApiRecord({
      ...configuration,
      providerKind: "legacy-provider",
      secret: "must-not-leak",
    } as NativeModelConfigurationRecord & Record<string, unknown>),
    configuration,
  );

  const endpointInUse = await handleModelRegistryAdminRequest(
    request("DELETE", "/v1/admin/model-endpoints/openai-main", {
      expectedRevision: 2,
    }),
    context,
  );
  assert.equal(endpointInUse.status, 409);
  assert.equal(
    responseError(endpointInUse).reason_code,
    "model_endpoint_in_use",
  );

  const configurationDelete = await handleModelRegistryAdminRequest(
    request("DELETE", "/v1/admin/model-configurations/gpt-main", {
      expectedRevision: 2,
    }),
    context,
  );
  assert.equal(
    responseData<{ configuration: NativeModelConfigurationRecord }>(
      configurationDelete,
    ).configuration.modelConfigId,
    "gpt-main",
  );

  const endpointDelete = await handleModelRegistryAdminRequest(
    request("DELETE", "/v1/admin/model-endpoints/openai-main", {
      expectedRevision: 2,
    }),
    context,
  );
  assert.equal(
    responseData<{ endpoint: NativeModelEndpointRecord }>(endpointDelete)
      .endpoint.endpointId,
    "openai-main",
  );
  assert.equal(context.configurations.size, 0);
  assert.equal(context.endpoints.size, 0);
});

test("normalized model registry rejects closed-enum and legacy secret fields", async () => {
  const context = new MemoryModelRegistry();
  const invalidDialect = await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-endpoints", {
      endpointId: "invalid-dialect",
      baseUrl: "https://models.test",
      protocol: "responses",
      wireDialect: "not-a-dialect",
    }),
    context,
  );
  assert.equal(
    responseError(invalidDialect).reason_code,
    "invalid_model_endpoint",
  );

  const forbiddenSecret = await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-endpoints", {
      endpointId: "secret-field",
      baseUrl: "https://models.test",
      protocol: "responses",
      wireDialect: "openai_stateful",
      secret: "should-not-be-accepted",
    }),
    context,
  );
  assert.match(responseError(forbiddenSecret).message, /secret/u);

  await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-endpoints", {
      endpointId: "policy-endpoint",
      baseUrl: "https://models.test",
      protocol: "responses",
      wireDialect: "openai_stateful",
    }),
    context,
  );

  const invalidPolicy = await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-configurations", {
      modelConfigId: "invalid-policy",
      endpointId: "policy-endpoint",
      modelId: "gpt-4.1",
      promptCachingPolicy: "always",
    }),
    context,
  );
  assert.equal(
    responseError(invalidPolicy).reason_code,
    "invalid_model_configuration",
  );
});

test("normalized model registry returns the current record for CAS conflicts", async () => {
  const context = new MemoryModelRegistry();
  await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-endpoints", {
      endpointId: "cas-endpoint",
      baseUrl: "https://models.test",
      protocol: "responses",
      wireDialect: "openai_stateful",
    }),
    context,
  );
  context.forceEndpointRevision("cas-endpoint", 2);
  const endpointConflict = await handleModelRegistryAdminRequest(
    request("PATCH", "/v1/admin/model-endpoints/cas-endpoint", {
      displayName: "stale update",
      expectedRevision: 1,
    }),
    context,
  );
  assert.equal(endpointConflict.status, 409);
  assert.equal(endpointConflict.body.ok, false);
  if (endpointConflict.body.ok) return;
  const endpointConflictBody =
    endpointConflict.body as typeof endpointConflict.body & {
      data: {
        currentRevision: number;
        expectedRevision: number;
        endpoint?: NativeModelEndpointRecord;
      };
    };
  assert.equal(endpointConflictBody.error.code, "conflict");
  assert.equal(endpointConflictBody.data.currentRevision, 2);
  assert.equal(endpointConflictBody.data.expectedRevision, 1);
  assert.equal(endpointConflictBody.data.endpoint?.revision, 2);

  await handleModelRegistryAdminRequest(
    request("POST", "/v1/admin/model-configurations", {
      modelConfigId: "cas-config",
      endpointId: "cas-endpoint",
      modelId: "gpt-4.1",
    }),
    context,
  );
  context.forceConfigurationRevision("cas-config", 2);
  const configurationConflict = await handleModelRegistryAdminRequest(
    request("PATCH", "/v1/admin/model-configurations/cas-config", {
      description: "stale update",
      expectedRevision: 1,
    }),
    context,
  );
  assert.equal(configurationConflict.status, 409);
  assert.equal(configurationConflict.body.ok, false);
  if (configurationConflict.body.ok) return;
  const configurationConflictBody =
    configurationConflict.body as typeof configurationConflict.body & {
      data: {
        currentRevision: number;
        configuration?: NativeModelConfigurationRecord;
      };
    };
  assert.equal(configurationConflictBody.error.code, "conflict");
  assert.equal(configurationConflictBody.data.currentRevision, 2);
  assert.equal(configurationConflictBody.data.configuration?.revision, 2);
});
