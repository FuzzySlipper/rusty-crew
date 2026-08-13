import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";
import type {
  NativeBridgeModule,
  NativeModelConfigurationRecord,
  NativeModelEndpointRecord,
  NativeProfileRegistryRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import type {
  ExternalMessageDeliveryPolicy,
  McpBindingRecord,
  ProfileId,
} from "@rusty-crew/contracts";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { NativeRuntimeConfigDiagnostic } from "@rusty-crew/native-bridge";
import {
  contextStrategyPolicyFromUnknown,
  type ContextStrategyPolicy,
} from "./context-strategy.js";
import { parseExternalMessageDeliveryPolicy } from "./profile-loading.js";

export type AdminProfileRegistrySource = "registry";
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
  modelConfigId?: string;
  providerAlias?: string;
  modelDependencies?: AdminProfileModelDependencies;
  externalMessageDeliveryPolicy: ExternalMessageDeliveryPolicy;
  localToolProfileId?: string;
  toolPolicy?: {
    requestedToolsets?: string[];
    requestedTools?: string[];
    deniedTools?: string[];
    includeDeprecated?: boolean;
  };
  contextPolicy?: ContextStrategyPolicy;
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
  fallbackStatus: "registry_authoritative";
}

export interface AdminProfileModelDependencies {
  configuration?: NativeModelConfigurationRecord;
  endpoint?: NativeModelEndpointRecord;
  credential?: {
    credentialId: string;
    displayName: string;
    credentialKind: string;
    revision: number;
    hasSecret: boolean;
  };
}

export interface AdminProfileRegistryDiagnostics {
  generatedAt: string;
  records: AdminProfileRegistryRecord[];
  registryCount: number;
  missingRegistryRefCount: number;
  driftCount: number;
  missingAssetCount: number;
  diagnostics: NativeRuntimeConfigDiagnostic[];
}

export interface BuildAdminProfileRegistryDiagnosticsInput {
  bridge: Pick<NativeBridgeModule, "listProfileRegistryRecords"> &
    Partial<
      Pick<
        NativeBridgeModule,
        "getModelConfiguration" | "getModelEndpoint" | "getServiceCredential"
      >
    >;
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
  const missingRegistryDiagnostics = configuredProfileIds
    .filter((profileId) => !registryProfileIds.has(profileId))
    .map(
      (profileId): NativeRuntimeConfigDiagnostic => ({
        severity: "error",
        code: "profile_registry_record_missing",
        path: `profiles.${profileId}`,
        message:
          "profile is referenced by runtime config but has no DB-backed profile registry record",
      }),
    );
  const records = (
    await Promise.all(
      registryRecords.map((record) =>
        registryAdminRecord(record, input.runtimeConfig, input.bridge),
      ),
    )
  ).sort((left, right) => left.profileId.localeCompare(right.profileId));
  const diagnostics = [
    ...missingRegistryDiagnostics,
    ...records.flatMap((record) => record.diagnostics),
  ];
  return {
    generatedAt: input.now,
    records,
    registryCount: registryRecords.length,
    missingRegistryRefCount: missingRegistryDiagnostics.length,
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
  bridge: BuildAdminProfileRegistryDiagnosticsInput["bridge"],
): Promise<AdminProfileRegistryRecord> {
  const sourceAssetStatuses = await assetStatuses(
    record.sourceAssetRefs,
    runtimeConfig.profilesDir,
  );
  const runtime = runtimeConfigReadbackFromRegistry(record, runtimeConfig);
  const modelDependencies = await resolveAdminModelDependencies(
    runtime.modelConfigId,
    bridge,
  );
  const modelDiagnostics = adminModelDependencyDiagnostics(
    record.profileId,
    runtime.modelConfigId,
    modelDependencies,
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
    modelConfigId: runtime.modelConfigId,
    providerAlias: runtime.providerAlias,
    ...(modelDependencies === undefined ? {} : { modelDependencies }),
    externalMessageDeliveryPolicy: runtime.externalMessageDeliveryPolicy,
    localToolProfileId: runtime.localToolProfileId,
    toolPolicy: runtime.toolPolicy,
    contextPolicy: runtime.contextPolicy,
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
    diagnostics: [
      ...driftDiagnostics(record.profileId, sourceAssetStatuses),
      ...modelDiagnostics,
    ],
    fallbackStatus: "registry_authoritative",
  };
}

async function resolveAdminModelDependencies(
  modelConfigId: string | undefined,
  bridge: BuildAdminProfileRegistryDiagnosticsInput["bridge"],
): Promise<AdminProfileModelDependencies | undefined> {
  if (
    modelConfigId === undefined ||
    bridge.getModelConfiguration === undefined
  ) {
    return undefined;
  }
  const configuration = await bridge.getModelConfiguration(modelConfigId);
  if (configuration === undefined) return {};
  if (bridge.getModelEndpoint === undefined) return { configuration };
  const endpoint = await bridge.getModelEndpoint(configuration.endpointId);
  if (endpoint === undefined) return { configuration };
  if (
    endpoint.credentialId === undefined ||
    bridge.getServiceCredential === undefined
  ) {
    return { configuration, endpoint };
  }
  const credential = await bridge.getServiceCredential(endpoint.credentialId);
  return {
    configuration,
    endpoint,
    ...(credential === undefined
      ? {}
      : { credential: redactedCredentialDependency(credential) }),
  };
}

function redactedCredentialDependency(
  credential: NativeServiceCredentialRecord,
): NonNullable<AdminProfileModelDependencies["credential"]> {
  return {
    credentialId: credential.credentialId,
    displayName: credential.displayName,
    credentialKind: credential.credentialKind,
    revision: credential.revision,
    hasSecret: credential.credential.hasSecret,
  };
}

function adminModelDependencyDiagnostics(
  profileId: string,
  modelConfigId: string | undefined,
  dependencies: AdminProfileModelDependencies | undefined,
): NativeRuntimeConfigDiagnostic[] {
  if (modelConfigId === undefined || dependencies === undefined) return [];
  const path = `profiles.${profileId}.modelConfigId`;
  if (dependencies.configuration === undefined) {
    return [
      {
        severity: "error",
        code: "model_configuration_missing",
        path,
        message: `model configuration ${modelConfigId} is missing`,
      },
    ];
  }
  if (dependencies.endpoint === undefined) {
    return [
      {
        severity: "error",
        code: "model_endpoint_missing",
        path,
        message: `model configuration ${modelConfigId} references missing endpoint ${dependencies.configuration.endpointId}`,
      },
    ];
  }
  const endpoint = dependencies.endpoint;
  if (endpoint.authScheme !== "none" && endpoint.credentialId === undefined) {
    return [
      {
        severity: "error",
        code: "model_endpoint_credential_missing",
        path,
        message: `model endpoint ${endpoint.endpointId} requires ${endpoint.authScheme} but has no credential`,
      },
    ];
  }
  if (
    endpoint.credentialId !== undefined &&
    dependencies.credential === undefined
  ) {
    return [
      {
        severity: "error",
        code: "service_credential_missing",
        path,
        message: `model endpoint ${endpoint.endpointId} references missing credential ${endpoint.credentialId}`,
      },
    ];
  }
  if (
    endpoint.authScheme !== "none" &&
    dependencies.credential?.hasSecret === false
  ) {
    return [
      {
        severity: "error",
        code: "service_credential_secret_missing",
        path,
        message: `credential ${dependencies.credential.credentialId} has no secret for ${endpoint.authScheme}`,
      },
    ];
  }
  return [];
}

function runtimeConfigReadbackFromRegistry(
  record: NativeProfileRegistryRecord,
  runtimeConfig: RustyCrewRuntimeConfig,
): {
  modelConfigId?: string;
  providerAlias?: string;
  externalMessageDeliveryPolicy: ExternalMessageDeliveryPolicy;
  localToolProfileId?: string;
  toolPolicy?: AdminProfileRegistryRecord["toolPolicy"];
  contextPolicy?: ContextStrategyPolicy;
  mcpBindings?: AdminProfileRegistryRecord["mcpBindings"];
} {
  const settings = recordValue(record.activeRuntimeSettingsJson);
  const settingsProfile = profileConfigFromRegistrySettings(settings);
  const mcpBindings = runtimeConfig.mcpBindings
    .filter((binding) => String(binding.profileId) === record.profileId)
    .map(adminMcpBindingFromRuntime);
  return {
    externalMessageDeliveryPolicy: parseExternalMessageDeliveryPolicy(
      settings.externalMessageDeliveryPolicy ??
        settingsProfile.externalMessageDeliveryPolicy,
    ),
    modelConfigId:
      stringValue(settings.modelConfigId) ?? settingsProfile.modelConfigId,
    providerAlias:
      stringValue(settings.providerAlias) ??
      stringValue(settings.provider_alias) ??
      settingsProfile.providerAlias,
    localToolProfileId:
      stringValue(settings.localToolProfileId) ??
      stringValue(settings.local_tool_profile_id) ??
      settingsProfile.localToolProfileId,
    toolPolicy:
      toolPolicyFromUnknown(settings.toolPolicy ?? settings.tool_policy) ??
      settingsProfile.toolPolicy,
    contextPolicy:
      contextStrategyPolicyFromUnknown(
        settings.contextPolicy ?? settings.context_policy,
      ) ?? settingsProfile.contextPolicy,
    mcpBindings:
      mcpBindings.length > 0
        ? mcpBindings
        : mcpBindingsFromSettings(
            settings.mcpBindings ?? settings.mcp_bindings,
          ),
  };
}

function profileConfigFromRegistrySettings(settings: Record<string, unknown>): {
  modelConfigId?: string;
  providerAlias?: string;
  externalMessageDeliveryPolicy?: ExternalMessageDeliveryPolicy;
  localToolProfileId?: string;
  toolPolicy?: AdminProfileRegistryRecord["toolPolicy"];
  contextPolicy?: ContextStrategyPolicy;
} {
  const profile = recordValue(settings.profile);
  return {
    modelConfigId: stringValue(profile.modelConfigId),
    providerAlias: stringValue(profile.providerAlias),
    externalMessageDeliveryPolicy:
      profile.externalMessageDeliveryPolicy === undefined
        ? undefined
        : parseExternalMessageDeliveryPolicy(
            profile.externalMessageDeliveryPolicy,
          ),
    localToolProfileId: stringValue(profile.localToolProfileId),
    toolPolicy: toolPolicyFromUnknown(profile.toolPolicy),
    contextPolicy: contextStrategyPolicyFromUnknown(profile.contextPolicy),
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
