import type {
  CrewAgentSessionCreationRecord,
  CrewAgentSessionCreationRequest,
} from "@rusty-crew/contracts";

export interface NativeCrewSessionBridgeMethods {
  createCrewAgentSession(
    request: CrewAgentSessionCreationRequest,
  ): Promise<CrewAgentSessionCreationRecord>;
}
