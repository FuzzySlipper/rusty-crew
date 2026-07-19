import type { ProfileId, SessionId } from "@rusty-crew/contracts";
import type { BrainWakeExecutor } from "@rusty-crew/native-bridge";
import { BufferedBrainWakeError } from "./buffered-brain-host.js";
import { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
import type { BrainHostExecutor, BrainWakeResult } from "./index.js";

export interface ServiceBrainWakeResultObservation {
  profileId: ProfileId;
  sessionId: SessionId;
  wakeId: string;
  result: BrainWakeResult;
}

export function createServiceBrainWakeExecutor(
  brain: BrainHostExecutor,
  options: {
    profileId?: ProfileId;
    onBrainWakeResult?: (
      observation: ServiceBrainWakeResultObservation,
    ) => void;
  } = {},
): BrainWakeExecutor {
  return {
    async wake(request, buffers, wakeOptions) {
      let result: BrainWakeResult;
      try {
        result = await wakeBrainFromBridgeRequest(
          buffers,
          brain,
          request,
          wakeOptions,
        );
      } catch (error) {
        if (
          options.profileId !== undefined &&
          error instanceof BufferedBrainWakeError &&
          error.transportMetrics !== undefined
        ) {
          options.onBrainWakeResult?.({
            profileId: options.profileId,
            sessionId: request.sessionId,
            wakeId: request.wakeId,
            result: {
              events: [],
              actions: [],
              transportMetrics: error.transportMetrics,
              brainEventCounts: error.brainEventCounts,
              brainStreamItemCounts: error.brainStreamItemCounts,
              streamRetentionMetrics: error.streamRetentionMetrics,
            },
          });
        }
        throw error;
      }

      if (
        options.profileId !== undefined &&
        result.transportMetrics !== undefined
      ) {
        options.onBrainWakeResult?.({
          profileId: options.profileId,
          sessionId: request.sessionId,
          wakeId: request.wakeId,
          result,
        });
      }
      return result;
    },
  };
}
