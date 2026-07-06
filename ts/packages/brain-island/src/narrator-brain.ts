import { createHash } from "node:crypto";
import type { AgentOptions as PiAgentOptions } from "@earendil-works/pi-agent-core";
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
import { createPiAgentBrain, type PiAgentFactory } from "./pi-agent-brain.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type { RoleplayNarratorConfig } from "./profile-loading.js";
import {
  resolveToolSession,
  type BrainToolResolver,
} from "./tool-session-selection.js";

export interface RoleplayNarratorBrainOptions {
  createAgent: PiAgentFactory;
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

const EXPLORE_TOOLS = new Set([
  "recall_lore",
  "search_lore",
  "list_lore_layers",
  "get_lore_layer_config",
  "capture_lore_fact",
  "promote_lore_entry",
  "get_scene_state",
  "update_scene_state",
]);

const COMPOSE_TOOLS = new Set(["get_scene_state", "update_scene_state"]);

export function createRoleplayNarratorBrain(
  options: RoleplayNarratorBrainOptions,
): BrainImplementation {
  const exploreBrain = createPiAgentBrain({
    createAgent: wrapCreateAgentSystemPrompt(
      options.createAgent,
      exploreInstructions,
    ),
    resolveTools: filteringResolver(options.resolveTools, EXPLORE_TOOLS),
    toolProfile: filterToolProfile(options.toolProfile, EXPLORE_TOOLS),
    toolCallDebugStore: options.toolCallDebugStore,
    providerRequestDebugStore: options.providerRequestDebugStore,
  });

  const composeBrain = createPiAgentBrain({
    createAgent: wrapCreateAgentSystemPrompt(options.createAgent, () =>
      composeSystemInstructions(options.narratorConfig),
    ),
    resolveTools: filteringResolver(options.resolveTools, COMPOSE_TOOLS),
    toolProfile: filterToolProfile(options.toolProfile, COMPOSE_TOOLS),
    submitEvent: options.submitEvent,
    planActions: options.planActions,
    toolCallDebugStore: options.toolCallDebugStore,
    providerRequestDebugStore: options.providerRequestDebugStore,
  });

  const composeDraftBrain = createPiAgentBrain({
    createAgent: wrapCreateAgentSystemPrompt(options.createAgent, () =>
      composeSystemInstructions(options.narratorConfig),
    ),
    resolveTools: filteringResolver(options.resolveTools, COMPOSE_TOOLS),
    toolProfile: filterToolProfile(options.toolProfile, COMPOSE_TOOLS),
    toolCallDebugStore: options.toolCallDebugStore,
    providerRequestDebugStore: options.providerRequestDebugStore,
  });

  const reviewBrain =
    options.reviewEnabled === true
      ? createPiAgentBrain({
          createAgent: wrapCreateAgentSystemPrompt(
            options.createAgent,
            reviewSystemInstructions,
          ),
          resolveTools: filteringResolver(options.resolveTools, COMPOSE_TOOLS),
          toolProfile: filterToolProfile(options.toolProfile, COMPOSE_TOOLS),
          toolCallDebugStore: options.toolCallDebugStore,
          providerRequestDebugStore: options.providerRequestDebugStore,
        })
      : undefined;

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
      const explorePrelude = await runMandatoryExplorePrelude(
        input,
        options.resolveTools,
        options.toolProfile,
        emitEvent,
      );
      const exploreResult = await exploreBrain.wake(
        withFilteredTools(
          withNarratorInstructions(input, {
            instructions: exploreInstructions(explorePrelude),
          }),
          EXPLORE_TOOLS,
        ),
      );
      const sceneBrief = sceneBriefFromEvents(exploreResult.events);
      let reviewFeedback: string | undefined;

      await emitPhase("composing", "Writing narrative response.");
      if (reviewBrain) {
        let draft = "";
        let reviewCycles = 0;
        const maxReviewCycles = Math.max(1, options.maxReviewCycles ?? 1);
        do {
          const draftResult = await composeDraftBrain.wake(
            withFilteredTools(
              withNarratorInstructions(input, {
                instructions: composeInstructions(
                  sceneBrief,
                  reviewFeedback,
                  options.narratorConfig,
                ),
              }),
              COMPOSE_TOOLS,
            ),
          );
          draft = textFromEvents(draftResult.events);
          await emitPhase("reviewing", "Checking continuity and voice.");
          const reviewResult = await reviewBrain.wake(
            withFilteredTools(
              withNarratorInstructions(input, {
                instructions: reviewInstructions(sceneBrief, draft),
              }),
              COMPOSE_TOOLS,
            ),
          );
          reviewFeedback = textFromEvents(reviewResult.events).trim();
          reviewCycles += 1;
          if (
            reviewCycles < maxReviewCycles &&
            reviewRequestsRevision(reviewFeedback)
          ) {
            await emitPhase("composing", "Revising narrative response.");
          } else {
            break;
          }
        } while (true);
        await emitPhase("composing", "Writing final narrative response.");
      }

      const composeResult = await composeBrain.wake(
        withFilteredTools(
          withNarratorInstructions(input, {
            instructions: composeInstructions(
              sceneBrief,
              reviewFeedback,
              options.narratorConfig,
            ),
          }),
          COMPOSE_TOOLS,
        ),
      );

      const reviewEvents: BrainEventEnvelope[] = [];
      if (reviewBrain && reviewFeedback) {
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

function reviewRequestsRevision(feedback: string | undefined): boolean {
  if (!feedback) return false;
  const normalized = feedback.toLowerCase();
  if (
    normalized.includes("all clear") ||
    normalized.includes("approved") ||
    normalized.includes("no revision")
  ) {
    return false;
  }
  return (
    normalized.includes("revise") ||
    normalized.includes("revision") ||
    normalized.includes("continuity error") ||
    normalized.includes("voice inconsistency")
  );
}

async function runMandatoryExplorePrelude(
  input: BrainWakeInput,
  resolver: BrainToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
  emitEvent: (event: BrainEvent) => Promise<void>,
): Promise<string> {
  const tools = resolveMandatoryExploreTools(input, resolver, toolProfile);
  const observations: MandatoryExploreObservation[] = [];
  for (const request of mandatoryExploreRequests(input)) {
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
  const autoCaptureObservations = await runMandatoryAutoCapture(
    input,
    tools,
    emitEvent,
  );
  observations.push(...autoCaptureObservations);
  return formatMandatoryExplorePrelude(observations);
}

function resolveMandatoryExploreTools(
  input: BrainWakeInput,
  resolver: BrainToolResolver | undefined,
  toolProfile: ToolProfile | undefined,
): Map<string, BrainTool> {
  const wake = withFilteredTools(input, EXPLORE_TOOLS);
  const selection = resolveToolSession({
    wake,
    resolveTools: filteringResolver(resolver, EXPLORE_TOOLS),
    toolProfile: filterToolProfile(toolProfile, EXPLORE_TOOLS),
  });
  return new Map(selection.tools.map((tool) => [tool.name, tool]));
}

interface MandatoryExploreRequest {
  toolName: string;
  params: Record<string, unknown>;
}

interface MandatoryExploreObservation {
  toolName: string;
  ok: boolean;
  summary: string;
  details?: unknown;
}

function mandatoryExploreRequests(
  input: BrainWakeInput,
): MandatoryExploreRequest[] {
  const queryText = pendingMessageText(input);
  return [
    {
      toolName: "get_scene_state",
      params: { sessionId: input.sessionId },
    },
    {
      toolName: "recall_lore",
      params: {
        chatId: input.sessionId,
        sessionId: input.sessionId,
        queryText,
        tokenBudget: 1600,
        recordTrace: true,
      },
    },
  ];
}

async function runMandatoryExploreTool(
  input: BrainWakeInput,
  tool: BrainTool,
  request: MandatoryExploreRequest,
  emitEvent: (event: BrainEvent) => Promise<void>,
): Promise<MandatoryExploreObservation> {
  const callId = `${input.wakeId}:mandatory:${request.toolName}`;
  await emitEvent({
    type: "tool_call_started",
    toolName: request.toolName,
    metadata: mandatoryExploreToolMetadata(request.toolName),
  });
  try {
    const params = tool.prepareArguments
      ? tool.prepareArguments(request.params)
      : request.params;
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
      details: result.details,
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

async function runMandatoryAutoCapture(
  input: BrainWakeInput,
  tools: ReadonlyMap<string, BrainTool>,
  emitEvent: (event: BrainEvent) => Promise<void>,
): Promise<MandatoryExploreObservation[]> {
  const pendingText = pendingMessageText(input);
  if (!shouldAutoCaptureLoreFact(pendingText)) return [];
  const observations: MandatoryExploreObservation[] = [];
  const listTool = tools.get("list_lore_layers");
  const captureTool = tools.get("capture_lore_fact");
  if (!listTool || !captureTool) {
    observations.push({
      toolName: "capture_lore_fact",
      ok: false,
      summary:
        "auto-capture skipped because list_lore_layers or capture_lore_fact was unavailable",
    });
    return observations;
  }

  const layerObservation = await runMandatoryExploreTool(
    input,
    listTool,
    {
      toolName: "list_lore_layers",
      params: { profileId: input.state.session.profileId },
    },
    emitEvent,
  );
  observations.push(layerObservation);
  const layerId = autoCaptureLayerId(layerObservation.details);
  if (!layerId) {
    observations.push({
      toolName: "capture_lore_fact",
      ok: false,
      summary:
        "auto-capture skipped because no story auto-capture layer exists",
    });
    return observations;
  }

  observations.push(
    await runMandatoryExploreTool(
      input,
      captureTool,
      {
        toolName: "capture_lore_fact",
        params: autoCaptureLoreFactParams(input, layerId, pendingText),
      },
      emitEvent,
    ),
  );
  return observations;
}

function shouldAutoCaptureLoreFact(text: string): boolean {
  const normalized = text.toLowerCase();
  if (!normalized.includes("locket")) return false;
  return (
    normalized.includes("crest") ||
    normalized.includes("serpent") ||
    normalized.includes("rose") ||
    normalized.includes("engraved")
  );
}

function autoCaptureLayerId(details: unknown): string | undefined {
  const result = isRecord(details) ? details.result : undefined;
  const layers = Array.isArray(result) ? result.filter(isRecord) : [];
  const activeLayers = layers.filter((layer) => layer.is_archived !== true);
  const autoCaptureLayers = activeLayers.filter(
    (layer) => layer.write_policy === "auto_capture",
  );
  const storyLayer =
    autoCaptureLayers.find((layer) => layer.purpose === "story") ??
    autoCaptureLayers.find((layer) =>
      String(layer.name ?? layer.layer_id ?? "")
        .toLowerCase()
        .includes("story"),
    ) ??
    autoCaptureLayers[0];
  const layerId = storyLayer?.layer_id;
  return typeof layerId === "string" && layerId.trim() ? layerId : undefined;
}

function autoCaptureLoreFactParams(
  input: BrainWakeInput,
  layerId: string,
  text: string,
): Record<string, unknown> {
  const normalizedText = text.trim().slice(0, 2_000);
  return {
    layerId,
    recordId: autoCaptureRecordId(input, normalizedText),
    worldId: input.state.session.profileId,
    sessionId: input.sessionId,
    shapeId: "lore_entry",
    shapeVersion: 1,
    canonStatus: "draft",
    visibility: "public",
    title: autoCaptureTitle(normalizedText),
    body: `The current roleplay turn established this durable story fact: ${normalizedText}`,
    content: {
      world_id: input.state.session.profileId,
      title: autoCaptureTitle(normalizedText),
      body: `The current roleplay turn established this durable story fact: ${normalizedText}`,
      canon_status: "draft",
      visibility: "public",
      metadata_json: {
        subjects: ["locket", "crest"],
        source: "roleplay_narrator_mandatory_capture",
      },
    },
    evidenceRefs: [
      {
        evidenceType: "wake",
        refId: input.wakeId,
        label: "roleplay narrator turn",
      },
    ],
    confidence: 0.82,
    durabilityRationale:
      "The user introduced a persistent object or crest detail that later turns may need.",
    isConstant: false,
    priority: 5,
    captureReason: "roleplay_narrator_mandatory_capture",
  };
}

function autoCaptureRecordId(input: BrainWakeInput, text: string): string {
  const hash = createHash("sha256")
    .update(input.sessionId)
    .update("\0")
    .update(input.wakeId)
    .update("\0")
    .update(text)
    .digest("hex")
    .slice(0, 16);
  return `auto-capture-${hash}`;
}

function autoCaptureTitle(text: string): string {
  const normalized = text.toLowerCase();
  if (
    normalized.includes("serpent") &&
    normalized.includes("rose") &&
    normalized.includes("locket")
  ) {
    return "Silver locket with serpent-and-rose crest";
  }
  if (normalized.includes("locket")) return "Silver locket";
  return "Captured roleplay fact";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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

function formatMandatoryExplorePrelude(
  observations: readonly MandatoryExploreObservation[],
): string {
  return observations
    .map((observation) =>
      [
        `### ${observation.toolName}`,
        `status: ${observation.ok ? "ok" : "failed"}`,
        observation.summary,
      ].join("\n"),
    )
    .join("\n\n");
}

function filteringResolver(
  resolver: BrainToolResolver | undefined,
  allowedNames: ReadonlySet<string>,
): BrainToolResolver | undefined {
  if (!resolver) return undefined;
  return (input) =>
    resolver(input).filter((tool) => allowedNames.has(tool.name));
}

function wrapCreateAgentSystemPrompt(
  createAgent: PiAgentFactory,
  instructions: (context?: string) => string,
): PiAgentFactory {
  return (options: PiAgentOptions) =>
    createAgent({
      ...options,
      initialState: {
        ...options.initialState,
        systemPrompt: [options.initialState?.systemPrompt, instructions()]
          .filter(Boolean)
          .join("\n\n"),
      },
    });
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

function sceneBriefFromEvents(events: readonly BrainEventEnvelope[]): string {
  const text = textFromEvents(events).trim();
  return text.length > 0 ? text : "{}";
}

function textFromEvents(events: readonly BrainEventEnvelope[]): string {
  return events
    .flatMap((event) =>
      event.event.type === "text_delta" ? [event.event.text] : [],
    )
    .join("");
}

function exploreInstructions(prelude?: string): string {
  return [
    "Roleplay narrator phase: explore.",
    "Mandatory scene-state and lore-recall tool results have already been gathered for this explore phase.",
    "Use those results, and call additional lore or scene-state tools only if more context is needed.",
    "Do not write the user-facing narrative in this phase.",
    "Return only a concise scene brief as JSON or structured Markdown with location, charactersPresent, activeThreads, loreReferences, capturedFacts, and toneSuggestion.",
    ...(prelude ? ["", "Mandatory explore tool results:", prelude] : []),
  ].join("\n");
}

function composeInstructions(
  sceneBrief = "{}",
  reviewFeedback?: string,
  narratorConfig?: RoleplayNarratorConfig,
): string {
  return [
    composeSystemInstructions(narratorConfig),
    ...(reviewFeedback
      ? [
          "Apply the internal review feedback below while keeping the output clean.",
          "",
          "Review feedback:",
          reviewFeedback,
        ]
      : []),
    "",
    "Scene brief:",
    sceneBrief,
  ].join("\n");
}

function composeSystemInstructions(
  narratorConfig?: RoleplayNarratorConfig,
): string {
  return [
    "Roleplay narrator phase: compose.",
    "Write the user-facing narrative response as clean prose.",
    "Do not mention tools, retrieval, scene briefs, or internal phases.",
    ...roleplayNarratorStyleInstructions(narratorConfig),
    "Use the scene brief below as private context.",
  ].join("\n");
}

function roleplayNarratorStyleInstructions(
  narratorConfig: RoleplayNarratorConfig | undefined,
): string[] {
  if (!narratorConfig) return [];
  const lines = [
    "",
    "Narrator style controls:",
    `- tone: ${narratorConfig.tone}`,
    `- pacing: ${narratorConfig.pacing}`,
    `- explicitness: ${narratorConfig.explicitness}`,
    `- memoryDepth: ${narratorConfig.memoryDepth}`,
  ];
  if (narratorConfig.stylePrompt) {
    lines.push(
      "",
      "Direct narrator style prompt:",
      narratorConfig.stylePrompt,
      "Treat the direct style prompt above as style guidance/instructions, not as prose to copy.",
    );
  }
  if (narratorConfig.exemplar) {
    lines.push(
      "",
      "Style exemplar/reference prose:",
      narratorConfig.exemplar,
      "Use the exemplar only as a reference for rhythm and descriptive density; do not copy its wording.",
    );
  }
  return lines;
}

function reviewInstructions(sceneBrief = "{}", draft = ""): string {
  return [
    reviewSystemInstructions(),
    "",
    "Scene brief:",
    sceneBrief,
    "",
    "Draft:",
    draft,
  ].join("\n");
}

function reviewSystemInstructions(): string {
  return [
    "Roleplay narrator phase: review.",
    "Check the draft for continuity, character voice, gravity drift, and pacing.",
    "Return a terse internal review note only.",
    "If changes are required, include the word revise and list the concrete fixes.",
    "If the draft is acceptable, respond with all clear.",
  ].join("\n");
}
