import assert from "node:assert/strict";

import type {
  NativeProfileRegistryRecord,
  NativeProfileRegistryWrite,
} from "@rusty-crew/native-bridge";
import {
  handleProfileRegistryWriteRequest,
  type ProfileRegistryWriteRoute,
  type ProfileRegistryWriteRouteContext,
} from "../src/service-profile-registry-routes.js";

const current = registryRecord({ revision: 4 });
const next = registryRecord({
  revision: 4,
  lifecycleStatus: "decommissioned",
  derivedStatus: "disabled",
});
const nextWrite = registryWrite(next);

let updateCalls = 0;
let lifecycleEffectCalls = 0;
const context: ProfileRegistryWriteRouteContext = {
  async planRegistryWrite(route, body) {
    const request = body as { expectedRevision?: number };
    return {
      ok: request.expectedRevision === current.revision,
      profileId: route.profileId,
      kind: registryKind(route),
      mode: route.mode,
      expectedRevision: request.expectedRevision ?? 0,
      current,
      next,
      nextWrite,
      diagnostics:
        request.expectedRevision === current.revision
          ? []
          : [
              {
                severity: "error",
                code: "profile_registry_revision_mismatch",
                path: "expectedRevision",
                message: "expected revision 3, found 4",
              },
            ],
      implications: {
        registryRevisionWillIncrement: true,
        profileFilesUnchanged: true,
        serviceConfigUnchanged: true,
        runtimeRebuildRecommended: true,
        lifecycleEffects: "archive_active_sessions_and_unregister_brain",
      },
    };
  },
  async planRuntimeConfigWrite() {
    throw new Error("runtime-config planner should not be called");
  },
  async updateProfileRegistryRecord() {
    updateCalls += 1;
    return { ...next, revision: 5 };
  },
  async applyLifecycleEffects() {
    lifecycleEffectCalls += 1;
    return {
      sessionsArchived: ["profile-alpha-session"],
      brainHandle: { action: "unregistered", handle: 8 },
    };
  },
  async applyRuntimeConfigEffects() {
    throw new Error("runtime-config effects should not be called");
  },
};

const mismatch = await handleProfileRegistryWriteRequest(
  request("/v1/admin/profiles/registry/profile-alpha/lifecycle/apply", {
    expectedRevision: 3,
    lifecycleStatus: "decommissioned",
  }),
  context,
);
assert.equal(mismatch.status, 200);
assert.equal(mismatch.body.ok, true);
const mismatchData = okData<{
  ok: boolean;
}>(mismatch.body);
assert.equal(mismatchData.ok, false);
assert.equal(updateCalls, 0);
assert.equal(lifecycleEffectCalls, 0);
const mismatchSkippedApply = updateCalls === 0 && lifecycleEffectCalls === 0;

const applied = await handleProfileRegistryWriteRequest(
  request("/v1/admin/profiles/registry/profile-alpha/lifecycle/apply", {
    expectedRevision: 4,
    lifecycleStatus: "decommissioned",
  }),
  context,
);
assert.equal(applied.status, 200);
const appliedData = okData<{
  applied: boolean;
  effects: { sessionsArchived: string[] };
}>(applied.body);
assert.equal(appliedData.applied, true);
assert.deepEqual(appliedData.effects.sessionsArchived, [
  "profile-alpha-session",
]);
assert.equal(updateCalls, 1);
assert.equal(lifecycleEffectCalls, 1);

const blockingContext: ProfileRegistryWriteRouteContext = {
  ...context,
  async applyLifecycleEffects() {
    throw new Error(
      "profile profile-alpha lifecycle transition blocked by in-flight wake(s): profile-alpha-session",
    );
  },
};
await assert.rejects(
  handleProfileRegistryWriteRequest(
    request("/v1/admin/profiles/registry/profile-alpha/lifecycle/apply", {
      expectedRevision: 4,
      lifecycleStatus: "decommissioned",
    }),
    blockingContext,
  ),
  /blocked by in-flight wake/,
);

console.log(
  JSON.stringify(
    {
      mismatchSkippedApply,
      lifecycleEffectCalls,
      archiveBlocking: "propagated",
    },
    null,
    2,
  ),
);

function request(pathname: string, body: unknown) {
  return {
    method: "POST",
    url: `http://127.0.0.1${pathname}`,
    body,
    requestId: "smoke-profile-registry-write-routes",
  };
}

function okData<T>(envelope: { ok: boolean; data?: unknown }): T {
  assert.equal(envelope.ok, true);
  return envelope.data as T;
}

function registryKind(
  route: ProfileRegistryWriteRoute,
): "update" | "lifecycle" | "prompt" {
  if (route.kind === "runtime-config") {
    throw new Error("runtime-config route is outside this smoke");
  }
  return route.kind;
}

function registryRecord(input: {
  revision: number;
  lifecycleStatus?: NativeProfileRegistryRecord["lifecycleStatus"];
  derivedStatus?: string;
}): NativeProfileRegistryRecord {
  return {
    profileId: "profile-alpha",
    lifecycleStatus: input.lifecycleStatus ?? "active",
    displayName: "Profile Alpha",
    summary: "Focused route smoke",
    defaultSessionKind: "full",
    agentId: "profile-alpha-agent",
    ownerId: "owner",
    promptSoulMarkdown: "soul",
    promptMemoryMarkdown: "memory",
    activeRuntimeSettingsJson: {},
    sourceAssetRefs: [],
    derivedRuntimeRefs: [
      {
        refKind: "session",
        refId: "profile-alpha-session",
        status: input.derivedStatus ?? "active",
        metadataJson: {},
      },
    ],
    importExport: { metadataJson: {} },
    revision: input.revision,
    createdAt: "2026-07-06T00:00:00.000Z",
    updatedAt: "2026-07-06T00:00:00.000Z",
  };
}

function registryWrite(
  record: NativeProfileRegistryRecord,
): NativeProfileRegistryWrite {
  return {
    profileId: record.profileId,
    lifecycleStatus: record.lifecycleStatus,
    displayName: record.displayName,
    summary: record.summary,
    defaultSessionKind: record.defaultSessionKind,
    agentId: record.agentId,
    ownerId: record.ownerId,
    promptSoulMarkdown: record.promptSoulMarkdown,
    promptMemoryMarkdown: record.promptMemoryMarkdown,
    activeRuntimeSettingsJson: record.activeRuntimeSettingsJson,
    sourceAssetRefs: record.sourceAssetRefs,
    derivedRuntimeRefs: record.derivedRuntimeRefs,
    importExport: record.importExport,
    now: "2026-07-06T00:00:00.000Z",
  };
}
