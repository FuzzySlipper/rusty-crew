export interface ServiceBackgroundLoopIntervals {
  readonly schedulerTickIntervalMs: number;
  readonly wakeDispatchIntervalMs: number;
  readonly denRuntimeHeartbeatIntervalMs: number;
  readonly denDeliveryPollIntervalMs: number;
  readonly telegramOutboundDrainIntervalMs?: number;
  readonly externalRuntimeControllerTickIntervalMs: number;
}

export interface ServiceBackgroundLoopFailure {
  source: string;
  eventType: string;
  summary: string;
  severity: "error";
}

export interface ServiceBackgroundLoopCallbacks {
  runSchedulerHeartbeat(): Promise<unknown>;
  recordSchedulerHeartbeatFailure(error: unknown): void;
  drainAndDispatchWakes(): Promise<unknown>;
  heartbeatDenRuntimeInstances(): Promise<unknown>;
  pollDenDeliveryIntents(): Promise<unknown>;
  drainTelegramOutboundMessages(): Promise<unknown>;
  tickExternalRuntimeController(): Promise<unknown>;
  recordFailure(failure: ServiceBackgroundLoopFailure): void;
  errorMessage(error: unknown, fallback: string): string;
}

export interface ServiceBackgroundLoopPort {
  readonly intervals: ServiceBackgroundLoopIntervals;
  readonly denGatewayAvailable: boolean;
  readonly telegramConnectorAvailable: boolean;
  readonly callbacks: ServiceBackgroundLoopCallbacks;
}

export interface ServiceBackgroundLoopRuntime extends ServiceBackgroundLoopPort {
  readonly timers: Set<NodeJS.Timeout>;
}
