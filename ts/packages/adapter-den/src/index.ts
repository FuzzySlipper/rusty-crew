import type {
  AdapterId,
  CoreEvent,
  DenDataUpdate,
  EventReceipt,
  ExternalEvent,
  ExternalEventPayload,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export * from "./den-channels.js";
export {
  DenChannelsTransportController,
  InMemoryDenChannelsCursorStore,
} from "./den-channel-transport.js";
export type {
  DenChannelsConnectivityState,
  DenChannelsCursorStore,
  DenChannelsInboundDecision,
  DenChannelsReconnectAttempt,
  DenChannelsRetryPolicy,
  DenChannelsTransport,
  DenChannelsTransportControllerOptions,
  DenChannelsTransportKind,
  DenChannelsTransportOpenRequest,
  DenChannelsTransportStatus,
} from "./den-channel-transport.js";
export * from "./channel-routing.js";
export * from "./channel-readback.js";
export * from "./completion-evidence-projection.js";
export * from "./den-product-ingress.js";
export * from "./den-router-metadata.js";
export * from "./channel-presence.js";
export * from "./channel-ingress.js";
export * from "./channel-projection.js";
export * from "./successor-gateway.js";
export * from "./memory-client.js";

export type DenAdapterConnectionState =
  | "connected"
  | "degraded"
  | "disconnected";

export interface DenProjection {
  adapterId: AdapterId;
  eventType: CoreEvent["type"];
  summary: string;
  event: CoreEvent;
}

export interface DenProjectionResult {
  accepted: boolean;
  dropped: boolean;
  reason?: string;
}

export interface DenProjectionSink {
  project(projection: DenProjection): Promise<void> | void;
}

export interface DenCoreIngress {
  injectDenDataUpdate(
    update: DenDataUpdate,
  ): Promise<EventReceipt> | EventReceipt;
  injectExternalEvent(
    event: ExternalEvent,
  ): Promise<EventReceipt> | EventReceipt;
}

export interface DenAdapterStatus {
  state: DenAdapterConnectionState;
  projectedEvents: number;
  droppedProjections: number;
  lastAcceptedSequence?: number;
  lastProjectionError?: string;
}

export interface DenAdapterOptions {
  adapterId: AdapterId;
  ingress: DenCoreIngress;
  projectionSink: DenProjectionSink;
  displayName?: string;
  projectionFailureMode?: "record" | "throw";
}

export interface DenAdapter {
  registration(): PlatformAdapterRegistration;
  status(): DenAdapterStatus;
  injectDataUpdate(update: DenDataUpdate): Promise<EventReceipt>;
  injectExternalEventPayload(
    source: string,
    payload: ExternalEventPayload,
  ): Promise<EventReceipt>;
  projectEvent(event: CoreEvent): Promise<DenProjectionResult>;
}

export function createDenAdapterRegistration(
  adapterId: AdapterId,
  displayName = "Den",
): PlatformAdapterRegistration {
  return { adapterId, kind: "den", displayName };
}

export function createDenAdapter(options: DenAdapterOptions): DenAdapter {
  const failureMode = options.projectionFailureMode ?? "record";
  const status: DenAdapterStatus = {
    state: "connected",
    projectedEvents: 0,
    droppedProjections: 0,
  };

  return {
    registration(): PlatformAdapterRegistration {
      return createDenAdapterRegistration(
        options.adapterId,
        options.displayName ?? "Den",
      );
    },

    status(): DenAdapterStatus {
      return { ...status };
    },

    async injectDataUpdate(update): Promise<EventReceipt> {
      const receipt = await options.ingress.injectDenDataUpdate(update);
      status.lastAcceptedSequence = receipt.sequence;
      return receipt;
    },

    async injectExternalEventPayload(source, payload): Promise<EventReceipt> {
      const receipt = await options.ingress.injectExternalEvent({
        adapterId: options.adapterId,
        source,
        payload,
      });
      status.lastAcceptedSequence = receipt.sequence;
      return receipt;
    },

    async projectEvent(event): Promise<DenProjectionResult> {
      const projection = toDenProjection(options.adapterId, event);

      try {
        await options.projectionSink.project(projection);
        status.state = "connected";
        status.projectedEvents += 1;
        status.lastProjectionError = undefined;
        return { accepted: true, dropped: false };
      } catch (error) {
        const reason = projectionErrorMessage(error);
        status.state = "degraded";
        status.droppedProjections += 1;
        status.lastProjectionError = reason;

        if (failureMode === "throw") {
          throw error;
        }

        return { accepted: false, dropped: true, reason };
      }
    },
  };
}

export function toDenProjection(
  adapterId: AdapterId,
  event: CoreEvent,
): DenProjection {
  return {
    adapterId,
    eventType: event.type,
    summary: summarizeCoreEvent(event),
    event,
  };
}

function summarizeCoreEvent(event: CoreEvent): string {
  switch (event.type) {
    case "session_created":
      return `session created for ${event.state.agentId}`;
    case "session_archived":
      return `session archived ${event.sessionId}`;
    case "agent_message_routed":
      if (event.message.correlationId?.startsWith("checkpoint:")) {
        return `delegation checkpoint routed to ${event.message.to}`;
      }
      return `agent message routed ${event.message.from} -> ${event.message.to}`;
    case "delegation_lifecycle_observed":
      return summarizeDelegationLifecycle(event.lifecycle);
    case "external_event_injected":
      return `external event injected from ${event.event.source}`;
    case "den_data_updated":
      return `den ${event.update.entityKind} updated ${event.update.entityId}`;
    case "brain_wake_requested":
      return `brain wake requested for ${event.sessionId}`;
    case "brain_event_observed":
      return `brain event observed for ${event.sessionId}`;
    case "brain_actions_accepted":
      return `brain accepted ${event.count} actions for ${event.sessionId}`;
    case "completion_packet_delivered":
      return `completion ${event.packet.status} for ${event.packet.sessionId}`;
  }
}

function summarizeDelegationLifecycle(
  lifecycle: Extract<
    CoreEvent,
    { type: "delegation_lifecycle_observed" }
  >["lifecycle"],
): string {
  switch (lifecycle.phase) {
    case "created":
      return `delegation created ${lifecycle.delegatedSessionId} from ${lifecycle.parentSessionId}`;
    case "wake_requested":
      return `delegation wake requested for ${lifecycle.delegatedSessionId}`;
    case "checkpoint_requested":
      return `delegation checkpoint requested for ${lifecycle.delegatedSessionId}`;
    case "completed":
    case "failed":
    case "blocked":
    case "exhausted":
      return `delegation ${lifecycle.phase} for ${lifecycle.delegatedSessionId}`;
    case "timed_out":
      return `delegation timed out for ${lifecycle.delegatedSessionId}`;
    case "cancelled":
      return `delegation cancelled for ${lifecycle.delegatedSessionId}`;
  }
}

function projectionErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
