import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import type { NativeProfileRegistryRecord } from "@rusty-crew/native-bridge";
import { handleAdminDiagnosticsRequest } from "./admin-diagnostics-api.js";
import { buildAdminProfileRegistryDiagnostics } from "./profile-registry-admin.js";
import { buildProfileBundleExportPlan } from "./profile-registry-export.js";
import { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-registry-export-"));
const profilesDir = join(root, "profiles");
mkdirSync(profilesDir, { recursive: true });

try {
  const registeredDir = join(profilesDir, "registered");
  mkdirSync(registeredDir, { recursive: true });
  const registeredProfilePath = join(registeredDir, "profile.yaml");
  const registeredSoulPath = join(registeredDir, "soul.md");
  writeFileSync(
    registeredProfilePath,
    `profileIdentity: registered
displayName: Registered
modelConfig:
  provider: den-router
  model: local-deterministic
`,
  );
  writeFileSync(registeredSoulPath, "Registered hidden soul text.");

  const fallbackDir = join(profilesDir, "file-only");
  mkdirSync(fallbackDir, { recursive: true });
  writeFileSync(
    join(fallbackDir, "profile.yaml"),
    `profileIdentity: file-only
displayName: File Only
modelConfig:
  provider: den-router
  model: local-deterministic
`,
  );
  writeFileSync(join(fallbackDir, "soul.md"), "Fallback hidden soul text.");
  writeFileSync(join(fallbackDir, "memory.md"), "Fallback static memory.");

  const registryRecords: NativeProfileRegistryRecord[] = [
    {
      profileId: "registered",
      lifecycleStatus: "active",
      displayName: "Registered",
      defaultSessionKind: "full",
      agentId: "registered",
      activeRuntimeSettingsJson: {
        modelConfig: { provider: "den-router" },
        apiKey: "must-not-export",
      },
      sourceAssetRefs: [
        {
          assetKind: "profile_yaml",
          path: registeredProfilePath,
          metadataJson: {},
        },
        {
          assetKind: "soul_md",
          path: registeredSoulPath,
          metadataJson: {},
        },
      ],
      derivedRuntimeRefs: [
        {
          refKind: "session",
          refId: "registered-session",
          status: "active",
          metadataJson: {},
        },
      ],
      importExport: {
        importedFrom: "directory_yaml",
        importedAt: "2026-06-26T10:00:00Z",
        metadataJson: {},
      },
      revision: 2,
      createdAt: "2026-06-26T10:00:00Z",
      updatedAt: "2026-06-26T10:00:00Z",
    },
  ];
  const runtimeConfig: RustyCrewRuntimeConfig = {
    profilesDir,
    brains: [
      {
        implementationId: "file-only-brain" as never,
        profileId: "file-only" as ProfileId,
      },
    ],
    sessions: [],
    scheduledJobs: [],
    channelBindings: [],
    mcpBindings: [],
  };
  const diagnostics = await buildAdminProfileRegistryDiagnostics({
    bridge: {
      listProfileRegistryRecords: async () => registryRecords,
    },
    runtimeConfig,
    now: "2026-06-26T10:05:00Z",
  });

  const registryPlan = buildProfileBundleExportPlan({
    profileId: "registered",
    diagnostics,
  });
  assert.equal(registryPlan.source, "registry");
  assert.equal(registryPlan.bundleRootName, "registered-profile-bundle");
  assert(registryPlan.activeDbStateEntries.includes("registry.json"));
  assert(registryPlan.activeDbStateEntries.includes("runtime-plan.json"));
  assert(registryPlan.fileAssetEntries.includes("soul.md"));
  assert(
    registryPlan.optionalEntries.includes("memory-spaces/profile_dense.json"),
  );
  assert.equal(
    registryPlan.entries.find((entry) => entry.targetPath === "soul.md")
      ?.originPath,
    registeredSoulPath,
  );
  assert.equal(
    JSON.stringify(registryPlan).includes("Registered hidden soul text"),
    false,
  );
  assert.equal(JSON.stringify(registryPlan).includes("must-not-export"), false);

  const fallbackPlan = buildProfileBundleExportPlan({
    profileId: "file-only",
    diagnostics,
  });
  assert.equal(fallbackPlan.source, "file_fallback");
  assert(
    fallbackPlan.warnings.some((warning) => warning.includes("file-backed")),
  );
  assert(fallbackPlan.fileAssetEntries.includes("memory.md"));
  assert.equal(
    JSON.stringify(fallbackPlan).includes("Fallback hidden soul text"),
    false,
  );

  const route = handleAdminDiagnosticsRequest(
    {
      method: "GET",
      url: "/v1/admin/profiles/registry/file-only/export-plan",
    },
    { diagnostics: emptyRuntimeDiagnostics(), profileRegistry: diagnostics },
  );
  assert.equal(route.status, 200);
  assert.equal(route.body.ok, true);
  const routePlan = route.body.ok
    ? (route.body.data as { profileId: string })
    : undefined;
  assert.equal(routePlan?.profileId, "file-only");

  console.log(
    JSON.stringify(
      {
        registryEntries: registryPlan.entries.length,
        fallbackEntries: fallbackPlan.entries.length,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

function emptyRuntimeDiagnostics(): Parameters<
  typeof handleAdminDiagnosticsRequest
>[1]["diagnostics"] {
  return buildRuntimeDiagnosticsProjection({
    now: "2026-06-26T10:05:00Z",
    sessions: [],
    delegatedSessions: [],
    brainModules: [],
    providerStates: [],
    recentErrors: [],
  });
}
