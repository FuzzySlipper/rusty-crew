import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type RuntimeActivityMethodName =
  | "beginRuntimeActivity"
  | "progressRuntimeActivity"
  | "finishRuntimeActivity"
  | "runtimeActivityCensus";

export function createNativeBridgeRuntimeActivityMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, RuntimeActivityMethodName> {
  return {
    beginRuntimeActivity: async (input) =>
      JSON.parse(
        binding.beginRuntimeActivityJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["beginRuntimeActivity"]>>,
    progressRuntimeActivity: async (input) =>
      JSON.parse(
        binding.progressRuntimeActivityJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["progressRuntimeActivity"]>>,
    finishRuntimeActivity: async (input) =>
      JSON.parse(
        binding.finishRuntimeActivityJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["finishRuntimeActivity"]>>,
    runtimeActivityCensus: async (query = {}) =>
      JSON.parse(
        binding.runtimeActivityCensusJson(JSON.stringify(query)),
      ) as Awaited<ReturnType<NativeBridgeModule["runtimeActivityCensus"]>>,
  };
}
