import type { SessionId } from "@rusty-crew/contracts";

export class WakeDispatchTimeoutError extends Error {
  constructor(
    readonly wakeId: string,
    readonly sessionId: SessionId,
    readonly timeoutMs: number,
  ) {
    super(
      `wake ${wakeId} for ${sessionId} exceeded maxTurnDurationMs ${timeoutMs}`,
    );
    this.name = "WakeDispatchTimeoutError";
  }
}

export function effectiveTurnTimeoutMs(
  value: number | undefined,
): number | undefined {
  if (value === undefined) return undefined;
  if (!Number.isFinite(value) || value <= 0) return undefined;
  return Math.floor(value);
}

export function withWakeTimeout<T>(
  promise: Promise<T>,
  input: {
    wakeId: string;
    sessionId: SessionId;
    timeoutMs?: number;
  },
): Promise<T> {
  if (input.timeoutMs === undefined) return promise;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(
        new WakeDispatchTimeoutError(
          input.wakeId,
          input.sessionId,
          input.timeoutMs!,
        ),
      );
    }, input.timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}
