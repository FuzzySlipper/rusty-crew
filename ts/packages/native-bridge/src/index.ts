import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

import type {
  ActionBatchReceipt,
  AdapterId,
  AgentId,
  AgentMessage,
  BrainAction,
  BrainActionBatch,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainWakeAccepted,
  BrainWakeRequest,
  CoreEvent,
  DelegatedSessionRuntimeStatus,
  DenDataUpdate,
  EngineConfig,
  EngineHandle,
  EventReceipt,
  EventSubscription,
  ExternalEvent,
  ManifestOperationName,
  PlatformAdapterHandle,
  PlatformAdapterRegistration,
  ProfileId,
  ProjectId,
  RunId,
  RuntimeBufferHandle,
  RuntimeBufferView,
  SessionId,
  SessionState,
  ShutdownRequest,
  ShutdownSummary,
  SubscriptionHandle,
  TaskId,
  Unit,
} from "@rusty-crew/contracts";

interface NativeAddon {
  NativeBridgeBinding: new () => NativeBridgeBinding;
}

interface NativeBridgeBinding {
  readonly manifestVersion: number;
  readonly operationNames: string[];
  initializeEngine(config: {
    engineDataDir: string;
    fixedClock?: string;
    defaultTurnBudget: number;
    defaultIdleTimeoutMs: number;
  }): number;
  registerBrainImplementation(registration: {
    implementationId: string;
    profileId: string;
    toolProfile: {
      tools: Array<{
        name: string;
        description: string;
        inputSchema?: number;
      }>;
    };
    modelConfig: {
      provider: string;
      modelName: string;
      temperatureMilli?: number;
      maxOutputTokens?: number;
    };
  }): number;
  registerPlatformAdapter(registration: {
    adapterId: string;
    kind: string;
    displayName: string;
  }): number;
  shutdownEngine(
    engine: number,
    drainTimeoutMs: number,
  ): {
    archivedSessions: number;
    droppedSubscriptions: number;
  };
  submitBrainEvent(
    wakeId: string,
    sessionId: string,
    eventType: string,
    text?: string,
    toolName?: string,
    isError?: boolean,
  ): { accepted: boolean; sequence: number };
  injectExternalEvent(eventJson: Uint8Array): {
    accepted: boolean;
    sequence: number;
  };
  injectDenDataUpdate(updateJson: Uint8Array): {
    accepted: boolean;
    sequence: number;
  };
  cancelDelegatedSession(delegatedSessionId: string): {
    handle: number;
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
  };
  requestDelegatedCheckpoint(
    parentSessionId: string,
    delegatedSessionId: string,
    reason: string,
  ): { accepted: boolean; sequence: number };
  drainDelegatedSessions(parentSessionId?: string): string[];
  delegatedSessionStatusJson(delegatedSessionId: string): string;
  submitBrainTextDelta(
    wakeId: string,
    sessionId: string,
    text: string,
  ): { accepted: boolean; sequence: number };
  createSession(config: {
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
  }): {
    handle: number;
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
  };
  routeAgentMessage(
    from: string,
    to: string,
    body: string,
    correlationId?: string,
  ): { accepted: boolean; sequence: number };
  buildBrainWakeRequest(
    brain: number,
    sessionId: string,
    bodyStateJson: Uint8Array,
    systemPrompt: string,
    roleAssemblyJson: Uint8Array,
    wakeId: string,
  ): {
    bodyState: number;
    systemPrompt: number;
    roleAssembly: number;
  };
  buildBrainWakeRequestForSession(
    brain: number,
    sessionId: string,
    systemPrompt: string,
    roleAssemblyJson: Uint8Array,
    wakeId: string,
  ): {
    bodyState: number;
    systemPrompt: number;
    roleAssembly: number;
  };
  projectBodyStateJson(sessionId: string): Uint8Array;
  submitBrainActionsJson(
    wakeId: string,
    sessionId: string,
    actionsJson: Uint8Array,
  ): {
    wakeId: string;
    acceptedActions: number;
    rejectedActionsJson: string;
  };
  countRows(table: string): number;
  getBuffer(handle: number): {
    handle: number;
    mediaType: string;
    byteLen: number;
    bytes: Uint8Array;
  };
  releaseBuffer(handle: number): void;
  subscribeEvents(subscription: {
    eventKinds: string[];
    sessionId?: string;
    agentId?: string;
    adapterId?: string;
  }): number;
  unsubscribeEvents(handle: number): void;
  drainSubscriptionEvents(handle: number, maxEvents: number): string[];
}

export interface BridgeBufferClient {
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
}

export interface BrainWakeExecutionResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
}

export interface BrainWakeExecutor {
  wake(
    request: BrainWakeRequest,
    buffers: BridgeBufferClient,
  ): Promise<BrainWakeExecutionResult> | BrainWakeExecutionResult;
}

export interface BrainWakeBufferInput {
  brain: BrainImplementationHandle;
  sessionId: BrainWakeRequest["sessionId"];
  bodyStateJson: Uint8Array;
  systemPrompt: string;
  roleAssemblyJson: Uint8Array;
  wakeId: string;
}

export interface BrainWakeSessionBufferInput {
  brain: BrainImplementationHandle;
  sessionId: BrainWakeRequest["sessionId"];
  systemPrompt: string;
  roleAssemblyJson: Uint8Array;
  wakeId: string;
}

export interface NativeSessionStateSummary {
  handle: number;
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: string;
  status: string;
}

export interface NativeBridgeModule {
  readonly manifestVersion: number;
  readonly operationNames: readonly ManifestOperationName[];
  initializeEngine(config: EngineConfig): Promise<EngineHandle>;
  shutdownEngine(request: ShutdownRequest): Promise<ShutdownSummary>;
  registerBrainImplementation(
    registration: BrainImplementationRegistration,
  ): Promise<BrainImplementationHandle>;
  registerBrainRuntime(
    registration: BrainImplementationRegistration,
    executor: BrainWakeExecutor,
  ): Promise<BrainImplementationHandle>;
  wakeBrain(request: BrainWakeRequest): Promise<BrainWakeAccepted>;
  submitBrainEvent(event: BrainEventEnvelope): Promise<EventReceipt>;
  submitBrainActions(batch: BrainActionBatch): Promise<ActionBatchReceipt>;
  registerPlatformAdapter(
    registration: PlatformAdapterRegistration,
  ): Promise<PlatformAdapterHandle>;
  injectDenDataUpdate(update: DenDataUpdate): Promise<EventReceipt>;
  injectExternalEvent(event: ExternalEvent): Promise<EventReceipt>;
  cancelDelegatedSession(
    delegatedSessionId: SessionId,
  ): Promise<NativeSessionStateSummary>;
  requestDelegatedCheckpoint(input: {
    parentSessionId: SessionId;
    delegatedSessionId: SessionId;
    reason: string;
  }): Promise<EventReceipt>;
  drainDelegatedSessions(input?: {
    parentSessionId?: SessionId;
  }): Promise<SessionId[]>;
  delegatedSessionStatus(
    delegatedSessionId: SessionId,
  ): Promise<DelegatedSessionRuntimeStatus>;
  subscribeEvents(subscription: EventSubscription): Promise<SubscriptionHandle>;
  unsubscribeEvents(handle: SubscriptionHandle): Promise<Unit>;
  drainSubscriptionEvents(
    handle: SubscriptionHandle,
    maxEvents?: number,
  ): Promise<CoreEvent[]>;
  /**
   * Startup/config setup surface. This creates a Rust session for a configured
   * agent; it is not a brain wake-loop diagnostic bypass.
   */
  createSession(config: {
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: "full" | "worker" | "delegated";
  }): Promise<NativeSessionStateSummary>;
  /**
   * Internal agent-to-agent routing trigger. This publishes through
   * CoreEngine::route_agent_message and runs scheduler evaluation.
   */
  routeAgentMessage(
    from: string,
    to: string,
    body: string,
    correlationId?: string,
  ): Promise<EventReceipt>;
  /**
   * Runtime-local helper: projects body state in Rust and builds the three
   * runtime-buffer handles used by a registered brain wake.
   */
  buildBrainWakeRequest(input: BrainWakeBufferInput): Promise<BrainWakeRequest>;
  buildBrainWakeRequestForSession(
    input: BrainWakeSessionBufferInput,
  ): Promise<BrainWakeRequest>;
  diagnosticProjectBodyStateJson(sessionId: string): Promise<Uint8Array>;
  diagnosticSubmitBrainActionsJson(
    wakeId: string,
    sessionId: string,
    actions: BrainActionBatch["actions"],
  ): Promise<ActionBatchReceipt>;
  diagnosticCountRows(table: string): Promise<number>;
  /** @deprecated Diagnostic helper. Use diagnosticProjectBodyStateJson. */
  projectBodyStateJson(sessionId: string): Promise<Uint8Array>;
  /** @deprecated Diagnostic helper. Use diagnosticSubmitBrainActionsJson. */
  submitBrainActionsJson(
    wakeId: string,
    sessionId: string,
    actions: BrainActionBatch["actions"],
  ): Promise<ActionBatchReceipt>;
  /** @deprecated Diagnostic helper. Use diagnosticCountRows. */
  countRows(table: string): Promise<number>;
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
}

export const nativeManifestOperationNames = [
  "initialize_engine",
  "shutdown_engine",
  "register_brain_implementation",
  "wake_brain",
  "submit_brain_event",
  "submit_brain_actions",
  "register_platform_adapter",
  "inject_external_event",
  "inject_den_data_update",
  "cancel_delegated_session",
  "request_delegated_checkpoint",
  "drain_delegated_sessions",
  "delegated_session_status",
  "subscribe_events",
  "unsubscribe_events",
  "get_buffer",
  "release_buffer",
] as const satisfies readonly ManifestOperationName[];

export async function loadNativeBridge(): Promise<NativeBridgeModule> {
  const addon = loadNativeAddon();
  if (!addon) {
    return createUnavailableNativeBridge();
  }

  return createNativeBridgeModule(new addon.NativeBridgeBinding());
}

export function createUnavailableNativeBridge(): NativeBridgeModule {
  return {
    manifestVersion: 1,
    operationNames: nativeManifestOperationNames,
    initializeEngine: unavailable("initialize_engine"),
    shutdownEngine: unavailable("shutdown_engine"),
    registerBrainImplementation: unavailable("register_brain_implementation"),
    registerBrainRuntime: unavailable("register_brain_implementation"),
    wakeBrain: unavailable("wake_brain"),
    submitBrainEvent: unavailable("submit_brain_event"),
    submitBrainActions: unavailable("submit_brain_actions"),
    registerPlatformAdapter: unavailable("register_platform_adapter"),
    injectExternalEvent: unavailable("inject_external_event"),
    injectDenDataUpdate: unavailable("inject_den_data_update"),
    cancelDelegatedSession: unavailable("cancel_delegated_session"),
    requestDelegatedCheckpoint: unavailable("request_delegated_checkpoint"),
    drainDelegatedSessions: unavailable("drain_delegated_sessions"),
    delegatedSessionStatus: unavailable("delegated_session_status"),
    subscribeEvents: unavailable("subscribe_events"),
    unsubscribeEvents: unavailable("unsubscribe_events"),
    drainSubscriptionEvents: unavailable("subscribe_events"),
    createSession: unavailable("initialize_engine"),
    routeAgentMessage: unavailable("inject_external_event"),
    buildBrainWakeRequest: unavailable("wake_brain"),
    buildBrainWakeRequestForSession: unavailable("wake_brain"),
    diagnosticProjectBodyStateJson: unavailable("wake_brain"),
    diagnosticSubmitBrainActionsJson: unavailable("submit_brain_actions"),
    diagnosticCountRows: unavailable("initialize_engine"),
    projectBodyStateJson: unavailable("wake_brain"),
    submitBrainActionsJson: unavailable("submit_brain_actions"),
    countRows: unavailable("initialize_engine"),
    getBuffer: unavailable("get_buffer"),
    releaseBuffer: unavailable("release_buffer"),
  };
}

function unavailable<Args extends unknown[], Result>(
  operation: ManifestOperationName,
): (...args: Args) => Promise<Result> {
  return async () => {
    throw new Error(`native bridge operation ${operation} is unavailable`);
  };
}

function loadNativeAddon(): NativeAddon | undefined {
  const artifactName = nativeArtifactName();
  if (!artifactName) {
    return undefined;
  }

  try {
    const nativeRequire = createRequire(import.meta.url);
    const artifactPath = fileURLToPath(
      new URL(`../native/${artifactName}`, import.meta.url),
    );
    return nativeRequire(artifactPath) as NativeAddon;
  } catch {
    return undefined;
  }
}

function nativeArtifactName(): string | undefined {
  if (process.platform === "linux" && process.arch === "x64") {
    return "index.linux-x64-gnu.node";
  }

  return undefined;
}

function createNativeBridgeModule(
  binding: NativeBridgeBinding,
): NativeBridgeModule {
  const wakeExecutors = new Map<BrainImplementationHandle, BrainWakeExecutor>();
  const module: NativeBridgeModule = {
    manifestVersion: binding.manifestVersion,
    operationNames:
      binding.operationNames.length > 0
        ? (binding.operationNames as ManifestOperationName[])
        : nativeManifestOperationNames,
    initializeEngine: async (config) =>
      binding.initializeEngine({
        engineDataDir: config.engineDataDir,
        fixedClock: config.clock === "system" ? undefined : config.clock.fixed,
        defaultTurnBudget: config.defaultTurnBudget,
        defaultIdleTimeoutMs: config.defaultIdleTimeoutMs,
      }) as EngineHandle,
    shutdownEngine: async (request) =>
      binding.shutdownEngine(request.engine, request.drainTimeoutMs),
    registerBrainImplementation: async (registration) =>
      binding.registerBrainImplementation({
        implementationId: registration.implementationId,
        profileId: registration.profileId,
        toolProfile: {
          tools: registration.toolProfile.tools.map((tool) => ({
            name: tool.name,
            description: tool.description,
            inputSchema: tool.inputSchema,
          })),
        },
        modelConfig: {
          provider: registration.modelConfig.provider,
          modelName: registration.modelConfig.modelName,
          temperatureMilli: registration.modelConfig.temperatureMilli,
          maxOutputTokens: registration.modelConfig.maxOutputTokens,
        },
      }) as BrainImplementationHandle,
    registerBrainRuntime: async (registration, executor) => {
      const handle = await module.registerBrainImplementation(registration);
      wakeExecutors.set(handle, executor);
      return handle;
    },
    wakeBrain: async (request) => {
      const executor = wakeExecutors.get(request.brain);
      if (!executor) {
        throw new Error(
          `brain implementation handle ${request.brain} is not registered in the TS runtime`,
        );
      }

      const result = await executor.wake(request, module);
      for (const event of result.events) {
        await module.submitBrainEvent(event);
      }
      await module.submitBrainActions({
        wakeId: request.wakeId,
        sessionId: request.sessionId,
        actions: result.actions,
      });
      return { wakeId: request.wakeId, accepted: true };
    },
    submitBrainEvent: async (event) => {
      const nativeEvent = toNativeBrainEvent(event.event);
      return binding.submitBrainEvent(
        event.wakeId,
        event.sessionId,
        nativeEvent.eventType,
        nativeEvent.text,
        nativeEvent.toolName,
        nativeEvent.isError,
      );
    },
    submitBrainActions: async (batch) => {
      const receipt = binding.submitBrainActionsJson(
        batch.wakeId,
        batch.sessionId,
        new TextEncoder().encode(
          JSON.stringify(batch.actions.map(toNativeBrainAction)),
        ),
      );
      return {
        wakeId: receipt.wakeId,
        acceptedActions: receipt.acceptedActions,
        rejectedActions: JSON.parse(
          receipt.rejectedActionsJson,
        ) as ActionBatchReceipt["rejectedActions"],
      };
    },
    registerPlatformAdapter: async (registration) =>
      binding.registerPlatformAdapter({
        adapterId: registration.adapterId,
        kind: registration.kind,
        displayName: registration.displayName,
      }) as PlatformAdapterHandle,
    injectExternalEvent: async (event) =>
      binding.injectExternalEvent(encodeJson(toNativeExternalEvent(event))),
    injectDenDataUpdate: async (update) =>
      binding.injectDenDataUpdate(encodeJson(toNativeDenDataUpdate(update))),
    cancelDelegatedSession: async (delegatedSessionId) =>
      binding.cancelDelegatedSession(delegatedSessionId),
    requestDelegatedCheckpoint: async (input) =>
      binding.requestDelegatedCheckpoint(
        input.parentSessionId,
        input.delegatedSessionId,
        input.reason,
      ),
    drainDelegatedSessions: async (input) =>
      binding.drainDelegatedSessions(input?.parentSessionId) as SessionId[],
    delegatedSessionStatus: async (delegatedSessionId) =>
      toDelegatedSessionRuntimeStatus(
        JSON.parse(
          binding.delegatedSessionStatusJson(delegatedSessionId),
        ) as RawDelegatedSessionRuntimeStatus,
      ),
    subscribeEvents: async (subscription) =>
      binding.subscribeEvents({
        eventKinds: subscription.eventKinds,
        sessionId: subscription.sessionId,
        agentId: subscription.agentId,
        adapterId: subscription.adapterId,
      }) as SubscriptionHandle,
    unsubscribeEvents: async (handle) => {
      binding.unsubscribeEvents(handle);
      return {};
    },
    drainSubscriptionEvents: async (handle, maxEvents = 32) =>
      binding
        .drainSubscriptionEvents(handle, maxEvents)
        .map((eventJson) => toCoreEvent(JSON.parse(eventJson) as RawCoreEvent)),
    createSession: async (config) => binding.createSession(config),
    routeAgentMessage: async (from, to, body, correlationId) =>
      binding.routeAgentMessage(from, to, body, correlationId),
    buildBrainWakeRequest: async (input) => {
      const buffered = binding.buildBrainWakeRequest(
        input.brain,
        input.sessionId,
        input.bodyStateJson,
        input.systemPrompt,
        input.roleAssemblyJson,
        input.wakeId,
      );
      return {
        brain: input.brain,
        sessionId: input.sessionId as BrainWakeRequest["sessionId"],
        bodyState: buffered.bodyState as RuntimeBufferHandle,
        systemPrompt: buffered.systemPrompt as RuntimeBufferHandle,
        roleAssembly: buffered.roleAssembly as RuntimeBufferHandle,
        wakeId: input.wakeId,
      };
    },
    buildBrainWakeRequestForSession: async (input) => {
      const buffered = binding.buildBrainWakeRequestForSession(
        input.brain,
        input.sessionId,
        input.systemPrompt,
        input.roleAssemblyJson,
        input.wakeId,
      );
      return {
        brain: input.brain,
        sessionId: input.sessionId,
        bodyState: buffered.bodyState as RuntimeBufferHandle,
        systemPrompt: buffered.systemPrompt as RuntimeBufferHandle,
        roleAssembly: buffered.roleAssembly as RuntimeBufferHandle,
        wakeId: input.wakeId,
      };
    },
    diagnosticProjectBodyStateJson: async (sessionId) =>
      binding.projectBodyStateJson(sessionId),
    diagnosticSubmitBrainActionsJson: async (wakeId, sessionId, actions) => {
      const receipt = binding.submitBrainActionsJson(
        wakeId,
        sessionId,
        new TextEncoder().encode(
          JSON.stringify(actions.map(toNativeBrainAction)),
        ),
      );
      return {
        wakeId: receipt.wakeId,
        acceptedActions: receipt.acceptedActions,
        rejectedActions: JSON.parse(receipt.rejectedActionsJson) as [],
      };
    },
    diagnosticCountRows: async (table) => binding.countRows(table),
    projectBodyStateJson: async (sessionId) =>
      module.diagnosticProjectBodyStateJson(sessionId),
    submitBrainActionsJson: async (wakeId, sessionId, actions) =>
      module.diagnosticSubmitBrainActionsJson(wakeId, sessionId, actions),
    countRows: async (table) => module.diagnosticCountRows(table),
    getBuffer: async (handle) => {
      const view = binding.getBuffer(handle);
      return {
        ...view,
        handle: view.handle as RuntimeBufferHandle,
      };
    },
    releaseBuffer: async (handle) => {
      binding.releaseBuffer(handle);
      return {};
    },
  };

  return module;
}

function toNativeBrainAction(action: BrainAction): unknown {
  switch (action.type) {
    case "send_message":
      return {
        type: action.type,
        message: {
          from: action.message.from,
          to: action.message.to,
          body: action.message.body,
          correlation_id: action.message.correlationId,
        },
      };
    case "request_delegation":
      return {
        type: action.type,
        profile_id: action.profileId,
        task_id: action.taskId,
        prompt: action.prompt,
        expected_output: action.expectedOutput,
        resource_limits: action.resourceLimits
          ? {
              workdir: action.resourceLimits.workdir,
              max_duration_ms: action.resourceLimits.maxDurationMs,
              max_delegation_depth: action.resourceLimits.maxDelegationDepth,
            }
          : undefined,
        timeout_ms: action.timeoutMs,
        priority: action.priority,
        fan_out_group_id: action.fanOutGroupId,
        fan_out_max_concurrency: action.fanOutMaxConcurrency,
        fan_out_failure_policy: action.fanOutFailurePolicy,
        correlation_id: action.correlationId,
        parent_consumption: action.parentConsumption,
      };
    case "deliver_completion":
      return {
        type: action.type,
        packet: {
          session_id: action.packet.sessionId,
          status: action.packet.status,
          summary: action.packet.summary,
        },
      };
  }
}

function toNativeDenDataUpdate(update: DenDataUpdate): unknown {
  return {
    project_id: update.projectId,
    entity_kind: update.entityKind,
    entity_id: update.entityId,
    revision: update.revision,
  };
}

function toNativeExternalEvent(event: ExternalEvent): unknown {
  return {
    adapter_id: event.adapterId,
    source: event.source,
    payload: toNativeExternalEventPayload(event.payload),
  };
}

function toNativeExternalEventPayload(
  payload: ExternalEvent["payload"],
): unknown {
  switch (payload.type) {
    case "human_message":
      return payload;
    case "channel_message":
      return {
        type: payload.type,
        binding_id: payload.bindingId,
        correlation_id: payload.correlationId,
        idempotency_key: payload.idempotencyKey,
        provider: payload.provider,
        external_channel_id: payload.externalChannelId,
        external_thread_id: payload.externalThreadId,
        external_message_id: payload.externalMessageId,
        from: payload.from,
        text: payload.text,
        received_at: payload.receivedAt,
        expires_at: payload.expiresAt,
      };
    case "adapter_status":
      return payload;
    case "tool_catalog_changed":
      return {
        type: payload.type,
        catalog_id: payload.catalogId,
      };
    case "raw_json":
      return payload;
  }
}

function toExternalEventPayload(payload: unknown): ExternalEvent["payload"] {
  const raw = payload as Record<string, unknown>;
  switch (raw["type"]) {
    case "channel_message":
      return {
        type: "channel_message",
        bindingId: raw["binding_id"] as string,
        correlationId: raw["correlation_id"] as string,
        idempotencyKey: raw["idempotency_key"] as string,
        provider: raw["provider"] as string,
        externalChannelId: raw["external_channel_id"] as string,
        externalThreadId: raw["external_thread_id"] as string | undefined,
        externalMessageId: raw["external_message_id"] as string | undefined,
        from: raw["from"] as string,
        text: raw["text"] as string,
        receivedAt: raw["received_at"] as string,
        expiresAt: raw["expires_at"] as string,
      };
    case "tool_catalog_changed":
      return {
        type: "tool_catalog_changed",
        catalogId: raw["catalog_id"] as string,
      };
    default:
      return payload as ExternalEvent["payload"];
  }
}

function encodeJson(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

function toCoreEvent(event: RawCoreEvent): CoreEvent {
  switch (event.type) {
    case "session_created":
      return { type: event.type, state: toSessionState(event.state) };
    case "session_archived":
      return { type: event.type, sessionId: event.session_id };
    case "agent_message_routed":
      return { type: event.type, message: toAgentMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: toDelegationLifecycleEvent(event.lifecycle),
      };
    case "external_event_injected":
      return {
        type: event.type,
        event: {
          adapterId: event.event.adapter_id,
          source: event.event.source,
          payload: toExternalEventPayload(event.event.payload),
        },
      };
    case "den_data_updated":
      return {
        type: event.type,
        update: {
          projectId: event.update.project_id,
          entityKind: event.update.entity_kind,
          entityId: event.update.entity_id,
          revision: event.update.revision,
        },
      };
    case "brain_wake_requested":
      return { type: event.type, sessionId: event.session_id };
    case "brain_event_observed":
      return {
        type: event.type,
        sessionId: event.session_id,
        wakeId: event.wake_id,
        event: toBrainEvent(event.event),
      };
    case "brain_actions_accepted":
      return {
        type: event.type,
        sessionId: event.session_id,
        count: event.count,
      };
    case "completion_packet_delivered":
      return {
        type: event.type,
        packet: {
          sessionId: event.packet.session_id,
          status: event.packet.status,
          summary: event.packet.summary,
        },
      };
  }
}

function toDelegationLifecycleEvent(
  lifecycle: RawDelegationLifecycleEvent,
): Extract<CoreEvent, { type: "delegation_lifecycle_observed" }>["lifecycle"] {
  return {
    parentSessionId: lifecycle.parent_session_id,
    delegatedSessionId: lifecycle.delegated_session_id,
    runId: lifecycle.run_id,
    phase: lifecycle.phase,
    detail: lifecycle.detail,
  };
}

function toDelegatedSessionRuntimeStatus(
  status: RawDelegatedSessionRuntimeStatus,
): DelegatedSessionRuntimeStatus {
  return {
    session: toSessionState(status.session),
    parentSessionId: status.parent_session_id,
    runId: status.run_id,
    runStatus: status.run_status,
    terminal: status.terminal,
  };
}

function toSessionState(state: RawSessionState): SessionState {
  return {
    handle: state.handle as SessionState["handle"],
    sessionId: state.session_id,
    agentId: state.agent_id,
    profileId: state.profile_id,
    kind: state.kind,
    delegation: state.delegation
      ? {
          parentSessionId: state.delegation.parent_session_id,
          parentAgentId: state.delegation.parent_agent_id,
          sourceWakeId: state.delegation.source_wake_id,
          sourceActionIndex: state.delegation.source_action_index,
          requestedTaskId: state.delegation.requested_task_id,
          correlationId: state.delegation.correlation_id,
        }
      : undefined,
    resourceLimits: {
      workdir: state.resource_limits?.workdir,
      maxDurationMs: state.resource_limits?.max_duration_ms,
      maxDelegationDepth: state.resource_limits?.max_delegation_depth,
    },
    toolProfile: {
      tools: state.tool_profile?.tools ?? [],
    },
    status: state.status,
    brainTurnCount: state.brain_turn_count,
    createdAt: state.created_at,
    lastActiveAt: state.last_active_at,
  };
}

function toAgentMessage(message: RawAgentMessage): AgentMessage {
  return {
    from: message.from,
    to: message.to,
    body: message.body,
    correlationId: message.correlation_id,
  };
}

function toBrainEvent(event: RawBrainEvent): BrainEvent {
  switch (event.type) {
    case "started":
    case "finished":
      return event;
    case "text_delta":
      return { type: event.type, text: event.text };
    case "tool_call_started":
      return { type: event.type, toolName: event.tool_name };
    case "tool_call_finished":
      return {
        type: event.type,
        toolName: event.tool_name,
        isError: event.is_error,
      };
  }
}

function toNativeBrainEvent(event: BrainEvent): {
  eventType: string;
  text?: string;
  toolName?: string;
  isError?: boolean;
} {
  switch (event.type) {
    case "started":
      return { eventType: event.type };
    case "text_delta":
      return { eventType: event.type, text: event.text };
    case "tool_call_started":
      return { eventType: event.type, toolName: event.toolName };
    case "tool_call_finished":
      return {
        eventType: event.type,
        toolName: event.toolName,
        isError: event.isError,
      };
    case "finished":
      return { eventType: event.type };
  }
}

type RawCoreEvent =
  | { type: "session_created"; state: RawSessionState }
  | { type: "session_archived"; session_id: SessionId }
  | { type: "agent_message_routed"; message: RawAgentMessage }
  | {
      type: "delegation_lifecycle_observed";
      lifecycle: RawDelegationLifecycleEvent;
    }
  | {
      type: "external_event_injected";
      event: {
        adapter_id: AdapterId;
        source: string;
        payload: unknown;
      };
    }
  | {
      type: "den_data_updated";
      update: {
        project_id: ProjectId;
        entity_kind: string;
        entity_id: string;
        revision?: string;
      };
    }
  | { type: "brain_wake_requested"; session_id: SessionId }
  | {
      type: "brain_event_observed";
      session_id: SessionId;
      wake_id?: string;
      event: RawBrainEvent;
    }
  | {
      type: "brain_actions_accepted";
      session_id: SessionId;
      count: number;
    }
  | {
      type: "completion_packet_delivered";
      packet: {
        session_id: SessionId;
        status: Extract<
          CoreEvent,
          { type: "completion_packet_delivered" }
        >["packet"]["status"];
        summary: string;
      };
    };

interface RawDelegationLifecycleEvent {
  parent_session_id: SessionId;
  delegated_session_id: SessionId;
  run_id?: RunId;
  phase: Extract<
    CoreEvent,
    { type: "delegation_lifecycle_observed" }
  >["lifecycle"]["phase"];
  detail?: string;
}

interface RawDelegatedSessionRuntimeStatus {
  session: RawSessionState;
  parent_session_id?: SessionId;
  run_id?: RunId;
  run_status?: DelegatedSessionRuntimeStatus["runStatus"];
  terminal: boolean;
}

interface RawSessionState {
  handle: number;
  session_id: SessionId;
  agent_id: AgentId;
  profile_id: ProfileId;
  kind: SessionState["kind"];
  delegation?: {
    parent_session_id: SessionId;
    parent_agent_id: AgentId;
    source_wake_id: string;
    source_action_index: number;
    requested_task_id?: TaskId;
    correlation_id: string;
  };
  resource_limits?: {
    workdir?: string;
    max_duration_ms?: number;
    max_delegation_depth?: number;
  };
  tool_profile?: SessionState["toolProfile"];
  status: SessionState["status"];
  brain_turn_count: number;
  created_at: string;
  last_active_at: string;
}

interface RawAgentMessage {
  from: AgentId;
  to: AgentId;
  body: string;
  correlation_id?: string;
}

type RawBrainEvent =
  | { type: "started" }
  | { type: "text_delta"; text: string }
  | { type: "tool_call_started"; tool_name: string }
  | { type: "tool_call_finished"; tool_name: string; is_error: boolean }
  | { type: "finished" };
