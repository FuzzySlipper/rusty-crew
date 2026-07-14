import type {
  ExternalAgentBinding,
  ExternalAgentBindingMetadataWrite,
  ExternalAgentSessionCreationRecord,
  ExternalAgentSessionCreationRequest,
  ExternalControlReceipt,
  ExternalControlRequest,
  ExternalControlStatus,
  ExternalControllerContext,
  ExternalControllerLease,
  ExternalInteractionRecord,
  ExternalRuntimeEventInput,
  ExternalRuntimeHandshakeDecision,
  ExternalRuntimeHandshakeObservation,
  ExternalRuntimeRegistration,
  ExternalRuntimeStateObservation,
  ExternalTurnCorrelation,
  ExternalTurnPhase,
  NormalizedExternalRuntimeEvent,
} from "@rusty-crew/contracts";
import type { NativeExternalRuntimeCertificationBridgeMethods } from "./external-runtime-certification-public-api.js";
export interface NativeExternalRuntimeBridgeMethods extends NativeExternalRuntimeCertificationBridgeMethods {
  registerExternalRuntime(input: {
    registration: ExternalRuntimeRegistration;
    expectedRevision?: number;
  }): Promise<ExternalRuntimeRegistration>;
  authorizeExternalRuntimeHandshake(
    observation: ExternalRuntimeHandshakeObservation,
  ): Promise<ExternalRuntimeHandshakeDecision>;
  recordExternalRuntimeState(
    observation: ExternalRuntimeStateObservation,
  ): Promise<ExternalRuntimeRegistration>;
  listExternalRuntimes(): Promise<ExternalRuntimeRegistration[]>;
  getExternalRuntime(
    runtimeId: string,
  ): Promise<ExternalRuntimeRegistration | undefined>;
  acquireExternalController(input: {
    lease: ExternalControllerLease;
    now: string;
  }): Promise<ExternalControllerLease>;
  releaseExternalController(input: {
    runtimeId: string;
    holderInstanceId: string;
    generation: number;
    now: string;
  }): Promise<ExternalControllerLease>;
  bindExternalAgent(input: {
    binding: ExternalAgentBinding;
    expectedRevision?: number;
  }): Promise<ExternalAgentBinding>;
  updateExternalBindingMetadata(
    write: ExternalAgentBindingMetadataWrite,
  ): Promise<ExternalAgentBinding>;
  listExternalBindings(): Promise<ExternalAgentBinding[]>;
  getExternalBinding(
    bindingId: string,
  ): Promise<ExternalAgentBinding | undefined>;
  prepareExternalAgentSessionCreation(
    request: ExternalAgentSessionCreationRequest,
  ): Promise<ExternalAgentSessionCreationRecord>;
  markExternalAgentSessionNativeStarting(input: {
    controller: ExternalControllerContext;
    creationId: string;
    expectedRevision: number;
    now: string;
  }): Promise<ExternalAgentSessionCreationRecord>;
  completeExternalAgentSessionCreation(input: {
    controller: ExternalControllerContext;
    creationId: string;
    expectedRevision: number;
    nativeThreadId: string;
    now: string;
  }): Promise<ExternalAgentSessionCreationRecord>;
  recordExternalAgentSessionCreationFailure(input: {
    controller: ExternalControllerContext;
    creationId: string;
    expectedRevision: number;
    reasonCode: string;
    reasonMessage: string;
    now: string;
  }): Promise<ExternalAgentSessionCreationRecord>;
  getExternalTurn(
    requestId: string,
  ): Promise<ExternalTurnCorrelation | undefined>;
  listActiveExternalTurns(): Promise<ExternalTurnCorrelation[]>;
  expireExternalTurnDispatches(now: string): Promise<ExternalTurnCorrelation[]>;
  transitionExternalTurn(input: {
    controller: ExternalControllerContext;
    requestId: string;
    nextPhase: ExternalTurnPhase;
    nativeTurnId?: string;
    terminalReasonCode?: string;
    now: string;
  }): Promise<ExternalTurnCorrelation>;
  submitExternalControl(
    request: ExternalControlRequest,
  ): Promise<ExternalControlReceipt>;
  completeExternalControl(input: {
    controller: ExternalControllerContext;
    controlId: string;
    status: ExternalControlStatus;
    outcome?: unknown;
    reasonCode?: string;
    now: string;
  }): Promise<ExternalControlReceipt>;
  recordExternalInteraction(input: {
    controller: ExternalControllerContext;
    interaction: ExternalInteractionRecord;
  }): Promise<ExternalInteractionRecord>;
  resolveExternalInteraction(input: {
    interaction: ExternalInteractionRecord;
    expectedRevision: number;
  }): Promise<ExternalInteractionRecord>;
  terminalizeExternalInteraction(input: {
    controller: ExternalControllerContext;
    interaction: ExternalInteractionRecord;
    expectedRevision: number;
  }): Promise<ExternalInteractionRecord>;
  listPendingExternalInteractions(): Promise<ExternalInteractionRecord[]>;
  recordExternalRuntimeEvent(input: {
    controller: ExternalControllerContext;
    event: ExternalRuntimeEventInput;
  }): Promise<NormalizedExternalRuntimeEvent>;
  queryExternalRuntimeEvents(input: {
    runtimeId: string;
    afterSequence: number;
    limit: number;
  }): Promise<NormalizedExternalRuntimeEvent[]>;
}
