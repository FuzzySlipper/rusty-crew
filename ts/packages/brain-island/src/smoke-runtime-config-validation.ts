import assert from "node:assert/strict";

import type {
  AdapterId,
  AgentId,
  BrainImplementationId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type { ProfileConfig } from "./profile-loading.js";
import {
  planCreateProfileWithRust,
  runtimeConfigValidationInput,
  validateRuntimeConfigWithRust,
} from "./runtime-config-validation.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";

const bridge = await loadNativeBridge();

const profileId = "runtime-validator" as ProfileId;
const brainImplementationId =
  "runtime-validator-brain" as BrainImplementationId;
const sessionId = "runtime-validator-session" as SessionId;
const agentId = "runtime-validator" as AgentId;
const channelAdapterId = "den-channels" as AdapterId;
const mcpAdapterId = "den-mcp" as AdapterId;

const profile: ProfileConfig = {
  profileId,
  modelConfig: {
    provider: "den-router",
    modelName: "local-deterministic",
  },
  brain: {
    module: "local",
    strategy: "default",
  },
  runtime: {
    defaultResourceLimits: {
      workdir: "/tmp/rusty-crew-runtime-validator",
      maxDurationMs: 60_000,
      maxDelegationDepth: 4,
    },
    maxTurnDurationMs: 30_000,
    maxTokensPerTurn: 2048,
  },
  sessionDefaults: {
    ownerId: "smoke",
    maxHistoryMessages: 128,
    turnTimeoutMs: 30_000,
  },
  mcpConfig: {
    bindingId: "runtime-validator-mcp",
    endpointRef: "den-core",
    serverNames: ["den"],
    transport: "streamable_http",
    toolProfile: "runner",
  },
  backgroundReview: {
    enabled: true,
    reviewType: "memory",
    schedule: "0 3 * * *",
  },
  channelDefaults: {
    wakePolicy: "subscription",
  },
};

const runtimeConfig: RustyCrewRuntimeConfig = {
  profilesDir: "/tmp/rusty-crew/profiles",
  skillsDir: "/tmp/rusty-crew/skills",
  brains: [
    {
      implementationId: brainImplementationId,
      profileId,
    },
  ],
  sessions: [
    {
      sessionId,
      agentId,
      profileId,
      kind: "full",
      resourceLimits: {
        workdir: "/tmp/rusty-crew-runtime-validator",
        maxDurationMs: 60_000,
        maxDelegationDepth: 4,
      },
      ownerId: "smoke",
      maxHistoryMessages: 128,
      turnTimeoutMs: 30_000,
    },
  ],
  scheduledJobs: [
    {
      id: "runtime-validator-wake",
      schedule: "*/5 * * * *",
      shape: "session_wake",
      targetSessionId: sessionId,
    },
    {
      id: "runtime-validator-review",
      schedule: "0 3 * * *",
      shape: "host_job",
      jobKind: "runtime_review.memory_skills",
    },
  ],
  channelBindings: [
    {
      bindingId: "runtime-validator-channel",
      adapterId: channelAdapterId,
      provider: "den_channels",
      agentId,
      sessionId,
      profileId,
      externalChannelId: "40",
      conversationProjectId: "rusty-crew",
      conversationChannelId: 40,
      status: "active",
    },
  ],
  mcpBindings: [
    {
      bindingId: "runtime-validator-mcp",
      adapterId: mcpAdapterId,
      agentId,
      sessionId,
      profileId,
      serverNames: ["den"],
      endpointRef: "den-core",
      transport: "streamable_http",
      toolProfileKey: "runner",
      status: "active",
      diagnostics: {},
    },
  ],
};

const valid = await validateRuntimeConfigWithRust({
  bridge,
  runtimeConfig,
  profiles: [profile],
});
assert.deepEqual(valid.diagnostics, []);

const input = runtimeConfigValidationInput(runtimeConfig, [profile]);
const invalid = await bridge.validateRuntimeConfigDraft({
  ...input,
  runtimeConfig: {
    ...input.runtimeConfig,
    sessions: [
      ...input.runtimeConfig.sessions,
      {
        ...input.runtimeConfig.sessions[0]!,
        sessionId: "runtime-validator-session",
        agentId: "runtime-validator-shadow",
      },
    ],
    mcpBindings: [
      {
        ...input.runtimeConfig.mcpBindings[0]!,
        bindingId: "bad mcp binding",
        serverNames: [],
        status: "disconnected",
      },
    ],
  },
});

assert(
  invalid.diagnostics.some(
    (diagnostic) =>
      diagnostic.severity === "error" &&
      diagnostic.code === "duplicate_session_id" &&
      diagnostic.path === "sessions[1].sessionId" &&
      diagnostic.message.includes("duplicate session"),
  ),
);
assert(
  invalid.diagnostics.some(
    (diagnostic) =>
      diagnostic.severity === "error" &&
      diagnostic.code === "invalid_binding_id" &&
      diagnostic.path === "mcpBindings[0].bindingId",
  ),
);
assert(
  invalid.diagnostics.some(
    (diagnostic) =>
      diagnostic.severity === "error" &&
      diagnostic.code === "mcp_binding_missing_server_names" &&
      diagnostic.path === "mcpBindings[0].serverNames",
  ),
);

const createPlan = await planCreateProfileWithRust({
  bridge,
  runtimeConfig,
  profiles: [profile],
  request: {
    profileId: "field-created-profile",
    displayName: "Field Created Profile",
    profileFileExists: false,
  },
});
assert.deepEqual(createPlan.diagnostics, []);
assert.equal(createPlan.profileSeed?.profileId, "field-created-profile");
assert.equal(createPlan.profileSeed?.displayName, "Field Created Profile");
assert.equal(createPlan.profileSeed?.modelConfig.provider, "local");
assert.equal(createPlan.profileSeed?.modelConfig.modelName, "deterministic");
assert.equal(createPlan.profileSeed?.brain.module, "local");
assert.equal(createPlan.profileSeed?.skillsMode, "all");
assert.equal(
  createPlan.runtimeBrain?.implementationId,
  "field-created-profile-brain",
);
assert.equal(
  createPlan.runtimeSession?.sessionId,
  "field-created-profile-session",
);
assert.equal(createPlan.profileMcpConfig?.toolProfile, "field-created-profile");

const duplicatePlan = await planCreateProfileWithRust({
  bridge,
  runtimeConfig,
  profiles: [profile],
  request: {
    profileId: profileId,
    agentId: agentId,
    sessionId: sessionId,
    implementationId: brainImplementationId,
    profileFileExists: true,
  },
});
assert(
  duplicatePlan.diagnostics.some(
    (diagnostic) =>
      diagnostic.code === "duplicate_session_id" &&
      diagnostic.path === "request.sessionId",
  ),
);
assert.equal(duplicatePlan.profileSeed, undefined);

const invalidCreatePlan = await planCreateProfileWithRust({
  bridge,
  runtimeConfig,
  profiles: [profile],
  request: {
    profileId: "../bad",
    mcpToolProfile: "bad tool",
    profileFileExists: false,
  },
});
assert(
  invalidCreatePlan.diagnostics.some(
    (diagnostic) =>
      diagnostic.code === "invalid_profile_id" &&
      diagnostic.path === "request.profileId",
  ),
);
assert.equal(invalidCreatePlan.runtimeSession, undefined);

console.log("runtime config validation native bridge smoke passed");
