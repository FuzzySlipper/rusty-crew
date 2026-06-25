import { createHash } from "node:crypto";
import type { BrainProviderStateScope } from "@rusty-crew/contracts";
import type { BrainModuleStrategyMetadata } from "./brain-module.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import type { BrainStrategyMetadata } from "@rusty-crew/contracts";

export interface ProviderStateFingerprintInput {
  profile: LoadedProfileContext;
  strategy: BrainStrategyMetadata;
  moduleStrategy?: BrainModuleStrategyMetadata;
}

export interface ProviderStateFingerprintMaterial {
  profile: unknown;
  provider: unknown;
}

export function providerStateScopeForProfile(
  input: ProviderStateFingerprintInput,
): BrainProviderStateScope {
  const material = providerStateFingerprintMaterial(input);
  return {
    profileFingerprint: stableFingerprint(material.profile),
    providerFingerprint: stableFingerprint(material.provider),
  };
}

export function providerStateFingerprintMaterial(
  input: ProviderStateFingerprintInput,
): ProviderStateFingerprintMaterial {
  const profile = input.profile.profile;
  const moduleFingerprints = input.moduleStrategy?.fingerprints;
  return {
    profile: {
      profileId: profile.profileId,
      moduleId: input.strategy.moduleId,
      strategyId: input.strategy.strategyId,
      prompt: profile.prompt,
      skills: input.profile.skills.map((skill) => ({
        slug: skill.slug,
        title: skill.title,
        summary: skill.summary,
        tags: skill.tags,
        bodyMarkdown: skill.bodyMarkdown,
      })),
      toolIdentity: {
        catalogId: input.profile.toolSelection.catalogId,
        toolProfile: input.profile.toolSelection.toolProfile,
      },
      moduleOptions: moduleFingerprints?.profileOptions,
    },
    provider: {
      moduleId: input.strategy.moduleId,
      strategyId: input.strategy.strategyId,
      providerStateMode: input.strategy.providerState.mode,
      provider: profile.modelConfig.provider,
      modelName: profile.modelConfig.modelName,
      baseUrl: profile.modelConfig.baseUrl,
      api: profile.modelConfig.api,
      apiKeyEnv: profile.modelConfig.apiKeyEnv,
      modelSettings: {
        temperatureMilli: profile.modelConfig.temperatureMilli,
        maxOutputTokens: profile.modelConfig.maxOutputTokens,
      },
      moduleOptions: moduleFingerprints?.providerOptions,
    },
  };
}

export function stableFingerprint(value: unknown): string {
  return createHash("sha256").update(stableJson(value)).digest("hex");
}

export function stableJson(value: unknown): string {
  return JSON.stringify(sortJsonValue(value));
}

function sortJsonValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sortJsonValue);
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([, entry]) => entry !== undefined)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, entry]) => [key, sortJsonValue(entry)]),
    );
  }
  return value;
}
