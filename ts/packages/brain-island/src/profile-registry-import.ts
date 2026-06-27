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
  bridge?: Pick<NativeBridgeModule, "validateRuntimeConfigDraft">;
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

function activeRuntimeSettingsJson(
  profile: ProfileConfig,
): Record<string, unknown> {
  return stripUndefined({
    schemaVersion: 1,
    modelConfig: profile.modelConfig,
    brain: profile.brain,
    runtime: profile.runtime,
    toolPolicy: profile.toolPolicy,
    skills: profile.skills,
    skillsMode: profile.skillsMode,
    mcpConfig: profile.mcpConfig,
    backgroundReview: profile.backgroundReview,
    memoryConfig: profile.memoryConfig,
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
    "modelConfig",
    "brain",
    "runtime",
    "runtimeConfig",
    "toolPolicy",
    "prompt",
    "skills",
    "mcpConfig",
    "backgroundReview",
    "memoryConfig",
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
    runtime: schema(["maxTurns", "maxTokensPerTurn", "maxTurnDurationMs"], {
      defaultResourceLimits: schema([
        "workdir",
        "maxDurationMs",
        "maxDelegationDepth",
      ]),
    }),
    runtimeConfig: schema([
      "maxIterations",
      "maxTokensPerTurn",
      "maxDurationMs",
      "maxTurnDurationMs",
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
      "dryRun",
    ]),
    memoryConfig: schema([
      "enabled",
      "denMemory",
      "denseProfileMemory",
      "sessionMemory",
      "sessionMemoryPrompt",
    ]),
    sessionDefaults: schema(["ownerId", "maxHistoryMessages", "turnTimeoutMs"]),
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
