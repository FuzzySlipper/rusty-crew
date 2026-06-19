import type {
  AdapterId,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export function createTelegramAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "telegram" };
}
