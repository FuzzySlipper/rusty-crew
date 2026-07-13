import assert from "node:assert/strict";
import type { ProfileConfig } from "../src/profile-loading.js";
import {
  inspectLoreRetrievalTool,
  inspectRoleplaySceneTool,
  inspectRoleplayTranscriptTool,
} from "../src/roleplay-mechanic-tools.js";

const profile = {
  profileId: "mechanic",
  displayName: "Maren",
  roleplayMechanic: { autoMonitor: false },
} as ProfileConfig;

const bridge = {
  async planRoleplayMechanicProfile() {
    throw new Error("not used");
  },
  async getRoleplaySessionMetadata(sessionId: string) {
    if (sessionId === "missing") return undefined;
    return {
      sessionId,
      profileId: "narrator",
      narratorDiagnostic: {
        wakeId: "wake-7",
        sceneBrief: "The observatory door is open.",
        relevantLoreRecordIds: ["lore-observatory"],
        updatedAt: "2026-07-13T00:00:00Z",
      },
    };
  },
  async queryMessageSlots() {
    return [
      slot("slot-user", "variant-user", "user", "Open the door."),
      {
        ...slot(
          "slot-assistant",
          "variant-primary",
          "assistant",
          "It stays shut.",
        ),
        active_variant_id: "variant-selected",
        alternates: [
          {
            variant_id: "variant-selected",
            source: "regenerated",
            message: message("assistant", "The door swings inward."),
          },
        ],
      },
      slot("slot-tool", "variant-tool", "tool", "internal tool noise"),
    ];
  },
  async listSimpleKv() {
    return [
      {
        valueJson: JSON.stringify({
          sessionId: "rp-session",
          location: "Observatory",
        }),
        updatedAt: "2026-07-13T00:00:00Z",
        revision: 3,
      },
    ];
  },
  async readRoleplaySceneState(input: Record<string, unknown>) {
    return { state: JSON.parse(String(input.record_value_json)), revision: 3 };
  },
  async getProfileRegistryRecord() {
    return {
      profileId: "narrator",
      displayName: "Storyteller",
      activeRuntimeSettingsJson: {
        providerAlias: "deepseek-flash",
        roleplayNarrator: { tone: "moody", exemplar: "Rain on brass." },
      },
    };
  },
  async getChatLayers() {
    return [{ layer_id: "world", enabled: true, priority: 0 }];
  },
  async listRecallTraces() {
    return [
      {
        trace_id: "trace-1",
        entries_considered: 2,
        entries_returned: 1,
        token_budget: 100,
        tokens_consumed: 20,
        entry_decisions: [
          {
            record_id: "lore-observatory",
            layer_id: "world",
            score: 1.4,
            token_estimate: 20,
            is_constant: false,
            included: true,
            reason: "included",
          },
          {
            record_id: "lore-distant-city",
            layer_id: "world",
            score: 0.7,
            token_estimate: 90,
            is_constant: false,
            included: false,
            reason: "token_budget_exceeded",
          },
        ],
      },
    ];
  },
} as never;

const transcript = await inspectRoleplayTranscriptTool({
  bridge,
  profile,
}).execute("transcript", { sessionId: "rp-session" });
assert.equal(transcript.details.ok, true);
const transcriptResult = transcript.details.result as {
  messages: Array<Record<string, unknown>>;
};
assert.equal(transcriptResult.messages.length, 2);
assert.equal(transcriptResult.messages[1]?.body, "The door swings inward.");
assert.equal(transcriptResult.messages[1]?.variantSource, "regenerated");

const scene = await inspectRoleplaySceneTool({ bridge, profile }).execute(
  "scene",
  { sessionId: "rp-session" },
);
assert.equal(scene.details.ok, true);
const sceneResult = scene.details.result as Record<string, unknown>;
assert.deepEqual(sceneResult.sceneState, {
  status: "available",
  state: { sessionId: "rp-session", location: "Observatory" },
  revision: 3,
});
assert.equal(
  (sceneResult.narratorDiagnostic as { status: string }).status,
  "available",
);

const traces = await inspectLoreRetrievalTool({ bridge, profile }).execute(
  "traces",
  { sessionId: "rp-session" },
);
assert.equal(traces.details.ok, true);
assert.equal(
  (
    (traces.details.result as { traces: unknown[] }).traces[0] as {
      entry_decisions: unknown[];
    }
  ).entry_decisions.length,
  2,
);

const missing = await inspectRoleplaySceneTool({ bridge, profile }).execute(
  "missing",
  { sessionId: "missing" },
);
assert.equal(missing.details.ok, false);
assert.equal(missing.details.reasonCode, "roleplay_session_not_found");

console.log(
  JSON.stringify({
    transcriptMessages: transcriptResult.messages.length,
    selectedAlternatePreserved: true,
    sceneBriefAvailable: true,
    traceDecisions: 2,
    missingSessionReason: missing.details.reasonCode,
  }),
);

function slot(slotId: string, variantId: string, role: string, body: string) {
  return {
    slot_id: slotId,
    active_variant_id: null,
    primary: {
      variant_id: variantId,
      source: "primary",
      message: message(role, body),
    },
    alternates: [],
  };
}

function message(role: string, body: string) {
  return {
    author_id: role === "user" ? "user-1" : role,
    author_role: role,
    body,
    created_at: "2026-07-13T00:00:00Z",
  };
}
