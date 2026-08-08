import type {
  CrewAgentSessionCreationRecord,
  CrewAgentSessionCreationRequest,
  SessionWorkspaceUpdate,
  SessionWorkspaceUpdateRecord,
} from "@rusty-crew/contracts";

export interface NativeCrewSessionBridgeMethods {
  createCrewAgentSession(
    request: CrewAgentSessionCreationRequest,
  ): Promise<CrewAgentSessionCreationRecord>;
  updateSessionWorkspace(
    update: SessionWorkspaceUpdate,
  ): Promise<SessionWorkspaceUpdateRecord>;
}
