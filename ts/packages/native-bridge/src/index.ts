import type {
  ActionBatchReceipt,
  BrainActionBatch,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainWakeAccepted,
  BrainWakeRequest,
  DenDataUpdate,
  EngineConfig,
  EngineHandle,
  EventReceipt,
  EventSubscription,
  ExternalEvent,
  ManifestOperationName,
  PlatformAdapterHandle,
  PlatformAdapterRegistration,
  RuntimeBufferHandle,
  RuntimeBufferView,
  ShutdownRequest,
  ShutdownSummary,
  SubscriptionHandle,
  Unit,
} from "@rusty-crew/contracts";

export interface NativeBridgeModule {
  readonly manifestVersion: number;
  readonly operationNames: readonly ManifestOperationName[];
  initializeEngine(config: EngineConfig): Promise<EngineHandle>;
  shutdownEngine(request: ShutdownRequest): Promise<ShutdownSummary>;
  registerBrainImplementation(
    registration: BrainImplementationRegistration,
  ): Promise<BrainImplementationHandle>;
  wakeBrain(request: BrainWakeRequest): Promise<BrainWakeAccepted>;
  submitBrainEvent(event: BrainEventEnvelope): Promise<EventReceipt>;
  submitBrainActions(batch: BrainActionBatch): Promise<ActionBatchReceipt>;
  registerPlatformAdapter(
    registration: PlatformAdapterRegistration,
  ): Promise<PlatformAdapterHandle>;
  injectDenDataUpdate(update: DenDataUpdate): Promise<EventReceipt>;
  injectExternalEvent(event: ExternalEvent): Promise<EventReceipt>;
  subscribeEvents(subscription: EventSubscription): Promise<SubscriptionHandle>;
  unsubscribeEvents(handle: SubscriptionHandle): Promise<Unit>;
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
  "subscribe_events",
  "unsubscribe_events",
  "get_buffer",
  "release_buffer",
] as const satisfies readonly ManifestOperationName[];

export async function loadNativeBridge(): Promise<NativeBridgeModule> {
  return createUnavailableNativeBridge();
}

export function createUnavailableNativeBridge(): NativeBridgeModule {
  return {
    manifestVersion: 1,
    operationNames: nativeManifestOperationNames,
    initializeEngine: unavailable("initialize_engine"),
    shutdownEngine: unavailable("shutdown_engine"),
    registerBrainImplementation: unavailable("register_brain_implementation"),
    wakeBrain: unavailable("wake_brain"),
    submitBrainEvent: unavailable("submit_brain_event"),
    submitBrainActions: unavailable("submit_brain_actions"),
    registerPlatformAdapter: unavailable("register_platform_adapter"),
    injectExternalEvent: unavailable("inject_external_event"),
    injectDenDataUpdate: unavailable("inject_den_data_update"),
    subscribeEvents: unavailable("subscribe_events"),
    unsubscribeEvents: unavailable("unsubscribe_events"),
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
