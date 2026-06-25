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
  DelegatedResourceCleanupReport,
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
  SessionId,
  RuntimeBufferHandle,
  RuntimeBufferView,
  ScheduledHostJobManualRunRequest,
  ScheduledHostJobRegistrationInput,
  ScheduledHostRunClaimQuery,
  ScheduledHostRunCompletionInput,
  ScheduledJobListQuery,
  ScheduledJobSummary,
  ScheduledRunListQuery,
  ScheduledRunSummary,
  SchedulerTickReport,
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

  async replaceBrainImplementation(
    registration: BrainImplementationRegistration,
  ): Promise<BrainImplementationHandle> {
    return this.native.replaceBrainImplementation(registration);
  }

  async unregisterBrainImplementationForProfile(
    profileId: ProfileId,
  ): Promise<BrainImplementationHandle> {
    return this.native.unregisterBrainImplementationForProfile(profileId);
  }

  async saveMessageSlot(input: unknown): Promise<void> {
    return this.native.saveMessageSlot(input);
  }

  async saveMessageVariant(input: unknown): Promise<unknown> {
    return this.native.saveMessageVariant(input);
  }

  async queryMessageSlots(query: unknown): Promise<unknown[]> {
    return this.native.queryMessageSlots(query);
  }

  async queryMessageVariants(query: unknown): Promise<unknown[]> {
    return this.native.queryMessageVariants(query);
  }

  async selectActiveMessageVariant(input: unknown): Promise<unknown> {
    return this.native.selectActiveMessageVariant(input);
  }

  async deleteMessageVariant(input: unknown): Promise<unknown> {
    return this.native.deleteMessageVariant(input);
  }

  async reorderMessageVariants(input: unknown): Promise<unknown[]> {
    return this.native.reorderMessageVariants(input);
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

  async registerScheduledWakeJob(input: {
    jobId: string;
    targetSessionId: SessionId;
    intervalMs?: number;
    firstDueAt: string;
  }): Promise<ScheduledJobSummary> {
    return this.native.registerScheduledWakeJob(input);
  }

  async registerScheduledHostJob(
    input: ScheduledHostJobRegistrationInput,
  ): Promise<ScheduledJobSummary> {
    return this.native.registerScheduledHostJob(input);
  }

  async listScheduledJobs(
    query?: ScheduledJobListQuery,
  ): Promise<ScheduledJobSummary[]> {
    return this.native.listScheduledJobs(query);
  }

  async listScheduledRuns(
    query?: ScheduledRunListQuery,
  ): Promise<ScheduledRunSummary[]> {
    return this.native.listScheduledRuns(query);
  }

  async claimScheduledHostRuns(
    query: ScheduledHostRunClaimQuery,
  ): Promise<ScheduledRunSummary[]> {
    return this.native.claimScheduledHostRuns(query);
  }

  async requestScheduledHostJobRun(
    input: ScheduledHostJobManualRunRequest,
  ): Promise<ScheduledRunSummary | undefined> {
    return this.native.requestScheduledHostJobRun(input);
  }

  async completeScheduledHostRun(
    input: ScheduledHostRunCompletionInput,
  ): Promise<Unit> {
    return this.native.completeScheduledHostRun(input);
  }

  async runSchedulerTick(): Promise<SchedulerTickReport> {
    return this.native.runSchedulerTick();
  }

  async requestScheduledJobRun(
    jobId: string,
  ): Promise<ScheduledRunSummary | undefined> {
    return this.native.requestScheduledJobRun(jobId);
  }

  async pauseScheduledJob(jobId: string): Promise<Unit> {
    return this.native.pauseScheduledJob(jobId);
  }

  async resumeScheduledJob(input: {
    jobId: string;
    nextDueAt: string;
  }): Promise<Unit> {
    return this.native.resumeScheduledJob(input);
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

  async cleanupDelegatedResources(): Promise<DelegatedResourceCleanupReport> {
    return this.native.cleanupDelegatedResources();
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
