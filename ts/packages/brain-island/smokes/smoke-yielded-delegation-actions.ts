import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  BrainAction,
  BrainContinuationPayload,
  BrainImplementationId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { registerBrainHostRuntime } from "../src/index.js";

const dataDir = mkdtempSync(join(tmpdir(), "rusty-crew-yielded-delegation-"));
const native = await loadNativeBridge();
let engine = await initialize();
let engineRunning = true;

const parentSessionId = "yielded-delegation-parent" as SessionId;
const parentProfileId = "yielded-delegation-parent-profile" as ProfileId;
const childProfileId = "yielded-delegation-child-profile" as ProfileId;
const firstWakeId = "yielded-delegation-wake-1";
const childSessionId =
  `${parentSessionId}:delegated:${firstWakeId}:0` as SessionId;
const delegationAction: BrainAction = {
  type: "request_delegation",
  profileId: childProfileId,
  prompt: "Return a durable delegated completion.",
  correlationId: "yielded-delegation-proof",
  parentConsumption: "await_completion",
};
let continuationObserved = false;

try {
  await registerBrainHostRuntime(
    native,
    {
      implementationId:
        "yielded-delegation-child-brain" as BrainImplementationId,
      profileId: childProfileId,
      toolProfile: { tools: [] },
      modelConfig: { provider: "local", modelName: "child" },
    },
    {
      async wake() {
        return { events: [], actions: [] };
      },
    },
  );
  const parentBrain = await registerBrainHostRuntime(
    native,
    {
      implementationId:
        "yielded-delegation-parent-brain" as BrainImplementationId,
      profileId: parentProfileId,
      toolProfile: { tools: [] },
      modelConfig: { provider: "local", modelName: "parent" },
      strategy: {
        moduleId: "chat-completions",
        strategyId: "yielded-delegation-smoke",
        providerState: { mode: "unused" },
      },
    },
    {
      async wake(wake) {
        if (wake.continuationState === undefined) {
          return {
            events: [],
            actions: [delegationAction],
            outcome: "yielded" as const,
            continuationState: continuation({ round: 1 }),
          };
        }
        continuationObserved = true;
        assert.equal(wake.state.childCompletions.length, 1);
        assert.equal(
          wake.state.childCompletions[0]?.childSessionId,
          childSessionId,
        );
        assert.equal(
          wake.state.childCompletions[0]?.packet.summary,
          "delegated child completed after the parent yielded",
        );
        return { events: [], actions: [], outcome: "completed" as const };
      },
    },
  );

  await native.createSession({
    sessionId: parentSessionId,
    agentId: "yielded-delegation-parent-agent" as AgentId,
    profileId: parentProfileId,
    kind: "full",
    resourceLimits: { maxDelegationDepth: 1 },
  });

  const first = await native.buildBrainWakeRequestForSession({
    brain: parentBrain,
    sessionId: parentSessionId,
    systemPrompt: "yielded delegation smoke",
    roleAssemblyJson: new TextEncoder().encode("{}"),
    wakeId: firstWakeId,
  });
  assert.deepEqual(await native.wakeBrain(first), {
    wakeId: firstWakeId,
    accepted: true,
    outcome: "continuing",
  });

  const requested = await native.delegatedSessionStatus(childSessionId);
  assert.equal(requested.runStatus, "wake_requested");
  assert.equal(await native.diagnosticCountRows("worker_runs"), 1);

  const replay = await native.diagnosticSubmitBrainActionsJson(
    firstWakeId,
    parentSessionId,
    [delegationAction],
  );
  assert.equal(replay.acceptedActions, 1);
  assert.deepEqual(replay.rejectedActions, []);
  assert.equal(await native.diagnosticCountRows("worker_runs"), 1);

  const completion = await native.diagnosticSubmitBrainActionsJson(
    "yielded-delegation-child-completion",
    childSessionId,
    [
      {
        type: "deliver_completion",
        packet: {
          sessionId: childSessionId,
          status: "completed",
          summary: "delegated child completed after the parent yielded",
        },
      },
    ],
  );
  assert.equal(completion.acceptedActions, 1);

  const rejectedProfileId = "yielded-delegation-rejected-profile" as ProfileId;
  const rejectedSessionId = "yielded-delegation-rejected-session" as SessionId;
  const rejectedWakeId = "yielded-delegation-rejected-wake";
  const rejectedBrain = await registerBrainHostRuntime(
    native,
    {
      implementationId:
        "yielded-delegation-rejected-brain" as BrainImplementationId,
      profileId: rejectedProfileId,
      toolProfile: { tools: [] },
      modelConfig: { provider: "local", modelName: "rejected-parent" },
      strategy: {
        moduleId: "chat-completions",
        strategyId: "yielded-delegation-rejection-smoke",
        providerState: { mode: "unused" },
      },
    },
    {
      async wake() {
        return {
          events: [],
          actions: [
            {
              type: "request_delegation",
              profileId: "unregistered-child-profile" as ProfileId,
              prompt: "This action must be rejected before continuation.",
            },
          ],
          outcome: "yielded" as const,
          continuationState: continuation({ round: "rejected" }),
        };
      },
    },
  );
  await native.createSession({
    sessionId: rejectedSessionId,
    agentId: "yielded-delegation-rejected-agent" as AgentId,
    profileId: rejectedProfileId,
    kind: "full",
    resourceLimits: { maxDelegationDepth: 1 },
  });
  const rejectionEvents = await native.subscribeEvents({
    eventKinds: ["brain_event_observed"],
    sessionId: rejectedSessionId,
  });
  const rejectedWake = await native.buildBrainWakeRequestForSession({
    brain: rejectedBrain,
    sessionId: rejectedSessionId,
    systemPrompt: "yielded delegation rejection smoke",
    roleAssemblyJson: new TextEncoder().encode("{}"),
    wakeId: rejectedWakeId,
  });
  await assert.rejects(
    () => native.wakeBrain(rejectedWake),
    /brain action admission rejected/,
  );
  const observedRejectionEvents = await native.drainSubscriptionEvents(
    rejectionEvents,
    10,
  );
  await native.unsubscribeEvents(rejectionEvents);
  assert.ok(
    observedRejectionEvents.some(
      (event) =>
        event.type === "brain_event_observed" &&
        event.event.type === "provider_status" &&
        event.event.metadataJson?.includes("brain_action_rejected"),
    ),
  );
  const rejectedDiagnostics = await native.logicalTurnDiagnostics({
    sessionId: rejectedSessionId,
    includeTerminal: true,
    limit: 10,
  });
  assert.equal(rejectedDiagnostics.items[0]?.operatorState, "failed");
  assert.equal(rejectedDiagnostics.items[0]?.reasonCode, "logical_turn_failed");

  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  engineRunning = false;
  engine = await initialize();
  engineRunning = true;
  assert.equal(await native.diagnosticCountRows("worker_runs"), 1);

  const resumed = await native.buildBrainWakeRequestForSession({
    brain: parentBrain,
    sessionId: parentSessionId,
    systemPrompt: "ignored on durable continuation",
    roleAssemblyJson: new TextEncoder().encode("{}"),
    wakeId: "yielded-delegation-wake-2",
  });
  assert.ok(resumed.continuationState);
  assert.deepEqual(await native.wakeBrain(resumed), {
    wakeId: "yielded-delegation-wake-2",
    accepted: true,
    outcome: "completed",
  });
  assert.equal(continuationObserved, true);

  const diagnostics = await native.logicalTurnDiagnostics({
    sessionId: parentSessionId,
    includeTerminal: true,
    limit: 10,
  });
  assert.equal(diagnostics.items.length, 1);
  assert.equal(diagnostics.items[0]?.operatorState, "completed");
  assert.equal(diagnostics.items[0]?.continuationCount, 2);

  console.log(
    JSON.stringify(
      {
        firstWakeOutcome: "continuing",
        workerRuns: await native.diagnosticCountRows("worker_runs"),
        replayWorkerRuns: 1,
        restartHydrated: true,
        childCompletionObserved: continuationObserved,
        rejectionReasonCode: "brain_action_rejected",
        logicalTurnState: diagnostics.items[0]?.operatorState,
      },
      null,
      2,
    ),
  );
} finally {
  if (engineRunning) {
    await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  }
  rmSync(dataDir, { force: true, recursive: true });
}

async function initialize() {
  return native.initializeEngine({
    engineDataDir: dataDir,
    clock: { fixed: "2026-08-01T00:00:00Z" },
    defaultTurnBudget: 3,
    defaultIdleTimeoutMs: 1_000,
  });
}

function continuation(payload: unknown): BrainContinuationPayload {
  return {
    moduleId: "chat-completions",
    payloadVersion: "yielded-delegation-smoke-v1",
    payload,
    payloadFingerprint: createHash("sha256")
      .update(JSON.stringify(payload))
      .digest("hex"),
  };
}
