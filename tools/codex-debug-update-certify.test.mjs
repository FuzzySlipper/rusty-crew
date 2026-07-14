import assert from "node:assert/strict";
import test from "node:test";

import {
  assertDebugRuntimeTarget,
  certificationIdentity,
  newThreadIds,
  parseOperatorArgs,
} from "./codex-debug-update-certify.mjs";

test("requires an explicit update decision", () => {
  assert.deepEqual(parseOperatorArgs(["--update"]), {
    help: false,
    updateMode: "update",
  });
  assert.deepEqual(parseOperatorArgs(["--skip-update"]), {
    help: false,
    updateMode: "explicit_skip",
  });
  assert.throws(() => parseOperatorArgs([]), /choose exactly one/);
  assert.throws(
    () => parseOperatorArgs(["--update", "--skip-update"]),
    /choose exactly one/,
  );
});

test("certification identity is stable for one exact runtime identity", () => {
  const runtime = {
    runtimeId: "debug-codex",
    observedCliVersion: "0.144.3",
    consumedContractRevision: "2026-07-13",
    probeSuiteRevision: "probe-v1",
  };
  assert.deepEqual(
    certificationIdentity(runtime),
    certificationIdentity(runtime),
  );
  assert.notDeepEqual(
    certificationIdentity(runtime),
    certificationIdentity({ ...runtime, observedCliVersion: "0.145.0" }),
  );
});

test("cleanup selects only threads introduced by the certification run", () => {
  assert.deepEqual(
    newThreadIds(["old-a", "old-b"], ["new-b", "old-b", "new-a", "old-a"]),
    ["new-a", "new-b"],
  );
});

test("operator target cannot be redirected to live Crew", () => {
  assert.doesNotThrow(() =>
    assertDebugRuntimeTarget(
      "http://127.0.0.1:9348",
      "rusty-crew-debug.service",
      "/run/user/1001/codex-app-server/app-server.sock",
    ),
  );
  assert.throws(
    () =>
      assertDebugRuntimeTarget(
        "http://127.0.0.1:9347",
        "rusty-crew.service",
        "/run/user/1001/codex-app-server-live/app-server.sock",
      ),
    /restricted to debug Crew/,
  );
});
