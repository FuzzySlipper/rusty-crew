import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool,
} from "@earendil-works/pi-agent-core";
import type {
  DenMemoryClient,
  DenMemoryRecallRequest,
} from "@rusty-crew/adapter-den";
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
  buildToolContextDiagnosticsReport,
  buildToolRegistryDiagnostics,
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  denseProfileMemoryTool,
  loadProfileContext,
  registerBrainImplementationRuntime,
  renderDenMemoryContext,
  renderDenseProfileMemoryContext,
  resolveDenMemoryTools,
  resolveSkillsTools,
} from "./index.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-memory-skills-engine-"),
);
const profileRoot = mkdtempSync(
  join(tmpdir(), "rusty-crew-memory-skills-profile-"),
);
const profilesDir = join(profileRoot, "profiles");
const skillsDir = join(profileRoot, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T09:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

class MemorySkillsFakeAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(
    private readonly options: PiAgentOptions,
    private readonly outputs: Record<string, string>,
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
      /Dense Profile Memory/,
    );
    assert.match(this.options.initialState?.systemPrompt ?? "", /Memory Skill/);

    await this.callTool("den_memory_recall", {
      prompt: "What memory guidance is relevant?",
    });
    await this.callTool("dense_profile_memory", {
      action: "list",
      targetType: "profile",
    });
    await this.callTool("skills_list", {});
    await this.callTool("skill_view", {
      slug: "memory-skill",
      includeBody: false,
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
      this.outputs[name] = result.content
        .flatMap((content) =>
          content.type === "text" && typeof content.text === "string"
            ? [content.text]
            : [],
        )
        .join("");
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
  const sessionId = "memory-skills-session" as SessionId;
  const agentId = "memory-skills-agent" as AgentId;
  const profileId = "memory-skills-profile" as ProfileId;
  const wakeId = "memory-skills-wake";
  const denCalls: DenMemoryRecallRequest[] = [];
  const denClient: DenMemoryClient = {
    async recall(request) {
      denCalls.push(request);
      return {
        memories: [
          {
            id: "den-memory-1",
            summary: "Den memory belongs to Den.",
            score: 0.9,
          },
        ],
        total: 1,
      };
    },
    async read() {
      throw new Error("read not used in smoke");
    },
    async search() {
      throw new Error("search not used in smoke");
    },
    async store() {
      throw new Error("store not used in smoke");
    },
    async propose() {
      throw new Error("propose not used in smoke");
    },
  };

  writeFileSync(
    join(profilesDir, `${profileId}.json`),
    JSON.stringify(
      {
        profileId,
        displayName: "Memory Skills Profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        toolPolicy: {
          requestedToolsets: [
            "memory_den_read",
            "memory_profile",
            "skills_read",
          ],
        },
        prompt: {
          system: "Use selected memory and skills tools.",
          instructions: ["Keep memory sources distinct."],
        },
        skills: ["memory-skill"],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(skillsDir, "memory-skill.md"),
    `---
title: Memory Skill
summary: Keep Den and profile memory separate.
tags:
  - memory
---

Use Den memory for product/project facts and dense profile memory for stable profile-local preferences.
`,
  );

  await native.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
  });
  await native.addProfileMemory({
    profileId,
    targetType: "profile",
    key: "memory-boundary",
    content: "Dense profile memory is compact profile-local runtime state.",
  });
  const profileMemory = await native.listProfileMemory({ profileId });
  const profileContext = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId,
  });
  const assembled = buildProfileRoleAssembly(profileContext, {
    denMemoryContext: renderDenMemoryContext({
      mode: "metadata",
      projectId: "rusty-crew",
      profileId,
    }),
    denseProfileMemoryContext: renderDenseProfileMemoryContext(profileMemory),
  });
  const toolOutputs: Record<string, string> = {};
  const brainEvents = await native.subscribeEvents({
    eventKinds: ["brain_event_observed"],
    sessionId,
  });

  const brain = await registerBrainImplementationRuntime(
    native,
    {
      implementationId: "memory-skills-proof" as BrainImplementationId,
      profileId,
      toolProfile: profileContext.toolSelection.toolProfile,
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createPiAgentBrain({
      createAgent: (options) => new MemorySkillsFakeAgent(options, toolOutputs),
      resolveTools: ({ wake }) => [
        ...resolveDenMemoryTools({
          client: denClient,
          policy: { mode: "metadata" },
          session: wake.state.session,
          runtimeContext: {
            projectId: "rusty-crew",
            taskId: 2910,
          },
        }),
        denseProfileMemoryTool({
          client: native,
          mode: "read_only",
          session: wake.state.session,
        }),
        ...resolveSkillsTools({ skillsDir }),
      ],
      planActions: ({ wake }): BrainAction[] => [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "memory and skills wake completed",
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
  assert.equal(denCalls[0]?.context?.sessionId, sessionId);
  assert.match(toolOutputs.den_memory_recall, /Den memory belongs to Den/);
  assert.match(toolOutputs.dense_profile_memory, /memory-boundary/);
  assert.match(toolOutputs.skills_list, /memory-skill/);
  assert.match(toolOutputs.skill_view, /Memory Skill/);
  assert.equal(await native.countRows("completion_packets"), 1);
  assert.equal(await native.countRows("tool_call_history"), 8);

  const observedEvents = await native.drainSubscriptionEvents(brainEvents, 20);
  const toolEvents = observedEvents.filter(
    (event): event is Extract<CoreEvent, { type: "brain_event_observed" }> =>
      event.type === "brain_event_observed" &&
      event.event.type.startsWith("tool_call_"),
  );
  assert.equal(toolEvents.length, 8);
  await native.unsubscribeEvents(brainEvents);

  const toolDiagnostics = buildToolRegistryDiagnostics({
    catalogId: "memory-skills-proof",
    inventoryRequest: {
      requestedToolsets: ["memory_den_read", "memory_profile", "skills_read"],
    },
  });
  const contextDiagnostics = buildToolContextDiagnosticsReport({
    now: "2026-06-20T09:00:00Z",
    session: {
      sessionId,
      agentId,
      profileId,
      kind: "full",
    },
    toolDiagnostics,
    toolSelection: profileContext.toolSelection,
    profileContext,
    roleAssembly: assembled.roleAssembly,
    systemPrompt: assembled.systemPrompt,
    memorySkillsPlanning: {
      denMemory: {
        configured: true,
        clientAvailable: true,
        mode: "metadata",
        endpointConfigured: true,
      },
      skills: {
        rootConfigured: true,
        rootReadable: true,
        profileSkillCount: 1,
        loadedSkillCount: 1,
      },
      denseProfileMemory: {
        clientAvailable: true,
        recordCount: profileMemory.length,
        maxRecordsPerProfile: 50,
        capReached: false,
      },
    },
  });
  assert.equal(
    contextDiagnostics.memorySkillsPlanning.denMemory.clientAvailable,
    true,
  );
  assert.equal(
    contextDiagnostics.memorySkillsPlanning.skills.loadedSkillCount,
    1,
  );
  assert.equal(
    contextDiagnostics.memorySkillsPlanning.denseProfileMemory.recordCount,
    1,
  );

  console.log(
    JSON.stringify(
      {
        wakeId,
        selectedTools: profileContext.toolSelection.toolProfile.tools.map(
          (tool) => tool.name,
        ),
        denCalls: denCalls.length,
        denseMemoryRecords: profileMemory.length,
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
        toolProfile: profileContext.toolSelection.toolProfile,
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-20T09:00:00Z",
        lastActiveAt: "2026-06-20T09:00:00Z",
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
  rmSync(profileRoot, { force: true, recursive: true });
}
