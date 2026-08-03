import assert from "node:assert/strict";
import test from "node:test";

import type {
  AgentCorrelatedRound,
  AgentDirectoryEntry,
  AgentId,
  AgentMessageCommand,
  AgentMessageDeliveryReceipt,
  AgentMessageReplyCommand,
  AgentRoundCommand,
  AgentRoundStartReceipt,
  AgentRouteKey,
  ProfileId,
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

test("Codex review submission derives external caller and returns without polling", async () => {
  const port = new RecordingPort();
  const calls: unknown[] = [];
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-review",
      turnId: "turn-review",
      callId: "call-review",
      namespace: "rusty_crew",
      tool: "submit_task_for_review",
      arguments: {
        taskId: 6574,
        repository: "earendil-works/rusty-crew",
        commitSha: "a".repeat(40),
        ref: "main",
        requiredChecks: ["Verify Offline"],
        baseCommit: "b".repeat(40),
        reviewSummaryMd: "Implemented and verified.",
      },
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "binding-review",
      controllerInstanceId: "controller-review",
      controllerGeneration: 11,
    },
    port,
    onReviewSubmission: async (input) => {
      calls.push(input);
      return {
        ok: true,
        submissionId: "review-submission:test",
        phase: "gate_pending",
        taskId: input.taskId,
        commitSha: input.commitSha,
        summary: "accepted without polling",
      };
    },
  });
  assert.equal(result?.success, true);
  const content = result?.contentItems[0];
  assert.equal(content?.type, "inputText");
  assert.equal(
    content?.type === "inputText" ? content.text : undefined,
    "accepted without polling",
  );
  assert.deepEqual((calls[0] as { caller: unknown }).caller, {
    type: "external_agent",
    runtimeId: "codex-local",
    bindingId: "binding-review",
    controllerInstanceId: "controller-review",
    controllerGeneration: 11,
    nativeThreadId: "thread-review",
    nativeTurnId: "turn-review",
    nativeRequestId: "call-review",
  });
});

test("Codex coordination lists routes separately from raw same-service diagnostics", async () => {
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
  assert.match(
    content?.type === "inputText" ? content.text : "",
    /recipient=worker/,
  );
  assert.match(content?.type === "inputText" ? content.text : "", /@reviewer/);
  assert.doesNotMatch(
    content?.type === "inputText" ? content.text : "",
    /recipient=reviewer(?:;|\n)/,
  );
  assert.doesNotMatch(
    content?.type === "inputText" ? content.text : "",
    /recipient=planner(?:;|\n)/,
  );
  assert.equal(port.directoryReads, 1);
});

test("Codex coordination rejects a bare raw address reserved by a curated route", async () => {
  const port = new RecordingPort();
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-ambiguous",
      turnId: "turn-ambiguous",
      callId: "call-ambiguous",
      namespace: "rusty_crew",
      tool: "send_agent_message",
      arguments: { recipient: "reviewer", body: "review this" },
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "binding-ambiguous",
      controllerInstanceId: "controller-ambiguous",
      controllerGeneration: 7,
    },
    port,
  });
  assert.equal(result?.success, false);
  const content = result?.contentItems[0];
  assert.match(
    content?.type === "inputText" ? content.text : "",
    /use @reviewer/,
  );
  assert.equal(port.deliveries.length, 0);
});

test("Codex coordination reports the exact curated delivery target", async () => {
  const port = new RecordingPort();
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-route",
      turnId: "turn-route",
      callId: "call-route",
      namespace: "rusty_crew",
      tool: "send_agent_message",
      arguments: { recipient: "@reviewer", body: "review this" },
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "binding-route",
      controllerInstanceId: "controller-route",
      controllerGeneration: 7,
    },
    port,
  });
  assert.equal(result?.success, true);
  const content = result?.contentItems[0];
  const text = content?.type === "inputText" ? content.text : "";
  assert.match(text, /address=@reviewer/);
  assert.match(text, /addressKind=curated_route:@reviewer/);
  assert.match(text, /agent=planner/);
  assert.match(text, /session=planner-session/);
  assert.match(text, /runtime=direct_brain/);
  assert.match(text, /activation=external_turn_requested/);
});

test("Codex coordination reports queued raw Codex and raw direct runtimes from directory identity", async () => {
  const port = new RecordingPort();
  const binding = {
    runtimeId: "codex-local",
    bindingId: "binding-raw-targets",
    controllerInstanceId: "controller-raw-targets",
    controllerGeneration: 7,
  };
  const codexResult = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-raw-codex",
      turnId: "turn-raw-codex",
      callId: "call-raw-codex",
      namespace: "rusty_crew",
      tool: "send_agent_message",
      arguments: { recipient: "unrouted-codex", body: "continue later" },
    },
    binding,
    port,
  });
  const codexContent = codexResult?.contentItems[0];
  const codexText = codexContent?.type === "inputText" ? codexContent.text : "";
  assert.match(codexText, /addressKind=raw_agent/);
  assert.match(codexText, /session=unrouted-codex-session/);
  assert.match(codexText, /runtime=codex_app_server/);
  assert.match(codexText, /activation=queued_for_next_turn/);

  const directResult = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "thread-raw-direct",
      turnId: "turn-raw-direct",
      callId: "call-raw-direct",
      namespace: "rusty_crew",
      tool: "send_agent_message",
      arguments: { recipient: "worker", body: "work now" },
    },
    binding,
    port,
  });
  const directContent = directResult?.contentItems[0];
  const directText =
    directContent?.type === "inputText" ? directContent.text : "";
  assert.match(directText, /session=worker-session/);
  assert.match(directText, /runtime=direct_brain/);
  assert.match(directText, /activation=direct_brain_wake_requested/);
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
    onDelivery: async (receipt) => {
      delivered.push(receipt);
      return receipt;
    },
    now: () => new Date("2026-07-10T00:00:00Z"),
  });
  assert.equal(result?.success, true);
  assert.equal(result?.contentItems[0]?.type, "inputText");
  assert.equal(result?.contentItems[0]?.text, "durable reply");
  assert.equal(delivered.length, 1);
  assert.equal(delivered[0]?.activation?.type, "external_turn_requested");
});

test("Codex coordination replies by durable message id without model routing metadata", async () => {
  const port = new RecordingPort();
  const result = await resolveCodexCoordinationToolCall({
    params: {
      threadId: "review-thread",
      turnId: "review-turn",
      callId: "reply-call",
      namespace: "rusty_crew",
      tool: "reply_agent_message",
      arguments: {
        messageId: "review-request-17",
        body: "review passed",
        ttlSeconds: 3_600,
      },
    },
    binding: {
      runtimeId: "codex-local",
      bindingId: "review-binding",
      controllerInstanceId: "review-controller",
      controllerGeneration: 9,
    },
    port,
    now: () => new Date("2026-07-10T00:00:00Z"),
  });
  assert.equal(result?.success, true);
  assert.equal(port.replies.length, 1);
  assert.equal(port.replies[0]?.inReplyToMessageId, "review-request-17");
  assert.equal(port.replies[0]?.expiresAt, "2026-07-10T01:00:00.000Z");
  assert.equal(
    "toAgentId" in (port.replies[0] ?? {}),
    false,
    "the reply tool must not accept model-supplied routing",
  );
});

class RecordingPort implements CodexCoordinationPort {
  directoryReads = 0;
  readonly deliveries: AgentMessageCommand[] = [];
  readonly replies: AgentMessageReplyCommand[] = [];
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
      {
        agentId: "reviewer",
        sessionId: "reviewer-raw-session" as SessionId,
        profileId: "reviewer-profile",
        displayLabel: "Reviewer raw twin",
        sessionKind: "full",
        sessionStatus: "idle",
        runtimeKind: "direct_brain",
        routable: true,
      },
      {
        agentId: "worker",
        sessionId: "worker-session" as SessionId,
        profileId: "worker-profile",
        displayLabel: "Worker",
        sessionKind: "full",
        sessionStatus: "idle",
        runtimeKind: "direct_brain",
        routable: true,
      },
      {
        agentId: "unrouted-codex",
        sessionId: "unrouted-codex-session" as SessionId,
        profileId: "unrouted-codex-profile",
        displayLabel: "Unrouted Codex",
        sessionKind: "full",
        sessionStatus: "active",
        runtimeKind: "codex_app_server",
        runtimeId: "codex-local",
        bindingId: "unrouted-codex-binding",
        bindingStatus: "active",
        routable: true,
      },
    ];
  }

  async listAgentRouteResolutions() {
    return [
      {
        address: "@reviewer",
        routable: true,
        route: {
          routeKey: "reviewer" as AgentRouteKey,
          label: "Reviewer",
          enabled: true,
          target: {
            type: "direct_brain" as const,
            agentId: "planner" as AgentId,
            sessionId: "planner-session" as SessionId,
          },
          revision: 1,
          createdAt: "2026-07-10T00:00:00Z",
          updatedAt: "2026-07-10T00:00:00Z",
        },
        resolvedTarget: {
          agentId: "planner" as AgentId,
          sessionId: "planner-session" as SessionId,
          profileId: "planner-profile" as ProfileId,
          displayLabel: "Planner",
          runtimeKind: "direct_brain" as const,
        },
      },
    ];
  }

  async deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt> {
    this.deliveries.push(command);
    return deliveryReceipt(command);
  }

  async replyAgentMessage(
    command: AgentMessageReplyCommand,
  ): Promise<AgentMessageDeliveryReceipt> {
    this.replies.push(command);
    return {
      request: {
        deliveryId: command.deliveryId,
        idempotencyKey: command.idempotencyKey,
        messageId: command.messageId,
        fromAgentId: "codex-agent",
        fromSessionId: "codex-session" as SessionId,
        requestedAddress: "requester",
        toAgentId: "requester",
        toSessionId: "requester-session" as SessionId,
        replyToMessageId: command.inReplyToMessageId,
        inputKind: "routed_agent_message",
        body: command.body,
        correlationId: command.inReplyToMessageId,
        requireWake: true,
        createdAt: command.createdAt,
        expiresAt: command.expiresAt,
      },
      status: "accepted",
      sequence: 9,
      terminalAt: command.createdAt,
      revision: 2,
    };
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
      recipientAgentId: command.toAddress,
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
        toAddress: command.toAddress,
        inputKind: "routed_agent_message",
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
  const routed = command.toAddress === "@reviewer";
  const targetSessionId = routed
    ? "planner-session"
    : command.toAddress === "unrouted-codex"
      ? "unrouted-codex-session"
      : command.toAddress === "worker"
        ? "worker-session"
        : "planner-session";
  const activation =
    command.toAddress === "unrouted-codex"
      ? {
          type: "queued_for_next_turn" as const,
          sessionId: targetSessionId as SessionId,
          queueId: "unrouted-codex-queue",
        }
      : command.toAddress === "worker"
        ? {
            type: "direct_brain_wake_requested" as const,
            sessionId: targetSessionId as SessionId,
            wakeId: "worker-wake",
          }
        : {
            type: "external_turn_requested" as const,
            sessionId: targetSessionId as SessionId,
            requestId: "external-request-1",
            bindingId: "recipient-binding",
          };
  return {
    request: {
      deliveryId: command.deliveryId,
      idempotencyKey: command.idempotencyKey,
      messageId: command.messageId,
      fromAgentId: "codex-agent",
      fromSessionId: "codex-session" as SessionId,
      requestedAddress: command.toAddress,
      toAgentId: routed ? "planner" : command.toAddress,
      toSessionId: targetSessionId as SessionId,
      routing: routed
        ? {
            address: "@reviewer",
            routeKey: "reviewer" as AgentRouteKey,
            routeRevision: 1,
            resolvedTarget: {
              agentId: "planner" as AgentId,
              sessionId: "planner-session" as SessionId,
              profileId: "planner-profile" as ProfileId,
              displayLabel: "Planner",
              runtimeKind: "direct_brain",
            },
          }
        : undefined,
      replyToMessageId: null,
      inputKind: command.inputKind,
      body: command.body,
      requireWake: command.requireWake,
      createdAt: command.createdAt,
      expiresAt: command.expiresAt,
    },
    status: "accepted",
    sequence: 7,
    activation,
    terminalAt: command.createdAt,
    revision: 2,
  };
}
