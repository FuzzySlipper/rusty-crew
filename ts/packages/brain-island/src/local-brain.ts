import type {
  AgentMessage as RustyAgentMessage,
  BodyState,
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainWakeProviderStateInput,
  BrainWakeProviderStateOutput,
  BrainWakeStreamItem,
  CompletionPacket,
  ProviderStateAbsenceReason,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  BrainWakeExecutor,
  NativeBridgeModule,
  OpenAiResponsesTransportMetrics,
  PiAgentTransportMetrics,
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
  providerStateAbsence?: ProviderStateAbsenceReason;
}

export interface BrainWakeOptions {
  signal?: AbortSignal;
}

export interface BrainWakeResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  stream?: BrainWakeStreamItem[];
  transportMetrics?: OpenAiResponsesTransportMetrics | PiAgentTransportMetrics;
  brainEventCounts?: Record<string, number>;
  brainStreamItemCounts?: Record<string, number>;
}

export interface BrainImplementation {
  wake(
    input: BrainWakeInput,
    options?: BrainWakeOptions,
  ): Promise<BrainWakeResult>;
}

export function createBrainWakeExecutor(
  brain: BrainImplementation,
): BrainWakeExecutor {
  return {
    wake(request, buffers, options): Promise<BrainWakeResult> {
      return wakeBrainFromBridgeRequest(buffers, brain, request, options);
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
  toolActions?: readonly BrainAction[];
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
