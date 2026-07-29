import type {
  AgentMessage as RustyAgentMessage,
  BodyState,
  BrainAction,
  BrainContinuationPayload,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainWakeAttention,
  BrainWakeProviderStateInput,
  BrainWakeProviderStateOutput,
  BrainWakeStreamItem,
  ProviderStateAbsenceReason,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  BrainWakeExecutor,
  NativeBridgeModule,
  OpenAiResponsesTransportMetrics,
  ChatCompletionsTransportMetrics,
  NativeBufferedBrainStreamRetentionMetrics,
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
  providerState?: BrainWakeProviderStateInput;
  continuationState?: BrainContinuationPayload;
  providerStateAbsence?: ProviderStateAbsenceReason;
}

export interface BrainWakeOptions {
  signal?: AbortSignal;
}

export interface BrainWakeResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  outcome?: "completed" | "yielded" | "attention_required";
  continuationState?: BrainContinuationPayload;
  attention?: BrainWakeAttention;
  stream?: BrainWakeStreamItem[];
  transportMetrics?:
    | OpenAiResponsesTransportMetrics
    | ChatCompletionsTransportMetrics;
  brainEventCounts?: Record<string, number>;
  brainStreamItemCounts?: Record<string, number>;
  streamRetentionMetrics?: NativeBufferedBrainStreamRetentionMetrics;
}

export interface BrainHostExecutor {
  wake(
    input: BrainWakeInput,
    options?: BrainWakeOptions,
  ): Promise<BrainWakeResult>;
}

export function createBrainHostWakeExecutor(
  brain: BrainHostExecutor,
): BrainWakeExecutor {
  return {
    wake(request, buffers, options): Promise<BrainWakeResult> {
      return wakeBrainFromBridgeRequest(buffers, brain, request, options);
    },
  };
}

export function registerBrainHostRuntime(
  bridge: NativeBridgeModule,
  registration: BrainImplementationRegistration,
  brain: BrainHostExecutor,
): Promise<BrainImplementationHandle> {
  return bridge.registerBrainRuntime(
    registration,
    createBrainHostWakeExecutor(brain),
  );
}

export type BrainActionPlanner = (input: {
  wake: BrainWakeInput;
  events: BrainEventEnvelope[];
  toolActions?: readonly BrainAction[];
}) => Promise<BrainAction[]> | BrainAction[];
