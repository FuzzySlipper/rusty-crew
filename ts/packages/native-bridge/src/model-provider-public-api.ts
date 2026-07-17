export type NativeModelProviderStatus = "active" | "disabled" | "archived";
export type NativeModelProviderProtocol = "responses" | "chat_completions";
export type NativeModelProviderCredentialKind =
  | "api_key"
  | "openai_oauth"
  | "legacy_raw_api_key";

export interface NativeModelProviderCredential {
  hasSecret: boolean;
  secretRef?: string;
  updatedAt?: string;
  kind?: NativeModelProviderCredentialKind;
  revision?: number;
}

export interface NativeServiceCredentialRecord {
  credentialId: string;
  displayName: string;
  providerKind: string;
  credentialKind: NativeModelProviderCredentialKind;
  credential: NativeModelProviderCredential;
  linkedProviderAliases: string[];
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeServiceCredentialWrite {
  credentialId: string;
  displayName: string;
  providerKind: string;
  credentialKind: NativeModelProviderCredentialKind;
  secret?: string;
  clearSecret?: boolean;
  expectedRevision?: number;
  now: string;
}

export interface NativeServiceCredentialQuery {
  providerKind?: string;
  limit?: number;
  offset?: number;
}

export interface NativeServiceCredentialDelete {
  credentialId: string;
  expectedRevision?: number;
}

export interface NativeModelProviderCredentialLink {
  providerAlias: string;
  credentialId: string;
  expectedProviderRevision?: number;
  expectedCredentialRevision?: number;
  now: string;
}

export interface NativeModelProviderCredentialUnlink {
  providerAlias: string;
  expectedProviderRevision?: number;
  now: string;
}

export interface NativeModelProviderCredentialLinkResult {
  provider: NativeModelProviderRecord;
  credential: NativeServiceCredentialRecord;
}

export interface NativeModelProviderRecord {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  providerKind: string;
  displayName?: string;
  description?: string;
  baseUrl?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  credentialId?: string;
  credential: NativeModelProviderCredential;
  metadataJson: unknown;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeModelProviderWrite {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  providerKind: string;
  displayName?: string;
  description?: string;
  baseUrl?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  secret?: string;
  clearSecret?: boolean;
  expectedCredentialRevision?: number;
  metadataJson?: unknown;
  expectedRevision?: number;
  now: string;
}

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
