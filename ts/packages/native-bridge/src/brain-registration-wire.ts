import type { BrainImplementationRegistration } from "@rusty-crew/contracts";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";

type NativeBrainRegistration = Parameters<
  NativeBridgeBinding["registerBrainImplementation"]
>[0];

export function toNativeBrainRegistration(
  registration: BrainImplementationRegistration,
): NativeBrainRegistration {
  const compatibility = registration.providerStateScope?.compatibility;
  return {
    implementationId: registration.implementationId,
    profileId: registration.profileId,
    toolProfile: {
      tools: registration.toolProfile.tools.map((tool) => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema ?? undefined,
      })),
    },
    modelConfig: {
      provider: registration.modelConfig.provider,
      modelName: registration.modelConfig.modelName,
      temperatureMilli: registration.modelConfig.temperatureMilli,
      maxOutputTokens: registration.modelConfig.maxOutputTokens,
    },
    strategy: registration.strategy
      ? {
          moduleId: registration.strategy.moduleId,
          strategyId: registration.strategy.strategyId,
          providerState: { mode: registration.strategy.providerState.mode },
        }
      : undefined,
    providerStateScope: registration.providerStateScope
      ? {
          profileFingerprint:
            registration.providerStateScope.profileFingerprint,
          providerFingerprint:
            registration.providerStateScope.providerFingerprint,
          compatibility: compatibility
            ? {
                version: compatibility.version,
                profileIdentity: compatibility.profileIdentity,
                displayMetadata: compatibility.displayMetadata,
                prompt: compatibility.prompt,
                skills: compatibility.skills,
                toolCatalog: compatibility.toolCatalog,
                providerEndpoint: compatibility.providerEndpoint,
                model: compatibility.model,
                protocol: compatibility.protocol,
                dialect: compatibility.dialect,
                reasoningSemantics: compatibility.reasoningSemantics,
                brainModule: compatibility.brainModule,
                brainStrategy: compatibility.brainStrategy,
                providerStateSchema: compatibility.providerStateSchema,
              }
            : undefined,
        }
      : undefined,
  };
}
