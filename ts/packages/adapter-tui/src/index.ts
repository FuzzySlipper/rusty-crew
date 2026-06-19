import type {
  AdapterId,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export function createTuiAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "tui", displayName: "TUI" };
}
