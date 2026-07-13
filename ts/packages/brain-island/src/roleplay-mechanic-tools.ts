import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { Type } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { ProfileConfig } from "./profile-loading.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export interface RoleplayMechanicToolDetails {
  ok: boolean;
  operation: "get_mechanic_capabilities";
  action: "read" | "denied" | "failed";
  reasonCode?: string;
  result?: unknown;
}

interface RoleplayMechanicProfilePlan {
  config: {
    name: string;
    providerAlias?: string;
    autoMonitor: {
      enabled: false;
      available: false;
      status: "inactive_future";
    };
  };
  systemPrompt: string;
  localToolProfileId: string;
}

const noParameters = Type.Object({}, { additionalProperties: false });

export function createRoleplayMechanicToolResolver(input: {
  bridge?: Pick<NativeBridgeModule, "planRoleplayMechanicProfile">;
  profile: ProfileConfig;
}): BrainToolResolver {
  return () => [getMechanicCapabilitiesTool(input)];
}

export function getMechanicCapabilitiesTool(input: {
  bridge?: Pick<NativeBridgeModule, "planRoleplayMechanicProfile">;
  profile: ProfileConfig;
}): BrainTool<typeof noParameters, RoleplayMechanicToolDetails> {
  return {
    name: "get_mechanic_capabilities",
    label: "Get mechanic capabilities",
    description:
      "Read this mechanic profile's identity, provider selection, proposal-only mutation policy, and auto-monitor availability.",
    parameters: noParameters,
    executionMode: "parallel",
    execute: async () => {
      if (input.profile.roleplayMechanic === undefined) {
        return mechanicResult("denied", {
          ok: false,
          reasonCode: "roleplay_mechanic_profile_required",
        });
      }
      if (input.bridge === undefined) {
        return mechanicResult("failed", {
          ok: false,
          reasonCode: "roleplay_mechanic_bridge_unavailable",
        });
      }
      try {
        const plan = (await input.bridge.planRoleplayMechanicProfile({
          name: input.profile.displayName ?? input.profile.profileId,
          providerAlias: input.profile.providerAlias,
          autoMonitor: input.profile.roleplayMechanic.autoMonitor,
        })) as RoleplayMechanicProfilePlan;
        return mechanicResult("read", {
          ok: true,
          result: {
            config: plan.config,
            localToolProfileId: plan.localToolProfileId,
            mutationPolicy: "proposal_only",
            directStateWrites: false,
          },
        });
      } catch (error) {
        return mechanicResult("failed", {
          ok: false,
          reasonCode: errorReasonCode(error),
          result: { message: errorMessage(error) },
        });
      }
    },
  };
}

function mechanicResult(
  action: RoleplayMechanicToolDetails["action"],
  detail: Omit<RoleplayMechanicToolDetails, "operation" | "action">,
): BrainToolResult<RoleplayMechanicToolDetails> {
  const details: RoleplayMechanicToolDetails = {
    operation: "get_mechanic_capabilities",
    action,
    ...detail,
  };
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function errorReasonCode(error: unknown): string {
  const message = errorMessage(error);
  return (
    message.match(/\b(roleplay_[a-z0-9_]+)\b/)?.[1] ??
    "roleplay_mechanic_capabilities_failed"
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
