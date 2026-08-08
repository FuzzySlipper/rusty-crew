import type {
  AgentId,
  ProfileId,
  SessionId,
  SessionState,
  TaskId,
} from "@rusty-crew/contracts";
import {
  toSessionWorkspace,
  type RawSessionWorkspace,
} from "./session-workspace-wire.js";
import {
  toSessionResourceLimits,
  type RawSessionResourceLimits,
} from "./session-resource-wire.js";
export interface RawSessionState {
  handle: number;
  session_id: SessionId;
  agent_id: AgentId;
  profile_id: ProfileId;
  kind: SessionState["kind"];
  delegation?: {
    parent_session_id: SessionId;
    parent_agent_id: AgentId;
    source_wake_id: string;
    source_action_index: number;
    requested_task_id?: TaskId;
    correlation_id: string;
    workspace_constraint?: { cwd: string };
  };
  workspace?: RawSessionWorkspace | null;
  resource_limits?: RawSessionResourceLimits;
  tool_profile?: {
    tools: Array<{
      name: string;
      description: string;
      input_schema?: number;
    }>;
  };
  history_window?: {
    max_messages?: number;
  };
  inference_overrides?: {
    reasoning_effort?: string;
  };
  status: SessionState["status"];
  brain_turn_count: number;
  created_at: string;
  last_active_at: string;
}

export function toSessionState(state: RawSessionState): SessionState {
  return {
    handle: state.handle as SessionState["handle"],
    sessionId: state.session_id,
    agentId: state.agent_id,
    profileId: state.profile_id,
    kind: state.kind,
    delegation: state.delegation
      ? {
          parentSessionId: state.delegation.parent_session_id,
          parentAgentId: state.delegation.parent_agent_id,
          sourceWakeId: state.delegation.source_wake_id,
          sourceActionIndex: state.delegation.source_action_index,
          requestedTaskId: state.delegation.requested_task_id,
          correlationId: state.delegation.correlation_id,
          workspaceConstraint: state.delegation.workspace_constraint
            ? { cwd: state.delegation.workspace_constraint.cwd }
            : undefined,
        }
      : undefined,
    workspace:
      state.workspace == null ? undefined : toSessionWorkspace(state.workspace),
    resourceLimits: toSessionResourceLimits(state.resource_limits),
    toolProfile: {
      tools:
        state.tool_profile?.tools.map((tool) => ({
          name: tool.name,
          description: tool.description,
          inputSchema:
            typeof tool.input_schema === "number"
              ? (tool.input_schema as SessionState["toolProfile"]["tools"][number]["inputSchema"])
              : undefined,
        })) ?? [],
    },
    historyWindow: state.history_window
      ? {
          maxMessages: state.history_window.max_messages,
        }
      : undefined,
    inferenceOverrides: {
      reasoningEffort: state.inference_overrides?.reasoning_effort,
    },
    status: state.status,
    brainTurnCount: state.brain_turn_count,
    createdAt: state.created_at,
    lastActiveAt: state.last_active_at,
  };
}
