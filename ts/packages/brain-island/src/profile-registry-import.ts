import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { basename, join, relative, sep } from "node:path";
import type { ProfileId, SessionKind } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeRuntimeConfigDiagnostic,
} from "@rusty-crew/native-bridge";
import {
  loadProfileConfigWithSource,
  type LoadedProfileConfigSource,
  type ProfileConfig,
} from "./profile-loading.js";
import { validateRuntimeConfigWithRust } from "./runtime-config-validation.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

export type ProfileRegistryLifecycleStatus =
  | "active"
  | "paused"
  | "decommissioned"
  | "archived";

export type ProfileRegistryImportMode = "template" | "activation";

export interface ProfileRegistrySourceAssetRefDraft {
  assetKind: string;
  path: string;
  contentHash?: string;
  lastSeenAt?: string;
  metadataJson: Record<string, unknown>;
}

export interface ProfileRegistryDerivedRuntimeRefDraft {
  refKind: string;
  refId: string;
  status: string;
  updatedAt?: string;
  metadataJson: Record<string, unknown>;
}

export interface ProfileRegistryImportExportMetadataDraft {
  importedFrom?: string;
  importedAt?: string;
  exportedTo?: string;
  exportedAt?: string;
  metadataJson: Record<string, unknown>;
}

export interface ProfileRegistryWriteDraft {
  profileId: ProfileId;
  lifecycleStatus: ProfileRegistryLifecycleStatus;
  displayName?: string;
  summary?: string;
  defaultSessionKind?: SessionKind;
  agentId?: string;
  ownerId?: string;
  promptSoulMarkdown?: string;
  promptMemoryMarkdown?: string;
  activeRuntimeSettingsJson: Record<string, unknown>;
  sourceAssetRefs: ProfileRegistrySourceAssetRefDraft[];
  derivedRuntimeRefs: ProfileRegistryDerivedRuntimeRefDraft[];
  importExport: ProfileRegistryImportExportMetadataDraft;
  now: string;
}

export interface ProfileRegistryImportPlan {
  profile: ProfileConfig;
  mode: ProfileRegistryImportMode;
  activatesRuntime: boolean;
  sourceFormat: LoadedProfileConfigSource["sourceFormat"];
  profilePath: string;
  profileDir: string;
  registryWrite: ProfileRegistryWriteDraft;
  diagnostics: NativeRuntimeConfigDiagnostic[];
}

export interface BuildProfileRegistryImportPlanInput {
  profilesDir: string;
  profileId: ProfileId;
  mode?: ProfileRegistryImportMode;
  now?: string;
  runtimeConfig?: RustyCrewRuntimeConfig;
  existingProfiles?: readonly ProfileConfig[];
  bridge?: Pick<NativeBridgeModule, "validateRuntimeConfigDraft"> &
    Partial<
      Pick<
        NativeBridgeModule,
        "getModelConfiguration" | "getModelEndpoint" | "getServiceCredential"
      >
    >;
}

export async function buildProfileRegistryImportPlan(
  input: BuildProfileRegistryImportPlanInput,
): Promise<ProfileRegistryImportPlan> {
  const mode = input.mode ?? "template";
  const now = input.now ?? new Date().toISOString();
  const source = await loadProfileConfigWithSource(
    input.profilesDir,
    input.profileId,
  );
  const diagnostics = [
    ...profileFieldDiagnostics(source),
    ...activationDiagnostics(mode, input),
    ...(await modelDependencyDiagnostics(source.profile, input.bridge)),
    ...(await modelDependencyArtifactDiagnostics(source, input.bridge)),
  ];
  if (input.runtimeConfig && input.bridge) {
    const profiles = mergeProfileForValidation(
      input.existingProfiles ?? [],
      source.profile,
    );
    const validation = await validateRuntimeConfigWithRust({
      bridge: input.bridge,
      runtimeConfig: input.runtimeConfig,
      profiles,
    });
    diagnostics.push(...validation.diagnostics);
  }

  const sourceAssetRefs = await collectProfileSourceAssetRefs(source, now);
  return {
    profile: source.profile,
    mode,
    activatesRuntime: mode === "activation",
    sourceFormat: source.sourceFormat,
    profilePath: source.profilePath,
    profileDir: source.profileDir,
    registryWrite: {
      profileId: source.profile.profileId,
      lifecycleStatus: mode === "activation" ? "active" : "paused",
      displayName: source.profile.displayName,
      summary: profileImportSummary(source),
      defaultSessionKind: "full",
      ownerId: source.profile.sessionDefaults?.ownerId,
      promptSoulMarkdown: source.profile.prompt?.soulMarkdown,
      promptMemoryMarkdown: source.profile.prompt?.memoryMarkdown,
      activeRuntimeSettingsJson: activeRuntimeSettingsJson(source.profile),
      sourceAssetRefs,
      derivedRuntimeRefs:
        mode === "activation" ? derivedRuntimeRefs(source.profile, now) : [],
      importExport: {
        importedFrom: source.sourceFormat,
        importedAt: now,
        metadataJson: {
          importMode: mode,
          activatesRuntime: mode === "activation",
          profilePath: source.profilePath,
        },
      },
      now,
    },
    diagnostics,
  };
}

async function modelDependencyDiagnostics(
  profile: ProfileConfig,
  bridge: BuildProfileRegistryImportPlanInput["bridge"],
): Promise<NativeRuntimeConfigDiagnostic[]> {
  const modelConfigId = profile.modelConfigId;
  if (modelConfigId === undefined) return [];
  if (
    bridge?.getModelConfiguration === undefined ||
    bridge.getModelEndpoint === undefined ||
    bridge.getServiceCredential === undefined
  ) {
    return [
      {
        severity: "warning",
        code: "model_dependency_validation_unavailable",
        path: "modelConfigId",
        message:
          "model dependency readers were not supplied; import cannot dry-run configuration, endpoint, auth, or secret references",
      },
    ];
  }
  const configuration = await bridge.getModelConfiguration(modelConfigId);
  if (configuration === undefined) {
    return [
      {
        severity: "error",
        code: "model_configuration_missing",
        path: "modelConfigId",
        message: `model configuration ${modelConfigId} was not found`,
      },
    ];
  }
  const diagnostics: NativeRuntimeConfigDiagnostic[] = [];
  if (configuration.revision < 1 || configuration.capabilities.version !== 1) {
    diagnostics.push({
      severity: "error",
      code: "model_configuration_version_unsupported",
      path: "modelConfigId",
      message: `model configuration ${modelConfigId} has revision ${configuration.revision} and capabilities version ${configuration.capabilities.version}; positive revision and capabilities version 1 are required`,
    });
  }
  const endpoint = await bridge.getModelEndpoint(configuration.endpointId);
  if (endpoint === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_endpoint_missing",
      path: "modelConfigId",
      message: `model configuration ${modelConfigId} references missing endpoint ${configuration.endpointId}`,
    });
    return diagnostics;
  }
  if (endpoint.revision < 1) {
    diagnostics.push({
      severity: "error",
      code: "model_endpoint_version_unsupported",
      path: "modelConfigId",
      message: `model endpoint ${endpoint.endpointId} has invalid revision ${endpoint.revision}`,
    });
  }
  if (endpoint.authScheme === "none") return diagnostics;
  if (endpoint.credentialId === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_endpoint_credential_missing",
      path: "modelConfigId",
      message: `model endpoint ${endpoint.endpointId} requires ${endpoint.authScheme} but has no credential reference`,
    });
    return diagnostics;
  }
  const credential = await bridge.getServiceCredential(endpoint.credentialId);
  if (credential === undefined) {
    diagnostics.push({
      severity: "error",
      code: "service_credential_missing",
      path: "modelConfigId",
      message: `model endpoint ${endpoint.endpointId} references missing credential ${endpoint.credentialId}`,
    });
    return diagnostics;
  }
  const expectedCredentialKind =
    endpoint.authScheme === "openai_codex_oauth" ? "openai_oauth" : "api_key";
  if (
    credential.credentialKind !== expectedCredentialKind &&
    !(
      expectedCredentialKind === "api_key" &&
      credential.credentialKind === "legacy_raw_api_key"
    )
  ) {
    diagnostics.push({
      severity: "error",
      code: "model_endpoint_auth_incompatible",
      path: "modelConfigId",
      message: `model endpoint ${endpoint.endpointId} uses ${endpoint.authScheme} but credential ${credential.credentialId} is ${credential.credentialKind}`,
    });
  }
  if (!credential.credential.hasSecret) {
    diagnostics.push({
      severity: "error",
      code: "service_credential_secret_missing",
      path: "modelConfigId",
      message: `credential ${credential.credentialId} has no secret`,
    });
  }
  return diagnostics;
}

interface LoadedDependencyArtifact {
  path: string;
  value?: unknown;
  error?: string;
}

async function modelDependencyArtifactDiagnostics(
  source: LoadedProfileConfigSource,
  bridge: BuildProfileRegistryImportPlanInput["bridge"],
): Promise<NativeRuntimeConfigDiagnostic[]> {
  const paths = {
    configuration: join(
      source.profileDir,
      "dependencies",
      "model-configuration.json",
    ),
    endpoint: join(source.profileDir, "dependencies", "model-endpoint.json"),
    credential: join(
      source.profileDir,
      "dependencies",
      "credential-reference.json",
    ),
    checksums: join(source.profileDir, "checksums.json"),
  };
  const [configuration, endpoint, credential, checksums] = await Promise.all([
    readDependencyArtifact(paths.configuration),
    readDependencyArtifact(paths.endpoint),
    readDependencyArtifact(paths.credential),
    readDependencyArtifact(paths.checksums),
  ]);
  const artifacts = { configuration, endpoint, credential, checksums };
  if (Object.values(artifacts).every((artifact) => artifact === undefined)) {
    return [];
  }

  const diagnostics: NativeRuntimeConfigDiagnostic[] = [];
  for (const artifact of Object.values(artifacts)) {
    if (artifact?.error !== undefined) {
      diagnostics.push({
        severity: "error",
        code: "model_dependency_artifact_invalid",
        path: relative(source.profileDir, artifact.path),
        message: artifact.error,
      });
    }
  }
  if (diagnostics.length > 0) return diagnostics;
  if (source.profile.modelConfigId === undefined) {
    if (
      configuration?.value !== undefined ||
      endpoint?.value !== undefined ||
      credential?.value !== undefined
    ) {
      diagnostics.push({
        severity: "error",
        code: "model_dependency_artifact_reference_mismatch",
        path: "dependencies",
        message:
          "portable model dependency artifacts were supplied for a profile without modelConfigId",
      });
    }
    return diagnostics;
  }
  if (configuration?.value === undefined || endpoint?.value === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_dependency_artifact_missing",
      path: "dependencies",
      message:
        "portable model dependencies require both model-configuration.json and model-endpoint.json",
    });
    return diagnostics;
  }
  if (checksums?.value === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_dependency_artifact_checksum_missing",
      path: "checksums.json",
      message:
        "portable model dependency artifacts require checksums.json proof",
    });
  }
  if (
    bridge?.getModelConfiguration === undefined ||
    bridge.getModelEndpoint === undefined ||
    bridge.getServiceCredential === undefined
  ) {
    return [
      {
        severity: "error",
        code: "model_dependency_artifact_validation_unavailable",
        path: "dependencies",
        message:
          "portable model dependency artifacts require configuration, endpoint, and credential readers",
      },
    ];
  }

  const expectedConfiguration = asRecord(configuration.value);
  const expectedEndpoint = asRecord(endpoint.value);
  const modelConfigId = stringField(expectedConfiguration, "modelConfigId");
  const endpointId = stringField(expectedConfiguration, "endpointId");
  const exportedEndpointId = stringField(expectedEndpoint, "endpointId");
  if (
    modelConfigId === undefined ||
    endpointId === undefined ||
    exportedEndpointId !== endpointId ||
    modelConfigId !== source.profile.modelConfigId
  ) {
    diagnostics.push({
      severity: "error",
      code: "model_dependency_artifact_reference_mismatch",
      path: "dependencies",
      message:
        "portable model dependency artifact identities do not match the imported profile reference chain",
    });
    return diagnostics;
  }

  const currentConfiguration =
    await bridge.getModelConfiguration(modelConfigId);
  const currentEndpoint = await bridge.getModelEndpoint(endpointId);
  compareDependencyArtifact(
    diagnostics,
    "model_configuration",
    configuration.value,
    currentConfiguration,
  );
  compareDependencyArtifact(
    diagnostics,
    "model_endpoint",
    endpoint.value,
    currentEndpoint,
  );

  const credentialId = stringField(expectedEndpoint, "credentialId");
  if (credentialId !== undefined) {
    if (credential?.value === undefined) {
      diagnostics.push({
        severity: "error",
        code: "model_dependency_artifact_missing",
        path: "dependencies/credential-reference.json",
        message: `endpoint ${endpointId} requires credential reference ${credentialId}`,
      });
    } else {
      const expectedCredential = asRecord(credential.value);
      if (stringField(expectedCredential, "credentialId") !== credentialId) {
        diagnostics.push({
          severity: "error",
          code: "model_dependency_artifact_reference_mismatch",
          path: "dependencies/credential-reference.json",
          message: `credential dependency does not match endpoint reference ${credentialId}`,
        });
      } else {
        const currentCredential =
          await bridge.getServiceCredential(credentialId);
        compareDependencyArtifact(
          diagnostics,
          "credential_reference",
          credential.value,
          currentCredential === undefined
            ? undefined
            : {
                credentialId: currentCredential.credentialId,
                displayName: currentCredential.displayName,
                credentialKind: currentCredential.credentialKind,
                revision: currentCredential.revision,
                hasSecret: currentCredential.credential.hasSecret,
              },
        );
      }
    }
  }

  validateDeclaredDependencyChecksums(diagnostics, checksums?.value, artifacts);
  return diagnostics;
}

async function readDependencyArtifact(
  path: string,
): Promise<LoadedDependencyArtifact | undefined> {
  let raw: string;
  try {
    raw = await readFile(path, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return undefined;
    return { path, error: `could not read dependency artifact ${path}` };
  }
  try {
    return { path, value: JSON.parse(raw) as unknown };
  } catch {
    return { path, error: `dependency artifact ${path} is not valid JSON` };
  }
}

function compareDependencyArtifact(
  diagnostics: NativeRuntimeConfigDiagnostic[],
  kind: string,
  expected: unknown,
  current: unknown,
): void {
  if (
    current === undefined ||
    dependencyChecksum(expected) !== dependencyChecksum(current)
  ) {
    diagnostics.push({
      severity: "error",
      code: "model_dependency_artifact_drift",
      path: `dependencies/${kind}`,
      message: `${kind} does not match the exported dependency snapshot`,
    });
  }
}

function validateDeclaredDependencyChecksums(
  diagnostics: NativeRuntimeConfigDiagnostic[],
  checksumsValue: unknown,
  artifacts: Record<string, LoadedDependencyArtifact | undefined>,
): void {
  if (checksumsValue === undefined) return;
  const dependencyChecksums = asRecord(
    asRecord(checksumsValue).modelDependencies,
  );
  for (const [artifactKey, checksumKey] of [
    ["configuration", "modelConfiguration"],
    ["endpoint", "modelEndpoint"],
    ["credential", "credentialReference"],
  ] as const) {
    const artifact = artifacts[artifactKey]?.value;
    const declared = stringField(dependencyChecksums, checksumKey);
    if (artifact !== undefined && declared !== dependencyChecksum(artifact)) {
      diagnostics.push({
        severity: "error",
        code: "model_dependency_artifact_checksum_mismatch",
        path: `checksums.json:modelDependencies.${checksumKey}`,
        message: `declared ${checksumKey} checksum does not match its dependency artifact`,
      });
    }
  }
}

function dependencyChecksum(value: unknown): string {
  return `sha256:${createHash("sha256").update(stableJson(value)).digest("hex")}`;
}

function stableJson(value: unknown): string {
  if (value === undefined) return "null";
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, entry]) => `${JSON.stringify(key)}:${stableJson(entry)}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function asRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function stringField(
  value: Record<string, unknown>,
  key: string,
): string | undefined {
  return typeof value[key] === "string" ? value[key] : undefined;
}

function activeRuntimeSettingsJson(
  profile: ProfileConfig,
): Record<string, unknown> {
  return stripUndefined({
    schemaVersion: 1,
    modelConfigId: profile.modelConfigId,
    ...(profile.modelConfigId === undefined
      ? { modelConfig: profile.modelConfig }
      : {}),
    externalMessageDeliveryPolicy: profile.externalMessageDeliveryPolicy,
    brain: profile.brain,
    runtime: profile.runtime,
    toolPolicy: profile.toolPolicy,
    skills: profile.skills,
    skillsMode: profile.skillsMode,
    mcpConfig: profile.mcpConfig,
    backgroundReview: profile.backgroundReview,
    memoryConfig: profile.memoryConfig,
    roleplayNarrator: profile.roleplayNarrator,
    roleplayMechanic: profile.roleplayMechanic,
    sessionDefaults: profile.sessionDefaults,
    channelDefaults: profile.channelDefaults,
  });
}

function derivedRuntimeRefs(
  profile: ProfileConfig,
  now: string,
): ProfileRegistryDerivedRuntimeRefDraft[] {
  const refs: ProfileRegistryDerivedRuntimeRefDraft[] = [
    derivedRef("brain", `brain:${profile.profileId}`, now),
    derivedRef("session", `session:${profile.profileId}`, now),
  ];
  if (profile.mcpConfig) {
    refs.push(
      derivedRef(
        "mcp_binding",
        profile.mcpConfig.bindingId ?? `mcp:${profile.profileId}`,
        now,
      ),
    );
  }
  if (profile.backgroundReview?.enabled) {
    refs.push(
      derivedRef(
        "scheduled_job",
        `background-review-${profile.profileId}`,
        now,
      ),
    );
  }
  if (profile.channelDefaults?.wakePolicy) {
    refs.push(
      derivedRef("channel_binding", `channel:${profile.profileId}`, now),
    );
  }
  return refs;
}

function derivedRef(
  refKind: string,
  refId: string,
  now: string,
): ProfileRegistryDerivedRuntimeRefDraft {
  return {
    refKind,
    refId,
    status: "planned",
    updatedAt: now,
    metadataJson: {
      importProjection: true,
    },
  };
}

async function collectProfileSourceAssetRefs(
  source: LoadedProfileConfigSource,
  now: string,
): Promise<ProfileRegistrySourceAssetRefDraft[]> {
  const refs: ProfileRegistrySourceAssetRefDraft[] = [
    await fileAssetRef(
      source.sourceFormat === "flat_json" ? "profile_json" : "profile_yaml",
      source.profilePath,
      now,
      {
        sourceFormat: source.sourceFormat,
      },
    ),
  ];
  if (source.sourceFormat === "directory_yaml") {
    refs.push(
      ...(await optionalFileAssetRefs(source.profileDir, now, [
        ["soul_md", "soul.md"],
        ["memory_md", "memory.md"],
        ["profile_readme", "README.md"],
      ])),
    );
    refs.push(
      ...(await collectNestedFileAssetRefs(
        "profile_local_skill",
        join(source.profileDir, "skills"),
        now,
      )),
    );
    refs.push(
      ...(await collectNestedFileAssetRefs(
        "template_file",
        join(source.profileDir, "templates"),
        now,
      )),
    );
    refs.push(
      ...(await collectNestedFileAssetRefs(
        "template_file",
        join(source.profileDir, "template"),
        now,
      )),
    );
  }
  return dedupeAssetRefs(refs);
}

async function optionalFileAssetRefs(
  root: string,
  now: string,
  files: readonly (readonly [string, string])[],
): Promise<ProfileRegistrySourceAssetRefDraft[]> {
  const refs: ProfileRegistrySourceAssetRefDraft[] = [];
  for (const [assetKind, filename] of files) {
    const path = join(root, filename);
    const ref = await maybeFileAssetRef(assetKind, path, now, {
      filename,
    });
    if (ref) refs.push(ref);
  }
  return refs;
}

async function collectNestedFileAssetRefs(
  assetKind: string,
  root: string,
  now: string,
): Promise<ProfileRegistrySourceAssetRefDraft[]> {
  const paths = await collectFiles(root, 0);
  return Promise.all(
    paths.map((path) =>
      fileAssetRef(assetKind, path, now, {
        relativePath: relative(root, path).split(sep).join("/"),
        root,
      }),
    ),
  );
}

async function collectFiles(root: string, depth: number): Promise<string[]> {
  if (depth > 6) return [];
  let entries: Array<{
    isDirectory(): boolean;
    isFile(): boolean;
    name: string;
  }>;
  try {
    entries = await readdir(root, { withFileTypes: true });
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return [];
    throw error;
  }
  const files: string[] = [];
  for (const entry of entries) {
    if (entry.name.startsWith(".")) continue;
    const child = join(root, entry.name);
    if (entry.isFile()) {
      files.push(child);
    } else if (entry.isDirectory()) {
      files.push(...(await collectFiles(child, depth + 1)));
    }
  }
  return files.sort();
}

async function maybeFileAssetRef(
  assetKind: string,
  path: string,
  now: string,
  metadataJson: Record<string, unknown>,
): Promise<ProfileRegistrySourceAssetRefDraft | undefined> {
  try {
    return await fileAssetRef(assetKind, path, now, metadataJson);
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return undefined;
    throw error;
  }
}

async function fileAssetRef(
  assetKind: string,
  path: string,
  now: string,
  metadataJson: Record<string, unknown>,
): Promise<ProfileRegistrySourceAssetRefDraft> {
  const raw = await readFile(path);
  return {
    assetKind,
    path,
    contentHash: `sha256:${createHash("sha256").update(raw).digest("hex")}`,
    lastSeenAt: now,
    metadataJson: {
      basename: basename(path),
      ...metadataJson,
    },
  };
}

function dedupeAssetRefs(
  refs: readonly ProfileRegistrySourceAssetRefDraft[],
): ProfileRegistrySourceAssetRefDraft[] {
  const seen = new Set<string>();
  return refs.filter((ref) => {
    const key = `${ref.assetKind}\0${ref.path}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function profileFieldDiagnostics(
  source: LoadedProfileConfigSource,
): NativeRuntimeConfigDiagnostic[] {
  return [
    ...unsupportedFieldDiagnostics(source.rawProfileConfig, "", PROFILE_SCHEMA),
    ...ambiguousFieldDiagnostics(source),
  ];
}

function activationDiagnostics(
  mode: ProfileRegistryImportMode,
  input: BuildProfileRegistryImportPlanInput,
): NativeRuntimeConfigDiagnostic[] {
  if (mode !== "activation") return [];
  if (input.runtimeConfig && input.bridge) return [];
  return [
    {
      severity: "warning",
      code: "activation_without_rust_validation",
      path: "import.mode",
      message:
        "activation import requested without runtimeConfig and bridge; plan records derived refs but cannot Rust-validate the runtime graph",
    },
  ];
}

function ambiguousFieldDiagnostics(
  source: LoadedProfileConfigSource,
): NativeRuntimeConfigDiagnostic[] {
  const raw = source.rawProfileConfig;
  const diagnostics: NativeRuntimeConfigDiagnostic[] = [];
  const profileId = optionalString(raw.profileId);
  const profileIdentity = optionalString(raw.profileIdentity);
  if (
    profileId !== undefined &&
    profileIdentity !== undefined &&
    profileId !== profileIdentity
  ) {
    diagnostics.push(
      ambiguous("profileIdentity", "profileId and profileIdentity differ"),
    );
  }
  const modelConfigId = optionalString(raw.modelConfigId);
  const providerAlias = optionalString(raw.providerAlias);
  if (
    modelConfigId !== undefined &&
    providerAlias !== undefined &&
    modelConfigId !== providerAlias
  ) {
    diagnostics.push(
      ambiguous(
        "modelConfigId",
        "modelConfigId and compatibility-only providerAlias differ",
      ),
    );
  }
  const modelConfig = record(raw.modelConfig);
  if (modelConfig) {
    const modelName = optionalString(modelConfig.modelName);
    const model = optionalString(modelConfig.model);
    if (modelName !== undefined && model !== undefined && modelName !== model) {
      diagnostics.push(
        ambiguous("modelConfig.model", "modelName and model differ"),
      );
    }
    const maxOutputTokens = optionalNumber(modelConfig.maxOutputTokens);
    const maxTokens = optionalNumber(modelConfig.maxTokens);
    if (
      maxOutputTokens !== undefined &&
      maxTokens !== undefined &&
      maxOutputTokens !== maxTokens
    ) {
      diagnostics.push(
        ambiguous(
          "modelConfig.maxTokens",
          "maxOutputTokens and maxTokens differ",
        ),
      );
    }
  }
  if (record(raw.runtime) && record(raw.runtimeConfig)) {
    diagnostics.push(
      ambiguous(
        "runtimeConfig",
        "runtime and runtimeConfig are both present; runtime takes precedence",
      ),
    );
  }
  const prompt = record(raw.prompt);
  if (
    source.soulMarkdown !== undefined &&
    optionalString(prompt?.soulMarkdown) !== undefined
  ) {
    diagnostics.push(
      ambiguous(
        "prompt.soulMarkdown",
        "directory soul.md and prompt.soulMarkdown are both present; soul.md takes precedence",
      ),
    );
  }
  if (
    source.memoryMarkdown !== undefined &&
    optionalString(prompt?.memoryMarkdown) !== undefined
  ) {
    diagnostics.push(
      ambiguous(
        "prompt.memoryMarkdown",
        "directory memory.md and prompt.memoryMarkdown are both present; memory.md takes precedence",
      ),
    );
  }
  return diagnostics;
}

function unsupportedFieldDiagnostics(
  value: unknown,
  path: string,
  schema: FieldSchema,
): NativeRuntimeConfigDiagnostic[] {
  const current = record(value);
  if (!current) return [];
  const diagnostics: NativeRuntimeConfigDiagnostic[] = [];
  for (const key of Object.keys(current).sort()) {
    const childPath = path ? `${path}.${key}` : key;
    const childSchema = schema.children?.[key];
    if (!schema.keys.has(key)) {
      diagnostics.push({
        severity: "warning",
        code: "unsupported_profile_field",
        path: childPath,
        message: `profile import does not map ${childPath}; field will remain file-asset-only until explicitly supported`,
      });
      continue;
    }
    if (childSchema) {
      diagnostics.push(
        ...unsupportedFieldDiagnostics(current[key], childPath, childSchema),
      );
    }
  }
  return diagnostics;
}

function ambiguous(
  path: string,
  message: string,
): NativeRuntimeConfigDiagnostic {
  return {
    severity: "warning",
    code: "ambiguous_profile_field",
    path,
    message,
  };
}

function mergeProfileForValidation(
  existingProfiles: readonly ProfileConfig[],
  profile: ProfileConfig,
): ProfileConfig[] {
  const profiles = existingProfiles.filter(
    (candidate) => candidate.profileId !== profile.profileId,
  );
  profiles.push(profile);
  return profiles;
}

function profileImportSummary(source: LoadedProfileConfigSource): string {
  const name = source.profile.displayName ?? source.profile.profileId;
  return `${name} imported from ${source.sourceFormat}`;
}

function stripUndefined(
  value: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined),
  );
}

interface FieldSchema {
  keys: Set<string>;
  children?: Record<string, FieldSchema>;
}

function schema(
  keys: readonly string[],
  children?: Record<string, FieldSchema>,
): FieldSchema {
  return {
    keys: new Set([...keys, ...Object.keys(children ?? {})]),
    children,
  };
}

const PROFILE_SCHEMA = schema(
  [
    "profileId",
    "profileIdentity",
    "name",
    "displayName",
    "modelConfigId",
    "providerAlias",
    "modelConfig",
    "externalMessageDeliveryPolicy",
    "brain",
    "runtime",
    "runtimeConfig",
    "toolPolicy",
    "prompt",
    "skills",
    "mcpConfig",
    "backgroundReview",
    "memoryConfig",
    "roleplayNarrator",
    "roleplayMechanic",
    "sessionDefaults",
    "channelDefaults",
  ],
  {
    modelConfig: schema([
      "provider",
      "modelName",
      "model",
      "baseUrl",
      "api",
      "apiKeyEnv",
      "temperatureMilli",
      "temperature",
      "maxOutputTokens",
      "maxTokens",
    ]),
    brain: schema(["module", "strategy"]),
    runtime: schema(["maxTurns", "maxTokensPerTurn"], {
      defaultResourceLimits: schema(["maxDurationMs", "maxDelegationDepth"]),
    }),
    runtimeConfig: schema([
      "maxIterations",
      "maxTokensPerTurn",
      "maxDurationMs",
    ]),
    toolPolicy: schema([
      "mode",
      "requestedToolsets",
      "requestedTools",
      "deniedTools",
      "includeDeprecated",
    ]),
    prompt: schema([
      "system",
      "instructions",
      "soulMarkdown",
      "memoryMarkdown",
    ]),
    mcpConfig: schema([
      "bindingId",
      "endpointRef",
      "serverNames",
      "transport",
      "toolProfile",
    ]),
    backgroundReview: schema([
      "enabled",
      "reviewType",
      "schedule",
      "memoryNudgeInterval",
      "skillNudgeInterval",
      "maxTokens",
      "maxFindings",
      "maxCandidates",
      "llmReviewEnabled",
      "captureModelConfigId",
      "captureProviderAlias",
      "dryRun",
    ]),
    memoryConfig: schema([
      "enabled",
      "denMemory",
      "denseProfileMemory",
      "sessionMemory",
      "sessionMemoryPrompt",
    ]),
    roleplayNarrator: schema([
      "tone",
      "explicitness",
      "pacing",
      "memoryDepth",
      "stylePrompt",
      "exemplar",
      "review",
    ]),
    roleplayMechanic: schema(["autoMonitor", "auto_monitor"]),
    sessionDefaults: schema(["ownerId", "maxHistoryMessages"]),
    channelDefaults: schema(["wakePolicy"]),
  },
);

function record(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() !== "" ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
