import { hostname } from "node:os";
import type {
  DenSuccessorGatewayClient,
  DenSuccessorGatewayHealth,
} from "@rusty-crew/adapter-den";
import type { RustyCrewConfiguredSession } from "./service-runtime-config.js";

export interface DenSuccessorGatewayStartupReport {
  enabled: boolean;
  gatewayHealth?: DenSuccessorGatewayHealth;
  sessionsAnnounced: number;
  runtimeInstancesRegistered: number;
  runtimeInstancesHeartbeated: number;
  failures: string[];
}

export async function announceConfiguredSessionsToDenGateway(input: {
  client: DenSuccessorGatewayClient;
  sessions: readonly RustyCrewConfiguredSession[];
  now: string;
}): Promise<DenSuccessorGatewayStartupReport> {
  const failures: string[] = [];
  let gatewayHealth: DenSuccessorGatewayHealth | undefined;
  try {
    gatewayHealth = await input.client.health();
  } catch (error) {
    failures.push(`gateway health check failed: ${safeErrorMessage(error)}`);
  }

  let sessionsAnnounced = 0;
  let runtimeInstancesRegistered = 0;
  let runtimeInstancesHeartbeated = 0;
  for (const session of input.sessions) {
    try {
      await input.client.registerRuntimeInstance({
        instance_id: instanceId(session),
        profile_identity: session.profileId,
        host: hostname(),
        pid: process.pid,
      });
      runtimeInstancesRegistered += 1;
      await input.client.heartbeatRuntimeInstance(instanceId(session));
      runtimeInstancesHeartbeated += 1;
    } catch (error) {
      failures.push(
        `registering runtime ${session.agentId} failed: ${safeErrorMessage(error)}`,
      );
    }

    try {
      await input.client.createObservationActivityEvent({
        source_domain: "runtime",
        event_type: "adapter_connected",
        agent_identity: sessionIdentity(session),
        runtime_instance_id: instanceId(session),
        payload: {
          kind: "agent_activity.v1",
          schema_version: 1,
          summary: `${session.agentId} connected through Rusty Crew Gateway adapter`,
          severity: "info",
          visibility: "agent",
          adapter: "pi-crew",
          surface: "gateway",
          session_key: session.sessionId,
          observed_at: input.now,
        },
      });
      sessionsAnnounced += 1;
    } catch (error) {
      failures.push(
        `announcing ${session.agentId} failed: ${safeErrorMessage(error)}`,
      );
    }
  }

  return {
    enabled: true,
    gatewayHealth,
    sessionsAnnounced,
    runtimeInstancesRegistered,
    runtimeInstancesHeartbeated,
    failures,
  };
}

export async function heartbeatConfiguredSessionsToDenRuntime(input: {
  client: DenSuccessorGatewayClient;
  sessions: readonly RustyCrewConfiguredSession[];
}): Promise<{ heartbeated: number; failures: string[] }> {
  let heartbeated = 0;
  const failures: string[] = [];
  for (const session of input.sessions) {
    try {
      await input.client.heartbeatRuntimeInstance(instanceId(session));
      heartbeated += 1;
    } catch (error) {
      failures.push(
        `heartbeating runtime ${session.agentId} failed: ${safeErrorMessage(error)}`,
      );
    }
  }
  return { heartbeated, failures };
}

export function denGatewayStartupSummary(
  report: DenSuccessorGatewayStartupReport | undefined,
): string {
  if (report === undefined || !report.enabled) {
    return "Den successor Gateway integration disabled.";
  }
  const health = report.gatewayHealth?.status ?? "unknown";
  const suffix =
    report.failures.length === 0
      ? ""
      : ` ${report.failures.length} issue(s): ${report.failures.join("; ")}`;
  return `Den successor Gateway health=${health}; registered ${report.runtimeInstancesRegistered} runtime instance(s), heartbeated ${report.runtimeInstancesHeartbeated}, announced ${report.sessionsAnnounced} configured session(s).${suffix}`;
}

function sessionIdentity(session: RustyCrewConfiguredSession): {
  profile: string;
  instance_id: string;
  session_key: string;
} {
  return {
    profile: session.profileId,
    instance_id: instanceId(session),
    session_key: session.sessionId,
  };
}

function instanceId(session: RustyCrewConfiguredSession): string {
  return `${session.agentId}@rusty-crew`;
}

function safeErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.replace(/Bearer\s+[^\s,;]+/gi, "Bearer <redacted>");
}
