import type {
  ActionBatchReceipt,
  AgentMessage,
  BrainActionBatch,
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
  SessionId,
  RuntimeBufferHandle,
  RuntimeBufferView,
  ShutdownRequest,
  ShutdownSummary,
  SubscriptionHandle,
  Unit,
} from "@rusty-crew/contracts";
import {
  type BrainWakeSessionBufferInput,
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";

export class CoreBridge {
  private constructor(
    readonly manifestVersion: number,
    private readonly native: NativeBridgeModule,
  ) {}

  static async initialize(_config: EngineConfig): Promise<CoreBridge> {
    const native = await loadNativeBridge();
    return new CoreBridge(native.manifestVersion, native);
  }

  operationNames(): readonly ManifestOperationName[] {
    return this.native.operationNames;
  }

  async initializeEngine(config: EngineConfig): Promise<EngineHandle> {
    return this.native.initializeEngine(config);
  }

  async shutdownEngine(request: ShutdownRequest): Promise<ShutdownSummary> {
    return this.native.shutdownEngine(request);
  }

  async registerBrainImplementation(
    registration: BrainImplementationRegistration,
  ): Promise<BrainImplementationHandle> {
    return this.native.registerBrainImplementation(registration);
  }

  async wakeBrain(request: BrainWakeRequest): Promise<BrainWakeAccepted> {
    return this.native.wakeBrain(request);
  }

  async submitBrainEvent(event: BrainEventEnvelope): Promise<EventReceipt> {
    return this.native.submitBrainEvent(event);
  }

  async submitBrainActions(
    batch: BrainActionBatch,
  ): Promise<ActionBatchReceipt> {
    return this.native.submitBrainActions(batch);
  }

  async registerPlatformAdapter(
    registration: PlatformAdapterRegistration,
  ): Promise<PlatformAdapterHandle> {
    return this.native.registerPlatformAdapter(registration);
  }

  async injectDenDataUpdate(update: DenDataUpdate): Promise<EventReceipt> {
    return this.native.injectDenDataUpdate(update);
  }

  async injectExternalEvent(event: ExternalEvent): Promise<EventReceipt> {
    return this.native.injectExternalEvent(event);
  }

  async routeAgentMessage(message: AgentMessage): Promise<EventReceipt> {
    return this.native.routeAgentMessage(
      message.from,
      message.to,
      message.body,
      message.correlationId,
    );
  }

  async cancelDelegatedSession(
    delegatedSessionId: SessionId,
  ): Promise<
    Awaited<ReturnType<NativeBridgeModule["cancelDelegatedSession"]>>
  > {
    return this.native.cancelDelegatedSession(delegatedSessionId);
  }

  async requestDelegatedCheckpoint(input: {
    parentSessionId: SessionId;
    delegatedSessionId: SessionId;
    reason: string;
  }): Promise<EventReceipt> {
    return this.native.requestDelegatedCheckpoint(input);
  }

  async drainDelegatedSessions(input?: {
    parentSessionId?: SessionId;
  }): Promise<SessionId[]> {
    return this.native.drainDelegatedSessions(input);
  }

  async delegatedSessionStatus(
    delegatedSessionId: SessionId,
  ): Promise<DelegatedSessionRuntimeStatus> {
    return this.native.delegatedSessionStatus(delegatedSessionId);
  }

  async subscribeEvents(
    subscription: EventSubscription,
  ): Promise<SubscriptionHandle> {
    return this.native.subscribeEvents(subscription);
  }

  async unsubscribeEvents(handle: SubscriptionHandle): Promise<Unit> {
    return this.native.unsubscribeEvents(handle);
  }

  async drainSubscriptionEvents(
    handle: SubscriptionHandle,
    maxEvents?: number,
  ): Promise<CoreEvent[]> {
    return this.native.drainSubscriptionEvents(handle, maxEvents);
  }

  async getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView> {
    return this.native.getBuffer(handle);
  }

  async buildBrainWakeRequestForSession(
    input: BrainWakeSessionBufferInput,
  ): Promise<BrainWakeRequest> {
    return this.native.buildBrainWakeRequestForSession(input);
  }

  async releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit> {
    return this.native.releaseBuffer(handle);
  }
}

export * from "@rusty-crew/contracts";
