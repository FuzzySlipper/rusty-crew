import type {
  CoreEvent,
  SessionId,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

export interface WakeEventDrainContext<TReport> {
  bridge: Pick<NativeBridgeModule, "drainSubscriptionEvents">;
  wakeSubscription: SubscriptionHandle;
  suppressedWakeEvents: Map<SessionId, number>;
  dispatchWake(
    event: Extract<CoreEvent, { type: "brain_wake_requested" }>,
  ): Promise<TReport>;
}

export async function drainAndDispatchWakes<TReport>(
  context: WakeEventDrainContext<TReport>,
): Promise<TReport[]> {
  const events = await context.bridge.drainSubscriptionEvents(
    context.wakeSubscription,
    128,
  );
  const reports: TReport[] = [];
  for (const event of events) {
    if (event.type === "session_archived") {
      consumeSuppressedWakeEvent(context, event.sessionId);
      continue;
    }
    if (event.type !== "brain_wake_requested") continue;
    if (consumeSuppressedWakeEvent(context, event.sessionId)) continue;
    reports.push(await context.dispatchWake(event));
  }
  return reports;
}

export function suppressNextWakeEvent(
  context: Pick<WakeEventDrainContext<unknown>, "suppressedWakeEvents">,
  sessionId: SessionId,
): void {
  context.suppressedWakeEvents.set(
    sessionId,
    (context.suppressedWakeEvents.get(sessionId) ?? 0) + 1,
  );
}

function consumeSuppressedWakeEvent(
  context: Pick<WakeEventDrainContext<unknown>, "suppressedWakeEvents">,
  sessionId: SessionId,
): boolean {
  const count = context.suppressedWakeEvents.get(sessionId) ?? 0;
  if (count <= 0) return false;
  if (count === 1) context.suppressedWakeEvents.delete(sessionId);
  else context.suppressedWakeEvents.set(sessionId, count - 1);
  return true;
}
