import type {
  BrainAction,
  BrainEventEnvelope,
  ToolProfile,
} from "@rusty-crew/contracts";
import type {
  PiAgentBrainRunInput,
  PiAgentTransportMetrics,
} from "@rusty-crew/native-bridge";
import type {
  BrainActionPlanner,
  BrainHostExecutor,
  BrainWakeInput,
  BrainWakeOptions,
  BrainWakeResult,
} from "./index.js";
import {
  createRoleplayNarratorBrain,
  type RoleplayNarratorPhaseBrainOptions,
} from "./narrator-brain.js";
import { createRoleplayNarratorFsmBridge } from "./roleplay-narrator-fsm.js";
import type { BrainToolResolver } from "./tool-session-selection.js";
import type { BrainHostContext } from "./brain-host-context.js";
import { brainWakeTimeoutMs } from "./brain-host-timeout.js";
import { providerRequestDebugEvent } from "./provider-debug-projection.js";
import { runBufferedBrainHost } from "./buffered-brain-host.js";

export function createPiAgentBrainHost(
  context: BrainHostContext,
  client: RustPiAgentClientConfig = rustPiAgentClientConfig(context),
): BrainHostExecutor {
  if (
    context.profile.profile.brain?.strategy === "roleplay_narrator" ||
    context.profile.profile.roleplayNarrator !== undefined
  ) {
    if (!context.bridge) {
      throw new Error("roleplay narrator Rust FSM bridge is required");
    }
    return createRoleplayNarratorBrain({
      narratorFsm: createRoleplayNarratorFsmBridge(context.bridge),
      createPhaseBrain: (phase: RoleplayNarratorPhaseBrainOptions) =>
        createRustPiAgentBrainHostExecutor(context, {
          moduleLabel: "pi-agent",
          client,
          toolResolver: phase.resolveTools,
          toolProfile: phase.toolProfile,
          submitEvent: phase.submitEvent,
          liveEvents: phase.submitEvent !== undefined,
          planActions: phase.planActions,
        }),
      planActions: context.planActions,
      resolveTools: context.toolResolver,
      toolProfile: context.profile.toolSelection.toolProfile,
      toolCallDebugStore: context.toolCallDebugStore,
      providerRequestDebugStore: context.providerRequestDebugStore,
      maxReviewCycles:
        context.profile.profile.roleplayNarrator?.review.maxReviewCycles,
      reviewEnabled: context.profile.profile.roleplayNarrator?.review.enabled,
      narratorConfig: context.profile.profile.roleplayNarrator,
      submitEvent: async (event) => {
        await context.bridge?.submitBrainEvent(event);
      },
    });
  }
  return createRustPiAgentBrainHostExecutor(context, {
    moduleLabel: "pi-agent",
    client,
    planActions: context.planActions,
  });
}

export type RustPiAgentClientConfig = NonNullable<
  PiAgentBrainRunInput["client"]
>;

function rustPiAgentClientConfig(
  context: BrainHostContext,
): RustPiAgentClientConfig {
  const baseUrl = context.profile.profile.modelConfig.baseUrl;
  if (!baseUrl) {
    throw new Error("rust-pi-agent live client requires modelConfig.baseUrl");
  }
  const keyEnv = context.profile.profile.modelConfig.apiKeyEnv;
  const apiKey = keyEnv ? process.env[keyEnv] : undefined;
  return {
    mode: "live",
    baseUrl,
    ...(apiKey ? { apiKey } : {}),
  };
}

interface RustPiAgentBrainImplementationOptions {
  moduleLabel: string;
  client: RustPiAgentClientConfig;
  toolResolver?: BrainToolResolver;
  toolProfile?: ToolProfile;
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  liveEvents?: boolean;
  planActions?: BrainActionPlanner;
}

function createRustPiAgentBrainHostExecutor(
  context: BrainHostContext,
  implementation: RustPiAgentBrainImplementationOptions,
): BrainHostExecutor {
  const client = implementation.client;
  return {
    async wake(wake, options): Promise<BrainWakeResult> {
      const bridge = context.bridge;
      if (bridge === undefined) {
        throw new Error(
          `${implementation.moduleLabel} brain requires native bridge`,
        );
      }
      const submitEvent =
        implementation.liveEvents === false
          ? undefined
          : (implementation.submitEvent ??
            (async (event: BrainEventEnvelope) => {
              await bridge.submitBrainEvent(event);
            }));
      const input: PiAgentBrainRunInput = {
        wakeId: wake.wakeId,
        sessionId: wake.sessionId,
        messages: rustPiAgentMessages(wake),
        config: {
          model: context.profile.profile.modelConfig.modelName,
          streamIdleTimeoutMs: rustPiAgentStreamIdleTimeoutMs(),
          wakeTimeoutMs: brainWakeTimeoutMs(context, wake),
          temperatureMilli:
            context.profile.profile.modelConfig.temperatureMilli,
          maxOutputTokens:
            context.profile.profile.modelConfig.maxOutputTokens ??
            context.maxTokens,
        },
        client,
      };
      const providerDebug = context.providerRequestDebugStore?.record({
        sessionId: wake.sessionId,
        wakeId: wake.wakeId,
        brainModule: implementation.moduleLabel,
        providerAlias: context.profile.profile.providerAlias,
        model: input.config.model,
        protocol: "chat_completions",
        providerKind: context.profile.profile.modelConfig.provider,
        request: {
          boundary: "ts_to_native_rust_pi_agent",
          wakeId: input.wakeId,
          sessionId: input.sessionId,
          messages: input.messages,
          config: input.config,
          client: input.client,
        },
      });
      const events: BrainEventEnvelope[] = [];
      if (providerDebug) {
        const event = providerRequestDebugEvent(wake, providerDebug);
        if (submitEvent) {
          await submitEvent(event);
        } else {
          events.push(event);
        }
      }
      return runRustPiAgentBrainWithIncrementalDrain(
        context,
        wake,
        input,
        options,
        {
          moduleLabel: implementation.moduleLabel,
          events,
          submitEvent,
          toolResolver: implementation.toolResolver,
          toolProfile: implementation.toolProfile,
          planActions: implementation.planActions,
        },
      );
    },
  };
}

function rustPiAgentStreamIdleTimeoutMs(
  env: Partial<
    Pick<NodeJS.ProcessEnv, "RUSTY_CREW_PI_AGENT_STREAM_IDLE_TIMEOUT_MS">
  > = process.env,
): number {
  const configured = Number.parseInt(
    env.RUSTY_CREW_PI_AGENT_STREAM_IDLE_TIMEOUT_MS ?? "",
    10,
  );
  if (Number.isFinite(configured) && configured > 0) {
    return configured;
  }
  return 300_000;
}

function rustPiAgentMessages(
  wake: BrainWakeInput,
): PiAgentBrainRunInput["messages"] {
  const system = [wake.systemPrompt, wake.roleAssembly.instructions]
    .filter(Boolean)
    .join("\n\n");
  return [
    ...(system ? [{ role: "system" as const, content: system }] : []),
    ...(wake.roleAssembly.initialMessages ?? []).map((message) => ({
      role: "user" as const,
      content: message.body,
    })),
    ...wake.state.pendingMessages.map((message) => ({
      role: "user" as const,
      content: message.body,
    })),
  ];
}

async function runRustPiAgentBrainWithIncrementalDrain(
  context: BrainHostContext,
  wake: BrainWakeInput,
  input: PiAgentBrainRunInput,
  options?: BrainWakeOptions,
  runOptions: {
    moduleLabel: string;
    events: BrainEventEnvelope[];
    submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
    toolResolver?: BrainToolResolver;
    toolProfile?: ToolProfile;
    planActions?: BrainActionPlanner;
  } = {
    moduleLabel: "pi-agent",
    events: [],
  },
): Promise<{
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  transportMetrics?: PiAgentTransportMetrics;
  brainEventCounts?: Record<string, number>;
  brainStreamItemCounts?: Record<string, number>;
}> {
  const bridge = context.bridge;
  if (bridge === undefined) {
    throw new Error("pi-agent host requires native bridge");
  }
  const result = await runBufferedBrainHost({
    bridge,
    run: { moduleId: "pi-agent", providerInput: input },
    moduleLabel: runOptions.moduleLabel,
    wake,
    wakeOptions: options,
    toolResolver: runOptions.toolResolver ?? context.toolResolver,
    toolProfile:
      runOptions.toolProfile ?? context.profile.toolSelection.toolProfile,
    toolCallDebugStore: context.toolCallDebugStore,
    events: runOptions.events,
    submitEvent: runOptions.submitEvent,
    planActions: runOptions.planActions,
  });
  return {
    ...result,
    transportMetrics: result.transportMetrics as
      | PiAgentTransportMetrics
      | undefined,
  };
}
