import assert from "node:assert/strict";
import type {
  Agent as PiAgent,
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  BrainAction,
  BrainEventEnvelope,
  ProfileId,
  SessionHandle,
  SessionId,
  ToolDescriptor,
} from "@rusty-crew/contracts";
import { Type } from "typebox";
import type { BrainTool } from "./brain-tool.js";
import type { BrainWakeInput } from "./index.js";
import { createRoleplayNarratorBrain } from "./narrator-brain.js";

const sessionId = "roleplay-narrator-session" as SessionId;

async function runSmoke(): Promise<void> {
  const agentFactory = new RecordingAgentFactory([
    '{"sceneBrief":{"location":"Moonlit Garden","capturedFacts":["silver locket missing"]}}',
    "Moonlight gathered around Katheryn as her hand closed on empty ribbon.",
  ]);
  const submittedEvents: BrainEventEnvelope[] = [];

  const brain = createRoleplayNarratorBrain({
    createAgent: (options) => agentFactory.create(options),
    resolveTools: () => ALL_TOOLS,
    submitEvent: async (event) => {
      submittedEvents.push(event);
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
        checkGravityDrift: true,
        checkCharacterVoice: true,
        checkContinuity: true,
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
  assert.equal(agentFactory.calls.length, 2);
  assert.deepEqual(agentFactory.calls[0]?.toolNames.sort(), [
    "capture_lore_fact",
    "get_lore_layer_config",
    "get_scene_state",
    "list_lore_layers",
    "promote_lore_entry",
    "recall_lore",
    "search_lore",
    "update_scene_state",
  ]);
  assert.deepEqual(agentFactory.calls[1]?.toolNames.sort(), [
    "get_scene_state",
    "update_scene_state",
  ]);
  assert.match(
    agentFactory.calls[1]?.systemPrompt ?? "",
    /Direct narrator style prompt:\nFavor crisp emotional interiority/,
  );
  assert.match(
    agentFactory.calls[1]?.systemPrompt ?? "",
    /Style exemplar\/reference prose:\nRain softened the window-glow/,
  );
  assert.match(
    agentFactory.calls[1]?.systemPrompt ?? "",
    /Treat the direct style prompt above as style guidance\/instructions, not as prose to copy/,
  );
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

  console.log(
    JSON.stringify(
      {
        phases: submittedEvents
          .filter((event) => event.event.type === "phase_change")
          .map((event) =>
            event.event.type === "phase_change" ? event.event.phase : "unknown",
          ),
        exploreTools: agentFactory.calls[0]?.toolNames.sort(),
        composeTools: agentFactory.calls[1]?.toolNames.sort(),
        completion: result.actions.find(
          (action) => action.type === "deliver_completion",
        )?.packet.summary,
      },
      null,
      2,
    ),
  );

  const reviewFactory = new RecordingAgentFactory([
    '{"sceneBrief":{"location":"Moonlit Garden"}}',
    "Internal draft that should not stream.",
    "all clear",
    "Final reviewed response only.",
  ]);
  const reviewSubmittedEvents: BrainEventEnvelope[] = [];
  const reviewBrain = createRoleplayNarratorBrain({
    createAgent: (options) => reviewFactory.create(options),
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

class RecordingAgentFactory {
  readonly calls: Array<{ toolNames: string[]; systemPrompt: string }> = [];

  constructor(private readonly responses: readonly string[]) {}

  create(
    options: PiAgentOptions,
  ): Pick<PiAgent, "prompt" | "subscribe" | "waitForIdle"> &
    Partial<Pick<PiAgent, "clearAllQueues">> {
    const index = this.calls.length;
    this.calls.push({
      toolNames: (
        (options.initialState?.tools ?? []) as Array<{ name: string }>
      ).map((tool) => tool.name),
      systemPrompt: options.initialState?.systemPrompt ?? "",
    });
    return new FinalMessageAgent(this.responses[index] ?? "");
  }
}

class FinalMessageAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(private readonly text: string) {}

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: PiAgentMessage[] | PiAgentMessage | string,
  ): Promise<void> {
    const signal = new AbortController().signal;
    this.listener?.({ type: "agent_start" } as PiAgentEvent, signal);
    this.listener?.(
      {
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", delta: this.text },
      } as PiAgentEvent,
      signal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      signal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
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
