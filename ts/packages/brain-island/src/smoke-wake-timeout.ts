import assert from "node:assert/strict";
import type { SessionId } from "@rusty-crew/contracts";
import {
  effectiveTurnTimeoutMs,
  WakeDispatchTimeoutError,
  withWakeTimeout,
} from "./wake-timeout.js";

assert.equal(effectiveTurnTimeoutMs(undefined), undefined);
assert.equal(effectiveTurnTimeoutMs(0), undefined);
assert.equal(effectiveTurnTimeoutMs(-1), undefined);
assert.equal(effectiveTurnTimeoutMs(12.9), 12);

const sessionId = "timeout-session" as SessionId;
const success = await withWakeTimeout(Promise.resolve("ok"), {
  wakeId: "wake-success",
  sessionId,
  timeoutMs: 100,
});
assert.equal(success, "ok");

await assert.rejects(
  () =>
    withWakeTimeout(new Promise((resolve) => setTimeout(resolve, 50)), {
      wakeId: "wake-timeout",
      sessionId,
      timeoutMs: 5,
    }),
  (error: unknown) => {
    assert.equal(error instanceof WakeDispatchTimeoutError, true);
    const timeout = error as WakeDispatchTimeoutError;
    assert.equal(timeout.wakeId, "wake-timeout");
    assert.equal(timeout.sessionId, sessionId);
    assert.equal(timeout.timeoutMs, 5);
    return true;
  },
);

console.log(
  JSON.stringify(
    {
      success,
      timeout: "wake_timeout",
      floorMs: effectiveTurnTimeoutMs(12.9),
    },
    null,
    2,
  ),
);
