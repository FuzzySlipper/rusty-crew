import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type CertificationMethodName =
  | "certifyExternalRuntime"
  | "invalidateExternalRuntimeCertification"
  | "listExternalRuntimeCertifications"
  | "getExternalRuntimeCertification";

export function createNativeBridgeExternalRuntimeCertificationMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, CertificationMethodName> {
  return {
    certifyExternalRuntime: async (request) =>
      JSON.parse(
        binding.certifyExternalRuntimeJson(JSON.stringify(request)),
      ) as Awaited<ReturnType<NativeBridgeModule["certifyExternalRuntime"]>>,
    invalidateExternalRuntimeCertification: async (invalidation) =>
      JSON.parse(
        binding.invalidateExternalRuntimeCertificationJson(
          JSON.stringify(invalidation),
        ),
      ) as Awaited<
        ReturnType<NativeBridgeModule["invalidateExternalRuntimeCertification"]>
      >,
    listExternalRuntimeCertifications: async () =>
      JSON.parse(binding.listExternalRuntimeCertificationsJson()) as Awaited<
        ReturnType<NativeBridgeModule["listExternalRuntimeCertifications"]>
      >,
    getExternalRuntimeCertification: async (certificationId) => {
      const value =
        binding.getExternalRuntimeCertificationJson(certificationId);
      return value === null || value === undefined
        ? undefined
        : (JSON.parse(value) as Awaited<
            ReturnType<NativeBridgeModule["getExternalRuntimeCertification"]>
          >);
    },
  };
}
