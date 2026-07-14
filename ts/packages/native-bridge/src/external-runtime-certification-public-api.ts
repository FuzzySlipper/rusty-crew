import type {
  ExternalRuntimeCertificationInvalidation,
  ExternalRuntimeCertificationRecord,
  ExternalRuntimeCertificationRequest,
} from "@rusty-crew/contracts";

export interface NativeExternalRuntimeCertificationBridgeMethods {
  certifyExternalRuntime(
    request: ExternalRuntimeCertificationRequest,
  ): Promise<ExternalRuntimeCertificationRecord>;
  invalidateExternalRuntimeCertification(
    invalidation: ExternalRuntimeCertificationInvalidation,
  ): Promise<ExternalRuntimeCertificationRecord>;
  listExternalRuntimeCertifications(): Promise<
    ExternalRuntimeCertificationRecord[]
  >;
  getExternalRuntimeCertification(
    certificationId: string,
  ): Promise<ExternalRuntimeCertificationRecord | undefined>;
}
