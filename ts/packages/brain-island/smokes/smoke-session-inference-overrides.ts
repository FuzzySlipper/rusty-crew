import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-session-effort-"));
const native = await loadNativeBridge();
const sessionId = "effort-session" as SessionId;

try {
  const engine = await start("2026-07-16T00:00:00Z");
  await native.createSession({
    sessionId,
    agentId: "effort-agent" as AgentId,
    profileId: "effort-profile" as ProfileId,
    kind: "full",
  });
  const updated = await native.setSessionReasoningEffort(sessionId, "high");
  assert.equal(updated.reasoningEffort, "high");
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });

  const restarted = await start("2026-07-16T00:01:00Z");
  const restored = await session();
  assert.equal(restored?.inferenceOverrides?.reasoningEffort, "high");
  const concurrent = await Promise.all([
    native.setSessionReasoningEffort(sessionId, "low"),
    native.setSessionReasoningEffort(sessionId, "medium"),
  ]);
  assert.deepEqual(
    concurrent.map((candidate) => candidate.reasoningEffort).sort(),
    ["low", "medium"],
  );
  const concurrentRead = await session();
  assert.ok(
    concurrentRead?.inferenceOverrides?.reasoningEffort === "low" ||
      concurrentRead?.inferenceOverrides?.reasoningEffort === "medium",
    `concurrent update was not persisted: ${JSON.stringify(concurrentRead)}`,
  );
  const cleared = await native.setSessionReasoningEffort(sessionId, undefined);
  assert.equal(cleared.reasoningEffort, undefined);
  await native.shutdownEngine({ engine: restarted, drainTimeoutMs: 1_000 });

  const clearedRestart = await start("2026-07-16T00:02:00Z");
  const restoredClear = await session();
  assert.equal(restoredClear?.inferenceOverrides?.reasoningEffort, undefined);
  console.log(
    JSON.stringify(
      {
        sessionId,
        restoredEffort: restored?.inferenceOverrides?.reasoningEffort,
        concurrentEfforts: concurrent.map(
          (candidate) => candidate.reasoningEffort,
        ),
        clearedEffort:
          restoredClear?.inferenceOverrides?.reasoningEffort ?? null,
      },
      null,
      2,
    ),
  );
  await native.shutdownEngine({
    engine: clearedRestart,
    drainTimeoutMs: 1_000,
  });
} finally {
  rmSync(root, { force: true, recursive: true });
}

function start(fixed: string) {
  return native.initializeEngine({
    engineDataDir: root,
    clock: { fixed },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
}

async function session() {
  return (await native.listSessions()).find(
    (candidate) => candidate.sessionId === sessionId,
  );
}
