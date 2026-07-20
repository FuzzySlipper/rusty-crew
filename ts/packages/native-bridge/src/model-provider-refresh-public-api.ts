import type { NativeModelProviderStatus } from "./model-provider-public-api.js";

export interface NativeModelProviderQuery {
  status?: NativeModelProviderStatus;
  aliasPrefix?: string;
  limit?: number;
  offset?: number;
}

export interface NativeModelProviderAffectedProfile {
  profileId: string;
  sessionIds: string[];
  configuredSessionIds: string[];
  activeSessionIds: string[];
}

export interface NativeModelProviderRefreshImpact {
  providerAlias: string;
  affectedProfiles: NativeModelProviderAffectedProfile[];
}

export interface NativeModelProviderRefreshImpactRequest {
  providerAlias: string;
}

export type NativeModelProviderRefreshMode = "none" | "plan" | "apply";

export interface NativeModelProviderRefreshPlanRequest {
  providerAlias: string;
  mode: NativeModelProviderRefreshMode;
}

export interface NativeModelProviderRefreshProfileAction {
  profileId: string;
  commandName: string;
  reason: string;
  plannedSummary: string;
  appliedSummary: string;
  blockedSummary: string;
  failureReasonCode: string;
}

export interface NativeModelProviderRefreshPlan {
  providerAlias: string;
  mode: NativeModelProviderRefreshMode;
  affectedProfiles: NativeModelProviderAffectedProfile[];
  actions: NativeModelProviderRefreshProfileAction[];
}
