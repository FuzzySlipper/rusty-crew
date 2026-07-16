import assert from "node:assert/strict";

import { BridgeValidationError } from "./bridge-validation.js";
import {
  directBridgeValidatedOperations,
  withDirectBridgeOutputValidation,
} from "./direct-binding-validation.js";

const env = { RUSTY_CREW_BRIDGE_VALIDATE: "1" };
const session = {
  handle: 4,
  sessionId: "session-1",
  agentId: "agent-1",
  profileId: "profile-1",
  kind: "delegated",
  status: "active",
  resourceLimits: {
    workdir: "/home/dev/rusty-crew",
    maxDurationMs: 120_000,
    maxDelegationDepth: 0,
  },
  toolProfile: {
    tools: [
      {
        name: "shell",
        description: "Run a shell command.",
        inputSchema: 1,
      },
    ],
  },
};
const validBinding = withDirectBridgeOutputValidation(
  {
    initializeEngine: () => 1,
    shutdownEngine: () => ({
      archivedSessions: 1,
      droppedSubscriptions: 2,
    }),
    registerBrainImplementation: () => 2,
    replaceBrainImplementation: () => 3,
    unregisterBrainImplementationForProfile: () => 3,
    registerPlatformAdapter: () => 5,
    injectExternalEvent: () => ({ accepted: true, sequence: 6 }),
    injectDenDataUpdate: () => ({ accepted: true, sequence: 7 }),
    enqueueBodyFollowUpMessage: () => ({
      messageId: "message-1",
      ownerSessionId: "session-1",
      ownerAgentId: "agent-1",
      fromAgent: "agent-1",
      toAgent: "agent-2",
      body: "checkpoint",
      correlationId: "correlation-1",
      enqueuedAt: "2026-07-10T00:00:00Z",
      expiresAt: "2026-07-10T00:01:00Z",
      ttlMs: 60_000,
      deliveryAttempts: 0,
      state: "pending",
    }),
    archiveSession: () => ({ ...session, status: "archived" }),
    ensureConfiguredSession: () => session,
    cancelDelegatedSession: () => ({ ...session, status: "archived" }),
    requestDelegatedCheckpoint: () => ({ accepted: true, sequence: 8 }),
    drainDelegatedSessions: () => ["session-1"],
    cleanupDelegatedResourcesJson: () =>
      JSON.stringify({
        cleaned_at: "2026-07-10T00:00:00Z",
        terminal_archived: ["session-1"],
        orphaned_archived: [],
        expired_archived: [],
        resources_released: 1,
      }),
    delegatedSessionStatusJson: () =>
      JSON.stringify({
        session: {
          handle: 4,
          session_id: "session-1",
          agent_id: "agent-1",
          profile_id: "profile-1",
          kind: "delegated",
          delegation: null,
          resource_limits: {
            workdir: null,
            max_duration_ms: null,
            max_delegation_depth: null,
          },
          tool_profile: { tools: [] },
          history_window: null,
          status: "active",
          brain_turn_count: 1,
          created_at: "2026-07-10T00:00:00Z",
          last_active_at: "2026-07-10T00:00:00Z",
        },
        parent_session_id: "parent-1",
        run_id: "run-1",
        run_status: "running",
        terminal: false,
      }),
    exchangeOpenaiOauthCodeJson: async () =>
      JSON.stringify({
        ok: true,
        secret: "synthetic-secret-not-a-credential",
        summary: {
          kind: "openai_oauth",
          version: 1,
          has_secret: true,
          account_id: "synthetic-account",
          email: null,
          plan_type: null,
          is_fedramp_account: false,
          access_token_expires_at: null,
        },
      }),
    bufferedBrainRunDiagnosticsJson: () =>
      JSON.stringify({
        active_run_count: 0,
        modules: [{ module_label: "chat-completions", active_run_count: 0 }],
        runs: [],
      }),
    cleanupBufferedBrainRunsJson: () =>
      JSON.stringify({
        active_runs: 0,
        terminal_runs: 0,
        cancelled_nonterminal_runs: 0,
        removed_runs: 0,
        modules: [
          {
            module_label: "chat-completions",
            active_runs: 0,
            terminal_runs: 0,
            cancelled_nonterminal_runs: 0,
            removed_runs: 0,
          },
        ],
      }),
    getModelProviderSecretJson: () =>
      JSON.stringify("synthetic-secret-not-a-credential"),
    suspendForGithubGateJson: () => JSON.stringify(githubWait()),
    consumeGithubGateTerminalEventJson: () =>
      JSON.stringify({
        event_id: 9,
        cursor: 9,
        duplicate: false,
        wake_scheduled: true,
        ignored_reason: null,
        wait: githubWait(),
      }),
    recoverGithubGateWakes: () => 1,
    githubGateWaitJson: () => JSON.stringify(githubWait()),
    githubGateEventCursor: () => 9,
    subscribeEvents: () => 10,
    getBuffer: () => ({
      handle: 11,
      mediaType: "application/json",
      byteLen: 0,
      bytes: new Uint8Array(),
    }),
  },
  env,
);

for (const method of Object.keys(validBinding) as Array<
  keyof typeof validBinding
>) {
  await Promise.resolve(Reflect.apply(validBinding[method], validBinding, []));
}

const wrongCase = withDirectBridgeOutputValidation(
  {
    shutdownEngine: () => ({
      archived_sessions: 1,
      dropped_subscriptions: 2,
    }),
  },
  env,
);
assert.throws(() => wrongCase.shutdownEngine(), BridgeValidationError);

const extraField = withDirectBridgeOutputValidation(
  {
    injectExternalEvent: () => ({
      accepted: true,
      sequence: 1,
      untracked: true,
    }),
  },
  env,
);
assert.throws(() => extraField.injectExternalEvent(), BridgeValidationError);

const invalidBuffer = withDirectBridgeOutputValidation(
  {
    getBuffer: () => ({
      handle: 1,
      mediaType: "application/octet-stream",
      byteLen: 1,
      bytes: new Uint8Array(),
    }),
  },
  env,
);
assert.throws(() => invalidBuffer.getBuffer(), /does not match bytes length/);

assert.equal(new Set(directBridgeValidatedOperations).size, 27);
console.log(
  JSON.stringify({
    directOperationsValidated: directBridgeValidatedOperations.length,
    camelCaseChokepoint: true,
    strictAdditionalProperties: true,
    jsonTextValidation: true,
    syntheticSecretSamples: true,
    secretPayloadsLogged: false,
    binaryPayloadCopiedToFixture: false,
  }),
);

function githubWait() {
  return {
    session_id: "session-1",
    run_id: "run-1",
    provider_thread_id: null,
    project_id: "rusty-crew",
    task_id: "5565",
    gate_id: 9,
    commit_sha: "synthetic-commit-sha",
    phase: "waiting",
    terminal_event_id: null,
    created_at: "2026-07-10T00:00:00Z",
    updated_at: "2026-07-10T00:00:00Z",
  };
}
