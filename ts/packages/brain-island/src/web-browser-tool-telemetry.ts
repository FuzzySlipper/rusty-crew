import type {
  BrainEvent,
  ProfileId,
  ToolCallMetadata,
  ToolDescriptor,
  ToolCallSource,
} from "@rusty-crew/contracts";

export type WebBrowserResourceDenialReason =
  | "tool_profile_denied"
  | "timeout"
  | "cancelled"
  | "session_archived"
  | "network_denied"
  | "browser_unavailable"
  | "screenshot_store_unavailable";

export interface WebBrowserToolTelemetryInput {
  toolName: string;
  source?: Extract<ToolCallSource, "web" | "browser">;
  profileId?: ProfileId;
  timeoutMs?: number;
  allowed?: boolean;
  denialReason?: WebBrowserResourceDenialReason;
  cancelled?: boolean;
  archiveCleanup?: boolean;
}

export interface WebBrowserResourceHookInput {
  toolName: string;
  toolProfile?: { tools: readonly Pick<ToolDescriptor, "name">[] };
  profileId?: ProfileId;
  timeoutMs?: number;
  timedOut?: boolean;
  cancelled?: boolean;
  sessionArchived?: boolean;
  resourceDeniedReason?: Extract<
    WebBrowserResourceDenialReason,
    "network_denied" | "browser_unavailable" | "screenshot_store_unavailable"
  >;
}

export interface WebBrowserResourceHookDecision {
  allowed: boolean;
  denialReason?: WebBrowserResourceDenialReason;
  metadata: ToolCallMetadata;
}

export function createWebBrowserToolCallMetadata(
  input: WebBrowserToolTelemetryInput,
): ToolCallMetadata {
  return {
    source: input.source ?? webBrowserToolSource(input.toolName),
    serverNames: [],
    profileId: input.profileId,
    sourceToolName: input.toolName,
    policy: {
      allowed: input.allowed,
      denialReason: input.denialReason,
      timeoutMs: input.timeoutMs,
      cancelled: input.cancelled,
      archiveCleanup: input.archiveCleanup,
    },
  };
}

export function createWebBrowserToolStartedEvent(
  input: WebBrowserToolTelemetryInput,
): BrainEvent {
  return {
    type: "tool_call_started",
    toolName: input.toolName,
    metadata: createWebBrowserToolCallMetadata({ ...input, allowed: true }),
  };
}

export function createWebBrowserToolFinishedEvent(
  input: WebBrowserToolTelemetryInput & { isError: boolean },
): BrainEvent {
  return {
    type: "tool_call_finished",
    toolName: input.toolName,
    isError: input.isError,
    metadata: createWebBrowserToolCallMetadata(input),
  };
}

export function evaluateWebBrowserResourceHooks(
  input: WebBrowserResourceHookInput,
): WebBrowserResourceHookDecision {
  const denialReason = webBrowserDenialReason(input);
  const allowed = denialReason === undefined;
  return {
    allowed,
    denialReason,
    metadata: createWebBrowserToolCallMetadata({
      toolName: input.toolName,
      profileId: input.profileId,
      timeoutMs: input.timeoutMs,
      allowed,
      denialReason,
      cancelled: input.cancelled,
      archiveCleanup: input.sessionArchived,
    }),
  };
}

export function webBrowserToolSource(
  toolName: string,
): Extract<ToolCallSource, "web" | "browser"> {
  return toolName.startsWith("browser_") ? "browser" : "web";
}

function webBrowserDenialReason(
  input: WebBrowserResourceHookInput,
): WebBrowserResourceDenialReason | undefined {
  if (input.sessionArchived) return "session_archived";
  if (input.cancelled) return "cancelled";
  if (input.timedOut) return "timeout";
  if (input.resourceDeniedReason) return input.resourceDeniedReason;
  if (
    input.toolProfile &&
    !input.toolProfile.tools.some((tool) => tool.name === input.toolName)
  ) {
    return "tool_profile_denied";
  }
  return undefined;
}
