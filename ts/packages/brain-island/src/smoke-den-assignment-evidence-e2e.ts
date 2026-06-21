import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AdapterId,
  AgentId,
  BrainActionBatch,
  ChannelBindingRecord,
  CoreEvent,
  NormalizedChannelActivityProjection,
  ProfileId,
  ProjectId,
  SessionId,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import {
  createDenRouterMetadataProjection,
  createMemoryDenRouterMetadataStore,
  denProductWorkRef,
  dispatchCompletionEvidenceProjection,
  ingestDenProductReference,
} from "@rusty-crew/adapter-den";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-den-evidence-e2e-"));
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir: root,
  clock: { fixed: "2026-06-20T10:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

let completionSubscription: SubscriptionHandle | undefined;

try {
  const adapterId = "den-adapter" as AdapterId;
  const sessionId = "session-alpha" as SessionId;
  const agentId = "agent-alpha" as AgentId;
  const profileId = "profile-alpha" as ProfileId;
  const projectId = "rusty-crew" as ProjectId;
  const binding: ChannelBindingRecord = {
    bindingId: "binding-alpha",
    adapterId,
    provider: "den_channels",
    agentId,
    sessionId,
    profileId,
    externalChannelId: "channel-alpha",
    externalThreadId: "thread-alpha",
    status: "active",
  };
  let denProductIngressCalls = 0;
  const denProductIngress = {
    async injectDenDataUpdate(
      update: Parameters<typeof native.injectDenDataUpdate>[0],
    ) {
      denProductIngressCalls += 1;
      return native.injectDenDataUpdate(update);
    },
  };

  await native.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
  });
  completionSubscription = await native.subscribeEvents({
    eventKinds: ["completion_packet_delivered"],
    sessionId,
  });

  const assignment = await ingestDenProductReference(
    {
      projectId,
      entityKind: "assignment",
      entityId: "assignment-3056",
      revision: "rev-1",
      label: "Den product context for e2e proof",
      provenance: {
        source: "smoke",
        providerToken: "should-not-leak",
      },
    },
    denProductIngress,
  );
  assert.equal(assignment.status, "accepted");
  assert.equal(denProductIngressCalls, 1);

  const claimAttempt = await ingestDenProductReference(
    {
      projectId,
      entityKind: "assignment",
      entityId: "assignment-3056",
      operation: "claim",
    },
    denProductIngress,
  );
  assert.equal(claimAttempt.status, "denied");
  assert.equal(denProductIngressCalls, 1);

  const metadataStore = createMemoryDenRouterMetadataStore({
    now: () => "2026-06-20T10:00:10Z",
  });
  metadataStore.upsertRouterMetadata(
    createDenRouterMetadataProjection({
      adapterId,
      bindingId: binding.bindingId,
      runtime: { agentId, sessionId, profileId },
      providerRefs: {
        provider: "den_channels",
        externalChannelId: binding.externalChannelId,
        externalThreadId: binding.externalThreadId,
      },
      denWorkRefs: [
        {
          refKind: "assignment",
          id: "assignment-3056",
          projectId,
        },
      ],
      status: "active",
      observedAt: "2026-06-20T10:00:09Z",
      provenance: {
        source: "smoke",
        rawPrompt: "should-not-leak",
      },
    }),
  );

  const wakeId = "wake-3056";
  const batch: BrainActionBatch = {
    wakeId,
    sessionId,
    actions: [
      {
        type: "deliver_completion",
        packet: {
          sessionId,
          status: "completed",
          summary:
            "Rust-owned completion for Den assignment evidence e2e proof.",
        },
      },
    ],
  };
  const receipt = await native.submitBrainActions(batch);
  assert.equal(receipt.acceptedActions, 1);
  assert.equal(await native.countRows("completion_packets"), 1);
  assert.equal(await native.countRows("worker_runs"), 0);

  const completionEvents = await native.drainSubscriptionEvents(
    completionSubscription,
    10,
  );
  const completionEvent = completionEvents.find(
    (
      event,
    ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
      event.type === "completion_packet_delivered" &&
      event.packet.sessionId === sessionId,
  );
  assert(completionEvent, "runtime completion event should be observed");

  const projectedActivities: NormalizedChannelActivityProjection[] = [];
  let failNextProjection = true;
  const projectionSink = {
    sendMessage(): void {
      throw new Error("message projection should not run in this proof");
    },
    sendActivity(activity: NormalizedChannelActivityProjection): void {
      if (failNextProjection) {
        failNextProjection = false;
        throw new Error("simulated Den projection outage");
      }
      projectedActivities.push(activity);
    },
  };

  const assignmentWorkRef = denProductWorkRef({
    refKind: "assignment",
    id: "assignment-3056",
    projectId,
  });
  const degraded = await dispatchCompletionEvidenceProjection(projectionSink, {
    binding,
    event: completionEvent,
    workRefs: [assignmentWorkRef],
    now: "2026-06-20T10:00:11Z",
  });
  assert.equal(degraded.dispatch.accepted, false);
  assert.equal(await native.countRows("completion_packets"), 1);

  const accepted = await dispatchCompletionEvidenceProjection(projectionSink, {
    binding,
    event: completionEvent,
    workRefs: [assignmentWorkRef],
    now: "2026-06-20T10:00:12Z",
  });
  assert.equal(accepted.dispatch.accepted, true);
  assert.equal(projectedActivities.length, 1);
  assert.equal(
    projectedActivities[0]?.resultRef,
    "runtime:completion_packet:session-alpha:completed",
  );
  assert.equal(
    projectedActivities[0]?.workRef,
    "den:assignment:assignment-3056",
  );

  const metadata = metadataStore.queryRouterMetadata({
    bindingId: "binding-alpha",
    agentId,
    sessionId,
    provider: "den_channels",
    externalChannelId: "channel-alpha",
  });
  assert.equal(metadata.total, 1);
  assert.equal(metadata.items[0]?.provenance.rawPrompt, "[redacted]");

  console.log(
    JSON.stringify(
      {
        assignmentIngress: assignment.status,
        claimAttempt: claimAttempt.status,
        completionPackets: await native.countRows("completion_packets"),
        workerRuns: await native.countRows("worker_runs"),
        degradedProjection: degraded.dispatch,
        acceptedProjection: accepted.dispatch,
        metadataRefs: metadata.items[0]?.workRefs.map(
          (ref) => `${ref.sourceDomain}:${ref.refKind}:${ref.id}`,
        ),
      },
      null,
      2,
    ),
  );
} finally {
  if (completionSubscription !== undefined) {
    await native.unsubscribeEvents(completionSubscription);
  }
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(root, { recursive: true, force: true });
}
