import assert from "node:assert/strict";
import test from "node:test";

import type { AdminControlCommand } from "../src/admin-control-api.js";
import {
  applyServiceRuntimeRebuild,
  planServiceRuntimeRebuild,
  type ServiceRuntimeRebuildContext,
} from "../src/service-runtime-rebuild.js";
import type { RustyCrewRuntimeConfigApplyResult } from "../src/service-runtime-config.js";

test("ordinary runtime rebuild reconstructs provider state without clearing it", async () => {
  const harness = rebuildHarness();
  const result = await applyServiceRuntimeRebuild(
    harness.context,
    rebuildCommand(),
  );

  assert.equal(result.apply.status, "completed");
  assert.equal(result.providerState.transition, "reconstructed");
  assert.equal(result.providerState.outcome, "reconstructed");
  assert.equal(result.providerState.clearedSessions, 0);
  assert.deepEqual(harness.clearCalls, []);
  assert.equal(harness.rebuildCalls(), 1);
  assert.deepEqual(harness.durableTransitions, [
    {
      sessionId: "session-alpha",
      transition: {
        transitionId: "runtime-rebuild-test-request",
        profileId: "profile-alpha",
        sessionId: "session-alpha",
        outcome: "reconstructed",
        action: "reconstruct",
        transition: "reconstructed",
        clearedSessions: 0,
        reason: "rebuild from durable session projection",
      },
    },
  ]);
  assert.deepEqual(
    harness.events.find(
      (event) =>
        event.eventType === "runtime_rebuild_provider_state_reconstructed",
    )?.resultRef,
    {
      outcome: "reconstructed",
      action: "reconstruct",
      transition: "reconstructed",
      clearedSessions: 0,
    },
  );
});

test("runtime rebuild records failed recovery instead of hiding the failure", async () => {
  const harness = rebuildHarness({
    rebuildError: new Error("brain replacement failed"),
  });

  await assert.rejects(
    applyServiceRuntimeRebuild(harness.context, rebuildCommand()),
    /brain replacement failed/,
  );
  assert.deepEqual(harness.clearCalls, []);
  assert.equal(
    harness.durableTransitions[0]?.transition.outcome,
    "failed_recovery",
  );
  assert.deepEqual(
    harness.events.find(
      (event) =>
        event.eventType === "runtime_rebuild_provider_state_recovery_failed",
    )?.resultRef,
    {
      outcome: "failed_recovery",
      action: "reconstruct",
      transition: "reconstructed",
    },
  );
});

test("replacement planning marks context loss as an explicit reset", async () => {
  const harness = rebuildHarness();
  const plan = await planServiceRuntimeRebuild(
    harness.context,
    rebuildCommand({
      target: { scope: "session", sessionId: "session-alpha" },
      body: { sessionIdentity: "replace" },
    }),
  );

  assert.equal(plan.preservesSessionId, false);
  assert.equal(plan.preservesHistory, false);
  assert.equal(plan.providerState.transition, "explicit_reset");
});

function rebuildCommand(
  overrides: Partial<AdminControlCommand> = {},
): AdminControlCommand {
  return {
    name: "apply_runtime_rebuild",
    target: { scope: "profile", profileId: "profile-alpha" },
    actor: { operatorId: "runtime-rebuild-test" },
    requestId: "runtime-rebuild-test-request",
    body: {},
    ...overrides,
  };
}

function rebuildHarness(options: { rebuildError?: Error } = {}): {
  context: ServiceRuntimeRebuildContext;
  clearCalls: string[];
  events: Array<{
    eventType: string;
    resultRef?: Record<string, unknown>;
  }>;
  durableTransitions: Array<{
    sessionId: string;
    transition: Record<string, unknown>;
  }>;
  rebuildCalls: () => number;
} {
  const clearCalls: string[] = [];
  const events: Array<{
    eventType: string;
    resultRef?: Record<string, unknown>;
  }> = [];
  const durableTransitions: Array<{
    sessionId: string;
    transition: Record<string, unknown>;
  }> = [];
  let rebuildCallCount = 0;
  const runtimeConfigApplyResult = {
    brainHandlesByProfileId: { "profile-alpha": "brain-old" },
    brainModulesByProfileId: {
      "profile-alpha": { moduleId: "chat-completions", strategy: "default" },
    },
    brainDiagnosticsByProfileId: {
      "profile-alpha": {
        providerStateMode: "optional",
        providerStateRebuild: {
          action: "reconstruct",
          reason: "rebuild from durable session projection",
        },
      },
    },
  } as unknown as RustyCrewRuntimeConfigApplyResult;
  const context = {
    bridge: {
      async listSessions() {
        return [{ sessionId: "session-alpha", profileId: "profile-alpha" }];
      },
      async clearBrainProviderState(input: { sessionId: string }) {
        clearCalls.push(input.sessionId);
        throw new Error("ordinary rebuild must not clear provider state");
      },
    },
    runtimeConfig: {
      brains: [{ profileId: "profile-alpha" }],
      sessions: [{ sessionId: "session-alpha", profileId: "profile-alpha" }],
      channelBindings: [],
      mcpBindings: [],
    },
    runtimeConfigApplyResult,
    inFlightWakes: new Set(),
    now: () => "2026-08-04T00:00:00.000Z",
    nextReplacementSessionId: () => "session-alpha-replacement",
    readRuntimeConfigFile: async () => ({ array: () => [] }),
    validateRuntimeConfigFile: async () => ({}) as never,
    writeRuntimeConfigFile: async () => undefined,
    serviceSessionById: async () => ({}) as never,
    archiveSession: async () => undefined,
    applyRuntimeConfigFromDisk: async () => runtimeConfigApplyResult,
    rebuildBrainRuntime: async () => {
      rebuildCallCount += 1;
      if (options.rebuildError !== undefined) throw options.rebuildError;
      return {
        profileId: "profile-alpha",
        implementationId: "implementation-next",
        handle: "brain-next",
        module: { moduleId: "chat-completions", strategy: "default" },
        diagnostics:
          runtimeConfigApplyResult.brainDiagnosticsByProfileId["profile-alpha"],
      };
    },
    refreshMcpBindingsAfterRuntimeRebuild: async () => ({
      action: "refresh_after_rebuild",
      bindingIds: [],
      refreshedBindingIds: [],
      degradedBindingIds: [],
      missingBindingIds: [],
      results: [],
    }),
    recordEvent: (event: {
      eventType: string;
      resultRef?: Record<string, unknown>;
    }) => events.push(event),
    recordDurableTransition: async (
      sessionId: string,
      transition: Record<string, unknown>,
    ) => {
      durableTransitions.push({ sessionId, transition });
    },
  } as unknown as ServiceRuntimeRebuildContext;

  return {
    context,
    clearCalls,
    events,
    durableTransitions,
    rebuildCalls: () => rebuildCallCount,
  };
}
