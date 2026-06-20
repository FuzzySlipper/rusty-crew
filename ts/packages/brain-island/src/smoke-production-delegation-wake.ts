import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  BrainAction,
  BrainImplementationHandle,
  BrainImplementationId,
  CoreEvent,
  ProfileId,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createLocalBrain,
  registerBrainImplementationRuntime,
} from "./index.js";

const encoder = new TextEncoder();
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-production-delegation-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-19T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const plannerSessionId = "planner-session" as SessionId;
  const plannerAgentId = "planner" as AgentId;
  const plannerProfileId = "planner-profile" as ProfileId;
  const coderProfileId = "coder-profile" as ProfileId;

  await native.createSession({
    sessionId: plannerSessionId,
    agentId: plannerAgentId,
    profileId: plannerProfileId,
    kind: "full",
  });

  const plannerBrain = await registerBrainImplementationRuntime(
    native,
    {
      implementationId: "planner-brain" as BrainImplementationId,
      profileId: plannerProfileId,
      toolProfile: { tools: [] },
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createLocalBrain(({ wake }): BrainAction[] => {
      assert.equal(wake.sessionId, plannerSessionId);
      assert.equal(wake.state.pendingMessages.length, 1);
      return [
        {
          type: "request_delegation",
          profileId: coderProfileId,
          taskId: "2844" as TaskId,
          prompt: "Complete the delegated production wake proof.",
          expectedOutput: "completion packet",
          resourceLimits: {
            workdir: "/home/dev/rusty-crew",
            maxDurationMs: 30_000,
            maxDelegationDepth: 0,
          },
          timeoutMs: 30_000,
          priority: "high",
          fanOutGroupId: "production-delegation-proof",
          correlationId: "production-delegation-correlation",
          parentConsumption: "await_completion",
        },
      ];
    }),
  );

  const coderBrain = await registerBrainImplementationRuntime(
    native,
    {
      implementationId: "coder-brain" as BrainImplementationId,
      profileId: coderProfileId,
      toolProfile: { tools: [] },
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createLocalBrain(({ wake }): BrainAction[] => {
      assert.equal(wake.state.session.kind, "delegated");
      assert.equal(
        wake.state.session.delegation?.parentSessionId,
        plannerSessionId,
      );
      assert.equal(
        wake.state.session.delegation?.correlationId,
        "production-delegation-correlation",
      );
      assert.equal(wake.state.session.resourceLimits.maxDelegationDepth, 0);
      assert.equal(wake.state.pendingMessages.length, 1);
      assert.match(
        wake.state.pendingMessages[0]!.body,
        /production wake proof/,
      );

      return [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "delegated production wake proof completed",
          },
        },
      ];
    }),
  );

  const wakeSubscription = await native.subscribeEvents({
    eventKinds: ["brain_wake_requested"],
  });
  const completionSubscription = await native.subscribeEvents({
    eventKinds: ["completion_packet_delivered"],
  });

  await native.routeAgentMessage(
    "human",
    plannerAgentId,
    "Start a production delegation wake.",
  );

  const processedWakes: SessionId[] = [];
  const brainBySession = new Map<SessionId, BrainImplementationHandle>([
    [plannerSessionId, plannerBrain],
  ]);

  for (
    let attempt = 0;
    processedWakes.length < 2 && attempt < 4;
    attempt += 1
  ) {
    const wakeEvents = await native.drainSubscriptionEvents(
      wakeSubscription,
      8,
    );
    for (const event of wakeEvents) {
      assert.equal(event.type, "brain_wake_requested");
      const sessionId = event.sessionId;
      const brain = brainBySession.get(sessionId) ?? coderBrain;
      const request = await native.buildBrainWakeRequestForSession({
        brain,
        sessionId,
        systemPrompt: `Production delegation wake for ${sessionId}`,
        roleAssemblyJson: encoder.encode(
          JSON.stringify({
            instructions: "Use the deterministic local brain.",
          }),
        ),
        wakeId: `production-wake-${processedWakes.length + 1}`,
      });
      await native.wakeBrain(request);
      processedWakes.push(sessionId);
    }
  }

  assert.equal(processedWakes[0], plannerSessionId);
  assert.match(processedWakes[1]!, /^planner-session:delegated:/);

  const completionEvents = await native.drainSubscriptionEvents(
    completionSubscription,
    4,
  );
  const completion = completionEvents.find(
    (
      event,
    ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
      event.type === "completion_packet_delivered",
  );
  assert.equal(
    completion?.packet.summary,
    "delegated production wake proof completed",
  );

  await native.unsubscribeEvents(wakeSubscription);
  await native.unsubscribeEvents(completionSubscription);

  console.log(
    JSON.stringify(
      {
        processedWakes,
        completionSummary: completion?.packet.summary,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}
