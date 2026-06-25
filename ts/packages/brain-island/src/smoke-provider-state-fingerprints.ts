import assert from "node:assert/strict";
import type { BrainStrategyMetadata, ProfileId } from "@rusty-crew/contracts";
import type { BrainModuleStrategyMetadata } from "./brain-module.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import { providerStateScopeForProfile } from "./provider-state-fingerprints.js";

const strategy: BrainStrategyMetadata = {
  moduleId: "openai-responses",
  strategyId: "replay",
  providerState: { mode: "optional" },
};
const profileId = "rusty-crew-runner" as ProfileId;

const moduleStrategy: BrainModuleStrategyMetadata = {
  strategyId: "replay",
  providerState: { mode: "optional" },
  fingerprints: {
    profileOptions: { instructionsVersion: 1 },
    providerOptions: { reasoningEffort: "medium" },
  },
};

const baseline = profileContext();
const sameAgain = profileContext();

const first = providerStateScopeForProfile({
  profile: baseline,
  strategy,
  moduleStrategy,
});
const second = providerStateScopeForProfile({
  profile: sameAgain,
  strategy,
  moduleStrategy,
});
assert.deepEqual(second, first, "same profile/config should hash identically");

const promptChanged = providerStateScopeForProfile({
  profile: profileContext({
    prompt: {
      system: "different system",
      soulMarkdown: "same soul",
      memoryMarkdown: "same memory",
    },
  }),
  strategy,
  moduleStrategy,
});
assert.notEqual(
  promptChanged.profileFingerprint,
  first.profileFingerprint,
  "prompt material should change the profile fingerprint",
);
assert.equal(
  promptChanged.providerFingerprint,
  first.providerFingerprint,
  "prompt material should not change the provider fingerprint",
);

const toolsChanged = providerStateScopeForProfile({
  profile: profileContext({ toolName: "search_docs" }),
  strategy,
  moduleStrategy,
});
assert.notEqual(
  toolsChanged.profileFingerprint,
  first.profileFingerprint,
  "tool identity should change the profile fingerprint",
);
assert.equal(
  toolsChanged.providerFingerprint,
  first.providerFingerprint,
  "tool identity should not change the provider fingerprint",
);

const providerChanged = providerStateScopeForProfile({
  profile: profileContext({
    modelConfig: {
      provider: "openai",
      modelName: "gpt-5.1",
      baseUrl: "https://api.openai.com/v1",
      api: "responses",
      apiKeyEnv: "OPENAI_API_KEY",
      temperatureMilli: 200,
      maxOutputTokens: 4096,
    },
  }),
  strategy,
  moduleStrategy,
});
assert.equal(
  providerChanged.profileFingerprint,
  first.profileFingerprint,
  "provider settings should not change the profile fingerprint",
);
assert.notEqual(
  providerChanged.providerFingerprint,
  first.providerFingerprint,
  "provider settings should change the provider fingerprint",
);

const profileOptionChanged = providerStateScopeForProfile({
  profile: baseline,
  strategy,
  moduleStrategy: {
    ...moduleStrategy,
    fingerprints: {
      ...moduleStrategy.fingerprints,
      profileOptions: { instructionsVersion: 2 },
    },
  },
});
assert.notEqual(
  profileOptionChanged.profileFingerprint,
  first.profileFingerprint,
  "module-declared profile options should change profile fingerprint",
);
assert.equal(
  profileOptionChanged.providerFingerprint,
  first.providerFingerprint,
  "module-declared profile options should not change provider fingerprint",
);

const providerOptionChanged = providerStateScopeForProfile({
  profile: baseline,
  strategy,
  moduleStrategy: {
    ...moduleStrategy,
    fingerprints: {
      ...moduleStrategy.fingerprints,
      providerOptions: { reasoningEffort: "high" },
    },
  },
});
assert.equal(
  providerOptionChanged.profileFingerprint,
  first.profileFingerprint,
  "module-declared provider options should not change profile fingerprint",
);
assert.notEqual(
  providerOptionChanged.providerFingerprint,
  first.providerFingerprint,
  "module-declared provider options should change provider fingerprint",
);

console.log("provider state fingerprint smoke passed");

function profileContext(
  overrides: {
    prompt?: LoadedProfileContext["profile"]["prompt"];
    modelConfig?: LoadedProfileContext["profile"]["modelConfig"];
    toolName?: string;
  } = {},
): LoadedProfileContext {
  const toolName = overrides.toolName ?? "read_file";
  return {
    profile: {
      profileId,
      displayName: "Rusty Crew Runner",
      modelConfig:
        overrides.modelConfig ??
        ({
          provider: "openai",
          modelName: "gpt-5",
          baseUrl: "https://api.openai.com/v1",
          api: "responses",
          apiKeyEnv: "OPENAI_API_KEY",
          temperatureMilli: 200,
          maxOutputTokens: 2048,
        } satisfies LoadedProfileContext["profile"]["modelConfig"]),
      prompt:
        overrides.prompt ??
        ({
          system: "baseline system",
          soulMarkdown: "same soul",
          memoryMarkdown: "same memory",
        } satisfies LoadedProfileContext["profile"]["prompt"]),
    },
    skills: [
      {
        slug: "planning",
        title: "Planning",
        summary: "Plan carefully",
        tags: ["planning"],
        bodyMarkdown: "Plan carefully.",
        sourcePath: "/profiles/skills/planning.md",
      },
    ],
    toolSelection: {
      profileId,
      catalogId: "service:mcp:rusty-crew-runner",
      inventory: {
        selectedTools: [],
        selectedBindings: [],
        selectedDescriptors: [],
        items: [],
      },
      toolProfile: {
        tools: [
          {
            name: toolName,
            description: `${toolName} tool`,
          },
        ],
      },
    },
  };
}
