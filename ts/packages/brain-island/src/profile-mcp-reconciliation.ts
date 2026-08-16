import type { McpBindingRecord, SessionState } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeProfileRegistryRecord,
} from "@rusty-crew/native-bridge";

import {
  profileMcpBindingsFromRegistryRecord,
  type EditableProfileMcpBinding,
} from "./service-profile-runtime-mutations.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import {
  desiredMcpBindingTemplateId,
  materializedMcpBindingId,
} from "./mcp-binding-identity.js";

export { materializedMcpBindingId as materializedBindingId } from "./mcp-binding-identity.js";

export interface ProfileMcpReconciliationDiagnostic {
  severity: "info" | "warning";
  code:
    | "profile_mcp_binding_materialized"
    | "profile_mcp_binding_removed"
    | "profile_mcp_binding_replaced"
    | "profile_mcp_no_active_sessions";
  profileId: string;
  sessionId?: string;
  bindingId?: string;
  message: string;
}

export interface ProfileMcpReconciliationResult {
  bindings: McpBindingRecord[];
  materialized: McpBindingRecord[];
  removedBindingIds: string[];
  diagnostics: ProfileMcpReconciliationDiagnostic[];
  changed: boolean;
}

export interface RuntimeProfileMcpReconciliationResult {
  runtimeConfig: RustyCrewRuntimeConfig;
  profiles: Array<{
    profileId: string;
    desiredCount: number;
    activeSessionCount: number;
    materializedCount: number;
    removedBindingIds: string[];
    changed: boolean;
    diagnostics: ProfileMcpReconciliationDiagnostic[];
  }>;
}

export async function reconcileRuntimeProfileMcpBindings(input: {
  bridge: Pick<
    NativeBridgeModule,
    "listSessions" | "getProfileRegistryRecord" | "planRuntimeConfig"
  >;
  runtimeConfig: RustyCrewRuntimeConfig;
  profileIds?: readonly string[];
}): Promise<RuntimeProfileMcpReconciliationResult> {
  const sessions = await input.bridge.listSessions();
  const activeSessions = sessions.filter(
    (session) => session.status !== "archived",
  );
  const validationSessions = new Map(
    input.runtimeConfig.sessions.map((session) => [
      String(session.sessionId),
      {
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: session.kind,
        workspaceCwd: session.workspaceCwd,
        resourceLimits: session.resourceLimits,
      },
    ]),
  );
  for (const session of activeSessions) {
    validationSessions.set(String(session.sessionId), {
      sessionId: session.sessionId,
      agentId: session.agentId,
      profileId: session.profileId,
      kind: session.kind,
      workspaceCwd: session.workspace?.cwd,
      resourceLimits: session.resourceLimits,
    });
  }
  const profileIds = new Set(
    input.profileIds ?? [
      ...sessions.map((session) => String(session.profileId)),
      ...input.runtimeConfig.mcpBindings.map((binding) =>
        String(binding.profileId),
      ),
    ],
  );
  let bindings = [...input.runtimeConfig.mcpBindings];
  const profiles: RuntimeProfileMcpReconciliationResult["profiles"] = [];
  for (const profileId of [...profileIds].sort()) {
    const profile = await input.bridge.getProfileRegistryRecord(profileId);
    if (profile === undefined) continue;
    if (!hasExplicitProfileMcpIntent(profile)) continue;
    const result = reconcileProfileMcpBindings({
      profileId,
      desired: profileMcpBindingsFromRegistryRecord(profile),
      sessions,
      existing: bindings,
    });
    bindings = result.bindings;
    profiles.push(profileReconciliationProjection(profile, sessions, result));
  }
  const rustPlan = await input.bridge.planRuntimeConfig({
    runtimeConfig: {
      profilesDir: input.runtimeConfig.profilesDir,
      brains: [],
      sessions: [...validationSessions.values()],
      scheduledJobs: [],
      channelBindings: [],
      mcpBindings: bindings.map((binding) => ({
        bindingId: binding.bindingId,
        adapterId: String(binding.adapterId),
        agentId: String(binding.agentId),
        instanceId:
          binding.instanceId === undefined
            ? undefined
            : String(binding.instanceId),
        sessionId:
          binding.sessionId === undefined
            ? undefined
            : String(binding.sessionId),
        profileId: String(binding.profileId),
        serverNames: [...binding.serverNames],
        endpointRef: binding.endpointRef,
        transport: binding.transport,
        toolProfileKey: binding.toolProfileKey,
        status: binding.status,
      })),
    },
    profiles: [
      ...new Set([
        ...[...validationSessions.values()].map((session) =>
          String(session.profileId),
        ),
        ...activeSessions.map((session) => String(session.profileId)),
        ...bindings.map((binding) => String(binding.profileId)),
      ]),
    ].map((profileId) => ({ profileId })),
  });
  const rustErrors = rustPlan.diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (rustErrors.length > 0) {
    throw new Error(
      `profile MCP reconciliation rejected by Rust planner: ${rustErrors
        .map((diagnostic) => `${diagnostic.code}: ${diagnostic.message}`)
        .join("; ")}`,
    );
  }
  return {
    runtimeConfig: { ...input.runtimeConfig, mcpBindings: bindings },
    profiles,
  };
}

function hasExplicitProfileMcpIntent(
  profile: NativeProfileRegistryRecord,
): boolean {
  const settings = profile.activeRuntimeSettingsJson;
  return (
    settings !== null &&
    typeof settings === "object" &&
    (Object.hasOwn(settings, "mcpBindings") ||
      Object.hasOwn(settings, "mcp_bindings"))
  );
}

/**
 * Reconcile profile capability intent onto exact live Crew sessions.
 *
 * Profiles supply templates only. Session identity always comes from Rust's
 * session registry, and every materialization receives a session-qualified
 * binding id so concurrent sessions never share or retarget one record.
 */
export function reconcileProfileMcpBindings(input: {
  profileId: string;
  desired: readonly EditableProfileMcpBinding[];
  sessions: readonly SessionState[];
  existing: readonly McpBindingRecord[];
}): ProfileMcpReconciliationResult {
  const sessions = input.sessions
    .filter(
      (session) =>
        String(session.profileId) === input.profileId &&
        session.status !== "archived",
    )
    .sort((left, right) =>
      String(left.sessionId).localeCompare(String(right.sessionId)),
    );
  const owned = input.existing.filter(
    (binding) => String(binding.profileId) === input.profileId,
  );
  const unrelated = input.existing.filter(
    (binding) => String(binding.profileId) !== input.profileId,
  );
  const existingById = new Map(
    owned.map((binding) => [String(binding.bindingId), binding]),
  );
  const materialized: McpBindingRecord[] = [];
  const diagnostics: ProfileMcpReconciliationDiagnostic[] = [];

  for (const session of sessions) {
    input.desired.forEach((template, index) => {
      const templateId = desiredMcpBindingTemplateId(
        template.bindingId ?? `${input.profileId}-mcp-${index + 1}`,
      );
      const bindingId = materializedMcpBindingId(templateId, session.sessionId);
      const binding = {
        bindingId,
        adapterId: (template.adapterId ?? "mcp-ts-main") as never,
        agentId: session.agentId,
        sessionId: session.sessionId,
        profileId: session.profileId,
        serverNames: template.serverNames ?? [template.serverId],
        endpointRef: `config://mcp/${template.serverId}`,
        transport: template.transport ?? "streamable_http",
        toolProfileKey: template.toolProfileKey ?? input.profileId,
        status: "active",
        diagnostics: {
          desiredProfileBindingId: templateId,
          reconciliationSource: "profile_registry",
        },
      } satisfies McpBindingRecord;
      materialized.push(binding);
      const previous = existingById.get(bindingId);
      if (previous === undefined) {
        diagnostics.push({
          severity: "info",
          code: "profile_mcp_binding_materialized",
          profileId: input.profileId,
          sessionId: String(session.sessionId),
          bindingId,
          message: `Materialized desired MCP binding ${templateId} for exact session ${session.sessionId}.`,
        });
      } else if (!sameBinding(previous, binding)) {
        diagnostics.push({
          severity: "info",
          code: "profile_mcp_binding_replaced",
          profileId: input.profileId,
          sessionId: String(session.sessionId),
          bindingId,
          message: `Updated materialized MCP binding ${bindingId} for exact session ${session.sessionId}.`,
        });
      }
    });
  }

  if (sessions.length === 0 && input.desired.length > 0) {
    diagnostics.push({
      severity: "warning",
      code: "profile_mcp_no_active_sessions",
      profileId: input.profileId,
      message: `Profile ${input.profileId} has desired MCP bindings but no active Crew session; intent remains persisted and will reconcile when a session becomes active.`,
    });
  }

  const desiredIds = new Set(materialized.map((binding) => binding.bindingId));
  const removedBindingIds = owned
    .filter((binding) => !desiredIds.has(binding.bindingId))
    .map((binding) => binding.bindingId)
    .sort();
  for (const bindingId of removedBindingIds) {
    diagnostics.push({
      severity: "warning",
      code: "profile_mcp_binding_removed",
      profileId: input.profileId,
      bindingId,
      message: `Removed stale or no-longer-desired profile MCP materialization ${bindingId}.`,
    });
  }

  const bindings = [...unrelated, ...materialized].sort((left, right) =>
    String(left.bindingId).localeCompare(String(right.bindingId)),
  );
  return {
    bindings,
    materialized,
    removedBindingIds,
    diagnostics,
    changed: !sameBindingCollection(input.existing, bindings),
  };
}

function sameBindingCollection(
  left: readonly McpBindingRecord[],
  right: readonly McpBindingRecord[],
): boolean {
  if (left.length !== right.length) return false;
  const leftById = new Map(left.map((binding) => [binding.bindingId, binding]));
  return right.every((binding) => {
    const previous = leftById.get(binding.bindingId);
    return previous !== undefined && sameBinding(previous, binding);
  });
}

function sameBinding(left: McpBindingRecord, right: McpBindingRecord): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function profileReconciliationProjection(
  profile: NativeProfileRegistryRecord,
  sessions: readonly SessionState[],
  result: ProfileMcpReconciliationResult,
): RuntimeProfileMcpReconciliationResult["profiles"][number] {
  return {
    profileId: profile.profileId,
    desiredCount: profileMcpBindingsFromRegistryRecord(profile).length,
    activeSessionCount: sessions.filter(
      (session) =>
        String(session.profileId) === profile.profileId &&
        session.status !== "archived",
    ).length,
    materializedCount: result.materialized.length,
    removedBindingIds: result.removedBindingIds,
    changed: result.changed,
    diagnostics: result.diagnostics,
  };
}
