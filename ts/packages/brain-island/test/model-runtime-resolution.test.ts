import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import type { ProfileId } from "@rusty-crew/contracts";
import type {
  NativeModelConfigurationRecord,
  NativeModelEndpointRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import { resolveModelConfigurationForBrain } from "../src/model-runtime-resolution.js";
import { loadProfileContext } from "../src/profile-loading.js";

const now = "2026-08-13T00:00:00.000Z";

test("normalized resolver freezes a redacted endpoint/model/credential snapshot", async () => {
  const endpoint = modelEndpoint({
    credentialId: "shared-key",
    promptCacheTransport: "openrouter_anthropic",
  });
  const configuration = modelConfiguration({
    promptCachingPolicy: "automatic_5m",
  });
  const credential = serviceCredential("api_key");
  const resolved = await resolveModelConfigurationForBrain(
    {
      getModelConfiguration: async () => configuration,
      getModelEndpoint: async () => endpoint,
      getServiceCredential: async () => credential,
      getServiceCredentialSecret: async () =>
        JSON.stringify({ kind: "api_key", value: "secret-value" }),
    },
    configuration.modelConfigId,
  );
  assert.equal(Object.isFrozen(resolved), true);
  assert.equal(resolved.modelConfigId, "model-main");
  assert.equal(resolved.modelConfigRevision, 7);
  assert.equal(resolved.endpointId, "endpoint-main");
  assert.equal(resolved.endpointRevision, 4);
  assert.equal(resolved.credentialRevision, 3);
  assert.equal(resolved.promptCacheTransport, "openrouter_anthropic");
  assert.equal(resolved.promptCaching, "automatic_5m");
  assert.equal(JSON.stringify(resolved).includes("secret-value"), false);
});

test("normalized OAuth uses credential identity without exposing its secret as an API key", async () => {
  const resolved = await resolveModelConfigurationForBrain(
    {
      getModelConfiguration: async () => modelConfiguration(),
      getModelEndpoint: async () =>
        modelEndpoint({
          protocol: "responses",
          wireDialect: "openai_stateful",
          authScheme: "openai_codex_oauth",
          credentialId: "oauth-main",
        }),
      getServiceCredential: async () => serviceCredential("openai_oauth"),
      getServiceCredentialSecret: async () => "oauth-secret-envelope",
    },
    "model-main",
  );
  assert.equal(resolved.credentialId, "oauth-main");
  assert.equal(resolved.credentialKind, "openai_oauth");
  assert.equal(resolved.apiKeyEnv, undefined);
});

test("legacy runtime selection flag can roll a modelConfigId profile back to the shadow provider", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-crew-model-source-"));
  try {
    await writeFile(
      join(root, "profile.json"),
      JSON.stringify({ profileId: "profile", modelConfigId: "model-main" }),
    );
    let normalizedCalls = 0;
    let legacyCalls = 0;
    const loaded = await loadProfileContext({
      profilesDir: root,
      profileId: "profile" as ProfileId,
      modelSelectionSource: "legacy",
      modelConfigResolver: async () => {
        normalizedCalls += 1;
        return { provider: "normalized", modelName: "normalized" };
      },
      modelProviderResolver: async (alias) => {
        legacyCalls += 1;
        assert.equal(alias, "model-main");
        return { provider: "legacy", modelName: "legacy" };
      },
    });
    assert.equal(normalizedCalls, 0);
    assert.equal(legacyCalls, 1);
    assert.equal(loaded.profile.modelConfig.provider, "legacy");
    assert.equal(
      loaded.profile.modelSelectionCompatibilityDiagnostics?.at(-1)?.code,
      "legacy_runtime_model_selection",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

function modelEndpoint(
  overrides: Partial<NativeModelEndpointRecord> = {},
): NativeModelEndpointRecord {
  return {
    endpointId: "endpoint-main",
    status: "active",
    baseUrl: "https://models.example/v1",
    protocol: "chat_completions",
    wireDialect: "standard",
    authScheme: "bearer_api_key",
    credentialId: "shared-key",
    promptCacheTransport: "none",
    metadataJson: {},
    revision: 4,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

function modelConfiguration(
  overrides: Partial<NativeModelConfigurationRecord> = {},
): NativeModelConfigurationRecord {
  return {
    modelConfigId: "model-main",
    endpointId: "endpoint-main",
    status: "active",
    modelId: "anthropic/claude-test",
    reasoningHistory: "provider_default",
    thinkingMode: "provider_default",
    promptCachingPolicy: "disabled",
    capabilities: { version: 1, imageInput: false },
    metadataJson: {},
    revision: 7,
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

function serviceCredential(
  credentialKind: NativeServiceCredentialRecord["credentialKind"],
): NativeServiceCredentialRecord {
  return {
    credentialId:
      credentialKind === "openai_oauth" ? "oauth-main" : "shared-key",
    displayName: "Shared credential",
    providerKind: "display-only",
    credentialKind,
    credential: { hasSecret: true, kind: credentialKind, revision: 3 },
    linkedProviderAliases: [],
    revision: 3,
    createdAt: now,
    updatedAt: now,
  };
}
