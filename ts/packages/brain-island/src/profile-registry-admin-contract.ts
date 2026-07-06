export const PROFILE_REGISTRY_ADMIN_CONTRACT_VERSION = "0.1.0";

export const PROFILE_REGISTRY_ADMIN_OPENAPI_PATH =
  "docs/profile-registry-admin-api-v0.openapi.json";

export const PROFILE_REGISTRY_LIFECYCLE_STATUS_VALUES = [
  "active",
  "paused",
  "decommissioned",
  "archived",
] as const;

export const PROFILE_REGISTRY_SESSION_KIND_VALUES = [
  "full",
  "worker",
  "delegated",
] as const;

export const PROFILE_REGISTRY_WRITE_KIND_VALUES = [
  "update",
  "lifecycle",
  "prompt",
  "runtime-config",
] as const;

export const PROFILE_REGISTRY_WRITE_MODE_VALUES = ["plan", "apply"] as const;

export const PROFILE_REGISTRY_ADMIN_PATHS = {
  updatePlan: "/v1/admin/profiles/registry/{profile_id}/update/plan",
  updateApply: "/v1/admin/profiles/registry/{profile_id}/update/apply",
  lifecyclePlan: "/v1/admin/profiles/registry/{profile_id}/lifecycle/plan",
  lifecycleApply: "/v1/admin/profiles/registry/{profile_id}/lifecycle/apply",
  promptPlan: "/v1/admin/profiles/registry/{profile_id}/prompt/plan",
  promptApply: "/v1/admin/profiles/registry/{profile_id}/prompt/apply",
  runtimeConfigPlan:
    "/v1/admin/profiles/registry/{profile_id}/runtime-config/plan",
  runtimeConfigApply:
    "/v1/admin/profiles/registry/{profile_id}/runtime-config/apply",
} as const;

export const PROFILE_REGISTRY_ADMIN_REASON_CODES = {
  methodNotAllowed: "profile_registry_write_requires_post_or_patch",
  unknownRoute: "unknown_profile_registry_write_route",
  recordMissing: "profile_registry_record_missing",
  revisionMismatch: "profile_registry_revision_mismatch",
  modelProviderNotFound: "model_provider_not_found",
  modelProviderNotActive: "model_provider_not_active",
  inlineToolPolicyRejectsMcpToolset: "inline_tool_policy_rejects_mcp_toolset",
  inlineToolPolicyUnknownToolset: "inline_tool_policy_unknown_toolset",
  inlineToolPolicyUnknownTool: "inline_tool_policy_unknown_tool",
} as const;

export const PROFILE_REGISTRY_RECORD_REQUIRED_FIELDS = [
  "profileId",
  "lifecycleStatus",
  "activeRuntimeSettingsJson",
  "sourceAssetRefs",
  "derivedRuntimeRefs",
  "importExport",
  "revision",
  "createdAt",
  "updatedAt",
] as const;

export const PROFILE_REGISTRY_WRITE_REQUIRED_FIELDS = [
  "profileId",
  "lifecycleStatus",
  "activeRuntimeSettingsJson",
  "sourceAssetRefs",
  "derivedRuntimeRefs",
  "importExport",
  "now",
] as const;

export const PROFILE_REGISTRY_MUTATION_PLAN_REQUIRED_FIELDS = [
  "ok",
  "profileId",
  "kind",
  "mode",
  "expectedRevision",
  "current",
  "next",
  "nextWrite",
  "diagnostics",
  "implications",
] as const;

export const PROFILE_REGISTRY_RUNTIME_CONFIG_REQUIRED_FIELDS = [
  "providerAlias",
  "contextPolicy",
  "mcpBindings",
] as const;

export function profileRegistryAdminPathToConcrete(path: string): string {
  return path.replace("{profile_id}", "profile-alpha");
}
