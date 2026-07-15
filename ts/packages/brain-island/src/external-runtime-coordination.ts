import type {
  AgentCorrelatedRound,
  AgentDirectoryEntry,
  AgentMessageCommand,
  AgentMessageDeliveryReceipt,
  AgentMessageReplyCommand,
  AgentRoundCommand,
  AgentRoundStartReceipt,
} from "@rusty-crew/contracts";
import type {
  DynamicToolCallParams,
  DynamicToolCallResponse,
} from "@rusty-crew/external-runtime-codex";

const COORDINATION_NAMESPACE = "rusty_crew";
const MAX_ROUND_TIMEOUT_MS = 300_000;
const DEFAULT_ROUND_TIMEOUT_MS = 30_000;
const DEFAULT_MESSAGE_TTL_MS = 5 * 60_000;
const MAX_MESSAGE_TTL_MS = 24 * 60 * 60_000;

export interface CodexCoordinationBinding {
  readonly runtimeId: string;
  readonly bindingId: string;
  readonly controllerInstanceId: string;
  readonly controllerGeneration: number;
}

export interface CodexCoordinationPort {
  listAgentDirectory(): Promise<AgentDirectoryEntry[]>;
  deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  replyAgentMessage(
    command: AgentMessageReplyCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  beginAgentRound(command: AgentRoundCommand): Promise<AgentRoundStartReceipt>;
  getAgentRound(roundId: string): Promise<AgentCorrelatedRound | undefined>;
}

export async function resolveCodexCoordinationToolCall(input: {
  readonly params: DynamicToolCallParams;
  readonly binding: CodexCoordinationBinding;
  readonly port: CodexCoordinationPort;
  readonly onDelivery?: (
    receipt: AgentMessageDeliveryReceipt,
  ) => Promise<AgentMessageDeliveryReceipt>;
  readonly now?: () => Date;
}): Promise<DynamicToolCallResponse | undefined> {
  const { params } = input;
  if (params.namespace !== COORDINATION_NAMESPACE) return undefined;
  if (
    params.tool !== "list_agents" &&
    params.tool !== "send_agent_message" &&
    params.tool !== "reply_agent_message" &&
    params.tool !== "agent_round"
  ) {
    return failed(`unsupported Rusty Crew coordination tool ${params.tool}`);
  }
  if (params.tool === "list_agents") {
    if (
      params.arguments === null ||
      typeof params.arguments !== "object" ||
      Array.isArray(params.arguments) ||
      Object.keys(params.arguments as Record<string, unknown>).length !== 0
    ) {
      return failed("list_agents does not accept arguments");
    }
    const agents = await input.port.listAgentDirectory();
    return succeeded(formatAgentDirectory(agents));
  }
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
  if (params.tool === "reply_agent_message") {
    const replyArgs = parseReplyArguments(params.arguments);
    if (typeof replyArgs === "string") return failed(replyArgs);
    const ttlMs = boundedMessageTtlMs(replyArgs.ttlSeconds);
    const initialReceipt = await input.port.replyAgentMessage({
      caller,
      deliveryId: `codex-reply-delivery:${identity}`,
      idempotencyKey: `codex-reply-delivery:${identity}`,
      messageId: `codex-reply-message:${identity}`,
      inReplyToMessageId: replyArgs.messageId,
      body: replyArgs.body,
      createdAt: now.toISOString(),
      expiresAt: new Date(now.getTime() + ttlMs).toISOString(),
    });
    const receipt = input.onDelivery
      ? await input.onDelivery(initialReceipt)
      : initialReceipt;
    return receipt.status === "accepted"
      ? succeeded(
          `reply accepted: messageId=${receipt.request.messageId}; deliveryId=${receipt.request.deliveryId}`,
        )
      : failed(receipt.reasonCode ?? `reply ${receipt.status}`);
  }
  const args = parseArguments(params.arguments);
  if (typeof args === "string") return failed(args);
  if (params.tool === "send_agent_message") {
    const ttlMs = boundedMessageTtlMs(args.ttlSeconds);
    const command: AgentMessageCommand = {
      caller,
      deliveryId: `codex-delivery:${identity}`,
      idempotencyKey: `codex-delivery:${identity}`,
      messageId: `codex-message:${identity}`,
      toAgentId: args.recipient,
      inputKind: "routed_agent_message",
      body: args.body,
      ...(args.correlationId === undefined
        ? {}
        : { correlationId: args.correlationId }),
      requireWake: true,
      createdAt: now.toISOString(),
      expiresAt: new Date(now.getTime() + ttlMs).toISOString(),
    };
    const initialReceipt = await input.port.deliverAgentMessage(command);
    const receipt = input.onDelivery
      ? await input.onDelivery(initialReceipt)
      : initialReceipt;
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
  if (input.onDelivery) await input.onDelivery(started.delivery);
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
  readonly ttlSeconds?: number;
}

interface ReplyArguments {
  readonly messageId: string;
  readonly body: string;
  readonly ttlSeconds?: number;
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
    record.ttlSeconds !== undefined &&
    (typeof record.ttlSeconds !== "number" ||
      !Number.isInteger(record.ttlSeconds) ||
      record.ttlSeconds <= 0 ||
      record.ttlSeconds * 1_000 > MAX_MESSAGE_TTL_MS)
  ) {
    return "ttlSeconds must be an integer between 1 and 86400 when supplied";
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
    ...(record.ttlSeconds === undefined
      ? {}
      : { ttlSeconds: record.ttlSeconds as number }),
  };
}

function parseReplyArguments(value: unknown): ReplyArguments | string {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return "reply_agent_message arguments must be an object";
  }
  const record = value as Record<string, unknown>;
  if (typeof record.messageId !== "string" || record.messageId.trim() === "") {
    return "messageId must be a non-empty string";
  }
  if (typeof record.body !== "string" || record.body.trim() === "") {
    return "body must be a non-empty string";
  }
  if (
    record.ttlSeconds !== undefined &&
    (typeof record.ttlSeconds !== "number" ||
      !Number.isInteger(record.ttlSeconds) ||
      record.ttlSeconds <= 0 ||
      record.ttlSeconds * 1_000 > MAX_MESSAGE_TTL_MS)
  ) {
    return "ttlSeconds must be an integer between 1 and 86400 when supplied";
  }
  return {
    messageId: record.messageId,
    body: record.body,
    ...(record.ttlSeconds === undefined
      ? {}
      : { ttlSeconds: record.ttlSeconds as number }),
  };
}

function boundedMessageTtlMs(ttlSeconds: number | undefined): number {
  return Math.min(
    Math.max(
      ttlSeconds === undefined ? DEFAULT_MESSAGE_TTL_MS : ttlSeconds * 1_000,
      1_000,
    ),
    MAX_MESSAGE_TTL_MS,
  );
}

function succeeded(text: string): DynamicToolCallResponse {
  return { success: true, contentItems: [{ type: "inputText", text }] };
}

function failed(text: string): DynamicToolCallResponse {
  return { success: false, contentItems: [{ type: "inputText", text }] };
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
