import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type {
  BrainAction,
  FanOutFailurePolicy,
  ParentConsumptionPolicy,
  ProfileId,
  ResourceLimits,
  TaskId,
} from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import type {
  BrainActionCollector,
  BrainToolResolver,
} from "./tool-session-selection.js";

const prioritySchema = Type.Union([
  Type.Literal("low"),
  Type.Literal("normal"),
  Type.Literal("high"),
]);
const parentConsumptionSchema = Type.Union([
  Type.Literal("await_completion"),
  Type.Literal("observe_only"),
]);
const failurePolicySchema = Type.Union([
  Type.Literal("fail_fast"),
  Type.Literal("fail_soft"),
]);

const delegationResourceSchema = Type.Object({
  workdir: Type.Optional(Type.String({ minLength: 1 })),
  maxDurationMs: Type.Optional(Type.Number({ minimum: 1 })),
  maxDelegationDepth: Type.Optional(Type.Number({ minimum: 0 })),
});

const spawnSubagentParameters = Type.Object({
  profileId: Type.String({ minLength: 1 }),
  prompt: Type.String({ minLength: 1 }),
  taskId: Type.Optional(Type.String({ minLength: 1 })),
  expectedOutput: Type.Optional(Type.String({ minLength: 1 })),
  resourceLimits: Type.Optional(delegationResourceSchema),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
  priority: Type.Optional(prioritySchema),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
  parentConsumption: Type.Optional(parentConsumptionSchema),
});

const fanOutSubagentsParameters = Type.Object({
  groupId: Type.String({ minLength: 1 }),
  maxConcurrency: Type.Optional(Type.Number({ minimum: 1 })),
  failurePolicy: Type.Optional(failurePolicySchema),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
  priority: Type.Optional(prioritySchema),
  parentConsumption: Type.Optional(parentConsumptionSchema),
  expectedOutput: Type.Optional(Type.String({ minLength: 1 })),
  resourceLimits: Type.Optional(delegationResourceSchema),
  subagents: Type.Array(
    Type.Object({
      profileId: Type.String({ minLength: 1 }),
      prompt: Type.String({ minLength: 1 }),
      taskId: Type.Optional(Type.String({ minLength: 1 })),
      expectedOutput: Type.Optional(Type.String({ minLength: 1 })),
      correlationId: Type.Optional(Type.String({ minLength: 1 })),
      resourceLimits: Type.Optional(delegationResourceSchema),
    }),
    { minItems: 1, maxItems: 20 },
  ),
});

const scoutCodebaseParameters = Type.Object({
  goal: Type.String({ minLength: 1 }),
  profileId: Type.Optional(Type.String({ minLength: 1 })),
  taskId: Type.Optional(Type.String({ minLength: 1 })),
  paths: Type.Optional(
    Type.Array(Type.String({ minLength: 1 }), {
      maxItems: 40,
    }),
  ),
  expectedOutput: Type.Optional(Type.String({ minLength: 1 })),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
});

const summarizeFilesParameters = Type.Object({
  files: Type.Array(Type.String({ minLength: 1 }), {
    minItems: 1,
    maxItems: 40,
  }),
  focus: Type.Optional(Type.String({ minLength: 1 })),
  profileId: Type.Optional(Type.String({ minLength: 1 })),
  taskId: Type.Optional(Type.String({ minLength: 1 })),
  expectedOutput: Type.Optional(Type.String({ minLength: 1 })),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
});

const findRelevantPathsParameters = Type.Object({
  query: Type.String({ minLength: 1 }),
  profileId: Type.Optional(Type.String({ minLength: 1 })),
  taskId: Type.Optional(Type.String({ minLength: 1 })),
  roots: Type.Optional(
    Type.Array(Type.String({ minLength: 1 }), {
      maxItems: 20,
    }),
  ),
  expectedOutput: Type.Optional(Type.String({ minLength: 1 })),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
});

type SpawnSubagentParams = Static<typeof spawnSubagentParameters>;
type FanOutSubagentsParams = Static<typeof fanOutSubagentsParameters>;
type ScoutCodebaseParams = Static<typeof scoutCodebaseParameters>;
type SummarizeFilesParams = Static<typeof summarizeFilesParameters>;
type FindRelevantPathsParams = Static<typeof findRelevantPathsParameters>;

export interface DelegationToolContext {
  actions?: BrainActionCollector;
  parentResourceLimits?: ResourceLimits;
  defaultProfileId?: ProfileId | string;
}

export interface DelegationToolDetails {
  ok: boolean;
  operation:
    | "spawn_subagent"
    | "fan_out_subagents"
    | "scout_codebase"
    | "summarize_files"
    | "find_relevant_paths";
  reasonCode?: string;
  queuedActions: number;
  actions: BrainAction[];
  groupId?: string;
  failurePolicy?: FanOutFailurePolicy;
  maxConcurrency?: number;
}

export const resolveDelegationTools: BrainToolResolver = ({ wake, actions }) =>
  delegationTools({
    actions,
    parentResourceLimits: wake.state.session.resourceLimits,
  });

export function delegationTools(context: DelegationToolContext): BrainTool[] {
  return [
    spawnSubagentTool(context),
    fanOutSubagentsTool(context),
    scoutCodebaseTool(context),
    summarizeFilesTool(context),
    findRelevantPathsTool(context),
  ];
}

export function spawnSubagentTool(
  context: DelegationToolContext,
): BrainTool<typeof spawnSubagentParameters, DelegationToolDetails> {
  return {
    name: "spawn_subagent",
    label: "Spawn subagent",
    description:
      "Queue one Rust-owned delegated subagent request through BrainAction::RequestDelegation.",
    parameters: spawnSubagentParameters,
    execute: async (_toolCallId, params: SpawnSubagentParams) =>
      queueDelegationActions(context, "spawn_subagent", [
        requestDelegationAction(context, {
          profileId: params.profileId,
          prompt: params.prompt,
          taskId: params.taskId,
          expectedOutput: params.expectedOutput,
          resourceLimits: params.resourceLimits,
          timeoutMs: params.timeoutMs,
          priority: params.priority,
          correlationId: params.correlationId,
          parentConsumption: params.parentConsumption,
        }),
      ]),
  };
}

export function fanOutSubagentsTool(
  context: DelegationToolContext,
): BrainTool<typeof fanOutSubagentsParameters, DelegationToolDetails> {
  return {
    name: "fan_out_subagents",
    label: "Fan out subagents",
    description:
      "Queue a bounded group of Rust-owned delegated subagent requests with a shared fan-out policy.",
    parameters: fanOutSubagentsParameters,
    execute: async (_toolCallId, params: FanOutSubagentsParams) => {
      const failurePolicy = params.failurePolicy ?? "fail_soft";
      const actions = params.subagents.map((subagent) =>
        requestDelegationAction(context, {
          profileId: subagent.profileId,
          prompt: subagent.prompt,
          taskId: subagent.taskId,
          expectedOutput: subagent.expectedOutput ?? params.expectedOutput,
          resourceLimits: subagent.resourceLimits ?? params.resourceLimits,
          timeoutMs: params.timeoutMs,
          priority: params.priority,
          fanOutGroupId: params.groupId,
          fanOutMaxConcurrency: params.maxConcurrency,
          fanOutFailurePolicy: failurePolicy,
          correlationId: subagent.correlationId,
          parentConsumption: params.parentConsumption,
        }),
      );
      return queueDelegationActions(context, "fan_out_subagents", actions, {
        groupId: params.groupId,
        failurePolicy,
        maxConcurrency: params.maxConcurrency,
      });
    },
  };
}

export function scoutCodebaseTool(
  context: DelegationToolContext,
): BrainTool<typeof scoutCodebaseParameters, DelegationToolDetails> {
  return {
    name: "scout_codebase",
    label: "Scout codebase",
    description:
      "Delegate a read-only codebase scouting task and request concise evidence back.",
    parameters: scoutCodebaseParameters,
    execute: async (_toolCallId, params: ScoutCodebaseParams) =>
      queueDelegationActions(context, "scout_codebase", [
        requestDelegationAction(context, {
          profileId: profileId(params.profileId, context),
          taskId: params.taskId,
          prompt: [
            "Scout the codebase for the requested goal.",
            `Goal: ${params.goal}`,
            params.paths?.length
              ? `Suggested paths:\n${bullets(params.paths)}`
              : "",
            "Return relevant files, symbols, and evidence. Do not edit files.",
          ]
            .filter(Boolean)
            .join("\n\n"),
          expectedOutput:
            params.expectedOutput ??
            "Concise scouting report with relevant paths and evidence.",
          resourceLimits: { maxDelegationDepth: 0 },
          timeoutMs: params.timeoutMs,
          correlationId: params.correlationId,
          parentConsumption: "await_completion",
        }),
      ]),
  };
}

export function summarizeFilesTool(
  context: DelegationToolContext,
): BrainTool<typeof summarizeFilesParameters, DelegationToolDetails> {
  return {
    name: "summarize_files",
    label: "Summarize files",
    description:
      "Delegate a read-only file summarization task for bounded context gathering.",
    parameters: summarizeFilesParameters,
    execute: async (_toolCallId, params: SummarizeFilesParams) =>
      queueDelegationActions(context, "summarize_files", [
        requestDelegationAction(context, {
          profileId: profileId(params.profileId, context),
          taskId: params.taskId,
          prompt: [
            "Summarize the following files for the parent agent.",
            bullets(params.files),
            params.focus ? `Focus: ${params.focus}` : "",
            "Return key behavior, important types/functions, and any risks. Do not edit files.",
          ]
            .filter(Boolean)
            .join("\n\n"),
          expectedOutput:
            params.expectedOutput ??
            "Structured file summary with path-specific notes.",
          resourceLimits: { maxDelegationDepth: 0 },
          timeoutMs: params.timeoutMs,
          correlationId: params.correlationId,
          parentConsumption: "await_completion",
        }),
      ]),
  };
}

export function findRelevantPathsTool(
  context: DelegationToolContext,
): BrainTool<typeof findRelevantPathsParameters, DelegationToolDetails> {
  return {
    name: "find_relevant_paths",
    label: "Find relevant paths",
    description:
      "Delegate a read-only search for files likely relevant to a question or implementation task.",
    parameters: findRelevantPathsParameters,
    execute: async (_toolCallId, params: FindRelevantPathsParams) =>
      queueDelegationActions(context, "find_relevant_paths", [
        requestDelegationAction(context, {
          profileId: profileId(params.profileId, context),
          taskId: params.taskId,
          prompt: [
            "Find paths relevant to the parent agent's query.",
            `Query: ${params.query}`,
            params.roots?.length
              ? `Search roots:\n${bullets(params.roots)}`
              : "",
            "Return a ranked list of paths with short reasons and evidence. Do not edit files.",
          ]
            .filter(Boolean)
            .join("\n\n"),
          expectedOutput:
            params.expectedOutput ??
            "Ranked relevant path list with evidence snippets.",
          resourceLimits: { maxDelegationDepth: 0 },
          timeoutMs: params.timeoutMs,
          correlationId: params.correlationId,
          parentConsumption: "await_completion",
        }),
      ]),
  };
}

function requestDelegationAction(
  context: DelegationToolContext,
  input: {
    profileId: string;
    prompt: string;
    taskId?: string;
    expectedOutput?: string;
    resourceLimits?: ResourceLimits;
    timeoutMs?: number;
    priority?: "low" | "normal" | "high";
    fanOutGroupId?: string;
    fanOutMaxConcurrency?: number;
    fanOutFailurePolicy?: FanOutFailurePolicy;
    correlationId?: string;
    parentConsumption?: ParentConsumptionPolicy;
  },
): BrainAction {
  return {
    type: "request_delegation",
    profileId: input.profileId as ProfileId,
    taskId: input.taskId as TaskId | undefined,
    prompt: input.prompt,
    expectedOutput: input.expectedOutput,
    resourceLimits: childResourceLimits(context, input.resourceLimits),
    timeoutMs: input.timeoutMs,
    priority: input.priority,
    fanOutGroupId: input.fanOutGroupId,
    fanOutMaxConcurrency: input.fanOutMaxConcurrency,
    fanOutFailurePolicy: input.fanOutFailurePolicy,
    correlationId: input.correlationId,
    parentConsumption: input.parentConsumption,
  };
}

function queueDelegationActions(
  context: DelegationToolContext,
  operation: DelegationToolDetails["operation"],
  actions: BrainAction[],
  metadata: Pick<
    DelegationToolDetails,
    "groupId" | "failurePolicy" | "maxConcurrency"
  > = {},
): BrainToolResult<DelegationToolDetails> {
  if (!context.actions) {
    return result({
      ok: false,
      operation,
      reasonCode: "delegation_action_collector_unavailable",
      queuedActions: 0,
      actions: [],
      ...metadata,
    });
  }
  const parentDepth = context.parentResourceLimits?.maxDelegationDepth;
  if (parentDepth === 0) {
    return result({
      ok: false,
      operation,
      reasonCode: "delegation_depth_exhausted",
      queuedActions: 0,
      actions: [],
      ...metadata,
    });
  }
  context.actions.addMany(actions);
  return result({
    ok: true,
    operation,
    queuedActions: actions.length,
    actions,
    ...metadata,
  });
}

function childResourceLimits(
  context: DelegationToolContext,
  requested: ResourceLimits | undefined,
): ResourceLimits {
  const parent = context.parentResourceLimits ?? {};
  const parentDepth = parent.maxDelegationDepth;
  const inheritedDepth =
    parentDepth === undefined ? undefined : Math.max(0, parentDepth - 1);
  return {
    workdir: requested?.workdir ?? parent.workdir,
    maxDurationMs: requested?.maxDurationMs ?? parent.maxDurationMs,
    maxDelegationDepth: requested?.maxDelegationDepth ?? inheritedDepth,
  };
}

function profileId(
  requested: string | undefined,
  context: DelegationToolContext,
): string {
  return requested ?? context.defaultProfileId ?? "coder-profile";
}

function bullets(values: readonly string[]): string {
  return values.map((value) => `- ${value}`).join("\n");
}

function result(
  details: DelegationToolDetails,
): BrainToolResult<DelegationToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}
