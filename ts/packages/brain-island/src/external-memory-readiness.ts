import type {
  DenMemoryClient,
  DenMemoryClientErrorLike,
} from "./service-adapter-ports.js";

export type ExternalMemoryReadinessReasonCode =
  | "external_memory_not_configured"
  | "external_memory_adapter_unavailable"
  | "external_memory_dependency_unavailable";

export interface ExternalMemoryReadinessSnapshot {
  configured: boolean;
  clientAvailable: boolean;
  mode: "metadata";
  reasonCode?: ExternalMemoryReadinessReasonCode;
  lastError?: string;
}

export interface ExternalMemoryReadiness {
  current(): ExternalMemoryReadinessSnapshot;
  refresh(): Promise<ExternalMemoryReadinessSnapshot>;
}

export function createExternalMemoryReadiness(input: {
  configured: boolean;
  client?: DenMemoryClient;
  projectId?: string;
}): ExternalMemoryReadiness {
  let snapshot = initialSnapshot(input);
  let inFlight: Promise<ExternalMemoryReadinessSnapshot> | undefined;

  return {
    current: () => snapshot,
    refresh: async () => {
      if (!input.configured || input.client === undefined) return snapshot;
      if (inFlight !== undefined) return inFlight;
      inFlight = probeExternalMemory(input.client, input.projectId)
        .then((next) => {
          snapshot = next;
          return snapshot;
        })
        .finally(() => {
          inFlight = undefined;
        });
      return inFlight;
    },
  };
}

function initialSnapshot(input: {
  configured: boolean;
  client?: DenMemoryClient;
}): ExternalMemoryReadinessSnapshot {
  if (!input.configured) {
    return {
      configured: false,
      clientAvailable: false,
      mode: "metadata",
      reasonCode: "external_memory_not_configured",
      lastError: "external memory endpoint is not configured",
    };
  }
  if (input.client === undefined) {
    return {
      configured: true,
      clientAvailable: false,
      mode: "metadata",
      reasonCode: "external_memory_adapter_unavailable",
      lastError: "external memory adapter is unavailable",
    };
  }
  return {
    configured: true,
    clientAvailable: false,
    mode: "metadata",
    reasonCode: "external_memory_dependency_unavailable",
    lastError: "external memory readiness has not been checked",
  };
}

async function probeExternalMemory(
  client: DenMemoryClient,
  projectId: string | undefined,
): Promise<ExternalMemoryReadinessSnapshot> {
  try {
    await client.search({
      query: "rusty-crew external memory readiness",
      limit: 1,
      ...(projectId === undefined ? {} : { context: { projectId } }),
      metadata: { purpose: "readiness_probe" },
    });
    return {
      configured: true,
      clientAvailable: true,
      mode: "metadata",
    };
  } catch (error) {
    const failureCode = safeFailureCode(error);
    return {
      configured: true,
      clientAvailable: false,
      mode: "metadata",
      reasonCode: "external_memory_dependency_unavailable",
      lastError: `external memory readiness probe failed (${failureCode})`,
    };
  }
}

function safeFailureCode(error: unknown): string {
  const memoryError = error as DenMemoryClientErrorLike;
  const candidate = memoryError.code ?? memoryError.options?.reasonCode;
  if (
    candidate !== undefined &&
    candidate.length <= 64 &&
    /^[a-z0-9_.-]+$/i.test(candidate)
  ) {
    return candidate.toLowerCase();
  }
  return "memory_readiness_failed";
}
