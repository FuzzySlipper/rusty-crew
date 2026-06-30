import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import type { NativeProfileRegistryRecord } from "@rusty-crew/native-bridge";
import {
  buildAdminProfileRegistryDiagnostics,
  filterAdminProfileRegistryRecords,
} from "./profile-registry-admin.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-registry-admin-"));
const profilesDir = join(root, "profiles");
mkdirSync(profilesDir, { recursive: true });

try {
  const registeredDir = join(profilesDir, "registered");
  mkdirSync(registeredDir, { recursive: true });
  const registeredProfilePath = join(registeredDir, "profile.yaml");
  writeFileSync(
    registeredProfilePath,
    `profileIdentity: registered
displayName: Registered
modelConfig:
  provider: den-router
  model: local-deterministic
`,
  );
  const originalHash =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

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
  writeFileSync(join(fallbackDir, "soul.md"), "Do useful fallback work.");

  const registryRecords: NativeProfileRegistryRecord[] = [
    {
      profileId: "registered",
      lifecycleStatus: "active",
      displayName: "Registered",
      activeRuntimeSettingsJson: { modelConfig: { provider: "den-router" } },
      sourceAssetRefs: [
        {
          assetKind: "profile_yaml",
          path: registeredProfilePath,
          contentHash: originalHash,
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
        importedAt: "2026-06-26T09:00:00Z",
        metadataJson: {},
      },
      revision: 2,
      createdAt: "2026-06-26T09:00:00Z",
      updatedAt: "2026-06-26T09:00:00Z",
    },
    {
      profileId: "archived",
      lifecycleStatus: "archived",
      activeRuntimeSettingsJson: {},
      sourceAssetRefs: [],
      derivedRuntimeRefs: [],
      importExport: { metadataJson: {} },
      revision: 4,
      createdAt: "2026-06-26T09:00:00Z",
      updatedAt: "2026-06-26T09:05:00Z",
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
    now: "2026-06-26T09:10:00Z",
  });

  assert.equal(diagnostics.registryCount, 2);
  assert.equal(diagnostics.fileFallbackCount, 1);
  assert.equal(diagnostics.driftCount, 1);
  assert.equal(
    diagnostics.records.find((record) => record.profileId === "registered")
      ?.sourceAssetStatuses[0]?.status,
    "changed",
  );
  assert.equal(
    diagnostics.records.find((record) => record.profileId === "file-only")
      ?.source,
    "file_fallback",
  );
  assert.equal(
    diagnostics.records
      .find((record) => record.profileId === "file-only")
      ?.sourceAssetStatuses.some((asset) => asset.assetKind === "soul_md"),
    true,
  );
  assert.equal(
    diagnostics.records.find((record) => record.profileId === "file-only")
      ?.promptSoulMarkdown,
    "Do useful fallback work.",
  );
  assert.equal(
    diagnostics.records.find((record) => record.profileId === "registered")
      ?.providerAlias,
    undefined,
  );
  assert.equal(
    filterAdminProfileRegistryRecords(
      diagnostics.records,
      new URL(
        "/v1/admin/profiles/registry?lifecycle_status=archived",
        "http://local",
      ),
    )[0]?.profileId,
    "archived",
  );

  console.log(
    JSON.stringify(
      {
        registryCount: diagnostics.registryCount,
        fileFallbackCount: diagnostics.fileFallbackCount,
        driftCount: diagnostics.driftCount,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
