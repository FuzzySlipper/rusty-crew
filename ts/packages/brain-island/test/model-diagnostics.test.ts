import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import type { ProfileId, SessionState } from "@rusty-crew/contracts";
import type {
  NativeModelConfigurationRecord,
  NativeModelEndpointRecord,
  NativeProfileRegistryRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import { buildAdminProfileRegistryDiagnostics } from "../src/profile-registry-admin.js";
import { buildReadOnlySlashCommandResponse } from "../src/slash-command-responses.js";
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
  assert.equal(result.degraded, true);
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
  assert.equal(result.degraded, true);
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
  assert.equal(result.degraded, true);
});

test("context diagnostics label a legacy-only provider selection as compatibility identity", async () => {
  const context = contextForCredentialLookup(async () => undefined);
  context.bridge.getProfileRegistryRecord = async () => ({
    ...registryRecord(),
    activeRuntimeSettingsJson: { providerAlias: "legacy-diagnostics" },
  });
  context.bridge.getModelProvider = async () =>
    ({
      alias: "legacy-diagnostics",
      status: "active",
      modelId: "legacy-model",
    }) as never;

  const result = await rustyViewSessionContextUsage(context, {
    session: session(),
    requestId: "legacy-provider-identity",
  });

  assert.equal(result.provider.model_config_id, undefined);
  assert.equal(result.provider.provider_alias, "legacy-diagnostics");
  assert.equal(result.provider.alias, "legacy-diagnostics");
});

test("normalized model diagnostics ignore stale provider aliases and remain healthy with informational reasoning metadata", async (t) => {
  const profilesDir = await mkdtemp(
    join(tmpdir(), "rusty-crew-model-diagnostics-"),
  );
  t.after(() => rm(profilesDir, { recursive: true, force: true }));
  await writeFile(
    join(profilesDir, "profile-diagnostics.json"),
    JSON.stringify({
      profileId: "profile-diagnostics",
      modelConfigId: "model-diagnostics",
      prompt: { system: "Model diagnostics regression profile." },
    }),
  );
  const context = contextForCredentialLookup(
    async () => credential({}),
    profilesDir,
  );
  context.bridge.getProfileRegistryRecord = async () => ({
    ...registryRecord(),
    activeRuntimeSettingsJson: {
      modelConfigId: "model-diagnostics",
      providerAlias: "stale-legacy-provider",
    },
  });
  context.bridge.getModelConfiguration = async () => ({
    ...modelConfiguration(),
    reasoningFormat: "openai",
  });

  const result = await rustyViewSessionContextUsage(context, {
    session: session(),
    requestId: "normalized-provider-identity",
  });

  assert.equal(result.provider.model_config_id, "model-diagnostics");
  assert.equal(result.provider.provider_alias, undefined);
  assert.equal(result.provider.endpoint_id, "endpoint-diagnostics");
  assert.deepEqual(
    result.diagnostics.find(
      (diagnostic) =>
        diagnostic.code === "provider_reasoning_format_not_applied",
    ),
    {
      severity: "info",
      code: "provider_reasoning_format_not_applied",
      message:
        "reasoningFormat is stored for provider diagnostics but is not mapped by the selected native brain protocol",
    },
  );
  assert.equal(result.degraded, false, JSON.stringify(result.diagnostics));

  const modelResponse = buildReadOnlySlashCommandResponse("model", {
    diagnostics: {} as never,
    session: {
      sessionId: result.session_id,
      profileId: result.profile_id,
    } as never,
    modelContext: result,
  });
  assert.equal(
    modelResponse.summary,
    "profile-diagnostics uses diagnostic-model via model-diagnostics.",
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
  profilesDir?: string,
): RustyViewChatOperationsContext {
  return {
    bridge: {
      getProfileRegistryRecord: async () => ({
        activeRuntimeSettingsJson: { modelConfigId: "model-diagnostics" },
      }),
      getModelConfiguration: async () => modelConfiguration(),
      getModelEndpoint: async () => modelEndpoint(),
      getServiceCredential,
      getServiceCredentialSecret: async () =>
        JSON.stringify({ kind: "api_key", value: "diagnostics-secret" }),
      listContextCompactionArtifacts: async () => [],
    },
    runtimeConfig: runtimeConfig(profilesDir),
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

function runtimeConfig(profilesDir?: string): RustyCrewRuntimeConfig {
  return {
    profilesDir: profilesDir ?? "/tmp/rusty-crew-missing-diagnostics-profile",
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
