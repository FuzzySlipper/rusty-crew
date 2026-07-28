import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";
import {
  toCrewAgentSessionCreationRecord,
  type RawCrewAgentSessionCreationRecord,
} from "./crew-session-wire.js";

type CrewSessionMethods = Pick<NativeBridgeModule, "createCrewAgentSession">;

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
  };
}
