import { validateBridgeValue } from "./bridge-validation.js";
import {
  rawModelProviderRefreshImpactSchema,
  rawModelProviderRefreshPlanSchema,
  rawModelProviderRecordArraySchema,
  rawModelProviderRecordSchema,
  rawProfilePurgeReportSchema,
  rawProfileRegistryRecordArraySchema,
  rawProfileRegistryRecordSchema,
} from "./bridge-validation-schemas.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";
import {
  toNativeModelProviderRecord,
  toNativeModelProviderRefreshImpact,
  toNativeModelProviderRefreshPlan,
  toNativeProfilePurgeReport,
  toNativeProfileRegistryRecord,
  toRawModelProviderQuery,
  toRawModelProviderWrite,
  toRawProfileRegistryQuery,
  toRawProfileRegistryUpdate,
  toRawProfileRegistryWrite,
  type RawModelProviderRecord,
  type RawModelProviderRefreshImpact,
  type RawModelProviderRefreshPlan,
  type RawProfilePurgeReport,
  type RawProfileRegistryRecord,
} from "./profile-provider-wire.js";

type ProfileProviderMethodName =
  | "createProfileRegistryRecord"
  | "updateProfileRegistryRecord"
  | "listProfileRegistryRecords"
  | "getProfileRegistryRecord"
  | "purgeProfile"
  | "upsertModelProvider"
  | "listModelProviders"
  | "getModelProvider"
  | "getModelProviderSecret"
  | "modelProviderRefreshImpact"
  | "planModelProviderRefresh";

export function createNativeBridgeProfileProviderMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ProfileProviderMethodName> {
  return {
    createProfileRegistryRecord: async (write) =>
      toNativeProfileRegistryRecord(
        validateBridgeValue<RawProfileRegistryRecord>({
          operation: "create_profile_registry_record",
          direction: "rust_to_ts",
          schema: rawProfileRegistryRecordSchema,
          value: JSON.parse(
            binding.createProfileRegistryRecordJson(
              JSON.stringify(toRawProfileRegistryWrite(write)),
            ),
          ),
        }),
      ),
    updateProfileRegistryRecord: async (update) =>
      toNativeProfileRegistryRecord(
        validateBridgeValue<RawProfileRegistryRecord>({
          operation: "update_profile_registry_record",
          direction: "rust_to_ts",
          schema: rawProfileRegistryRecordSchema,
          value: JSON.parse(
            binding.updateProfileRegistryRecordJson(
              JSON.stringify(toRawProfileRegistryUpdate(update)),
            ),
          ),
        }),
      ),
    listProfileRegistryRecords: async (query = {}) =>
      validateBridgeValue<RawProfileRegistryRecord[]>({
        operation: "list_profile_registry_records",
        direction: "rust_to_ts",
        schema: rawProfileRegistryRecordArraySchema,
        value: JSON.parse(
          binding.listProfileRegistryRecordsJson(
            JSON.stringify(toRawProfileRegistryQuery(query)),
          ),
        ),
      }).map(toNativeProfileRegistryRecord),
    getProfileRegistryRecord: async (profileId) => {
      const raw = JSON.parse(
        binding.getProfileRegistryRecordJson(profileId),
      ) as RawProfileRegistryRecord | null;
      return raw
        ? toNativeProfileRegistryRecord(
            validateBridgeValue<RawProfileRegistryRecord>({
              operation: "get_profile_registry_record",
              direction: "rust_to_ts",
              schema: rawProfileRegistryRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
    purgeProfile: async (profileId) =>
      toNativeProfilePurgeReport(
        validateBridgeValue<RawProfilePurgeReport>({
          operation: "purge_profile",
          direction: "rust_to_ts",
          schema: rawProfilePurgeReportSchema,
          value: JSON.parse(binding.purgeProfileJson(profileId)),
        }),
      ),
    upsertModelProvider: async (write) =>
      toNativeModelProviderRecord(
        validateBridgeValue<RawModelProviderRecord>({
          operation: "upsert_model_provider",
          direction: "rust_to_ts",
          schema: rawModelProviderRecordSchema,
          value: JSON.parse(
            binding.upsertModelProviderJson(
              JSON.stringify(toRawModelProviderWrite(write)),
            ),
          ),
        }),
      ),
    listModelProviders: async (query = {}) =>
      validateBridgeValue<RawModelProviderRecord[]>({
        operation: "list_model_providers",
        direction: "rust_to_ts",
        schema: rawModelProviderRecordArraySchema,
        value: JSON.parse(
          binding.listModelProvidersJson(
            JSON.stringify(toRawModelProviderQuery(query)),
          ),
        ),
      }).map(toNativeModelProviderRecord),
    getModelProvider: async (alias) => {
      const raw = JSON.parse(
        binding.getModelProviderJson(alias),
      ) as RawModelProviderRecord | null;
      return raw
        ? toNativeModelProviderRecord(
            validateBridgeValue<RawModelProviderRecord>({
              operation: "get_model_provider",
              direction: "rust_to_ts",
              schema: rawModelProviderRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
    getModelProviderSecret: async (alias) =>
      (JSON.parse(binding.getModelProviderSecretJson(alias)) as
        | string
        | null) ?? undefined,
    modelProviderRefreshImpact: async (request) =>
      toNativeModelProviderRefreshImpact(
        validateBridgeValue<RawModelProviderRefreshImpact>({
          operation: "model_provider_refresh_impact",
          direction: "rust_to_ts",
          schema: rawModelProviderRefreshImpactSchema,
          value: JSON.parse(
            binding.modelProviderRefreshImpactJson(
              JSON.stringify({ provider_alias: request.providerAlias }),
            ),
          ),
        }),
      ),
    planModelProviderRefresh: async (request) =>
      toNativeModelProviderRefreshPlan(
        validateBridgeValue<RawModelProviderRefreshPlan>({
          operation: "plan_model_provider_refresh",
          direction: "rust_to_ts",
          schema: rawModelProviderRefreshPlanSchema,
          value: JSON.parse(
            binding.planModelProviderRefreshJson(
              JSON.stringify({
                provider_alias: request.providerAlias,
                mode: request.mode,
              }),
            ),
          ),
        }),
      ),
  };
}
