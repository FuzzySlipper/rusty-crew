export const MODEL_PROVIDER_ADMIN_CONTRACT_VERSION = "0.6.0";

export const MODEL_PROVIDER_ADMIN_OPENAPI_PATH =
  "docs/model-provider-admin-api-v0.openapi.json";

export const MODEL_PROVIDER_TEMPERATURE_MILLI_SCALE = 1_000;

export const MODEL_PROVIDER_STATUS_VALUES = [
  "active",
  "disabled",
  "archived",
] as const;

export const MODEL_PROVIDER_PROTOCOL_VALUES = [
  "responses",
  "chat_completions",
] as const;

export const CHAT_COMPLETIONS_DIALECT_VALUES = [
  "standard",
  "kimi",
  "glm",
  "qwen",
  "deepseek",
] as const;

export type ChatCompletionsDialect =
  (typeof CHAT_COMPLETIONS_DIALECT_VALUES)[number];

export const CHAT_COMPLETIONS_THINKING_MODE_VALUES = [
  "provider_default",
  "enabled",
  "disabled",
] as const;

export type ChatCompletionsThinkingMode =
  (typeof CHAT_COMPLETIONS_THINKING_MODE_VALUES)[number];

export const CHAT_COMPLETIONS_REASONING_HISTORY_VALUES = [
  "provider_default",
  "discard",
  "preserve_all",
  "tool_calls_only",
] as const;

export type ChatCompletionsReasoningHistory =
  (typeof CHAT_COMPLETIONS_REASONING_HISTORY_VALUES)[number];

export const CHAT_COMPLETIONS_PROMPT_CACHING_VALUES = [
  "disabled",
  "automatic_5m",
  "automatic_1h",
] as const;

export type ChatCompletionsPromptCaching =
  (typeof CHAT_COMPLETIONS_PROMPT_CACHING_VALUES)[number];

export const MODEL_PROVIDER_REFRESH_MODE_VALUES = [
  "none",
  "plan",
  "apply",
] as const;

export const MODEL_PROVIDER_CREDENTIAL_KIND_VALUES = [
  "api_key",
  "openai_oauth",
  "legacy_raw_api_key",
] as const;

export const MODEL_PROVIDER_ADMIN_PATHS = {
  listCreate: "/v1/admin/model-providers",
  getUpdate: "/v1/admin/model-providers/{alias}",
  credentialLink: "/v1/admin/model-providers/{alias}/credential/link",
  credentialUnlink: "/v1/admin/model-providers/{alias}/credential/unlink",
  credentialListCreate: "/v1/admin/service-credentials",
  credentialGetUpdateDelete: "/v1/admin/service-credentials/{credentialId}",
  credentialImpact: "/v1/admin/service-credentials/{credentialId}/impact",
  credentialClear: "/v1/admin/service-credentials/{credentialId}/clear",
  credentialProviderLink:
    "/v1/admin/service-credentials/{credentialId}/providers/{alias}/link",
  credentialProviderUnlink:
    "/v1/admin/service-credentials/{credentialId}/providers/{alias}/unlink",
  credentialOpenAiOauthStatus:
    "/v1/admin/service-credentials/{credentialId}/oauth/openai/status",
  credentialOpenAiOauthStart:
    "/v1/admin/service-credentials/{credentialId}/oauth/openai/start",
  credentialOpenAiOauthComplete:
    "/v1/admin/service-credentials/{credentialId}/oauth/openai/complete",
  credentialOpenAiOauthClear:
    "/v1/admin/service-credentials/{credentialId}/oauth/openai/clear",
  openAiOauthStatus: "/v1/admin/model-providers/{alias}/oauth/openai/status",
  openAiOauthStart: "/v1/admin/model-providers/{alias}/oauth/openai/start",
  openAiOauthComplete:
    "/v1/admin/model-providers/{alias}/oauth/openai/complete",
  openAiOauthClear: "/v1/admin/model-providers/{alias}/oauth/openai/clear",
} as const;

export const MODEL_PROVIDER_ADMIN_REASON_CODES = {
  invalidProvider: "invalid_model_provider",
  invalidStatus: "invalid_model_provider_status",
  notFound: "model_provider_not_found",
  revisionMismatch: "model_provider_revision_mismatch",
  methodNotAllowed: "model_provider_method_not_allowed",
  credentialNotFound: "service_credential_not_found",
  credentialInvalid: "invalid_service_credential",
  credentialRevisionMismatch: "service_credential_revision_mismatch",
  credentialLinked: "service_credential_linked",
  credentialLinkMismatch: "service_credential_link_mismatch",
  credentialMethodNotAllowed: "service_credential_method_not_allowed",
  oauthMethodNotAllowed: "openai_oauth_provider_method_not_allowed",
  oauthIncompatibleTarget: "openai_oauth_incompatible_target",
  oauthUnregisteredRedirectUri: "openai_oauth_unregistered_redirect_uri",
  oauthInvalidCallbackUrl: "openai_oauth_invalid_callback_url",
  oauthCallbackError: "openai_oauth_callback_error",
  oauthPendingLoginNotFound: "openai_oauth_pending_login_not_found",
  oauthStateMismatch: "openai_oauth_state_mismatch",
  oauthTestModeRequired: "openai_oauth_test_mode_required",
} as const;

export const MODEL_PROVIDER_API_RECORD_REQUIRED_FIELDS = [
  "alias",
  "status",
  "protocol",
  "providerKind",
  "modelId",
  "chatCompletionsDialect",
  "thinkingMode",
  "reasoningHistory",
  "promptCaching",
  "credential",
  "metadataJson",
  "revision",
  "createdAt",
  "updatedAt",
] as const;

export const MODEL_PROVIDER_REVISION_CONFLICT_DATA_FIELDS = [
  "provider",
  "expectedRevision",
  "currentRevision",
] as const;

export const OPENAI_OAUTH_LOGIN_CONFIG_REQUIRED_FIELDS = [
  "issuer",
  "clientId",
  "redirectUri",
  "redirectUriOverrideAllowed",
  "redirectUriMode",
  "callbackUrlCompletionAccepted",
  "callbackUrlCompletionField",
  "pendingLoginIdRequiredForCallbackUrl",
  "remoteOperatorFlow",
] as const;

export const OPENAI_OAUTH_PENDING_LOGIN_PUBLIC_FIELDS = [
  "pendingLoginId",
  "credentialId",
  "issuer",
  "clientId",
  "redirectUri",
  "scopes",
  "codeChallenge",
  "authorizationUrl",
  "createdAt",
  "expiresAt",
] as const;

export function isModelProviderStatusContractValue(
  value: string,
): value is (typeof MODEL_PROVIDER_STATUS_VALUES)[number] {
  return MODEL_PROVIDER_STATUS_VALUES.includes(
    value as (typeof MODEL_PROVIDER_STATUS_VALUES)[number],
  );
}

export function isModelProviderProtocolContractValue(
  value: string,
): value is (typeof MODEL_PROVIDER_PROTOCOL_VALUES)[number] {
  return MODEL_PROVIDER_PROTOCOL_VALUES.includes(
    value as (typeof MODEL_PROVIDER_PROTOCOL_VALUES)[number],
  );
}

export function isModelProviderRefreshModeContractValue(
  value: string,
): value is (typeof MODEL_PROVIDER_REFRESH_MODE_VALUES)[number] {
  return MODEL_PROVIDER_REFRESH_MODE_VALUES.includes(
    value as (typeof MODEL_PROVIDER_REFRESH_MODE_VALUES)[number],
  );
}
