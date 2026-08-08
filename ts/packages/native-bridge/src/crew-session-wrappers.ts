import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";
import {
  toCrewAgentSessionCreationRecord,
  toSessionWorkspaceUpdateRecord,
  type RawCrewAgentSessionCreationRecord,
  type RawSessionWorkspaceUpdateRecord,
} from "./crew-session-wire.js";

type CrewSessionMethods = Pick<
  NativeBridgeModule,
  "createCrewAgentSession" | "updateSessionWorkspace"
>;

export function createNativeBridgeCrewSessionMethods(
  binding: NativeBridgeBinding,
): CrewSessionMethods {
  return {
    createCrewAgentSession: async (request) =>
      toCrewAgentSessionCreationRecord(
        JSON.parse(
          binding.createCrewAgentSessionJson(JSON.stringify(request)),
        ) as RawCrewAgentSessionCreationRecord,
      ),
    updateSessionWorkspace: async (update) =>
      toSessionWorkspaceUpdateRecord(
        JSON.parse(
          binding.updateSessionWorkspaceJson(JSON.stringify(update)),
        ) as RawSessionWorkspaceUpdateRecord,
      ),
  };
}
