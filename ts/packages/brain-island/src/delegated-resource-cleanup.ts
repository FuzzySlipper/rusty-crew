import type { DelegatedResourceCleanupReport } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import {
  type AgentActivityObservationProducer,
  type AgentActivityPublishResult,
  type AgentObservationIdentity,
  workActivity,
} from "./agent-activity-observation.js";

export interface AdapterCleanupResult {
  adapter: string;
  released: number;
  degraded: number;
  resultRef?: string;
}

export interface DelegatedResourceAdapterCleanup {
  adapter: string;
  cleanup(): Promise<AdapterCleanupResult> | AdapterCleanupResult;
}

export interface DelegatedResourceCleanupInput {
  runtime: Pick<NativeBridgeModule, "cleanupDelegatedResources">;
  adapters?: readonly DelegatedResourceAdapterCleanup[];
  observation?: {
    producer?: AgentActivityObservationProducer;
    identity: AgentObservationIdentity;
    workRef?: Parameters<typeof workActivity>[0]["workRef"];
    resultRef?: Parameters<typeof workActivity>[0]["resultRef"];
  };
}

export interface DelegatedResourceCleanupResult {
  runtime: DelegatedResourceCleanupReport;
  adapters: readonly AdapterCleanupResult[];
  observation: {
    started?: AgentActivityPublishResult["status"];
    terminal?: AgentActivityPublishResult["status"];
  };
}

export async function runDelegatedResourceCleanup(
  input: DelegatedResourceCleanupInput,
): Promise<DelegatedResourceCleanupResult> {
  const started = await publishCleanupObservation(
    input,
    "work_started",
    "Delegated resource cleanup started.",
  );

  let runtime: DelegatedResourceCleanupReport;
  const adapters: AdapterCleanupResult[] = [];
  try {
    runtime = await input.runtime.cleanupDelegatedResources();
    for (const adapter of input.adapters ?? []) {
      adapters.push(await adapter.cleanup());
    }
  } catch (error) {
    await publishCleanupObservation(
      input,
      "work_failed",
      error instanceof Error ? error.message : "Delegated cleanup failed.",
    );
    throw error;
  }

  const terminal = await publishCleanupObservation(
    input,
    "work_completed",
    cleanupSummary(runtime, adapters),
  );
  return {
    runtime,
    adapters,
    observation: {
      started: started?.status,
      terminal: terminal?.status,
    },
  };
}

function cleanupSummary(
  runtime: DelegatedResourceCleanupReport,
  adapters: readonly AdapterCleanupResult[],
): string {
  const runtimeArchived =
    runtime.terminalArchived.length +
    runtime.orphanedArchived.length +
    runtime.expiredArchived.length;
  const adapterReleased = adapters.reduce(
    (sum, adapter) => sum + adapter.released,
    0,
  );
  const adapterDegraded = adapters.reduce(
    (sum, adapter) => sum + adapter.degraded,
    0,
  );
  return `Delegated cleanup archived ${runtimeArchived} session(s), released ${adapterReleased} adapter resource(s), degraded ${adapterDegraded}.`;
}

async function publishCleanupObservation(
  input: DelegatedResourceCleanupInput,
  eventType: "work_started" | "work_completed" | "work_failed",
  summary: string,
): Promise<AgentActivityPublishResult | undefined> {
  const observation = input.observation;
  if (!observation?.producer) return undefined;
  return observation.producer.publish(
    workActivity({
      eventType,
      identity: observation.identity,
      summary,
      workRef: observation.workRef ?? {},
      resultRef: observation.resultRef,
      visibility: "agent",
    }),
  );
}
