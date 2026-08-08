import {
  toNativeProfileRegistryWrite,
  type RawProfileRegistryWrite,
} from "./profile-provider-wire.js";

import type {
  AgentId,
  ChannelBindingRecord,
  ExternalMessageDeliveryPolicy,
  ResourceLimits,
  SessionId,
} from "@rusty-crew/contracts";

import {
  fromCoreConfigWireRuntimeGraphPlan,
  toCoreConfigWireCreateProfilePlanInput,
  toCoreConfigWireRuntimeConfigValidationInput,
} from "./generated/core-config-facade.js";
import type {
  NativeProfileRegistryLifecycleStatus,
  NativeProfileRegistrySourceAssetRef,
  NativeProfileRegistryDerivedRuntimeRef,
  NativeProfileRegistryImportExportMetadata,
  NativeProfileRegistryRecord,
  NativeProfileRegistryQuery,
  NativeProfilePurgeReport,
  NativeModelProviderStatus,
  NativeModelProviderProtocol,
  NativeModelProviderCredentialKind,
  NativeModelProviderRecord,
  NativeModelProviderWrite,
  NativeModelProviderQuery,
  NativeModelProviderRefreshImpact,
  NativeModelProviderRefreshMode,
  NativeModelProviderRefreshPlan,
  NativeExternalBindingStatus,
  NativeRuntimeConfigDiagnostic,
  NativeRuntimeConfigPlan,
  NativeRuntimeConfigValidationInput,
  NativeRuntimeConfigDraft,
  NativeScheduledJobConfigDraft,
  NativeChannelBindingConfigDraft,
  NativeMcpBindingConfigDraft,
  NativeCreateProfilePlanInput,
  NativeNewSessionControlPlanInput,
  NativeDelegatedRoleLifecyclePlanInput,
  NativeDelegatedRoleLifecyclePlan,
  NativeNewSessionControlPlan,
  NativeReloadMcpControlPlanInput,
  NativeReloadMcpControlPlan,
  NativeChannelIngressRouteDecision,
  NativeChannelIngressRoutePlanInput,
  NativeChannelIngressRoutePlan,
  NativeDenProductIngressPolicyInput,
  NativeDenProductIngressPolicyPlan,
  NativeProfileModelConfigSeed,
  NativeCreateProfilePlan,
  NativeProfileRegistryWrite,
  NativeProfileRegistryUpdate,
  NativeProfileRegistryMutationRequest,
  NativeProfileRegistryMutationPlan,
} from "./public-api.js";

export function toNativeRuntimeConfigValidationInput(
  input: NativeRuntimeConfigValidationInput,
): unknown {
  return toCoreConfigWireRuntimeConfigValidationInput(input);
}

export function toNativeCreateProfilePlanInput(
  input: NativeCreateProfilePlanInput,
): unknown {
  return toCoreConfigWireCreateProfilePlanInput(input);
}

export function toRawNewSessionControlPlanInput(
  input: NativeNewSessionControlPlanInput,
): unknown {
  return {
    command: {
      command_kind: input.command.commandKind,
      target_session_id: input.command.targetSessionId,
      request_id: input.command.requestId,
      idempotency_key: input.command.idempotencyKey,
      operator_reason: input.command.operatorReason,
      operator_reason_code: input.command.operatorReasonCode,
    },
    template: input.template
      ? {
          agent_id: input.template.agentId,
          profile_id: input.template.profileId,
          kind: input.template.kind,
          channel_binding_id: input.template.channelBindingId,
          channel_id: input.template.channelId,
          tool_profile_key: input.template.toolProfileKey,
        }
      : undefined,
    generated_session_id: input.generatedSessionId,
    rebind_handler_available: input.rebindHandlerAvailable ?? false,
  };
}

export function toRawReloadMcpControlPlanInput(
  input: NativeReloadMcpControlPlanInput,
): unknown {
  return {
    command: {
      command_kind: input.command.commandKind,
      target_session_id: input.command.targetSessionId,
      request_id: input.command.requestId,
      idempotency_key: input.command.idempotencyKey,
      operator_reason: input.command.operatorReason,
      operator_reason_code: input.command.operatorReasonCode,
    },
    binding: input.binding
      ? {
          binding_id: input.binding.bindingId,
          session_id: input.binding.sessionId,
          profile_id: input.binding.profileId,
          tool_profile_key: input.binding.toolProfileKey,
          endpoint_ref: input.binding.endpointRef,
        }
      : undefined,
    reload_handler_available: input.reloadHandlerAvailable ?? false,
  };
}

export function toRawDelegatedRoleLifecyclePlanInput(
  input: NativeDelegatedRoleLifecyclePlanInput,
): unknown {
  return {
    parent_session: {
      session_id: input.parentSession.sessionId,
      agent_id: input.parentSession.agentId,
      kind: input.parentSession.kind,
      resource_limits: input.parentSession.resourceLimits
        ? toRawResourceLimits(input.parentSession.resourceLimits)
        : undefined,
    },
    delegated_session_id: input.delegatedSessionId,
    delegated_agent_id: input.delegatedAgentId,
    profile_id: input.profileId,
    tool_profile_key: input.toolProfileKey,
    requested_resource_limits: input.requestedResourceLimits
      ? toRawResourceLimits(input.requestedResourceLimits)
      : undefined,
    requested_workspace_constraint: input.requestedWorkspaceConstraint
      ? { cwd: input.requestedWorkspaceConstraint.cwd }
      : undefined,
    source_wake_id: input.sourceWakeId,
    source_action_index: input.sourceActionIndex,
    task_id: input.taskId,
    correlation_id: input.correlationId,
  };
}

export function toNativeDelegatedRoleLifecyclePlan(
  raw: Record<string, unknown>,
): NativeDelegatedRoleLifecyclePlan {
  return {
    accepted: raw["accepted"] as boolean,
    reasonCode: raw["reason_code"] as string,
    diagnostics: raw["diagnostics"] as NativeRuntimeConfigDiagnostic[],
    sessionId: raw["session_id"] as string,
    agentId: raw["agent_id"] as string,
    parentSessionId: raw["parent_session_id"] as string,
    parentAgentId: raw["parent_agent_id"] as string,
    profileId: raw["profile_id"] as string,
    kind: "delegated",
    resourceLimits:
      toResourceLimits(raw["resource_limits"] as RawResourceLimits) ?? {},
    workspaceConstraint:
      raw["workspace_constraint"] !== null &&
      typeof raw["workspace_constraint"] === "object"
        ? {
            cwd: (raw["workspace_constraint"] as { cwd: string }).cwd,
          }
        : undefined,
    toolProfileKey:
      typeof raw["tool_profile_key"] === "string"
        ? raw["tool_profile_key"]
        : undefined,
    sourceWakeId: raw["source_wake_id"] as string,
    sourceActionIndex: raw["source_action_index"] as number,
    taskId: typeof raw["task_id"] === "string" ? raw["task_id"] : undefined,
    correlationId: raw["correlation_id"] as string,
  };
}

export function toRawChannelIngressRoutePlanInput(
  input: NativeChannelIngressRoutePlanInput,
): RawChannelIngressRoutePlanInput {
  return {
    message: {
      adapter_id: input.message.adapterId,
      binding_id: input.message.bindingId,
      provider: input.message.provider,
      external_channel_id: input.message.externalChannelId,
      external_thread_id: input.message.externalThreadId,
      external_user_id: input.message.externalUserId,
      body: input.message.body,
      mentions: input.message.mentions,
      expires_at: input.message.expiresAt,
      idempotency_key: input.message.idempotencyKey,
      runtime_agent_id: input.message.runtimeAgentId,
    },
    bindings: input.bindings.map(toRawChannelBindingConfigDraft),
    mention_aliases: input.mentionAliases ?? {},
    system_agent_id: input.systemAgentId,
    now: input.now,
    seen_idempotency_keys: input.seenIdempotencyKeys ?? [],
  };
}

export function toRawChannelBindingConfigDraft(
  binding: NativeChannelBindingConfigDraft | ChannelBindingRecord,
): RawChannelBindingConfigDraft {
  return {
    binding_id: binding.bindingId,
    adapter_id: binding.adapterId,
    provider: binding.provider,
    agent_id: binding.agentId,
    instance_id: binding.instanceId,
    session_id: binding.sessionId,
    profile_id: binding.profileId,
    external_channel_id: binding.externalChannelId,
    external_thread_id: binding.externalThreadId,
    external_user_id: binding.externalUserId,
    conversation_project_id: binding.conversationProjectId,
    conversation_channel_id: binding.conversationChannelId,
    provider_subscription_id: binding.providerSubscriptionId,
    status: binding.status,
  };
}

export function toRawDenProductIngressPolicyInput(
  input: NativeDenProductIngressPolicyInput,
): RawDenProductIngressPolicyInput {
  return {
    operation: input.operation,
    entity_kind: input.entityKind,
    entity_id: input.entityId,
    project_id: input.projectId,
  };
}

export function toNativeNewSessionControlPlan(
  plan: RawNewSessionControlPlan,
): NativeNewSessionControlPlan {
  return {
    accepted: plan.accepted,
    commandKind: plan.command_kind,
    target: {
      oldSessionId: plan.target.old_session_id ?? undefined,
      newSessionId: plan.target.new_session_id ?? undefined,
      agentId: plan.target.agent_id ?? undefined,
      profileId: plan.target.profile_id ?? undefined,
      channelBindingId: plan.target.channel_binding_id ?? undefined,
      channelId: plan.target.channel_id ?? undefined,
      toolProfileKey: plan.target.tool_profile_key ?? undefined,
    },
    idempotencyKey: plan.idempotency_key ?? undefined,
    operatorReason: plan.operator_reason,
    reasonCode: plan.reason_code,
    denial: plan.denial
      ? {
          reasonCode: plan.denial.reason_code,
          summary: plan.denial.summary,
        }
      : undefined,
    preconditions: (plan.preconditions ?? []).map((precondition) => ({
      code: precondition.code,
      status: precondition.status,
      summary: precondition.summary,
    })),
    actions: (plan.actions ?? []).map((action) => ({
      action: action.action,
      sessionId: action.session_id ?? undefined,
      oldSessionId: action.old_session_id ?? undefined,
      newSessionId: action.new_session_id ?? undefined,
      reasonCode: action.reason_code,
    })),
  };
}

export function toNativeReloadMcpControlPlan(
  plan: RawReloadMcpControlPlan,
): NativeReloadMcpControlPlan {
  return {
    accepted: plan.accepted,
    commandKind: plan.command_kind,
    target: {
      sessionId: plan.target.session_id ?? undefined,
      bindingId: plan.target.binding_id ?? undefined,
      profileId: plan.target.profile_id ?? undefined,
      toolProfileKey: plan.target.tool_profile_key ?? undefined,
      endpointRef: plan.target.endpoint_ref ?? undefined,
    },
    idempotencyKey: plan.idempotency_key ?? undefined,
    operatorReason: plan.operator_reason,
    reasonCode: plan.reason_code,
    denial: plan.denial
      ? {
          reasonCode: plan.denial.reason_code,
          summary: plan.denial.summary,
        }
      : undefined,
    preconditions: (plan.preconditions ?? []).map((precondition) => ({
      code: precondition.code,
      status: precondition.status,
      summary: precondition.summary,
    })),
    actions: (plan.actions ?? []).map((action) => ({
      action: action.action,
      sessionId: action.session_id,
      bindingId: action.binding_id,
      reasonCode: action.reason_code,
    })),
  };
}

export function toNativeChannelIngressRoutePlan(
  plan: RawChannelIngressRoutePlan,
): NativeChannelIngressRoutePlan {
  return {
    status: plan.status,
    reasonCode: plan.reason_code,
    reason: plan.reason,
    correlationId: plan.correlation_id ?? undefined,
    binding: plan.binding
      ? toNativeChannelBindingConfigDraft(plan.binding)
      : undefined,
    candidates: plan.candidates.map(toNativeChannelBindingConfigDraft),
    route: plan.route
      ? {
          from: plan.route.from as AgentId,
          to: plan.route.to as AgentId,
          body: plan.route.body,
          correlationId: plan.route.correlation_id,
          bindingId: plan.route.binding_id,
          ...(plan.route.session_id === undefined ||
          plan.route.session_id === null
            ? {}
            : { sessionId: plan.route.session_id as SessionId }),
        }
      : undefined,
  };
}

export function toNativeChannelBindingConfigDraft(
  binding: RawChannelBindingConfigDraft,
): NativeChannelBindingConfigDraft {
  return {
    bindingId: binding.binding_id,
    adapterId: binding.adapter_id,
    provider: binding.provider,
    agentId: binding.agent_id,
    instanceId: binding.instance_id ?? undefined,
    sessionId: binding.session_id ?? undefined,
    profileId: binding.profile_id,
    externalChannelId: binding.external_channel_id,
    externalThreadId: binding.external_thread_id ?? undefined,
    externalUserId: binding.external_user_id ?? undefined,
    conversationProjectId: binding.conversation_project_id ?? undefined,
    conversationChannelId: binding.conversation_channel_id ?? undefined,
    providerSubscriptionId: binding.provider_subscription_id ?? undefined,
    status: binding.status,
  };
}

export function toNativeDenProductIngressPolicyPlan(
  plan: RawDenProductIngressPolicyPlan,
): NativeDenProductIngressPolicyPlan {
  return {
    status: plan.status,
    operation: plan.operation,
    reasonCode: plan.reason_code,
    reason: plan.reason,
    lifecycleOperation: plan.lifecycle_operation,
  };
}

export function toNativeCreateProfilePlan(
  plan: RawCreateProfilePlan,
): NativeCreateProfilePlan {
  return {
    diagnostics: plan.diagnostics,
    registryWrite: plan.registry_write
      ? toNativeProfileRegistryWrite(plan.registry_write)
      : undefined,
    fileAssetActions: (plan.file_asset_actions ?? []).map((action) => ({
      kind: action.kind,
      profileId: action.profile_id,
      relativePath: action.relative_path,
      overwrite: action.overwrite,
      metadataJson: action.metadata_json,
    })),
    derivedRuntimeActions: (plan.derived_runtime_actions ?? []).map(
      (action) => ({
        kind: action.kind,
        refKind: action.ref_kind,
        refId: action.ref_id,
        applyPhase: action.apply_phase,
        metadataJson: action.metadata_json,
      }),
    ),
    profileSeed: plan.profile_seed
      ? {
          profileId: plan.profile_seed.profile_id,
          displayName: plan.profile_seed.display_name ?? undefined,
          providerAlias: plan.profile_seed.provider_alias,
          modelConfig: toProfileModelConfigSeed(plan.profile_seed.model_config),
          brain: {
            module: plan.profile_seed.brain.module ?? undefined,
            strategy: plan.profile_seed.brain.strategy ?? undefined,
          },
          externalMessageDeliveryPolicy:
            plan.profile_seed.external_message_delivery_policy,
          skillsMode: plan.profile_seed.skills_mode,
        }
      : undefined,
    runtimeBrain: plan.runtime_brain
      ? {
          implementationId: plan.runtime_brain.implementation_id,
          profileId: plan.runtime_brain.profile_id,
        }
      : undefined,
    runtimeSession: plan.runtime_session
      ? {
          sessionId: plan.runtime_session.session_id,
          agentId: plan.runtime_session.agent_id,
          profileId: plan.runtime_session.profile_id,
          kind: plan.runtime_session.kind,
          workspaceCwd: plan.runtime_session.workspace_cwd ?? undefined,
          resourceLimits: toResourceLimits(
            plan.runtime_session.resource_limits,
          ),
          ownerId: plan.runtime_session.owner_id ?? undefined,
          historyWindow: plan.runtime_session.history_window
            ? {
                maxMessages:
                  plan.runtime_session.history_window.max_messages ?? undefined,
              }
            : undefined,
          maxHistoryMessages:
            plan.runtime_session.max_history_messages ?? undefined,
        }
      : undefined,
    profileMcpConfig: plan.profile_mcp_config
      ? {
          bindingId: plan.profile_mcp_config.binding_id ?? undefined,
          endpointRef: plan.profile_mcp_config.endpoint_ref ?? undefined,
          serverNames: plan.profile_mcp_config.server_names,
          transport: plan.profile_mcp_config.transport ?? undefined,
          toolProfile: plan.profile_mcp_config.tool_profile ?? undefined,
        }
      : undefined,
    runtimeMcpBindings: (plan.runtime_mcp_bindings ?? []).map(
      toMcpBindingDraft,
    ),
  };
}

export function toNativeRuntimeConfigPlan(
  plan: RawRuntimeConfigPlan,
): NativeRuntimeConfigPlan {
  return {
    runtimeConfig: toRuntimeConfigDraft(plan.runtime_config),
    diagnostics: plan.diagnostics,
    derivedScheduledJobs: plan.derived_scheduled_jobs.map(toScheduledJobDraft),
    derivedMcpBindings: plan.derived_mcp_bindings.map(toMcpBindingDraft),
  };
}

export function toRuntimeConfigDraft(
  draft: RawRuntimeConfigDraft,
): NativeRuntimeConfigDraft {
  return {
    profilesDir: draft.profiles_dir,
    skillsDir: draft.skills_dir ?? undefined,
    brains: draft.brains.map((brain) => ({
      implementationId: brain.implementation_id,
      profileId: brain.profile_id,
    })),
    sessions: draft.sessions.map((session) => ({
      sessionId: session.session_id,
      agentId: session.agent_id,
      profileId: session.profile_id,
      kind: session.kind,
      workspaceCwd: session.workspace_cwd ?? undefined,
      resourceLimits: toResourceLimits(session.resource_limits),
      ownerId: session.owner_id ?? undefined,
      historyWindow: session.history_window
        ? {
            maxMessages: session.history_window.max_messages ?? undefined,
          }
        : undefined,
      maxHistoryMessages: session.max_history_messages ?? undefined,
    })),
    scheduledJobs: draft.scheduled_jobs.map(toScheduledJobDraft),
    channelBindings: draft.channel_bindings.map((binding) => ({
      bindingId: binding.binding_id,
      adapterId: binding.adapter_id,
      provider: binding.provider,
      agentId: binding.agent_id,
      instanceId: binding.instance_id ?? undefined,
      sessionId: binding.session_id,
      profileId: binding.profile_id,
      externalChannelId: binding.external_channel_id,
      externalThreadId: binding.external_thread_id ?? undefined,
      externalUserId: binding.external_user_id ?? undefined,
      conversationProjectId: binding.conversation_project_id ?? undefined,
      conversationChannelId: binding.conversation_channel_id ?? undefined,
      providerSubscriptionId: binding.provider_subscription_id ?? undefined,
      status: binding.status,
    })),
    mcpBindings: draft.mcp_bindings.map(toMcpBindingDraft),
  };
}

export function toScheduledJobDraft(
  job: RawScheduledJobConfigDraft,
): NativeScheduledJobConfigDraft {
  return {
    id: job.id,
    schedule: job.schedule,
    shape: job.shape,
    jobKind: job.job_kind ?? undefined,
    targetSessionId: job.target_session_id ?? undefined,
    script: job.script ?? undefined,
    deliveryChannelId: job.delivery_channel_id ?? undefined,
  };
}

export function toMcpBindingDraft(
  binding: RawMcpBindingConfigDraft,
): NativeMcpBindingConfigDraft {
  return {
    bindingId: binding.binding_id,
    adapterId: binding.adapter_id,
    agentId: binding.agent_id,
    instanceId: binding.instance_id ?? undefined,
    sessionId: binding.session_id ?? undefined,
    profileId: binding.profile_id,
    serverNames: binding.server_names,
    endpointRef: binding.endpoint_ref,
    transport: binding.transport,
    toolProfileKey: binding.tool_profile_key,
    status: binding.status,
  };
}

export function toProfileModelConfigSeed(
  modelConfig: RawProfileModelConfigSeed,
): NativeProfileModelConfigSeed {
  return {
    provider: modelConfig.provider,
    modelName: modelConfig.model_name,
    baseUrl: modelConfig.base_url,
    api: modelConfig.api,
    apiKeyEnv: modelConfig.api_key_env,
    temperatureMilli: modelConfig.temperature_milli,
    maxOutputTokens: modelConfig.max_output_tokens,
  };
}

export function toResourceLimits(
  limits: RawResourceLimits | undefined,
): ResourceLimits | undefined {
  if (!limits) {
    return undefined;
  }
  return {
    maxDurationMs: limits.max_duration_ms ?? undefined,
    maxDelegationDepth: limits.max_delegation_depth ?? undefined,
  };
}

export function toRawResourceLimits(limits: ResourceLimits): RawResourceLimits {
  return {
    max_duration_ms: limits.maxDurationMs ?? undefined,
    max_delegation_depth: limits.maxDelegationDepth ?? undefined,
  };
}

export interface RawCreateProfilePlan {
  diagnostics: NativeRuntimeConfigDiagnostic[];
  registry_write?: RawProfileRegistryWrite;
  file_asset_actions: RawCreateProfileFileAssetAction[];
  derived_runtime_actions: RawCreateProfileDerivedRuntimeAction[];
  profile_seed?: {
    profile_id: string;
    display_name?: string;
    provider_alias: string;
    model_config: RawProfileModelConfigSeed;
    brain: {
      module?: string;
      strategy?: string;
    };
    external_message_delivery_policy: ExternalMessageDeliveryPolicy;
    skills_mode: string;
  };
  runtime_brain?: {
    implementation_id: string;
    profile_id: string;
  };
  runtime_session?: {
    session_id: string;
    agent_id: string;
    profile_id: string;
    kind: "full" | "worker" | "delegated";
    workspace_cwd?: string;
    resource_limits?: RawResourceLimits;
    owner_id?: string;
    history_window?: {
      max_messages?: number;
    };
    max_history_messages?: number;
  };
  profile_mcp_config?: {
    binding_id?: string;
    endpoint_ref?: string;
    server_names: string[];
    transport?: string;
    tool_profile?: string;
  };
  runtime_mcp_bindings?: RawMcpBindingConfigDraft[];
}

export interface RawNewSessionControlPlan {
  accepted: boolean;
  command_kind: string;
  target: {
    old_session_id?: string | null;
    new_session_id?: string | null;
    agent_id?: string | null;
    profile_id?: string | null;
    channel_binding_id?: string | null;
    channel_id?: string | null;
    tool_profile_key?: string | null;
  };
  idempotency_key?: string | null;
  operator_reason: string;
  reason_code: string;
  denial?: {
    reason_code: string;
    summary: string;
  } | null;
  preconditions?: Array<{
    code: string;
    status: "satisfied" | "failed";
    summary: string;
  }>;
  actions?: Array<{
    action: "archive_session" | "create_session" | "rebind_channel";
    session_id?: string | null;
    old_session_id?: string | null;
    new_session_id?: string | null;
    reason_code: string;
  }>;
}

export interface RawReloadMcpControlPlan {
  accepted: boolean;
  command_kind: string;
  target: {
    session_id?: string | null;
    binding_id?: string | null;
    profile_id?: string | null;
    tool_profile_key?: string | null;
    endpoint_ref?: string | null;
  };
  idempotency_key?: string | null;
  operator_reason: string;
  reason_code: string;
  denial?: {
    reason_code: string;
    summary: string;
  } | null;
  preconditions?: Array<{
    code: string;
    status: "satisfied" | "failed";
    summary: string;
  }>;
  actions?: Array<{
    action: "reload_mcp_surface";
    session_id: string;
    binding_id: string;
    reason_code: string;
  }>;
}

export interface RawChannelIngressRoutePlanInput {
  message: RawChannelIngressRouteMessage;
  bindings: RawChannelBindingConfigDraft[];
  mention_aliases: Record<string, string>;
  system_agent_id?: string;
  now?: string;
  seen_idempotency_keys: string[];
}

export interface RawChannelIngressRouteMessage {
  adapter_id: string;
  binding_id: string;
  provider: string;
  external_channel_id: string;
  external_thread_id?: string;
  external_user_id: string;
  body: string;
  mentions: string[];
  expires_at: string;
  idempotency_key: string;
  runtime_agent_id?: string;
}

export interface RawChannelIngressRoutePlan {
  status: NativeChannelIngressRouteDecision;
  reason_code: string;
  reason: string;
  correlation_id?: string | null;
  binding?: RawChannelBindingConfigDraft | null;
  candidates: RawChannelBindingConfigDraft[];
  route?: RawChannelIngressRouteRequest | null;
}

export interface RawChannelIngressRouteRequest {
  from: string;
  to: string;
  body: string;
  correlation_id: string;
  binding_id: string;
  session_id?: string | null;
}

export interface RawDenProductIngressPolicyInput {
  operation: string;
  entity_kind: string;
  entity_id: string;
  project_id?: string;
}

export interface RawDenProductIngressPolicyPlan {
  status: "allowed" | "denied";
  operation: string;
  reason_code: string;
  reason: string;
  lifecycle_operation: boolean;
}

export interface RawCreateProfileFileAssetAction {
  kind: "write_profile_json";
  profile_id: string;
  relative_path: string;
  overwrite: boolean;
  metadata_json: unknown;
}

export interface RawCreateProfileDerivedRuntimeAction {
  kind:
    | "add_brain"
    | "add_session"
    | "add_profile_mcp_config"
    | "add_mcp_binding";
  ref_kind: string;
  ref_id: string;
  apply_phase: string;
  metadata_json: unknown;
}

export interface RawRuntimeConfigPlan {
  runtime_config: RawRuntimeConfigDraft;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  derived_scheduled_jobs: RawScheduledJobConfigDraft[];
  derived_mcp_bindings: RawMcpBindingConfigDraft[];
}

export interface RawRuntimeConfigDraft {
  profiles_dir: string;
  skills_dir?: string;
  brains: Array<{
    implementation_id: string;
    profile_id: string;
  }>;
  sessions: RawSessionConfigDraft[];
  scheduled_jobs: RawScheduledJobConfigDraft[];
  channel_bindings: RawChannelBindingConfigDraft[];
  mcp_bindings: RawMcpBindingConfigDraft[];
}

export interface RawSessionConfigDraft {
  session_id: string;
  agent_id: string;
  profile_id: string;
  kind: "full" | "worker" | "delegated";
  workspace_cwd?: string;
  resource_limits?: RawResourceLimits;
  owner_id?: string;
  history_window?: {
    max_messages?: number;
  };
  max_history_messages?: number;
}

export interface RawScheduledJobConfigDraft {
  id: string;
  schedule: string;
  shape: "host_job" | "session_wake" | "script_only" | "data_collection";
  job_kind?: string;
  target_session_id?: string;
  script?: string;
  delivery_channel_id?: string;
}

export interface RawChannelBindingConfigDraft {
  binding_id: string;
  adapter_id: string;
  provider: string;
  agent_id: string;
  instance_id?: string;
  session_id?: string;
  profile_id: string;
  external_channel_id: string;
  external_thread_id?: string;
  external_user_id?: string;
  conversation_project_id?: string;
  conversation_channel_id?: number;
  provider_subscription_id?: string;
  status: NativeExternalBindingStatus;
}

export interface RawMcpBindingConfigDraft {
  binding_id: string;
  adapter_id: string;
  agent_id: string;
  instance_id?: string;
  session_id?: string;
  profile_id: string;
  server_names: string[];
  endpoint_ref: string;
  transport: string;
  tool_profile_key: string;
  status: NativeExternalBindingStatus;
}

export interface RawProfileModelConfigSeed {
  provider: string;
  model_name: string;
  base_url?: string;
  api?: string;
  api_key_env?: string;
  temperature_milli?: number;
  max_output_tokens?: number;
}

export interface RawResourceLimits {
  max_duration_ms?: number;
  max_delegation_depth?: number;
}
