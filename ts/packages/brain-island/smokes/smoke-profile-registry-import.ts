import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AdapterId,
  AgentId,
  BrainImplementationId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { buildProfileRegistryImportPlan } from "../src/profile-registry-import.js";
import type { RustyCrewRuntimeConfig } from "../src/service-runtime-config.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-registry-import-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
  const runnerDir = join(profilesDir, "rusty-crew-runner");
  mkdirSync(join(runnerDir, "skills", "local-skill"), { recursive: true });
  mkdirSync(join(runnerDir, "templates"), { recursive: true });
  writeFileSync(
    join(runnerDir, "profile.yaml"),
    `name: "Rusty Crew Runner"
displayName: "Rusty Crew Runner"
profileIdentity: rusty-crew-runner
unsupportedFutureField: keep-me-visible
skills: all
modelConfig:
  provider: den-router
  model: deepseek-flash
  baseUrl: http://127.0.0.1:18082/v1
  api: openai-completions
  temperature: 0.2
  maxTokens: 4096
brain:
  module: chat-completions-core
  strategy: default
mcpConfig:
  bindingId: runner-mcp
  endpointRef: den-core
  serverNames:
    - den
  transport: streamable_http
  toolProfile: runner
runtimeConfig:
  maxIterations: 100
  maxTokensPerTurn: 8192
  maxDurationMs: 900000
toolPolicy:
  mode: allow_all
memoryConfig:
  enabled: true
  denMemory: true
  denseProfileMemory: true
sessionDefaults:
  ownerId: "owner:den-k8plus:rusty-crew-runner"
  maxHistoryMessages: 200
channelDefaults:
  wakePolicy: subscription
backgroundReview:
  enabled: true
  reviewType: combined
  schedule: "0 3 * * *"
`,
  );
  writeFileSync(
    join(runnerDir, "soul.md"),
    "You are Rusty Crew Runner.\n\nHandle implementation work.",
  );
  writeFileSync(join(runnerDir, "memory.md"), "Piper is the project lead.");
  writeFileSync(
    join(runnerDir, "skills", "local-skill", "SKILL.md"),
    "---\nname: local-skill\n---\nUse local profile skill material.",
  );
  writeFileSync(join(runnerDir, "templates", "starter.md"), "Template text.");

  writeFileSync(
    join(profilesDir, "flat-coder.json"),
    JSON.stringify(
      {
        profileId: "flat-coder",
        displayName: "Flat Coder",
        modelConfig: {
          provider: "den-router",
          modelName: "local-deterministic",
        },
        prompt: {
          system: "You are a flat profile coder.",
        },
        runtime: {
          maxTurns: 3,
        },
        roleplayMechanic: { autoMonitor: false },
        futureKnob: true,
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "normalized.json"),
    JSON.stringify({
      profileId: "normalized",
      modelConfigId: "normalized-model",
    }),
  );

  const bridge = await loadNativeBridge();
  const runtimeConfig: RustyCrewRuntimeConfig = {
    profilesDir,
    skillsDir,
    brains: [
      {
        implementationId: "rusty-crew-runner-brain" as BrainImplementationId,
        profileId: "rusty-crew-runner" as ProfileId,
      },
    ],
    sessions: [
      {
        sessionId: "rusty-crew-runner-session" as SessionId,
        agentId: "rusty-crew-runner" as AgentId,
        profileId: "rusty-crew-runner" as ProfileId,
        kind: "full",
        workspaceCwd: "/home/dev/rusty-crew",
        ownerId: "owner:den-k8plus:rusty-crew-runner",
        maxHistoryMessages: 200,
      },
    ],
    scheduledJobs: [
      {
        id: "background-review-rusty-crew-runner",
        schedule: "0 3 * * *",
        shape: "host_job",
        jobKind: "runtime_review.memory_skills",
      },
    ],
    channelBindings: [
      {
        bindingId: "runner-channel",
        adapterId: "den-channels" as AdapterId,
        provider: "den_channels",
        agentId: "rusty-crew-runner" as AgentId,
        sessionId: "rusty-crew-runner-session" as SessionId,
        profileId: "rusty-crew-runner" as ProfileId,
        externalChannelId: "40",
        conversationProjectId: "rusty-crew",
        conversationChannelId: 40,
        status: "active",
      },
    ],
    mcpBindings: [
      {
        bindingId: "runner-mcp",
        adapterId: "den-mcp" as AdapterId,
        agentId: "rusty-crew-runner" as AgentId,
        sessionId: "rusty-crew-runner-session" as SessionId,
        profileId: "rusty-crew-runner" as ProfileId,
        serverNames: ["den"],
        endpointRef: "den-core",
        transport: "streamable_http",
        toolProfileKey: "runner",
        status: "active",
        diagnostics: {},
      },
    ],
  };

  const runnerPlan = await buildProfileRegistryImportPlan({
    profilesDir,
    profileId: "rusty-crew-runner" as ProfileId,
    mode: "activation",
    now: "2026-06-26T08:00:00Z",
    runtimeConfig,
    bridge,
  });

  assert.equal(runnerPlan.mode, "activation");
  assert.equal(runnerPlan.activatesRuntime, true);
  assert.equal(runnerPlan.sourceFormat, "directory_yaml");
  assert.equal(runnerPlan.registryWrite.lifecycleStatus, "active");
  assert.equal(runnerPlan.registryWrite.defaultSessionKind, "full");
  assert.equal(
    runnerPlan.registryWrite.ownerId,
    "owner:den-k8plus:rusty-crew-runner",
  );
  assert.equal(
    runnerPlan.registryWrite.promptSoulMarkdown,
    "You are Rusty Crew Runner.\n\nHandle implementation work.",
  );
  assert.equal(
    runnerPlan.registryWrite.promptMemoryMarkdown,
    "Piper is the project lead.",
  );
  assert.equal(
    runnerPlan.registryWrite.activeRuntimeSettingsJson.modelConfig &&
      typeof runnerPlan.registryWrite.activeRuntimeSettingsJson.modelConfig,
    "object",
  );
  assert.equal(
    runnerPlan.registryWrite.activeRuntimeSettingsJson
      .externalMessageDeliveryPolicy,
    "immediate_steer",
  );
  assert.equal(
    runnerPlan.registryWrite.derivedRuntimeRefs.some(
      (ref) => ref.refKind === "session",
    ),
    true,
  );
  assert.equal(
    runnerPlan.registryWrite.sourceAssetRefs.some(
      (ref) =>
        ref.assetKind === "soul_md" && ref.contentHash?.startsWith("sha256:"),
    ),
    true,
  );
  assert.equal(
    runnerPlan.registryWrite.sourceAssetRefs.some(
      (ref) => ref.assetKind === "profile_local_skill",
    ),
    true,
  );
  assert.equal(
    runnerPlan.registryWrite.sourceAssetRefs.some(
      (ref) => ref.assetKind === "template_file",
    ),
    true,
  );
  assert.equal(
    runnerPlan.diagnostics.some(
      (diagnostic) =>
        diagnostic.code === "unsupported_profile_field" &&
        diagnostic.path === "unsupportedFutureField",
    ),
    true,
  );
  assert.equal(
    runnerPlan.diagnostics.some(
      (diagnostic) => diagnostic.severity === "error",
    ),
    false,
    JSON.stringify(runnerPlan.diagnostics),
  );

  const flatPlan = await buildProfileRegistryImportPlan({
    profilesDir,
    profileId: "flat-coder" as ProfileId,
    now: "2026-06-26T08:05:00Z",
  });
  assert.equal(flatPlan.mode, "template");
  assert.equal(flatPlan.activatesRuntime, false);
  assert.equal(flatPlan.sourceFormat, "flat_json");
  assert.equal(flatPlan.registryWrite.lifecycleStatus, "paused");
  assert.equal(flatPlan.registryWrite.derivedRuntimeRefs.length, 0);
  assert.equal(
    flatPlan.registryWrite.sourceAssetRefs[0]?.assetKind,
    "profile_json",
  );
  assert.deepEqual(
    flatPlan.registryWrite.sourceAssetRefs.map((ref) => ref.assetKind),
    ["profile_json"],
  );
  assert.deepEqual(
    flatPlan.registryWrite.activeRuntimeSettingsJson.roleplayMechanic,
    { autoMonitor: false },
  );
  assert.equal(
    flatPlan.diagnostics.some(
      (diagnostic) =>
        diagnostic.code === "unsupported_profile_field" &&
        diagnostic.path === "futureKnob",
    ),
    true,
  );

  const normalizedPlan = await buildProfileRegistryImportPlan({
    profilesDir,
    profileId: "normalized" as ProfileId,
    now: "2026-06-26T08:06:00Z",
    bridge: {
      validateRuntimeConfigDraft: async () => ({
        ok: true,
        diagnostics: [],
        normalized: {
          profilesDir,
          brains: [],
          sessions: [],
          scheduledJobs: [],
          channelBindings: [],
          mcpBindings: [],
        },
      }),
      getModelConfiguration: async (modelConfigId) => ({
        modelConfigId,
        endpointId: "normalized-endpoint",
        status: "active",
        modelId: "normalized/model",
        reasoningHistory: "provider_default",
        thinkingMode: "provider_default",
        promptCachingPolicy: "disabled",
        capabilities: { version: 1, imageInput: false },
        metadataJson: {},
        revision: 2,
        createdAt: "2026-06-26T08:00:00Z",
        updatedAt: "2026-06-26T08:00:00Z",
      }),
      getModelEndpoint: async (endpointId) => ({
        endpointId,
        status: "active",
        baseUrl: "https://example.invalid/v1",
        protocol: "responses",
        wireDialect: "openai_stateful",
        authScheme: "openai_codex_oauth",
        credentialId: "oauth-credential",
        promptCacheTransport: "none",
        metadataJson: {},
        revision: 3,
        createdAt: "2026-06-26T08:00:00Z",
        updatedAt: "2026-06-26T08:00:00Z",
      }),
      getServiceCredential: async (credentialId) => ({
        credentialId,
        displayName: "OAuth credential",
        providerKind: "display-only",
        credentialKind: "openai_oauth",
        credential: { hasSecret: false },
        linkedProviderAliases: [],
        revision: 4,
        createdAt: "2026-06-26T08:00:00Z",
        updatedAt: "2026-06-26T08:00:00Z",
      }),
    },
  });
  assert.equal(
    normalizedPlan.diagnostics.some(
      (diagnostic) => diagnostic.code === "service_credential_secret_missing",
    ),
    true,
  );

  console.log(
    JSON.stringify(
      {
        runnerAssets: runnerPlan.registryWrite.sourceAssetRefs.map(
          (ref) => ref.assetKind,
        ),
        flatMode: flatPlan.mode,
        runnerDiagnostics: runnerPlan.diagnostics.map(
          (diagnostic) => diagnostic.code,
        ),
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
