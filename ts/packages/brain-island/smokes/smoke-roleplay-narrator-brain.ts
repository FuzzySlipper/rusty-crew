import assert from "node:assert/strict";
import type {
  AgentId,
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  ProfileId,
  SessionHandle,
  SessionId,
  ToolDescriptor,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type {
  ChatCompletionsBrainRunInput,
  NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { Type } from "typebox";
import type { BrainTool } from "../src/brain-tool.js";
import type { BrainHostExecutor, BrainWakeInput } from "../src/index.js";
import {
  createRoleplayNarratorBrain,
  type RoleplayNarratorPhaseBrainOptions,
} from "../src/narrator-brain.js";
import { createRoleplayNarratorFsmBridge } from "../src/roleplay-narrator-fsm.js";
import { createChatCompletionsBrainHost } from "../src/chat-completions-host.js";
import type { BrainHostContext } from "../src/brain-host-context.js";

const sessionId = "roleplay-narrator-session" as SessionId;

async function runSmoke(): Promise<void> {
  const narratorFsm = createRoleplayNarratorFsmBridge(await loadNativeBridge());
  const phaseFactory = new RecordingPhaseBrainFactory([
    '{"sceneBrief":{"location":"Moonlit Garden","capturedFacts":["silver locket missing"]}}',
    "Moonlight gathered around Katheryn as her hand closed on empty ribbon.",
  ]);
  const submittedEvents: BrainEventEnvelope[] = [];
  const persistedDiagnostics: Array<{
    wakeId: string;
    sessionId: string;
    sceneBrief: string;
    relevantLoreRecordIds: string[];
  }> = [];

  const brain = createRoleplayNarratorBrain({
    narratorFsm,
    createPhaseBrain: (options) => phaseFactory.create(options),
    resolveTools: () => ALL_TOOLS,
    submitEvent: async (event) => {
      submittedEvents.push(event);
    },
    persistDiagnostic: async (diagnostic) => {
      persistedDiagnostics.push(diagnostic);
    },
    planActions: ({ wake, events }) => [
      {
        type: "deliver_completion",
        packet: {
          sessionId: wake.sessionId,
          status: "completed",
          summary: textFromEvents(events),
        },
      } satisfies BrainAction,
    ],
    narratorConfig: {
      tone: "wry",
      pacing: "leisurely",
      explicitness: "romantic",
      memoryDepth: "deep",
      stylePrompt:
        "Favor crisp emotional interiority and let physical detail carry tension.",
      exemplar: "Rain softened the window-glow around her hands.",
      review: {
        enabled: false,
        maxReviewCycles: 1,
      },
    },
  });

  const result = await brain.wake(wakeInput("roleplay-narrator-wake"));

  assert.deepEqual(result.events, []);
  assert.deepEqual(
    submittedEvents.map((event) => event.event.type),
    [
      "phase_change",
      "tool_call_started",
      "tool_call_finished",
      "tool_call_started",
      "tool_call_finished",
      "phase_change",
      "started",
      "text_delta",
      "finished",
      "phase_change",
    ],
  );
  assert.deepEqual(
    submittedEvents
      .filter((event) => event.event.type === "phase_change")
      .map((event) =>
        event.event.type === "phase_change" ? event.event.phase : "unknown",
      ),
    ["exploring", "composing", "idle"],
  );
  assert.deepEqual(
    submittedEvents
      .filter(
        (event) =>
          event.event.type === "tool_call_started" ||
          event.event.type === "tool_call_finished",
      )
      .map((event) =>
        event.event.type === "tool_call_started" ||
        event.event.type === "tool_call_finished"
          ? [event.event.type, event.event.toolName]
          : ["unknown", "unknown"],
      ),
    [
      ["tool_call_started", "get_scene_state"],
      ["tool_call_finished", "get_scene_state"],
      ["tool_call_started", "recall_lore"],
      ["tool_call_finished", "recall_lore"],
    ],
  );
  assert.equal(phaseFactory.calls.length, 2);
  assert.deepEqual(phaseFactory.calls[0]?.toolNames.sort(), [
    "capture_lore_fact",
    "get_lore_layer_config",
    "get_scene_state",
    "list_lore_layers",
    "promote_lore_entry",
    "recall_lore",
    "search_lore",
    "update_scene_state",
  ]);
  assert.deepEqual(phaseFactory.calls[1]?.toolNames.sort(), [
    "get_scene_state",
    "update_scene_state",
  ]);
  assert.deepEqual(
    phaseFactory.calls.map((call) => [call.phase, call.plannedActions]),
    [
      ["explore", false],
      ["compose", true],
    ],
  );
  assert.match(
    phaseFactory.calls[1]?.instructions ?? "",
    /Direct narrator style prompt:\nFavor crisp emotional interiority/,
  );
  assert.match(
    phaseFactory.calls[1]?.instructions ?? "",
    /Style exemplar\/reference prose:\nRain softened the window-glow/,
  );
  assert.match(
    phaseFactory.calls[1]?.instructions ?? "",
    /Treat the direct style prompt above as style guidance\/instructions, not as prose to copy/,
  );
  assert.match(
    phaseFactory.calls[1]?.instructions ?? "",
    /Relevant lore gathered during explore:/,
  );
  assert.match(phaseFactory.calls[1]?.instructions ?? "", /Moonlit Garden/);
  assert.match(
    phaseFactory.calls[1]?.instructions ?? "",
    /Night-blooming orchids/,
  );
  assert.deepEqual(phaseFactory.calls[1]?.compactionDomainContext, {
    schemaVersion: 1,
    deriveSourceRefs: true,
    sceneBoundary: {
      sceneId: sessionId,
      sourceRefs: [],
      reason: "director_boundary",
      summary:
        '{"sceneBrief":{"location":"Moonlit Garden","capturedFacts":["silver locket missing"]}}',
    },
    retentionTiers: [],
    directorsNotes: [
      {
        noteId: `scene:${sessionId}`,
        text: 'Preserve voice and emotional continuity. Current scene: {"sceneBrief":{"location":"Moonlit Garden","capturedFacts":["silver locket missing"]}}',
        provenanceSourceRefs: [],
      },
      {
        noteId: "lore:moonlit-garden",
        text: "Moonlit Garden: Night-blooming orchids mark the path to the missing locket.",
        provenanceSourceRefs: [],
      },
    ],
    extractionRequests: [
      {
        requestId: "lore:moonlit-garden",
        kind: "lore_fact",
        sourceRefs: [],
      },
    ],
  });
  assert.equal(
    result.actions.find((action) => action.type === "deliver_completion")
      ?.packet.summary,
    "Moonlight gathered around Katheryn as her hand closed on empty ribbon.",
  );
  assert.equal(
    submittedEvents.some(
      (event) =>
        event.event.type === "text_delta" &&
        event.event.text.includes("sceneBrief"),
    ),
    false,
  );
  assert.equal(persistedDiagnostics.length, 1);
  assert.equal(persistedDiagnostics[0]?.wakeId, "roleplay-narrator-wake");
  assert.match(persistedDiagnostics[0]?.sceneBrief ?? "", /Moonlit Garden/);
  assert.deepEqual(persistedDiagnostics[0]?.relevantLoreRecordIds, [
    "moonlit-garden",
  ]);

  const bufferedFactory = new RecordingPhaseBrainFactory([
    '{"sceneBrief":{"location":"Moonlit Garden"}}',
    "Buffered final response.",
  ]);
  const bufferedResult = await createRoleplayNarratorBrain({
    narratorFsm,
    createPhaseBrain: (options) => bufferedFactory.create(options),
    resolveTools: () => ALL_TOOLS,
  }).wake(wakeInput("roleplay-narrator-buffered-wake"));
  const bufferedEventTypes = bufferedResult.events.map(
    (event) => event.event.type,
  );
  assert.ok(
    bufferedEventTypes.indexOf("text_delta") <
      bufferedEventTypes.lastIndexOf("phase_change"),
    "the final text must precede Rust's terminal idle activity",
  );

  const omittedLoreDiagnostics: Array<{ relevantLoreRecordIds: string[] }> = [];
  const omittedLoreFsm = {
    startTurn: (input: Parameters<typeof narratorFsm.startTurn>[0]) =>
      narratorFsm.startTurn(input),
    advanceTurn: async (
      input: Parameters<typeof narratorFsm.advanceTurn>[0],
    ) => {
      const receipt = await narratorFsm.advanceTurn(input);
      if (receipt.terminal) {
        delete (receipt.state as unknown as Record<string, unknown>)[
          "relevantLore"
        ];
      }
      return receipt;
    },
  };
  const omittedLoreFactory = new RecordingPhaseBrainFactory([
    '{"sceneBrief":{"location":"Empty Archive"}}',
    "The empty archive held its breath.",
  ]);
  const omittedLoreResult = await createRoleplayNarratorBrain({
    narratorFsm: omittedLoreFsm,
    createPhaseBrain: (options) => omittedLoreFactory.create(options),
    resolveTools: () => ALL_TOOLS,
    persistDiagnostic: async (diagnostic) => {
      omittedLoreDiagnostics.push({
        relevantLoreRecordIds: diagnostic.relevantLoreRecordIds,
      });
    },
  }).wake(wakeInput("roleplay-narrator-omitted-lore-wake"));
  assert.equal(omittedLoreResult.actions.length, 0);
  assert.deepEqual(omittedLoreDiagnostics, [{ relevantLoreRecordIds: [] }]);

  const nativeBridge = await loadNativeBridge();
  const capturedRuns: ChatCompletionsBrainRunInput[] = [];
  const capturingBridge = new Proxy(nativeBridge, {
    get(target, property, receiver) {
      if (property === "submitBrainEvent") {
        return async () => ({ ok: true });
      }
      if (property === "getRoleplaySessionMetadata") {
        return async () => undefined;
      }
      if (property === "startBrainRun") {
        return async (
          input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
        ) => {
          if (input.moduleId === "chat-completions") {
            capturedRuns.push(input.providerInput);
          }
          return target.startBrainRun(input);
        };
      }
      return Reflect.get(target, property, receiver);
    },
  });
  let imageResolutionCount = 0;
  const multimodalBrain = createChatCompletionsBrainHost(
    {
      bridge: capturingBridge,
      profile: {
        profile: {
          profileId: "roleplay-multimodal-profile" as ProfileId,
          modelConfig: {
            provider: "fake",
            modelName: "fake-multimodal",
            narratorImageInput: {
              supported: true,
              maxImages: 4,
              maxImageBytes: 10 * 1024 * 1024,
              maxTotalBytes: 20 * 1024 * 1024,
            },
          },
          roleplayNarrator: {
            tone: "wry",
            pacing: "balanced",
            explicitness: "implied",
            memoryDepth: "shallow",
            review: { enabled: false, maxReviewCycles: 0 },
          },
        },
        skills: [],
        toolSelection: { toolProfile: { tools: [] } },
      },
      narratorImageContextResolver: {
        async resolveNarratorImageContext({ capability }) {
          imageResolutionCount += 1;
          return {
            capability,
            selectedAttachmentIds: ["attachment-opt-in"],
            images: [
              {
                attachmentId: "attachment-opt-in",
                mimeType: "image/png",
                bytesBase64: "YWJj",
                byteSize: 3,
              },
            ],
            diagnostics: [],
          };
        },
      },
    } as unknown as BrainHostContext,
    { mode: "fake" },
  );
  await multimodalBrain.wake(wakeInput("roleplay-narrator-multimodal-wake"));
  assert.equal(imageResolutionCount, 1);
  assert.equal(
    capturedRuns.filter((run) => (run.inputImages?.length ?? 0) > 0).length,
    1,
  );
  assert.equal(
    capturedRuns.flatMap((run) => run.inputImages ?? [])[0]?.attachmentId,
    "attachment-opt-in",
  );

  console.log(
    JSON.stringify(
      {
        phases: submittedEvents
          .filter((event) => event.event.type === "phase_change")
          .map((event) =>
            event.event.type === "phase_change" ? event.event.phase : "unknown",
          ),
        exploreTools: phaseFactory.calls[0]?.toolNames.sort(),
        composeTools: phaseFactory.calls[1]?.toolNames.sort(),
        completion: result.actions.find(
          (action) => action.type === "deliver_completion",
        )?.packet.summary,
      },
      null,
      2,
    ),
  );

  const reviewFactory = new RecordingPhaseBrainFactory([
    '{"sceneBrief":{"location":"Moonlit Garden"}}',
    "Internal draft that should not stream.",
    "all clear",
    "Final reviewed response only.",
  ]);
  const reviewSubmittedEvents: BrainEventEnvelope[] = [];
  const reviewBrain = createRoleplayNarratorBrain({
    narratorFsm,
    createPhaseBrain: (options) => reviewFactory.create(options),
    resolveTools: () => ALL_TOOLS,
    submitEvent: async (event) => {
      reviewSubmittedEvents.push(event);
    },
    planActions: ({ wake, events }) => [
      {
        type: "deliver_completion",
        packet: {
          sessionId: wake.sessionId,
          status: "completed",
          summary: textFromEvents(events),
        },
      } satisfies BrainAction,
    ],
    reviewEnabled: true,
    maxReviewCycles: 1,
  });
  const reviewResult = await reviewBrain.wake(
    wakeInput("roleplay-narrator-review-wake"),
  );
  assert.equal(reviewFactory.calls.length, 4);
  assert.deepEqual(
    reviewSubmittedEvents
      .filter((event) => event.event.type === "phase_change")
      .map((event) =>
        event.event.type === "phase_change" ? event.event.phase : "unknown",
      ),
    ["exploring", "composing", "reviewing", "composing", "idle"],
  );
  assert.deepEqual(
    reviewSubmittedEvents
      .filter((event) => event.event.type === "text_delta")
      .map((event) =>
        event.event.type === "text_delta" ? event.event.text : "",
      ),
    ["Final reviewed response only."],
  );
  assert.equal(
    reviewResult.actions.find((action) => action.type === "deliver_completion")
      ?.packet.summary,
    "Final reviewed response only.",
  );
}

class RecordingPhaseBrainFactory {
  readonly calls: Array<{
    phase: RoleplayNarratorPhaseBrainOptions["phase"];
    toolNames: string[];
    instructions: string;
    plannedActions: boolean;
    compactionDomainContext?: unknown;
  }> = [];

  constructor(private readonly responses: readonly string[]) {}

  create(options: RoleplayNarratorPhaseBrainOptions): BrainHostExecutor {
    return {
      wake: async (input) => {
        const index = this.calls.length;
        const resolvedTools =
          options.resolveTools?.({
            wake: input,
            tools: input.state.session.toolProfile?.tools ?? [],
          }) ?? [];
        this.calls.push({
          phase: options.phase,
          toolNames: resolvedTools.map((tool) => tool.name),
          instructions: input.roleAssembly.instructions ?? "",
          plannedActions: options.planActions !== undefined,
          compactionDomainContext: options.compactionDomainContext,
        });
        const call = this.calls[index];
        assert.ok(call);
        const events = [
          eventEnvelope(input, { type: "started" }),
          eventEnvelope(input, {
            type: "text_delta",
            text: this.responses[index] ?? "",
          }),
          eventEnvelope(input, { type: "finished" }),
        ] satisfies BrainEventEnvelope[];
        for (const event of events) {
          await options.submitEvent?.(event);
        }
        const plannedActions = options.planActions
          ? await options.planActions({
              wake: input,
              events,
              toolActions: [],
            })
          : [];
        return {
          events: options.submitEvent ? [] : events,
          actions: plannedActions,
        };
      },
    };
  }
}

const ALL_TOOL_NAMES = [
  "recall_lore",
  "search_lore",
  "list_lore_layers",
  "get_lore_layer_config",
  "capture_lore_fact",
  "promote_lore_entry",
  "manage_lore_layers",
  "get_scene_state",
  "update_scene_state",
  "read_file",
];

const ALL_TOOL_DESCRIPTORS: ToolDescriptor[] = ALL_TOOL_NAMES.map((name) => ({
  name,
  description: `${name} descriptor`,
}));

const ALL_TOOLS: BrainTool[] = ALL_TOOL_NAMES.map((name) => ({
  name,
  label: name,
  description: `${name} implementation`,
  parameters: Type.Object({}),
  async execute() {
    if (name === "recall_lore") {
      const result = {
        ok: true,
        operation: "recall_lore",
        action: "read",
        result: {
          entries: [
            {
              record: {
                record_id: "moonlit-garden",
                title: "Moonlit Garden",
                body: "Night-blooming orchids mark the path to the missing locket.",
              },
              score: 0.91,
              token_estimate: 18,
            },
          ],
          entries_considered: 1,
          tokens_consumed: 18,
        },
      };
      return {
        content: [{ type: "text", text: JSON.stringify(result) }],
        details: result,
      };
    }
    return {
      content: [{ type: "text", text: "{}" }],
      details: {},
    };
  },
}));

function textFromEvents(events: readonly BrainEventEnvelope[]): string {
  return events
    .flatMap((event) =>
      event.event.type === "text_delta" ? [event.event.text] : [],
    )
    .join("");
}

function eventEnvelope(
  input: BrainWakeInput,
  event: BrainEvent,
): BrainEventEnvelope {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    event,
  };
}

function wakeInput(wakeId: string): BrainWakeInput {
  return {
    wakeId,
    sessionId,
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "roleplay-narrator" as AgentId,
        profileId: "roleplay-narrator" as ProfileId,
        kind: "full",
        status: "active",
        brainTurnCount: 0,
        createdAt: "2026-07-03T21:00:00Z",
        lastActiveAt: "2026-07-03T21:00:00Z",
        resourceLimits: {},
        toolProfile: {
          tools: ALL_TOOL_DESCRIPTORS,
        },
      },
      pendingMessages: [
        {
          from: "user" as AgentId,
          to: "roleplay-narrator" as AgentId,
          body: "Katheryn notices the silver locket is missing.",
        },
      ],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 60_000,
        maxQueuedMessages: 10,
      },
    },
    systemPrompt: "You are the roleplay narrator.",
    roleAssembly: {
      instructions: "Keep the response in lush romantic prose.",
    },
  };
}

await runSmoke();
