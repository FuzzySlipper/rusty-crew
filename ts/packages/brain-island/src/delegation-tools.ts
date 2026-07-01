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

const capacityRequestSchema = Type.Object({
  memberId: Type.String({ minLength: 1 }),
  claimTtlMs: Type.Optional(Type.Number({ minimum: 1 })),
  fallbackPolicy: Type.Optional(
    Type.Union([
      Type.Literal("reject_on_no_capacity"),
      Type.Literal("direct_on_no_capacity"),
    ]),
  ),
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
  capacityRequest: Type.Optional(capacityRequestSchema),
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
      capacityRequest: Type.Optional(capacityRequestSchema),
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

const markdownDelegationParameters = Type.Object({
  markdown: Type.String({ minLength: 1 }),
});

type SpawnSubagentParams = Static<typeof spawnSubagentParameters>;
type FanOutSubagentsParams = Static<typeof fanOutSubagentsParameters>;
type ScoutCodebaseParams = Static<typeof scoutCodebaseParameters>;
type SummarizeFilesParams = Static<typeof summarizeFilesParameters>;
type FindRelevantPathsParams = Static<typeof findRelevantPathsParameters>;
type MarkdownDelegationParams = Static<typeof markdownDelegationParameters>;

export interface DelegationToolContext {
  actions?: BrainActionCollector;
  parentResourceLimits?: ResourceLimits;
  defaultProfileId?: ProfileId | string;
}

export interface DelegationToolDetails {
  ok: boolean;
  operation:
    | "spawn_subagent"
    | "spawn_subagent_md"
    | "fan_out_subagents"
    | "fan_out_subagents_md"
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
    spawnSubagentMarkdownTool(context),
    fanOutSubagentsTool(context),
    fanOutSubagentsMarkdownTool(context),
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
          capacityRequest: params.capacityRequest,
        }),
      ]),
  };
}

export function spawnSubagentMarkdownTool(
  context: DelegationToolContext,
): BrainTool<typeof markdownDelegationParameters, DelegationToolDetails> {
  return {
    name: "spawn_subagent_md",
    label: "Spawn subagent from markdown",
    description:
      "Queue one Rust-owned delegated subagent request from markdown with simple frontmatter. Put stable fields like profile, task, priority, timeout_ms, and parent_consumption in the header; put the delegated prompt in the markdown body. Do not write JSON.",
    parameters: markdownDelegationParameters,
    execute: async (_toolCallId, params: MarkdownDelegationParams) => {
      const parsed = parseSingleDelegationMarkdown(params.markdown, context);
      if (!parsed.ok) {
        return rejected("spawn_subagent_md", parsed.reasonCode);
      }
      return queueDelegationActions(context, "spawn_subagent_md", [
        requestDelegationAction(context, parsed.request),
      ]);
    },
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
          capacityRequest: subagent.capacityRequest,
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

export function fanOutSubagentsMarkdownTool(
  context: DelegationToolContext,
): BrainTool<typeof markdownDelegationParameters, DelegationToolDetails> {
  return {
    name: "fan_out_subagents_md",
    label: "Fan out subagents from markdown",
    description:
      "Queue a bounded fan-out group from markdown. Put group fields like group_id, max_concurrency, failure_policy, timeout_ms, priority, and parent_consumption in frontmatter. Add one markdown section per subagent using '## profile-id' headings and optional key: value lines before the prompt. Do not write JSON.",
    parameters: markdownDelegationParameters,
    execute: async (_toolCallId, params: MarkdownDelegationParams) => {
      const parsed = parseFanOutDelegationMarkdown(params.markdown, context);
      if (!parsed.ok) {
        return rejected("fan_out_subagents_md", parsed.reasonCode);
      }
      return queueDelegationActions(
        context,
        "fan_out_subagents_md",
        parsed.requests.map((request) =>
          requestDelegationAction(context, request),
        ),
        {
          groupId: parsed.groupId,
          failurePolicy: parsed.failurePolicy,
          maxConcurrency: parsed.maxConcurrency,
        },
      );
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
    capacityRequest?: Extract<
      BrainAction,
      { type: "request_delegation" }
    >["capacityRequest"];
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
    capacityRequest: input.capacityRequest,
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

type ParsedDelegationRequest = Parameters<typeof requestDelegationAction>[1];

type ParseResult<T> =
  | { ok: true; value: T }
  | { ok: false; reasonCode: string };

type SingleDelegationParseResult =
  | { ok: true; request: ParsedDelegationRequest }
  | { ok: false; reasonCode: string };

type FanOutDelegationParseResult =
  | {
      ok: true;
      groupId: string;
      failurePolicy: FanOutFailurePolicy;
      maxConcurrency?: number;
      requests: ParsedDelegationRequest[];
    }
  | { ok: false; reasonCode: string };

interface MarkdownEnvelope {
  frontmatter: Record<string, string>;
  bodyMarkdown: string;
}

function parseSingleDelegationMarkdown(
  markdown: string,
  context: DelegationToolContext,
): SingleDelegationParseResult {
  const envelope = parseMarkdownEnvelope(markdown);
  if (!envelope.ok) return envelope;
  const body = envelope.value.bodyMarkdown.trim();
  if (!body) return { ok: false, reasonCode: "markdown_body_required" };
  const mapped = delegationRequestFromFields(
    envelope.value.frontmatter,
    body,
    context,
  );
  return mapped.ok ? { ok: true, request: mapped.value } : mapped;
}

function parseFanOutDelegationMarkdown(
  markdown: string,
  context: DelegationToolContext,
): FanOutDelegationParseResult {
  const envelope = parseMarkdownEnvelope(markdown);
  if (!envelope.ok) return envelope;
  const groupId =
    stringField(envelope.value.frontmatter, "group_id") ??
    stringField(envelope.value.frontmatter, "group");
  if (!groupId) return { ok: false, reasonCode: "fan_out_group_required" };
  const failurePolicy = optionalFailurePolicy(envelope.value.frontmatter);
  if (!failurePolicy.ok) return failurePolicy;
  const maxConcurrency = optionalPositiveIntegerField(
    envelope.value.frontmatter,
    "max_concurrency",
  );
  if (!maxConcurrency.ok) return maxConcurrency;
  const timeoutMs = optionalPositiveIntegerField(
    envelope.value.frontmatter,
    "timeout_ms",
  );
  if (!timeoutMs.ok) return timeoutMs;
  const priority = optionalPriority(envelope.value.frontmatter);
  if (!priority.ok) return priority;
  const parentConsumption = optionalParentConsumption(
    envelope.value.frontmatter,
  );
  if (!parentConsumption.ok) return parentConsumption;
  const sections = parseFanOutSections(envelope.value.bodyMarkdown);
  if (!sections.ok) return sections;
  if (sections.value.length > 20) {
    return { ok: false, reasonCode: "fan_out_max_items_exceeded" };
  }
  const requests: ParsedDelegationRequest[] = [];
  for (const section of sections.value) {
    const mapped = delegationRequestFromFields(
      section.fields,
      section.prompt,
      context,
      section.profile,
    );
    if (!mapped.ok) return mapped;
    requests.push({
      ...mapped.value,
      timeoutMs: mapped.value.timeoutMs ?? timeoutMs.value,
      priority: mapped.value.priority ?? priority.value,
      fanOutGroupId: groupId,
      fanOutMaxConcurrency: maxConcurrency.value,
      fanOutFailurePolicy: failurePolicy.value ?? "fail_soft",
      parentConsumption:
        mapped.value.parentConsumption ?? parentConsumption.value,
      expectedOutput:
        mapped.value.expectedOutput ??
        stringField(envelope.value.frontmatter, "expected_output"),
    });
  }
  return {
    ok: true,
    groupId,
    failurePolicy: failurePolicy.value ?? "fail_soft",
    maxConcurrency: maxConcurrency.value,
    requests,
  };
}

function delegationRequestFromFields(
  fields: Record<string, string>,
  prompt: string,
  context: DelegationToolContext,
  profileFallback?: string,
): ParseResult<ParsedDelegationRequest> {
  const profile =
    stringField(fields, "profile") ??
    stringField(fields, "profile_id") ??
    profileFallback;
  const timeoutMs = optionalPositiveIntegerField(fields, "timeout_ms");
  if (!timeoutMs.ok) return timeoutMs;
  const maxDurationMs = optionalPositiveIntegerField(fields, "max_duration_ms");
  if (!maxDurationMs.ok) return maxDurationMs;
  const maxDelegationDepth = optionalNonNegativeIntegerField(
    fields,
    "max_delegation_depth",
  );
  if (!maxDelegationDepth.ok) return maxDelegationDepth;
  const priority = optionalPriority(fields);
  if (!priority.ok) return priority;
  const parentConsumption = optionalParentConsumption(fields);
  if (!parentConsumption.ok) return parentConsumption;
  const capacityRequest = optionalCapacityRequest(fields);
  if (!capacityRequest.ok) return capacityRequest;
  return {
    ok: true,
    value: {
      profileId: profileId(profile, context),
      prompt,
      taskId: stringField(fields, "task") ?? stringField(fields, "task_id"),
      expectedOutput:
        stringField(fields, "expected_output") ??
        sectionByHeading(prompt, "Expected Output"),
      resourceLimits: compactResourceLimits({
        workdir: stringField(fields, "workdir"),
        maxDurationMs: maxDurationMs.value,
        maxDelegationDepth: maxDelegationDepth.value,
      }),
      timeoutMs: timeoutMs.value,
      priority: priority.value,
      correlationId:
        stringField(fields, "correlation") ??
        stringField(fields, "correlation_id"),
      parentConsumption: parentConsumption.value,
      capacityRequest: capacityRequest.value,
    },
  };
}

function parseMarkdownEnvelope(
  markdown: string,
): ParseResult<MarkdownEnvelope> {
  const normalized = markdown.replace(/\r\n/g, "\n").trim();
  if (!normalized) return { ok: false, reasonCode: "markdown_body_required" };
  if (!normalized.startsWith("---\n")) {
    return { ok: true, value: { frontmatter: {}, bodyMarkdown: normalized } };
  }
  const closing = normalized.indexOf("\n---", 4);
  if (closing === -1) {
    return { ok: false, reasonCode: "invalid_frontmatter" };
  }
  const closeEnd = closing + "\n---".length;
  const afterClose = normalized.slice(closeEnd);
  if (afterClose.length > 0 && !afterClose.startsWith("\n")) {
    return { ok: false, reasonCode: "invalid_frontmatter" };
  }
  const frontmatter = parseSimpleFrontmatter(normalized.slice(4, closing));
  if (!frontmatter.ok) return frontmatter;
  return {
    ok: true,
    value: {
      frontmatter: frontmatter.value,
      bodyMarkdown: afterClose.trim(),
    },
  };
}

function parseSimpleFrontmatter(
  raw: string,
): ParseResult<Record<string, string>> {
  const fields: Record<string, string> = {};
  for (const line of raw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const match = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (!match) return { ok: false, reasonCode: "invalid_frontmatter" };
    const key = normalizeFieldName(match[1]!);
    const value = unquote(match[2]!.trim());
    if (!value) return { ok: false, reasonCode: "invalid_frontmatter" };
    fields[key] = value;
  }
  return { ok: true, value: fields };
}

interface FanOutSection {
  profile: string;
  fields: Record<string, string>;
  prompt: string;
}

function parseFanOutSections(markdown: string): ParseResult<FanOutSection[]> {
  const sections: FanOutSection[] = [];
  const heading = /^##\s+(.+)$/gm;
  const matches = [...markdown.matchAll(heading)];
  if (matches.length === 0) {
    return { ok: false, reasonCode: "fan_out_subagents_required" };
  }
  for (let index = 0; index < matches.length; index += 1) {
    const match = matches[index]!;
    const next = matches[index + 1];
    const profile = match[1]!.replace(/^subagent:\s*/i, "").trim();
    const rawSection = markdown
      .slice(match.index! + match[0].length, next?.index ?? markdown.length)
      .trim();
    const extracted = extractLeadingFields(rawSection);
    if (!profile && !stringField(extracted.fields, "profile")) {
      return { ok: false, reasonCode: "profile_required" };
    }
    if (!extracted.body.trim()) {
      return { ok: false, reasonCode: "markdown_body_required" };
    }
    sections.push({
      profile,
      fields: extracted.fields,
      prompt: extracted.body.trim(),
    });
  }
  return { ok: true, value: sections };
}

function extractLeadingFields(markdown: string): {
  fields: Record<string, string>;
  body: string;
} {
  const fields: Record<string, string> = {};
  const lines = markdown.split("\n");
  let cursor = 0;
  for (; cursor < lines.length; cursor += 1) {
    const line = lines[cursor]!;
    if (!line.trim()) {
      cursor += 1;
      break;
    }
    const match = /^([A-Za-z0-9_-]+):\s*(.+)$/.exec(line.trim());
    if (!match) break;
    fields[normalizeFieldName(match[1]!)] = unquote(match[2]!.trim());
  }
  return { fields, body: lines.slice(cursor).join("\n") };
}

function optionalPriority(
  fields: Record<string, string>,
): ParseResult<"low" | "normal" | "high" | undefined> {
  const raw = stringField(fields, "priority");
  if (raw === undefined) return { ok: true, value: undefined };
  if (raw === "low" || raw === "normal" || raw === "high") {
    return { ok: true, value: raw };
  }
  return { ok: false, reasonCode: "invalid_priority" };
}

function optionalParentConsumption(
  fields: Record<string, string>,
): ParseResult<ParentConsumptionPolicy | undefined> {
  const raw = stringField(fields, "parent_consumption");
  if (raw === undefined) return { ok: true, value: undefined };
  if (raw === "await_completion" || raw === "observe_only") {
    return { ok: true, value: raw };
  }
  return { ok: false, reasonCode: "invalid_parent_consumption" };
}

function optionalCapacityRequest(
  fields: Record<string, string>,
): ParseResult<
  | Extract<BrainAction, { type: "request_delegation" }>["capacityRequest"]
  | undefined
> {
  const memberId =
    stringField(fields, "pool_member_id") ??
    stringField(fields, "pool_member") ??
    stringField(fields, "worker_pool_member_id");
  if (memberId === undefined) return { ok: true, value: undefined };
  const claimTtlMs = optionalPositiveIntegerField(fields, "pool_claim_ttl_ms");
  if (!claimTtlMs.ok) return claimTtlMs;
  const fallback =
    stringField(fields, "pool_fallback_policy") ??
    stringField(fields, "pool_fallback");
  if (
    fallback !== undefined &&
    fallback !== "reject_on_no_capacity" &&
    fallback !== "direct_on_no_capacity"
  ) {
    return { ok: false, reasonCode: "invalid_pool_fallback_policy" };
  }
  return {
    ok: true,
    value: {
      memberId,
      claimTtlMs: claimTtlMs.value,
      fallbackPolicy: fallback,
    },
  };
}

function optionalFailurePolicy(
  fields: Record<string, string>,
): ParseResult<FanOutFailurePolicy | undefined> {
  const raw = stringField(fields, "failure_policy");
  if (raw === undefined) return { ok: true, value: undefined };
  if (raw === "fail_fast" || raw === "fail_soft") {
    return { ok: true, value: raw };
  }
  return { ok: false, reasonCode: "invalid_failure_policy" };
}

function optionalPositiveIntegerField(
  fields: Record<string, string>,
  key: string,
): ParseResult<number | undefined> {
  const value = stringField(fields, key);
  if (value === undefined) return { ok: true, value: undefined };
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 1) {
    return { ok: false, reasonCode: `invalid_${key}` };
  }
  return { ok: true, value: parsed };
}

function optionalNonNegativeIntegerField(
  fields: Record<string, string>,
  key: string,
): ParseResult<number | undefined> {
  const value = stringField(fields, key);
  if (value === undefined) return { ok: true, value: undefined };
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 0) {
    return { ok: false, reasonCode: `invalid_${key}` };
  }
  return { ok: true, value: parsed };
}

function stringField(
  fields: Record<string, string>,
  key: string,
): string | undefined {
  const value = fields[normalizeFieldName(key)]?.trim();
  return value ? value : undefined;
}

function compactResourceLimits(
  input: ResourceLimits,
): ResourceLimits | undefined {
  return input.workdir !== undefined ||
    input.maxDurationMs !== undefined ||
    input.maxDelegationDepth !== undefined
    ? input
    : undefined;
}

function sectionByHeading(
  markdown: string,
  heading: string,
): string | undefined {
  const pattern = new RegExp(
    `^##\\s+${escapeRegExp(heading)}\\s*$\\n([\\s\\S]*?)(?=^##\\s+|$)`,
    "im",
  );
  return pattern.exec(markdown)?.[1]?.trim() || undefined;
}

function normalizeFieldName(value: string): string {
  return value.trim().toLowerCase().replace(/-/g, "_");
}

function unquote(value: string): string {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1).trim();
  }
  return value.trim();
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function rejected(
  operation: DelegationToolDetails["operation"],
  reasonCode: string,
): BrainToolResult<DelegationToolDetails> {
  return result({
    ok: false,
    operation,
    reasonCode,
    queuedActions: 0,
    actions: [],
  });
}

function result(
  details: DelegationToolDetails,
): BrainToolResult<DelegationToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}
