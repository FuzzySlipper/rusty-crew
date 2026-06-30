import { buildRuntimeHealthProjection } from "./runtime-health.js";
import type { SessionContextUsageResult } from "./rusty-view-chat-api.js";
import type { RuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import type {
  SlashCommandName,
  SlashCommandResponse,
  SlashCommandRouterOptions,
  SlashCommandSession,
} from "./slash-command-router.js";

export interface SlashCommandResponseContext {
  diagnostics: RuntimeDiagnosticsProjection;
  session: SlashCommandSession;
  modelContext?: SessionContextUsageResult;
  options?: SlashCommandRouterOptions;
}

export function buildReadOnlySlashCommandResponse(
  commandName: Extract<
    SlashCommandName,
    "help" | "status" | "session" | "model"
  >,
  context: SlashCommandResponseContext,
): SlashCommandResponse {
  switch (commandName) {
    case "help":
      return helpResponse(context.options);
    case "status":
      return statusResponse(context);
    case "session":
      return sessionResponse(context);
    case "model":
      return modelResponse(context);
  }
}

function helpResponse(
  options: SlashCommandRouterOptions | undefined,
): SlashCommandResponse {
  const commands = (
    ["help", "status", "session", "model", "new", "reload-mcp"] as const
  )
    .filter(
      (command) =>
        options?.allowedCommands === undefined ||
        options.allowedCommands.includes(command),
    )
    .map((command) => `/${command}`);
  return {
    title: "Commands",
    summary:
      "Slash commands are intercepted before the LLM and use diagnostics/control APIs.",
    items: [
      ...commands,
      "Control commands require an authorized full/prime session by default.",
      "Responses are bounded and omit raw prompts, logs, secrets, and full tool output.",
    ],
  };
}

function modelResponse(
  context: SlashCommandResponseContext,
): SlashCommandResponse {
  const model = context.modelContext;
  if (!model) {
    return {
      title: "Model",
      summary: "Model diagnostics are not available on this service.",
      fields: {
        sessionId: context.session.sessionId,
        profileId: context.session.profileId,
      },
    };
  }
  return {
    title: "Model",
    summary: model.degraded
      ? `Model diagnostics for ${model.profile_id} are degraded.`
      : `${model.profile_id} uses ${model.provider.model_id ?? "unknown model"} via ${model.provider.alias}.`,
    fields: {
      sessionId: model.session_id,
      profileId: model.profile_id,
      providerAlias: model.provider.alias,
      providerStatus: model.provider.status,
      protocol: model.provider.protocol ?? "unknown",
      providerKind: model.provider.provider_kind ?? "unknown",
      modelId: model.provider.model_id ?? "unknown",
      brainBackend: model.brain.backend,
      brainModule: model.brain.module ?? "unknown",
      contextStrategy: model.context_strategy.strategy_id,
      autoCompactionEnabled: model.context_strategy.auto_compaction_enabled,
      contextWindowTokens: model.context.context_window_tokens ?? 0,
      estimatedPromptTokens: model.context.estimated_prompt_tokens ?? 0,
      estimatedRemainingTokens: model.context.estimated_remaining_tokens ?? 0,
      maxOutputTokens: model.context.max_output_tokens ?? 0,
      estimateQuality: model.context.estimate_quality,
      toolCount: model.tools.tool_count,
      mcpBindings: model.tools.mcp_binding_count,
      mcpActive: model.tools.mcp_active_count,
    },
    items: boundedItems([
      model.provider.base_url_redacted
        ? `provider endpoint ${model.provider.base_url_redacted}`
        : "",
      model.provider.temperature === undefined
        ? ""
        : `temperature ${model.provider.temperature}`,
      model.provider.reasoning_effort
        ? `reasoning effort ${model.provider.reasoning_effort}`
        : "",
      model.provider.reasoning_format
        ? `reasoning format ${model.provider.reasoning_format}`
        : "",
      model.tools.local_tool_profile_id
        ? `local tool profile ${model.tools.local_tool_profile_id}`
        : "",
      ...model.diagnostics.map((diagnostic) => diagnostic.message),
    ]),
  };
}

function statusResponse(
  context: SlashCommandResponseContext,
): SlashCommandResponse {
  const health = buildRuntimeHealthProjection(context.diagnostics);
  const session = currentSession(context);
  return {
    title: "Status",
    summary: health.readiness.ready
      ? health.degradedStatus.degraded
        ? "Runtime is ready with degraded dependencies."
        : "Runtime is ready."
      : "Runtime is not ready.",
    fields: {
      health: context.diagnostics.health,
      ready: health.readiness.ready,
      sessionStatus: session?.status ?? "missing",
      pendingQueueItems: context.diagnostics.summary.pendingQueueItems,
      expiredQueueItems: context.diagnostics.summary.expiredQueueItems,
      activeSessions: context.diagnostics.summary.activeSessions,
      channelDegraded:
        context.diagnostics.adapters?.channels.degradedBindings ?? 0,
      mcpDegraded: context.diagnostics.adapters?.mcp.degradedSurfaces ?? 0,
      recentErrors: context.diagnostics.summary.recentErrors,
    },
    items: boundedItems(
      context.diagnostics.issues.map((issue) => issue.message),
    ),
  };
}

function sessionResponse(
  context: SlashCommandResponseContext,
): SlashCommandResponse {
  const session = currentSession(context);
  const channelBindings =
    context.diagnostics.adapters?.channels.bindings.filter(
      (binding) => binding.sessionId === context.session.sessionId,
    ) ?? [];
  const mcpSurfaces =
    context.diagnostics.adapters?.mcp.surfaces.filter(
      (surface) => surface.sessionId === context.session.sessionId,
    ) ?? [];
  const currentIssues = context.diagnostics.issues.filter(
    (issue) => issue.sessionId === context.session.sessionId,
  );

  return {
    title: "Session",
    summary: session
      ? `Session ${session.sessionId} is ${session.status}.`
      : `Session ${context.session.sessionId} is missing from diagnostics.`,
    fields: {
      sessionId: context.session.sessionId,
      agentId: context.session.agentId,
      profileId: context.session.profileId,
      kind: context.session.kind,
      status: session?.status ?? "missing",
      stale: session?.stale ?? false,
      brainTurns: session?.brainTurnCount ?? 0,
      tools: session?.toolCount ?? 0,
      channelBindings: channelBindings.length,
      channelPresence: joinValues(
        channelBindings.map((binding) => binding.presenceStatus),
      ),
      channelStatus: joinValues(
        channelBindings.map((binding) => binding.status),
      ),
      mcpSurfaces: mcpSurfaces.length,
      mcpStatus: joinValues(mcpSurfaces.map((surface) => surface.status)),
      mcpCollisions: mcpSurfaces.reduce(
        (sum, surface) => sum + surface.collisionCount,
        0,
      ),
    },
    items: boundedItems([
      ...currentIssues.map((issue) => issue.message),
      ...channelBindings.flatMap((binding) =>
        binding.lastError
          ? [`channel ${binding.bindingId}: ${binding.lastError}`]
          : [],
      ),
      ...mcpSurfaces.flatMap((surface) =>
        surface.lastError
          ? [`mcp ${surface.bindingId}: ${surface.lastError}`]
          : [],
      ),
    ]),
  };
}

function currentSession(context: SlashCommandResponseContext) {
  return context.diagnostics.runtime.sessions.find(
    (session) => session.sessionId === context.session.sessionId,
  );
}

function joinValues(values: readonly string[]): string {
  return values.length > 0 ? [...new Set(values)].sort().join(",") : "none";
}

function boundedItems(items: readonly string[]): string[] {
  return items
    .filter((item) => item.length > 0)
    .slice(0, 6)
    .map((item) => (item.length > 180 ? `${item.slice(0, 177)}...` : item));
}
