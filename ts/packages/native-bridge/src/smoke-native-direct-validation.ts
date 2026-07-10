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
  },
  env,
);

for (const method of Object.keys(validBinding) as Array<
  keyof typeof validBinding
>) {
  Reflect.apply(validBinding[method], validBinding, []);
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

assert.equal(new Set(directBridgeValidatedOperations).size, 16);
console.log(
  JSON.stringify({
    directOperationsValidated: directBridgeValidatedOperations.length,
    camelCaseChokepoint: true,
    strictAdditionalProperties: true,
    jsonTextValidation: true,
  }),
);
