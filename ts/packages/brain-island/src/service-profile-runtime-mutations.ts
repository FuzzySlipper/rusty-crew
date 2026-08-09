import { randomBytes } from "node:crypto";
import { readFile, mkdir, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import type {
  ExternalMessageDeliveryPolicy,
  McpBindingRecord,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeModelProviderRecord,
  NativeProfileRegistryMutationPlan,
  NativeProfileRegistryRecord,
  NativeProfileRegistryWrite,
} from "@rusty-crew/native-bridge";
import {
  contextStrategyPolicyFromPatch,
  contextStrategyPolicyFromUnknown,
  type ContextStrategyPolicy,
} from "./context-strategy.js";
import {
  createLocalToolProfileStore,
  LocalToolProfileError,
} from "./local-tool-profiles.js";
import type { ProfileConfig } from "./profile-loading.js";
import {
  loadProfileConfig,
  parseExternalMessageDeliveryPolicy,
} from "./profile-loading.js";
import type { ProfileRegistryWriteRoute } from "./service-profile-registry-routes.js";
import type {
  RustyCrewRuntimeConfig,
  RustyCrewRuntimeConfigApplyResult,
} from "./service-runtime-config.js";
import { buildBuiltInToolCatalog } from "./tool-registry.js";

export type ProfileRegistryWritePlan = NativeProfileRegistryMutationPlan;

export interface ProfileRegistryRuntimeConfigMutationContext {
  bridge: Pick<
    NativeBridgeModule,
    | "getModelProvider"
    | "getProfileRegistryRecord"
    | "listSimpleKv"
    | "putSimpleKv"
    | "deleteSimpleKv"
    | "validateLocalToolProfilePolicy"
    | "planProfileRegistryMutation"
    | "planBrainSelection"
  >;
  runtimeConfig: RustyCrewRuntimeConfig;
  serviceConfigFile: string;
  now(): string;
  applyRuntimeConfigFromDisk(options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<RustyCrewRuntimeConfigApplyResult>;
  rebuildBrainRuntime(profileId: string): Promise<void>;
}

export interface ProfileRegistryRuntimeConfigPlan {
  ok: boolean;
  profileId: string;
  mode: "plan" | "apply";
  expectedRevision: number;
  current: NativeProfileRegistryRecord;
  next: NativeProfileRegistryRecord;
  nextWrite: NativeProfileRegistryWrite;
  runtimeConfig: EditableProfileRuntimeConfig;
  diagnostics: ProfileRegistryWritePlan["diagnostics"];
  implications: {
    registryRevisionWillIncrement: true;
    profileFileWillChange: boolean;
    serviceConfigWillChange: boolean;
    configReloadRequired: true;
    runtimeRebuildRecommended: boolean;
    mcpRefreshRecommended: boolean;
    externalBindingRebuildRecommended: boolean;
  };
}

export interface EditableProfileMcpBinding {
  serverId: string;
  bindingId?: string;
  adapterId?: string;
  serverNames?: string[];
  transport?: string;
  toolProfileKey?: string;
}

interface EditableProfileRuntimeConfig {
  providerAlias: string;
  externalMessageDeliveryPolicy: ExternalMessageDeliveryPolicy;
  brain?: { module?: string; strategy?: string };
  localToolProfileId?: string;
  toolPolicy?: {
    requestedToolsets?: string[];
    requestedTools?: string[];
    deniedTools?: string[];
    includeDeprecated?: boolean;
  };
  contextPolicy: ContextStrategyPolicy;
  mcpBindings: EditableProfileMcpBinding[];
}

export function profileMcpBindingsFromRegistryRecord(
  record: NativeProfileRegistryRecord,
): EditableProfileMcpBinding[] {
  const settings = optionalRecord(record.activeRuntimeSettingsJson) ?? {};
  const value = settings.mcpBindings ?? settings.mcp_bindings;
  if (!Array.isArray(value)) return [];
  return value.flatMap((item): EditableProfileMcpBinding[] => {
    if (!isRecord(item)) return [];
    const serverNames = optionalStringArray(
      item.serverNames ?? item.server_names,
    );
    const endpointRef = optionalString(item.endpointRef ?? item.endpoint_ref);
    const serverId =
      optionalString(item.serverId ?? item.server_id) ??
      serverIdFromEndpointRef(endpointRef) ??
      serverNames?.[0];
    if (serverId === undefined) return [];
    return [
      {
        serverId,
        bindingId: optionalString(item.bindingId ?? item.binding_id),
        adapterId: optionalString(item.adapterId ?? item.adapter_id),
        serverNames,
        transport: optionalString(item.transport),
        toolProfileKey: optionalString(
          item.toolProfileKey ?? item.tool_profile_key,
        ),
      },
    ];
  });
}

export async function planProfileRegistryWrite(
  context: ProfileRegistryRuntimeConfigMutationContext,
  route: ProfileRegistryWriteRoute,
  body: unknown,
): Promise<ProfileRegistryWritePlan> {
  if (!isRecord(body)) {
    throw new Error("profile registry write body must be an object");
  }
  const current = await context.bridge.getProfileRegistryRecord(
    route.profileId,
  );
  if (current === undefined) {
    throw new Error(
      `profile registry record ${route.profileId} was not found; create or import a DB-backed profile before registry mutation`,
    );
  }
  if (route.kind === "runtime-config") {
    throw new Error("runtime-config writes use the runtime-config planner");
  }
  return context.bridge.planProfileRegistryMutation({
    profileId: route.profileId,
    kind: route.kind,
    mode: route.mode,
    current,
    bodyJson: body,
    now: context.now(),
  });
}

export async function planProfileRegistryRuntimeConfigWrite(
  context: ProfileRegistryRuntimeConfigMutationContext,
  route: ProfileRegistryWriteRoute,
  body: unknown,
): Promise<ProfileRegistryRuntimeConfigPlan> {
  if (!isRecord(body)) {
    throw new Error("profile registry runtime-config body must be an object");
  }
  const current = await context.bridge.getProfileRegistryRecord(
    route.profileId,
  );
  if (current === undefined) {
    throw new Error(
      `profile registry record ${route.profileId} was not found; create or import a DB-backed profile before registry mutation`,
    );
  }
  const expectedRevision = requiredRevision(body);
  const diagnostics: ProfileRegistryRuntimeConfigPlan["diagnostics"] = [];
  if (expectedRevision !== current.revision) {
    diagnostics.push({
      severity: "error",
      code: "profile_registry_revision_mismatch",
      path: "expectedRevision",
      message: `expected revision ${expectedRevision}, found ${current.revision}`,
    });
  }

  const existing = await editableRuntimeConfigForProfile(context, current);
  const runtimeConfig = await editableRuntimeConfigFromBody(
    context,
    current,
    existing,
    body,
    diagnostics,
  );
  const next = nextProfileRegistryRuntimeConfigRecord(
    current,
    runtimeConfig,
    context.now(),
  );
  const nextWrite = profileRegistryRecordToWrite(next, context.now());
  return {
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    profileId: route.profileId,
    mode: route.mode,
    expectedRevision,
    current,
    next,
    nextWrite,
    runtimeConfig,
    diagnostics,
    implications: {
      registryRevisionWillIncrement: true,
      profileFileWillChange:
        JSON.stringify(existing.profileFileRuntimeConfig) !==
        JSON.stringify(profileFileRuntimeConfig(runtimeConfig)),
      serviceConfigWillChange:
        JSON.stringify(existing.mcpBindings) !==
        JSON.stringify(runtimeConfig.mcpBindings),
      configReloadRequired: true,
      runtimeRebuildRecommended:
        existing.runtimeConfig.providerAlias !== runtimeConfig.providerAlias ||
        JSON.stringify(existing.runtimeConfig.brain ?? {}) !==
          JSON.stringify(runtimeConfig.brain ?? {}) ||
        JSON.stringify(existing.runtimeConfig.contextPolicy) !==
          JSON.stringify(runtimeConfig.contextPolicy),
      mcpRefreshRecommended:
        JSON.stringify(existing.mcpBindings) !==
        JSON.stringify(runtimeConfig.mcpBindings),
      externalBindingRebuildRecommended:
        existing.runtimeConfig.externalMessageDeliveryPolicy !==
        runtimeConfig.externalMessageDeliveryPolicy,
    },
  };
}

export async function applyProfileRegistryRuntimeConfigEffects(
  context: ProfileRegistryRuntimeConfigMutationContext,
  record: NativeProfileRegistryRecord,
  plan: ProfileRegistryRuntimeConfigPlan,
): Promise<{
  profilePath: string;
  runtimeConfigPath: string;
  mcpBindings: { removed: number; added: number };
  applyResult: RustyCrewRuntimeConfigApplyResult;
  brainRebuilt: boolean;
  externalBindingRebuildRecommended: boolean;
}> {
  const profilePath = safeProfileConfigPath(
    context.runtimeConfig.profilesDir,
    record.profileId,
  );
  if (profilePath === undefined) {
    throw new Error(
      `profile id ${record.profileId} is not a valid file profile id`,
    );
  }
  const profileConfig = await readProfileConfigJsonForMutation(
    profilePath,
    record.profileId,
  );
  applyEditableRuntimeConfigToProfileJson(profileConfig, plan.runtimeConfig);
  await writeJsonFileAtomic(profilePath, profileConfig);

  const runtimeConfigFile = await readRuntimeConfigFileForMutation(context);
  const mcpBindings = runtimeConfigFile.array("mcpBindings");
  const removed = removeRuntimeConfigEntries(
    mcpBindings,
    (entry) =>
      runtimeEntryString(entry, "profileId", "profile_id") === record.profileId,
  );
  const runtimeMcpBindings = runtimeMcpBindingsForProfile(
    context,
    record,
    plan.runtimeConfig,
  );
  mcpBindings.push(...runtimeMcpBindings);
  await writeJsonFileAtomic(context.serviceConfigFile, runtimeConfigFile.value);

  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "profile_runtime_config_updated",
    summaryPrefix: `Profile ${record.profileId} runtime config updated`,
  });
  if (plan.implications.runtimeRebuildRecommended) {
    await context.rebuildBrainRuntime(record.profileId);
  }
  return {
    profilePath,
    runtimeConfigPath: context.serviceConfigFile,
    mcpBindings: { removed, added: runtimeMcpBindings.length },
    applyResult,
    brainRebuilt: plan.implications.runtimeRebuildRecommended,
    externalBindingRebuildRecommended:
      plan.implications.externalBindingRebuildRecommended,
  };
}

function nextProfileRegistryRuntimeConfigRecord(
  current: NativeProfileRegistryRecord,
  runtimeConfig: EditableProfileRuntimeConfig,
  now: string,
): NativeProfileRegistryRecord {
  return {
    ...current,
    activeRuntimeSettingsJson: {
      ...(current.activeRuntimeSettingsJson ?? {}),
      ...profileRuntimeSettingsJson(runtimeConfig),
    },
    derivedRuntimeRefs: [
      ...current.derivedRuntimeRefs.filter(
        (ref) => ref.refKind !== "mcp_binding",
      ),
      ...runtimeConfig.mcpBindings.map((binding) => ({
        refKind: "mcp_binding",
        refId:
          binding.bindingId ??
          `${current.agentId ?? current.profileId}-mcp-${binding.serverId}`,
        status: "planned",
        updatedAt: now,
        metadataJson: {
          server_id: binding.serverId,
          server_names: binding.serverNames ?? [binding.serverId],
          endpoint_ref: `config://mcp/${binding.serverId}`,
          tool_profile_key: binding.toolProfileKey ?? current.profileId,
        },
      })),
    ],
    updatedAt: now,
  };
}

function profileRegistryRecordToWrite(
  record: NativeProfileRegistryRecord,
  now: string,
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
    activeRuntimeSettingsJson: record.activeRuntimeSettingsJson ?? {},
    sourceAssetRefs: record.sourceAssetRefs,
    derivedRuntimeRefs: record.derivedRuntimeRefs,
    importExport: record.importExport,
    now,
  };
}

async function editableRuntimeConfigForProfile(
  context: ProfileRegistryRuntimeConfigMutationContext,
  record: NativeProfileRegistryRecord,
): Promise<{
  runtimeConfig: EditableProfileRuntimeConfig;
  profileFileRuntimeConfig: ReturnType<typeof profileFileRuntimeConfig>;
  mcpBindings: EditableProfileRuntimeConfig["mcpBindings"];
}> {
  const profile = await loadProfileConfig(
    context.runtimeConfig.profilesDir,
    record.profileId as never,
  ).catch(() => undefined);
  const settings = optionalRecord(record.activeRuntimeSettingsJson) ?? {};
  const providerAlias =
    optionalString(settings.providerAlias) ??
    optionalString(settings.provider_alias) ??
    profile?.providerAlias ??
    "default";
  const externalMessageDeliveryPolicy = parseExternalMessageDeliveryPolicy(
    settings.externalMessageDeliveryPolicy ??
      profile?.externalMessageDeliveryPolicy,
  );
  const activeMcpBindings = context.runtimeConfig.mcpBindings
    .filter((binding) => String(binding.profileId) === record.profileId)
    .map(editableMcpBindingFromRuntime);
  const mcpBindings =
    activeMcpBindings.length > 0
      ? activeMcpBindings
      : profileMcpBindingsFromRegistryRecord(record);
  const runtimeConfig: EditableProfileRuntimeConfig = {
    providerAlias,
    externalMessageDeliveryPolicy,
    brain:
      profile?.brain ??
      brainMetadataFromUnknown(settings.brain) ??
      defaultProfileBrainForModelProvider(
        (await context.bridge.getModelProvider(providerAlias)) ??
          ({
            providerKind: "local",
            protocol: "chat_completions",
          } as NativeModelProviderRecord),
      ),
    localToolProfileId:
      profile?.localToolProfileId ??
      optionalString(settings.localToolProfileId) ??
      optionalString(settings.local_tool_profile_id),
    toolPolicy:
      editableToolPolicy(profile?.toolPolicy) ??
      profileToolPolicyFromUnknown(settings.toolPolicy ?? settings.tool_policy),
    contextPolicy:
      profile?.contextPolicy ??
      contextStrategyPolicyFromUnknown(
        settings.contextPolicy ?? settings.context_policy,
      ),
    mcpBindings,
  };
  return {
    runtimeConfig,
    profileFileRuntimeConfig: profileFileRuntimeConfig(runtimeConfig),
    mcpBindings,
  };
}

async function editableRuntimeConfigFromBody(
  context: ProfileRegistryRuntimeConfigMutationContext,
  record: NativeProfileRegistryRecord,
  existing: Awaited<ReturnType<typeof editableRuntimeConfigForProfile>>,
  body: Record<string, unknown>,
  diagnostics: ProfileRegistryRuntimeConfigPlan["diagnostics"],
): Promise<EditableProfileRuntimeConfig> {
  const providerAlias = Object.hasOwn(body, "providerAlias")
    ? requiredString(body.providerAlias, "providerAlias")
    : existing.runtimeConfig.providerAlias;
  let externalMessageDeliveryPolicy =
    existing.runtimeConfig.externalMessageDeliveryPolicy;
  if (Object.hasOwn(body, "externalMessageDeliveryPolicy")) {
    try {
      externalMessageDeliveryPolicy = parseExternalMessageDeliveryPolicy(
        body.externalMessageDeliveryPolicy,
      );
    } catch (error) {
      diagnostics.push({
        severity: "error",
        code: "external_message_delivery_policy_invalid",
        path: "externalMessageDeliveryPolicy",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }
  const modelProvider = await context.bridge.getModelProvider(providerAlias);
  if (modelProvider === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_provider_not_found",
      path: "providerAlias",
      message: `model provider alias ${providerAlias} was not found`,
    });
  } else if (modelProvider.status !== "active") {
    diagnostics.push({
      severity: "error",
      code: "model_provider_not_active",
      path: "providerAlias",
      message: `model provider alias ${providerAlias} is ${modelProvider.status}; active provider required`,
    });
  }

  let brain = Object.hasOwn(body, "brain")
    ? profileBrainFromBody(body.brain)
    : Object.hasOwn(body, "providerAlias") && modelProvider !== undefined
      ? defaultProfileBrainForModelProvider(modelProvider)
      : existing.runtimeConfig.brain;
  if (modelProvider !== undefined && brain !== undefined) {
    try {
      const selection = await context.bridge.planBrainSelection({
        ...(brain.module === undefined
          ? {}
          : { configuredModuleId: brain.module }),
        ...(brain.strategy === undefined
          ? {}
          : { configuredStrategyId: brain.strategy }),
        providerProtocol: modelProvider.protocol,
        providerKind: modelProvider.providerKind,
      });
      brain = {
        module: selection.module_id,
        strategy: selection.selected_strategy_id,
      };
    } catch (error) {
      diagnostics.push({
        severity: "error",
        code: "brain_selection_invalid",
        path: "brain",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  const localToolProfileId = Object.hasOwn(body, "localToolProfileId")
    ? optionalString(body.localToolProfileId)
    : existing.runtimeConfig.localToolProfileId;
  let toolPolicy = Object.hasOwn(body, "toolPolicy")
    ? (profileToolPolicyFromUnknown(body.toolPolicy) ?? {})
    : existing.runtimeConfig.toolPolicy;
  if (localToolProfileId !== undefined) {
    try {
      const localToolProfile = await createLocalToolProfileStore({
        bridge: context.bridge,
        now: context.now,
      }).resolve(localToolProfileId);
      toolPolicy = localToolProfile.toolPolicy;
    } catch (error) {
      diagnostics.push({
        severity: "error",
        code:
          error instanceof LocalToolProfileError
            ? error.reasonCode
            : "local_tool_profile_invalid",
        path: "localToolProfileId",
        message: errorMessage(
          error,
          `local tool profile ${localToolProfileId} is invalid`,
        ),
      });
    }
  } else {
    validateInlineToolPolicy(toolPolicy, diagnostics);
  }

  const mcpBindings = Object.hasOwn(body, "mcpBindings")
    ? editableMcpBindingsFromBody(body.mcpBindings)
    : existing.runtimeConfig.mcpBindings;
  const contextPolicy = Object.hasOwn(body, "contextPolicy")
    ? contextStrategyPolicyFromPatch(
        body.contextPolicy,
        existing.runtimeConfig.contextPolicy,
      )
    : {
        policy: existing.runtimeConfig.contextPolicy,
        diagnostics: [],
      };
  diagnostics.push(...contextPolicy.diagnostics);

  return {
    providerAlias,
    externalMessageDeliveryPolicy,
    brain,
    localToolProfileId,
    toolPolicy,
    contextPolicy: contextPolicy.policy,
    mcpBindings: mcpBindings.map((binding, index) =>
      normalizedEditableMcpBinding(record, binding, index),
    ),
  };
}

function profileRuntimeSettingsJson(
  runtimeConfig: EditableProfileRuntimeConfig,
): Record<string, unknown> {
  return compactRecord({
    provider_alias: runtimeConfig.providerAlias,
    providerAlias: runtimeConfig.providerAlias,
    externalMessageDeliveryPolicy: runtimeConfig.externalMessageDeliveryPolicy,
    brain: runtimeConfig.brain,
    skills_mode: "all",
    localToolProfileId: runtimeConfig.localToolProfileId,
    toolPolicy: runtimeConfig.toolPolicy,
    contextPolicy: runtimeConfig.contextPolicy,
    mcp_bindings: runtimeConfig.mcpBindings.map((binding) => ({
      server_id: binding.serverId,
      binding_id: binding.bindingId,
      adapter_id: binding.adapterId,
      server_names: binding.serverNames ?? [binding.serverId],
      transport: binding.transport ?? "streamable_http",
      tool_profile_key: binding.toolProfileKey,
      endpoint_ref: `config://mcp/${binding.serverId}`,
    })),
    mcpBindings: runtimeConfig.mcpBindings,
    profile: profileFileRuntimeConfig(runtimeConfig),
  });
}

function profileFileRuntimeConfig(
  runtimeConfig: EditableProfileRuntimeConfig,
): Record<string, unknown> {
  return compactRecord({
    providerAlias: runtimeConfig.providerAlias,
    externalMessageDeliveryPolicy: runtimeConfig.externalMessageDeliveryPolicy,
    brain: runtimeConfig.brain,
    localToolProfileId: runtimeConfig.localToolProfileId,
    toolPolicy: runtimeConfig.toolPolicy,
    contextPolicy: runtimeConfig.contextPolicy,
  });
}

function applyEditableRuntimeConfigToProfileJson(
  profileConfig: Record<string, unknown>,
  runtimeConfig: EditableProfileRuntimeConfig,
): void {
  profileConfig.providerAlias = runtimeConfig.providerAlias;
  profileConfig.externalMessageDeliveryPolicy =
    runtimeConfig.externalMessageDeliveryPolicy;
  delete profileConfig.modelConfig;
  if (runtimeConfig.brain === undefined) {
    delete profileConfig.brain;
  } else {
    profileConfig.brain = runtimeConfig.brain;
  }
  if (runtimeConfig.localToolProfileId === undefined) {
    delete profileConfig.localToolProfileId;
  } else {
    profileConfig.localToolProfileId = runtimeConfig.localToolProfileId;
  }
  if (runtimeConfig.toolPolicy === undefined) {
    delete profileConfig.toolPolicy;
  } else {
    profileConfig.toolPolicy = runtimeConfig.toolPolicy;
  }
  profileConfig.contextPolicy = runtimeConfig.contextPolicy;
}

async function readProfileConfigJsonForMutation(
  profilePath: string,
  profileId: string,
): Promise<Record<string, unknown>> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(profilePath, "utf8"));
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      parsed = { profileId };
    } else {
      throw error;
    }
  }
  if (!isRecord(parsed)) {
    throw new Error(`profile ${profileId} config root must be an object`);
  }
  parsed.profileId = profileId;
  return parsed;
}

function runtimeMcpBindingsForProfile(
  context: ProfileRegistryRuntimeConfigMutationContext,
  record: NativeProfileRegistryRecord,
  runtimeConfig: EditableProfileRuntimeConfig,
): Record<string, unknown>[] {
  const session = context.runtimeConfig.sessions.find(
    (candidate) => String(candidate.profileId) === record.profileId,
  );
  const agentId = String(
    record.agentId ?? session?.agentId ?? record.profileId,
  );
  return runtimeConfig.mcpBindings.map((binding, index) => ({
    bindingId: binding.bindingId ?? `${agentId}-mcp-${index + 1}`,
    adapterId: binding.adapterId ?? "mcp-ts-main",
    agentId,
    sessionId: String(session?.sessionId ?? `${record.profileId}-session`),
    profileId: record.profileId,
    serverNames: binding.serverNames ?? [binding.serverId],
    endpointRef: `config://mcp/${binding.serverId}`,
    transport: binding.transport ?? "streamable_http",
    toolProfileKey: binding.toolProfileKey ?? record.profileId,
    status: "active",
    diagnostics: {},
  }));
}

function editableMcpBindingFromRuntime(
  binding: McpBindingRecord,
): EditableProfileRuntimeConfig["mcpBindings"][number] {
  return {
    serverId:
      serverIdFromEndpointRef(binding.endpointRef) ??
      binding.serverNames[0] ??
      binding.bindingId,
    bindingId: binding.bindingId,
    adapterId: String(binding.adapterId),
    serverNames: binding.serverNames,
    transport: binding.transport,
    toolProfileKey: binding.toolProfileKey,
  };
}

function editableMcpBindingsFromBody(
  value: unknown,
): EditableProfileRuntimeConfig["mcpBindings"] {
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) {
    throw new Error("mcpBindings must be an array when provided");
  }
  return value.map((item, index) => {
    if (!isRecord(item)) {
      throw new Error(`mcpBindings[${index}] must be an object`);
    }
    const serverId = optionalString(item.serverId);
    if (serverId === undefined) {
      throw new Error(`mcpBindings[${index}].serverId is required`);
    }
    return {
      serverId,
      bindingId: optionalString(item.bindingId),
      adapterId: optionalString(item.adapterId),
      serverNames:
        item.serverNames === undefined
          ? undefined
          : stringArray(item.serverNames, `mcpBindings[${index}].serverNames`),
      transport: optionalString(item.transport),
      toolProfileKey:
        optionalString(item.toolProfileKey) ?? optionalString(item.toolProfile),
    };
  });
}

function normalizedEditableMcpBinding(
  record: NativeProfileRegistryRecord,
  binding: EditableProfileRuntimeConfig["mcpBindings"][number],
  index: number,
): EditableProfileRuntimeConfig["mcpBindings"][number] {
  const agentId = String(record.agentId ?? record.profileId);
  return {
    ...binding,
    bindingId: binding.bindingId ?? `${agentId}-mcp-${index + 1}`,
    adapterId: binding.adapterId ?? "mcp-ts-main",
    serverNames: binding.serverNames ?? [binding.serverId],
    transport: binding.transport ?? "streamable_http",
    toolProfileKey: binding.toolProfileKey ?? record.profileId,
  };
}

function profileToolPolicyFromUnknown(
  value: unknown,
): EditableProfileRuntimeConfig["toolPolicy"] | undefined {
  const policy = optionalRecord(value);
  if (policy === undefined) return undefined;
  return {
    requestedToolsets:
      policy.requestedToolsets === undefined
        ? undefined
        : stringArray(policy.requestedToolsets, "toolPolicy.requestedToolsets"),
    requestedTools:
      policy.requestedTools === undefined
        ? undefined
        : stringArray(policy.requestedTools, "toolPolicy.requestedTools"),
    deniedTools:
      policy.deniedTools === undefined
        ? undefined
        : stringArray(policy.deniedTools, "toolPolicy.deniedTools"),
    includeDeprecated:
      typeof policy.includeDeprecated === "boolean"
        ? policy.includeDeprecated
        : undefined,
  };
}

function editableToolPolicy(
  policy: ProfileConfig["toolPolicy"],
): EditableProfileRuntimeConfig["toolPolicy"] | undefined {
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

function validateInlineToolPolicy(
  policy: EditableProfileRuntimeConfig["toolPolicy"],
  diagnostics: ProfileRegistryRuntimeConfigPlan["diagnostics"],
): void {
  const catalog = buildBuiltInToolCatalog();
  const validToolsets = new Set(catalog.toolsets.map((toolset) => toolset.id));
  const validTools = new Set(catalog.tools.map((tool) => tool.name));
  for (const toolset of policy?.requestedToolsets ?? []) {
    if (toolset.startsWith("mcp:")) {
      diagnostics.push({
        severity: "error",
        code: "inline_tool_policy_rejects_mcp_toolset",
        path: "toolPolicy.requestedToolsets",
        message: `inline tool policy cannot reference dynamic MCP toolset ${toolset}`,
      });
    } else if (!validToolsets.has(toolset)) {
      diagnostics.push({
        severity: "error",
        code: "inline_tool_policy_unknown_toolset",
        path: "toolPolicy.requestedToolsets",
        message: `inline tool policy references unknown built-in toolset ${toolset}`,
      });
    }
  }
  for (const tool of policy?.requestedTools ?? []) {
    if (!validTools.has(tool)) {
      diagnostics.push({
        severity: "error",
        code: "inline_tool_policy_unknown_tool",
        path: "toolPolicy.requestedTools",
        message: `inline tool policy references unknown built-in tool ${tool}`,
      });
    }
  }
}

function brainMetadataFromUnknown(
  value: unknown,
): EditableProfileRuntimeConfig["brain"] | undefined {
  const brain = optionalRecord(value);
  if (brain === undefined) return undefined;
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as EditableProfileRuntimeConfig["brain"];
}

function serverIdFromEndpointRef(
  value: string | undefined,
): string | undefined {
  const prefix = "config://mcp/";
  return value?.startsWith(prefix) ? value.slice(prefix.length) : undefined;
}

function requiredRevision(body: Record<string, unknown>): number {
  const value = body.expectedRevision ?? body.expected_revision;
  if (!Number.isSafeInteger(value) || Number(value) < 1) {
    throw new Error(
      "expectedRevision is required and must be a positive integer",
    );
  }
  return Number(value);
}

function defaultProfileBrainForModelProvider(
  provider: NativeModelProviderRecord,
): { module?: string; strategy?: string } {
  if (provider.protocol === "responses") {
    return { module: "openai-responses" };
  }
  return { module: "chat-completions" };
}

function profileBrainFromBody(
  input: unknown,
): { module?: string; strategy?: string } | undefined {
  const brain = optionalRecord(input);
  if (!brain) {
    return undefined;
  }
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as { module?: string; strategy?: string };
}

interface RuntimeConfigFileForMutation {
  value: Record<string, unknown>;
  array(key: string): unknown[];
}

async function readRuntimeConfigFileForMutation(
  context: ProfileRegistryRuntimeConfigMutationContext,
): Promise<RuntimeConfigFileForMutation> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(context.serviceConfigFile, "utf8"));
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      parsed = {};
    } else {
      throw error;
    }
  }
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  if (parsed.profilesDir === undefined) {
    parsed.profilesDir = context.runtimeConfig.profilesDir;
  }
  if (
    context.runtimeConfig.skillsDir !== undefined &&
    parsed.skillsDir === undefined
  ) {
    parsed.skillsDir = context.runtimeConfig.skillsDir;
  }
  return {
    value: parsed,
    array(key) {
      const existing = parsed[key];
      if (existing === undefined) {
        const created: unknown[] = [];
        parsed[key] = created;
        return created;
      }
      if (!Array.isArray(existing)) {
        throw new Error(`runtime config ${key} must be an array`);
      }
      return existing;
    },
  };
}

async function writeJsonFileAtomic(
  path: string,
  value: unknown,
): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const tmpPath = `${path}.${process.pid}.${Date.now()}.${randomBytes(8).toString("hex")}.tmp`;
  await writeFile(tmpPath, `${JSON.stringify(value, null, 2)}\n`);
  await rename(tmpPath, path);
}

function removeRuntimeConfigEntries(
  entries: unknown[],
  shouldRemove: (entry: Record<string, unknown>) => boolean,
): number {
  let removed = 0;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!isRecord(entry) || !shouldRemove(entry)) continue;
    entries.splice(index, 1);
    removed += 1;
  }
  return removed;
}

function runtimeEntryString(
  entry: Record<string, unknown>,
  camelKey: string,
  snakeKey: string,
): string | undefined {
  const value = entry[camelKey] ?? entry[snakeKey];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function safeProfileConfigPath(
  profilesDir: string,
  profileId: string,
): string | undefined {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(profileId)) {
    return undefined;
  }
  return join(profilesDir, `${profileId}.json`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return isRecord(value) ? value : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function optionalStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const parsed = value.flatMap((item) => {
    const string = optionalString(item);
    return string === undefined ? [] : [string];
  });
  return parsed.length > 0 ? parsed : undefined;
}

function requiredString(value: unknown, fieldName: string): string {
  const result = optionalString(value);
  if (result === undefined) {
    throw new Error(`${fieldName} is required`);
  }
  return result;
}

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`${fieldName} must be an array`);
  }
  return value.map((item, index) => {
    const parsed = optionalString(item);
    if (parsed === undefined) {
      throw new Error(`${fieldName}[${index}] must be a non-empty string`);
    }
    return parsed;
  });
}

function compactRecord(
  value: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(
      ([, entry]) => entry !== null && entry !== undefined,
    ),
  );
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
