import assert from "node:assert/strict";

import { startServiceBackgroundLoopTimers } from "./service-background-loops.js";

const callbacks = {
  runSchedulerHeartbeat: async () => undefined,
  recordSchedulerHeartbeatFailure: () => undefined,
  drainAndDispatchWakes: async () => undefined,
  heartbeatDenRuntimeInstances: async () => undefined,
  pollDenDeliveryIntents: async () => undefined,
  drainTelegramOutboundMessages: async () => undefined,
  recordFailure: () => undefined,
  errorMessage: (error: unknown, fallback: string) =>
    error instanceof Error ? error.message : fallback,
};

const timers = new Set<NodeJS.Timeout>();
startServiceBackgroundLoopTimers({
  timers,
  intervals: {
    schedulerTickIntervalMs: 60_000,
    wakeDispatchIntervalMs: 60_000,
    denRuntimeHeartbeatIntervalMs: 60_000,
    denDeliveryPollIntervalMs: 60_000,
    telegramOutboundDrainIntervalMs: 100,
  },
  denGatewayAvailable: true,
  telegramConnectorAvailable: true,
  callbacks,
});
assert.equal(timers.size, 5);
for (const timer of timers) clearInterval(timer);

const disabledTimers = new Set<NodeJS.Timeout>();
startServiceBackgroundLoopTimers({
  timers: disabledTimers,
  intervals: {
    schedulerTickIntervalMs: 0,
    wakeDispatchIntervalMs: 0,
    denRuntimeHeartbeatIntervalMs: 60_000,
    denDeliveryPollIntervalMs: 60_000,
  },
  denGatewayAvailable: false,
  telegramConnectorAvailable: false,
  callbacks,
});
assert.equal(disabledTimers.size, 0);

console.log(
  JSON.stringify(
    {
      allEnabledTimers: timers.size,
      disabledTimers: disabledTimers.size,
    },
    null,
    2,
  ),
);
