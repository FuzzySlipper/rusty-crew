import type {
  CrewAgentSessionCreationOutcome,
  CrewAgentSessionCreationRecord,
  SessionId,
} from "@rusty-crew/contracts";

import { toSessionState, type RawSessionState } from "./session-wire.js";

export interface RawCrewAgentSessionCreationRecord {
  request_fingerprint: string;
  profile_revision: number;
  template_session_id?: SessionId | null;
  outcome: CrewAgentSessionCreationOutcome;
  session: RawSessionState;
}

export function toCrewAgentSessionCreationRecord(
  raw: RawCrewAgentSessionCreationRecord,
): CrewAgentSessionCreationRecord {
  return {
    requestFingerprint: raw.request_fingerprint,
    profileRevision: raw.profile_revision,
    templateSessionId: raw.template_session_id,
    outcome: raw.outcome,
    session: toSessionState(raw.session),
  };
}
