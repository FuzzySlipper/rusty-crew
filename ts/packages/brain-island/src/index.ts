import type {
  AgentMessage as RustyAgentMessage,
  BodyState,
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  CompletionPacket,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  BrainWakeExecutor,
  NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { wakeBrainFromBridgeRequest } from "./bridge-wake.js";

export interface BrainRoleAssembly {
  instructions?: string;
  initialMessages?: RustyAgentMessage[];
}

export interface BrainWakeInput {
  wakeId: string;
  sessionId: SessionId;
  state: BodyState;
  systemPrompt: string;
  roleAssembly: BrainRoleAssembly;
}

export interface BrainWakeResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
}

export interface BrainImplementation {
  wake(input: BrainWakeInput): Promise<BrainWakeResult>;
}

export function createBrainWakeExecutor(
  brain: BrainImplementation,
): BrainWakeExecutor {
  return {
    wake(request, buffers): Promise<BrainWakeResult> {
      return wakeBrainFromBridgeRequest(buffers, brain, request);
    },
  };
}

export function registerBrainImplementationRuntime(
  bridge: NativeBridgeModule,
  registration: BrainImplementationRegistration,
  brain: BrainImplementation,
): Promise<BrainImplementationHandle> {
  return bridge.registerBrainRuntime(
    registration,
    createBrainWakeExecutor(brain),
  );
}

export type BrainActionPlanner = (input: {
  wake: BrainWakeInput;
  events: BrainEventEnvelope[];
}) => Promise<BrainAction[]> | BrainAction[];

export function createLocalBrain(
  planner: BrainActionPlanner = defaultActionPlanner,
): BrainImplementation {
  return {
    async wake(input): Promise<BrainWakeResult> {
      const events = [
        envelope(input, { type: "started" }),
        envelope(input, {
          type: "text_delta",
          text: `local brain woke ${input.state.session.agentId}`,
        }),
        envelope(input, { type: "finished" }),
      ];

      return {
        events,
        actions: await planner({ wake: input, events }),
      };
    },
  };
}

export const createPlaceholderBrain = createLocalBrain;

function defaultActionPlanner({
  wake,
}: {
  wake: BrainWakeInput;
}): BrainAction[] {
  return [
    {
      type: "deliver_completion",
      packet: {
        sessionId: wake.sessionId,
        status: "completed",
        summary: "local brain smoke wake completed",
      } satisfies CompletionPacket,
    },
  ];
}

export function envelope(
  input: BrainWakeInput,
  event: BrainEvent,
): BrainEventEnvelope {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    event,
  };
}

export type {
  PiAgentBrainOptions,
  PiAgentFactory,
  PiAgentLike,
} from "./pi-agent-brain.js";
export { createPiAgentBrain } from "./pi-agent-brain.js";
export { resolveToolSession } from "./tool-session-selection.js";
export type {
  PiAgentToolResolver,
  ToolSessionSelection,
  ToolSessionSelectionInput,
  ToolSessionSelectionItem,
  ToolSessionSelectionStatus,
} from "./tool-session-selection.js";
export type { BridgeBufferClient } from "./bridge-wake.js";
export { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
export {
  BodyControlledDeltaQueue,
  defaultBodyDeltaPolicy,
} from "./mid-turn-delta.js";
export type { DrainResult, QueuedMidTurnMessage } from "./mid-turn-delta.js";
export {
  adapterActivity,
  adminCommandActivity,
  AgentActivityObservationProducer,
  createAgentActivityObservationEvent,
  createMemoryAgentActivityObservationSink,
  sessionActivity,
  toolActivity,
  workActivity,
} from "./agent-activity-observation.js";
export type {
  AgentActivityEventInput,
  AgentActivityEventType,
  AgentActivityObservationEvent,
  AgentActivityObservationSink,
  AgentActivityPayload,
  AgentActivityPublishResult,
  AgentActivityResultRef,
  AgentActivitySeverity,
  AgentActivityVisibility,
  AgentActivityWorkRef,
  AgentObservationIdentity,
  MemoryAgentActivityObservationSink,
  ObservationSourceDomain,
} from "./agent-activity-observation.js";
export {
  createRuntimeActivityObserver,
  RuntimeActivityObserver,
} from "./runtime-activity-observer.js";
export type {
  RuntimeActivityObserverOptions,
  RuntimeActivityResult,
  RuntimeAdapterActivityInput,
  RuntimeSessionActivityInput,
  RuntimeToolActivityInput,
  RuntimeWorkActivityInput,
} from "./runtime-activity-observer.js";
export {
  createDenRouterPiAgentFactory,
  resolveDenRouterModel,
} from "./den-router-agent.js";
export type {
  DenRouterAgentOptions,
  DenRouterModelSelection,
} from "./den-router-agent.js";
export {
  buildDelegatedRoleAssembly,
  normalizeDelegatedRole,
} from "./delegated-role-assembly.js";
export type {
  BuildDelegatedRoleAssemblyInput,
  DelegatedProfileData,
  DelegatedRole,
  DelegatedRoleInput,
  DelegationRoleContext,
} from "./delegated-role-assembly.js";
export {
  gitDiffTool,
  gitStatusTool,
  readFileTool,
  resolveLocalCodeTools,
  searchFilesTool,
  terminalTool,
  writeFileTool,
} from "./local-code-tools.js";
export type {
  LocalToolContext,
  LocalToolProcessResult,
} from "./local-code-tools.js";
export { patchTool } from "./patch-tool.js";
export {
  buildToolRegistryDiagnostics,
  formatToolRegistryDiagnosticsMarkdown,
} from "./tool-registry-diagnostics.js";
export type {
  ToolRegistryDiagnosticTool,
  ToolRegistryDiagnosticsInput,
  ToolRegistryDiagnosticsReport,
  ToolRegistryDiagnosticsSummary,
} from "./tool-registry-diagnostics.js";
export {
  integrateMcpToolsWithRegistry,
  mcpCandidateToRegistryEntry,
} from "./mcp-tool-registry-integration.js";
export type {
  McpNameCollisionPolicy,
  McpRegistryIntegrationInput,
  McpRegistryIntegrationReport,
  McpToolRegistryEntry,
} from "./mcp-tool-registry-integration.js";
export { reloadMcpSurface } from "./mcp-surface-reload.js";
export type {
  McpSurfaceReloadInput,
  McpSurfaceReloadReport,
  McpToolDiff,
} from "./mcp-surface-reload.js";
export {
  createMcpToolCallMetadata,
  createMcpToolFinishedEvent,
  createMcpToolStartedEvent,
  evaluateMcpResourceHooks,
} from "./mcp-tool-telemetry.js";
export type {
  McpResourceDenialReason,
  McpResourceHookDecision,
  McpResourceHookInput,
  McpToolTelemetryInput,
} from "./mcp-tool-telemetry.js";
export { buildAdapterDiagnosticsProjection } from "./adapter-diagnostics.js";
export type {
  AdapterDiagnosticsInput,
  AdapterDiagnosticsProjection,
  AdapterHealthStatus,
  ChannelAdapterBindingDiagnostics,
  ChannelProjectionFailureRecord,
  McpAdapterSurfaceDiagnostics,
} from "./adapter-diagnostics.js";
export { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
export type {
  DiagnosticsHealth,
  DiagnosticsIssue,
  DiagnosticsReasonCode,
  ObservationDiagnosticsInput,
  ObservationDiagnosticsProjection,
  PersistenceDiagnosticsInput,
  PersistenceDiagnosticsProjection,
  QueueDiagnosticsInput,
  QueueDiagnosticsProjection,
  RuntimeCounterSummary,
  RuntimeDelegationDiagnostics,
  RuntimeDiagnosticError,
  RuntimeDiagnosticsInput,
  RuntimeDiagnosticsProjection,
  RuntimeSessionDiagnostics,
  ToolDiagnosticsProjection,
} from "./runtime-diagnostics.js";
export { buildRuntimeHealthProjection, issueDomain } from "./runtime-health.js";
export type {
  RuntimeDegradedStatus,
  RuntimeHealthDomain,
  RuntimeHealthDomainStatus,
  RuntimeHealthProbe,
  RuntimeHealthProjection,
  RuntimeMetricSample,
  RuntimeReadinessProbe,
} from "./runtime-health.js";
export {
  buildToolContextDiagnosticsReport,
  formatToolContextDiagnosticsMarkdown,
} from "./tool-context-diagnostics.js";
export type {
  TextSurfaceSummary,
  ToolContextAdapterSummary,
  ToolContextAssemblySummary,
  ToolContextDiagnosticTool,
  ToolContextDiagnosticsInput,
  ToolContextDiagnosticsIssue,
  ToolContextDiagnosticsReport,
  ToolContextDiagnosticsSession,
  ToolContextDiagnosticsSummary,
  ToolContextPolicySummary,
  ToolContextReasonCode,
  ToolContextResourceSummary,
  ToolContextSkillSummary,
  ToolContextToolStatus,
} from "./tool-context-diagnostics.js";
export {
  inspectDirectDebugSession,
  requestDirectDebugTurn,
} from "./direct-debug-service.js";
export type {
  DirectDebugContextView,
  DirectDebugControlSummary,
  DirectDebugErrorCode,
  DirectDebugInspectRequest,
  DirectDebugMessageSummary,
  DirectDebugRecentEventSummary,
  DirectDebugResult,
  DirectDebugRuntimeSummary,
  DirectDebugServiceContext,
  DirectDebugSessionSource,
  DirectDebugSessionSummary,
  DirectDebugSessionView,
  DirectDebugTurnExecutor,
  DirectDebugTurnExecutorInput,
  DirectDebugTurnOutcome,
  DirectDebugTurnRequest,
} from "./direct-debug-service.js";
export {
  createDebugApiClient,
  DebugApiClientError,
} from "./debug-api-client.js";
export type {
  DebugApiClient,
  DebugApiClientOptions,
  DebugApiFetch,
  DebugApiQuery,
  DebugDiagnosticsBundle,
  DirectDebugContextRequest,
} from "./debug-api-client.js";
export { handleAdminDiagnosticsRequest } from "./admin-diagnostics-api.js";
export type {
  AdminAgentDiagnostics,
  AdminApiEnvelope,
  AdminApiMeta,
  AdminDiagnosticsContext,
  AdminDiagnosticsRouteRequest,
  AdminErrorCode,
  AdminPage,
  AdminRecentEvent,
  AdminRouteResult,
} from "./admin-diagnostics-api.js";
export {
  createMemoryAdminControlAuditSink,
  handleAdminControlRequest,
} from "./admin-control-api.js";
export type {
  AdminControlActor,
  AdminControlAuditEvent,
  AdminControlAuditSink,
  AdminControlAuthConfig,
  AdminControlCommand,
  AdminControlCommandName,
  AdminControlContext,
  AdminControlExecutor,
  AdminControlOutcome,
  AdminControlResponse,
  AdminControlRouteRequest,
  AdminControlStatus,
  MemoryAdminControlAuditSink,
} from "./admin-control-api.js";
export { routeSlashCommand } from "./slash-command-router.js";
export type {
  SlashCommandActor,
  SlashCommandControlRequest,
  SlashCommandInput,
  SlashCommandName,
  SlashCommandResponse,
  SlashCommandRouteResult,
  SlashCommandRouterOptions,
  SlashCommandSession,
  SlashCommandStatus,
} from "./slash-command-router.js";
export { buildReadOnlySlashCommandResponse } from "./slash-command-responses.js";
export type { SlashCommandResponseContext } from "./slash-command-responses.js";
export {
  createMemoryNewSessionLifecycleAuditSink,
  createNewSessionLifecycleExecutor,
} from "./new-session-lifecycle.js";
export type {
  MemoryNewSessionLifecycleAuditSink,
  NewSessionLifecycleAuditEvent,
  NewSessionLifecycleAuditSink,
  NewSessionLifecycleOptions,
  NewSessionLifecyclePhase,
  NewSessionTemplate,
} from "./new-session-lifecycle.js";
export {
  createMemoryReloadMcpLifecycleAuditSink,
  createReloadMcpControlExecutor,
} from "./reload-mcp-control.js";
export type {
  MemoryReloadMcpLifecycleAuditSink,
  ReloadMcpControlOptions,
  ReloadMcpLifecycleAuditEvent,
  ReloadMcpLifecycleAuditSink,
  ReloadMcpLifecyclePhase,
} from "./reload-mcp-control.js";
export {
  assertValidToolRegistry,
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  toToolDescriptor,
  validateToolRegistry,
  ToolRegistry,
} from "./tool-registry.js";
export type {
  ToolCategory,
  ToolDeprecation,
  ToolInventory,
  ToolInventoryItem,
  ToolInventoryRequest,
  ToolInventoryStatus,
  ToolRegistryEntry,
  ToolRegistryValidation,
  ToolRegistryValidationIssue,
  ToolSafetyFlag,
  ToolSurface,
} from "./tool-registry.js";
export {
  buildBrainRegistrationFromToolProfile,
  createToolCatalogChangedPayload,
  selectToolProfile,
} from "./tool-profile-selection.js";
export type {
  BrainRegistrationFromToolProfileInput,
  ProfileToolPolicy,
  SessionToolConstraints,
  ToolProfileSelection,
  ToolProfileSelectionInput,
} from "./tool-profile-selection.js";
export {
  loadProfileConfig,
  loadProfileContext,
  loadSkill,
  ProfileLoadError,
} from "./profile-loading.js";
export type {
  LoadedProfileContext,
  LoadedSkill,
  LoadProfileContextInput,
  ProfileConfig,
  ProfileLoadErrorCode,
  ProfilePromptFragments,
  ProfileRuntimeConfig,
} from "./profile-loading.js";
export { buildProfileRoleAssembly } from "./profile-role-assembly.js";
export type {
  BuildProfileRoleAssemblyOptions,
  ProfileRoleAssemblyResult,
} from "./profile-role-assembly.js";
