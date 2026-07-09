import assert from "node:assert/strict";
import type { SessionId } from "@rusty-crew/contracts";
import { effectiveWakeTimeoutMs } from "../src/service-runtime-config.js";
import {
  effectiveTurnTimeoutMs,
  WakeDispatchTimeoutError,
  withWakeTimeout,
} from "../src/wake-timeout.js";

assert.equal(effectiveTurnTimeoutMs(undefined), undefined);
assert.equal(effectiveTurnTimeoutMs(0), undefined);
assert.equal(effectiveTurnTimeoutMs(-1), undefined);
assert.equal(effectiveTurnTimeoutMs(12.9), 12);
assert.equal(effectiveWakeTimeoutMs({ profile: {} }), undefined);
assert.equal(
  effectiveWakeTimeoutMs({
    profile: {},
    service: { mode: "disabled" },
  }),
  undefined,
);
assert.equal(
  effectiveWakeTimeoutMs({
    profile: {},
    service: { mode: "default", defaultMs: 600_000 },
  }),
  600_000,
);
assert.equal(
  effectiveWakeTimeoutMs({
    session: { turnTimeoutMs: 2_500 },
    profile: { runtime: { maxTurnDurationMs: 8_000 } },
    service: { mode: "default", defaultMs: 600_000 },
  }),
  2_500,
);
assert.equal(
  effectiveWakeTimeoutMs({
    profile: {
      runtime: { maxTurnDurationMs: 8_000 },
      sessionDefaults: { turnTimeoutMs: 4_000 },
    },
    service: { mode: "default", defaultMs: 600_000 },
  }),
  8_000,
);

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

let timeoutCallbackCalled = false;
await assert.rejects(
  () =>
    withWakeTimeout(new Promise((resolve) => setTimeout(resolve, 50)), {
      wakeId: "wake-timeout-callback",
      sessionId,
      timeoutMs: 5,
      onTimeout: () => {
        timeoutCallbackCalled = true;
      },
    }),
  WakeDispatchTimeoutError,
);
assert.equal(timeoutCallbackCalled, true);

console.log(
  JSON.stringify(
    {
      success,
      timeout: "wake_timeout",
      noImplicitWakeTimeout:
        effectiveWakeTimeoutMs({ profile: {} }) === undefined,
      configuredWakeTimeoutMs: effectiveWakeTimeoutMs({
        profile: {},
        service: { mode: "default", defaultMs: 600_000 },
      }),
      floorMs: effectiveTurnTimeoutMs(12.9),
    },
    null,
    2,
  ),
);
