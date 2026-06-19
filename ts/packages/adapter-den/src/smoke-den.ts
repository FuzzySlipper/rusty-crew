import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  DenDataUpdate,
  EventReceipt,
  ExternalEvent,
  ProjectId,
} from "@rusty-crew/contracts";
import { createDenAdapter, createMemoryDenProjectionSink } from "./index.js";

const adapterId = "den" as AdapterId;
let sequence = 0;
const denUpdates: DenDataUpdate[] = [];
const externalEvents: ExternalEvent[] = [];

const ingress = {
  injectDenDataUpdate(update: DenDataUpdate): EventReceipt {
    denUpdates.push(update);
    sequence += 1;
    return { accepted: true, sequence };
  },
  injectExternalEvent(event: ExternalEvent): EventReceipt {
    externalEvents.push(event);
    sequence += 1;
    return { accepted: true, sequence };
  },
};

const projectionSink = createMemoryDenProjectionSink();
const adapter = createDenAdapter({
  adapterId,
  ingress,
  projectionSink,
});

const updateReceipt = await adapter.injectDataUpdate({
  projectId: "pi-crew" as ProjectId,
  entityKind: "task",
  entityId: "2767",
  revision: "smoke-revision",
});

assert.equal(updateReceipt.accepted, true);
assert.equal(denUpdates.length, 1);

await adapter.injectExternalEventPayload("den", {
  type: "adapter_status",
  status: "connected",
});

assert.equal(externalEvents.length, 1);

const projectionResult = await adapter.projectEvent({
  type: "agent_message_routed",
  message: {
    from: "planner" as AgentId,
    to: "worker" as AgentId,
    body: "internal routing is observed, not delegated to Den",
  },
});

assert.equal(projectionResult.accepted, true);
assert.equal(projectionSink.projections.length, 1);

projectionSink.failNext(new Error("simulated Den outage"));
const droppedProjection = await adapter.projectEvent({
  type: "den_data_updated",
  update: denUpdates[0]!,
});

assert.equal(droppedProjection.dropped, true);
assert.equal(adapter.status().state, "degraded");
assert.equal(adapter.status().droppedProjections, 1);

const postOutageReceipt = await adapter.injectExternalEventPayload("den", {
  type: "adapter_status",
  status: "disconnected",
  detail: "observability projection unavailable",
});

assert.equal(postOutageReceipt.accepted, true);
assert.equal(externalEvents.length, 2);

console.log(
  JSON.stringify(
    {
      registration: adapter.registration(),
      denUpdates: denUpdates.length,
      externalEvents: externalEvents.length,
      projectedEvents: adapter.status().projectedEvents,
      droppedProjections: adapter.status().droppedProjections,
      degradedWithoutBlockingIngress: postOutageReceipt.accepted,
    },
    null,
    2,
  ),
);
