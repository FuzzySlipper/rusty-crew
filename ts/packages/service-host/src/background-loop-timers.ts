import type { ServiceBackgroundLoopPort } from "@rusty-crew/brain-island";

export interface ServiceHostBackgroundLoopController {
  readonly timerCount: number;
  stop(): void;
}

export function startServiceHostBackgroundLoopTimers(
  port: ServiceBackgroundLoopPort,
): ServiceHostBackgroundLoopController {
  const timers = new Set<NodeJS.Timeout>();
  startTimers({ ...port, timers });
  return {
    get timerCount() {
      return timers.size;
    },
    stop: () => {
      for (const timer of timers) clearInterval(timer);
      timers.clear();
    },
  };
}

interface ServiceHostBackgroundLoopRuntime extends ServiceBackgroundLoopPort {
  readonly timers: Set<NodeJS.Timeout>;
}

function startTimers(runtime: ServiceHostBackgroundLoopRuntime): void {
  if (runtime.intervals.schedulerTickIntervalMs > 0) {
    const timer = setInterval(() => {
      void runtime.callbacks.runSchedulerHeartbeat().catch((error) => {
        runtime.callbacks.recordSchedulerHeartbeatFailure(error);
      });
    }, runtime.intervals.schedulerTickIntervalMs);
    runtime.timers.add(timer);
  }

  if (runtime.intervals.wakeDispatchIntervalMs > 0) {
    const timer = setInterval(() => {
      void runtime.callbacks.drainAndDispatchWakes().catch((error) =>
        runtime.callbacks.recordFailure({
          source: "service-host",
          eventType: "wake_dispatch_failed",
          severity: "error",
          summary: runtime.callbacks.errorMessage(
            error,
            "wake dispatch failed",
          ),
        }),
      );
    }, runtime.intervals.wakeDispatchIntervalMs);
    runtime.timers.add(timer);
  }

  if (
    runtime.denGatewayAvailable &&
    runtime.intervals.denRuntimeHeartbeatIntervalMs > 0
  ) {
    const timer = setInterval(() => {
      void runtime.callbacks.heartbeatDenRuntimeInstances().catch((error) =>
        runtime.callbacks.recordFailure({
          source: "den-successor-gateway",
          eventType: "den_runtime_heartbeat_failed",
          severity: "error",
          summary: runtime.callbacks.errorMessage(
            error,
            "Den Runtime heartbeat failed",
          ),
        }),
      );
    }, runtime.intervals.denRuntimeHeartbeatIntervalMs);
    runtime.timers.add(timer);
  }

  if (
    runtime.denGatewayAvailable &&
    runtime.intervals.denDeliveryPollIntervalMs > 0
  ) {
    const timer = setInterval(() => {
      void runtime.callbacks.pollDenDeliveryIntents().catch((error) =>
        runtime.callbacks.recordFailure({
          source: "den-successor-gateway",
          eventType: "den_delivery_poll_failed",
          severity: "error",
          summary: runtime.callbacks.errorMessage(
            error,
            "Den Delivery poll failed",
          ),
        }),
      );
    }, runtime.intervals.denDeliveryPollIntervalMs);
    runtime.timers.add(timer);
  }

  if (
    runtime.telegramConnectorAvailable &&
    runtime.intervals.telegramOutboundDrainIntervalMs !== undefined
  ) {
    const timer = setInterval(
      () => {
        void runtime.callbacks.drainTelegramOutboundMessages().catch((error) =>
          runtime.callbacks.recordFailure({
            source: "telegram",
            eventType: "telegram_outbound_drain_failed",
            severity: "error",
            summary: runtime.callbacks.errorMessage(
              error,
              "Telegram outbound drain failed",
            ),
          }),
        );
      },
      Math.max(250, runtime.intervals.telegramOutboundDrainIntervalMs),
    );
    runtime.timers.add(timer);
  }
}
