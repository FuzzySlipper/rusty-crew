import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";
import { serializeExternalAgentSessionCreationRequest } from "./external-runtime-wire.js";
import { createNativeBridgeExternalRuntimeTurnMethods } from "./external-runtime-turn-wrappers.js";

type ExternalRuntimeMethodName =
  | "registerExternalRuntime"
  | "authorizeExternalRuntimeHandshake"
  | "recordExternalRuntimeState"
  | "listExternalRuntimes"
  | "getExternalRuntime"
  | "acquireExternalController"
  | "releaseExternalController"
  | "bindExternalAgent"
  | "listExternalBindings"
  | "getExternalBinding"
  | "prepareExternalAgentSessionCreation"
  | "markExternalAgentSessionNativeStarting"
  | "completeExternalAgentSessionCreation"
  | "recordExternalAgentSessionCreationFailure"
  | "getExternalTurn"
  | "listActiveExternalTurns"
  | "expireExternalTurnDispatches"
  | "transitionExternalTurn"
  | "submitExternalControl"
  | "completeExternalControl"
  | "recordExternalInteraction"
  | "resolveExternalInteraction"
  | "terminalizeExternalInteraction"
  | "listPendingExternalInteractions"
  | "recordExternalRuntimeEvent"
  | "queryExternalRuntimeEvents";

export function createNativeBridgeExternalRuntimeMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ExternalRuntimeMethodName> {
  return {
    ...createNativeBridgeExternalRuntimeTurnMethods(binding),
    registerExternalRuntime: async (input) =>
      JSON.parse(
        binding.registerExternalRuntimeJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["registerExternalRuntime"]>>,
    listExternalRuntimes: async () =>
      JSON.parse(binding.listExternalRuntimesJson()) as Awaited<
        ReturnType<NativeBridgeModule["listExternalRuntimes"]>
      >,
    authorizeExternalRuntimeHandshake: async (observation) =>
      JSON.parse(
        binding.authorizeExternalRuntimeHandshakeJson(
          JSON.stringify(observation),
        ),
      ) as Awaited<
        ReturnType<NativeBridgeModule["authorizeExternalRuntimeHandshake"]>
      >,
    recordExternalRuntimeState: async (observation) =>
      JSON.parse(
        binding.recordExternalRuntimeStateJson(JSON.stringify(observation)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["recordExternalRuntimeState"]>
      >,
    getExternalRuntime: async (runtimeId) => {
      const value = binding.getExternalRuntimeJson(runtimeId);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getExternalRuntime"]>
          >);
    },
    acquireExternalController: async (input) =>
      JSON.parse(
        binding.acquireExternalControllerJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["acquireExternalController"]>>,
    releaseExternalController: async (input) =>
      JSON.parse(
        binding.releaseExternalControllerJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["releaseExternalController"]>>,
    bindExternalAgent: async (input) =>
      JSON.parse(
        binding.bindExternalAgentJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["bindExternalAgent"]>>,
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
    prepareExternalAgentSessionCreation: async (request) =>
      JSON.parse(
        binding.prepareExternalAgentSessionCreationJson(
          serializeExternalAgentSessionCreationRequest(request),
        ),
      ) as Awaited<
        ReturnType<NativeBridgeModule["prepareExternalAgentSessionCreation"]>
      >,
    markExternalAgentSessionNativeStarting: async (input) =>
      JSON.parse(
        binding.markExternalAgentSessionNativeStartingJson(
          JSON.stringify(input),
        ),
      ) as Awaited<
        ReturnType<NativeBridgeModule["markExternalAgentSessionNativeStarting"]>
      >,
    completeExternalAgentSessionCreation: async (input) =>
      JSON.parse(
        binding.completeExternalAgentSessionCreationJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["completeExternalAgentSessionCreation"]>
      >,
    recordExternalAgentSessionCreationFailure: async (input) =>
      JSON.parse(
        binding.recordExternalAgentSessionCreationFailureJson(
          JSON.stringify(input),
        ),
      ) as Awaited<
        ReturnType<
          NativeBridgeModule["recordExternalAgentSessionCreationFailure"]
        >
      >,
    submitExternalControl: async (request) =>
      JSON.parse(
        binding.submitExternalControlJson(JSON.stringify(request)),
      ) as Awaited<ReturnType<NativeBridgeModule["submitExternalControl"]>>,
    completeExternalControl: async (input) =>
      JSON.parse(
        binding.completeExternalControlJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["completeExternalControl"]>>,
    recordExternalInteraction: async (input) =>
      JSON.parse(
        binding.recordExternalInteractionJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["recordExternalInteraction"]>>,
    resolveExternalInteraction: async (input) =>
      JSON.parse(
        binding.resolveExternalInteractionJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["resolveExternalInteraction"]>
      >,
    terminalizeExternalInteraction: async (input) =>
      JSON.parse(
        binding.terminalizeExternalInteractionJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["terminalizeExternalInteraction"]>
      >,
    listPendingExternalInteractions: async () =>
      JSON.parse(binding.listPendingExternalInteractionsJson()) as Awaited<
        ReturnType<NativeBridgeModule["listPendingExternalInteractions"]>
      >,
    recordExternalRuntimeEvent: async (input) =>
      JSON.parse(
        binding.recordExternalRuntimeEventJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["recordExternalRuntimeEvent"]>
      >,
    queryExternalRuntimeEvents: async (input) =>
      JSON.parse(
        binding.queryExternalRuntimeEventsJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["queryExternalRuntimeEvents"]>
      >,
  };
}
