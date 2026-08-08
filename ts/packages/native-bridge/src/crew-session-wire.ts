import type {
  CrewAgentSessionCreationOutcome,
  CrewAgentSessionCreationRecord,
  SessionWorkspaceUpdateRecord,
  SessionId,
} from "@rusty-crew/contracts";

import {
  toSessionWorkspace,
  type RawSessionWorkspace,
} from "./session-workspace-wire.js";
import { toSessionState, type RawSessionState } from "./session-wire.js";

export interface RawCrewAgentSessionCreationRecord {
  request_fingerprint: string;
  profile_revision: number;
  template_session_id?: SessionId | null;
  outcome: CrewAgentSessionCreationOutcome;
  session: RawSessionState;
}

export interface RawSessionWorkspaceUpdateRecord {
  previous: RawSessionWorkspace;
  current: RawSessionWorkspace;
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

export function toSessionWorkspaceUpdateRecord(
  raw: RawSessionWorkspaceUpdateRecord,
): SessionWorkspaceUpdateRecord {
  return {
    previous: toSessionWorkspace(raw.previous),
    current: toSessionWorkspace(raw.current),
    session: toSessionState(raw.session),
  };
}
