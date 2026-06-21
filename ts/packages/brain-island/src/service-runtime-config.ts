import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type {
  AgentId,
  BrainAction,
  BrainEventEnvelope,
  BrainImplementationId,
  ProfileId,
  SessionId,
  SessionKind,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { loadProfileContext } from "./profile-loading.js";
import type { RustyCrewServiceConfig } from "./service-config.js";

export interface RustyCrewConfiguredBrain {
  implementationId: BrainImplementationId;
  profileId: ProfileId;
}

export interface RustyCrewConfiguredSession {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  kind: SessionKind;
}

export interface RustyCrewRuntimeConfig {
  profilesDir: string;
  skillsDir?: string;
  brains: RustyCrewConfiguredBrain[];
  sessions: RustyCrewConfiguredSession[];
}

export interface RustyCrewRuntimeConfigApplyResult {
  brainsRegistered: number;
  brainsAlreadyPresent: number;
  sessionsCreated: number;
  sessionsAlreadyPresent: number;
}

export async function loadRustyCrewRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): Promise<RustyCrewRuntimeConfig> {
  let raw: string;
  try {
    raw = await readFile(serviceConfig.paths.serviceConfigFile, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return emptyRuntimeConfig(serviceConfig);
    }
    throw error;
  }

  const parsed = JSON.parse(raw) as unknown;
  return validateRuntimeConfig(parsed, serviceConfig);
}

export async function applyRustyCrewRuntimeConfig(input: {
  serviceConfig: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  bridge: NativeBridgeModule;
}): Promise<RustyCrewRuntimeConfigApplyResult> {
  const result: RustyCrewRuntimeConfigApplyResult = {
    brainsRegistered: 0,
    brainsAlreadyPresent: 0,
    sessionsCreated: 0,
    sessionsAlreadyPresent: 0,
  };

  for (const brain of input.runtimeConfig.brains) {
    const profile = await loadProfileContext({
      profilesDir: input.runtimeConfig.profilesDir,
      skillsDir: input.runtimeConfig.skillsDir,
      profileId: brain.profileId,
    });
    try {
      await input.bridge.registerBrainRuntime(
        {
          implementationId: brain.implementationId,
          profileId: brain.profileId,
          toolProfile: profile.toolSelection.toolProfile,
          modelConfig: profile.profile.modelConfig,
        },
        {
          async wake(wake): Promise<{
            events: BrainEventEnvelope[];
            actions: BrainAction[];
          }> {
            return {
              events: [
                {
                  wakeId: wake.wakeId,
                  sessionId: wake.sessionId,
                  event: { type: "started" },
                },
                {
                  wakeId: wake.wakeId,
                  sessionId: wake.sessionId,
                  event: { type: "finished" },
                },
              ],
              actions: [
                {
                  type: "deliver_completion",
                  packet: {
                    sessionId: wake.sessionId,
                    status: "completed",
                    summary: "local service brain wake completed",
                  },
                },
              ],
            };
          },
        },
      );
      result.brainsRegistered += 1;
    } catch (error) {
      if (!isAlreadyPresentError(error)) throw error;
      result.brainsAlreadyPresent += 1;
    }
  }

  for (const session of input.runtimeConfig.sessions) {
    try {
      await input.bridge.createSession(session);
      result.sessionsCreated += 1;
    } catch (error) {
      if (!isAlreadyPresentError(error)) throw error;
      result.sessionsAlreadyPresent += 1;
    }
  }

  return result;
}

function emptyRuntimeConfig(
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewRuntimeConfig {
  return {
    profilesDir: join(serviceConfig.paths.configDir, "profiles"),
    brains: [],
    sessions: [],
  };
}

function validateRuntimeConfig(
  parsed: unknown,
  serviceConfig: RustyCrewServiceConfig,
): RustyCrewRuntimeConfig {
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  const profilesDir = pathValue(
    parsed.profilesDir,
    join(serviceConfig.paths.configDir, "profiles"),
  );
  const skillsDir =
    parsed.skillsDir === undefined ? undefined : pathValue(parsed.skillsDir);
  return {
    profilesDir,
    skillsDir,
    brains: arrayValue(parsed.brains).map((item, index) =>
      configuredBrain(item, index),
    ),
    sessions: arrayValue(parsed.sessions).map((item, index) =>
      configuredSession(item, index),
    ),
  };
}

function configuredBrain(
  parsed: unknown,
  index: number,
): RustyCrewConfiguredBrain {
  if (!isRecord(parsed)) {
    throw new Error(`configured brain ${index} must be an object`);
  }
  const profileId = requiredString(
    parsed.profileId,
    `brains[${index}].profileId`,
  );
  return {
    profileId: profileId as ProfileId,
    implementationId: (optionalString(parsed.implementationId) ??
      `${profileId}-brain`) as BrainImplementationId,
  };
}

function configuredSession(
  parsed: unknown,
  index: number,
): RustyCrewConfiguredSession {
  if (!isRecord(parsed)) {
    throw new Error(`configured session ${index} must be an object`);
  }
  const kind = optionalString(parsed.kind) ?? "full";
  if (kind !== "full" && kind !== "worker" && kind !== "delegated") {
    throw new Error(
      `sessions[${index}].kind must be full, worker, or delegated`,
    );
  }
  return {
    sessionId: requiredString(
      parsed.sessionId,
      `sessions[${index}].sessionId`,
    ) as SessionId,
    agentId: requiredString(
      parsed.agentId,
      `sessions[${index}].agentId`,
    ) as AgentId,
    profileId: requiredString(
      parsed.profileId,
      `sessions[${index}].profileId`,
    ) as ProfileId,
    kind,
  };
}

function arrayValue(input: unknown): unknown[] {
  if (input === undefined) return [];
  if (!Array.isArray(input))
    throw new Error("runtime config arrays must be arrays");
  return input;
}

function pathValue(input: unknown, fallback?: string): string {
  const raw = input === undefined ? fallback : requiredString(input, "path");
  if (!raw) throw new Error("path must not be empty");
  return resolve(raw);
}

function requiredString(input: unknown, name: string): string {
  if (typeof input !== "string" || !input.trim()) {
    throw new Error(`${name} must be a non-empty string`);
  }
  return input.trim();
}

function optionalString(input: unknown): string | undefined {
  return typeof input === "string" && input.trim() ? input.trim() : undefined;
}

function isAlreadyPresentError(error: unknown): boolean {
  const message = error instanceof Error ? error.message.toLowerCase() : "";
  return (
    message.includes("already exists") ||
    message.includes("already registered") ||
    message.includes("duplicate")
  );
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
