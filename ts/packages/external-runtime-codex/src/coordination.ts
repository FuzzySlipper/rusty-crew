import type {
  AgentCorrelatedRound,
  AgentMessageCommand,
  AgentMessageDeliveryReceipt,
  AgentRoundCommand,
  AgentRoundStartReceipt,
} from "@rusty-crew/contracts";
import type { DynamicToolCallParams } from "../protocol/0.144.1/ts/v2/DynamicToolCallParams";
import type { DynamicToolCallResponse } from "../protocol/0.144.1/ts/v2/DynamicToolCallResponse";
import type { DynamicToolSpec } from "../protocol/0.144.1/ts/v2/DynamicToolSpec";

const COORDINATION_NAMESPACE = "rusty_crew";
const MAX_ROUND_TIMEOUT_MS = 300_000;
const DEFAULT_ROUND_TIMEOUT_MS = 30_000;
const MESSAGE_TTL_MS = 5_000;

export const CODEX_COORDINATION_DYNAMIC_TOOLS: readonly DynamicToolSpec[] = [
  {
    type: "namespace",
    name: COORDINATION_NAMESPACE,
    description: "Rusty Crew internal agent coordination.",
    tools: [
      {
        type: "function",
        name: "send_agent_message",
        description: "Send a message to another Rusty Crew agent.",
        inputSchema: {
          type: "object",
          properties: {
            recipient: { type: "string", minLength: 1 },
            body: { type: "string", minLength: 1 },
            correlationId: { type: "string", minLength: 1 },
          },
          required: ["recipient", "body"],
          additionalProperties: false,
        },
      },
      {
        type: "function",
        name: "agent_round",
        description:
          "Send a message to another Rusty Crew agent and wait for one correlated reply.",
        inputSchema: {
          type: "object",
          properties: {
            recipient: { type: "string", minLength: 1 },
            body: { type: "string", minLength: 1 },
            correlationId: { type: "string", minLength: 1 },
            timeoutMs: {
              type: "integer",
              minimum: 1,
              maximum: MAX_ROUND_TIMEOUT_MS,
            },
          },
          required: ["recipient", "body"],
          additionalProperties: false,
        },
      },
    ],
  },
];

export interface CodexCoordinationBinding {
  readonly runtimeId: string;
  readonly bindingId: string;
  readonly controllerInstanceId: string;
  readonly controllerGeneration: number;
}

export interface CodexCoordinationPort {
  deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  beginAgentRound(command: AgentRoundCommand): Promise<AgentRoundStartReceipt>;
  getAgentRound(roundId: string): Promise<AgentCorrelatedRound | undefined>;
}

export async function resolveCodexCoordinationToolCall(input: {
  readonly params: DynamicToolCallParams;
  readonly binding: CodexCoordinationBinding;
  readonly port: CodexCoordinationPort;
  readonly now?: () => Date;
}): Promise<DynamicToolCallResponse | undefined> {
  const { params } = input;
  if (params.namespace !== COORDINATION_NAMESPACE) return undefined;
  if (params.tool !== "send_agent_message" && params.tool !== "agent_round") {
    return failed(`unsupported Rusty Crew coordination tool ${params.tool}`);
  }
  const args = parseArguments(params.arguments);
  if (typeof args === "string") return failed(args);
  const now = input.now?.() ?? new Date();
  const identity = `${input.binding.bindingId}:${params.threadId}:${params.turnId}:${params.callId}`;
  const caller = {
    type: "external_agent" as const,
    runtimeId: input.binding.runtimeId,
    bindingId: input.binding.bindingId,
    controllerInstanceId: input.binding.controllerInstanceId,
    controllerGeneration: input.binding.controllerGeneration,
    nativeThreadId: params.threadId,
    nativeTurnId: params.turnId,
    nativeRequestId: params.callId,
  };
  if (params.tool === "send_agent_message") {
    const command: AgentMessageCommand = {
      caller,
      deliveryId: `codex-delivery:${identity}`,
      idempotencyKey: `codex-delivery:${identity}`,
      messageId: `codex-message:${identity}`,
      toAgentId: args.recipient,
      body: args.body,
      ...(args.correlationId === undefined
        ? {}
        : { correlationId: args.correlationId }),
      requireWake: true,
      createdAt: now.toISOString(),
      expiresAt: new Date(now.getTime() + MESSAGE_TTL_MS).toISOString(),
    };
    const receipt = await input.port.deliverAgentMessage(command);
    return receipt.status === "accepted"
      ? succeeded(
          receipt.activation?.type === "queued_for_next_turn"
            ? `message queued for ${args.recipient}'s next turn`
            : `message accepted for ${args.recipient}`,
        )
      : failed(receipt.reasonCode ?? `message ${receipt.status}`);
  }

  const timeoutMs = Math.min(
    Math.max(args.timeoutMs ?? DEFAULT_ROUND_TIMEOUT_MS, 1),
    MAX_ROUND_TIMEOUT_MS,
  );
  const started = await input.port.beginAgentRound({
    caller,
    roundId: `codex-round:${identity}`,
    idempotencyKey: `codex-round:${identity}`,
    messageId: `codex-round-message:${identity}`,
    toAgentId: args.recipient,
    body: args.body,
    correlationId: args.correlationId ?? `codex-round:${identity}`,
    createdAt: now.toISOString(),
    expiresAt: new Date(now.getTime() + timeoutMs).toISOString(),
  });
  while (true) {
    const round = await input.port.getAgentRound(started.round.roundId);
    if (round === undefined) return failed("agent round disappeared");
    if (round.status === "replied") {
      const outcome = round.outcome as { body?: unknown } | undefined;
      return succeeded(
        typeof outcome?.body === "string" ? outcome.body : "reply received",
      );
    }
    if (round.status !== "pending") {
      return failed(round.terminalReasonCode ?? `agent round ${round.status}`);
    }
    await new Promise<void>((resolve) => setTimeout(resolve, 25));
  }
}

interface CoordinationArguments {
  readonly recipient: string;
  readonly body: string;
  readonly correlationId?: string;
  readonly timeoutMs?: number;
}

function parseArguments(value: unknown): CoordinationArguments | string {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return "coordination tool arguments must be an object";
  }
  const record = value as Record<string, unknown>;
  if (typeof record.recipient !== "string" || record.recipient.trim() === "") {
    return "recipient must be a non-empty string";
  }
  if (typeof record.body !== "string" || record.body.trim() === "") {
    return "body must be a non-empty string";
  }
  if (
    record.correlationId !== undefined &&
    (typeof record.correlationId !== "string" ||
      record.correlationId.trim() === "")
  ) {
    return "correlationId must be a non-empty string when supplied";
  }
  if (
    record.timeoutMs !== undefined &&
    (typeof record.timeoutMs !== "number" ||
      !Number.isInteger(record.timeoutMs) ||
      record.timeoutMs <= 0)
  ) {
    return "timeoutMs must be a positive integer when supplied";
  }
  return {
    recipient: record.recipient,
    body: record.body,
    ...(record.correlationId === undefined
      ? {}
      : { correlationId: record.correlationId as string }),
    ...(record.timeoutMs === undefined
      ? {}
      : { timeoutMs: record.timeoutMs as number }),
  };
}

function succeeded(text: string): DynamicToolCallResponse {
  return { success: true, contentItems: [{ type: "inputText", text }] };
}

function failed(text: string): DynamicToolCallResponse {
  return { success: false, contentItems: [{ type: "inputText", text }] };
}
