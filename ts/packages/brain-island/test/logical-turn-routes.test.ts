import assert from "node:assert/strict";
import test from "node:test";
import type {
  LogicalTurnDiagnosticPage,
  LogicalTurnOperatorState,
} from "@rusty-crew/contracts";
import {
  handleLogicalTurnRoute,
  isLogicalTurnRoute,
  type LogicalTurnRouteContext,
} from "../src/service-logical-turn-routes.js";

test("logical-turn routes expose session-scoped diagnostics and reject mismatched sessions", async () => {
  const context = routeContext("yielded");
  assert.equal(
    isLogicalTurnRoute("/v1/chat/sessions/session-a/logical-turns"),
    true,
  );
  const listed = await handleLogicalTurnRoute(
    {
      method: "GET",
      url: new URL(
        "http://local/v1/chat/sessions/session-a/logical-turns?include_terminal=true",
      ),
      requestId: "list",
    },
    context,
  );
  assert.equal(listed.status, 200);
  assert.equal(
    (listed.body as { data: LogicalTurnDiagnosticPage }).data.items[0]
      ?.sessionId,
    "session-a",
  );

  const mismatched = await handleLogicalTurnRoute(
    {
      method: "POST",
      url: new URL(
        "http://local/v1/chat/sessions/session-b/logical-turns/turn-a/cancel",
      ),
      body: { expectedRevision: 3 },
      requestId: "mismatch",
    },
    context,
  );
  assert.equal(mismatched.status, 404);
});

test("yielded cancellation emits one cancelling and one cancelled chat event", async () => {
  const emitted: string[] = [];
  const context = routeContext("yielded", emitted);
  const result = await handleLogicalTurnRoute(
    {
      method: "POST",
      url: new URL("http://local/v1/admin/logical-turns/turn-a/cancel"),
      body: { expectedRevision: 3 },
      requestId: "cancel-yielded",
      idempotencyKey: "cancel-yielded",
    },
    context,
  );
  assert.equal(result.status, 200);
  assert.deepEqual(emitted, [
    "logical_turn_cancelling",
    "logical_turn_cancelled",
  ]);
});

test("active cancellation relies on the running wake observer instead of duplicating SSE", async () => {
  const emitted: string[] = [];
  const context = routeContext("running", emitted);
  const result = await handleLogicalTurnRoute(
    {
      method: "POST",
      url: new URL("http://local/v1/admin/logical-turns/turn-a/cancel"),
      body: { expectedRevision: 3 },
      requestId: "cancel-running",
    },
    context,
  );
  assert.equal(result.status, 200);
  assert.deepEqual(emitted, []);
});

function routeContext(
  initialState: "yielded" | "running",
  emitted: string[] = [],
): LogicalTurnRouteContext {
  let state: LogicalTurnOperatorState =
    initialState === "running" ? "running" : "queued_to_continue";
  return {
    async logicalTurnDiagnostics(query) {
      if (query.sessionId !== undefined && query.sessionId !== "session-a") {
        return { items: [], total: 0 };
      }
      return {
        items: [diagnostic(state)],
        total: 1,
      };
    },
    async resolveLogicalTurnAttention(input) {
      state = "queued_to_continue";
      return {
        action: input.action,
        record: record("runnable"),
        checkpoint: checkpoint(),
        replayed: false,
      };
    },
    async cancelLogicalTurn() {
      state = "cancelled";
      return {
        record: record("cancelled"),
        replayed: false,
        alreadyTerminal: false,
      };
    },
    async appendChatLifecycleEvent(event) {
      emitted.push(event.kind);
    },
    now: () => "2026-07-29T00:00:00Z",
  };
}

function diagnostic(operatorState: LogicalTurnOperatorState) {
  const phase =
    operatorState === "running"
      ? "running"
      : operatorState === "cancelled"
        ? "cancelled"
        : "yielded";
  return {
    logicalTurnId: "turn-a",
    sessionId: "session-a",
    sourceWakeId: "wake-a",
    currentContinuationId: "continuation-a",
    continuationCount: 2,
    providerRequestTotal: 7,
    toolRoundTotal: 3,
    phase,
    operatorState,
    progressClassification:
      operatorState === "cancelled" ? "cancelled" : "tool_progress",
    lastProgressAt: "2026-07-29T00:00:00Z",
    lastLivenessAt: "2026-07-29T00:00:00Z",
    reasonCode:
      operatorState === "cancelled"
        ? "operator_cancelled"
        : "continuation_queued",
    summary:
      operatorState === "cancelled"
        ? "logical turn was cancelled"
        : "logical turn is active",
    revision: operatorState === "cancelled" ? 4 : 3,
    admittedAt: "2026-07-29T00:00:00Z",
    updatedAt: "2026-07-29T00:00:00Z",
    ...(operatorState === "cancelled"
      ? { terminalAt: "2026-07-29T00:00:00Z" }
      : {}),
  } as LogicalTurnDiagnosticPage["items"][number];
}

function record(phase: "runnable" | "cancelled") {
  return {
    logicalTurnId: "turn-a",
    sessionId: "session-a",
    sourceWakeId: "wake-a",
    phase,
    binding: {
      profileId: "profile-a",
      profileRevision: 1,
      promptFingerprint: "prompt",
      toolSelectionFingerprint: "tools",
      toolRegistryRevision: "tools",
      brainModuleId: "chat-completions",
      brainStrategyId: "default",
      providerAlias: "provider-a",
      providerRevision: 1,
      providerFingerprint: "provider",
    },
    currentContinuationId: "continuation-a",
    continuationSequence: 1,
    bindingGeneration: 1,
    cancellationGeneration: phase === "cancelled" ? 1 : 0,
    revision: phase === "cancelled" ? 4 : 3,
    admittedAt: "2026-07-29T00:00:00Z",
    updatedAt: "2026-07-29T00:00:00Z",
    ...(phase === "cancelled" ? { terminalAt: "2026-07-29T00:00:00Z" } : {}),
  };
}

function checkpoint() {
  return {
    continuationId: "continuation-a",
    logicalTurnId: "turn-a",
    sequence: 1,
    bindingGeneration: 1,
    frozenInput: {
      bodyStateRef: "body",
      bodyStateFingerprint: "body",
      systemPromptRef: "prompt",
      systemPromptFingerprint: "prompt",
      roleAssemblyRef: "role",
      roleAssemblyFingerprint: "role",
      transcriptCursor: 0,
    },
    moduleState: {
      moduleId: "chat-completions",
      payloadVersion: "1",
      payloadFingerprint: "state",
      payload: {},
    },
    operationCursor: 0,
    projectionCursor: 0,
    progress: {
      semanticRevision: 1,
      committedProviderOperations: 7,
      committedToolOperations: 3,
      committedProjectionCursor: 0,
      assistantContentBytes: 0,
      acceptedActionCount: 0,
      delegatedCompletionCount: 0,
      stateFingerprint: "state",
      lastLivenessAt: "2026-07-29T00:00:00Z",
      lastSemanticProgressAt: "2026-07-29T00:00:00Z",
    },
    yieldReason: "operator_requested" as const,
    createdAt: "2026-07-29T00:00:00Z",
  };
}
