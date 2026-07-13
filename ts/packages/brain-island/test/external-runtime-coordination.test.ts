import assert from "node:assert/strict";
import test from "node:test";

import type {
  AgentCorrelatedRound,
  AgentDirectoryEntry,
  AgentMessageCommand,
  AgentMessageDeliveryReceipt,
  AgentRoundCommand,
  AgentRoundStartReceipt,
  SessionId,
} from "@rusty-crew/contracts";

import {
  resolveCodexCoordinationToolCall,
  type CodexCoordinationPort,
} from "../src/external-runtime-coordination.js";

test("Codex coordination derives trusted identity outside model arguments", async () => {
  const port = new RecordingPort();
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-1",
      turnId: "turn-1",
      callId: "call-1",
      namespace: "rusty_crew",
      tool: "send_agent_message",
      arguments: { recipient: "planner", body: "inspect this" },
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "binding-1",
      controllerInstanceId: "controller-1",
      controllerGeneration: 7,
    },
    port,
    now: () => new Date("2026-07-10T00:00:00Z"),
  });
  assert.equal(result?.success, true);
  assert.deepEqual(port.deliveries[0]?.caller, {
    type: "external_agent",
    runtimeId: "codex-local",
    bindingId: "binding-1",
    controllerInstanceId: "controller-1",
    controllerGeneration: 7,
    nativeThreadId: "thread-1",
    nativeTurnId: "turn-1",
    nativeRequestId: "call-1",
  });
});

test("Codex coordination lists only the Rust-projected same-service directory", async () => {
  const port = new RecordingPort();
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-1",
      turnId: "turn-1",
      callId: "call-directory",
      namespace: "rusty_crew",
      tool: "list_agents",
      arguments: {},
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "binding-1",
      controllerInstanceId: "controller-1",
      controllerGeneration: 7,
    },
    port,
  });
  assert.equal(result?.success, true);
  const content = result?.contentItems[0];
  assert.equal(content?.type, "inputText");
  assert.match(content?.type === "inputText" ? content.text : "", /recipient=planner/);
  assert.equal(port.directoryReads, 1);
});

test("Codex coordination round returns the durable Rust reply", async () => {
  const port = new RecordingPort();
  const delivered: AgentMessageDeliveryReceipt[] = [];
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-2",
      turnId: "turn-2",
      callId: "call-2",
      namespace: "rusty_crew",
      tool: "agent_round",
      arguments: { recipient: "coder", body: "check status", timeoutMs: 1_000 },
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "binding-2",
      controllerInstanceId: "controller-2",
      controllerGeneration: 8,
    },
    port,
    onDelivery: (receipt) => {
      delivered.push(receipt);
    },
    now: () => new Date("2026-07-10T00:00:00Z"),
  });
  assert.equal(result?.success, true);
  assert.equal(result?.contentItems[0]?.type, "inputText");
  assert.equal(result?.contentItems[0]?.text, "durable reply");
  assert.equal(delivered.length, 1);
  assert.equal(delivered[0]?.activation?.type, "external_turn_requested");
});

class RecordingPort implements CodexCoordinationPort {
  directoryReads = 0;
  readonly deliveries: AgentMessageCommand[] = [];
  readonly rounds: AgentRoundCommand[] = [];
  #round?: AgentCorrelatedRound;

  async listAgentDirectory(): Promise<AgentDirectoryEntry[]> {
    this.directoryReads += 1;
    return [
      {
        agentId: "planner",
        sessionId: "planner-session" as SessionId,
        profileId: "planner-profile",
        displayLabel: "Planner",
        sessionKind: "full",
        sessionStatus: "idle",
        runtimeKind: "direct_brain",
        routable: true,
      },
    ];
  }

  async deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt> {
    this.deliveries.push(command);
    return deliveryReceipt(command);
  }

  async beginAgentRound(
    command: AgentRoundCommand,
  ): Promise<AgentRoundStartReceipt> {
    this.rounds.push(command);
    this.#round = {
      roundId: command.roundId,
      idempotencyKey: command.idempotencyKey,
      senderAgentId: "codex-agent",
      senderSessionId: "codex-session" as SessionId,
      recipientAgentId: command.toAgentId,
      recipientSessionId: "recipient-session" as SessionId,
      senderRequestId: "external-turn-1",
      messageId: command.messageId,
      correlationId: command.correlationId,
      replyMessageId: "reply-1",
      status: "replied",
      outcome: { body: "durable reply" },
      createdAt: command.createdAt,
      expiresAt: command.expiresAt,
      terminalAt: command.createdAt,
      revision: 2,
    };
    return {
      round: this.#round,
      delivery: deliveryReceipt({
        caller: command.caller,
        deliveryId: `delivery:${command.roundId}`,
        idempotencyKey: `delivery:${command.idempotencyKey}`,
        messageId: command.messageId,
        toAgentId: command.toAgentId,
        body: command.body,
        correlationId: command.correlationId,
        requireWake: true,
        createdAt: command.createdAt,
        expiresAt: command.expiresAt,
      }),
    };
  }

  async getAgentRound(): Promise<AgentCorrelatedRound | undefined> {
    return this.#round;
  }
}

function deliveryReceipt(
  command: AgentMessageCommand,
): AgentMessageDeliveryReceipt {
  return {
    request: {
      deliveryId: command.deliveryId,
      idempotencyKey: command.idempotencyKey,
      messageId: command.messageId,
      fromAgentId: "codex-agent",
      toAgentId: command.toAgentId,
      body: command.body,
      requireWake: command.requireWake,
      createdAt: command.createdAt,
      expiresAt: command.expiresAt,
    },
    status: "accepted",
    sequence: 7,
    activation: {
      type: "external_turn_requested",
      sessionId: "recipient-session" as SessionId,
      requestId: "external-request-1",
      bindingId: "recipient-binding",
    },
    terminalAt: command.createdAt,
    revision: 2,
  };
}
