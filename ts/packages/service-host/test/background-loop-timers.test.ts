import assert from "node:assert/strict";
import test from "node:test";

import { startServiceHostBackgroundLoopTimers } from "../src/background-loop-timers.js";

test("service-host owns and cancels background loop timers", async () => {
  let schedulerTicks = 0;
  let wakeDispatchTicks = 0;
  let denHeartbeatTicks = 0;
  let denDeliveryTicks = 0;
  let telegramDrainTicks = 0;

  const controller = startServiceHostBackgroundLoopTimers({
    intervals: {
      schedulerTickIntervalMs: 5,
      wakeDispatchIntervalMs: 5,
      denRuntimeHeartbeatIntervalMs: 5,
      denDeliveryPollIntervalMs: 5,
      telegramOutboundDrainIntervalMs: 5,
    },
    denGatewayAvailable: true,
    telegramConnectorAvailable: true,
    callbacks: {
      runSchedulerHeartbeat: async () => {
        schedulerTicks += 1;
      },
      recordSchedulerHeartbeatFailure: () => undefined,
      drainAndDispatchWakes: async () => {
        wakeDispatchTicks += 1;
      },
      heartbeatDenRuntimeInstances: async () => {
        denHeartbeatTicks += 1;
      },
      pollDenDeliveryIntents: async () => {
        denDeliveryTicks += 1;
      },
      drainTelegramOutboundMessages: async () => {
        telegramDrainTicks += 1;
      },
      recordFailure: () => undefined,
      errorMessage: (error, fallback) =>
        error instanceof Error ? error.message : fallback,
    },
  });

  assert.equal(controller.timerCount, 5);
  await delay(30);
  assert.ok(schedulerTicks > 0);
  assert.ok(wakeDispatchTicks > 0);
  assert.ok(denHeartbeatTicks > 0);
  assert.ok(denDeliveryTicks > 0);
  assert.equal(telegramDrainTicks, 0);

  controller.stop();
  assert.equal(controller.timerCount, 0);
  const ticksAfterStop = {
    schedulerTicks,
    wakeDispatchTicks,
    denHeartbeatTicks,
    denDeliveryTicks,
    telegramDrainTicks,
  };
  await delay(30);
  assert.deepEqual(
    {
      schedulerTicks,
      wakeDispatchTicks,
      denHeartbeatTicks,
      denDeliveryTicks,
      telegramDrainTicks,
    },
    ticksAfterStop,
  );
});

test("service-host skips disabled or unavailable background loops", () => {
  const controller = startServiceHostBackgroundLoopTimers({
    intervals: {
      schedulerTickIntervalMs: 0,
      wakeDispatchIntervalMs: 0,
      denRuntimeHeartbeatIntervalMs: 5,
      denDeliveryPollIntervalMs: 5,
    },
    denGatewayAvailable: false,
    telegramConnectorAvailable: false,
    callbacks: {
      runSchedulerHeartbeat: async () => undefined,
      recordSchedulerHeartbeatFailure: () => undefined,
      drainAndDispatchWakes: async () => undefined,
      heartbeatDenRuntimeInstances: async () => undefined,
      pollDenDeliveryIntents: async () => undefined,
      drainTelegramOutboundMessages: async () => undefined,
      recordFailure: () => undefined,
      errorMessage: (error, fallback) =>
        error instanceof Error ? error.message : fallback,
    },
  });

  assert.equal(controller.timerCount, 0);
  controller.stop();
  assert.equal(controller.timerCount, 0);
});

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
