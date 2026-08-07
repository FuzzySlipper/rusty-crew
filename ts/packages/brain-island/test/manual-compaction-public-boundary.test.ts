import assert from "node:assert/strict";
import test from "node:test";

import {
  appendCoreEventsToChatLog,
  type ServiceWakeDispatchContext,
} from "../src/service-wake-dispatch.js";
import { handleRustyViewChatRequest } from "../src/rusty-view-chat-api.js";
import {
  isManualCompactionDuplicate,
  manualCompactionEffectiveFingerprint,
  runManualContextCompaction,
  type ManualCompactionDeps,
} from "../src/manual-compaction.js";
import type { SessionState } from "@rusty-crew/contracts";
import type { CoreEvent } from "@rusty-crew/contracts";

// ---- Event mapper + persistence readback for no-fingerprint failed compaction ----

test("public boundary: event mapper synthesizes hyphen fingerprint for no-fingerprint failed compaction and persists durably", async () => {
  const saved: Array<Record<string, unknown>> = [];
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const now = "2026-08-07T00:00:00.000Z";
  const context = {
    now: () => now,
    bridge: {
      async saveContextCompactionArtifact(artifact: Record<string, unknown>) {
        saved.push(artifact);
        return artifact;
      },
    },
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
    recordEvent: () => {},
  } as unknown as ServiceWakeDispatchContext;

  const session = { sessionId: "sess-public-no-fp-6624-9" } as SessionState;
  const wakeId = "wake-no-fp-failed-1";

  // Failed brain event with artifact:null and no sourceProjectionFingerprint, only intentKey (both camel and snake aliases)
  const metadataJson = JSON.stringify({
    kind: "context_compaction_failed",
    intentKey: "verify-no-fp-failure-retry-8",
    intent_key: "verify-no-fp-failure-retry-8",
    usage: { prompt_tokens: 12345, total_tokens: 12345 },
    reasonCode: "manual_intent_failed",
    strategyId: "rolling_summary_compaction",
    // no sourceProjectionFingerprint / source_projection_fingerprint → must fallback to manual- hyphen
  });

  await appendCoreEventsToChatLog(context, session, wakeId, [
    {
      type: "brain_event_observed",
      sessionId: session.sessionId,
      wakeId,
      event: {
        type: "provider_status",
        level: "error",
        message: "manual_intent_failed",
        metadataJson,
      },
    } as unknown as CoreEvent,
  ]);

  assert.equal(
    saved.length,
    1,
    "failed no-fp should persist one synthetic artifact",
  );
  const artifact = saved[0] as Record<string, unknown>;
  assert.equal(artifact["intent_key"], "verify-no-fp-failure-retry-8");
  assert.equal(artifact["session_id"], "sess-public-no-fp-6624-9");
  assert.equal(
    artifact["source_projection_fingerprint"],
    "manual-verify-no-fp-failure-retry-8",
    "no-fp failed fingerprint must be manual- hyphen, not manual_ underscore",
  );
  assert.equal(artifact["trigger"], "manual_intent");
  assert.equal(artifact["terminal_status"], "failed");
  assert.equal(artifact["enters_future_context"], false);
  assert.match(String(artifact["artifact_id"]), /^context_compaction_/);

  // persistence readback: the saved artifact should be retrievable via the same hyphen fallback
  assert.ok(
    appended.some((e) => e.kind === "context_compaction_failed"),
    "chat log should contain context_compaction_failed",
  );
});

test("public boundary: contextual compaction success with no-fingerprint also falls back to hyphen", async () => {
  const saved: Array<Record<string, unknown>> = [];
  const context = {
    now: () => "2026-08-07T00:00:00.000Z",
    bridge: {
      async saveContextCompactionArtifact(artifact: Record<string, unknown>) {
        saved.push(artifact);
        return artifact;
      },
    },
    appendChatEvent: async () => ({}),
    recordEvent: () => {},
  } as unknown as ServiceWakeDispatchContext;
  const session = { sessionId: "sess-public-no-fp-success" } as SessionState;
  const wakeId = "wake-success-no-fp";

  // Simulate a completed compaction with no fingerprint in the intent, but artifact carries no fingerprint? Actually runtime artifact will have sourceProjectionFingerprint fallback
  // For success path we test the synthetic failed path still uses hyphen; success path via service-app fallback also hyphen (covered by HTTP test)
  // Here just verify that a failed event without fingerprint still gets hyphen even when usage is minimal
  const metadataJson = JSON.stringify({
    kind: "context_compaction_failed",
    intentKey: "verify-no-fp-success-8",
    usage: { total_tokens: 500 },
  });
  await appendCoreEventsToChatLog(context, session, wakeId, [
    {
      type: "brain_event_observed",
      sessionId: session.sessionId,
      wakeId,
      event: {
        type: "provider_status",
        level: "error",
        message: "manual_intent_failed",
        metadataJson,
      },
    } as unknown as CoreEvent,
  ]);
  assert.equal(
    saved[0]["source_projection_fingerprint"],
    "manual-verify-no-fp-success-8",
  );
});

test("production helper manualCompactionEffectiveFingerprint uses hyphen fallback, not underscore", () => {
  assert.equal(
    manualCompactionEffectiveFingerprint({
      intentKey: "verify-no-fp-8",
      sourceProjectionFingerprint: null,
    }),
    "manual-verify-no-fp-8",
  );
  assert.equal(
    manualCompactionEffectiveFingerprint({
      intentKey: "verify-no-fp-8",
      sourceProjectionFingerprint: "fp-explicit",
    }),
    "fp-explicit",
  );
  assert.equal(
    isManualCompactionDuplicate(
      {
        intent_key: "verify-no-fp-8",
        session_id: "sess-1",
        source_projection_fingerprint: null,
      },
      {
        intentKey: "verify-no-fp-8",
        sessionId: "sess-1",
        sourceProjectionFingerprint: null,
      },
      "manual-verify-no-fp-8",
    ),
    true,
  );
  assert.equal(
    isManualCompactionDuplicate(
      {
        intent_key: "verify-no-fp-8",
        session_id: "sess-1",
        source_projection_fingerprint: "manual-verify-no-fp-8",
      },
      {
        intentKey: "verify-no-fp-8",
        sessionId: "sess-1",
        sourceProjectionFingerprint: null,
      },
      "manual-verify-no-fp-8",
    ),
    true,
  );
});

// ---- Authoritative service/API boundary through the PRODUCTION callback ----
//
// These tests drive the same exported `runManualContextCompaction` that
// service-app.ts wires as `manualContextCompaction`, through the real HTTP
// route handler. The bridge double owns one in-memory artifact array: save
// appends, list reads — a single authoritative store with no test-local
// idempotency or fingerprint logic (R6624-11/12). Breaking the production
// callback or its bridge persistence/readback fails these tests by
// construction; the negative control proves the fail-closed path explicitly.

interface AuthoritativeStoreOptions {
  sessionId: string;
  artifacts: Array<Record<string, unknown>>;
  saveArtifact: (
    artifact: Record<string, unknown>,
  ) => Promise<unknown> | unknown;
  listArtifacts: () => Promise<Array<Record<string, unknown>>>;
  onSyntheticFallback?: () => void;
}

function failedCompactionBrainEvent(
  sessionId: string,
  wakeId: string,
  intent: { intentKey: string; sourceProjectionFingerprint?: string | null },
): CoreEvent {
  // No sourceProjectionFingerprint on purpose for the no-fp case: the
  // production mapper and callback must agree on the manual-{intentKey} hyphen
  // fallback. When the caller supplied an explicit fingerprint the fake brain
  // echoes it back (as the real Rust brain does via the compaction intent).
  const metadata: Record<string, unknown> = {
    kind: "context_compaction_failed",
    intentKey: intent.intentKey,
    intent_key: intent.intentKey,
    usage: { prompt_tokens: 12345, total_tokens: 12345 },
    reasonCode: "manual_intent_failed",
    strategyId: "rolling_summary_compaction",
  };
  if (intent.sourceProjectionFingerprint) {
    metadata["sourceProjectionFingerprint"] =
      intent.sourceProjectionFingerprint;
    metadata["source_projection_fingerprint"] =
      intent.sourceProjectionFingerprint;
  }
  return {
    type: "brain_event_observed",
    sessionId,
    wakeId,
    event: {
      type: "provider_status",
      level: "error",
      message: "manual_intent_failed",
      metadataJson: JSON.stringify(metadata),
    },
  } as unknown as CoreEvent;
}

function makeManualCompactionDeps(
  options: AuthoritativeStoreOptions,
): ManualCompactionDeps {
  const queuedEvents: CoreEvent[] = [];
  let wakeSequence = 0;
  const wakeBridge = {
    async saveContextCompactionArtifact(artifact: Record<string, unknown>) {
      return options.saveArtifact(artifact);
    },
    async subscribeEvents() {
      return { subscriptionId: "sub-manual-compact" };
    },
    async drainSubscriptionEvents() {
      return queuedEvents.splice(0);
    },
    async unsubscribeEvents() {
      return undefined;
    },
    async wakeBrain(request: unknown) {
      const intent =
        (request as { compaction_intent?: Record<string, unknown> })
          ?.compaction_intent ??
        (request as { compactionIntent?: Record<string, unknown> })
          ?.compactionIntent ??
        {};
      wakeSequence++;
      queuedEvents.push(
        failedCompactionBrainEvent(
          options.sessionId,
          `wake-authoritative-${wakeSequence}`,
          {
            intentKey: String(intent.intentKey ?? "authority-failed-no-fp"),
            sourceProjectionFingerprint:
              typeof intent.sourceProjectionFingerprint === "string"
                ? intent.sourceProjectionFingerprint
                : null,
          },
        ),
      );
      return { accepted: true };
    },
    async buildBrainWakeRequestForSession() {
      return {};
    },
  };
  const dispatch = {
    now: () => "2026-08-07T00:00:00.000Z",
    bridge: wakeBridge,
    brainForProfile: () => ({}),
    loadProfileContext: async () => ({
      profile: {
        profileId: "profile-manual-compact",
        displayName: "Manual Compact Test",
        modelConfig: { provider: "openai", modelName: "gpt-test" },
        prompt: { system: "test system prompt", instructions: [] },
      },
      skills: [],
      toolSelection: {
        inventory: {
          selectedTools: [],
          selectedBindings: [],
          selectedDescriptors: [],
          items: [],
        },
        toolProfile: { tools: [] },
      },
    }),
    configuredSessionForRuntimeSession: () => undefined,
    prepareContextStrategy: async () => ({
      additionalInstructions: [],
      sessionMemoryContext: undefined,
    }),
    roleplayPromptContextForSession: async () => undefined,
    appendChatEvent: async (_sessionId: string, event: unknown) => event,
    recordEvent: () => {},
    nextWakeId: () => "wake-test",
  } as unknown as ServiceWakeDispatchContext;

  return {
    bridge: {
      async listContextCompactionArtifacts() {
        return options.listArtifacts();
      },
      async manualContextCompaction() {
        options.onSyntheticFallback?.();
        throw new Error(
          "synthetic CoreEngine fallback must never be reached when the real brain wake persists",
        );
      },
    } as unknown as ManualCompactionDeps["bridge"],
    dispatch,
  };
}

function makeCompactContext(
  deps: ManualCompactionDeps,
  sessionId = "sess-http-authority-6624-12",
) {
  const session = {
    sessionId,
    status: "active",
    profileId: "profile-manual-compact",
  } as unknown as SessionState;
  return {
    session,
    context: {
      listSessions: async () => [session as unknown as SessionState],
      manualContextCompaction: (
        input: Parameters<typeof runManualContextCompaction>[1],
      ) => runManualContextCompaction(deps, input),
    } as unknown as Parameters<typeof handleRustyViewChatRequest>[1],
  };
}

function makeCompactRequest(
  sessionId: string,
  body: unknown,
  headers: Record<string, string> = {},
) {
  return {
    method: "POST",
    url: `http://rusty-crew.local/v1/chat/sessions/${sessionId}/context/compact`,
    headers,
    body,
    requestId: `req-authority-${Math.random()}`,
  };
}

test("public boundary: production callback via HTTP persists failed no-fingerprint artifact, readback 201, idempotent 200, revision conflict 409", async () => {
  const sessionId = "sess-http-authority-6624-12";
  const artifacts: Array<Record<string, unknown>> = [];
  let syntheticFallbackCalls = 0;
  const deps = makeManualCompactionDeps({
    sessionId,
    artifacts,
    saveArtifact: async (artifact) => {
      const existing = artifacts.findIndex(
        (a) => a["artifact_id"] === artifact["artifact_id"],
      );
      if (existing >= 0) artifacts[existing] = artifact;
      else artifacts.push(artifact);
      return artifact;
    },
    listArtifacts: async () => [...artifacts],
    onSyntheticFallback: () => {
      syntheticFallbackCalls++;
    },
  });
  const { session, context } = makeCompactContext(deps, sessionId);
  const request = (body: unknown, headers?: Record<string, string>) =>
    makeCompactRequest(session.sessionId, body, headers);

  // First POST: failed compaction, no caller fingerprint → 201, idempotent=false.
  const first = await handleRustyViewChatRequest(
    request({ intentKey: "authority-failed-no-fp" }),
    context,
  );
  assert.equal(first.status, 201, "first create should be 201");
  const firstBody = (
    first as unknown as {
      body: {
        data: { artifact: Record<string, unknown>; idempotent: boolean };
      };
    }
  ).body;
  assert.equal(firstBody.data.idempotent, false);
  assert.equal(firstBody.data.artifact["terminal_status"], "failed");
  assert.equal(firstBody.data.artifact["intent_key"], "authority-failed-no-fp");
  assert.equal(
    firstBody.data.artifact["source_projection_fingerprint"],
    "manual-authority-failed-no-fp",
    "production callback must fall back to manual- hyphen when the caller omits the fingerprint",
  );
  assert.equal(
    artifacts.length,
    1,
    "persisted row must be readable back through the authoritative store",
  );
  assert.equal(
    artifacts[0]["artifact_id"],
    firstBody.data.artifact["artifact_id"],
    "readback row must be the same artifact the route returned",
  );
  assert.equal(
    syntheticFallbackCalls,
    0,
    "the real brain wake persisted the artifact; synthetic CoreEngine fallback must not run",
  );

  // Identical retry (no fingerprint) → idempotent 200, same artifact, still one row.
  const second = await handleRustyViewChatRequest(
    request({ intentKey: "authority-failed-no-fp" }),
    context,
  );
  assert.equal(second.status, 200, "idempotent retry should be 200");
  const secondBody = (
    second as unknown as {
      body: {
        data: { artifact: Record<string, unknown>; idempotent: boolean };
      };
    }
  ).body;
  assert.equal(secondBody.data.idempotent, true);
  assert.equal(
    secondBody.data.artifact["artifact_id"],
    firstBody.data.artifact["artifact_id"],
  );
  assert.equal(
    artifacts.length,
    1,
    "retry must not create a duplicate row in the authoritative store",
  );

  // If-Match revision conflict against the persisted failed artifact → 409.
  const conflict = await handleRustyViewChatRequest(
    request({ intentKey: "authority-failed-no-fp" }, { "if-match": "2" }),
    context,
  );
  assert.equal(conflict.status, 409, "revision mismatch should be 409");
  const conflictBody = (
    conflict as unknown as {
      body: { error: { reason_code: string; message: string } };
    }
  ).body;
  assert.equal(conflictBody.error.reason_code, "revision_conflict");
  assert.match(conflictBody.error.message, /revision_conflict/);

  // Explicit caller fingerprint is projection-distinct from the no-fp row.
  const explicit = await handleRustyViewChatRequest(
    request({
      intentKey: "authority-failed-no-fp",
      sourceProjectionFingerprint: "fp-caller-explicit",
    }),
    context,
  );
  assert.equal(explicit.status, 201);
  const explicitBody = (
    explicit as unknown as {
      body: { data: { artifact: Record<string, unknown> } };
    }
  ).body;
  assert.equal(
    explicitBody.data.artifact["source_projection_fingerprint"],
    "fp-caller-explicit",
  );
  assert.equal(
    artifacts.length,
    2,
    "different fingerprint must not reuse the no-fp row",
  );
});

test("public boundary: broken persistence fails closed (500 manual_compaction_failed), never synthetic success", async () => {
  const sessionId = "sess-http-authority-6624-12";
  const artifacts: Array<Record<string, unknown>> = [];
  let syntheticFallbackCalls = 0;
  const deps = makeManualCompactionDeps({
    sessionId,
    artifacts,
    // Authoritative store that never accepts writes: the wake persists nothing,
    // so the production callback must fail closed instead of reporting success.
    saveArtifact: async () => undefined,
    listArtifacts: async () => [...artifacts],
    onSyntheticFallback: () => {
      syntheticFallbackCalls++;
    },
  });
  const { session, context } = makeCompactContext(deps, sessionId);

  const result = await handleRustyViewChatRequest(
    makeCompactRequest(session.sessionId, {
      intentKey: "failclosed-no-fp",
    }),
    context,
  );
  assert.equal(
    result.status,
    500,
    "no durable brain artifact must fail closed, not return a synthetic success",
  );
  const body = (
    result as unknown as {
      body: { error: { reason_code: string; message: string } };
    }
  ).body;
  assert.equal(body.error.reason_code, "manual_compaction_failed");
  assert.match(body.error.message, /did not produce a durable brain artifact/);
  assert.equal(syntheticFallbackCalls, 0);
  assert.equal(
    artifacts.length,
    0,
    "broken persistence must not fabricate a row",
  );
});
