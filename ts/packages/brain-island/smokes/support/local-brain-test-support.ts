import type {
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  CompletionPacket,
} from "@rusty-crew/contracts";
import type {
  BrainActionPlanner,
  BrainHostExecutor,
  BrainWakeInput,
  BrainWakeResult,
} from "../../src/brain-host-runtime.js";

export function createLocalBrain(
  planner: BrainActionPlanner = defaultActionPlanner,
): BrainHostExecutor {
  return {
    async wake(input): Promise<BrainWakeResult> {
      const events = [
        envelope(input, { type: "started" }),
        envelope(input, {
          type: "text_delta",
          text: `local brain woke ${input.state.session.agentId}`,
        }),
        envelope(input, { type: "finished" }),
      ];
      return {
        events,
        actions: await planner({ wake: input, events }),
      };
    },
  };
}

export const createPlaceholderBrain = createLocalBrain;

function defaultActionPlanner({
  wake,
}: {
  wake: BrainWakeInput;
}): BrainAction[] {
  return [
    {
      type: "deliver_completion",
      packet: {
        sessionId: wake.sessionId,
        status: "completed",
        summary: "local brain smoke wake completed",
      } satisfies CompletionPacket,
    },
  ];
}

export function envelope(
  input: BrainWakeInput,
  event: BrainEvent,
): BrainEventEnvelope {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    event,
  };
}
