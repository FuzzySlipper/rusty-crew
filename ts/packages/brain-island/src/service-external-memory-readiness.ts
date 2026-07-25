import {
  createExternalMemoryReadiness,
  type ExternalMemoryReadiness,
  type ExternalMemoryReadinessSnapshot,
} from "./external-memory-readiness.js";
import type {
  DenMemoryClient,
  ServiceAdapterFactories,
} from "./service-adapter-ports.js";
import type { RustyCrewServiceConfig } from "./service-config.js";

export function createServiceDenMemoryClient(
  serviceConfig: RustyCrewServiceConfig | undefined,
  adapterFactories:
    | Pick<ServiceAdapterFactories, "createDenMemoryClient">
    | undefined,
): DenMemoryClient | undefined {
  const config = serviceConfig?.denMemory;
  if (!config?.baseUrl || adapterFactories === undefined) return undefined;
  return adapterFactories.createDenMemoryClient({
    baseUrl: config.baseUrl,
    bearerToken: config.bearerToken,
    apiMode: config.apiMode,
    timeoutMs: config.timeoutMs,
    paths: config.paths,
  });
}

export function createServiceExternalMemoryReadiness(
  serviceConfig: RustyCrewServiceConfig | undefined,
  adapterFactories:
    | Pick<ServiceAdapterFactories, "createDenMemoryClient">
    | undefined,
): ExternalMemoryReadiness {
  return createExternalMemoryReadiness({
    configured: Boolean(serviceConfig?.denMemory.baseUrl),
    client: createServiceDenMemoryClient(serviceConfig, adapterFactories),
    projectId: serviceConfig?.denConversationProjectId,
  });
}

export async function serviceExternalMemoryAvailability(
  serviceConfig: RustyCrewServiceConfig | undefined,
  adapterFactories:
    | Pick<ServiceAdapterFactories, "createDenMemoryClient">
    | undefined,
): Promise<ExternalMemoryReadinessSnapshot> {
  return createServiceExternalMemoryReadiness(
    serviceConfig,
    adapterFactories,
  ).refresh();
}
