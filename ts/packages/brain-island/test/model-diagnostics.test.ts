import assert from "node:assert/strict";
import test from "node:test";

import type { ProfileId, SessionState } from "@rusty-crew/contracts";
import type {
  NativeModelConfigurationRecord,
  NativeModelEndpointRecord,
  NativeProfileRegistryRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import { buildAdminProfileRegistryDiagnostics } from "../src/profile-registry-admin.js";
import {
  rustyViewSessionContextUsage,
  type RustyViewChatOperationsContext,
} from "../src/service-rusty-view-chat-operations.js";
import type { RustyCrewRuntimeConfig } from "../src/service-runtime-config.js";

const NOW = "2026-08-13T08:00:00.000Z";

test("context diagnostics preserve an endpoint credential reference when the credential is missing", async () => {
  const result = await rustyViewSessionContextUsage(
    contextForCredentialLookup(async () => undefined),
    { session: session(), requestId: "missing-credential" },
  );

  assert.equal(result.provider.credential_id, "credential-missing");
  assert.equal(result.provider.endpoint_id, "endpoint-diagnostics");
  assert.ok(
    result.diagnostics.some(
      (diagnostic) => diagnostic.code === "service_credential_missing",
    ),
  );
  assert.equal(
    result.diagnostics.some(
      (diagnostic) => diagnostic.code === "model_selection_missing",
    ),
    false,
  );
});

test("context diagnostics distinguish an unavailable credential reader from a missing credential", async () => {
  const result = await rustyViewSessionContextUsage(
    contextForCredentialLookup(async () => {
      throw new Error("credential store offline");
    }),
    { session: session(), requestId: "unavailable-credential-reader" },
  );

  assert.equal(result.provider.credential_id, "credential-missing");
  assert.ok(
    result.diagnostics.some(
      (diagnostic) =>
        diagnostic.code === "model_dependency_validation_unavailable",
    ),
  );
  assert.equal(
    result.diagnostics.some(
      (diagnostic) => diagnostic.code === "service_credential_missing",
    ),
    false,
  );
});

test("context diagnostics expose a present credential with a missing secret", async () => {
  const result = await rustyViewSessionContextUsage(
    contextForCredentialLookup(async () => credential({ hasSecret: false })),
    { session: session(), requestId: "missing-secret" },
  );

  assert.equal(result.provider.credential_id, "credential-missing");
  assert.ok(
    result.diagnostics.some(
      (diagnostic) => diagnostic.code === "service_credential_secret_missing",
    ),
  );
});

test("profile registry diagnostics retain the endpoint credential reference when lookup returns no record", async () => {
  const diagnostics = await buildAdminProfileRegistryDiagnostics({
    bridge: {
      listProfileRegistryRecords: async () => [registryRecord()],
      getModelConfiguration: async () => modelConfiguration(),
      getModelEndpoint: async () => modelEndpoint(),
      getServiceCredential: async () => undefined,
    },
    runtimeConfig: runtimeConfig(),
    now: NOW,
  });

  const record = diagnostics.records[0];
  assert.equal(
    record?.modelDependencies?.endpoint?.credentialId,
    "credential-missing",
  );
  assert.ok(
    record?.diagnostics.some(
      (diagnostic) => diagnostic.code === "service_credential_missing",
    ),
  );
});

test("profile registry diagnostics report unavailable dependency readers without calling them missing", async () => {
  const diagnostics = await buildAdminProfileRegistryDiagnostics({
    bridge: {
      listProfileRegistryRecords: async () => [registryRecord()],
      getModelConfiguration: async () => modelConfiguration(),
      getModelEndpoint: async () => modelEndpoint(),
    },
    runtimeConfig: runtimeConfig(),
    now: NOW,
  });

  const record = diagnostics.records[0];
  assert.equal(
    record?.modelDependencies?.endpoint?.credentialId,
    "credential-missing",
  );
  assert.ok(
    record?.diagnostics.some(
      (diagnostic) =>
        diagnostic.code === "model_dependency_validation_unavailable",
    ),
  );
  assert.equal(
    record?.diagnostics.some(
      (diagnostic) => diagnostic.code === "service_credential_missing",
    ),
    false,
  );
});

function session(): SessionState {
  return {
    sessionId: "session-diagnostics",
    agentId: "agent-diagnostics",
    profileId: "profile-diagnostics",
    status: "active",
    toolProfile: { tools: [] },
  } as unknown as SessionState;
}

function contextForCredentialLookup(
  getServiceCredential: () => Promise<
    NativeServiceCredentialRecord | undefined
  >,
): RustyViewChatOperationsContext {
  return {
    bridge: {
      getProfileRegistryRecord: async () => ({
        activeRuntimeSettingsJson: { modelConfigId: "model-diagnostics" },
      }),
      getModelConfiguration: async () => modelConfiguration(),
      getModelEndpoint: async () => modelEndpoint(),
      getServiceCredential,
      listContextCompactionArtifacts: async () => [],
    },
    runtimeConfig: runtimeConfig(),
    toolCallDebugStore: { get: () => undefined },
    providerRequestDebugStore: { get: () => undefined },
    toolMediaAttachments: {} as never,
    now: () => NOW,
    appendChatEvent: async () => {
      throw new Error("not used");
    },
    listChatEventsAfterCursor: async () => [],
    roleplayRouteContext: () => ({}),
    submitServiceTurn: async () => ({ status: "completed", summary: "unused" }),
    resolveModelProviderForBrain: async () => ({
      provider: "unused",
      modelName: "unused",
    }),
  } as unknown as RustyViewChatOperationsContext;
}

function runtimeConfig(): RustyCrewRuntimeConfig {
  return {
    profilesDir: "/tmp/rusty-crew-missing-diagnostics-profile",
    brains: [],
    sessions: [],
    scheduledJobs: [],
    channelBindings: [],
    mcpBindings: [],
  };
}

function registryRecord(): NativeProfileRegistryRecord {
  return {
    profileId: "profile-diagnostics",
    lifecycleStatus: "active",
    activeRuntimeSettingsJson: { modelConfigId: "model-diagnostics" },
    sourceAssetRefs: [],
    derivedRuntimeRefs: [],
    importExport: { metadataJson: {} },
    revision: 1,
    createdAt: NOW,
    updatedAt: NOW,
  };
}

function modelConfiguration(): NativeModelConfigurationRecord {
  return {
    modelConfigId: "model-diagnostics",
    endpointId: "endpoint-diagnostics",
    status: "active",
    modelId: "diagnostic-model",
    reasoningHistory: "provider_default",
    thinkingMode: "provider_default",
    promptCachingPolicy: "disabled",
    capabilities: { version: 1, imageInput: false },
    metadataJson: {},
    revision: 2,
    createdAt: NOW,
    updatedAt: NOW,
  };
}

function modelEndpoint(): NativeModelEndpointRecord {
  return {
    endpointId: "endpoint-diagnostics",
    status: "active",
    baseUrl: "https://models.example.invalid/v1",
    protocol: "chat_completions",
    wireDialect: "standard",
    authScheme: "bearer_api_key",
    credentialId: "credential-missing",
    promptCacheTransport: "none",
    metadataJson: {},
    revision: 3,
    createdAt: NOW,
    updatedAt: NOW,
  };
}

function credential(
  overrides: Partial<NativeServiceCredentialRecord["credential"]>,
): NativeServiceCredentialRecord {
  return {
    credentialId: "credential-missing",
    displayName: "Diagnostics credential",
    providerKind: "diagnostics",
    credentialKind: "api_key",
    credential: { hasSecret: true, ...overrides },
    linkedProviderAliases: [],
    revision: 4,
    createdAt: NOW,
    updatedAt: NOW,
  };
}
