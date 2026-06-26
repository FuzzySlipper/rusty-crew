import { basename } from "node:path";
import type { NativeRuntimeConfigDiagnostic } from "@rusty-crew/native-bridge";
import type {
  AdminProfileRegistryDiagnostics,
  AdminProfileRegistryRecord,
} from "./profile-registry-admin.js";

export type ProfileBundleExportSource =
  | "registry_active_state"
  | "file_asset"
  | "generated_metadata"
  | "memory_space_optional";

export type ProfileBundleExportEntryKind =
  | "generated_profile_yaml"
  | "copy_file_asset"
  | "generated_registry_json"
  | "generated_runtime_plan_json"
  | "generated_checksums_json"
  | "optional_memory_space_export";

export interface ProfileBundleExportEntry {
  targetPath: string;
  kind: ProfileBundleExportEntryKind;
  source: ProfileBundleExportSource;
  originPath?: string;
  originAssetKind?: string;
  contentHash?: string;
  currentContentHash?: string;
  assetStatus?: string;
  contentJson?: unknown;
  notes: string[];
}

export interface ProfileBundleExportPlan {
  profileId: string;
  generatedAt: string;
  source: AdminProfileRegistryRecord["source"];
  lifecycleStatus: string;
  fallbackStatus: AdminProfileRegistryRecord["fallbackStatus"];
  bundleRootName: string;
  entries: ProfileBundleExportEntry[];
  activeDbStateEntries: string[];
  fileAssetEntries: string[];
  optionalEntries: string[];
  diagnostics: NativeRuntimeConfigDiagnostic[];
  warnings: string[];
}

export interface BuildProfileBundleExportPlanInput {
  profileId: string;
  diagnostics: AdminProfileRegistryDiagnostics;
}

export function buildProfileBundleExportPlan(
  input: BuildProfileBundleExportPlanInput,
): ProfileBundleExportPlan {
  const record = input.diagnostics.records.find(
    (candidate) => candidate.profileId === input.profileId,
  );
  if (!record) {
    throw new ProfileBundleExportPlanError(
      "profile_export_record_not_found",
      `profile ${input.profileId} was not found in profile registry diagnostics`,
    );
  }

  const entries = [
    generatedProfileYaml(record),
    ...fileAssetEntries(record),
    generatedRegistryJson(record),
    generatedRuntimePlanJson(record),
    generatedChecksumsJson(record),
    optionalMemorySpaceExport(record),
  ];

  return {
    profileId: record.profileId,
    generatedAt: input.diagnostics.generatedAt,
    source: record.source,
    lifecycleStatus: record.lifecycleStatus,
    fallbackStatus: record.fallbackStatus,
    bundleRootName: `${record.profileId}-profile-bundle`,
    entries,
    activeDbStateEntries: entries
      .filter((entry) => entry.source === "registry_active_state")
      .map((entry) => entry.targetPath),
    fileAssetEntries: entries
      .filter((entry) => entry.source === "file_asset")
      .map((entry) => entry.targetPath),
    optionalEntries: entries
      .filter((entry) => entry.source === "memory_space_optional")
      .map((entry) => entry.targetPath),
    diagnostics: [
      ...record.diagnostics,
      ...entries.flatMap((entry) => entryDiagnostics(record, entry)),
    ],
    warnings: exportWarnings(record, entries),
  };
}

export class ProfileBundleExportPlanError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
  }
}

function generatedProfileYaml(
  record: AdminProfileRegistryRecord,
): ProfileBundleExportEntry {
  return {
    targetPath: "profile.yaml",
    kind: "generated_profile_yaml",
    source:
      record.source === "registry"
        ? "registry_active_state"
        : "generated_metadata",
    contentJson: stripUndefined({
      profileId: record.profileId,
      displayName: record.displayName,
      summary: record.summary,
      lifecycleStatus: record.lifecycleStatus,
      defaultSessionKind: record.defaultSessionKind,
      agentId: record.agentId,
      ownerId: record.ownerId,
      source: record.source,
      fallbackStatus: record.fallbackStatus,
    }),
    notes: [
      record.source === "registry"
        ? "generated from DB-backed registry state; prompt assets remain separate file entries"
        : "generated from file-backed fallback projection; import into registry before treating this as active DB state",
    ],
  };
}

function fileAssetEntries(
  record: AdminProfileRegistryRecord,
): ProfileBundleExportEntry[] {
  return record.sourceAssetStatuses.map((asset) => ({
    targetPath: bundleAssetTargetPath(asset.assetKind, asset.path),
    kind: "copy_file_asset",
    source: "file_asset",
    originPath: asset.path,
    originAssetKind: asset.assetKind,
    contentHash: asset.contentHash,
    currentContentHash: asset.currentContentHash,
    assetStatus: asset.status,
    notes: [
      "planned as a file copy; raw file content is not embedded in the export plan",
    ],
  }));
}

function generatedRegistryJson(
  record: AdminProfileRegistryRecord,
): ProfileBundleExportEntry {
  return {
    targetPath: "registry.json",
    kind: "generated_registry_json",
    source:
      record.source === "registry"
        ? "registry_active_state"
        : "generated_metadata",
    contentJson: stripUndefined({
      profileId: record.profileId,
      source: record.source,
      lifecycleStatus: record.lifecycleStatus,
      displayName: record.displayName,
      summary: record.summary,
      defaultSessionKind: record.defaultSessionKind,
      agentId: record.agentId,
      ownerId: record.ownerId,
      revision: record.revision,
      createdAt: record.createdAt,
      updatedAt: record.updatedAt,
      importedFrom: record.importedFrom,
      importedAt: record.importedAt,
      fallbackStatus: record.fallbackStatus,
      sourceAssetRefs: record.sourceAssetRefs,
    }),
    notes: [
      "contains safe registry metadata and asset references, not raw prompt text or secret-bearing runtime settings",
    ],
  };
}

function generatedRuntimePlanJson(
  record: AdminProfileRegistryRecord,
): ProfileBundleExportEntry {
  return {
    targetPath: "runtime-plan.json",
    kind: "generated_runtime_plan_json",
    source:
      record.source === "registry"
        ? "registry_active_state"
        : "generated_metadata",
    contentJson: {
      profileId: record.profileId,
      derivedRuntimeRefs: record.activeRuntimeRefs,
      note: "runtime graph entries are exported as a plan snapshot and must be applied through service APIs",
    },
    notes: [
      "runtime graph is a snapshot/plan only; export does not mutate service config or sessions",
    ],
  };
}

function generatedChecksumsJson(
  record: AdminProfileRegistryRecord,
): ProfileBundleExportEntry {
  return {
    targetPath: "checksums.json",
    kind: "generated_checksums_json",
    source: "generated_metadata",
    contentJson: {
      profileId: record.profileId,
      assets: record.sourceAssetStatuses.map((asset) =>
        stripUndefined({
          assetKind: asset.assetKind,
          path: asset.path,
          plannedTargetPath: bundleAssetTargetPath(asset.assetKind, asset.path),
          registryContentHash: asset.contentHash,
          currentContentHash: asset.currentContentHash,
          status: asset.status,
        }),
      ),
    },
    notes: ["fingerprints are included for review and backup verification"],
  };
}

function optionalMemorySpaceExport(
  record: AdminProfileRegistryRecord,
): ProfileBundleExportEntry {
  return {
    targetPath: "memory-spaces/profile_dense.json",
    kind: "optional_memory_space_export",
    source: "memory_space_optional",
    contentJson: {
      profileId: record.profileId,
      included: false,
      reason:
        "profile_dense memory-space records are optional separate export data and are not merged into memory.md",
    },
    notes: [
      "dense runtime memory is intentionally separate from static memory.md prompt assets",
    ],
  };
}

function bundleAssetTargetPath(assetKind: string, path: string): string {
  const name = basename(path);
  switch (assetKind) {
    case "profile_yaml":
      return "profile.yaml";
    case "profile_json":
      return "profile.json";
    case "soul_md":
      return "soul.md";
    case "memory_md":
      return "memory.md";
    case "profile_readme":
      return "README.md";
    case "profile_local_skill":
      return `skills/${name}`;
    default:
      return `assets/${assetKind}/${name}`;
  }
}

function entryDiagnostics(
  record: AdminProfileRegistryRecord,
  entry: ProfileBundleExportEntry,
): NativeRuntimeConfigDiagnostic[] {
  if (
    entry.kind !== "copy_file_asset" ||
    (entry.assetStatus !== "missing" && entry.assetStatus !== "unknown")
  ) {
    return [];
  }
  return [
    {
      severity: entry.assetStatus === "missing" ? "error" : "warning",
      code:
        entry.assetStatus === "missing"
          ? "profile_export_asset_missing"
          : "profile_export_asset_unknown",
      path: `profiles.${record.profileId}.export.${entry.targetPath}`,
      message:
        entry.assetStatus === "missing"
          ? `profile export source asset is missing at ${entry.originPath}`
          : `profile export source asset could not be inspected at ${entry.originPath}`,
    },
  ];
}

function exportWarnings(
  record: AdminProfileRegistryRecord,
  entries: readonly ProfileBundleExportEntry[],
): string[] {
  const warnings: string[] = [];
  if (record.source === "file_fallback") {
    warnings.push(
      "profile is exported from file-backed fallback projection, not DB-authoritative registry state",
    );
  }
  if (entries.some((entry) => entry.assetStatus === "changed")) {
    warnings.push(
      "one or more source asset fingerprints differ from the registry snapshot",
    );
  }
  if (entries.some((entry) => entry.assetStatus === "missing")) {
    warnings.push("one or more source assets are missing and cannot be copied");
  }
  return warnings;
}

function stripUndefined(
  input: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(input).filter(([, value]) => value !== undefined),
  );
}
