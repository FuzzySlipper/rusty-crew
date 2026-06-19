import type { BrainAction, BrainEvent, SessionId } from "@rusty-crew/contracts";

export interface BrainWakeInput {
  sessionId: SessionId;
  stateSnapshotJson: string;
  systemPrompt: string;
}

export interface BrainWakeResult {
  events: BrainEvent[];
  actions: BrainAction[];
}

export interface BrainImplementation {
  wake(input: BrainWakeInput): Promise<BrainWakeResult>;
}

export function createPlaceholderBrain(): BrainImplementation {
  return {
    async wake(): Promise<BrainWakeResult> {
      return {
        events: [{ type: "started" }, { type: "finished" }],
        actions: [],
      };
    },
  };
}
