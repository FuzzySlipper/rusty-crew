import type {
  BrainWakeRequest,
  BrainWakeStreamItem,
} from "@rusty-crew/contracts";

import type { BrainWakeExecutionResult } from "./public-api.js";

export function brainWakeStreamItemsFromExecutionResult(
  request: BrainWakeRequest,
  result: BrainWakeExecutionResult,
): BrainWakeStreamItem[] {
  if (result.stream !== undefined) {
    if (result.outcome !== "yielded") {
      assertTerminalBrainWakeStream(request, result.stream);
    }
    return result.stream;
  }

  if (result.outcome === "yielded") {
    return result.events.map(
      (event): BrainWakeStreamItem => ({ type: "event", event }),
    );
  }

  return [
    ...result.events.map(
      (event): BrainWakeStreamItem => ({ type: "event", event }),
    ),
    {
      type: "actions",
      batch: {
        wakeId: request.wakeId,
        sessionId: request.sessionId,
        actions: result.actions,
      },
    },
  ];
}

function assertTerminalBrainWakeStream(
  request: BrainWakeRequest,
  stream: readonly BrainWakeStreamItem[],
): void {
  const terminal = stream.at(-1);
  if (terminal?.type !== "actions" && terminal?.type !== "wake_failed") {
    throw new Error(
      `brain wake ${request.wakeId} stream must end with actions or wake_failed`,
    );
  }
}
