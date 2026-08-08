import { createHash } from "node:crypto";
import type {
  BrainProviderStateScope,
  BrainStrategyMetadata,
  ProviderStateCompatibilityFacts,
} from "@rusty-crew/contracts";
import type { BrainModuleStrategyMetadata } from "./brain-catalog.js";
import type { LoadedProfileContext } from "./profile-loading.js";

export interface ProviderStateFingerprintInput {
  profile: LoadedProfileContext;
  strategy: BrainStrategyMetadata;
  moduleStrategy?: BrainModuleStrategyMetadata;
}

export interface ProviderStateFingerprintMaterial {
  profile: unknown;
  provider: unknown;
  compatibility: Record<
    keyof Omit<ProviderStateCompatibilityFacts, "version">,
    unknown
  >;
}

export function providerStateScopeForProfile(
  input: ProviderStateFingerprintInput,
): BrainProviderStateScope {
  const material = providerStateFingerprintMaterial(input);
  const compatibility: ProviderStateCompatibilityFacts = {
    version: "1",
    profileIdentity: stableFingerprint(material.compatibility.profileIdentity),
    displayMetadata: stableFingerprint(material.compatibility.displayMetadata),
    prompt: stableFingerprint(material.compatibility.prompt),
    skills: stableFingerprint(material.compatibility.skills),
    toolCatalog: stableFingerprint(material.compatibility.toolCatalog),
    providerEndpoint: stableFingerprint(
      material.compatibility.providerEndpoint,
    ),
    model: stableFingerprint(material.compatibility.model),
    protocol: stableFingerprint(material.compatibility.protocol),
    dialect: stableFingerprint(material.compatibility.dialect),
    reasoningSemantics: stableFingerprint(
      material.compatibility.reasoningSemantics,
    ),
    brainModule: stableFingerprint(material.compatibility.brainModule),
    brainStrategy: stableFingerprint(material.compatibility.brainStrategy),
    providerStateSchema: stableFingerprint(
      material.compatibility.providerStateSchema,
    ),
  };
  return {
    // Profile identity is deliberately narrow. Display, prompts, skills,
    // tools, effort, and workspace are compatible refresh dimensions.
    profileFingerprint: compatibility.profileIdentity,
    providerFingerprint: stableFingerprint({
      version: compatibility.version,
      profileIdentity: compatibility.profileIdentity,
      providerEndpoint: compatibility.providerEndpoint,
      model: compatibility.model,
      protocol: compatibility.protocol,
      dialect: compatibility.dialect,
      reasoningSemantics: compatibility.reasoningSemantics,
      brainModule: compatibility.brainModule,
      brainStrategy: compatibility.brainStrategy,
      providerStateSchema: compatibility.providerStateSchema,
    }),
    compatibility,
  };
}

export function providerStateFingerprintMaterial(
  input: ProviderStateFingerprintInput,
): ProviderStateFingerprintMaterial {
  const profile = input.profile.profile;
  const moduleFingerprints = input.moduleStrategy?.fingerprints;
  const compatibility: ProviderStateFingerprintMaterial["compatibility"] = {
    profileIdentity: { profileId: profile.profileId },
    displayMetadata: {
      displayName: profile.displayName,
    },
    prompt: {
      prompt: profile.prompt,
      roleplayMechanic: profile.roleplayMechanic,
      moduleOptions: moduleFingerprints?.profileOptions,
    },
    skills: input.profile.skills.map((skill) => ({
      slug: skill.slug,
      title: skill.title,
      summary: skill.summary,
      tags: skill.tags,
      bodyMarkdown: skill.bodyMarkdown,
    })),
    toolCatalog: {
      catalogId: input.profile.toolSelection.catalogId,
      selectedTools: input.profile.toolSelection.inventory.selectedTools.map(
        (tool) => ({
          name: tool.name,
          version: tool.version,
          outputShape: tool.outputShape,
          category: tool.category,
          safety: tool.safety,
          surfaces: tool.surfaces,
        }),
      ),
    },
    providerEndpoint: {
      provider: profile.modelConfig.provider,
      baseUrl: profile.modelConfig.baseUrl,
      apiKeyEnv: profile.modelConfig.apiKeyEnv,
    },
    model: { modelName: profile.modelConfig.modelName },
    protocol: { api: profile.modelConfig.api },
    dialect: {
      responsesDialect: profile.modelConfig.responsesDialect,
      chatCompletionsDialect: profile.modelConfig.chatCompletionsDialect,
    },
    reasoningSemantics: {
      reasoningFormat: profile.modelConfig.reasoningFormat,
      thinkingMode: profile.modelConfig.thinkingMode,
      reasoningHistory: profile.modelConfig.reasoningHistory,
      promptCaching: profile.modelConfig.promptCaching,
    },
    brainModule: {
      moduleId: input.strategy.moduleId,
    },
    brainStrategy: {
      strategyId: input.strategy.strategyId,
    },
    providerStateSchema: {
      mode: input.strategy.providerState.mode,
      moduleOptions: moduleFingerprints?.providerOptions,
    },
  };
  return {
    profile: compatibility.profileIdentity,
    provider: {
      providerEndpoint: compatibility.providerEndpoint,
      model: compatibility.model,
      protocol: compatibility.protocol,
      dialect: compatibility.dialect,
      reasoningSemantics: compatibility.reasoningSemantics,
      brainModule: compatibility.brainModule,
      brainStrategy: compatibility.brainStrategy,
      providerStateSchema: compatibility.providerStateSchema,
    },
    compatibility,
  };
}

export function stableFingerprint(value: unknown): string {
  return createHash("sha256").update(stableJson(value)).digest("hex");
}

export function stableJson(value: unknown): string {
  return JSON.stringify(sortJsonValue(value));
}

function sortJsonValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortJsonValue);
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
