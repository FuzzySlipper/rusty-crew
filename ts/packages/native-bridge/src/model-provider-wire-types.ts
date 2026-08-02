import type {
  NativeModelProviderCredentialKind,
  NativeModelProviderRecord,
  NativeModelProviderRefreshMode,
  NativeModelProviderStatus,
  NativeModelProviderProtocol,
  NativeModelProviderWrite,
} from "./public-api.js";

export interface RawModelProviderCredential {
  has_secret: boolean;
  secret_ref?: string | null;
  updated_at?: string | null;
  kind?: NativeModelProviderCredentialKind | null;
  revision?: number | null;
}

export interface RawModelProviderRecord {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  provider_kind: string;
  display_name?: string | null;
  description?: string | null;
  base_url?: string | null;
  model_id: string;
  context_window_tokens?: number | null;
  max_output_tokens?: number | null;
  temperature_milli?: number | null;
  reasoning_effort?: string | null;
  reasoning_format?: string | null;
  responses_dialect?: NativeModelProviderRecord["responsesDialect"] | null;
  chat_completions_dialect: NativeModelProviderRecord["chatCompletionsDialect"];
  thinking_mode: NativeModelProviderRecord["thinkingMode"];
  reasoning_history: NativeModelProviderRecord["reasoningHistory"];
  reasoning_budget_tokens?: number | null;
  prompt_caching: NativeModelProviderRecord["promptCaching"];
  credential_id?: string | null;
  credential: RawModelProviderCredential;
  metadata_json: unknown;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface RawModelProviderWrite {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  provider_kind: string;
  display_name?: string;
  description?: string;
  base_url?: string;
  model_id: string;
  context_window_tokens?: number;
  max_output_tokens?: number;
  temperature_milli?: number;
  reasoning_effort?: string;
  reasoning_format?: string;
  responses_dialect?: NativeModelProviderWrite["responsesDialect"];
  chat_completions_dialect: NonNullable<
    NativeModelProviderWrite["chatCompletionsDialect"]
  >;
  thinking_mode: NonNullable<NativeModelProviderWrite["thinkingMode"]>;
  reasoning_history: NonNullable<NativeModelProviderWrite["reasoningHistory"]>;
  reasoning_budget_tokens?: number;
  prompt_caching: NonNullable<NativeModelProviderWrite["promptCaching"]>;
  secret?: string;
  clear_secret: boolean;
  expected_credential_revision?: number;
  metadata_json: unknown;
  expected_revision?: number;
  now: string;
}

export interface RawModelProviderQuery {
  status?: NativeModelProviderStatus;
  alias_prefix?: string;
  limit?: number;
  offset?: number;
}

export interface RawModelProviderAffectedProfile {
  profile_id: string;
  session_ids: string[];
  configured_session_ids: string[];
  active_session_ids: string[];
}

export interface RawModelProviderRefreshImpact {
  provider_alias: string;
  affected_profiles: RawModelProviderAffectedProfile[];
}

export interface RawModelProviderRefreshProfileAction {
  profile_id: string;
  command_name: string;
  reason: string;
  planned_summary: string;
  applied_summary: string;
  blocked_summary: string;
  failure_reason_code: string;
}

export interface RawModelProviderRefreshPlan {
  provider_alias: string;
  mode: NativeModelProviderRefreshMode;
  affected_profiles: RawModelProviderAffectedProfile[];
  actions: RawModelProviderRefreshProfileAction[];
}
