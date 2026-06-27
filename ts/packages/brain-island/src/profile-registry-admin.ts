import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";
import type {
  NativeBridgeModule,
  NativeProfileRegistryRecord,
} from "@rusty-crew/native-bridge";
import type { ProfileId } from "@rusty-crew/contracts";
import { buildProfileRegistryImportPlan } from "./profile-registry-import.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { NativeRuntimeConfigDiagnostic } from "@rusty-crew/native-bridge";

export type AdminProfileRegistrySource = "registry" | "file_fallback";
export type AdminProfileAssetStatus =
  | "tracked"
  | "missing"
  | "changed"
  | "unknown";

export interface AdminProfileRegistryAssetStatus {
  assetKind: string;
  path: string;
  contentHash?: string;
  currentContentHash?: string;
  status: AdminProfileAssetStatus;
  metadataJson?: unknown;
}

export interface AdminProfileRegistryRecord {
  source: AdminProfileRegistrySource;
  profileId: string;
  lifecycleStatus: string;
  displayName?: string;
  summary?: string;
  defaultSessionKind?: string;
  agentId?: string;
  ownerId?: string;
  promptSoulMarkdown?: string;
  promptMemoryMarkdown?: string;
  revision?: number;
  createdAt?: string;
  updatedAt?: string;
  importedFrom?: string;
  importedAt?: string;
  activeRuntimeRefs: NativeProfileRegistryRecord["derivedRuntimeRefs"];
  sourceAssetRefs: NativeProfileRegistryRecord["sourceAssetRefs"];
  sourceAssetStatuses: AdminProfileRegistryAssetStatus[];
  diagnostics: NativeRuntimeConfigDiagnostic[];
  fallbackStatus: "registry_authoritative" | "file_backed_fallback";
}

export interface AdminProfileRegistryDiagnostics {
  generatedAt: string;
  records: AdminProfileRegistryRecord[];
  registryCount: number;
  fileFallbackCount: number;
  driftCount: number;
  missingAssetCount: number;
  diagnostics: NativeRuntimeConfigDiagnostic[];
}

export interface BuildAdminProfileRegistryDiagnosticsInput {
  bridge: Pick<NativeBridgeModule, "listProfileRegistryRecords">;
  runtimeConfig: RustyCrewRuntimeConfig;
  now: string;
  profileIds?: readonly ProfileId[];
}

export async function buildAdminProfileRegistryDiagnostics(
  input: BuildAdminProfileRegistryDiagnosticsInput,
): Promise<AdminProfileRegistryDiagnostics> {
  const registryRecords = await input.bridge.listProfileRegistryRecords({
    limit: 1_000,
  });
  const registryProfileIds = new Set(
    registryRecords.map((record) => record.profileId),
  );
  const configuredProfileIds =
    input.profileIds ?? profileIdsFromRuntimeConfig(input.runtimeConfig);
  const fallbackPlans = await Promise.all(
    configuredProfileIds
      .filter((profileId) => !registryProfileIds.has(profileId))
      .map(async (profileId) => {
        try {
          return await buildProfileRegistryImportPlan({
            profilesDir: input.runtimeConfig.profilesDir,
            profileId,
            now: input.now,
          });
        } catch (error) {
          return {
            profileId,
            error,
          };
        }
      }),
  );
  const records = [
    ...(await Promise.all(
      registryRecords.map((record) =>
        registryAdminRecord(record, input.runtimeConfig.profilesDir),
      ),
    )),
    ...(await Promise.all(
      fallbackPlans.flatMap((fallback) =>
        "registryWrite" in fallback
          ? [fallbackAdminRecord(fallback)]
          : [
              missingFallbackRecord(
                fallback.profileId,
                fallback.error,
                input.now,
              ),
            ],
      ),
    )),
  ].sort((left, right) => left.profileId.localeCompare(right.profileId));
  const diagnostics = records.flatMap((record) => record.diagnostics);
  return {
    generatedAt: input.now,
    records,
    registryCount: registryRecords.length,
    fileFallbackCount: records.filter(
      (record) => record.source === "file_fallback",
    ).length,
    driftCount: records.filter((record) =>
      record.sourceAssetStatuses.some((asset) => asset.status === "changed"),
    ).length,
    missingAssetCount: records.filter((record) =>
      record.sourceAssetStatuses.some((asset) => asset.status === "missing"),
    ).length,
    diagnostics,
  };
}

export function filterAdminProfileRegistryRecords(
  records: readonly AdminProfileRegistryRecord[],
  url: URL,
): AdminProfileRegistryRecord[] {
  const lifecycleStatus = url.searchParams.get("lifecycle_status");
  const source = url.searchParams.get("source");
  const fallbackStatus = url.searchParams.get("fallback_status");
  return records.filter(
    (record) =>
      (lifecycleStatus === null ||
        record.lifecycleStatus === lifecycleStatus) &&
      (source === null || record.source === source) &&
      (fallbackStatus === null || record.fallbackStatus === fallbackStatus),
  );
}

async function registryAdminRecord(
  record: NativeProfileRegistryRecord,
  profilesDir: string,
): Promise<AdminProfileRegistryRecord> {
  const sourceAssetStatuses = await assetStatuses(
    record.sourceAssetRefs,
    profilesDir,
  );
  return {
    source: "registry",
    profileId: record.profileId,
    lifecycleStatus: record.lifecycleStatus,
    displayName: record.displayName,
    summary: record.summary,
    defaultSessionKind: record.defaultSessionKind,
    agentId: record.agentId,
    ownerId: record.ownerId,
    promptSoulMarkdown: record.promptSoulMarkdown,
    promptMemoryMarkdown: record.promptMemoryMarkdown,
    revision: record.revision,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
    importedFrom: record.importExport.importedFrom,
    importedAt: record.importExport.importedAt,
    activeRuntimeRefs: record.derivedRuntimeRefs,
    sourceAssetRefs: record.sourceAssetRefs,
    sourceAssetStatuses,
    diagnostics: driftDiagnostics(record.profileId, sourceAssetStatuses),
    fallbackStatus: "registry_authoritative",
  };
}

async function fallbackAdminRecord(
  plan: Awaited<ReturnType<typeof buildProfileRegistryImportPlan>>,
): Promise<AdminProfileRegistryRecord> {
  const sourceAssetStatuses = await assetStatuses(
    plan.registryWrite.sourceAssetRefs.map((ref) => ({
      assetKind: ref.assetKind,
      path: ref.path,
      contentHash: ref.contentHash,
      lastSeenAt: ref.lastSeenAt,
      metadataJson: ref.metadataJson,
    })),
  );
  return {
    source: "file_fallback",
    profileId: plan.profile.profileId,
    lifecycleStatus: plan.registryWrite.lifecycleStatus,
    displayName: plan.registryWrite.displayName,
    summary: plan.registryWrite.summary,
    defaultSessionKind: plan.registryWrite.defaultSessionKind,
    agentId: plan.registryWrite.agentId,
    ownerId: plan.registryWrite.ownerId,
    promptSoulMarkdown: plan.registryWrite.promptSoulMarkdown,
    promptMemoryMarkdown: plan.registryWrite.promptMemoryMarkdown,
    importedFrom: plan.registryWrite.importExport.importedFrom,
    importedAt: plan.registryWrite.importExport.importedAt,
    activeRuntimeRefs: plan.registryWrite.derivedRuntimeRefs,
    sourceAssetRefs: plan.registryWrite.sourceAssetRefs.map((ref) => ({
      assetKind: ref.assetKind,
      path: ref.path,
      contentHash: ref.contentHash,
      lastSeenAt: ref.lastSeenAt,
      metadataJson: ref.metadataJson,
    })),
    sourceAssetStatuses,
    diagnostics: [
      {
        severity: "info",
        code: "file_backed_profile_fallback",
        path: `profiles.${plan.profile.profileId}`,
        message:
          "profile is currently available through file-backed compatibility loading and has no DB registry record",
      },
      ...plan.diagnostics,
    ],
    fallbackStatus: "file_backed_fallback",
  };
}

async function missingFallbackRecord(
  profileId: ProfileId,
  error: unknown,
  now: string,
): Promise<AdminProfileRegistryRecord> {
  return {
    source: "file_fallback",
    profileId,
    lifecycleStatus: "missing",
    activeRuntimeRefs: [],
    sourceAssetRefs: [],
    sourceAssetStatuses: [],
    diagnostics: [
      {
        severity: "error",
        code: "file_backed_profile_missing",
        path: `profiles.${profileId}`,
        message: error instanceof Error ? error.message : String(error),
      },
    ],
    fallbackStatus: "file_backed_fallback",
    updatedAt: now,
  };
}

async function assetStatuses(
  refs: readonly {
    assetKind: string;
    path: string;
    contentHash?: string;
    metadataJson?: unknown;
  }[],
  relativeBaseDir?: string,
): Promise<AdminProfileRegistryAssetStatus[]> {
  return Promise.all(
    refs.map(async (ref) => {
      const assetPath =
        relativeBaseDir === undefined || isAbsolute(ref.path)
          ? ref.path
          : resolve(relativeBaseDir, ref.path);
      try {
        const raw = await readFile(assetPath);
        const currentContentHash = `sha256:${createHash("sha256")
          .update(raw)
          .digest("hex")}`;
        return {
          assetKind: ref.assetKind,
          path: ref.path,
          contentHash: ref.contentHash,
          currentContentHash,
          status:
            ref.contentHash === undefined ||
            ref.contentHash === currentContentHash
              ? "tracked"
              : "changed",
          metadataJson: ref.metadataJson,
        };
      } catch (error) {
        if (isNodeError(error) && error.code === "ENOENT") {
          return {
            assetKind: ref.assetKind,
            path: ref.path,
            contentHash: ref.contentHash,
            status: "missing",
            metadataJson: ref.metadataJson,
          };
        }
        return {
          assetKind: ref.assetKind,
          path: ref.path,
          contentHash: ref.contentHash,
          status: "unknown",
          metadataJson: ref.metadataJson,
        };
      }
    }),
  );
}

function driftDiagnostics(
  profileId: string,
  assets: readonly AdminProfileRegistryAssetStatus[],
): NativeRuntimeConfigDiagnostic[] {
  return assets
    .filter((asset) => asset.status === "changed" || asset.status === "missing")
    .map((asset) => ({
      severity: asset.status === "missing" ? "error" : "warning",
      code:
        asset.status === "missing"
          ? "profile_registry_asset_missing"
          : "profile_registry_asset_drift",
      path: `profiles.${profileId}.assets.${asset.assetKind}`,
      message:
        asset.status === "missing"
          ? `profile registry asset is missing at ${asset.path}`
          : `profile registry asset fingerprint changed at ${asset.path}`,
    }));
}

function profileIdsFromRuntimeConfig(
  runtimeConfig: RustyCrewRuntimeConfig,
): ProfileId[] {
  return [
    ...new Set(
      [
        ...runtimeConfig.brains.map((brain) => brain.profileId),
        ...runtimeConfig.sessions.map((session) => session.profileId),
        ...runtimeConfig.channelBindings.map((binding) => binding.profileId),
        ...runtimeConfig.mcpBindings.map((binding) => binding.profileId),
      ].filter((profileId): profileId is ProfileId => profileId !== undefined),
    ),
  ].sort();
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
