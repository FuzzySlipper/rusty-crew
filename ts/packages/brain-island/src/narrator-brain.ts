import type {
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  BodyState,
  ToolDescriptor,
  ToolProfile,
} from "@rusty-crew/contracts";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type {
  BrainActionPlanner,
  BrainHostExecutor,
  BrainWakeInput,
  BrainWakeResult,
} from "./index.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type {
  RoleplayNarratorFsmBridge,
  RoleplayNarratorJsonValue,
  RoleplayNarratorConfig,
  RoleplayNarratorProviderPhase,
  RoleplayNarratorTurnReceipt,
  RoleplayNarratorToolObservation,
  RoleplayNarratorToolRequest,
} from "./roleplay-narrator-fsm.js";
import {
  resolveToolSession,
  type BrainToolResolver,
} from "./tool-session-selection.js";
import { roleplayCompactionDomainContext } from "./roleplay-compaction-domain-context.js";

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
  persistDiagnostic?: (input: {
    wakeId: string;
    sessionId: string;
    profileId: string;
    sceneBrief: string;
    relevantLoreRecordIds: string[];
  }) => Promise<void>;
}

export type RoleplayNarratorPhase = RoleplayNarratorProviderPhase;

export interface RoleplayNarratorPhaseBrainOptions {
  phase: RoleplayNarratorPhase;
  resolveTools?: BrainToolResolver;
  toolProfile?: ToolProfile;
  submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
  planActions?: BrainActionPlanner;
  compactionDomainContext?: unknown;
}

export type RoleplayNarratorPhaseBrainFactory = (
  options: RoleplayNarratorPhaseBrainOptions,
) => BrainHostExecutor;

export function createRoleplayNarratorBrain(
  options: RoleplayNarratorBrainOptions,
): BrainHostExecutor {
  return {
    async wake(input): Promise<BrainWakeResult> {
      const phaseEvents: BrainEventEnvelope[] = [];
      const emitEvent = async (event: BrainEvent) => {
        const envelope = brainEventEnvelope(input, event);
        phaseEvents.push(envelope);
        await options.submitEvent?.(envelope);
      };
      let composeResult: BrainWakeResult | undefined;
      let receipt = await options.narratorFsm.startTurn({
        wakeId: input.wakeId,
        sessionId: input.sessionId,
        profileId: input.state.session.profileId,
        pendingText: pendingMessageText(input),
        narratorConfig: options.narratorConfig,
        reviewEnabled: options.reviewEnabled === true,
        maxReviewCycles: options.maxReviewCycles,
      });

      while (!receipt.terminal) {
        if (receipt.activity) {
          await emitEvent({
            type: "phase_change",
            phase: receipt.activity.phase,
            message: receipt.activity.message,
          });
        }
        if (receipt.directive.kind === "tool_batch") {
          const observations = await runNarratorToolBatch(
            input,
            receipt.directive.requests,
            options.resolveTools,
            options.toolProfile,
            emitEvent,
          );
          receipt = await options.narratorFsm.advanceTurn({
            receipt,
            outcome: { kind: "tool_batch_completed", observations },
          });
          continue;
        }
        if (receipt.directive.kind !== "provider_phase") {
          throw new Error(
            `non-terminal narrator receipt ${receipt.receiptId} has no executable directive`,
          );
        }
        const result = await runNarratorProviderPhase(input, options, receipt);
        if (receipt.directive.outputMode === "final") {
          if (!options.submitEvent) {
            phaseEvents.push(...result.events);
          }
          composeResult = result;
        }
        receipt = await options.narratorFsm.advanceTurn({
          receipt,
          outcome: {
            kind: "provider_phase_completed",
            outputText: textFromEvents(result.events).trim(),
          },
        });
      }

      if (receipt.activity) {
        await emitEvent({
          type: "phase_change",
          phase: receipt.activity.phase,
          message: receipt.activity.message,
        });
      }
      if (!composeResult) {
        throw new Error(
          "roleplay narrator FSM completed without compose phase",
        );
      }

      const sceneBrief = receipt.state.sceneBrief?.trim();
      if (sceneBrief && options.persistDiagnostic) {
        const relevantLore = Array.isArray(receipt.state.relevantLore)
          ? receipt.state.relevantLore
          : [];
        await options.persistDiagnostic({
          wakeId: receipt.wakeId,
          sessionId: receipt.sessionId,
          profileId: receipt.state.profileId,
          sceneBrief,
          relevantLoreRecordIds: relevantLore.map((source) => source.source_id),
        });
      }

      const visibleEvents = options.submitEvent ? [] : phaseEvents;
      return {
        ...composeResult,
        events: visibleEvents,
      };
    },
  };
}

async function runNarratorProviderPhase(
  input: BrainWakeInput,
  options: RoleplayNarratorBrainOptions,
  receipt: RoleplayNarratorTurnReceipt,
): Promise<BrainWakeResult> {
  if (receipt.directive.kind !== "provider_phase") {
    throw new Error(
      `narrator receipt ${receipt.receiptId} is not a provider phase`,
    );
  }
  const phase = receipt.directive.phase;
  const allowedTools = new Set(receipt.directive.allowedTools);
  const brain = createNarratorPhaseBrain(options, {
    phase,
    allowedTools,
    submitEvent:
      receipt.directive.outputMode === "final"
        ? options.submitEvent
        : undefined,
    planActions:
      receipt.directive.outputMode === "final"
        ? options.planActions
        : undefined,
    compactionDomainContext: roleplayCompactionDomainContext({
      sceneId: receipt.sessionId,
      sceneBrief: receipt.state.sceneBrief,
      relevantLore: receipt.state.relevantLore.map((source) => ({
        sourceId: source.source_id,
        title: source.title,
        body: source.body,
      })),
    }),
  });
  return brain.wake(
    withFilteredTools(
      withNarratorInstructions(input, {
        instructions: receipt.directive.instructions,
      }),
      allowedTools,
    ),
  );
}

function createNarratorPhaseBrain(
  options: RoleplayNarratorBrainOptions,
  phase: {
    phase: RoleplayNarratorPhase;
    allowedTools: ReadonlySet<string>;
    submitEvent?: (event: BrainEventEnvelope) => Promise<void>;
    planActions?: BrainActionPlanner;
    compactionDomainContext?: unknown;
  },
): BrainHostExecutor {
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
    compactionDomainContext: phase.compactionDomainContext,
  });
}

async function runNarratorToolBatch(
  input: BrainWakeInput,
  requests: readonly RoleplayNarratorToolRequest[],
  resolver: BrainToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
  emitEvent: (event: BrainEvent) => Promise<void>,
): Promise<RoleplayNarratorToolObservation[]> {
  const tools = resolveMandatoryExploreTools(input, resolver, toolProfile);
  const observations: RoleplayNarratorToolObservation[] = [];
  for (const request of requests) {
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
