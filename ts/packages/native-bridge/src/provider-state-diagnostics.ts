import type {
  BrainImplementationRegistration,
  BrainWakeRequest,
  ProviderStateMode,
} from "@rusty-crew/contracts";

import type {
  NativeProviderStateDiagnostic,
  NativeProviderStateStatus,
} from "./public-api.js";

export function observeProviderStateWake(
  observations: Map<string, NativeProviderStateDiagnostic>,
  request: Pick<
    BrainWakeRequest,
    "sessionId" | "wakeId" | "providerState" | "providerStateAbsence"
  >,
  registration: BrainImplementationRegistration | undefined,
): void {
  const strategy = registration?.strategy;
  if (!strategy) return;
  const state = request.providerState;
  const status =
    state == null
      ? providerStateStatusFromAbsence(
          request.providerStateAbsence,
          strategy.providerState.mode,
        )
      : "valid";
  const diagnostic: NativeProviderStateDiagnostic = {
    sessionId: request.sessionId,
    moduleId: strategy.moduleId,
    strategyId: strategy.strategyId,
    status,
    source: "runtime_observation",
    lastWakeId: request.wakeId,
    ...(state == null
      ? {}
      : {
          payloadVersion: state.payloadVersion,
          payloadBytes: Buffer.byteLength(JSON.stringify(state.payload)),
          expiresAt: state.expiresAt ?? undefined,
        }),
  };
  observations.set(providerStateDiagnosticKey(diagnostic), diagnostic);
}

export function observeProviderStateFailure(
  observations: Map<string, NativeProviderStateDiagnostic>,
  request: Pick<BrainWakeRequest, "sessionId" | "wakeId">,
  registration: BrainImplementationRegistration | undefined,
  status: Extract<NativeProviderStateStatus, "save_failed" | "load_failed">,
): void {
  const strategy = registration?.strategy;
  if (!strategy) return;
  const diagnostic: NativeProviderStateDiagnostic = {
    sessionId: request.sessionId,
    moduleId: strategy.moduleId,
    strategyId: strategy.strategyId,
    status,
    source: "runtime_observation",
    lastWakeId: request.wakeId,
  };
  observations.set(providerStateDiagnosticKey(diagnostic), diagnostic);
}

function providerStateStatusFromAbsence(
  absence: BrainWakeRequest["providerStateAbsence"] | undefined,
  mode: ProviderStateMode,
): NativeProviderStateStatus {
  if (mode === "unused" || absence === "module_does_not_use_state") {
    return "unused";
  }
  if (absence === "expired") return "expired";
  if (absence === "invalidated") return "invalidated";
  if (absence === "load_failed") return "load_failed";
  return "missing";
}

export function mergeProviderStateDiagnostics(
  storedDiagnostics: Iterable<NativeProviderStateDiagnostic>,
  observations: Iterable<NativeProviderStateDiagnostic>,
): NativeProviderStateDiagnostic[] {
  type Candidate = {
    diagnostic: NativeProviderStateDiagnostic;
    source: "durable" | "runtime_observation";
  };
  const byKey = new Map<string, Candidate>();
  const consider = (candidate: Candidate): void => {
    const diagnostic = candidate.diagnostic;
    const key = providerStateDiagnosticKey(diagnostic);
    const existing = byKey.get(key);
    if (
      existing === undefined ||
      shouldReplaceProviderStateDiagnostic(existing, candidate)
    ) {
      byKey.set(key, candidate);
    }
  };
  for (const diagnostic of storedDiagnostics) {
    consider({
      diagnostic: { ...diagnostic, source: "durable" },
      source: "durable",
    });
  }
  for (const diagnostic of observations) {
    consider({
      diagnostic: { ...diagnostic, source: "runtime_observation" },
      source: "runtime_observation",
    });
  }
  return [...byKey.values()].map(({ diagnostic }) => diagnostic);
}

function shouldReplaceProviderStateDiagnostic(
  existing: {
    diagnostic: NativeProviderStateDiagnostic;
    source: "durable" | "runtime_observation";
  },
  candidate: {
    diagnostic: NativeProviderStateDiagnostic;
    source: "durable" | "runtime_observation";
  },
): boolean {
  const existingFailure = providerStateDiagnosticIsTransientFailure(
    existing.diagnostic,
  );
  const candidateFailure = providerStateDiagnosticIsTransientFailure(
    candidate.diagnostic,
  );
  if (existing.source !== candidate.source) {
    if (candidateFailure !== existingFailure) return candidateFailure;
    return candidate.source === "durable";
  }
  if (candidate.source === "durable") {
    return (
      compareDurableProviderStateDiagnostics(
        candidate.diagnostic,
        existing.diagnostic,
      ) > 0
    );
  }
  return (
    providerStateDiagnosticPriority(candidate.diagnostic) >
    providerStateDiagnosticPriority(existing.diagnostic)
  );
}

function providerStateDiagnosticIsTransientFailure(
  diagnostic: NativeProviderStateDiagnostic,
): boolean {
  return (
    diagnostic.status === "save_failed" || diagnostic.status === "load_failed"
  );
}

function compareDurableProviderStateDiagnostics(
  candidate: NativeProviderStateDiagnostic,
  existing: NativeProviderStateDiagnostic,
): number {
  const currentDifference =
    Number(candidate.isCurrent === true) - Number(existing.isCurrent === true);
  if (currentDifference !== 0) return currentDifference;
  const candidateUpdated = candidate.updatedAt ?? candidate.createdAt ?? "";
  const existingUpdated = existing.updatedAt ?? existing.createdAt ?? "";
  if (candidateUpdated !== existingUpdated) {
    return candidateUpdated > existingUpdated ? 1 : -1;
  }
  return (candidate.recordId ?? 0) - (existing.recordId ?? 0);
}

function providerStateDiagnosticKey(
  diagnostic: Pick<
    NativeProviderStateDiagnostic,
    "sessionId" | "moduleId" | "strategyId"
  >,
): string {
  return `${diagnostic.sessionId}\u0000${diagnostic.moduleId}\u0000${diagnostic.strategyId}`;
}

function providerStateDiagnosticPriority(
  diagnostic: NativeProviderStateDiagnostic,
): number {
  switch (diagnostic.status) {
    case "save_failed":
      return 7;
    case "load_failed":
      return 6;
    case "invalidated":
      return diagnostic.invalidationReason === "superseded" ? 2 : 4;
    case "valid":
      return 5;
    case "expired":
      return 3;
    case "missing":
      return 2;
    case "unused":
      return 1;
  }
}
