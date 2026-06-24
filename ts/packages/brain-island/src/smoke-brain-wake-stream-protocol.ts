import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  BrainImplementationId,
  BrainWakeRequest,
  BrainWakeStreamItem,
  CompletionPacket,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  brainWakeStreamItemsFromExecutionResult,
  loadNativeBridge,
} from "@rusty-crew/native-bridge";

const engineDataDir = mkdtempSync(join(tmpdir(), "rusty-crew-wake-stream-"));
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-24T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const sessionId = "wake-stream-session" as SessionId;
  const profileId = "wake-stream-profile" as ProfileId;
  const agentId = "wake-stream-agent" as AgentId;
  const wakeId = "wake-stream-1";

  const request = fakeRequest(wakeId, sessionId);
  const legacyItems = brainWakeStreamItemsFromExecutionResult(request, {
    events: [
      {
        wakeId,
        sessionId,
        event: { type: "started" },
      },
    ],
    actions: [],
  });
  assert.deepEqual(
    legacyItems.map((item) => item.type),
    ["event", "actions"],
  );

  const streamItems: BrainWakeStreamItem[] = [
    {
      type: "event",
      event: {
        wakeId,
        sessionId,
        event: {
          type: "provider_status",
          level: "degraded",
          message: "provider retry scheduled",
          metadataJson: JSON.stringify({ attempt: 1 }),
        },
      },
    },
    {
      type: "actions",
      batch: {
        wakeId,
        sessionId,
        actions: [],
      },
    },
  ];
  assert.equal(
    brainWakeStreamItemsFromExecutionResult(request, {
      events: [],
      actions: [],
      stream: streamItems,
    }),
    streamItems,
  );
  assert.throws(
    () =>
      brainWakeStreamItemsFromExecutionResult(request, {
        events: [],
        actions: [],
        stream: streamItems.slice(0, 1),
      }),
    /must end with actions or wake_failed/,
  );

  await native.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
  });

  const brain = await native.registerBrainRuntime(
    {
      implementationId: "wake-stream-brain" as BrainImplementationId,
      profileId,
      toolProfile: { tools: [] },
      modelConfig: {
        provider: "local",
        modelName: "stream-protocol",
      },
    },
    {
      wake(wakeRequest) {
        return {
          events: [],
          actions: [],
          stream: [
            {
              type: "event",
              event: {
                wakeId: wakeRequest.wakeId,
                sessionId: wakeRequest.sessionId,
                event: { type: "started" },
              },
            },
            {
              type: "event",
              event: {
                wakeId: wakeRequest.wakeId,
                sessionId: wakeRequest.sessionId,
                event: {
                  type: "provider_status",
                  level: "degraded",
                  message: "fake provider recovered",
                  metadataJson: JSON.stringify({ recovered: true }),
                },
              },
            },
            {
              type: "event",
              event: {
                wakeId: wakeRequest.wakeId,
                sessionId: wakeRequest.sessionId,
                event: { type: "finished" },
              },
            },
            {
              type: "actions",
              batch: {
                wakeId: wakeRequest.wakeId,
                sessionId: wakeRequest.sessionId,
                actions: [
                  {
                    type: "deliver_completion",
                    packet: {
                      sessionId: wakeRequest.sessionId,
                      status: "completed",
                      summary: "stream protocol smoke completed",
                    } satisfies CompletionPacket,
                  },
                ],
              },
            },
          ],
        };
      },
    },
  );

  const eventSubscription = await native.subscribeEvents({
    eventKinds: ["brain_event_observed", "brain_actions_accepted"],
    sessionId,
  });
  const buffered = await native.buildBrainWakeRequestForSession({
    brain,
    sessionId,
    systemPrompt: "stream protocol smoke",
    roleAssemblyJson: new TextEncoder().encode("{}"),
    wakeId,
  });
  const accepted = await native.wakeBrain(buffered);
  assert.deepEqual(accepted, { wakeId, accepted: true });

  const observed = await native.drainSubscriptionEvents(eventSubscription, 8);
  assert.equal(observed.length, 4);
  assert.deepEqual(
    observed.map((event) => event.type),
    [
      "brain_event_observed",
      "brain_event_observed",
      "brain_event_observed",
      "brain_actions_accepted",
    ],
  );
  assert.deepEqual(observed[1], {
    type: "brain_event_observed",
    sessionId,
    wakeId,
    event: {
      type: "provider_status",
      level: "degraded",
      message: "fake provider recovered",
      metadataJson: JSON.stringify({ recovered: true }),
    },
  });
  await native.unsubscribeEvents(eventSubscription);

  console.log(
    JSON.stringify(
      {
        wakeId,
        accepted: accepted.accepted,
        streamItemTypes: streamItems.map((item) => item.type),
        observedEvents: observed.map((event) => event.type),
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

function fakeRequest(wakeId: string, sessionId: SessionId): BrainWakeRequest {
  return {
    brain: 1 as BrainWakeRequest["brain"],
    sessionId,
    bodyState: 1 as BrainWakeRequest["bodyState"],
    systemPrompt: 2 as BrainWakeRequest["systemPrompt"],
    roleAssembly: 3 as BrainWakeRequest["roleAssembly"],
    wakeId,
  };
}
