import type {
  AdapterId,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export function createDenAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "den" };
}
