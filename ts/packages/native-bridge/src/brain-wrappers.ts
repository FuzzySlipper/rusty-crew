import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type {
  NativeBrainCatalog,
  NativeBrainSelectionPlan,
  NativeBridgeModule,
} from "./public-api.js";

export function createNativeBridgeBrainCatalogMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, "brainCatalog" | "planBrainSelection"> {
  return {
    brainCatalog: async () =>
      JSON.parse(binding.brainCatalogJson()) as NativeBrainCatalog,
    planBrainSelection: async (input) =>
      JSON.parse(
        binding.planBrainSelectionJson(
          JSON.stringify({
            ...(input.configuredModuleId === undefined
              ? {}
              : { configured_module_id: input.configuredModuleId }),
            ...(input.configuredStrategyId === undefined
              ? {}
              : { configured_strategy_id: input.configuredStrategyId }),
            provider_protocol: input.providerProtocol,
            provider_kind: input.providerKind,
            roleplay_narrator_enabled: input.roleplayNarratorEnabled ?? false,
          }),
        ),
      ) as NativeBrainSelectionPlan,
  };
}
