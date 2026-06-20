import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  BodyState,
  BrainAction,
  BrainImplementationId,
  CoreEvent,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  buildProfileRoleAssembly,
  counterResetTool,
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  MemorySessionTodoStore,
  registerBrainImplementationRuntime,
  renderPlanningContext,
  renderSessionTodoContext,
  selectToolProfile,
  sessionSearchTool,
  todoTool,
} from "./index.js";
import type { LoadedProfileContext } from "./index.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-planning-runtime-engine-"),
);

const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T10:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

class PlanningRuntimeFakeAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(
    private readonly options: PiAgentOptions,
    private readonly outputs: Record<string, string[]>,
  ) {}

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    await this.emit({ type: "agent_start" } as PiAgentEvent);
    assert.match(
      this.options.initialState?.systemPrompt ?? "",
      /Planning Context/,
    );

    await this.callTool("todo", {
      action: "replace",
      items: [
        {
          id: "search-proof",
          title: "Search runtime history",
          status: "in_progress",
        },
      ],
    });
    await this.callTool("session_search", {
      query: "Phoenix",
      rowType: "message",
      agentId: "planning-agent",
      limit: 5,
    });
    await this.callTool("counter_reset", {
      action: "summary",
      scopeType: "runtime",
    });
    await this.callTool("counter_reset", {
      action: "reset",
      scopeType: "runtime",
      counterName: "messages",
      triggerType: "manual",
      reason: "planning runtime wake proof",
      confirm: true,
    });
    await this.emit({ type: "agent_end", messages: [] } as PiAgentEvent);
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}

  private async callTool(name: string, params: Record<string, unknown>) {
    const tool = this.options.initialState?.tools?.find(
      (candidate) => candidate.name === name,
    );
    assert.ok(tool, `${name} should be selected`);
    await this.emit({
      type: "tool_execution_start",
      toolName: name,
    } as PiAgentEvent);
    try {
      const result = await (tool as AgentTool).execute(`${name}-call`, params);
      this.outputs[name] = [
        ...(this.outputs[name] ?? []),
        ...result.content.flatMap((content) =>
          content.type === "text" && typeof content.text === "string"
            ? [content.text]
            : [],
        ),
      ];
      await this.emit({
        type: "tool_execution_end",
        toolName: name,
        isError: false,
      } as PiAgentEvent);
    } catch (error) {
      await this.emit({
        type: "tool_execution_end",
        toolName: name,
        isError: true,
      } as PiAgentEvent);
      throw error;
    }
  }

  private async emit(event: PiAgentEvent): Promise<void> {
    this.listener?.(event, abortSignal);
  }
}

try {
  const sessionId = "planning-runtime-session" as SessionId;
  const agentId = "planning-agent" as AgentId;
  const profileId = "planning-profile" as ProfileId;
  const peerSessionId = "planning-peer-session" as SessionId;
  const peerAgentId = "planning-peer" as AgentId;
  const wakeId = "planning-runtime-wake";
  const selection = selectToolProfile({
    profileId,
    policy: {
      requestedToolsets: [
        "planning_session",
        "runtime_search",
        "runtime_counters",
      ],
    },
  });
  const profileContext: LoadedProfileContext = {
    profile: {
      profileId,
      displayName: "Planning Profile",
      modelConfig: { provider: "local", modelName: "deterministic" },
      toolPolicy: {
        requestedToolsets: [
          "planning_session",
          "runtime_search",
          "runtime_counters",
        ],
      },
      prompt: {
        system: "Use selected planning/runtime tools.",
        instructions: ["Keep todos session-local."],
      },
    },
    skills: [],
    toolSelection: selection,
  };
  const todoStore = new MemorySessionTodoStore();
  const assembled = buildProfileRoleAssembly(profileContext, {
    planningContext: renderPlanningContext({
      sessionSearchGuidance:
        "Search only Rust-owned runtime session and message history.",
      counterGuidance: "Counters are derived state and reset must be explicit.",
    }),
  });
  const toolOutputs: Record<string, string[]> = {};
  const brainEvents = await native.subscribeEvents({
    eventKinds: ["brain_event_observed"],
    sessionId,
  });

  await native.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
  });
  await native.createSession({
    sessionId: peerSessionId,
    agentId: peerAgentId,
    profileId: "peer-profile",
    kind: "worker",
  });
  await native.routeAgentMessage(
    peerAgentId,
    agentId,
    "Phoenix runtime handoff needs bounded search proof.",
    "planning-phoenix-1",
  );
  await native.routeAgentMessage(
    peerAgentId,
    agentId,
    "Phoenix counter reset should not delete history.",
    "planning-phoenix-2",
  );

  const brain = await registerBrainImplementationRuntime(
    native,
    {
      implementationId: "planning-runtime-proof" as BrainImplementationId,
      profileId,
      toolProfile: selection.toolProfile,
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createPiAgentBrain({
      createAgent: (options) =>
        new PlanningRuntimeFakeAgent(options, toolOutputs),
      resolveTools: ({ wake }) => [
        todoTool({ store: todoStore, sessionId: wake.sessionId }),
        sessionSearchTool({ client: native, maxBodyChars: 26 }),
        counterResetTool({ client: native, allowReset: true }),
      ],
      planActions: ({ wake }): BrainAction[] => [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "planning runtime wake completed",
          },
        },
      ],
    }),
  );

  const request = await native.buildBrainWakeRequest({
    brain,
    sessionId,
    bodyStateJson: encoder.encode(JSON.stringify(bodyState())),
    systemPrompt: assembled.systemPrompt,
    roleAssemblyJson: encoder.encode(JSON.stringify(assembled.roleAssembly)),
    wakeId,
  });

  const accepted = await native.wakeBrain(request);
  assert.deepEqual(accepted, { wakeId, accepted: true });
  assert.match(toolOutputs.todo?.[0] ?? "", /search-proof/);
  assert.match(
    toolOutputs.session_search?.[0] ?? "",
    /Phoenix runtime handoff/,
  );
  assert.match(toolOutputs.counter_reset?.[0] ?? "", /"messages": 2/);
  assert.match(toolOutputs.counter_reset?.[1] ?? "", /"resetRows": 1/);
  assert.equal(await native.countRows("completion_packets"), 1);
  assert.equal(await native.countRows("tool_call_history"), 8);

  const resetQuery = await native.queryRuntimeCounters({
    scopeType: "runtime",
    counterName: "messages",
  });
  assert.equal(resetQuery[0]?.value, 0);

  const subsequentAssembly = buildProfileRoleAssembly(profileContext, {
    planningContext: renderPlanningContext({
      todoContext: renderSessionTodoContext(todoStore.read(sessionId)),
      sessionSearchGuidance:
        "Search only Rust-owned runtime session and message history.",
      counterGuidance: "Counters are derived state and reset must be explicit.",
    }),
  });
  assert.match(
    subsequentAssembly.roleAssembly.instructions ?? "",
    /Session-local planning notes only/,
  );
  assert.match(
    subsequentAssembly.roleAssembly.instructions ?? "",
    /search-proof/,
  );

  const observedEvents = await native.drainSubscriptionEvents(brainEvents, 20);
  const toolEvents = observedEvents.filter(
    (event): event is Extract<CoreEvent, { type: "brain_event_observed" }> =>
      event.type === "brain_event_observed" &&
      event.event.type.startsWith("tool_call_"),
  );
  assert.equal(toolEvents.length, 8);
  await native.unsubscribeEvents(brainEvents);

  console.log(
    JSON.stringify(
      {
        wakeId,
        selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
        searchMatched: /Phoenix/.test(toolOutputs.session_search?.[0] ?? ""),
        todoItems: todoStore.read(sessionId).items.length,
        runtimeMessagesAfterReset: resetQuery[0]?.value,
        toolCallHistory: await native.countRows("tool_call_history"),
        toolEvents: toolEvents.map((event) => event.event.type),
      },
      null,
      2,
    ),
  );

  function bodyState(): BodyState {
    return {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId,
        profileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: selection.toolProfile,
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-20T10:00:00Z",
        lastActiveAt: "2026-06-20T10:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    };
  }
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}
