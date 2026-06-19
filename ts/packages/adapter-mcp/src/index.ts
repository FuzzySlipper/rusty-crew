import type {
  AdapterId,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export function createMcpAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "mcp", displayName: "MCP" };
}
