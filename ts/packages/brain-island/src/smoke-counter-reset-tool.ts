import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { counterResetTool } from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-counter-reset-"));

try {
  const native = await loadNativeBridge();
  const engine = await native.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-20T08:30:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  await native.createSession({
    sessionId: "session-alpha",
    agentId: "agent-alpha",
    profileId: "prime-profile",
    kind: "full",
  });
  await native.routeAgentMessage(
    "operator",
    "agent-alpha",
    "Counter reset smoke message one.",
    "counter-smoke-1",
  );
  await native.routeAgentMessage(
    "operator",
    "agent-alpha",
    "Counter reset smoke message two.",
    "counter-smoke-2",
  );

  const readOnlyTool = counterResetTool({ client: native });
  const summary = await readOnlyTool.execute("summary", {
    action: "summary",
    scopeType: "runtime",
  });
  assert.equal(summary.details.ok, true);
  assert.equal(summary.details.summary?.messages, 2);

  const query = await readOnlyTool.execute("query", {
    action: "query",
    scopeType: "runtime",
    counterName: "messages",
  });
  assert.equal(query.details.ok, true);
  assert.equal(query.details.records?.[0]?.value, 2);

  const disabled = await readOnlyTool.execute("disabled-reset", {
    action: "reset",
    scopeType: "runtime",
    counterName: "messages",
    triggerType: "manual",
    reason: "smoke test",
    confirm: true,
  });
  assert.equal(disabled.details.ok, false);
  assert.equal(disabled.details.reasonCode, "runtime_counter_reset_disabled");

  const writableTool = counterResetTool({ client: native, allowReset: true });
  const unconfirmed = await writableTool.execute("unconfirmed-reset", {
    action: "reset",
    scopeType: "runtime",
    counterName: "messages",
    triggerType: "manual",
    reason: "smoke test",
  });
  assert.equal(unconfirmed.details.ok, false);
  assert.equal(
    unconfirmed.details.reasonCode,
    "runtime_counter_reset_confirmation_required",
  );

  const reset = await writableTool.execute("reset", {
    action: "reset",
    scopeType: "runtime",
    counterName: "messages",
    triggerType: "manual",
    reason: "smoke test",
    confirm: true,
  });
  assert.equal(reset.details.ok, true);
  assert.equal(reset.details.resetRows, 1);
  assert.equal(reset.details.records?.[0]?.value, 0);

  const agentSummary = await readOnlyTool.execute("agent-summary", {
    action: "summary",
    scopeType: "agent",
    scopeId: "agent-alpha",
  });
  assert.equal(agentSummary.details.ok, true);
  assert.equal(agentSummary.details.summary?.messages, 2);

  console.log(
    JSON.stringify(
      {
        runtimeMessagesBefore: summary.details.summary?.messages,
        runtimeMessagesAfter: reset.details.records?.[0]?.value,
        agentMessages: agentSummary.details.summary?.messages,
        disabledReason: disabled.details.reasonCode,
      },
      null,
      2,
    ),
  );

  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
} finally {
  rmSync(root, { recursive: true, force: true });
}
