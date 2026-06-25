import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import type {
  BrainImplementationRegistration,
  BrainWakeRequest,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";

const bridge = await loadNativeBridge();
await bridge.initializeEngine({
  engineDataDir: await mkdtemp(join(tmpdir(), "rusty-provider-state-")),
  clock: { fixed: "2026-06-24T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

const unusedHandle = await bridge.registerBrainImplementation(
  registration("unused-brain", "unused-profile", "unused"),
);
await bridge.createSession(session("unused-session", "unused-profile"));
const unusedWake = await bridge.buildBrainWakeRequestForSession({
  brain: unusedHandle,
  sessionId: "unused-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "unused-wake",
});
assert.equal(unusedWake.providerStateAbsence, "module_does_not_use_state");
assert.equal(
  (await bridge.providerStateDiagnostics()).find(
    (state) => state.sessionId === "unused-session",
  )?.status,
  "unused",
);

const optionalHandle = await bridge.registerBrainRuntime(
  registration("optional-brain", "optional-profile", "optional"),
  {
    wake(request) {
      return {
        events: [],
        actions: [],
        providerState: replaceState(request, "module-v0", { responseId: "r1" }),
      };
    },
  },
);
await bridge.createSession(session("optional-session", "optional-profile"));
const missingWake = await bridge.buildBrainWakeRequestForSession({
  brain: optionalHandle,
  sessionId: "optional-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "optional-missing",
});
assert.equal(missingWake.providerStateAbsence, "missing");
assert.equal(
  (await bridge.providerStateDiagnostics()).find(
    (state) => state.sessionId === "optional-session",
  )?.status,
  "missing",
);
await bridge.wakeBrain(missingWake);
const valid = (await bridge.providerStateDiagnostics()).find(
  (state) => state.sessionId === "optional-session",
);
assert.equal(valid?.status, "valid");
assert.equal(valid?.payloadVersion, "module-v0");
assert.equal(typeof valid?.payloadBytes, "number");
assert.equal(valid?.lastWakeId, "optional-missing");

const versionMismatchWake = await bridge.buildBrainWakeRequestForSession({
  brain: optionalHandle,
  sessionId: "optional-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "optional-version-pass-through",
});
assert.equal(versionMismatchWake.providerState?.payloadVersion, "module-v0");
assert.equal(versionMismatchWake.providerStateAbsence, undefined);

const expiredHandle = await bridge.registerBrainRuntime(
  registration("expired-brain", "expired-profile", "optional"),
  {
    wake(request) {
      return {
        events: [],
        actions: [],
        providerState: replaceState(
          request,
          "short-lived",
          { responseId: "expired" },
          0,
        ),
      };
    },
  },
);
await bridge.createSession(session("expired-session", "expired-profile"));
const expireWrite = await bridge.buildBrainWakeRequestForSession({
  brain: expiredHandle,
  sessionId: "expired-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "expired-write",
});
await bridge.wakeBrain(expireWrite);
const expiredWake = await bridge.buildBrainWakeRequestForSession({
  brain: expiredHandle,
  sessionId: "expired-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "expired-read",
});
assert.equal(expiredWake.providerState, undefined);
assert.equal(expiredWake.providerStateAbsence, "expired");
assert.equal(
  (await bridge.providerStateDiagnostics()).find(
    (state) => state.sessionId === "expired-session",
  )?.status,
  "expired",
);

const changedScopeHandle = await bridge.registerBrainImplementation(
  registration(
    "changed-scope-brain",
    "changed-scope-profile",
    "optional",
    "changed-profile-fingerprint",
  ),
);
const invalidatedWake = await bridge.buildBrainWakeRequestForSession({
  brain: changedScopeHandle,
  sessionId: "optional-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "changed-scope-read",
});
assert.equal(invalidatedWake.providerState, undefined);
assert.equal(invalidatedWake.providerStateAbsence, "invalidated");
assert.equal(
  (await bridge.providerStateDiagnostics()).find(
    (state) => state.sessionId === "optional-session",
  )?.status,
  "invalidated",
);

const clearHandle = await bridge.registerBrainRuntime(
  registration("clear-brain", "clear-profile", "optional"),
  {
    wake(request) {
      return {
        events: [],
        actions: [],
        providerState:
          request.wakeId === "clear-state"
            ? { type: "clear", reason: "brain_requested_clear" }
            : replaceState(request, "clear-v1", { responseId: "clear" }),
      };
    },
  },
);
await bridge.createSession(session("clear-session", "clear-profile"));
const clearWrite = await bridge.buildBrainWakeRequestForSession({
  brain: clearHandle,
  sessionId: "clear-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "clear-write",
});
await bridge.wakeBrain(clearWrite);
const clearRequest = await bridge.buildBrainWakeRequestForSession({
  brain: clearHandle,
  sessionId: "clear-session" as SessionId,
  systemPrompt: "system",
  roleAssemblyJson: new TextEncoder().encode("{}"),
  wakeId: "clear-state",
});
await bridge.wakeBrain(clearRequest);
const cleared = (await bridge.providerStateDiagnostics()).find(
  (state) => state.sessionId === "clear-session",
);
assert.equal(cleared?.status, "invalidated");
assert.equal(cleared?.invalidationReason, "brain_requested_clear");

const badSaveHandle = await bridge.registerBrainRuntime(
  registration("bad-save-brain", "bad-save-profile", "optional"),
  {
    wake(request) {
      return {
        events: [],
        actions: [],
        providerState: replaceState(
          request,
          "bad-v1",
          { responseId: "bad" },
          60_000,
          "wrong-profile-fingerprint",
        ),
      };
    },
  },
);
await bridge.createSession(session("bad-save-session", "bad-save-profile"));
await bridge.wakeBrain(
  await bridge.buildBrainWakeRequestForSession({
    brain: badSaveHandle,
    sessionId: "bad-save-session" as SessionId,
    systemPrompt: "system",
    roleAssemblyJson: new TextEncoder().encode("{}"),
    wakeId: "bad-save",
  }),
);
assert.equal(
  (await bridge.providerStateDiagnostics()).find(
    (state) => state.sessionId === "bad-save-session",
  )?.status,
  "save_failed",
);

const projection = buildRuntimeDiagnosticsProjection({
  now: "2026-06-24T00:00:00Z",
  sessions: [
    sessionState("load-failed-session", "load-failed-profile"),
    sessionState("unused-projection-session", "unused-projection-profile"),
  ],
  brainModules: [
    brainModule("load-failed-profile", "optional"),
    brainModule("unused-projection-profile", "unused"),
  ],
  providerStates: [
    {
      sessionId: "load-failed-session" as SessionId,
      moduleId: "openai-responses",
      strategyId: "replay",
      status: "load_failed",
      lastWakeId: "load-failed-wake",
    },
  ],
});
const projectedLoadFailure = projection.runtime.brainModules.find(
  (module) => module.profileId === "load-failed-profile",
);
const projectedUnused = projection.runtime.brainModules.find(
  (module) => module.profileId === "unused-projection-profile",
);
assert.equal(projectedLoadFailure?.providerState?.status, "load_failed");
assert.equal(projectedUnused?.providerState?.status, "unused");

console.log("provider state diagnostics smoke passed");

function registration(
  implementationId: string,
  profileId: string,
  mode: "unused" | "optional" | "required",
  profileFingerprint = "profile-fingerprint",
): BrainImplementationRegistration {
  return {
    implementationId: implementationId as never,
    profileId: profileId as ProfileId,
    toolProfile: { tools: [] },
    modelConfig: {
      provider: "openai",
      modelName: "gpt-5",
    },
    strategy: {
      moduleId: "openai-responses",
      strategyId: "replay",
      providerState: { mode },
    },
    providerStateScope:
      mode === "unused"
        ? undefined
        : {
            profileFingerprint,
            providerFingerprint: "provider-fingerprint",
          },
  };
}

function session(sessionId: string, profileId: string) {
  return {
    sessionId: sessionId as SessionId,
    agentId: `agent:${sessionId}` as never,
    profileId: profileId as ProfileId,
    kind: "full" as const,
    resourceLimits: {},
    toolProfile: { tools: [] },
  };
}

function replaceState(
  request: Pick<BrainWakeRequest, "providerState" | "wakeId">,
  payloadVersion: string,
  payload: unknown,
  ttlMs = 60_000,
  profileFingerprint = "profile-fingerprint",
) {
  return {
    type: "replace" as const,
    state: {
      moduleId: "openai-responses",
      strategyId: "replay",
      profileFingerprint,
      providerFingerprint: "provider-fingerprint",
      payloadVersion,
      payload,
      ttlMs,
    },
  };
}

function sessionState(sessionId: string, profileId: string) {
  return {
    handle: 1 as never,
    sessionId: sessionId as SessionId,
    agentId: `agent:${sessionId}` as never,
    profileId: profileId as ProfileId,
    kind: "full" as const,
    resourceLimits: {},
    toolProfile: { tools: [] },
    status: "idle" as const,
    brainTurnCount: 0,
    createdAt: "2026-06-24T00:00:00Z",
    lastActiveAt: "2026-06-24T00:00:00Z",
  };
}

function brainModule(
  profileId: string,
  providerStateMode: "unused" | "optional" | "required",
) {
  return {
    profileId: profileId as ProfileId,
    implementationId: `${profileId}-brain` as never,
    moduleId: "openai-responses",
    effectiveStrategy: "replay",
    providerStateMode,
    selectedToolCount: 0,
    selectedToolSource: "test",
    toolAdapterStatus: "native_neutral_tools",
  };
}
