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
  PiAgentToolResolver,
} from "./pi-agent-brain.js";
export { createPiAgentBrain } from "./pi-agent-brain.js";
export type { BridgeBufferClient } from "./bridge-wake.js";
export { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
export {
  BodyControlledDeltaQueue,
  defaultBodyDeltaPolicy,
} from "./mid-turn-delta.js";
export type { DrainResult, QueuedMidTurnMessage } from "./mid-turn-delta.js";
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
