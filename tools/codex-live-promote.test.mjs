import assert from "node:assert/strict";
import test from "node:test";

import {
  compareBindingSnapshots,
  compareTurnSnapshots,
  parsePromotionArgs,
  promotionBlockers,
  selectCurrentCertification,
} from "./codex-live-promote.mjs";

test("live promotion requires an explicit operator command", () => {
  assert.throws(() => parsePromotionArgs([]), /requires --promote/);
  assert.deepEqual(parsePromotionArgs(["--promote"]), {
    help: false,
    overrideActive: false,
  });
  assert.deepEqual(parsePromotionArgs(["--promote", "--override-active"]), {
    help: false,
    overrideActive: true,
  });
});

test("certification selection requires the complete current identity", () => {
  const current = {
    observedCliVersion: "0.144.3",
    consumedContractRevision: "contract-a",
    probeSuiteRevision: "probe-a",
  };
  const matching = {
    certificationId: "match",
    status: "active",
    runtimeKind: "codex_app_server",
    ...current,
  };
  assert.equal(
    selectCurrentCertification(
      [
        { ...matching, certificationId: "old", observedCliVersion: "0.144.2" },
        matching,
      ],
      current,
    )?.certificationId,
    "match",
  );
  assert.equal(
    selectCurrentCertification(
      [{ ...matching, status: "invalidated" }],
      current,
    ),
    undefined,
  );
});

test("promotion blockers expose active turns and unresolved interactions", () => {
  assert.deepEqual(
    promotionBlockers({
      activeTurns: [{ request: { requestId: "turn-1" } }],
      pendingInteractions: [{ interactionId: "interaction-1" }],
    }),
    { activeTurnIds: ["turn-1"], interactionIds: ["interaction-1"] },
  );
});

test("snapshot comparisons reject replacement bindings and replayed turns", () => {
  const before = [{ bindingId: "binding-1", nativeThreadId: "thread-1" }];
  assert.deepEqual(compareBindingSnapshots(before, before), []);
  assert.deepEqual(
    compareBindingSnapshots(before, [
      { bindingId: "binding-1", nativeThreadId: "thread-2" },
    ])[0]?.reason,
    "native_thread_replaced",
  );
  assert.deepEqual(
    compareTurnSnapshots(
      { "binding-1": ["turn-1"] },
      { "binding-1": ["turn-1"] },
    ),
    [],
  );
  assert.equal(
    compareTurnSnapshots(
      { "binding-1": ["turn-1"] },
      { "binding-1": ["turn-1", "turn-2"] },
    ).length,
    1,
  );
  assert.equal(compareTurnSnapshots({ "binding-1": ["turn-1"] }, {}).length, 1);
});
