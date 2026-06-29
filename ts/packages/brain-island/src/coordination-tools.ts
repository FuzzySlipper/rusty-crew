import type { AgentId, CoreEvent } from "@rusty-crew/contracts";
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
});

const agentRoundParameters = Type.Object({
  toAgentId: Type.String({ minLength: 1 }),
  body: Type.String({ minLength: 1 }),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
});

type SendAgentMessageParams = Static<typeof sendAgentMessageParameters>;
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
  routeMessage(input: {
    fromAgentId: string;
    toAgentId: string;
    body: string;
    correlationId?: string;
    requireWake?: boolean;
  }): Promise<AgentMessageRouteResult>;
  roundTrip(input: {
    fromAgentId: string;
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
  operation: "send_agent_message" | "agent_round";
  reasonCode?: string;
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
  return [sendAgentMessageTool(context), agentRoundTool(context)];
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
        params,
      }),
    execute: async (_callId, params) =>
      sendAgentMessage(context, {
        fromAgentId: undefined,
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

  if (context.runtime !== undefined) {
    const routed = await context.runtime.routeMessage({
      fromAgentId,
      toAgentId: input.params.toAgentId,
      body: input.params.body,
      correlationId: input.params.correlationId,
      requireWake: input.params.requireWake ?? true,
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

function coordinationResult(input: {
  ok: boolean;
  operation: CoordinationToolDetails["operation"];
  text: string;
  reasonCode?: string;
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
      routed: input.routed,
      round: input.round,
      queuedActions: input.queuedActions,
    },
  };
}

export function isCorrelatedReply(
  event: CoreEvent,
  input: {
    fromAgentId: string;
    toAgentId: string;
    correlationId: string;
  },
): boolean {
  return (
    event.type === "agent_message_routed" &&
    event.message.from === (input.toAgentId as AgentId) &&
    event.message.to === (input.fromAgentId as AgentId) &&
    event.message.correlationId === input.correlationId
  );
}

export function replyFromEvent(
  event: CoreEvent,
): AgentRoundResult["reply"] | undefined {
  if (event.type !== "agent_message_routed") return undefined;
  return {
    from: event.message.from,
    to: event.message.to,
    body: event.message.body,
    correlationId: event.message.correlationId,
  };
}
