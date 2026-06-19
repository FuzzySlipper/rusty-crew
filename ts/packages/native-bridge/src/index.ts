import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

import type {
  ActionBatchReceipt,
  BrainActionBatch,
  BrainEvent,
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
  submitBrainTextDelta(
    wakeId: string,
    sessionId: string,
    text: string,
  ): { accepted: boolean; sequence: number };
  getBuffer(handle: number): {
    handle: number;
    mediaType: string;
    byteLen: number;
    bytes: Uint8Array;
  };
  releaseBuffer(handle: number): void;
}

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
  return {
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
    registerBrainImplementation: unavailable("register_brain_implementation"),
    wakeBrain: unavailable("wake_brain"),
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
    submitBrainActions: unavailable("submit_brain_actions"),
    registerPlatformAdapter: unavailable("register_platform_adapter"),
    injectExternalEvent: unavailable("inject_external_event"),
    injectDenDataUpdate: unavailable("inject_den_data_update"),
    subscribeEvents: unavailable("subscribe_events"),
    unsubscribeEvents: unavailable("unsubscribe_events"),
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
