import type {
  ChannelBindingDiagnostics,
  DenAdapterStatus,
} from "@rusty-crew/adapter-den";
import type {
  ChannelBindingRecord,
  McpBindingRecord,
  McpSurfaceDiagnostics,
} from "@rusty-crew/contracts";
import type { ChannelWakePolicy } from "./channel-wake-policy.js";
import type { McpSurfaceReloadReport } from "./mcp-surface-reload.js";

export type AdapterHealthStatus =
  | "active"
  | "degraded"
  | "archived"
  | "missing";

export interface ChannelProjectionFailureRecord {
  bindingId: string;
  kind: "message" | "activity";
  degradedReason: string;
  observedAt: string;
}

export type ChannelAdapterBindingSource = "configured" | "gateway_delivery";

export interface AdapterDiagnosticsInput {
  now: string;
  channelBindings: readonly ChannelBindingRecord[];
  dynamicChannelBindings?: readonly ChannelAdapterBindingDiagnostics[];
  channelWakePolicies?: Record<string, ChannelWakePolicy>;
  channelActivity?: readonly ChannelBindingDiagnostics[];
  channelProjectionFailures?: readonly ChannelProjectionFailureRecord[];
  denAdapterStatuses?: readonly DenAdapterStatus[];
  mcpBindings: readonly McpBindingRecord[];
  mcpSurfaces?: readonly McpSurfaceDiagnostics[];
  mcpReloadHistory?: readonly McpSurfaceReloadReport[];
}

export interface ChannelAdapterBindingDiagnostics {
  bindingId: string;
  bindingSource: ChannelAdapterBindingSource;
  adapterId: string;
  agentId: string;
  sessionId?: string;
  profileId: string;
  provider: string;
  externalChannelId?: string;
  externalThreadId?: string;
  conversationProjectId?: string;
  conversationChannelId?: number;
  sourceMessageId?: number;
  deliveryIntentId?: number;
  lastObservedAt?: string;
  wakePolicy?: ChannelWakePolicy;
  status: AdapterHealthStatus;
  membershipStatus: string;
  presenceStatus: string;
  subscriptionStatus: string;
  stalePresence: boolean;
  droppedProjections: number;
  lastError?: string;
}

export interface McpAdapterSurfaceDiagnostics {
  bindingId: string;
  adapterId: string;
  agentId: string;
  sessionId?: string;
  profileId: string;
  status: AdapterHealthStatus;
  transport: string;
  serverNames: string[];
  toolProfileKey: string;
  discoveredToolRevision?: string;
  reconnectAttempts: number;
  collisionCount: number;
  discoveryIssueCount: number;
  optionalServerFailures: string[];
  lastError?: string;
}

export interface AdapterDiagnosticsProjection {
  generatedAt: string;
  degraded: boolean;
  channels: {
    totalBindings: number;
    activeBindings: number;
    degradedBindings: number;
    droppedProjections: number;
    lastProjectionError?: string;
    bindings: ChannelAdapterBindingDiagnostics[];
  };
  mcp: {
    totalSurfaces: number;
    activeSurfaces: number;
    degradedSurfaces: number;
    collisionCount: number;
    reloadCount: number;
    surfaces: McpAdapterSurfaceDiagnostics[];
  };
  issues: string[];
}

export function buildAdapterDiagnosticsProjection(
  input: AdapterDiagnosticsInput,
): AdapterDiagnosticsProjection {
  const channelActivity = new Map(
    (input.channelActivity ?? []).map((item) => [item.bindingId, item]),
  );
  const projectionFailures = groupByBinding(
    input.channelProjectionFailures ?? [],
  );
  const configuredChannelBindings = input.channelBindings.map((binding) =>
    channelBindingDiagnostics(
      binding,
      channelActivity.get(binding.bindingId),
      projectionFailures.get(binding.bindingId) ?? [],
      input.channelWakePolicies?.[binding.bindingId],
    ),
  );
  const dynamicChannelBindings = [...(input.dynamicChannelBindings ?? [])];
  const channelBindings = [
    ...configuredChannelBindings,
    ...dynamicChannelBindings,
  ];
  const denAdapterDropped = (input.denAdapterStatuses ?? []).reduce(
    (sum, status) => sum + status.droppedProjections,
    0,
  );
  const denAdapterLastError = latestDefined(
    (input.denAdapterStatuses ?? []).map(
      (status) => status.lastProjectionError,
    ),
  );

  const mcpSurfaces = new Map(
    (input.mcpSurfaces ?? []).map((surface) => [surface.bindingId, surface]),
  );
  const reloadHistory = groupByBinding(input.mcpReloadHistory ?? []);
  const mcp = input.mcpBindings.map((binding) =>
    mcpSurfaceDiagnostics(
      binding,
      mcpSurfaces.get(binding.bindingId),
      reloadHistory.get(binding.bindingId) ?? [],
    ),
  );

  const channelDropped =
    (input.channelProjectionFailures ?? []).length + denAdapterDropped;
  const channelDegraded = channelBindings.filter(
    (binding) => binding.status === "degraded",
  ).length;
  const mcpDegraded = mcp.filter(
    (surface) => surface.status === "degraded",
  ).length;
  const issues = [
    ...channelBindings.flatMap((binding) =>
      binding.lastError
        ? [`channel ${binding.bindingId}: ${binding.lastError}`]
        : [],
    ),
    ...mcp.flatMap((surface) =>
      surface.lastError
        ? [`mcp ${surface.bindingId}: ${surface.lastError}`]
        : [],
    ),
  ];

  return {
    generatedAt: input.now,
    degraded: channelDegraded > 0 || mcpDegraded > 0 || channelDropped > 0,
    channels: {
      totalBindings: channelBindings.length,
      activeBindings: channelBindings.filter(
        (binding) => binding.status === "active",
      ).length,
      degradedBindings: channelDegraded,
      droppedProjections: channelDropped,
      lastProjectionError:
        latestProjectionFailure(input.channelProjectionFailures ?? [])
          ?.degradedReason ?? denAdapterLastError,
      bindings: channelBindings,
    },
    mcp: {
      totalSurfaces: mcp.length,
      activeSurfaces: mcp.filter((surface) => surface.status === "active")
        .length,
      degradedSurfaces: mcpDegraded,
      collisionCount: mcp.reduce(
        (sum, surface) => sum + surface.collisionCount,
        0,
      ),
      reloadCount: input.mcpReloadHistory?.length ?? 0,
      surfaces: mcp,
    },
    issues,
  };
}

function channelBindingDiagnostics(
  binding: ChannelBindingRecord,
  activity: ChannelBindingDiagnostics | undefined,
  failures: readonly ChannelProjectionFailureRecord[],
  wakePolicy: ChannelWakePolicy | undefined,
): ChannelAdapterBindingDiagnostics {
  const lastFailure = latestProjectionFailure(failures);
  const status = channelStatus(binding, activity, failures);
  return {
    bindingId: binding.bindingId,
    bindingSource: "configured",
    adapterId: binding.adapterId,
    agentId: binding.agentId,
    sessionId: binding.sessionId,
    profileId: binding.profileId,
    provider: binding.provider,
    externalChannelId: binding.externalChannelId,
    externalThreadId: binding.externalThreadId,
    conversationProjectId:
      activity?.conversationProjectId ?? binding.conversationProjectId,
    conversationChannelId: activity?.conversationChannelId,
    wakePolicy,
    status,
    membershipStatus: activity?.membershipStatus ?? "missing",
    presenceStatus: activity?.presenceStatus ?? "missing",
    subscriptionStatus: activity?.subscriptionStatus ?? "missing",
    stalePresence: activity?.stale ?? false,
    droppedProjections: failures.length,
    lastError:
      lastFailure?.degradedReason ??
      activity?.degradedReason ??
      binding.degradedReason,
  };
}

function channelStatus(
  binding: ChannelBindingRecord,
  activity: ChannelBindingDiagnostics | undefined,
  failures: readonly ChannelProjectionFailureRecord[],
): AdapterHealthStatus {
  if (binding.status === "archived") return "archived";
  if (binding.status !== "active") return "degraded";
  if (failures.length > 0) return "degraded";
  if (activity === undefined) return "missing";
  if (activity.stale || activity.degradedReason) return "degraded";
  if (activity.subscriptionStatus !== "active") return "degraded";
  return "active";
}

function mcpSurfaceDiagnostics(
  binding: McpBindingRecord,
  surface: McpSurfaceDiagnostics | undefined,
  reloads: readonly McpSurfaceReloadReport[],
): McpAdapterSurfaceDiagnostics {
  const latestReload = reloads.at(-1);
  const status = mcpStatus(binding, surface, latestReload);
  return {
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    agentId: binding.agentId,
    sessionId: binding.sessionId,
    profileId: binding.profileId,
    status,
    transport: surface?.transport ?? binding.transport,
    serverNames: [...(surface?.serverNames ?? binding.serverNames)],
    toolProfileKey: surface?.toolProfileKey ?? binding.toolProfileKey,
    discoveredToolRevision: binding.discoveredToolRevision,
    reconnectAttempts: surface?.reconnectAttempts ?? 0,
    collisionCount: reloads.reduce(
      (sum, reload) => sum + reload.collisionCount,
      0,
    ),
    discoveryIssueCount: reloads.reduce(
      (sum, reload) => sum + reload.discoveryIssueCount,
      0,
    ),
    optionalServerFailures: reloads.flatMap(
      (reload) => reload.optionalServerFailures,
    ),
    lastError:
      surface?.lastError ??
      latestReload?.degradedReason ??
      binding.degradedReason,
  };
}

function mcpStatus(
  binding: McpBindingRecord,
  surface: McpSurfaceDiagnostics | undefined,
  latestReload: McpSurfaceReloadReport | undefined,
): AdapterHealthStatus {
  if (binding.status === "archived" || surface?.status === "archived") {
    return "archived";
  }
  if (surface === undefined) return "missing";
  if (binding.status !== "active" || surface.status !== "active") {
    return "degraded";
  }
  if (latestReload?.status === "degraded") return "degraded";
  return "active";
}

function groupByBinding<T extends { bindingId: string }>(
  items: readonly T[],
): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const group = groups.get(item.bindingId);
    if (group) {
      group.push(item);
    } else {
      groups.set(item.bindingId, [item]);
    }
  }
  return groups;
}

function latestProjectionFailure(
  failures: readonly ChannelProjectionFailureRecord[],
): ChannelProjectionFailureRecord | undefined {
  return [...failures]
    .sort((left, right) => left.observedAt.localeCompare(right.observedAt))
    .at(-1);
}

function latestDefined(
  values: readonly (string | undefined)[],
): string | undefined {
  return values.filter((value): value is string => value !== undefined).at(-1);
}
