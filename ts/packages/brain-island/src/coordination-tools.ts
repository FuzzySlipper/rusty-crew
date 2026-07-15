import type { AgentDirectoryEntry, AgentId } from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type {
  BrainActionCollector,
  BrainToolResolver,
} from "./tool-session-selection.js";

const sendAgentMessageParameters = Type.Object({
  toAgentId: Type.String({ minLength: 1 }),
  body: Type.String({ minLength: 1 }),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
  requireWake: Type.Optional(Type.Boolean()),
  ttlSeconds: Type.Optional(Type.Integer({ minimum: 1, maximum: 86_400 })),
});

const replyAgentMessageParameters = Type.Object({
  messageId: Type.String({ minLength: 1 }),
  body: Type.String({ minLength: 1 }),
  ttlSeconds: Type.Optional(Type.Integer({ minimum: 1, maximum: 86_400 })),
});

const agentRoundParameters = Type.Object({
  toAgentId: Type.String({ minLength: 1 }),
  body: Type.String({ minLength: 1 }),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
});

const listAgentsParameters = Type.Object({}, { additionalProperties: false });

type SendAgentMessageParams = Static<typeof sendAgentMessageParameters>;
type ReplyAgentMessageParams = Static<typeof replyAgentMessageParameters>;
type AgentRoundParams = Static<typeof agentRoundParameters>;

export interface AgentMessageRouteResult {
  accepted: boolean;
  sequence?: number;
  wake?: {
    status: "completed" | "skipped" | "failed";
    wakeId?: string;
    summary: string;
    reasonCode?: string;
  };
}

export interface AgentRoundResult extends AgentMessageRouteResult {
  reply?: {
    from: string;
    to: string;
    body: string;
    correlationId?: string;
  };
  timedOut?: boolean;
}

export interface CoordinationToolRuntime {
  listAgents(): Promise<AgentDirectoryEntry[]>;
  routeMessage(input: {
    fromAgentId: string;
    fromSessionId: string;
    wakeId: string;
    toolCallId: string;
    toAgentId: string;
    body: string;
    correlationId?: string;
    requireWake?: boolean;
    ttlSeconds?: number;
  }): Promise<AgentMessageRouteResult>;
  replyMessage(input: {
    fromAgentId: string;
    fromSessionId: string;
    wakeId: string;
    toolCallId: string;
    messageId: string;
    body: string;
    ttlSeconds?: number;
  }): Promise<AgentMessageRouteResult>;
  roundTrip(input: {
    fromAgentId: string;
    fromSessionId: string;
    wakeId: string;
    toolCallId: string;
    toAgentId: string;
    body: string;
    correlationId: string;
    timeoutMs: number;
  }): Promise<AgentRoundResult>;
}

export interface CoordinationToolContext {
  actions?: BrainActionCollector;
  runtime?: CoordinationToolRuntime;
}

export interface CoordinationToolDetails {
  ok: boolean;
  operation:
    | "list_agents"
    | "send_agent_message"
    | "reply_agent_message"
    | "agent_round";
  reasonCode?: string;
  agents?: AgentDirectoryEntry[];
  routed?: AgentMessageRouteResult;
  round?: AgentRoundResult;
  queuedActions: number;
}

export const resolveCoordinationTools: BrainToolResolver = ({ actions }) =>
  coordinationTools({
    actions,
    runtime: undefined,
  });

export function createCoordinationToolResolver(
  runtime?: CoordinationToolRuntime,
): BrainToolResolver {
  return ({ actions }) => coordinationTools({ actions, runtime });
}

export function coordinationTools(
  context: CoordinationToolContext,
): BrainTool[] {
  return [
    listAgentsTool(context),
    sendAgentMessageTool(context),
    replyAgentMessageTool(context),
    agentRoundTool(context),
  ];
}

export function replyAgentMessageTool(
  context: CoordinationToolContext,
): BrainTool<typeof replyAgentMessageParameters, CoordinationToolDetails> {
  return {
    name: "reply_agent_message",
    label: "Reply to agent message",
    description:
      "Reply once to a routed Rusty Crew message by its message ID; Crew resolves the sender and correlation.",
    parameters: replyAgentMessageParameters,
    executeWithContext: async (params, toolContext) =>
      replyAgentMessage(context, {
        fromAgentId: toolContext.wake.state.session.agentId,
        fromSessionId: toolContext.sessionId,
        wakeId: toolContext.wakeId,
        toolCallId: toolContext.callId,
        params,
      }),
    execute: async () =>
      coordinationResult({
        ok: false,
        operation: "reply_agent_message",
        reasonCode: "tool_context_required",
        queuedActions: 0,
        text: "reply_agent_message requires wake context.",
      }),
  };
}

export function listAgentsTool(
  context: CoordinationToolContext,
): BrainTool<typeof listAgentsParameters, CoordinationToolDetails> {
  const execute = async () => {
    if (context.runtime === undefined) {
      return coordinationResult({
        ok: false,
        operation: "list_agents",
        reasonCode: "coordination_runtime_unavailable",
        queuedActions: 0,
        text: "list_agents requires the Rusty Crew service coordination runtime.",
      });
    }
    const agents = await context.runtime.listAgents();
    return coordinationResult({
      ok: true,
      operation: "list_agents",
      agents,
      queuedActions: 0,
      text: formatAgentDirectory(agents),
    });
  };
  return {
    name: "list_agents",
    label: "List agents",
    description:
      "List agents addressable through this Rusty Crew service and their stable recipient IDs.",
    parameters: listAgentsParameters,
    executeWithContext: execute,
    execute: execute,
  };
}

export function sendAgentMessageTool(
  context: CoordinationToolContext,
): BrainTool<typeof sendAgentMessageParameters, CoordinationToolDetails> {
  return {
    name: "send_agent_message",
    label: "Send agent message",
    description:
      "Route a Rusty Crew internal message to another agent and request a wake when the service runtime is available.",
    parameters: sendAgentMessageParameters,
    executeWithContext: async (params, toolContext) =>
      sendAgentMessage(context, {
        fromAgentId: toolContext.wake.state.session.agentId,
        fromSessionId: toolContext.sessionId,
        wakeId: toolContext.wakeId,
        toolCallId: toolContext.callId,
        params,
      }),
    execute: async (_callId, params) =>
      sendAgentMessage(context, {
        fromAgentId: undefined,
        fromSessionId: undefined,
        wakeId: undefined,
        toolCallId: undefined,
        params,
      }),
  };
}

export function agentRoundTool(
  context: CoordinationToolContext,
): BrainTool<typeof agentRoundParameters, CoordinationToolDetails> {
  return {
    name: "agent_round",
    label: "Agent round",
    description:
      "Send an internal message to another agent, wake it, and wait for one correlated reply.",
    parameters: agentRoundParameters,
    executeWithContext: async (params, toolContext) => {
      const fromAgentId = toolContext.wake.state.session.agentId;
      if (context.runtime === undefined) {
        return coordinationResult({
          ok: false,
          operation: "agent_round",
          reasonCode: "coordination_runtime_unavailable",
          queuedActions: 0,
          text: "agent_round requires the Rusty Crew service coordination runtime.",
        });
      }
      const correlationId =
        params.correlationId ??
        `${toolContext.sessionId}:${toolContext.callId}:agent-round`;
      const round = await context.runtime.roundTrip({
        fromAgentId,
        fromSessionId: toolContext.sessionId,
        wakeId: toolContext.wakeId,
        toolCallId: toolContext.callId,
        toAgentId: params.toAgentId,
        body: params.body,
        correlationId,
        timeoutMs: Math.min(Math.max(params.timeoutMs ?? 30_000, 1), 300_000),
      });
      return coordinationResult({
        ok: round.accepted && round.reply !== undefined && !round.timedOut,
        operation: "agent_round",
        reasonCode: round.timedOut ? "agent_round_timeout" : undefined,
        queuedActions: 0,
        round,
        text:
          round.reply === undefined
            ? `round message sent to ${params.toAgentId}; no reply received`
            : `reply from ${round.reply.from}: ${round.reply.body}`,
      });
    },
    execute: async () =>
      coordinationResult({
        ok: false,
        operation: "agent_round",
        reasonCode: "tool_context_required",
        queuedActions: 0,
        text: "agent_round requires wake context.",
      }),
  };
}

async function sendAgentMessage(
  context: CoordinationToolContext,
  input: {
    fromAgentId: string | undefined;
    fromSessionId: string | undefined;
    wakeId: string | undefined;
    toolCallId: string | undefined;
    params: SendAgentMessageParams;
  },
): Promise<BrainToolResult<CoordinationToolDetails>> {
  const fromAgentId = input.fromAgentId;
  if (fromAgentId === undefined) {
    return coordinationResult({
      ok: false,
      operation: "send_agent_message",
      reasonCode: "tool_context_required",
      queuedActions: 0,
      text: "send_agent_message requires wake context.",
    });
  }
  if (
    input.fromSessionId === undefined ||
    input.wakeId === undefined ||
    input.toolCallId === undefined
  ) {
    return coordinationResult({
      ok: false,
      operation: "send_agent_message",
      reasonCode: "tool_context_required",
      queuedActions: 0,
      text: "send_agent_message requires durable wake and tool-call context.",
    });
  }

  if (context.runtime !== undefined) {
    const routed = await context.runtime.routeMessage({
      fromAgentId,
      fromSessionId: input.fromSessionId,
      wakeId: input.wakeId,
      toolCallId: input.toolCallId,
      toAgentId: input.params.toAgentId,
      body: input.params.body,
      ...(input.params.correlationId === undefined
        ? {}
        : { correlationId: input.params.correlationId }),
      requireWake: input.params.requireWake ?? true,
      ...(input.params.ttlSeconds === undefined
        ? {}
        : { ttlSeconds: input.params.ttlSeconds }),
    });
    return coordinationResult({
      ok: routed.accepted,
      operation: "send_agent_message",
      routed,
      queuedActions: 0,
      text: routed.wake
        ? `message routed to ${input.params.toAgentId}; wake ${routed.wake.status}`
        : `message routed to ${input.params.toAgentId}`,
    });
  }

  context.actions?.add({
    type: "send_message",
    message: {
      from: fromAgentId as AgentId,
      to: input.params.toAgentId as AgentId,
      body: input.params.body,
      correlationId: input.params.correlationId,
    },
  });
  return coordinationResult({
    ok: context.actions !== undefined,
    operation: "send_agent_message",
    reasonCode:
      context.actions === undefined
        ? "coordination_runtime_unavailable"
        : undefined,
    queuedActions: context.actions === undefined ? 0 : 1,
    text:
      context.actions === undefined
        ? "message could not be routed because no coordination runtime or action collector is available"
        : "message action queued for post-turn routing",
  });
}

async function replyAgentMessage(
  context: CoordinationToolContext,
  input: {
    fromAgentId: string;
    fromSessionId: string;
    wakeId: string;
    toolCallId: string;
    params: ReplyAgentMessageParams;
  },
): Promise<BrainToolResult<CoordinationToolDetails>> {
  if (context.runtime === undefined) {
    return coordinationResult({
      ok: false,
      operation: "reply_agent_message",
      reasonCode: "coordination_runtime_unavailable",
      queuedActions: 0,
      text: "reply_agent_message requires the Rusty Crew service coordination runtime.",
    });
  }
  const routed = await context.runtime.replyMessage({
    fromAgentId: input.fromAgentId,
    fromSessionId: input.fromSessionId,
    wakeId: input.wakeId,
    toolCallId: input.toolCallId,
    messageId: input.params.messageId,
    body: input.params.body,
    ...(input.params.ttlSeconds === undefined
      ? {}
      : { ttlSeconds: input.params.ttlSeconds }),
  });
  return coordinationResult({
    ok: routed.accepted,
    operation: "reply_agent_message",
    routed,
    queuedActions: 0,
    text: routed.accepted
      ? `reply accepted for message ${input.params.messageId}`
      : `reply rejected for message ${input.params.messageId}`,
  });
}

function coordinationResult(input: {
  ok: boolean;
  operation: CoordinationToolDetails["operation"];
  text: string;
  reasonCode?: string;
  agents?: AgentDirectoryEntry[];
  routed?: AgentMessageRouteResult;
  round?: AgentRoundResult;
  queuedActions: number;
}): BrainToolResult<CoordinationToolDetails> {
  return {
    content: [{ type: "text", text: input.text }],
    details: {
      ok: input.ok,
      operation: input.operation,
      reasonCode: input.reasonCode,
      agents: input.agents,
      routed: input.routed,
      round: input.round,
      queuedActions: input.queuedActions,
    },
  };
}

function formatAgentDirectory(agents: readonly AgentDirectoryEntry[]): string {
  if (agents.length === 0) {
    return "No non-archived agents are registered on this Rusty Crew service.";
  }
  return [
    "Agents on this Rusty Crew service:",
    ...agents.map((agent) => {
      const status = agent.routable
        ? "routable"
        : `unavailable (${agent.routabilityReasonCode ?? "unknown_reason"})`;
      const task = agent.taskRef?.projectId
        ? `; project=${agent.taskRef.projectId}`
        : "";
      return `- ${agent.displayLabel}: recipient=${agent.agentId}; profile=${agent.profileId}; runtime=${agent.runtimeKind}; status=${status}${task}`;
    }),
  ].join("\n");
}
