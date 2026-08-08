import assert from "node:assert/strict";
import test from "node:test";

import type { CoreEvent, SessionState } from "@rusty-crew/contracts";

import {
  appendCoreEventsToChatLog,
  type ServiceWakeDispatchContext,
} from "../src/service-wake-dispatch.js";

test("chat terminal events keep completion before turn finished", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const context = {
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
  } as unknown as ServiceWakeDispatchContext;
  const session = {
    sessionId: "session-1",
  } as SessionState;
  const wakeId = "wake-1";

  await appendCoreEventsToChatLog(context, session, wakeId, [
    {
      type: "brain_event_observed",
      sessionId: session.sessionId,
      wakeId,
      event: { type: "finished" },
    } as CoreEvent,
  ]);

  assert.equal(appended.length, 0);

  await appendCoreEventsToChatLog(context, session, wakeId, [
    {
      type: "completion_packet_delivered",
      packet: {
        sessionId: session.sessionId,
        status: "completed",
        summary: "wake completed",
      },
    } as CoreEvent,
  ]);

  assert.deepEqual(
    appended.map((event) => event.kind),
    ["assistant_message_completed", "assistant_turn_finished"],
  );
  assert.deepEqual(appended[0]?.payload, {
    status: "completed",
    summary: "wake completed",
    wake_id: wakeId,
  });
  assert.deepEqual(appended[1]?.payload, { wake_id: wakeId });
});

test("logical turn yields project as continuing rather than terminal chat events", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const context = {
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
  } as unknown as ServiceWakeDispatchContext;
  const session = { sessionId: "session-1" } as SessionState;

  await appendCoreEventsToChatLog(context, session, "dispatch-wake", [
    {
      type: "logical_turn_lifecycle_observed",
      lifecycle: {
        projectionId: "projection-1",
        logicalTurnId: "turn-1",
        sessionId: session.sessionId,
        wakeId: "source-wake",
        continuationId: "continuation-2",
        continuationCount: 2,
        kind: "continuation_yielded",
        phase: "yielded",
        operatorState: "queued_to_continue",
        progressClassification: "provider_progress",
        progress: {
          semanticRevision: 2,
          committedProviderOperations: 1,
          committedToolOperations: 0,
          committedProjectionCursor: 1,
          assistantContentBytes: 128,
          acceptedActionCount: 0,
          delegatedCompletionCount: 0,
          stateFingerprint: "progress-2",
          lastLivenessAt: "2026-07-29T00:00:00Z",
          lastSemanticProgressAt: "2026-07-29T00:00:00Z",
          consecutiveNoProgressSamples: 0,
        },
        reasonCode: "work_quantum_reached",
        summary: "turn will continue",
        occurredAt: "2026-07-29T00:00:00Z",
        logicalTurnRevision: 3,
      },
    },
  ]);

  assert.deepEqual(
    appended.map((event) => event.kind),
    ["logical_turn_yielding", "logical_turn_queued_to_continue"],
  );
  assert.equal((appended[0]?.payload as { phase?: string }).phase, "yielded");
});

test("completed compaction events persist a stable artifact for restart hydration", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const saved: Array<Record<string, unknown>> = [];
  const context = {
    bridge: {
      saveContextCompactionArtifact: async (
        artifact: Record<string, unknown>,
      ) => {
        saved.push(artifact);
        return artifact;
      },
    },
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
    now: () => "2026-08-04T00:00:00.000Z",
    recordEvent: () => undefined,
  } as unknown as ServiceWakeDispatchContext;
  const session = { sessionId: "session-1" } as SessionState;
  const event = {
    type: "brain_event_observed",
    sessionId: session.sessionId,
    wakeId: "wake-1",
    event: {
      type: "provider_status",
      level: "info",
      message: "context compacted",
      metadataJson: JSON.stringify({
        kind: "context_compaction_completed",
        artifact: {
          sequence: 1,
          strategyId: "rolling_summary_compaction",
          strategyRevision: "roleplay-adapter-v7",
          strategyPayloadMetadata: {
            schema_version: 1,
            payload_lineage: { parentArtifactId: "artifact-parent" },
            preservation_payload: { facts: ["scene fact"] },
          },
          reasonCode: "context_fill_threshold_exceeded",
          usageBefore: {
            inputTokens: 90,
            contextWindowTokens: 100,
            fillPercent: 90,
            source: "provider",
          },
          estimatedTokensAfter: 40,
          compactedItemCount: 4,
          retainedItemCount: 3,
          summaryText: "Earlier work was summarized.",
          providerChainAction: null,
        },
      }),
    },
  } as unknown as CoreEvent;

  await appendCoreEventsToChatLog(context, session, "wake-1", [event]);
  await appendCoreEventsToChatLog(context, session, "wake-1", [event]);

  assert.equal(appended.length, 2);
  assert.equal(saved.length, 2);
  assert.equal(saved[0]?.artifact_id, saved[1]?.artifact_id);
  assert.equal(saved[0]?.session_id, session.sessionId);
  assert.equal(saved[0]?.strategy_id, "rolling_summary_compaction");
  assert.equal(saved[0]?.created_at, "2026-08-04T00:00:00.000Z");
  assert.deepEqual(saved[0]?.estimate_after_json, {
    tokens: 40,
    source: "serialized_compaction_projection",
  });
  assert.deepEqual(
    (saved[0]?.metadata_json as Record<string, unknown>).strategy_payload,
    {
      schema_version: 1,
      payload_lineage: { parentArtifactId: "artifact-parent" },
      preservation_payload: { facts: ["scene fact"] },
    },
  );
});

test("compaction artifact persistence failure is degraded observability", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const recorded: Array<Record<string, unknown>> = [];
  const context = {
    bridge: {
      saveContextCompactionArtifact: async () => {
        throw new Error("database unavailable");
      },
    },
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
    now: () => "2026-08-04T00:00:00.000Z",
    recordEvent: (event: Record<string, unknown>) => recorded.push(event),
  } as unknown as ServiceWakeDispatchContext;
  const session = { sessionId: "session-1" } as SessionState;

  await appendCoreEventsToChatLog(context, session, "wake-1", [
    {
      type: "brain_event_observed",
      sessionId: session.sessionId,
      wakeId: "wake-1",
      event: {
        type: "provider_status",
        level: "info",
        message: "context compacted",
        metadataJson: JSON.stringify({
          kind: "context_compaction_completed",
          artifact: {
            sequence: 1,
            strategyId: "rolling_summary_compaction",
            reasonCode: "context_fill_threshold_exceeded",
            usageBefore: {
              inputTokens: 90,
              contextWindowTokens: 100,
              fillPercent: 90,
              source: "provider",
            },
            estimatedTokensAfter: 40,
            compactedItemCount: 4,
            retainedItemCount: 3,
            summaryText: "Earlier work was summarized.",
            providerChainAction: null,
          },
        }),
      },
    } as unknown as CoreEvent,
  ]);

  assert.equal(appended.length, 1);
  assert.equal(recorded.length, 1);
  assert.equal(
    recorded[0]?.eventType,
    "context_compaction_artifact_persist_failed",
  );
  assert.equal(recorded[0]?.severity, "warning");
});
