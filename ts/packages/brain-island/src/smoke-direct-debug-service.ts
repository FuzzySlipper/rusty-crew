import assert from "node:assert/strict";
import type {
  AgentId,
  BodyState,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  buildRuntimeDiagnosticsProjection,
  buildToolRegistryDiagnostics,
  inspectDirectDebugSession,
  requestDirectDebugTurn,
} from "./index.js";
import type {
  DirectDebugTurnExecutorInput,
  DirectDebugTurnOutcome,
} from "./index.js";

const session = {
  handle: 1 as SessionHandle,
  sessionId: "debug-session" as SessionId,
  agentId: "debug-agent" as AgentId,
  profileId: "debug-profile" as ProfileId,
  kind: "full",
  resourceLimits: {
    workdir: "/home/dev/rusty-crew",
    maxDurationMs: 30_000,
    maxDelegationDepth: 2,
  },
  toolProfile: {
    tools: [
      {
        name: "read_file",
        description: "Read files.",
      },
    ],
  },
  status: "active",
  brainTurnCount: 3,
  createdAt: "2026-06-20T00:00:00Z",
  lastActiveAt: "2026-06-20T00:01:00Z",
} satisfies SessionState;
const bodyState = {
  session,
  pendingMessages: [
    {
      from: "debug-user" as AgentId,
      to: session.agentId,
      body: "Please inspect this. token=super-secret",
      correlationId: "debug-correlation",
    },
  ],
  recentEvents: [],
  childCompletions: [],
  fanOutGroups: [],
  deltaPolicy: {
    mode: "frozen_snapshot_next_wake",
    queueOwner: "body",
    queuedMessageTtlMs: 5_000,
    maxQueuedMessages: 10,
  },
} satisfies BodyState;
const diagnostics = buildRuntimeDiagnosticsProjection({
  now: "2026-06-20T00:02:00Z",
  sessions: [session],
  tools: [
    buildToolRegistryDiagnostics({
      inventoryRequest: {
        requestedToolsets: ["local_code_read"],
      },
    }),
  ],
  observation: {
    enabled: true,
    writerAvailable: true,
  },
});
const source = {
  session,
  bodyState,
  systemPrompt: "System prompt with password=hunter2.",
  roleAssembly: {
    instructions: "# Profile\nKeep secrets out of direct debug output.",
    initialMessages: [
      {
        from: "system" as AgentId,
        to: session.agentId,
        body: "Initial context with api_key=abc123.",
      },
    ],
  },
  toolDiagnostics: buildToolRegistryDiagnostics({
    inventoryRequest: {
      requestedToolsets: ["local_code_read"],
      requestedTools: ["missing_debug_tool"],
    },
  }),
};
const inspection = inspectDirectDebugSession(
  {
    sessionId: session.sessionId,
    includeMessageBodies: true,
  },
  {
    diagnostics,
    sessions: [source],
    recentEvents: [
      {
        id: "event-1",
        createdAt: "2026-06-20T00:02:00Z",
        source: "direct-debug-smoke",
        eventType: "debug.inspect",
        summary: "Debug inspected token=event-secret",
        workRef: {
          sessionId: session.sessionId,
        },
      },
    ],
    now: () => "2026-06-20T00:03:00Z",
  },
);

assert.equal(inspection.ok, true);
assert.equal(inspection.data.source, "direct_debug");
assert.equal(inspection.data.context.rawPromptIncluded, false);
assert.equal(inspection.data.context.systemPrompt.text, undefined);
assert.equal(inspection.data.context.systemPrompt.chars, 36);
assert.match(
  inspection.data.pendingMessages[0]?.bodyPreview ?? "",
  /token=\[redacted\]/,
);
assert.doesNotMatch(
  JSON.stringify(inspection.data),
  /hunter2|abc123|super-secret|event-secret/,
);
assert.equal(inspection.data.toolContext?.summary.missingTools, 1);
assert.equal(inspection.data.controls.directTurnInjection, "disabled");

const captured: DirectDebugTurnExecutorInput[] = [];
const outcome: DirectDebugTurnOutcome = {
  status: "accepted",
  summary: "direct debug turn accepted",
  wakeId: "wake-debug-1",
};
const disabledTurn = await requestDirectDebugTurn(
  {
    sessionId: session.sessionId,
    actorId: "operator",
    body: "Run one debug turn.",
  },
  {
    diagnostics,
    sessions: [source],
  },
);
assert.equal(disabledTurn.ok, false);
assert.equal(disabledTurn.error.reasonCode, "direct_turn_injection_disabled");

const acceptedTurn = await requestDirectDebugTurn(
  {
    sessionId: session.sessionId,
    actorId: "operator",
    body: "Run one debug turn with Bearer abc.def",
    reason: "operator requested direct debug",
    requestId: "debug-request",
    idempotencyKey: "debug-key",
  },
  {
    diagnostics,
    sessions: [source],
    allowDirectTurnInjection: true,
    now: () => "2026-06-20T00:04:00Z",
    turnExecutor: {
      submitDirectDebugTurn(input) {
        captured.push(input);
        return outcome;
      },
    },
  },
);
assert.equal(acceptedTurn.ok, true);
assert.equal(acceptedTurn.data.wakeId, "wake-debug-1");
assert.equal(captured[0]?.source, "direct_debug");
assert.equal(captured[0]?.requestId, "debug-request");
assert.equal(captured[0]?.idempotencyKey, "debug-key");
assert.doesNotMatch(captured[0]?.body ?? "", /abc\.def/);

console.log(
  JSON.stringify(
    {
      session: inspection.data.session,
      pendingMessages: inspection.data.pendingMessages.length,
      toolContextMissing: inspection.data.toolContext?.summary.missingTools,
      control: inspection.data.controls.directTurnInjection,
      turnSource: captured[0]?.source,
      wakeId: acceptedTurn.data.wakeId,
    },
    null,
    2,
  ),
);
