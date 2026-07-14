import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { loadNativeBridge } from "./index.js";

const dataDir = mkdtempSync(join(tmpdir(), "rusty-crew-nullability-"));
const now = new Date().toISOString();
const oneHourLater = new Date(Date.now() + 60 * 60 * 1_000).toISOString();
const runtimeId = "nullability-runtime";
const bindingId = "nullability-binding";
const sessionId = "nullability-session";
const agentId = "nullability-agent";

const bridge = await loadNativeBridge();
const engine = await bridge.initializeEngine({
  engineDataDir: dataDir,
  clock: "system",
  defaultTurnBudget: 8,
  defaultIdleTimeoutMs: 30_000,
  storage: { backend: "sqlite" },
});

try {
  await bridge.registerExternalRuntime({
    registration: {
      runtimeId,
      kind: "codex_app_server",
      endpoint: {
        transport: "unix_web_socket",
        address: "/run/user/1001/codex-app-server/app-server.sock",
      },
      processOwnership: "attached",
      observedCliVersion: "0.144.3",
      consumedContractRevision: "contract-v1",
      compatibilityState: "compatible_uncertified",
      lastCompatibilityProbe: {
        suiteRevision: "codex-required-capabilities-v1",
        outcome: "passed",
        steps: [
          {
            stepId: "model_list",
            status: "passed",
            durationMs: 1,
          },
        ],
        completedAt: now,
      },
      desiredState: "enabled",
      observedState: "ready",
      revision: 0,
      createdAt: now,
      updatedAt: now,
    },
  });
  await bridge.ensureConfiguredSession({
    sessionId,
    agentId,
    profileId: "nullability-profile",
    kind: "full",
    resourceLimits: { workdir: dataDir },
    toolProfile: { tools: [] },
  });

  const unassignedBinding = await bridge.bindExternalAgent({
    binding: {
      bindingId,
      runtimeId,
      sessionId,
      agentId,
      purpose: "crew_agent",
      effectiveConfigFingerprint: "nullability-v1",
      status: "active",
      revision: 0,
      createdAt: now,
      updatedAt: now,
    },
  });
  assert.equal(unassignedBinding.nativeThreadId, null);
  assert.equal(unassignedBinding.cwd, null);
  assert.equal(unassignedBinding.taskRef, null);
  assert(Object.hasOwn(unassignedBinding, "nativeThreadId"));

  const assignedBinding = await bridge.bindExternalAgent({
    binding: {
      ...unassignedBinding,
      nativeThreadId: "native-thread-nullability",
      cwd: dataDir,
    },
    expectedRevision: unassignedBinding.revision,
  });
  assert.equal(assignedBinding.nativeThreadId, "native-thread-nullability");

  const lease = await bridge.acquireExternalController({
    lease: {
      runtimeId,
      holderInstanceId: "nullability-controller",
      generation: 0,
      acquiredAt: now,
      renewedAt: now,
      expiresAt: oneHourLater,
      revision: 0,
    },
    now,
  });
  const delivery = await bridge.deliverAgentMessage({
    caller: { type: "system", senderAgentId: "nullability-sender" },
    deliveryId: "nullability-delivery",
    idempotencyKey: "nullability-delivery",
    messageId: "nullability-message",
    toAgentId: agentId,
    body: "Exercise external turn nullability.",
    requireWake: true,
    createdAt: now,
    expiresAt: oneHourLater,
  });
  assert.equal(delivery.activation?.type, "external_turn_requested");
  assert(delivery.activation?.type === "external_turn_requested");

  const unassignedTurn = await bridge.getExternalTurn(
    delivery.activation.requestId,
  );
  assert(unassignedTurn !== undefined);
  assert.equal(unassignedTurn.nativeTurnId, null);
  assert.equal(unassignedTurn.taskRef, null);
  assert.equal(unassignedTurn.terminalReasonCode, null);
  assert(Object.hasOwn(unassignedTurn, "nativeTurnId"));

  const assignedTurn = await bridge.transitionExternalTurn({
    controller: {
      holderInstanceId: lease.holderInstanceId,
      generation: lease.generation,
    },
    requestId: delivery.activation.requestId,
    nextPhase: "starting",
    nativeTurnId: "native-turn-nullability",
    now: new Date().toISOString(),
  });
  assert.equal(assignedTurn.nativeTurnId, "native-turn-nullability");

  console.log(
    JSON.stringify({
      bindingNullBeforeAssignment: true,
      bindingStringAfterAssignment: true,
      turnNullBeforeAssignment: true,
      turnStringAfterAssignment: true,
      nativeAddonSerialization: true,
    }),
  );
} finally {
  await bridge.shutdownEngine({ engine, drainTimeoutMs: 5_000 });
  rmSync(dataDir, { recursive: true, force: true });
}
