import assert from "node:assert/strict";
import test from "node:test";

import { handleProfileRegistryWriteRequest } from "../src/service-profile-registry-routes.js";

test("reports persisted-not-applied when a late profile effect fails", async () => {
  const record = {
    profileId: "reviewer",
    revision: 23,
  } as never;
  const result = await handleProfileRegistryWriteRequest(
    {
      method: "POST",
      url: "http://localhost/v1/admin/profiles/registry/reviewer/runtime-config/apply",
      body: {},
      requestId: "request-1",
    },
    {
      async planRegistryWrite() {
        throw new Error("unexpected");
      },
      async planRuntimeConfigWrite() {
        return {
          ok: true,
          expectedRevision: 22,
          nextWrite: {} as never,
        };
      },
      async updateProfileRegistryRecord() {
        return record;
      },
      async applyLifecycleEffects() {},
      async applyPromptEffects() {},
      async applyRuntimeConfigEffects() {
        throw new Error("adapter discovery unavailable");
      },
    },
  );

  assert.equal(result.status, 200);
  assert.equal(result.body.ok, true);
  const data = result.body.data as {
    durableApplied: boolean;
    effectsApplied: boolean;
    record: { revision: number };
    reconciliation: { state: string; action: string; message: string };
  };
  assert.equal(data.durableApplied, true);
  assert.equal(data.effectsApplied, false);
  assert.equal(data.record.revision, 23);
  assert.deepEqual(data.reconciliation, {
    state: "persisted_not_applied",
    persistedRevision: 23,
    action: "retry_profile_runtime_config_apply",
    reasonCode: "profile_registry_effects_failed_after_persist",
    message: "adapter discovery unavailable",
  });
});
