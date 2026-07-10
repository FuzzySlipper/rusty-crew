import {
  validateBridgeJsonText,
  validateBridgeValue,
} from "./bridge-validation.js";
import {
  rawChannelIngressRoutePlanInputSchema,
  rawChannelIngressRoutePlanSchema,
  rawDenProductIngressPolicyInputSchema,
  rawDenProductIngressPolicyPlanSchema,
} from "./bridge-validation-schemas.js";
import {
  fromCoreConfigWireRuntimeGraphPlan,
  toCoreConfigWireRuntimeGraphPlanInput,
} from "./generated/core-config-facade.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type {
  NativeBridgeModule,
  NativeLocalCodeResourcePolicyPlan,
  NativeLocalToolProfilePolicyValidationResult,
  NativeRuntimeConfigValidationResult,
  NativeRuntimeGraphPlan,
  NativeToolAvailabilityPlan,
  NativeToolMetadataPolicyValidationResult,
  NativeWebBrowserResourcePolicyPlan,
} from "./public-api.js";
import {
  toNativeChannelIngressRoutePlan,
  toNativeCreateProfilePlanInput,
  toNativeCreateProfilePlan,
  toNativeDelegatedRoleLifecyclePlan,
  toNativeDenProductIngressPolicyPlan,
  toNativeNewSessionControlPlan,
  toNativeReloadMcpControlPlan,
  toNativeRuntimeConfigPlan,
  toNativeRuntimeConfigValidationInput,
  toRawChannelIngressRoutePlanInput,
  toRawDelegatedRoleLifecyclePlanInput,
  toRawDenProductIngressPolicyInput,
  toRawNewSessionControlPlanInput,
  toRawReloadMcpControlPlanInput,
  type RawChannelIngressRoutePlan,
  type RawCreateProfilePlan,
  type RawDenProductIngressPolicyPlan,
  type RawNewSessionControlPlan,
  type RawReloadMcpControlPlan,
  type RawRuntimeConfigPlan,
} from "./runtime-config-wire.js";
import {
  toNativeProfileRegistryMutationPlan,
  toRawProfileRegistryMutationRequest,
  type RawProfileRegistryMutationPlan,
} from "./profile-provider-wire.js";

type RuntimeConfigMethodName =
  | "validateToolMetadataPolicy"
  | "validateLocalToolProfilePolicy"
  | "planToolAvailability"
  | "planLocalCodeResourcePolicy"
  | "planWebBrowserResourcePolicy"
  | "validateRuntimeConfigDraft"
  | "planRuntimeConfig"
  | "planRuntimeGraph"
  | "planCreateProfile"
  | "planProfileRegistryMutation"
  | "planNewSessionControl"
  | "planReloadMcpControl"
  | "planDelegatedRoleLifecycle"
  | "planChannelIngressRoute"
  | "planDenProductIngressPolicy";

export function createNativeBridgeRuntimeConfigMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, RuntimeConfigMethodName> {
  return {
    validateToolMetadataPolicy: async (input) =>
      JSON.parse(
        binding.validateToolMetadataPolicyJson(JSON.stringify(input)),
      ) as NativeToolMetadataPolicyValidationResult,
    validateLocalToolProfilePolicy: async (input) =>
      JSON.parse(
        binding.validateLocalToolProfilePolicyJson(JSON.stringify(input)),
      ) as NativeLocalToolProfilePolicyValidationResult,
    planToolAvailability: async (input) =>
      JSON.parse(
        binding.planToolAvailabilityJson(JSON.stringify(input)),
      ) as NativeToolAvailabilityPlan,
    planLocalCodeResourcePolicy: async (input) =>
      JSON.parse(
        binding.planLocalCodeResourcePolicyJson(JSON.stringify(input)),
      ) as NativeLocalCodeResourcePolicyPlan,
    planWebBrowserResourcePolicy: async (input) =>
      JSON.parse(
        binding.planWebBrowserResourcePolicyJson(JSON.stringify(input)),
      ) as NativeWebBrowserResourcePolicyPlan,
    validateRuntimeConfigDraft: async (input) =>
      JSON.parse(
        binding.validateRuntimeConfigDraftJson(
          JSON.stringify(toNativeRuntimeConfigValidationInput(input)),
        ),
      ) as NativeRuntimeConfigValidationResult,
    planRuntimeConfig: async (input) =>
      toNativeRuntimeConfigPlan(
        JSON.parse(
          binding.planRuntimeConfigJson(
            JSON.stringify(toNativeRuntimeConfigValidationInput(input)),
          ),
        ) as RawRuntimeConfigPlan,
      ),
    planRuntimeGraph: async (input) =>
      fromCoreConfigWireRuntimeGraphPlan(
        JSON.parse(
          binding.planRuntimeGraphJson(
            JSON.stringify(toCoreConfigWireRuntimeGraphPlanInput(input)),
          ),
        ),
      ) as NativeRuntimeGraphPlan,
    planCreateProfile: async (input) =>
      toNativeCreateProfilePlan(
        JSON.parse(
          binding.planCreateProfileJson(
            JSON.stringify(toNativeCreateProfilePlanInput(input)),
          ),
        ) as RawCreateProfilePlan,
      ),
    planProfileRegistryMutation: async (input) =>
      toNativeProfileRegistryMutationPlan(
        JSON.parse(
          binding.planProfileRegistryMutationJson(
            JSON.stringify(toRawProfileRegistryMutationRequest(input)),
          ),
        ) as RawProfileRegistryMutationPlan,
      ),
    planNewSessionControl: async (input) =>
      toNativeNewSessionControlPlan(
        JSON.parse(
          binding.planNewSessionControlJson(
            JSON.stringify(toRawNewSessionControlPlanInput(input)),
          ),
        ) as RawNewSessionControlPlan,
      ),
    planReloadMcpControl: async (input) =>
      toNativeReloadMcpControlPlan(
        JSON.parse(
          binding.planReloadMcpControlJson(
            JSON.stringify(toRawReloadMcpControlPlanInput(input)),
          ),
        ) as RawReloadMcpControlPlan,
      ),
    planDelegatedRoleLifecycle: async (input) =>
      toNativeDelegatedRoleLifecyclePlan(
        JSON.parse(
          binding.planDelegatedRoleLifecycleJson(
            JSON.stringify(toRawDelegatedRoleLifecyclePlanInput(input)),
          ),
        ) as Record<string, unknown>,
      ),
    planChannelIngressRoute: async (input) => {
      const inputJson = JSON.stringify(
        toRawChannelIngressRoutePlanInput(input),
      );
      validateBridgeJsonText({
        operation: "plan_channel_ingress_route",
        direction: "ts_to_rust",
        schema: rawChannelIngressRoutePlanInputSchema,
        text: inputJson,
      });
      const rawPlan = validateBridgeValue<RawChannelIngressRoutePlan>({
        operation: "plan_channel_ingress_route",
        direction: "rust_to_ts",
        schema: rawChannelIngressRoutePlanSchema,
        value: JSON.parse(binding.planChannelIngressRouteJson(inputJson)),
      });
      return toNativeChannelIngressRoutePlan(rawPlan);
    },
    planDenProductIngressPolicy: async (input) => {
      const inputJson = JSON.stringify(
        toRawDenProductIngressPolicyInput(input),
      );
      validateBridgeJsonText({
        operation: "plan_den_product_ingress_policy",
        direction: "ts_to_rust",
        schema: rawDenProductIngressPolicyInputSchema,
        text: inputJson,
      });
      const rawPlan = validateBridgeValue<RawDenProductIngressPolicyPlan>({
        operation: "plan_den_product_ingress_policy",
        direction: "rust_to_ts",
        schema: rawDenProductIngressPolicyPlanSchema,
        value: JSON.parse(binding.planDenProductIngressPolicyJson(inputJson)),
      });
      return toNativeDenProductIngressPolicyPlan(rawPlan);
    },
  };
}
