import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type ExternalRuntimeTurnMethodName =
  | "getExternalTurn"
  | "listExternalTurnsForNativeThread"
  | "listActiveExternalTurns"
  | "expireExternalTurnDispatches"
  | "transitionExternalTurn";

export function createNativeBridgeExternalRuntimeTurnMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ExternalRuntimeTurnMethodName> {
  return {
    getExternalTurn: async (requestId) => {
      const value = binding.getExternalTurnJson(requestId);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getExternalTurn"]>
          >);
    },
    listExternalTurnsForNativeThread: async (runtimeId, nativeThreadId) =>
      JSON.parse(
        binding.listExternalTurnsForNativeThreadJson(runtimeId, nativeThreadId),
      ) as Awaited<
        ReturnType<NativeBridgeModule["listExternalTurnsForNativeThread"]>
      >,
    listActiveExternalTurns: async () =>
      JSON.parse(binding.listActiveExternalTurnsJson()) as Awaited<
        ReturnType<NativeBridgeModule["listActiveExternalTurns"]>
      >,
    expireExternalTurnDispatches: async (now) =>
      JSON.parse(binding.expireExternalTurnDispatchesJson(now)) as Awaited<
        ReturnType<NativeBridgeModule["expireExternalTurnDispatches"]>
      >,
    transitionExternalTurn: async (input) =>
      JSON.parse(
        binding.transitionExternalTurnJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["transitionExternalTurn"]>>,
  };
}
