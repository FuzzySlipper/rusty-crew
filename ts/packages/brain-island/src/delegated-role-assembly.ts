import type {
  AgentId,
  AgentMessage,
  ProfileId,
  ResourceLimits,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";
import type { BrainRoleAssembly } from "./index.js";

export type DelegatedRole = "coder" | "reviewer" | "packet_auditor";
export type DelegatedRoleInput = DelegatedRole | "packet-auditor";

export interface DelegatedProfileData {
  profileId: ProfileId;
  displayName?: string;
  systemPrompt?: string;
  toolNames?: string[];
}

export interface DelegationRoleContext {
  sessionId: SessionId;
  agentId: AgentId;
  parentSessionId?: SessionId;
  parentAgentId?: AgentId;
  sourceWakeId?: string;
  sourceActionIndex?: number;
  taskId?: TaskId;
  prompt: string;
  expectedOutput?: string;
  correlationId?: string;
  resourceLimits?: ResourceLimits;
  taskContext?: string;
  acceptanceCriteria?: string[];
  parentInstructions?: string;
}

export interface BuildDelegatedRoleAssemblyInput {
  role: DelegatedRoleInput;
  profile: DelegatedProfileData;
  context: DelegationRoleContext;
}

export function normalizeDelegatedRole(
  role: DelegatedRoleInput,
): DelegatedRole {
  return role === "packet-auditor" ? "packet_auditor" : role;
}

export function buildDelegatedRoleAssembly(
  input: BuildDelegatedRoleAssemblyInput,
): BrainRoleAssembly {
  const role = normalizeDelegatedRole(input.role);
  const instructions = [
    baseRoleInstruction(role),
    "",
    "## Profile",
    `- profileId: ${input.profile.profileId}`,
    input.profile.displayName
      ? `- displayName: ${input.profile.displayName}`
      : undefined,
    input.profile.systemPrompt,
    "",
    "## Delegation Context",
    `- sessionId: ${input.context.sessionId}`,
    `- agentId: ${input.context.agentId}`,
    input.context.parentSessionId
      ? `- parentSessionId: ${input.context.parentSessionId}`
      : undefined,
    input.context.parentAgentId
      ? `- parentAgentId: ${input.context.parentAgentId}`
      : undefined,
    input.context.sourceWakeId
      ? `- sourceWakeId: ${input.context.sourceWakeId}`
      : undefined,
    input.context.sourceActionIndex !== undefined
      ? `- sourceActionIndex: ${input.context.sourceActionIndex}`
      : undefined,
    input.context.taskId ? `- taskId: ${input.context.taskId}` : undefined,
    input.context.correlationId
      ? `- correlationId: ${input.context.correlationId}`
      : undefined,
    "",
    "## Resource Limits",
    resourceLimitsText(input.context.resourceLimits),
    "",
    "## Delegated Prompt",
    input.context.prompt,
    input.context.taskContext
      ? ["", "## Task Context", input.context.taskContext]
      : undefined,
    input.context.acceptanceCriteria?.length
      ? [
          "",
          "## Acceptance Criteria",
          ...input.context.acceptanceCriteria.map(
            (criterion) => `- ${criterion}`,
          ),
        ]
      : undefined,
    input.context.expectedOutput
      ? ["", "## Expected Output", input.context.expectedOutput]
      : undefined,
    input.context.parentInstructions
      ? ["", "## Parent Instructions", input.context.parentInstructions]
      : undefined,
    "",
    completionInstruction(role),
    toolInstruction(input.profile.toolNames),
  ]
    .flat(2)
    .filter(
      (part): part is string => part !== undefined && part.trim().length > 0,
    )
    .join("\n");

  return {
    instructions,
    initialMessages: [
      {
        from: (input.context.parentAgentId ?? "rusty-core") as AgentId,
        to: input.context.agentId,
        body: [
          `Begin delegated ${role} work for session ${input.context.sessionId}.`,
          "",
          input.context.prompt,
          input.context.expectedOutput
            ? `\nExpected output: ${input.context.expectedOutput}`
            : undefined,
        ]
          .filter(Boolean)
          .join("\n"),
        correlationId: input.context.correlationId,
      } satisfies AgentMessage,
    ],
  };
}

function baseRoleInstruction(role: DelegatedRole): string {
  switch (role) {
    case "coder":
      return [
        "You are a bounded delegated coder subagent.",
        "Implement only the requested slice, preserve existing project boundaries, and verify behavior before reporting completion.",
      ].join("\n");
    case "reviewer":
      return [
        "You are a bounded delegated reviewer subagent.",
        "Inspect the requested change or evidence, produce concrete findings, and say when the work looks good.",
      ].join("\n");
    case "packet_auditor":
      return [
        "You are a bounded packet auditor subagent.",
        "Validate completion packets against the Rusty Crew delegation contract and report structured findings.",
      ].join("\n");
  }
}

function completionInstruction(role: DelegatedRole): string {
  switch (role) {
    case "coder":
      return "Return a completion packet with status completed, failed, blocked, or exhausted and a concise implementation summary.";
    case "reviewer":
      return "Return a review completion packet with concrete findings or a clear looks-good summary.";
    case "packet_auditor":
      return "Return an audit completion packet. Valid packet statuses are completed, failed, blocked, and exhausted.";
  }
}

function resourceLimitsText(limits: ResourceLimits | undefined): string {
  if (!limits) {
    return "- No child-specific resource limits were supplied.";
  }
  return [
    limits.workdir ? `- workdir: ${limits.workdir}` : undefined,
    limits.maxDurationMs !== undefined
      ? `- maxDurationMs: ${limits.maxDurationMs}`
      : undefined,
    limits.maxDelegationDepth !== undefined
      ? `- maxDelegationDepth: ${limits.maxDelegationDepth}`
      : undefined,
  ]
    .filter((part): part is string => part !== undefined)
    .join("\n");
}

function toolInstruction(toolNames: string[] | undefined): string {
  if (!toolNames?.length) {
    return "Use only the tools resolved for this delegated profile.";
  }
  return [
    "Use only the tools resolved for this delegated profile:",
    ...toolNames.map((name) => `- ${name}`),
  ].join("\n");
}
