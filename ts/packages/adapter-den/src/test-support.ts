import type { DenProjection, DenProjectionSink } from "./index.js";

export {
  createSimulatedDenChannelsTransport,
  type SimulatedDenChannelsTransport,
} from "./den-channel-transport.js";

export interface MemoryDenProjectionSink extends DenProjectionSink {
  readonly projections: DenProjection[];
  failNext(error?: Error): void;
}

export function createMemoryDenProjectionSink(): MemoryDenProjectionSink {
  const projections: DenProjection[] = [];
  let nextFailure: Error | undefined;

  return {
    projections,

    failNext(error = new Error("Den projection sink unavailable")): void {
      nextFailure = error;
    },

    project(projection): void {
      if (nextFailure) {
        const error = nextFailure;
        nextFailure = undefined;
        throw error;
      }

      projections.push(projection);
    },
  };
}
