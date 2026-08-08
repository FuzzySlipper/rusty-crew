import assert from "node:assert/strict";
import test from "node:test";

import type { CoreEvent, SessionState } from "@rusty-crew/contracts";
import { handleRustyViewChatRequest } from "../src/rusty-view-chat-api.js";
import {
  appendCoreEventsToChatLog,
  type ServiceWakeDispatchContext,
} from "../src/service-wake-dispatch.js";
import {
  rustyViewSessionContextUsage,
  type RustyViewChatOperationsContext,
} from "../src/service-rusty-view-chat-operations.js";
import { MemoryProviderRequestDebugStore } from "../src/provider-request-debug-store.js";
import { MemoryToolCallDebugStore } from "../src/tool-call-debug-store.js";

// ---------------------------------------------------------------------------
// Parent 6614 acceptance proofs at the public Crew boundary:
//   - GET /v1/chat/sessions/{id}/context route contract (success with versioned
//     Rust snapshot replay, unavailable measurement, policy disabled, and
//     route failure modes) driven through the PRODUCTION contextUsage
//     implementation with a narrow operations double.
//   - SSE/chat-log compaction status ordering around the relevant logical turn.
//   - Redaction proofs for provider URLs, headers, keys, raw debug payloads,
//     and unrestricted tool arguments through the diagnostics/debug surfaces.
// ---------------------------------------------------------------------------

const SESSION_ID = "sess-ctx-6614";

function sessionState(overrides: Record<string, unknown> = {}): SessionState {
  return {
    sessionId: SESSION_ID,
    agentId: "agent-1",
    profileId: "profile-1",
    status: "active",
    toolProfile: { tools: [] },
    ...overrides,
  } as unknown as SessionState;
}

const COMPLETED_ARTIFACT = {
  artifact_id: "context_compaction_abc",
  strategy_id: "rolling_summary_compaction",
  branch_id: null,
  enters_future_context: true,
  context_policy: "rolling_summary_compaction",
  created_at: "2026-08-07T00:00:00.000Z",
  updated_at: "2026-08-07T00:00:00.000Z",
  estimate_before_json: { tokens: 120000, source: "provider" },
  estimate_after_json: {
    tokens: 64000,
    source: "serialized_compaction_projection",
  },
};

const ENABLED_POLICY = {
  providerAlias: "openai",
  brain: { module: "openai-responses", strategy: "native" },
  contextPolicy: {
    strategyId: "rolling_summary_compaction",
    enabled: true,
    autoCompactionEnabled: true,
    compactAtPercent: 85,
    targetPercentAfterCompaction: 60,
    maxContextPercentForWake: 95,
    debugVisibility: "internal",
    includeDebugEventsInModelContext: false,
  },
};

const PROVIDER_WITH_SECRET_URL = {
  alias: "openai",
  status: "active",
  protocol: "responses",
  providerKind: "openai",
  displayName: "OpenAI",
  baseUrl: "https://user:sekret-pass@api.openai.example.com/v1/chat?x=1",
  modelId: "gpt-x",
  contextWindowTokens: 128000,
  maxOutputTokens: 4096,
  reasoningEffort: "medium",
  revision: 3,
  apiKey: "sk-secret-123",
};

function accountingSnapshotEvent(): Record<string, unknown> {
  return {
    kind: "provider_status",
    payload: {
      wake_id: "wake-1",
      level: "info",
      message: "context accounting snapshot",
      metadata_json: JSON.stringify({
        kind: "context_accounting_snapshot",
        snapshot: {
          schemaVersion: 1,
          provider: {
            alias: "openai",
            protocol: "responses",
            modelId: "gpt-x",
            contextWindowTokens: 128000,
            revision: 3,
          },
          promptProjection: {
            status: "available",
            tokens: 120000,
            fillPercent: 93.75,
            source: "provider",
          },
          reservedOutput: { maxOutputTokens: 4096, reserved: 4096 },
          admission: { allowed: true, reason: "below_threshold" },
          providerUsage: {
            promptTokens: 120000,
            completionTokens: 100,
            totalTokens: 120100,
            source: "provider",
          },
          durableTranscript: { messageCount: 42, source: "durable" },
          providerState: { conversationId: "conv-1", source: "provider" },
          compaction: {
            phase: "idle",
            latestStatus: "completed",
            artifactId: "context_compaction_abc",
            recoverableFailure: false,
          },
          diagnostics: [],
        },
      }),
    },
  };
}

interface FakeOpsOptions {
  chatEvents?: Array<Record<string, unknown>>;
  artifacts?: Array<Record<string, unknown>>;
  registrySettings?: Record<string, unknown>;
  provider?: Record<string, unknown>;
}

function makeOpsContext(options: FakeOpsOptions = {}) {
  const ops = {
    bridge: {
      getProfileRegistryRecord: async () => ({
        activeRuntimeSettingsJson: options.registrySettings ?? {},
      }),
      getModelProvider: async () => options.provider ?? {},
      listContextCompactionArtifacts: async () => options.artifacts ?? [],
    },
    runtimeConfig: {
      mcpBindings: [],
      profilesDir: "/tmp/rusty-crew-test-profiles",
      skillsDir: "/tmp/rusty-crew-test-skills",
    },
    listChatEventsAfterCursor: async () => (options.chatEvents ?? []) as never,
    resolveModelProviderForBrain: async () => ({}),
    roleplayRouteContext: () => ({}),
  } as unknown as RustyViewChatOperationsContext;
  return ops;
}

function makeRouteContext(
  ops: RustyViewChatOperationsContext,
  session: SessionState,
  withContextUsage = true,
) {
  return {
    listSessions: async () => [session as unknown as SessionState],
    ...(withContextUsage
      ? {
          contextUsage: (input: { session: SessionState; requestId: string }) =>
            rustyViewSessionContextUsage(ops, input),
        }
      : {}),
  } as unknown as Parameters<typeof handleRustyViewChatRequest>[1];
}

function contextRequest(sessionId: string, requestId = "ctx-req") {
  return {
    method: "GET",
    url: `http://rusty-crew.local/v1/chat/sessions/${sessionId}/context`,
    headers: {},
    requestId,
  };
}

// ---- GET context route contract -------------------------------------------

test("GET context: replays the versioned native snapshot with redacted provider URL and compaction artifact readback", async () => {
  const session = sessionState();
  const ops = makeOpsContext({
    chatEvents: [accountingSnapshotEvent()],
    artifacts: [COMPLETED_ARTIFACT as unknown as Record<string, unknown>],
    registrySettings: ENABLED_POLICY,
    provider: PROVIDER_WITH_SECRET_URL,
  });
  const context = makeRouteContext(ops, session);

  const result = await handleRustyViewChatRequest(
    contextRequest(SESSION_ID),
    context,
  );
  assert.equal(result.status, 200);
  const body = (
    result as unknown as { body: { data: Record<string, unknown> } }
  ).body;
  const data = body.data as {
    native_snapshot: Record<string, unknown>;
    provider: Record<string, unknown>;
    latest_compaction_artifact: Record<string, unknown> | undefined;
    context_strategy: Record<string, unknown>;
  };

  assert.equal(
    (data.native_snapshot as Record<string, unknown>).schemaVersion,
    1,
    "versioned Rust snapshot must be replayed from the durable provider-status event",
  );
  assert.equal(
    (
      (data.native_snapshot as Record<string, unknown>).compaction as Record<
        string,
        unknown
      >
    ).latestStatus,
    "completed",
  );
  assert.equal(
    data.provider.base_url_host,
    "api.openai.example.com",
    "provider block exposes the host, never the full URL",
  );
  assert.equal(
    data.provider.base_url_redacted,
    "https://api.openai.example.com",
    "redacted URL is the origin only (no credentials, path, or query)",
  );
  assert.equal(
    data.latest_compaction_artifact?.artifact_id,
    "context_compaction_abc",
    "latest completed compaction artifact must be readable through the context API",
  );
  assert.equal(
    data.context_strategy.enabled,
    true,
    "policy readback reflects runtime config",
  );
  assert.equal(data.context_strategy.auto_compaction_enabled, true);

  // Redaction proof: no provider credentials, key material, path, or query
  // leaks through the normal diagnostics surface.
  const serialized = JSON.stringify(body);
  assert.ok(!serialized.includes("sekret-pass"), "URL password must not leak");
  assert.ok(
    !serialized.includes("sk-secret-123"),
    "provider key material must not leak",
  );
  assert.ok(
    !serialized.includes("/v1/chat"),
    "URL path/query must not leak through the provider block",
  );
});

test("GET context: unavailable measurement is explicit approximate/compatibility diagnostics", async () => {
  const session = sessionState();
  const ops = makeOpsContext({
    chatEvents: [],
    artifacts: [],
    registrySettings: ENABLED_POLICY,
    provider: PROVIDER_WITH_SECRET_URL,
  });
  const context = makeRouteContext(ops, session);

  const result = await handleRustyViewChatRequest(
    contextRequest(SESSION_ID),
    context,
  );
  assert.equal(result.status, 200);
  const data = (
    result as unknown as {
      body: {
        data: {
          native_snapshot?: unknown;
          degraded: boolean;
          diagnostics: Array<{ code: string; severity: string }>;
        };
      };
    }
  ).body.data;
  assert.equal(data.native_snapshot, undefined);
  assert.ok(
    data.diagnostics.some(
      (diagnostic) =>
        diagnostic.code === "native_context_snapshot_not_yet_available",
    ),
    "legacy fields must be explicitly marked approximate until the Rust snapshot exists",
  );
  assert.equal(data.degraded, true);
});

test("GET context: disabled policy is reflected and the session is not created/archived/rebuild implicitly", async () => {
  const session = sessionState();
  const ops = makeOpsContext({
    chatEvents: [],
    artifacts: [],
    registrySettings: {
      providerAlias: "openai",
      contextPolicy: {
        strategyId: "rolling_summary_compaction",
        enabled: false,
        autoCompactionEnabled: false,
        compactAtPercent: 85,
        targetPercentAfterCompaction: 60,
        maxContextPercentForWake: 95,
        debugVisibility: "internal",
        includeDebugEventsInModelContext: false,
      },
    },
    provider: PROVIDER_WITH_SECRET_URL,
  });
  const context = makeRouteContext(ops, session);

  const result = await handleRustyViewChatRequest(
    contextRequest(SESSION_ID),
    context,
  );
  assert.equal(result.status, 200);
  const data = (
    result as unknown as {
      body: {
        data: {
          context_strategy: {
            enabled: boolean;
            auto_compaction_enabled: boolean;
          };
          session_id: string;
        };
      };
    }
  ).body.data;
  assert.equal(data.context_strategy.enabled, false);
  assert.equal(data.context_strategy.auto_compaction_enabled, false);
  assert.equal(
    data.session_id,
    SESSION_ID,
    "read-only context API must return the same session without mutation",
  );
});

test("GET context: route failures are explicit (404 missing session, 412 not configured)", async () => {
  const session = sessionState();
  const ops = makeOpsContext({});
  const missing = await handleRustyViewChatRequest(
    contextRequest("sess-does-not-exist"),
    makeRouteContext(ops, session),
  );
  assert.equal(missing.status, 404);
  assert.equal(
    (
      missing as unknown as {
        body: { error: { reason_code: string } };
      }
    ).body.error.reason_code,
    "chat_session_not_found",
  );

  const notWired = await handleRustyViewChatRequest(
    contextRequest(SESSION_ID),
    makeRouteContext(ops, session, false),
  );
  assert.equal(notWired.status, 412);
  assert.equal(
    (
      notWired as unknown as {
        body: { error: { reason_code: string } };
      }
    ).body.error.reason_code,
    "chat_context_usage_not_configured",
  );
});

// ---- SSE / chat-log compaction status ordering -----------------------------

function compactionProviderStatus(
  kind: "context_compaction_started" | "context_compaction_completed",
): CoreEvent {
  const metadata: Record<string, unknown> = { kind };
  if (kind === "context_compaction_completed") {
    metadata["artifact"] = {
      sequence: 1,
      strategyId: "rolling_summary_compaction",
      reasonCode: "manual_intent",
      terminalStatus: "completed",
      usageBefore: {
        inputTokens: 120000,
        contextWindowTokens: 128000,
        fillPercent: 93.75,
        source: "provider",
      },
      estimatedTokensAfter: 64000,
      compactedItemCount: 20,
      retainedItemCount: 22,
      summaryText: "Earlier work was summarized.",
      providerChainAction: null,
    };
  }
  return {
    type: "brain_event_observed",
    sessionId: "sess-ctx-6614",
    wakeId: "wake-compact-1",
    event: {
      type: "provider_status",
      level: kind === "context_compaction_started" ? "info" : "info",
      message:
        kind === "context_compaction_started"
          ? "compaction started"
          : "compaction completed",
      metadataJson: JSON.stringify(metadata),
    },
  } as unknown as CoreEvent;
}

function logicalTurnContinuingEvent(): CoreEvent {
  return {
    type: "logical_turn_lifecycle_observed",
    lifecycle: {
      projectionId: "projection-1",
      logicalTurnId: "turn-1",
      sessionId: "sess-ctx-6614",
      wakeId: "wake-compact-1",
      continuationId: "continuation-1",
      continuationCount: 1,
      kind: "continuation_resumed",
      phase: "continuing",
      operatorState: "running",
      progressClassification: "provider_progress",
      progress: {
        semanticRevision: 1,
        committedProviderOperations: 0,
        committedToolOperations: 0,
        committedProjectionCursor: 1,
        assistantContentBytes: 0,
        acceptedActionCount: 0,
        delegatedCompletionCount: 0,
        stateFingerprint: "progress-1",
        lastLivenessAt: "2026-08-07T00:00:00Z",
        lastSemanticProgressAt: "2026-08-07T00:00:00Z",
        consecutiveNoProgressSamples: 0,
      },
      reasonCode: "context_compaction_observed",
      summary: "compaction around logical turn",
      occurredAt: "2026-08-07T00:00:00Z",
      logicalTurnRevision: 1,
    },
  } as unknown as CoreEvent;
}

test("SSE/chat log: compaction started/completed status is ordered around the relevant logical turn", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const saved: Array<Record<string, unknown>> = [];
  const context = {
    bridge: {
      saveContextCompactionArtifact: async (
        artifact: Record<string, unknown>,
      ) => {
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
    now: () => "2026-08-07T00:00:00.000Z",
    recordEvent: () => undefined,
  } as unknown as ServiceWakeDispatchContext;
  const session = { sessionId: "sess-ctx-6614" } as SessionState;

  await appendCoreEventsToChatLog(context, session, "wake-compact-1", [
    compactionProviderStatus("context_compaction_started"),
    logicalTurnContinuingEvent(),
    compactionProviderStatus("context_compaction_completed"),
    {
      type: "completion_packet_delivered",
      packet: {
        sessionId: session.sessionId,
        status: "completed",
        summary: "wake completed",
      },
    } as CoreEvent,
  ]);

  assert.deepEqual(
    appended.map((event) => event.kind),
    [
      "context_compaction_started",
      "logical_turn_continuing",
      "context_compaction_completed",
      "assistant_message_completed",
      "assistant_turn_finished",
    ],
    "SSE replay must expose compaction status ordered around the logical turn",
  );
  assert.equal(
    saved.length,
    1,
    "completed compaction must persist an artifact",
  );
  assert.equal(saved[0]?.terminal_status, "completed");
  assert.equal(
    saved[0]?.session_id,
    session.sessionId,
    "artifact must be durable for restart hydration",
  );
});

// ---- Redaction proofs ------------------------------------------------------

test("redaction: provider request debug store redacts headers, keys, and bearer tokens before readback", () => {
  const store = new MemoryProviderRequestDebugStore({
    now: () => "2026-08-07T00:00:00.000Z",
  });
  const record = store.record({
    sessionId: "sess-ctx-6614",
    wakeId: "wake-1",
    brainModule: "openai-responses",
    providerAlias: "openai",
    request: {
      url: "https://api.openai.example.com/v1/responses",
      headers: {
        authorization: "Bearer sk-secret-abc",
        "x-api-key": "key-123",
      },
      body: { prompt: "raw prompt content" },
    },
  });

  assert.equal(record.request.redacted, true);
  const value = record.request.value as {
    headers: Record<string, unknown>;
    url: string;
  };
  assert.equal(
    typeof value.headers.authorization,
    "object",
    "authorization header must be replaced with a redaction marker",
  );
  assert.equal(
    (value.headers.authorization as { redacted: boolean }).redacted,
    true,
  );
  assert.equal(
    (value.headers["x-api-key"] as { redacted: boolean }).redacted,
    true,
  );
  const serialized = JSON.stringify(record);
  assert.ok(
    !serialized.includes("sk-secret-abc"),
    "bearer token must not be readable back",
  );
  assert.ok(
    !serialized.includes("key-123"),
    "api key must not be readable back",
  );
  assert.ok(
    serialized.includes("sha256"),
    "redacted values keep a non-reversible digest for forensics",
  );
});

test("redaction: tool call debug store redacts unrestricted arguments and media bytes", () => {
  const store = new MemoryToolCallDebugStore({
    now: () => "2026-08-07T00:00:00.000Z",
  });
  const started = store.start({
    toolCallId: "call-1",
    sessionId: "sess-ctx-6614",
    wakeId: "wake-1",
    toolName: "filesystem_write",
    arguments: {
      path: "/tmp/out.txt",
      api_key: "sk-tool-arg-secret",
      image: { type: "image", data: "base64-raw-bytes" },
    },
  });

  const record = store.get({
    sessionId: "sess-ctx-6614",
    debugDetailId: started.debug_detail_id,
  });
  assert.ok(record, "tool call debug record must be readable");
  assert.equal(record.arguments.redacted, true);
  const value = record.arguments.value as Record<string, unknown>;
  assert.equal(
    value.api_key,
    "[redacted]",
    "unrestricted secret-looking tool argument must not leak",
  );
  assert.equal(
    (value.image as Record<string, unknown>).data,
    "[redacted media bytes]",
    "tool image bytes must not leak through debug readback",
  );
  const serialized = JSON.stringify(record);
  assert.ok(
    !serialized.includes("sk-tool-arg-secret"),
    "tool argument secret must not be readable back",
  );
  assert.ok(
    !serialized.includes("base64-raw-bytes"),
    "media bytes must not be readable back",
  );
});
