import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type {
  AgentId,
  BrainAction,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationId,
  CompletionPacket,
  ChannelBindingRecord,
  McpBindingRecord,
  ProfileId,
  ResourceLimits,
  SessionId,
  SessionKind,
  ToolProfile,
} from "@rusty-crew/contracts";
import type {
  BrainWakeExecutor,
  NativeBridgeModule,
  NativeSessionStateSummary,
} from "@rusty-crew/native-bridge";
import { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
import { createDenRouterPiAgentFactory } from "./den-router-agent.js";
import type { BrainImplementation } from "./index.js";
import { resolveLocalCodeTools } from "./local-code-tools.js";
import { createPiAgentBrain } from "./pi-agent-brain.js";
import type { PiAgentFactory } from "./pi-agent-brain.js";
import { loadProfileConfig, loadProfileContext } from "./profile-loading.js";
import type { RustyCrewServiceConfig } from "./service-config.js";

export interface RustyCrewConfiguredBrain {
  implementationId: BrainImplementationId;
  profileId: ProfileId;
}

export interface RustyCrewConfiguredSession {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  kind: SessionKind;
  resourceLimits?: ResourceLimits;
  toolProfile?: ToolProfile;
}

export interface RustyCrewRuntimeConfig {
  profilesDir: string;
  skillsDir?: string;
  brains: RustyCrewConfiguredBrain[];
  sessions: RustyCrewConfiguredSession[];
  channelBindings: ChannelBindingRecord[];
  mcpBindings: McpBindingRecord[];
}

export interface RustyCrewRuntimeConfigApplyResult {
  brainsRegistered: number;
  brainsAlreadyPresent: number;
  sessionsCreated: number;
  sessionsAlreadyPresent: number;
  sessionsReactivated: number;
  sessionsMissing: number;
  brainHandlesByProfileId: Record<string, BrainImplementationHandle>;
}

export async function loadRustyCrewRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): Promise<RustyCrewRuntimeConfig> {
  let raw: string;
  try {
    raw = await readFile(serviceConfig.paths.serviceConfigFile, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return emptyRuntimeConfig(serviceConfig);
    }
    throw error;
  }

  const parsed = JSON.parse(raw) as unknown;
  return expandRuntimeConfigFromProfiles(
    validateRuntimeConfig(parsed, serviceConfig),
  );
}

async function expandRuntimeConfigFromProfiles(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<RustyCrewRuntimeConfig> {
  const mcpBindings = [...runtimeConfig.mcpBindings];
  for (const session of runtimeConfig.sessions) {
    const profile = await loadProfileConfig(
      runtimeConfig.profilesDir,
      session.profileId,
    );
    const mcpConfig = profile.mcpConfig;
    if (mcpConfig?.toolProfile === undefined) continue;
    const bindingId = mcpConfig.bindingId ?? `${session.agentId}-mcp`;
    if (
      hasProfileMcpBinding(
        mcpBindings,
        session,
        bindingId,
        mcpConfig.toolProfile,
      )
    ) {
      continue;
    }
    mcpBindings.push({
      bindingId,
      adapterId: "mcp-ts-main" as never,
      agentId: session.agentId,
      sessionId: session.sessionId,
      profileId: session.profileId,
      serverNames:
        mcpConfig.serverNames && mcpConfig.serverNames.length > 0
          ? mcpConfig.serverNames
          : [session.agentId],
      endpointRef: mcpConfig.endpointRef ?? `config://mcp/${session.agentId}`,
      transport: mcpConfig.transport ?? "stdio",
      toolProfileKey: mcpConfig.toolProfile,
      status: "active",
      diagnostics: {},
    });
  }
  return {
    ...runtimeConfig,
    mcpBindings,
  };
}

function hasProfileMcpBinding(
  bindings: readonly McpBindingRecord[],
  session: RustyCrewConfiguredSession,
  bindingId: string,
  toolProfileKey: string,
): boolean {
  return bindings.some(
    (binding) =>
      binding.bindingId === bindingId ||
      (binding.profileId === session.profileId &&
        (binding.sessionId === undefined ||
          binding.sessionId === session.sessionId) &&
        binding.toolProfileKey === toolProfileKey),
  );
}

export async function applyRustyCrewRuntimeConfig(input: {
  serviceConfig: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  bridge: NativeBridgeModule;
  existingBrainHandlesByProfileId?: Record<string, BrainImplementationHandle>;
  createMissingSessions?: boolean;
  createDenRouterAgentFactory?: (
    options: Parameters<typeof createDenRouterPiAgentFactory>[0],
  ) => Promise<PiAgentFactory>;
}): Promise<RustyCrewRuntimeConfigApplyResult> {
  const createMissingSessions = input.createMissingSessions ?? true;
  const profileContexts = new Map<
    ProfileId,
    Awaited<ReturnType<typeof loadProfileContext>>
  >();
  const loadProfile = async (profileId: ProfileId) => {
    const existing = profileContexts.get(profileId);
    if (existing !== undefined) return existing;
    const profile = await loadProfileContext({
      profilesDir: input.runtimeConfig.profilesDir,
      skillsDir: input.runtimeConfig.skillsDir,
      profileId,
    });
    profileContexts.set(profileId, profile);
    return profile;
  };
  const result: RustyCrewRuntimeConfigApplyResult = {
    brainsRegistered: 0,
    brainsAlreadyPresent: 0,
    sessionsCreated: 0,
    sessionsAlreadyPresent: 0,
    sessionsReactivated: 0,
    sessionsMissing: 0,
    brainHandlesByProfileId: {},
  };

  for (const brain of input.runtimeConfig.brains) {
    const profile = await loadProfile(brain.profileId);
    try {
      const handle = await input.bridge.registerBrainRuntime(
        {
          implementationId: brain.implementationId,
          profileId: brain.profileId,
          toolProfile: profile.toolSelection.toolProfile,
          modelConfig: profile.profile.modelConfig,
        },
        toBridgeWakeExecutor(
          await createConfiguredBrain(profile, {
            createDenRouterAgentFactory: input.createDenRouterAgentFactory,
          }),
        ),
      );
      result.brainHandlesByProfileId[brain.profileId] = handle;
      result.brainsRegistered += 1;
    } catch (error) {
      if (!isAlreadyPresentError(error)) throw error;
      const existingHandle =
        input.existingBrainHandlesByProfileId?.[brain.profileId];
      if (existingHandle !== undefined) {
        result.brainHandlesByProfileId[brain.profileId] = existingHandle;
      }
      result.brainsAlreadyPresent += 1;
    }
  }

  const existingSessionsById = new Map(
    (await input.bridge.listSessions()).map((session) => [
      session.sessionId,
      session,
    ]),
  );
  for (const session of input.runtimeConfig.sessions) {
    const profile = await loadProfile(session.profileId);
    const configuredSession = sessionWithProfileDefaults(session, profile);
    const existing = existingSessionsById.get(session.sessionId);
    if (!existing && !createMissingSessions) {
      result.sessionsMissing += 1;
      continue;
    }
    const ensured =
      await input.bridge.ensureConfiguredSession(configuredSession);
    if (!existing) {
      result.sessionsCreated += 1;
    } else if (
      existing.status === "archived" &&
      ensured.status !== "archived"
    ) {
      result.sessionsReactivated += 1;
    } else {
      result.sessionsAlreadyPresent += 1;
    }
  }

  return result;
}

function sessionWithProfileDefaults(
  session: RustyCrewConfiguredSession,
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
): RustyCrewConfiguredSession {
  return {
    ...session,
    resourceLimits:
      session.resourceLimits ??
      profile.profile.runtime?.defaultResourceLimits ??
      undefined,
    toolProfile: session.toolProfile ?? profile.toolSelection.toolProfile,
  };
}

export function configuredSessionForChannelBinding(
  runtimeConfig: RustyCrewRuntimeConfig,
  binding: ChannelBindingRecord,
): RustyCrewConfiguredSession | undefined {
  const matchingSessions =
    binding.sessionId === undefined
      ? runtimeConfig.sessions.filter(
          (session) => session.agentId === binding.agentId,
        )
      : runtimeConfig.sessions.filter(
          (session) => session.sessionId === binding.sessionId,
        );

  if (matchingSessions.length === 0) return undefined;
  if (matchingSessions.length > 1) {
    throw new Error(
      `channel binding ${binding.bindingId} matches multiple configured sessions for agent ${binding.agentId}`,
    );
  }

  const session = matchingSessions[0]!;
  if (session.agentId !== binding.agentId) {
    throw new Error(
      `channel binding ${binding.bindingId} targets agent ${binding.agentId} but configured session ${session.sessionId} belongs to ${session.agentId}`,
    );
  }
  if (session.profileId !== binding.profileId) {
    throw new Error(
      `channel binding ${binding.bindingId} targets profile ${binding.profileId} but configured session ${session.sessionId} uses ${session.profileId}`,
    );
  }
  return session;
}

export async function ensureConfiguredSessionForChannelBinding(input: {
  bridge: Pick<NativeBridgeModule, "ensureConfiguredSession">;
  runtimeConfig: RustyCrewRuntimeConfig;
  binding: ChannelBindingRecord;
}): Promise<NativeSessionStateSummary> {
  const session = configuredSessionForChannelBinding(
    input.runtimeConfig,
    input.binding,
  );
  if (session === undefined) {
    throw new Error(
      `channel binding ${input.binding.bindingId} has no matching configured session`,
    );
  }
  return input.bridge.ensureConfiguredSession(session);
}

async function createConfiguredBrain(
  profile: Awaited<ReturnType<typeof loadProfileContext>>,
  options: {
    createDenRouterAgentFactory?: (
      options: Parameters<typeof createDenRouterPiAgentFactory>[0],
    ) => Promise<PiAgentFactory>;
  } = {},
): Promise<BrainImplementation> {
  if (profile.profile.modelConfig.provider === "den-router") {
    const createAgent = await (
      options.createDenRouterAgentFactory ?? createDenRouterPiAgentFactory
    )({
      modelId: profile.profile.modelConfig.modelName,
      maxTokens: profile.profile.modelConfig.maxOutputTokens,
      baseUrl: profile.profile.modelConfig.baseUrl,
      api: profile.profile.modelConfig.api,
      apiKeyEnv: profile.profile.modelConfig.apiKeyEnv,
      temperature:
        profile.profile.modelConfig.temperatureMilli === undefined
          ? undefined
          : profile.profile.modelConfig.temperatureMilli / 1_000,
    });
    return createPiAgentBrain({
      createAgent,
      planActions: completionActionFromEvents,
      resolveTools: resolveLocalCodeTools,
      toolProfile: profile.toolSelection.toolProfile,
    });
  }

  return {
    async wake(wake): Promise<{
      events: BrainEventEnvelope[];
      actions: BrainAction[];
    }> {
      return {
        events: [
          {
            wakeId: wake.wakeId,
            sessionId: wake.sessionId,
            event: { type: "started" },
          },
          {
            wakeId: wake.wakeId,
            sessionId: wake.sessionId,
            event: { type: "finished" },
          },
        ],
        actions: [
          {
            type: "deliver_completion",
            packet: {
              sessionId: wake.sessionId,
              status: "completed",
              summary: "local service brain wake completed",
            },
          },
        ],
      };
    },
  };
}

function toBridgeWakeExecutor(brain: BrainImplementation): BrainWakeExecutor {
  return {
    wake(request, buffers) {
      return wakeBrainFromBridgeRequest(buffers, brain, request);
    },
  };
}

function completionActionFromEvents(input: {
  wake: { sessionId: SessionId };
  events: BrainEventEnvelope[];
}): BrainAction[] {
  const text = mergeTextDeltas(
    input.events.flatMap((event) =>
      event.event.type === "text_delta" ? [event.event.text] : [],
    ),
  ).trim();
  return [
    {
      type: "deliver_completion",
      packet: {
        sessionId: input.wake.sessionId,
        status: "completed",
        summary: text ? truncate(text, 480) : "LLM wake completed.",
      } satisfies CompletionPacket,
    },
  ];
}

function mergeTextDeltas(parts: readonly string[]): string {
  return parts
    .filter((part) => part.length > 0)
    .reduce((merged, part) => {
      if (!merged) return part;
      if (part.startsWith(merged)) return part;
      if (merged.endsWith(part)) return merged;
      return `${merged}${part}`;
    }, "");
}

function truncate(value: string, maxChars: number): string {
  return value.length <= maxChars ? value : `${value.slice(0, maxChars)}...`;
}

function emptyRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewRuntimeConfig {
  return {
    profilesDir: join(serviceConfig.paths.configDir, "profiles"),
    brains: [],
    sessions: [],
    channelBindings: [],
    mcpBindings: [],
  };
}

function validateRuntimeConfig(
  parsed: unknown,
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewRuntimeConfig {
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  const profilesDir = pathValue(
    parsed.profilesDir,
    join(serviceConfig.paths.configDir, "profiles"),
  );
  const skillsDir =
    parsed.skillsDir === undefined ? undefined : pathValue(parsed.skillsDir);
  return {
    profilesDir,
    skillsDir,
    brains: arrayValue(parsed.brains).map((item, index) =>
      configuredBrain(item, index),
    ),
    sessions: arrayValue(parsed.sessions).map((item, index) =>
      configuredSession(item, index),
    ),
    channelBindings: arrayValue(parsed.channelBindings).map((item, index) =>
      configuredChannelBinding(item, index),
    ),
    mcpBindings: arrayValue(parsed.mcpBindings).map((item, index) =>
      configuredMcpBinding(item, index),
    ),
  };
}

function configuredBrain(
  parsed: unknown,
  index: number,
): RustyCrewConfiguredBrain {
  if (!isRecord(parsed)) {
    throw new Error(`configured brain ${index} must be an object`);
  }
  const profileId = requiredString(
    parsed.profileId,
    `brains[${index}].profileId`,
  );
  return {
    profileId: profileId as ProfileId,
    implementationId: (optionalString(parsed.implementationId) ??
      `${profileId}-brain`) as BrainImplementationId,
  };
}

function configuredSession(
  parsed: unknown,
  index: number,
): RustyCrewConfiguredSession {
  if (!isRecord(parsed)) {
    throw new Error(`configured session ${index} must be an object`);
  }
  const kind = optionalString(parsed.kind) ?? "full";
  if (kind !== "full" && kind !== "worker" && kind !== "delegated") {
    throw new Error(
      `sessions[${index}].kind must be full, worker, or delegated`,
    );
  }
  return {
    sessionId: requiredString(
      parsed.sessionId,
      `sessions[${index}].sessionId`,
    ) as SessionId,
    agentId: requiredString(
      parsed.agentId,
      `sessions[${index}].agentId`,
    ) as AgentId,
    profileId: requiredString(
      parsed.profileId,
      `sessions[${index}].profileId`,
    ) as ProfileId,
    kind,
    resourceLimits: isRecord(parsed.resourceLimits)
      ? resourceLimits(parsed.resourceLimits)
      : undefined,
  };
}

function resourceLimits(parsed: Record<string, unknown>): ResourceLimits {
  return {
    workdir: optionalString(parsed.workdir),
    maxDurationMs: optionalNumber(parsed.maxDurationMs),
    maxDelegationDepth: optionalNumber(parsed.maxDelegationDepth),
  };
}

function configuredChannelBinding(
  parsed: unknown,
  index: number,
): ChannelBindingRecord {
  if (!isRecord(parsed)) {
    throw new Error(`configured channel binding ${index} must be an object`);
  }
  return {
    bindingId: requiredString(
      parsed.bindingId,
      `channelBindings[${index}].bindingId`,
    ),
    adapterId: requiredString(
      parsed.adapterId,
      `channelBindings[${index}].adapterId`,
    ) as never,
    provider: optionalString(parsed.provider) ?? "den_channels",
    agentId: requiredString(
      parsed.agentId,
      `channelBindings[${index}].agentId`,
    ) as AgentId,
    sessionId: optionalString(parsed.sessionId) as SessionId | undefined,
    profileId: requiredString(
      parsed.profileId,
      `channelBindings[${index}].profileId`,
    ) as ProfileId,
    externalChannelId: requiredString(
      parsed.externalChannelId,
      `channelBindings[${index}].externalChannelId`,
    ),
    externalThreadId: optionalString(parsed.externalThreadId),
    externalUserId: optionalString(parsed.externalUserId),
    providerSubscriptionId: optionalString(parsed.providerSubscriptionId),
    cursor: optionalString(parsed.cursor),
    membershipState: optionalString(parsed.membershipState),
    presenceState: optionalString(parsed.presenceState),
    status: externalBindingStatus(parsed.status),
    degradedReason: optionalString(parsed.degradedReason),
  };
}

function configuredMcpBinding(
  parsed: unknown,
  index: number,
): McpBindingRecord {
  if (!isRecord(parsed)) {
    throw new Error(`configured MCP binding ${index} must be an object`);
  }
  const profileId = requiredString(
    parsed.profileId,
    `mcpBindings[${index}].profileId`,
  );
  return {
    bindingId: requiredString(
      parsed.bindingId,
      `mcpBindings[${index}].bindingId`,
    ),
    adapterId: requiredString(
      parsed.adapterId,
      `mcpBindings[${index}].adapterId`,
    ) as never,
    agentId: requiredString(
      parsed.agentId,
      `mcpBindings[${index}].agentId`,
    ) as AgentId,
    sessionId: optionalString(parsed.sessionId) as SessionId | undefined,
    profileId: profileId as ProfileId,
    serverNames: stringList(
      parsed.serverNames,
      `mcpBindings[${index}].serverNames`,
    ),
    endpointRef: requiredString(
      parsed.endpointRef,
      `mcpBindings[${index}].endpointRef`,
    ),
    transport: optionalString(parsed.transport) ?? "stdio",
    toolProfileKey: optionalString(parsed.toolProfileKey) ?? `${profileId}-mcp`,
    discoveredToolRevision: optionalString(parsed.discoveredToolRevision),
    status: externalBindingStatus(parsed.status),
    degradedReason: optionalString(parsed.degradedReason),
    diagnostics: isRecord(parsed.diagnostics)
      ? {
          lastError: optionalString(parsed.diagnostics.lastError),
          lastCheckedAt: optionalString(parsed.diagnostics.lastCheckedAt),
          notes: optionalString(parsed.diagnostics.notes),
        }
      : {},
  };
}

function arrayValue(input: unknown): unknown[] {
  if (input === undefined) return [];
  if (!Array.isArray(input))
    throw new Error("runtime config arrays must be arrays");
  return input;
}

function stringList(input: unknown, name: string): string[] {
  if (!Array.isArray(input) || input.length === 0) {
    throw new Error(`${name} must be a non-empty string array`);
  }
  return input.map((item, index) => requiredString(item, `${name}[${index}]`));
}

function externalBindingStatus(
  input: unknown,
): "active" | "degraded" | "archived" {
  const status = optionalString(input) ?? "active";
  if (status !== "active" && status !== "degraded" && status !== "archived") {
    throw new Error(
      "external binding status must be active, degraded, or archived",
    );
  }
  return status;
}

function pathValue(input: unknown, fallback?: string): string {
  const raw = input === undefined ? fallback : requiredString(input, "path");
  if (!raw) throw new Error("path must not be empty");
  return resolve(raw);
}

function requiredString(input: unknown, name: string): string {
  if (typeof input !== "string" || !input.trim()) {
    throw new Error(`${name} must be a non-empty string`);
  }
  return input.trim();
}

function optionalString(input: unknown): string | undefined {
  return typeof input === "string" && input.trim() ? input.trim() : undefined;
}

function optionalNumber(input: unknown): number | undefined {
  return typeof input === "number" && Number.isFinite(input)
    ? input
    : undefined;
}

function isAlreadyPresentError(error: unknown): boolean {
  const message = error instanceof Error ? error.message.toLowerCase() : "";
  return (
    message.includes("already exists") ||
    message.includes("already registered") ||
    message.includes("duplicate")
  );
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
