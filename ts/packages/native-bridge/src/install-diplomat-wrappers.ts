import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type {
  NativeInstallDiplomatBindingRecord,
  NativeInstallDiplomatBridgeMethods,
  NativeTelegramDiplomatIngressPlan,
  NativeTelegramOperatorConsultRecord,
} from "./install-diplomat-public-api.js";

export function createNativeBridgeInstallDiplomatMethods(
  binding: NativeBridgeBinding,
): NativeInstallDiplomatBridgeMethods {
  return {
    putInstallDiplomatBinding: async (write) =>
      JSON.parse(
        binding.putInstallDiplomatBindingJson(JSON.stringify(write)),
      ) as NativeInstallDiplomatBindingRecord,
    rebindInstallDiplomat: async (request) =>
      JSON.parse(
        binding.rebindInstallDiplomatJson(JSON.stringify(request)),
      ) as NativeInstallDiplomatBindingRecord,
    setInstallDiplomatBindingStatus: async (update) =>
      JSON.parse(
        binding.setInstallDiplomatBindingStatusJson(JSON.stringify(update)),
      ) as NativeInstallDiplomatBindingRecord,
    getInstallDiplomatBinding: async (bindingId) =>
      (JSON.parse(
        binding.getInstallDiplomatBindingJson(bindingId),
      ) as NativeInstallDiplomatBindingRecord | null) ?? undefined,
    listInstallDiplomatBindings: async (query = {}) =>
      JSON.parse(
        binding.listInstallDiplomatBindingsJson(JSON.stringify(query)),
      ) as NativeInstallDiplomatBindingRecord[],
    planTelegramDiplomatIngress: async (request) =>
      JSON.parse(
        binding.planTelegramDiplomatIngressJson(JSON.stringify(request)),
      ) as NativeTelegramDiplomatIngressPlan,
    requestTelegramOperatorConsult: async (request) =>
      JSON.parse(
        binding.requestTelegramOperatorConsultJson(JSON.stringify(request)),
      ) as NativeTelegramOperatorConsultRecord,
    prepareTelegramOperatorConsultDelivery: async (consultId) =>
      JSON.parse(
        binding.prepareTelegramOperatorConsultDeliveryJson(consultId),
      ) as NativeTelegramOperatorConsultRecord,
    settleTelegramOperatorConsult: async (settlement) =>
      JSON.parse(
        binding.settleTelegramOperatorConsultJson(JSON.stringify(settlement)),
      ) as NativeTelegramOperatorConsultRecord,
    listTelegramOperatorConsults: async (query = {}) =>
      JSON.parse(
        binding.listTelegramOperatorConsultsJson(JSON.stringify(query)),
      ) as NativeTelegramOperatorConsultRecord[],
  };
}
