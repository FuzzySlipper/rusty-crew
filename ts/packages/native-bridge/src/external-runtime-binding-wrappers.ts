import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";
import { serializeExternalAgentBindingMetadataWrite } from "./external-runtime-wire.js";
import { toSessionState, type RawSessionState } from "./session-wire.js";

export type ExternalBindingMethodName =
  | "bindExternalAgent"
  | "restoreExternalAgentBinding"
  | "updateExternalBindingMetadata"
  | "listExternalBindings"
  | "getExternalBinding";

export function createNativeBridgeExternalBindingMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ExternalBindingMethodName> {
  return {
    bindExternalAgent: async (input) =>
      JSON.parse(
        binding.bindExternalAgentJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["bindExternalAgent"]>>,
    restoreExternalAgentBinding: async (request) => {
      const raw = JSON.parse(
        binding.restoreExternalAgentBindingJson(JSON.stringify(request)),
      ) as Omit<
        Awaited<ReturnType<NativeBridgeModule["restoreExternalAgentBinding"]>>,
        "session"
      > & { session: RawSessionState };
      return { ...raw, session: toSessionState(raw.session) };
    },
    updateExternalBindingMetadata: async (write) =>
      JSON.parse(
        binding.updateExternalBindingMetadataJson(
          serializeExternalAgentBindingMetadataWrite(write),
        ),
      ) as Awaited<
        ReturnType<NativeBridgeModule["updateExternalBindingMetadata"]>
      >,
    listExternalBindings: async () =>
      JSON.parse(binding.listExternalBindingsJson()) as Awaited<
        ReturnType<NativeBridgeModule["listExternalBindings"]>
      >,
    getExternalBinding: async (bindingId) => {
      const value = binding.getExternalBindingJson(bindingId);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getExternalBinding"]>
          >);
    },
  };
}
