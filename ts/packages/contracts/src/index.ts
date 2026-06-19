export type Brand<T, Name extends string> = T & { readonly __brand: Name };

export type EngineHandle = Brand<number, "EngineHandle">;
export type SessionHandle = Brand<number, "SessionHandle">;
export type SubscriptionHandle = Brand<number, "SubscriptionHandle">;
export type RuntimeBufferHandle = Brand<number, "RuntimeBufferHandle">;

export type AgentId = Brand<string, "AgentId">;
export type SessionId = Brand<string, "SessionId">;
export type ProfileId = Brand<string, "ProfileId">;
export type ProjectId = Brand<string, "ProjectId">;
export type TaskId = Brand<string, "TaskId">;
export type AdapterId = Brand<string, "AdapterId">;

export type ClockConfig = "system" | { fixed: string };

export interface EngineConfig {
  engineDataDir: string;
  clock: ClockConfig;
  defaultTurnBudget: number;
  defaultIdleTimeoutMs: number;
}

export type SessionKind = "full" | "worker" | "delegated";
export type SessionStatus = "active" | "idle" | "archived";

export interface ToolDescriptor {
  name: string;
  description: string;
}

export interface ToolProfile {
  tools: ToolDescriptor[];
}

export interface ResourceLimits {
  workdir?: string;
  maxDurationMs?: number;
  maxDelegationDepth?: number;
}

export interface SessionConfig {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  kind: SessionKind;
  resourceLimits: ResourceLimits;
  toolProfile: ToolProfile;
}

export interface SessionState extends SessionConfig {
  handle: SessionHandle;
  status: SessionStatus;
  brainTurnCount: number;
  createdAt: string;
  lastActiveAt: string;
}

export interface AgentMessage {
  from: AgentId;
  to: AgentId;
  body: string;
  correlationId?: string;
}

export type CompletionStatus = "completed" | "failed" | "blocked" | "exhausted";

export interface CompletionPacket {
  sessionId: SessionId;
  status: CompletionStatus;
  summary: string;
}

export type BrainEvent =
  | { type: "started" }
  | { type: "text_delta"; text: string }
  | { type: "tool_call_started"; toolName: string }
  | { type: "tool_call_finished"; toolName: string; isError: boolean }
  | { type: "finished" };

export type BrainAction =
  | { type: "send_message"; message: AgentMessage }
  | {
      type: "request_delegation";
      profileId: ProfileId;
      taskId?: TaskId;
      prompt: string;
    }
  | { type: "deliver_completion"; packet: CompletionPacket };

export type PlatformAdapterKind = "den" | "telegram" | "mcp" | "tui" | "cli";

export interface PlatformAdapterRegistration {
  adapterId: AdapterId;
  kind: PlatformAdapterKind;
}
