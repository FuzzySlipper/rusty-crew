import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";
import type {
  NativeBridgeModule,
  NativeProfileRegistryRecord,
} from "@rusty-crew/native-bridge";
import type { McpBindingRecord, ProfileId } from "@rusty-crew/contracts";
import { buildProfileRegistryImportPlan } from "./profile-registry-import.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { NativeRuntimeConfigDiagnostic } from "@rusty-crew/native-bridge";
import { loadProfileConfig, type ProfileConfig } from "./profile-loading.js";

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
  providerAlias?: string;
  localToolProfileId?: string;
  toolPolicy?: {
    requestedToolsets?: string[];
    requestedTools?: string[];
    deniedTools?: string[];
    includeDeprecated?: boolean;
  };
  mcpBindings?: Array<{
    serverId: string;
    bindingId?: string;
    adapterId?: string;
    serverNames?: string[];
    transport?: string;
    toolProfileKey?: string;
  }>;
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
        registryAdminRecord(record, input.runtimeConfig),
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
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<AdminProfileRegistryRecord> {
  const sourceAssetStatuses = await assetStatuses(
    record.sourceAssetRefs,
    runtimeConfig.profilesDir,
  );
  const profile = await loadProfileConfig(
    runtimeConfig.profilesDir,
    record.profileId as ProfileId,
  ).catch(() => undefined);
  const runtime = runtimeConfigReadbackFromRegistry(
    record,
    runtimeConfig,
    profile,
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
    providerAlias: runtime.providerAlias,
    localToolProfileId: runtime.localToolProfileId,
    toolPolicy: runtime.toolPolicy,
    mcpBindings: runtime.mcpBindings,
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
    providerAlias: plan.profile.providerAlias,
    localToolProfileId: plan.profile.localToolProfileId,
    toolPolicy: adminToolPolicy(plan.profile.toolPolicy),
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

function runtimeConfigReadbackFromRegistry(
  record: NativeProfileRegistryRecord,
  runtimeConfig: RustyCrewRuntimeConfig,
  profile?: ProfileConfig,
): {
  providerAlias?: string;
  localToolProfileId?: string;
  toolPolicy?: AdminProfileRegistryRecord["toolPolicy"];
  mcpBindings?: AdminProfileRegistryRecord["mcpBindings"];
} {
  const settings = recordValue(record.activeRuntimeSettingsJson);
  const settingsProfile = profileConfigFromRegistrySettings(settings);
  const mcpBindings = runtimeConfig.mcpBindings
    .filter((binding) => String(binding.profileId) === record.profileId)
    .map(adminMcpBindingFromRuntime);
  return {
    providerAlias:
      stringValue(settings.providerAlias) ??
      stringValue(settings.provider_alias) ??
      profile?.providerAlias ??
      settingsProfile.providerAlias,
    localToolProfileId:
      stringValue(settings.localToolProfileId) ??
      stringValue(settings.local_tool_profile_id) ??
      profile?.localToolProfileId ??
      settingsProfile.localToolProfileId,
    toolPolicy:
      adminToolPolicy(profile?.toolPolicy) ??
      toolPolicyFromUnknown(settings.toolPolicy ?? settings.tool_policy) ??
      adminToolPolicy(settingsProfile.toolPolicy),
    mcpBindings:
      mcpBindings.length > 0
        ? mcpBindings
        : mcpBindingsFromSettings(
            settings.mcpBindings ?? settings.mcp_bindings,
          ),
  };
}

function profileConfigFromRegistrySettings(
  settings: Record<string, unknown>,
): Pick<ProfileConfig, "providerAlias" | "localToolProfileId" | "toolPolicy"> {
  const profile = recordValue(settings.profile);
  return {
    providerAlias: stringValue(profile.providerAlias),
    localToolProfileId: stringValue(profile.localToolProfileId),
    toolPolicy: toolPolicyFromUnknown(profile.toolPolicy),
  };
}

function adminMcpBindingFromRuntime(
  binding: McpBindingRecord,
): NonNullable<AdminProfileRegistryRecord["mcpBindings"]>[number] {
  return {
    serverId: serverIdFromMcpBinding(binding),
    bindingId: binding.bindingId,
    adapterId: String(binding.adapterId),
    serverNames: binding.serverNames,
    transport: binding.transport,
    toolProfileKey: binding.toolProfileKey,
  };
}

function mcpBindingsFromSettings(
  value: unknown,
): AdminProfileRegistryRecord["mcpBindings"] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.flatMap(
    (item): NonNullable<AdminProfileRegistryRecord["mcpBindings"]> => {
      const binding = recordValue(item);
      const serverId =
        stringValue(binding.serverId) ??
        serverIdFromEndpointRef(stringValue(binding.endpointRef)) ??
        stringList(binding.serverNames)?.[0];
      if (serverId === undefined) return [];
      return [
        {
          serverId,
          bindingId: stringValue(binding.bindingId),
          adapterId: stringValue(binding.adapterId),
          serverNames: stringList(binding.serverNames),
          transport: stringValue(binding.transport),
          toolProfileKey: stringValue(binding.toolProfileKey),
        },
      ];
    },
  );
}

function serverIdFromMcpBinding(binding: McpBindingRecord): string {
  return (
    serverIdFromEndpointRef(binding.endpointRef) ??
    binding.serverNames[0] ??
    binding.bindingId
  );
}

function serverIdFromEndpointRef(
  value: string | undefined,
): string | undefined {
  const prefix = "config://mcp/";
  return value?.startsWith(prefix) ? value.slice(prefix.length) : undefined;
}

function toolPolicyFromUnknown(
  value: unknown,
): AdminProfileRegistryRecord["toolPolicy"] | undefined {
  const policy = recordValue(value);
  if (Object.keys(policy).length === 0) return undefined;
  return {
    requestedToolsets: stringList(policy.requestedToolsets),
    requestedTools: stringList(policy.requestedTools),
    deniedTools: stringList(policy.deniedTools),
    includeDeprecated:
      typeof policy.includeDeprecated === "boolean"
        ? policy.includeDeprecated
        : undefined,
  };
}

function adminToolPolicy(
  policy: ProfileConfig["toolPolicy"],
): AdminProfileRegistryRecord["toolPolicy"] | undefined {
  if (policy === undefined) return undefined;
  return {
    requestedToolsets:
      policy.requestedToolsets === undefined
        ? undefined
        : [...policy.requestedToolsets],
    requestedTools:
      policy.requestedTools === undefined
        ? undefined
        : [...policy.requestedTools],
    deniedTools:
      policy.deniedTools === undefined ? undefined : [...policy.deniedTools],
    includeDeprecated: policy.includeDeprecated,
  };
}

function recordValue(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stringList(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter(
    (item): item is string => typeof item === "string" && item.trim() !== "",
  );
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
