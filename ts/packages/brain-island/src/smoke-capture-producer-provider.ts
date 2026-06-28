import assert from "node:assert/strict";
import type {
  MemorySpaceId,
  ProfileId,
  SessionActivityDigest,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  NativeModelProviderRecord,
  NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import {
  normalizeCaptureProviderOutput,
  runStructuredCaptureProvider,
} from "./capture-producer-provider.js";

const profileId = "runner" as ProfileId;
const digest: SessionActivityDigest = {
  digest_id: "sad_alpha",
  profile_id: profileId,
  session_id: "session-alpha" as SessionId,
  wake_id: "wake-alpha",
  source: "direct_debug",
  summary_text: "User corrected that the database lives on den-srv.",
  event_counts_json: { "brain_event_observed.tool_call_finished": 1 },
  tool_calls_json: [{ tool_name: "shell", status: "failed" }],
  signals_json: [{ signal_type: "user_correction" }],
  completion_summary: "wake completed",
  allowed_capture_spaces: ["profile_dense" as MemorySpaceId],
  created_at: "2026-06-27T12:00:00.000Z",
};

const bridge = fakeBridge(provider({ alias: "capture", hasSecret: true }));
const result = await runStructuredCaptureProvider({
  runId: "capture-run",
  profileId,
  providerAlias: "capture",
  bridge,
  sessionActivityDigests: [digest],
  transport: async (url, init) => {
    assert.equal(url, "http://provider.local/v1/chat/completions");
    assert.equal(init.headers.authorization, "Bearer secret-value");
    return {
      proposals: [profileDenseProposal()],
      skippedReasons: [],
    };
  },
});
assert.equal(result.skippedReasons.length, 0);
assert.equal(result.proposals.length, 1);
assert.equal(result.proposals[0]?.space_id, "profile_dense");

const unsupported = normalizeCaptureProviderOutput({
  runId: "capture-run",
  profileId,
  output: {
    proposals: [
      { ...profileDenseProposal(), space_id: "roleplay_lore" },
      profileDenseProposal(),
    ],
  },
});
assert.equal(unsupported.proposals.length, 1);

const invalid = normalizeCaptureProviderOutput({
  runId: "capture-run",
  profileId,
  output: { skippedReasons: ["provider_said_no"] },
});
assert.deepEqual(invalid.skippedReasons, [
  "provider_said_no",
  "capture_provider_invalid_json",
]);

const missingAlias = await runStructuredCaptureProvider({
  runId: "capture-run",
  profileId,
  providerAlias: "",
  bridge,
  sessionActivityDigests: [digest],
});
assert.deepEqual(missingAlias.skippedReasons, [
  "capture_provider_alias_missing",
]);

const unavailable = await runStructuredCaptureProvider({
  runId: "capture-run",
  profileId,
  providerAlias: "missing",
  bridge,
  sessionActivityDigests: [digest],
});
assert.deepEqual(unavailable.skippedReasons, ["capture_provider_unavailable"]);

console.log("smoke-capture-producer-provider ok");

function profileDenseProposal(): Record<string, unknown> {
  return {
    summary: "Remember database host.",
    space_id: "profile_dense",
    operation: "add",
    scope: { scope_type: "profile", scope_id: profileId },
    shape: { shape_id: "profile_dense_item", version: 1 },
    content: {
      key: "den_core_database_location",
      content: "The Den Core database lives on den-srv.",
    },
    evidence_refs: [
      {
        eventType: "user_correction",
        wakeId: "wake-alpha",
        summary: "User corrected database location.",
      },
    ],
    confidence: 0.86,
    durability_rationale: "Infrastructure topology is durable.",
    governance_policy: "curator_route",
  };
}

function fakeBridge(
  record: NativeModelProviderRecord,
): Pick<NativeBridgeModule, "getModelProvider" | "getModelProviderSecret"> {
  return {
    async getModelProvider(alias) {
      return alias === record.alias ? record : undefined;
    },
    async getModelProviderSecret(alias) {
      return alias === record.alias ? "secret-value" : undefined;
    },
  };
}

function provider(input: {
  alias: string;
  hasSecret: boolean;
}): NativeModelProviderRecord {
  return {
    alias: input.alias,
    status: "active",
    protocol: "chat_completions",
    providerKind: "openai",
    baseUrl: "http://provider.local/v1",
    modelId: "capture-model",
    credential: { hasSecret: input.hasSecret },
    metadataJson: {},
    revision: 1,
    createdAt: "2026-06-27T12:00:00.000Z",
    updatedAt: "2026-06-27T12:00:00.000Z",
  };
}
