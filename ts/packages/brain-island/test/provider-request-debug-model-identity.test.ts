import assert from "node:assert/strict";
import test from "node:test";
import { MemoryProviderRequestDebugStore } from "../src/provider-request-debug-store.js";

test("provider request diagnostics preserve normalized model identity without secrets", () => {
  const store = new MemoryProviderRequestDebugStore({
    now: () => "2026-08-13T07:00:00Z",
  });
  const record = store.record({
    sessionId: "session-1",
    wakeId: "wake-1",
    brainModule: "openai-responses",
    modelConfigId: "model-config-1",
    modelConfigRevision: 4,
    endpointId: "endpoint-1",
    endpointRevision: 3,
    credentialId: "credential-1",
    credentialRevision: 2,
    providerAlias: "deprecated-shadow",
    model: "gpt-test",
    protocol: "responses",
    request: {
      authorization: "Bearer must-not-survive",
      nested: { apiKey: "must-not-survive-either" },
    },
  });

  assert.deepEqual(record.provider, {
    brain_module: "openai-responses",
    model_config_id: "model-config-1",
    model_config_revision: 4,
    endpoint_id: "endpoint-1",
    endpoint_revision: 3,
    credential_id: "credential-1",
    credential_revision: 2,
    provider_alias: "deprecated-shadow",
    model: "gpt-test",
    protocol: "responses",
  });
  assert.equal(JSON.stringify(record).includes("must-not-survive"), false);
});
