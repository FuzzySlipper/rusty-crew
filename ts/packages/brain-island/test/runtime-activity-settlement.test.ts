import assert from "node:assert/strict";
import test from "node:test";

import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { SessionId } from "@rusty-crew/contracts";

import { DurableConversationReconstructionError } from "../src/bridge-wake.js";
import { DeferredRuntimeActivitySettlementQueue } from "../src/runtime-activity-settlement.js";
import { classifyWakeDispatchFailure } from "../src/service-wake-dispatch.js";

const settlement = {
  wake: {
    wakeId: "wake-1",
    status: "failed" as const,
    reasonCode: "postgres_storage_exhausted",
    summary: "wake persistence failed",
  },
  dispatch: {
    activityId: "dispatch:wake-1",
    status: "failed" as const,
    phase: "failed",
    reasonCode: "postgres_storage_exhausted",
    summary: "wake dispatch failed",
  },
};

test("deferred settlement remains queued until both Rust tree and dispatch settle", async () => {
  const queue = new DeferredRuntimeActivitySettlementQueue();
  queue.defer(settlement);
  const calls: string[] = [];
  let storageWritable = false;
  const bridge = {
    settleRuntimeActivityWake: async () => {
      calls.push("wake");
      if (!storageWritable) throw new Error("postgres storage exhausted");
      return [];
    },
    finishRuntimeActivity: async () => {
      calls.push("dispatch");
      return settlement.dispatch;
    },
  } as unknown as Pick<
    NativeBridgeModule,
    "settleRuntimeActivityWake" | "finishRuntimeActivity"
  >;

  const failed = await queue.reconcile(bridge);
  assert.equal(queue.size, 1);
  assert.equal(failed.failure?.wakeId, "wake-1");
  assert.deepEqual(calls, ["wake"]);

  storageWritable = true;
  const recovered = await queue.reconcile(bridge);
  assert.equal(queue.size, 0);
  assert.deepEqual(recovered.reconciledWakeIds, ["wake-1"]);
  assert.deepEqual(calls, ["wake", "wake", "dispatch"]);
});

test("PostgreSQL reason marker survives generic wake dispatch classification", () => {
  assert.deepEqual(
    classifyWakeDispatchFailure(
      new Error(
        "PersistenceFailure: [postgres_storage_exhausted] insert PostgreSQL event index value: PostgreSQL SQLSTATE 53100: No space left on device",
      ),
      "session-1" as SessionId,
    ),
    {
      message:
        "PersistenceFailure: [postgres_storage_exhausted] insert PostgreSQL event index value: PostgreSQL SQLSTATE 53100: No space left on device",
      reasonCode: "postgres_storage_exhausted",
    },
  );
});

test("typed durable replay failures survive generic wake dispatch classification", () => {
  const error = new DurableConversationReconstructionError(
    "session-1" as SessionId,
    "session-1:0",
    0,
    "read_failed",
    "durable conversation read failed",
  );
  assert.deepEqual(
    classifyWakeDispatchFailure(error, "session-1" as SessionId),
    {
      message: "durable conversation read failed",
      reasonCode: "durable_conversation_reconstruction_failed",
    },
  );
});
