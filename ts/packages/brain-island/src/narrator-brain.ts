import type {
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  BrainPhase,
  BodyState,
  ToolDescriptor,
  ToolProfile,
} from "@rusty-crew/contracts";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type {
  BrainActionPlanner,
  BrainImplementation,
  BrainWakeInput,
  BrainWakeResult,
} from "./index.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type {
  RoleplayNarratorFsmBridge,
  RoleplayNarratorJsonValue,
  RoleplayNarratorConfig,
  RoleplayNarratorPhaseKind,
  RoleplayNarratorPhasePlan,
  RoleplayNarratorToolObservation,
  RoleplayNarratorToolRequest,
} from "./roleplay-narrator-fsm.js";
import {
  resolveToolSession,
  type BrainToolResolver,
} from "./tool-session-selection.js";

export interface RoleplayNarratorBrainOptions {
  createPhaseBrain: RoleplayNarratorPhaseBrainFactory;
  narratorFsm: RoleplayNarratorFsmBridge;
  resolveTools?: BrainToolResolver;
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  planActions?: BrainActionPlanner;
  maxReviewCycles?: number;
  reviewEnabled?: boolean;
  narratorConfig?: RoleplayNarratorConfig;
  toolProfile?: ToolProfile;
  toolCallDebugStore?: ToolCallDebugStore;
  providerRequestDebugStore?: ProviderRequestDebugStore;
}

export type RoleplayNarratorPhase =
  | "explore"
  | "compose"
  | "compose_draft"
  | "review";

export interface RoleplayNarratorPhaseBrainOptions {
  phase: RoleplayNarratorPhase;
  resolveTools?: BrainToolResolver;
  toolProfile?: ToolProfile;
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  planActions?: BrainActionPlanner;
}

export type RoleplayNarratorPhaseBrainFactory = (
  options: RoleplayNarratorPhaseBrainOptions,
) => BrainImplementation;

export function createRoleplayNarratorBrain(
  options: RoleplayNarratorBrainOptions,
): BrainImplementation {
  return {
    async wake(input): Promise<BrainWakeResult> {
      const phaseEvents: BrainEventEnvelope[] = [];
      const emitEvent = async (event: BrainEvent) => {
        const envelope = brainEventEnvelope(input, event);
        phaseEvents.push(envelope);
        await options.submitEvent?.(envelope);
      };
      const emitPhase = async (phase: BrainPhase, message?: string) => {
        await emitEvent({
          type: "phase_change",
          phase,
          message,
        });
      };

      await emitPhase("exploring", "Gathering lore and scene context.");
      const preludeObservations = await runMandatoryExplorePrelude(
        input,
        options.narratorFsm,
        options.resolveTools,
        options.toolProfile,
        emitEvent,
      );
      let reviewFeedback: string | undefined;
      let composeResult: BrainWakeResult | undefined;
      let plan = await options.narratorFsm.startTurn({
        narratorConfig: options.narratorConfig,
        reviewEnabled: options.reviewEnabled === true,
        maxReviewCycles: options.maxReviewCycles,
        preludeObservations,
      });

      while (plan.phase !== "done") {
        if (plan.phase !== "explore") {
          await emitNarratorPhase(plan.phase, emitPhase);
        }
        const result = await runNarratorPhasePlan(input, options, plan);
        const outputText = textFromEvents(result.events).trim();
        if (plan.phase === "review") {
          reviewFeedback = outputText;
        }
        if (plan.phase === "compose") {
          composeResult = result;
        }
        plan = await options.narratorFsm.nextPhase({
          state: plan.state,
          completedPhase: plan.phase,
          outputText,
        });
      }

      if (!composeResult) {
        throw new Error(
          "roleplay narrator FSM completed without compose phase",
        );
      }

      const reviewEvents: BrainEventEnvelope[] = [];
      if (reviewFeedback) {
        reviewEvents.push(
          brainEventEnvelope(input, {
            type: "provider_status",
            level: "info",
            message: "Narrator review completed.",
            metadataJson: JSON.stringify({ reviewFeedback }),
          }),
        );
      }

      await emitPhase("idle", "Narrator turn complete.");
      const visibleEvents = options.submitEvent
        ? []
        : [...phaseEvents, ...composeResult.events, ...reviewEvents];
      return {
        events: visibleEvents,
        actions: composeResult.actions,
      };
    },
  };
}

async function runNarratorPhasePlan(
  input: BrainWakeInput,
  options: RoleplayNarratorBrainOptions,
  plan: RoleplayNarratorPhasePlan,
): Promise<BrainWakeResult> {
  const phase = narratorPhase(plan.phase);
  const allowedTools = new Set(plan.allowedTools);
  const brain = createNarratorPhaseBrain(options, {
    phase,
    allowedTools,
    submitEvent: phase === "compose" ? options.submitEvent : undefined,
    planActions: phase === "compose" ? options.planActions : undefined,
  });
  return brain.wake(
    withFilteredTools(
      withNarratorInstructions(input, {
        instructions: plan.instructions,
      }),
      allowedTools,
    ),
  );
}

function narratorPhase(
  phase: RoleplayNarratorPhaseKind,
): RoleplayNarratorPhase {
  if (
    phase === "explore" ||
    phase === "compose" ||
    phase === "compose_draft" ||
    phase === "review"
  ) {
    return phase;
  }
  throw new Error(`roleplay narrator phase ${phase} cannot run as a wake`);
}

async function emitNarratorPhase(
  phase: RoleplayNarratorPhase,
  emitPhase: (phase: BrainPhase, message?: string) => Promise<void>,
): Promise<void> {
  if (phase === "review") {
    await emitPhase("reviewing", "Checking continuity and voice.");
    return;
  }
  if (phase === "compose_draft") {
    await emitPhase("composing", "Writing narrative response.");
    return;
  }
  await emitPhase("composing", "Writing final narrative response.");
}

function createNarratorPhaseBrain(
  options: RoleplayNarratorBrainOptions,
  phase: {
    phase: RoleplayNarratorPhase;
    allowedTools: ReadonlySet<string>;
    submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
    planActions?: BrainActionPlanner;
  },
): BrainImplementation {
  const resolveTools = filteringResolver(
    options.resolveTools,
    phase.allowedTools,
  );
  const toolProfile = filterToolProfile(
    options.toolProfile,
    phase.allowedTools,
  );
  return options.createPhaseBrain({
    phase: phase.phase,
    resolveTools,
    toolProfile,
    submitEvent: phase.submitEvent,
    planActions: phase.planActions,
  });
}

async function runMandatoryExplorePrelude(
  input: BrainWakeInput,
  narratorFsm: RoleplayNarratorFsmBridge,
  resolver: BrainToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
  emitEvent: (event: BrainEvent) => Promise<void>,
): Promise<RoleplayNarratorToolObservation[]> {
  const tools = resolveMandatoryExploreTools(input, resolver, toolProfile);
  const pendingText = pendingMessageText(input);
  const observations: RoleplayNarratorToolObservation[] = [];
  for (const request of await narratorFsm.mandatoryExploreRequests({
    sessionId: input.sessionId,
    profileId: input.state.session.profileId,
    pendingText,
  })) {
    const tool = tools.get(request.toolName);
    if (!tool) {
      observations.push({
        toolName: request.toolName,
        ok: false,
        summary: "tool was not available to the narrator explore phase",
      });
      continue;
    }
    observations.push(
      await runMandatoryExploreTool(input, tool, request, emitEvent),
    );
  }
  const layerObservation = observations.find(
    (observation) => observation.toolName === "list_lore_layers",
  );
  const autoCaptureRequest = await narratorFsm.autoCaptureRequest({
    sessionId: input.sessionId,
    profileId: input.state.session.profileId,
    wakeId: input.wakeId,
    pendingText,
    layerDetailsJson: layerObservation?.detailsJson ?? null,
  });
  if (autoCaptureRequest) {
    const tool = tools.get(autoCaptureRequest.toolName);
    if (!tool) {
      observations.push({
        toolName: autoCaptureRequest.toolName,
        ok: false,
        summary: "tool was not available to the narrator explore phase",
      });
    } else {
      observations.push(
        await runMandatoryExploreTool(
          input,
          tool,
          autoCaptureRequest,
          emitEvent,
        ),
      );
    }
  }
  return observations;
}

function resolveMandatoryExploreTools(
  input: BrainWakeInput,
  resolver: BrainToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
): Map<string, BrainTool> {
  const selection = resolveToolSession({
    wake: input,
    resolveTools: resolver,
    toolProfile,
  });
  return new Map(selection.tools.map((tool) => [tool.name, tool]));
}

async function runMandatoryExploreTool(
  input: BrainWakeInput,
  tool: BrainTool,
  request: RoleplayNarratorToolRequest,
  emitEvent: (event: BrainEvent) => Promise<void>,
): Promise<RoleplayNarratorToolObservation> {
  const callId = `${input.wakeId}:mandatory:${request.toolName}`;
  await emitEvent({
    type: "tool_call_started",
    toolName: request.toolName,
    metadata: mandatoryExploreToolMetadata(request.toolName),
  });
  try {
    const rawParams = paramsRecord(request.paramsJson);
    const params = tool.prepareArguments
      ? tool.prepareArguments(rawParams)
      : rawParams;
    const result = tool.executeWithContext
      ? await tool.executeWithContext(params as never, {
          wake: input,
          wakeId: input.wakeId,
          sessionId: input.sessionId,
          callId,
          signal: new AbortController().signal,
        })
      : await tool.execute(callId, params as never);
    await emitEvent({
      type: "tool_call_finished",
      toolName: request.toolName,
      isError: false,
      metadata: mandatoryExploreToolMetadata(request.toolName),
    });
    return {
      toolName: request.toolName,
      ok: true,
      summary: summarizeToolResult(result),
      detailsJson: narratorJsonValue(result.details),
    };
  } catch (error) {
    await emitEvent({
      type: "tool_call_finished",
      toolName: request.toolName,
      isError: true,
      metadata: mandatoryExploreToolMetadata(request.toolName),
    });
    return {
      toolName: request.toolName,
      ok: false,
      summary: error instanceof Error ? error.message : String(error),
    };
  }
}

function paramsRecord(value: unknown): Record<string, unknown> {
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return {};
}

function narratorJsonValue(value: unknown): RoleplayNarratorJsonValue {
  if (value === undefined) return null;
  return value as RoleplayNarratorJsonValue;
}

function pendingMessageText(input: BrainWakeInput): string {
  const text = input.state.pendingMessages
    .map((message) => message.body.trim())
    .filter(Boolean)
    .join("\n\n");
  return text.length > 0 ? text.slice(0, 4_000) : "Current roleplay turn.";
}

function mandatoryExploreToolMetadata(toolName: string) {
  return {
    source: "local" as const,
    serverNames: ["roleplay_narrator"],
    sourceToolName: toolName,
  };
}

function summarizeToolResult(result: BrainToolResult): string {
  const text = result.content
    .flatMap((item) => (item.type === "text" ? [item.text] : []))
    .join("\n")
    .trim();
  if (text.length > 0) return text.slice(0, 6_000);
  return JSON.stringify(result.details).slice(0, 6_000);
}

function filteringResolver(
  resolver: BrainToolResolver | undefined,
  allowedNames: ReadonlySet<string>,
): BrainToolResolver | undefined {
  if (!resolver) return undefined;
  return (input) =>
    resolver(input).filter((tool) => allowedNames.has(tool.name));
}

function withNarratorInstructions(
  input: BrainWakeInput,
  extra: { instructions: string },
): BrainWakeInput {
  return {
    ...input,
    roleAssembly: {
      ...input.roleAssembly,
      instructions: [input.roleAssembly.instructions, extra.instructions]
        .filter(Boolean)
        .join("\n\n"),
    },
  };
}

function withFilteredTools(
  input: BrainWakeInput,
  allowedNames: ReadonlySet<string>,
): BrainWakeInput {
  return {
    ...input,
    state: {
      ...input.state,
      session: {
        ...input.state.session,
        toolProfile: {
          tools: filterToolDescriptors(
            input.state.session.toolProfile.tools,
            allowedNames,
          ),
        },
      },
    } satisfies BodyState,
  };
}

function filterToolDescriptors(
  tools: readonly ToolDescriptor[],
  allowedNames: ReadonlySet<string>,
): ToolDescriptor[] {
  return tools.filter((tool) => allowedNames.has(tool.name));
}

function filterToolProfile(
  toolProfile: ToolProfile | undefined,
  allowedNames: ReadonlySet<string>,
): ToolProfile | undefined {
  if (toolProfile === undefined) return undefined;
  return {
    tools: filterToolDescriptors(toolProfile.tools, allowedNames),
  };
}

function brainEventEnvelope(
  input: BrainWakeInput,
  event: BrainEvent,
): BrainEventEnvelope {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    event,
  };
}

function textFromEvents(events: readonly BrainEventEnvelope[]): string {
  return events
    .flatMap((event) =>
      event.event.type === "text_delta" ? [event.event.text] : [],
    )
    .join("");
}
