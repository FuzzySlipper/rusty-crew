import { validateBridgeValue } from "./bridge-validation.js";
import {
  rawModelConfigurationRecordArraySchema,
  rawModelConfigurationRecordSchema,
  rawModelEndpointRecordArraySchema,
  rawModelEndpointRecordSchema,
} from "./model-endpoint-validation-schemas.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";
import {
  toNativeModelConfigurationRecord,
  toNativeModelEndpointRecord,
  toRawModelConfigurationQuery,
  toRawModelConfigurationWrite,
  toRawModelEndpointQuery,
  toRawModelEndpointWrite,
  type RawModelConfigurationRecord,
  type RawModelEndpointRecord,
} from "./model-endpoint-wire.js";

export interface NormalizedModelNativeBridgeBinding {
  upsertModelEndpointJson(writeJson: string): string;
  listModelEndpointsJson(queryJson: string): string;
  getModelEndpointJson(endpointId: string): string;
  upsertModelConfigurationJson(writeJson: string): string;
  listModelConfigurationsJson(queryJson: string): string;
  getModelConfigurationJson(modelConfigId: string): string;
}

export type NativeBridgeBindingWithNormalizedModelMethods =
  NativeBridgeBinding & NormalizedModelNativeBridgeBinding;

type NormalizedModelMethodName =
  | "upsertModelEndpoint"
  | "listModelEndpoints"
  | "getModelEndpoint"
  | "upsertModelConfiguration"
  | "listModelConfigurations"
  | "getModelConfiguration";

export function createNativeBridgeNormalizedModelMethods(
  binding: NativeBridgeBindingWithNormalizedModelMethods,
): Pick<NativeBridgeModule, NormalizedModelMethodName> {
  return {
    upsertModelEndpoint: async (write) =>
      toNativeModelEndpointRecord(
        validateBridgeValue<RawModelEndpointRecord>({
          operation: "upsert_model_endpoint",
          direction: "rust_to_ts",
          schema: rawModelEndpointRecordSchema,
          value: JSON.parse(
            binding.upsertModelEndpointJson(
              JSON.stringify(toRawModelEndpointWrite(write)),
            ),
          ),
        }),
      ),
    listModelEndpoints: async (query = {}) =>
      validateBridgeValue<RawModelEndpointRecord[]>({
        operation: "list_model_endpoints",
        direction: "rust_to_ts",
        schema: rawModelEndpointRecordArraySchema,
        value: JSON.parse(
          binding.listModelEndpointsJson(
            JSON.stringify(toRawModelEndpointQuery(query)),
          ),
        ),
      }).map(toNativeModelEndpointRecord),
    getModelEndpoint: async (endpointId) => {
      const raw = JSON.parse(
        binding.getModelEndpointJson(endpointId),
      ) as RawModelEndpointRecord | null;
      return raw
        ? toNativeModelEndpointRecord(
            validateBridgeValue<RawModelEndpointRecord>({
              operation: "get_model_endpoint",
              direction: "rust_to_ts",
              schema: rawModelEndpointRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
    upsertModelConfiguration: async (write) =>
      toNativeModelConfigurationRecord(
        validateBridgeValue<RawModelConfigurationRecord>({
          operation: "upsert_model_configuration",
          direction: "rust_to_ts",
          schema: rawModelConfigurationRecordSchema,
          value: JSON.parse(
            binding.upsertModelConfigurationJson(
              JSON.stringify(toRawModelConfigurationWrite(write)),
            ),
          ),
        }),
      ),
    listModelConfigurations: async (query = {}) =>
      validateBridgeValue<RawModelConfigurationRecord[]>({
        operation: "list_model_configurations",
        direction: "rust_to_ts",
        schema: rawModelConfigurationRecordArraySchema,
        value: JSON.parse(
          binding.listModelConfigurationsJson(
            JSON.stringify(toRawModelConfigurationQuery(query)),
          ),
        ),
      }).map(toNativeModelConfigurationRecord),
    getModelConfiguration: async (modelConfigId) => {
      const raw = JSON.parse(
        binding.getModelConfigurationJson(modelConfigId),
      ) as RawModelConfigurationRecord | null;
      return raw
        ? toNativeModelConfigurationRecord(
            validateBridgeValue<RawModelConfigurationRecord>({
              operation: "get_model_configuration",
              direction: "rust_to_ts",
              schema: rawModelConfigurationRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
  };
}
