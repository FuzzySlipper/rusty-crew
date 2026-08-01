import assert from "node:assert/strict";
import test from "node:test";

import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { Type } from "typebox";

import type { BrainTool, BrainToolTurnDisposition } from "../src/brain-tool.js";
import {
  BufferedBrainWakeError,
  runBufferedBrainHost,
} from "../src/buffered-brain-host.js";
import { resolveCompletionTools } from "../src/completion-tools.js";

const sessionId = "turn-disposition-session" as SessionId;
const wakeId = "turn-disposition-wake";

test("successful completion tool settles the buffered wake without a failure", async () => {
  const harness = dispositionBridge();
  let planCalls = 0;
  const result = await runBufferedBrainHost({
    bridge: harness.bridge,
    moduleLabel: "Chat Completions",
    run: chatCompletionsRun(),
    wake: wake(),
    toolProfile: {
      tools: [
        {
          name: "deliver_completion_md",
          description: "Deliver completion",
        },
      ],
    },
    toolResolver: resolveCompletionTools,
    planActions: async () => {
      planCalls += 1;
      return [];
    },
  });

  assert.equal(result.outcome, "completed");
  assert.deepEqual(
    result.actions.map((action) => action.type),
    ["deliver_completion"],
  );
  assert.deepEqual(harness.cancelReasonCodes, []);
  assert.equal(harness.submittedResults.length, 1);
  assert.equal(harness.submittedResults[0]?.status, "succeeded");
  assert.equal(harness.submittedResults[0]?.turnDisposition, "complete_turn");
  assert.equal(planCalls, 0);
});

test("external suspension retains its distinct native disposition", async () => {
  const harness = dispositionBridge();
  let planCalls = 0;
  const result = await runBufferedBrainHost({
    bridge: harness.bridge,
    moduleLabel: "Chat Completions",
    run: chatCompletionsRun(),
    wake: wake(),
    toolProfile: {
      tools: [{ name: "deliver_completion_md", description: "Wait for gate" }],
    },
    toolResolver: () => [dispositionTool("suspend_external")],
    planActions: async () => {
      planCalls += 1;
      return [];
    },
  });

  assert.equal(result.outcome, "completed");
  assert.deepEqual(harness.cancelReasonCodes, []);
  assert.equal(
    harness.submittedResults[0]?.turnDisposition,
    "suspend_external",
  );
  assert.equal(planCalls, 0);
});

test("ordinary tool output continues without cancelling the native run", async () => {
  let drains = 0;
  let cancellations = 0;
  const bridge = {
    startBrainRun: async () => ({
      moduleId: "chat-completions" as const,
      wakeId,
    }),
    drainBrainRun: async () => {
      drains += 1;
      return drains === 1
        ? pendingToolDrain()
        : { items: [], toolRequests: [], terminal: true };
    },
    submitBrainHostResult: async () => ({}),
    cancelBrainRun: async () => {
      cancellations += 1;
      return {};
    },
  } as unknown as NativeBridgeModule;

  const result = await runBufferedBrainHost({
    bridge,
    moduleLabel: "Chat Completions",
    run: chatCompletionsRun(),
    wake: wake(),
    toolProfile: {
      tools: [{ name: "deliver_completion_md", description: "Ordinary" }],
    },
    toolResolver: () => [ordinaryTool()],
  });

  assert.equal(result.outcome, "completed");
  assert.equal(cancellations, 0);
});

test("live event submission retains text deltas for completion planning", async () => {
  const submittedEvents: string[] = [];
  let plannedText = "";
  const bridge = {
    startBrainRun: async () => ({
      moduleId: "chat-completions" as const,
      wakeId,
    }),
    drainBrainRun: async () => ({
      items: [
        {
          type: "event" as const,
          event: {
            wakeId,
            sessionId,
            event: { type: "text_delta" as const, text: "scout evidence" },
          },
        },
      ],
      toolRequests: [],
      terminal: true,
    }),
  } as unknown as NativeBridgeModule;

  await runBufferedBrainHost({
    bridge,
    moduleLabel: "Chat Completions",
    run: chatCompletionsRun(),
    wake: wake(),
    toolProfile: { tools: [] },
    submitEvent: async (event) => {
      submittedEvents.push(event.event.type);
    },
    planActions: ({ events }) => {
      plannedText = events
        .flatMap((event) =>
          event.event.type === "text_delta" ? [event.event.text] : [],
        )
        .join("");
      return [];
    },
  });

  assert.deepEqual(submittedEvents, ["text_delta"]);
  assert.equal(plannedText, "scout evidence");
});

test("a genuine native terminal error remains a buffered wake failure", async () => {
  const harness = dispositionBridge({
    terminalReasonCode: "provider_response_failed",
    error: "provider request failed",
  });

  await assert.rejects(
    runBufferedBrainHost({
      bridge: harness.bridge,
      moduleLabel: "Chat Completions",
      run: chatCompletionsRun(),
      wake: wake(),
      toolProfile: {
        tools: [{ name: "complete", description: "Complete" }],
      },
      toolResolver: () => [dispositionTool("complete_turn")],
    }),
    (error: unknown) => {
      assert.ok(error instanceof BufferedBrainWakeError);
      assert.equal(error.reasonCode, "provider_response_failed");
      return true;
    },
  );
});

function dispositionBridge(terminal?: {
  terminalReasonCode: string;
  error: string;
}): {
  bridge: NativeBridgeModule;
  cancelReasonCodes: string[];
  submittedResults: Array<{
    status?: string;
    turnDisposition?: BrainToolTurnDisposition;
  }>;
} {
  let drains = 0;
  const cancelReasonCodes: string[] = [];
  const submittedResults: Array<{
    status?: string;
    turnDisposition?: BrainToolTurnDisposition;
  }> = [];
  const bridge = {
    startBrainRun: async () => ({
      moduleId: "chat-completions" as const,
      wakeId,
    }),
    drainBrainRun: async () => {
      drains += 1;
      return drains === 1
        ? pendingToolDrain()
        : terminal === undefined
          ? { items: [], toolRequests: [], terminal: true }
          : {
              items: [],
              toolRequests: [],
              terminal: true,
              terminalReasonCode: terminal.terminalReasonCode,
              error: terminal.error,
            };
    },
    submitBrainHostResult: async (input: {
      status?: string;
      turnDisposition?: BrainToolTurnDisposition;
    }) => {
      submittedResults.push(input);
      return {};
    },
    cancelBrainRun: async (input: { reasonCode: string }) => {
      cancelReasonCodes.push(input.reasonCode);
      return {};
    },
  } as unknown as NativeBridgeModule;
  return { bridge, cancelReasonCodes, submittedResults };
}

function pendingToolDrain() {
  return {
    items: [],
    toolRequests: [
      {
        wakeId,
        callId: "call-1",
        name: "deliver_completion_md",
        argumentsJson: JSON.stringify({
          markdown: "---\nstatus: completed\n---\n\n## Summary\n\nDone.",
        }),
      },
    ],
    terminal: false,
  };
}

function dispositionTool(disposition: BrainToolTurnDisposition): BrainTool {
  return {
    name: "deliver_completion_md",
    label: "Disposition tool",
    description: "Request a turn disposition",
    parameters: Type.Object({ markdown: Type.Optional(Type.String()) }),
    execute: async () => ({
      content: [{ type: "text", text: "done" }],
      details: {},
      turnDisposition: disposition,
    }),
  };
}

function ordinaryTool(): BrainTool {
  return {
    name: "deliver_completion_md",
    label: "Ordinary tool",
    description: "Return normally",
    parameters: Type.Object({ markdown: Type.Optional(Type.String()) }),
    execute: async () => ({
      content: [{ type: "text", text: "continue" }],
      details: {},
    }),
  };
}

function chatCompletionsRun() {
  return {
    moduleId: "chat-completions" as const,
    providerInput: {
      wakeId,
      sessionId,
      bodyState: wake().state,
      messages: [],
      config: { model: "test-model" },
      client: { mode: "fake" as const },
    },
  };
}

function wake() {
  return {
    wakeId,
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "instructions" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "turn-disposition-agent" as AgentId,
        profileId: "turn-disposition-profile" as ProfileId,
        kind: "full" as const,
        resourceLimits: {},
        toolProfile: { tools: [] },
        status: "idle" as const,
        brainTurnCount: 0,
        createdAt: "2026-07-31T00:00:00Z",
        lastActiveAt: "2026-07-31T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake" as const,
        queueOwner: "body" as const,
        queuedMessageTtlMs: 5_000,
        maxQueuedMessages: 32,
      },
    },
  };
}
