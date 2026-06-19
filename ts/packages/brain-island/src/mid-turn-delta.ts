import type {
  AgentMessage,
  BodyDeltaPolicy,
  SessionId,
} from "@rusty-crew/contracts";

export interface QueuedMidTurnMessage {
  sessionId: SessionId;
  activeWakeId: string;
  message: AgentMessage;
  enqueuedAtMs: number;
  expiresAtMs: number;
}

export interface DrainResult {
  messages: AgentMessage[];
  droppedExpired: number;
}

export class BodyControlledDeltaQueue {
  private readonly entries: QueuedMidTurnMessage[] = [];

  constructor(private readonly policy: BodyDeltaPolicy) {
    if (policy.mode !== "frozen_snapshot_next_wake") {
      throw new Error(`unsupported mid-turn delta mode ${policy.mode}`);
    }
    if (policy.queueOwner !== "body") {
      throw new Error(`unsupported delta queue owner ${policy.queueOwner}`);
    }
  }

  enqueue(input: {
    sessionId: SessionId;
    activeWakeId: string;
    message: AgentMessage;
    nowMs: number;
  }): QueuedMidTurnMessage {
    this.dropExpired(input.nowMs);

    const entry = {
      sessionId: input.sessionId,
      activeWakeId: input.activeWakeId,
      message: input.message,
      enqueuedAtMs: input.nowMs,
      expiresAtMs: input.nowMs + this.policy.queuedMessageTtlMs,
    };
    this.entries.push(entry);
    this.enforceCap();
    return entry;
  }

  drainForNextWake(sessionId: SessionId, nowMs: number): DrainResult {
    let droppedExpired = 0;
    const messages: AgentMessage[] = [];

    for (let index = this.entries.length - 1; index >= 0; index -= 1) {
      const entry = this.entries[index];
      if (entry.sessionId !== sessionId) {
        continue;
      }

      this.entries.splice(index, 1);
      if (entry.expiresAtMs <= nowMs) {
        droppedExpired += 1;
        continue;
      }

      messages.unshift(entry.message);
    }

    return { messages, droppedExpired };
  }

  clear(): void {
    this.entries.splice(0);
  }

  size(): number {
    return this.entries.length;
  }

  private dropExpired(nowMs: number): void {
    for (let index = this.entries.length - 1; index >= 0; index -= 1) {
      if (this.entries[index].expiresAtMs <= nowMs) {
        this.entries.splice(index, 1);
      }
    }
  }

  private enforceCap(): void {
    const maxQueuedMessages = this.policy.maxQueuedMessages;
    while (this.entries.length > maxQueuedMessages) {
      this.entries.shift();
    }
  }
}

export const defaultBodyDeltaPolicy: BodyDeltaPolicy = {
  mode: "frozen_snapshot_next_wake",
  queueOwner: "body",
  queuedMessageTtlMs: 5_000,
  maxQueuedMessages: 32,
};
