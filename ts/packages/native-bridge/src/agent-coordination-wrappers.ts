import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type AgentCoordinationMethodName =
  | "routeAgentMessage"
  | "listAgentDirectory"
  | "listAgentRouteResolutions"
  | "getAgentRouteResolution"
  | "resolveAgentAddress"
  | "putAgentRoute"
  | "deleteAgentRoute"
  | "deliverAgentMessage"
  | "replyAgentMessage"
  | "listAgentMessageInbox"
  | "beginAgentRound"
  | "getAgentRound"
  | "getAgentMessageDelivery"
  | "completeAgentMessageDelivery";

export function createNativeBridgeAgentCoordinationMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, AgentCoordinationMethodName> {
  return {
    routeAgentMessage: async (from, to, body, correlationId) =>
      binding.routeAgentMessage(from, to, body, correlationId),
    listAgentDirectory: async () =>
      JSON.parse(binding.listAgentDirectoryJson()) as Awaited<
        ReturnType<NativeBridgeModule["listAgentDirectory"]>
      >,
    listAgentRouteResolutions: async () =>
      JSON.parse(binding.listAgentRouteResolutionsJson()) as Awaited<
        ReturnType<NativeBridgeModule["listAgentRouteResolutions"]>
      >,
    getAgentRouteResolution: async (routeKey) => {
      const value = binding.getAgentRouteResolutionJson(routeKey);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getAgentRouteResolution"]>
          >);
    },
    resolveAgentAddress: async (address) =>
      JSON.parse(binding.resolveAgentAddressJson(address)) as Awaited<
        ReturnType<NativeBridgeModule["resolveAgentAddress"]>
      >,
    putAgentRoute: async (write) =>
      JSON.parse(binding.putAgentRouteJson(JSON.stringify(write))) as Awaited<
        ReturnType<NativeBridgeModule["putAgentRoute"]>
      >,
    deleteAgentRoute: async (deleteRequest) =>
      JSON.parse(
        binding.deleteAgentRouteJson(JSON.stringify(deleteRequest)),
      ) as Awaited<ReturnType<NativeBridgeModule["deleteAgentRoute"]>>,
    deliverAgentMessage: async (command) =>
      JSON.parse(
        binding.deliverAgentMessageJson(JSON.stringify(command)),
      ) as Awaited<ReturnType<NativeBridgeModule["deliverAgentMessage"]>>,
    replyAgentMessage: async (command) =>
      JSON.parse(
        binding.replyAgentMessageJson(JSON.stringify(command)),
      ) as Awaited<ReturnType<NativeBridgeModule["replyAgentMessage"]>>,
    listAgentMessageInbox: async (query) =>
      JSON.parse(
        binding.listAgentMessageInboxJson(JSON.stringify(query)),
      ) as Awaited<ReturnType<NativeBridgeModule["listAgentMessageInbox"]>>,
    beginAgentRound: async (command) =>
      JSON.parse(
        binding.beginAgentRoundJson(JSON.stringify(command)),
      ) as Awaited<ReturnType<NativeBridgeModule["beginAgentRound"]>>,
    getAgentRound: async (roundId) => {
      const value = binding.getAgentRoundJson(roundId);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getAgentRound"]>
          >);
    },
    getAgentMessageDelivery: async (deliveryId) => {
      const value = binding.getAgentMessageDeliveryJson(deliveryId);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getAgentMessageDelivery"]>
          >);
    },
    completeAgentMessageDelivery: async (completion) =>
      JSON.parse(
        binding.completeAgentMessageDeliveryJson(JSON.stringify(completion)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["completeAgentMessageDelivery"]>
      >,
  };
}
