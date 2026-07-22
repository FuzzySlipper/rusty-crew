import type { SessionId, SessionState } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

export function projectInFlightSessionState(
  session: SessionState,
  inFlightWakes: ReadonlySet<SessionId>,
): SessionState {
  if (
    session.status === "archived" ||
    !inFlightWakes.has(session.sessionId) ||
    session.status === "active"
  ) {
    return session;
  }
  return { ...session, status: "active" };
}

export async function listProjectedServiceSessions(input: {
  bridge: Pick<NativeBridgeModule, "listSessions">;
  inFlightWakes: ReadonlySet<SessionId>;
}): Promise<SessionState[]> {
  return (await input.bridge.listSessions()).map((session) =>
    projectInFlightSessionState(session, input.inFlightWakes),
  );
}
