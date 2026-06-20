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
  buildDelegatedRoleAssembly,
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

  let parentConsumedChildCompletion = false;
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
      if (wake.state.childCompletions.length > 0) {
        assert.equal(wake.state.childCompletions.length, 1);
        assert.equal(
          wake.state.childCompletions[0]!.packet.status,
          "completed",
        );
        assert.equal(
          wake.state.childCompletions[0]!.packet.summary,
          "delegated production wake proof completed",
        );
        parentConsumedChildCompletion = true;
        return [
          {
            type: "deliver_completion",
            packet: {
              sessionId: plannerSessionId,
              status: "completed",
              summary: "prime consumed delegated completion and finished",
            },
          },
        ];
      }

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
      toolProfile: {
        tools: ["read_file", "patch", "terminal"].map((name) => ({
          name,
          description: `${name} delegated profile tool`,
        })),
      },
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createLocalBrain(({ wake }): BrainAction[] => {
      assert.equal(wake.state.session.kind, "delegated");
      assert.deepEqual(
        wake.state.session.toolProfile.tools.map((tool) => tool.name),
        ["read_file", "patch", "terminal"],
      );
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
  const lifecycleSubscription = await native.subscribeEvents({
    eventKinds: ["delegation_lifecycle_observed"],
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
    processedWakes.length < 3 && attempt < 6;
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
      const roleAssembly =
        sessionId === plannerSessionId
          ? { instructions: "Use the deterministic local brain." }
          : buildDelegatedRoleAssembly({
              role: "coder",
              profile: {
                profileId: coderProfileId,
                displayName: "Delegated Coder",
                systemPrompt: "Use a focused implementation posture.",
                toolNames: ["read_file", "patch", "terminal"],
              },
              context: {
                sessionId,
                agentId: `agent:${sessionId}` as AgentId,
                parentSessionId: plannerSessionId,
                parentAgentId: plannerAgentId,
                sourceWakeId: "production-wake-1",
                sourceActionIndex: 0,
                taskId: "2844" as TaskId,
                prompt: "Complete the delegated production wake proof.",
                expectedOutput: "completion packet",
                correlationId: "production-delegation-correlation",
                resourceLimits: {
                  workdir: "/home/dev/rusty-crew",
                  maxDurationMs: 30_000,
                  maxDelegationDepth: 0,
                },
                acceptanceCriteria: [
                  "Wake through the registered bridge path.",
                  "Return a completion packet.",
                ],
              },
            });
      const request = await native.buildBrainWakeRequestForSession({
        brain,
        sessionId,
        systemPrompt: `Production delegation wake for ${sessionId}`,
        roleAssemblyJson: encoder.encode(JSON.stringify(roleAssembly)),
        wakeId: `production-wake-${processedWakes.length + 1}`,
      });
      await native.wakeBrain(request);
      processedWakes.push(sessionId);
    }
  }

  assert.equal(processedWakes[0], plannerSessionId);
  assert.match(processedWakes[1]!, /^planner-session:delegated:/);
  assert.equal(processedWakes[2], plannerSessionId);

  const completionEvents = await native.drainSubscriptionEvents(
    completionSubscription,
    4,
  );
  const childCompletion = completionEvents.find(
    (
      event,
    ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
      event.type === "completion_packet_delivered" &&
      event.packet.sessionId !== plannerSessionId,
  );
  const parentCompletion = completionEvents.find(
    (
      event,
    ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
      event.type === "completion_packet_delivered" &&
      event.packet.sessionId === plannerSessionId,
  );
  assert.equal(
    childCompletion?.packet.summary,
    "delegated production wake proof completed",
  );
  assert.equal(
    parentCompletion?.packet.summary,
    "prime consumed delegated completion and finished",
  );
  assert.equal(parentConsumedChildCompletion, true);
  const lifecycleEvents = await native.drainSubscriptionEvents(
    lifecycleSubscription,
    8,
  );
  const lifecyclePhases = lifecycleEvents
    .filter(
      (
        event,
      ): event is Extract<
        CoreEvent,
        { type: "delegation_lifecycle_observed" }
      > => event.type === "delegation_lifecycle_observed",
    )
    .map((event) => event.lifecycle.phase);
  assert.deepEqual(lifecyclePhases, ["created", "wake_requested", "completed"]);
  const delegatedStatus = await native.delegatedSessionStatus(
    processedWakes[1]!,
  );
  assert.equal(delegatedStatus.runStatus, "completed");
  assert.equal(delegatedStatus.terminal, true);
  const counts = {
    sessions: await native.countRows("sessions"),
    workerRuns: await native.countRows("worker_runs"),
    completionPackets: await native.countRows("completion_packets"),
  };
  assert.equal(counts.sessions, 2);
  assert.equal(counts.workerRuns, 1);
  assert.equal(counts.completionPackets, 2);

  await native.unsubscribeEvents(wakeSubscription);
  await native.unsubscribeEvents(completionSubscription);
  await native.unsubscribeEvents(lifecycleSubscription);

  console.log(
    JSON.stringify(
      {
        processedWakes,
        childCompletionSummary: childCompletion?.packet.summary,
        parentCompletionSummary: parentCompletion?.packet.summary,
        parentConsumedChildCompletion,
        lifecyclePhases,
        delegatedRunStatus: delegatedStatus.runStatus,
        counts,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}
