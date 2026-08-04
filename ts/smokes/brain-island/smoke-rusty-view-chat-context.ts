import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type { SessionId } from "@rusty-crew/contracts";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const root = mkdtempSync(join(tmpdir(), "rusty-view-chat-context-"));
const port = await openPort();
const token = "rusty-view-chat-context-token";
writeRuntimeConfig(root);
const bridge = await loadNativeBridge();
const host = await startHost();

try {
  await bridge.createProfileRegistryRecord({
    profileId: "chat-profile",
    lifecycleStatus: "active",
    displayName: "Chat profile",
    defaultSessionKind: "full",
    agentId: "chat-agent",
    activeRuntimeSettingsJson: {},
    sourceAssetRefs: [],
    derivedRuntimeRefs: [],
    importExport: { metadataJson: {} },
    now: new Date().toISOString(),
  });
  const provider = await post("/v1/admin/model-providers", token, {
    alias: "default",
    status: "active",
    protocol: "chat_completions",
    providerKind: "local",
    displayName: "Local GPT",
    baseUrl: "http://127.0.0.1:18082/v1",
    modelId: "gpt",
    contextWindowTokens: 128_000,
    maxOutputTokens: 4_096,
    temperature: 0.5,
    reasoningEffort: "low",
    reasoningFormat: "none",
    chatCompletionsDialect: "qwen",
    thinkingMode: "enabled",
    reasoningHistory: "preserve_all",
    reasoningBudgetTokens: 8_192,
  });
  assert.ok(provider.status === 200 || provider.status === 201);
  const loreLayer = await post("/v1/profile/chat-profile/layers", token, {
    layerId: "chat-world",
    name: "Chat World",
    description: "Context smoke lore layer.",
    purpose: "world",
    writePolicy: "auto_capture",
  });
  assert.equal(loreLayer.status, 200, JSON.stringify(loreLayer.body));
  const character = await post(
    "/v1/admin/roleplay/profiles/chat-profile/characters",
    token,
    {
      id: "chat-hero",
      name: "Chat Hero",
      description: "A context-smoke roleplay character.",
      personality: "careful and curious",
      scenario: "Measuring token segments.",
      firstMessage: "Ready for context diagnostics.",
      alternateGreetings: ["Still ready for diagnostics."],
      exampleMessages: ["We should keep the lore budget visible."],
      tags: ["context"],
    },
  );
  assert.equal(character.status, 200, JSON.stringify(character.body));
  const narrator = await patch(
    "/v1/admin/roleplay/profiles/chat-profile/narrator-config",
    token,
    {
      tone: "wry",
      pacing: "balanced",
      explicitness: "romantic",
      memoryDepth: "deep",
      stylePrompt: "Keep context diagnostics visible and concise.",
      exemplar: "A precise narrator keeps lore in view.",
      review: { enabled: true, maxReviewCycles: 1 },
    },
  );
  assert.equal(narrator.status, 200, JSON.stringify(narrator.body));
  const roleplaySession = await post("/v1/admin/roleplay/sessions", token, {
    sessionId: "chat-session",
    profileId: "chat-profile",
    displayName: "Context Smoke Session",
    characterId: "chat-hero",
    activeLayerIds: ["chat-world"],
  });
  assert.equal(
    roleplaySession.status,
    200,
    JSON.stringify(roleplaySession.body),
  );
  await bridge.appendChatEvent({
    session_id: "chat-session" as SessionId,
    created_at: "2026-06-30T00:59:00Z",
    kind: "provider_status",
    payload: {
      metadata_json: JSON.stringify({
        kind: "context_accounting_snapshot",
        snapshot: {
          schemaVersion: 1,
          sessionId: "chat-session",
          wakeId: "wake-context-smoke",
          logicalTurnId: null,
          executionEpochId: null,
          measuredAt: "2026-06-30T00:59:00Z",
          provider: {
            protocol: "chat_completions",
            providerAlias: "default",
            modelId: "gpt",
          },
          promptProjection: {
            inputTokens: {
              tokens: 1234,
              source: "provider",
              quality: "exact",
            },
            contextWindowTokens: {
              tokens: 128000,
              source: "serialized_estimate",
              quality: "approximate",
              estimator_id: "configured_context_budget_v1",
            },
            protocol_projection: {
              kind: "chat_completions",
              message_count: 4,
              tool_schema_count: 2,
              reasoning_policy: "preserve_all",
            },
            segments: [],
          },
          reservedOutput: {},
          admission: {},
          providerUsage: {
            currentRequest: {},
            logicalWake: {},
            requestCount: 1,
          },
          durableTranscript: {},
          providerState: {},
          compaction: {
            phase: "idle",
            enabled: false,
            auto_compaction_enabled: false,
          },
          diagnostics: [],
        },
      }),
    },
  });
  await bridge.saveContextCompactionArtifact({
    artifact_id: "context_artifact_smoke",
    session_id: "chat-session" as SessionId,
    strategy_id: "rolling_summary_compaction",
    source_refs_json: {
      message_slot_ids: ["slot-smoke"],
      cursor_range: { from: "chat-session:0", to: "chat-session:4" },
    },
    provider_metadata_json: {
      provider_alias: "default",
      model_id: "gpt",
    },
    estimate_before_json: {
      estimator_id: "fallback_chars_words_v1",
      estimated_prompt_tokens: 90_000,
    },
    estimate_after_json: {
      estimated_prompt_tokens: 22_000,
    },
    summary_text:
      "Smoke artifact summary is intentionally not returned by /context.",
    enters_future_context: true,
    context_policy: "summary_context",
    metadata_json: { smoke: true },
    created_at: "2026-06-30T01:00:00Z",
    updated_at: "2026-06-30T01:00:00Z",
  });

  const contextUsage = await get(
    "/v1/chat/sessions/chat-session/context",
    token,
  );
  assert.equal(contextUsage.status, 200);
  assert.equal(contextUsage.body.data.session_id, "chat-session");
  assert.equal(contextUsage.body.data.provider.alias, "default");
  assert.equal(contextUsage.body.data.provider.model_id, "gpt");
  assert.equal(contextUsage.body.data.provider.temperature, 0.5);
  assert.equal(contextUsage.body.data.provider.context_window_tokens, 128_000);
  assert.equal(
    contextUsage.body.data.provider.chat_completions_dialect,
    "qwen",
  );
  assert.equal(contextUsage.body.data.provider.thinking_mode, "enabled");
  assert.equal(
    contextUsage.body.data.provider.reasoning_history,
    "preserve_all",
  );
  assert.equal(contextUsage.body.data.provider.reasoning_budget_tokens, 8_192);
  assert.equal(contextUsage.body.data.provider.thinking_settings_applied, true);
  assert.equal(contextUsage.body.data.provider.thinking_mode_applied, true);
  assert.equal(contextUsage.body.data.provider.reasoning_history_applied, true);
  assert.equal(contextUsage.body.data.provider.reasoning_budget_applied, true);
  assert.equal(contextUsage.body.data.context.estimate_quality, "approximate");
  assert.equal(
    contextUsage.body.data.context.estimator_id,
    "fallback_chars_words_v1",
  );
  assert.equal(contextUsage.body.data.context.reserved_response_tokens, 4_096);
  assert.equal(contextUsage.body.data.context.safety_margin_tokens, 2_560);
  assert.equal(contextUsage.body.data.context.usable_input_tokens, 121_344);
  assert.equal(typeof contextUsage.body.data.context.system_tokens, "number");
  assert.equal(typeof contextUsage.body.data.context.lore_tokens, "number");
  assert.equal(typeof contextUsage.body.data.context.history_tokens, "number");
  assert.ok(contextUsage.body.data.context.system_tokens > 0);
  assert.ok(contextUsage.body.data.context.lore_tokens > 0);
  assert.equal(contextUsage.body.data.context.history_tokens, 0);
  assert.equal(
    contextUsage.body.data.context.estimated_prompt_tokens,
    contextUsage.body.data.context.system_tokens +
      contextUsage.body.data.context.lore_tokens +
      contextUsage.body.data.context.history_tokens,
  );
  assert.equal(
    contextUsage.body.data.context.token_segments.estimate_quality,
    "approximate",
  );
  assert.ok(
    contextUsage.body.data.context.token_segments.notes.some(
      (note: { segment: string; status: string }) =>
        note.segment === "lore" && note.status === "estimated",
    ),
  );
  assert.equal(typeof contextUsage.body.data.brain.backend, "string");
  assert.equal(
    contextUsage.body.data.context_strategy.strategy_id,
    "recent_window",
  );
  assert.equal(
    contextUsage.body.data.context_strategy.auto_compaction_enabled,
    false,
  );
  assert.equal(contextUsage.body.data.native_snapshot.schemaVersion, 1);
  assert.equal(
    contextUsage.body.data.native_snapshot.promptProjection.inputTokens.tokens,
    1234,
  );
  assert.equal(
    contextUsage.body.data.native_snapshot.providerUsage.requestCount,
    1,
  );
  assert.equal(
    contextUsage.body.data.latest_compaction_artifact.artifact_id,
    "context_artifact_smoke",
  );
  assert.equal(
    contextUsage.body.data.latest_compaction_artifact.strategy_id,
    "rolling_summary_compaction",
  );
  assert.equal(
    contextUsage.body.data.latest_compaction_artifact.enters_future_context,
    true,
  );
  assert.equal(
    contextUsage.body.data.latest_compaction_artifact.summary_text,
    undefined,
  );

  const commandCatalog = await get("/v1/chat/commands", token);
  assert.equal(commandCatalog.status, 200);
  assert.ok(
    commandCatalog.body.data.commands.some(
      (item: { name: string }) => item.name === "model",
    ),
  );

  const modelCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/model",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(modelCommand.status, 200);
  assert.equal(modelCommand.body.data.status, "completed");
  assert.equal(modelCommand.body.data.command_name, "model");
  assert.equal(modelCommand.body.data.response.fields.providerAlias, "default");
  assert.equal(modelCommand.body.data.response.fields.modelId, "gpt");
  assert.equal(
    modelCommand.body.data.response.fields.contextWindowTokens,
    128_000,
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        contextSession: contextUsage.body.data.session_id,
        modelProvider: modelCommand.body.data.response.fields.providerAlias,
        brainBackend: modelCommand.body.data.response.fields.brainBackend,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

async function startHost() {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "0",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "0",
    },
    bridge,
  });
}

async function get(path: string, bearer?: string) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: bearer ? { authorization: `Bearer ${bearer}` } : undefined,
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function post(path: string, bearer: string, body: unknown) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${bearer}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function patch(path: string, bearer: string, body: unknown) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "PATCH",
    headers: {
      authorization: `Bearer ${bearer}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

function writeRuntimeConfig(dataRoot: string): void {
  const configDir = join(dataRoot, "config");
  const profilesDir = join(configDir, "profiles");
  const skillsDir = join(configDir, "skills");
  mkdirSync(profilesDir, { recursive: true });
  mkdirSync(skillsDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        skillsDir,
        brains: [{ profileId: "chat-profile" }],
        sessions: [],
        mcpBindings: [],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "chat-profile.json"),
    JSON.stringify(
      {
        profileId: "chat-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
          baseUrl: "http://127.0.0.1:18082/v1",
        },
        prompt: {
          system: "Chat profile system prompt.",
          instructions: ["Answer concisely."],
        },
        localToolProfileId: "full-agent",
        toolPolicy: {
          requestedToolsets: ["session", "filesystem"],
          requestedTools: ["session_search"],
        },
      },
      null,
      2,
    ),
  );
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createTcpServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const open = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(open);
      });
    });
  });
}
