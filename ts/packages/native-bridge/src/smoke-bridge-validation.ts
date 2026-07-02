import { readFileSync } from "node:fs";

import type { BodyState } from "@rusty-crew/contracts";

import {
  BridgeValidationError,
  validateBridgeValue,
} from "./bridge-validation.js";
import {
  actionBatchReceiptSchema,
  brainActionBatchSchema,
  brainEventEnvelopeSchema,
  brainWakeRequestSchema,
  eventReceiptSchema,
  openAiResponsesBrainRunInputSchema,
  rawBodyStateSchema,
  rawModelProviderRecordSchema,
  rawOpenAiResponsesBrainRunResultSchema,
  rawProfileRegistryRecordSchema,
  rawSessionStateArraySchema,
} from "./bridge-validation-schemas.js";
import type { OpenAiResponsesBrainRunInput } from "./index.js";

const validationEnv = { RUSTY_CREW_BRIDGE_VALIDATE: "1" };

interface RustBridgeValidationFixtureFile {
  fixtures: Array<{
    name: string;
    value: unknown;
  }>;
}

const rustFixtures = JSON.parse(
  readFileSync(
    new URL("../bridge-validation-rust-fixtures.json", import.meta.url),
    "utf8",
  ),
) as RustBridgeValidationFixtureFile;

const rustFixtureValues = new Map(
  rustFixtures.fixtures.map((fixture) => [fixture.name, fixture.value]),
);

function rustFixture(name: string): unknown {
  const value = rustFixtureValues.get(name);
  if (value === undefined) {
    throw new Error(`missing Rust bridge validation fixture ${name}`);
  }
  return value;
}

const bodyState: BodyState = {
  session: {
    handle: 1 as BodyState["session"]["handle"],
    sessionId: "validation-session" as BodyState["session"]["sessionId"],
    agentId: "validation-agent" as BodyState["session"]["agentId"],
    profileId: "validation-profile" as BodyState["session"]["profileId"],
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
    status: "idle",
    brainTurnCount: 0,
    createdAt: "2026-07-02T00:00:00.000Z",
    lastActiveAt: "2026-07-02T00:00:00.000Z",
  },
  pendingMessages: [],
  recentEvents: [],
  childCompletions: [],
  fanOutGroups: [],
  deltaPolicy: {
    mode: "frozen_snapshot_next_wake",
    queueOwner: "body",
    queuedMessageTtlMs: 30_000,
    maxQueuedMessages: 20,
  },
};

const input: OpenAiResponsesBrainRunInput = {
  wakeId: "validation-wake",
  sessionId: "validation-session" as OpenAiResponsesBrainRunInput["sessionId"],
  bodyState,
  config: {
    model: "gpt",
    streamIdleTimeoutMs: 30_000,
  },
  client: { mode: "fake" },
};

validateBridgeValue<OpenAiResponsesBrainRunInput>({
  operation: "run_openai_responses_brain",
  direction: "ts_to_rust",
  schema: openAiResponsesBrainRunInputSchema,
  value: input,
  env: validationEnv,
});

validateBridgeValue({
  operation: "run_openai_responses_brain",
  direction: "rust_to_ts",
  schema: rawOpenAiResponsesBrainRunResultSchema,
  value: {
    stream: [
      {
        type: "event",
        event: {
          wake_id: "validation-wake",
          session_id: "validation-session",
          event: { type: "started" },
        },
      },
      {
        type: "actions",
        batch: {
          wake_id: "validation-wake",
          session_id: "validation-session",
          actions: [],
        },
      },
    ],
    provider_state: { type: "unchanged" },
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "wake_brain",
  direction: "ts_to_rust",
  schema: brainWakeRequestSchema,
  value: {
    brain: 1,
    sessionId: "validation-session",
    bodyState: 2,
    systemPrompt: 3,
    roleAssembly: 4,
    wakeId: "validation-wake",
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_event",
  direction: "ts_to_rust",
  schema: brainEventEnvelopeSchema,
  value: {
    wakeId: "validation-wake",
    sessionId: "validation-session",
    event: { type: "text_delta", text: "hello" },
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_actions",
  direction: "ts_to_rust",
  schema: brainActionBatchSchema,
  value: {
    wakeId: "validation-wake",
    sessionId: "validation-session",
    actions: [
      {
        type: "send_message",
        message: {
          from: "validation-agent",
          to: "operator",
          body: "hello",
        },
      },
    ],
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_event",
  direction: "rust_to_ts",
  schema: eventReceiptSchema,
  value: { accepted: true, sequence: 1 },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_actions",
  direction: "rust_to_ts",
  schema: actionBatchReceiptSchema,
  value: {
    wakeId: "validation-wake",
    acceptedActions: 1,
    rejectedActions: [],
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "list_sessions",
  direction: "rust_to_ts",
  schema: rawSessionStateArraySchema,
  value: [
    {
      handle: 1,
      session_id: "validation-session",
      agent_id: "validation-agent",
      profile_id: "validation-profile",
      kind: "full",
      resource_limits: {},
      tool_profile: { tools: [] },
      status: "idle",
      brain_turn_count: 0,
      created_at: "2026-07-02T00:00:00.000Z",
      last_active_at: "2026-07-02T00:00:00.000Z",
    },
  ],
  env: validationEnv,
});

validateBridgeValue({
  operation: "project_body_state_json",
  direction: "rust_to_ts",
  schema: rawBodyStateSchema,
  value: {
    session: {
      handle: 1,
      session_id: "validation-session",
      agent_id: "validation-agent",
      profile_id: "validation-profile",
      kind: "full",
      resource_limits: {},
      tool_profile: { tools: [] },
      status: "idle",
      brain_turn_count: 0,
      created_at: "2026-07-02T00:00:00.000Z",
      last_active_at: "2026-07-02T00:00:00.000Z",
    },
    pending_messages: [],
    recent_events: [],
    child_completions: [],
    fan_out_groups: [],
    delta_policy: {
      mode: "frozen_snapshot_next_wake",
      queue_owner: "body",
      queued_message_ttl_ms: 30_000,
      max_queued_messages: 20,
    },
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "rust_fixture_project_body_state_json",
  direction: "rust_to_ts",
  schema: rawBodyStateSchema,
  value: rustFixture("body_state_v1"),
  env: validationEnv,
});

validateBridgeValue({
  operation: "rust_fixture_list_sessions",
  direction: "rust_to_ts",
  schema: rawSessionStateArraySchema,
  value: rustFixture("list_sessions_v1"),
  env: validationEnv,
});

validateBridgeValue({
  operation: "rust_fixture_run_openai_responses_brain",
  direction: "rust_to_ts",
  schema: rawOpenAiResponsesBrainRunResultSchema,
  value: rustFixture("brain_wake_stream_result_v1"),
  env: validationEnv,
});

validateBridgeValue({
  operation: "rust_fixture_profile_registry_record",
  direction: "rust_to_ts",
  schema: rawProfileRegistryRecordSchema,
  value: rustFixture("profile_registry_record_v1"),
  env: validationEnv,
});

validateBridgeValue({
  operation: "rust_fixture_model_provider_record",
  direction: "rust_to_ts",
  schema: rawModelProviderRecordSchema,
  value: rustFixture("model_provider_record_v1"),
  env: validationEnv,
});

try {
  validateBridgeValue({
    operation: "run_openai_responses_brain",
    direction: "rust_to_ts",
    schema: rawOpenAiResponsesBrainRunResultSchema,
    value: { provider_state: { type: "unchanged" } },
    env: validationEnv,
  });
  throw new Error("expected bridge validation to reject missing stream");
} catch (error) {
  if (!(error instanceof BridgeValidationError)) throw error;
  if (
    !error.message.includes("run_openai_responses_brain") ||
    !error.message.includes("rust_to_ts")
  ) {
    throw new Error(`unexpected validation error message: ${error.message}`);
  }
}

try {
  validateBridgeValue({
    operation: "submit_brain_actions",
    direction: "ts_to_rust",
    schema: brainActionBatchSchema,
    value: {
      wakeId: "validation-wake",
      sessionId: "validation-session",
      actions: [{ type: "request_delegation", prompt: "missing profile" }],
    },
    env: validationEnv,
  });
  throw new Error("expected bridge validation to reject malformed action");
} catch (error) {
  if (!(error instanceof BridgeValidationError)) throw error;
  if (
    !error.message.includes("submit_brain_actions") ||
    !error.message.includes("ts_to_rust")
  ) {
    throw new Error(`unexpected validation error message: ${error.message}`);
  }
}

try {
  validateBridgeValue({
    operation: "list_sessions",
    direction: "rust_to_ts",
    schema: rawSessionStateArraySchema,
    value: [{ session_id: "missing required fields" }],
    env: validationEnv,
  });
  throw new Error("expected bridge validation to reject malformed session");
} catch (error) {
  if (!(error instanceof BridgeValidationError)) throw error;
  if (
    !error.message.includes("list_sessions") ||
    !error.message.includes("rust_to_ts")
  ) {
    throw new Error(`unexpected validation error message: ${error.message}`);
  }
}

console.log("bridge validation smoke passed");
