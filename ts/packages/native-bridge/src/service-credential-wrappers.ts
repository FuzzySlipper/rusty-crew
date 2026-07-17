import { validateBridgeValue } from "./bridge-validation.js";
import {
  rawModelProviderCredentialLinkResultSchema,
  rawModelProviderRecordSchema,
  rawServiceCredentialRecordArraySchema,
  rawServiceCredentialRecordSchema,
} from "./bridge-validation-schemas.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import {
  toNativeModelProviderRecord,
  type RawModelProviderRecord,
} from "./profile-provider-wire.js";
import type { NativeBridgeModule } from "./public-api.js";
import {
  serviceCredentialWire,
  type RawModelProviderCredentialLinkResult,
  type RawServiceCredentialRecord,
} from "./service-credential-wire.js";

type ServiceCredentialMethodName =
  | "upsertServiceCredential"
  | "listServiceCredentials"
  | "getServiceCredential"
  | "getServiceCredentialSecret"
  | "linkModelProviderCredential"
  | "unlinkModelProviderCredential";

export function createNativeBridgeServiceCredentialMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ServiceCredentialMethodName> {
  return {
    upsertServiceCredential: async (write) =>
      serviceCredentialWire.toNativeRecord(
        validateBridgeValue<RawServiceCredentialRecord>({
          operation: "upsert_service_credential",
          direction: "rust_to_ts",
          schema: rawServiceCredentialRecordSchema,
          value: JSON.parse(
            binding.upsertServiceCredentialJson(
              JSON.stringify(serviceCredentialWire.toRawWrite(write)),
            ),
          ),
        }),
      ),
    listServiceCredentials: async (query = {}) =>
      validateBridgeValue<RawServiceCredentialRecord[]>({
        operation: "list_service_credentials",
        direction: "rust_to_ts",
        schema: rawServiceCredentialRecordArraySchema,
        value: JSON.parse(
          binding.listServiceCredentialsJson(
            JSON.stringify(serviceCredentialWire.toRawQuery(query)),
          ),
        ),
      }).map(serviceCredentialWire.toNativeRecord),
    getServiceCredential: async (credentialId) => {
      const raw = JSON.parse(
        binding.getServiceCredentialJson(credentialId),
      ) as RawServiceCredentialRecord | null;
      return raw
        ? serviceCredentialWire.toNativeRecord(
            validateBridgeValue<RawServiceCredentialRecord>({
              operation: "get_service_credential",
              direction: "rust_to_ts",
              schema: rawServiceCredentialRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
    getServiceCredentialSecret: async (credentialId) =>
      (JSON.parse(binding.getServiceCredentialSecretJson(credentialId)) as
        | string
        | null) ?? undefined,
    linkModelProviderCredential: async (link) =>
      serviceCredentialWire.toNativeLinkResult(
        validateBridgeValue<RawModelProviderCredentialLinkResult>({
          operation: "link_model_provider_credential",
          direction: "rust_to_ts",
          schema: rawModelProviderCredentialLinkResultSchema,
          value: JSON.parse(
            binding.linkModelProviderCredentialJson(
              JSON.stringify(serviceCredentialWire.toRawLink(link)),
            ),
          ),
        }),
      ),
    unlinkModelProviderCredential: async (unlink) =>
      toNativeModelProviderRecord(
        validateBridgeValue<RawModelProviderRecord>({
          operation: "unlink_model_provider_credential",
          direction: "rust_to_ts",
          schema: rawModelProviderRecordSchema,
          value: JSON.parse(
            binding.unlinkModelProviderCredentialJson(
              JSON.stringify(serviceCredentialWire.toRawUnlink(unlink)),
            ),
          ),
        }),
      ),
  };
}
