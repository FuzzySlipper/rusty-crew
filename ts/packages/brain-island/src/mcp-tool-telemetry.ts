import type { McpRegistryCandidate } from "@rusty-crew/adapter-mcp";
import type {
  BrainEvent,
  McpBindingRecord,
  ToolCallMetadata,
  ToolDescriptor,
} from "@rusty-crew/contracts";

export type McpResourceDenialReason =
  | "tool_profile_denied"
  | "timeout"
  | "cancelled"
  | "session_archived";

export interface McpToolTelemetryInput {
  binding: McpBindingRecord;
  toolName: string;
  sourceToolName?: string;
  catalogRevision?: string;
  timeoutMs?: number;
  allowed?: boolean;
  denialReason?: McpResourceDenialReason;
  cancelled?: boolean;
  archiveCleanup?: boolean;
}

export interface McpResourceHookInput {
  binding: McpBindingRecord;
  candidate: Pick<McpRegistryCandidate, "name" | "source">;
  toolProfile?: { tools: readonly Pick<ToolDescriptor, "name">[] };
  timeoutMs?: number;
  timedOut?: boolean;
  cancelled?: boolean;
  sessionArchived?: boolean;
}

export interface McpResourceHookDecision {
  allowed: boolean;
  denialReason?: McpResourceDenialReason;
  metadata: ToolCallMetadata;
}

export function createMcpToolCallMetadata(
  input: McpToolTelemetryInput,
): ToolCallMetadata {
  return {
    source: "mcp",
    adapterId: input.binding.adapterId,
    bindingId: input.binding.bindingId,
    serverNames: [...input.binding.serverNames],
    profileId: input.binding.profileId,
    toolProfileKey: input.binding.toolProfileKey,
    sourceToolName: input.sourceToolName,
    catalogRevision:
      input.catalogRevision ?? input.binding.discoveredToolRevision,
    policy: {
      allowed: input.allowed,
      denialReason: input.denialReason,
      timeoutMs: input.timeoutMs,
      cancelled: input.cancelled,
      archiveCleanup: input.archiveCleanup,
    },
  };
}

export function createMcpToolStartedEvent(
  input: McpToolTelemetryInput,
): BrainEvent {
  return {
    type: "tool_call_started",
    toolName: input.toolName,
    metadata: createMcpToolCallMetadata({ ...input, allowed: true }),
  };
}

export function createMcpToolFinishedEvent(
  input: McpToolTelemetryInput & { isError: boolean },
): BrainEvent {
  return {
    type: "tool_call_finished",
    toolName: input.toolName,
    isError: input.isError,
    metadata: createMcpToolCallMetadata(input),
  };
}

export function evaluateMcpResourceHooks(
  input: McpResourceHookInput,
): McpResourceHookDecision {
  const denialReason = mcpResourceDenialReason(input);
  const allowed = denialReason === undefined;
  return {
    allowed,
    denialReason,
    metadata: createMcpToolCallMetadata({
      binding: input.binding,
      toolName: input.candidate.name,
      sourceToolName: input.candidate.source.sourceToolName,
      catalogRevision: input.candidate.source.catalogRevision,
      timeoutMs: input.timeoutMs,
      allowed,
      denialReason,
      cancelled: input.cancelled,
      archiveCleanup: input.sessionArchived,
    }),
  };
}

function mcpResourceDenialReason(
  input: McpResourceHookInput,
): McpResourceDenialReason | undefined {
  if (input.sessionArchived) return "session_archived";
  if (input.cancelled) return "cancelled";
  if (input.timedOut) return "timeout";
  if (
    input.toolProfile &&
    !input.toolProfile.tools.some((tool) => tool.name === input.candidate.name)
  ) {
    return "tool_profile_denied";
  }
  return undefined;
}
