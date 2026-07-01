import assert from "node:assert/strict";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  BrainAction,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  resolveDelegationTools,
  selectToolProfile,
} from "./index.js";

const sessionId = "delegation-tools-session" as SessionId;
const agentId = "delegation-tools-agent" as AgentId;
const profileId = "delegation-tools-profile" as ProfileId;

const selection = selectToolProfile({
  profileId,
  policy: { requestedToolsets: ["delegation_basic"] },
});

assert.deepEqual(
  selection.toolProfile.tools.map((tool) => tool.name),
  [
    "spawn_subagent",
    "spawn_subagent_md",
    "fan_out_subagents",
    "fan_out_subagents_md",
    "scout_codebase",
    "summarize_files",
    "find_relevant_paths",
    "deliver_completion_md",
  ],
);

class DelegationToolFakeAgent {
  constructor(private readonly options: PiAgentOptions) {}

  subscribe(
    _listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    return () => {};
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    await this.callTool("spawn_subagent", {
      profileId: "coder-profile",
      taskId: "3107",
      prompt: "Implement one delegated tool slice.",
      expectedOutput: "completion packet",
      priority: "high",
      correlationId: "spawn-proof",
      parentConsumption: "await_completion",
    });
    await this.callTool("spawn_subagent_md", {
      markdown: `---
profile: coder-profile
task: 3877
priority: normal
correlation: spawn-md-proof
parent_consumption: await_completion
---

Implement the markdown delegation tool.

## Expected Output

Completion packet with smoke evidence.`,
    });
    await this.callTool(
      "spawn_subagent_md",
      {
        markdown: `---
profile: coder-profile

This frontmatter is not closed.`,
      },
      false,
    );
    await this.callTool("fan_out_subagents", {
      groupId: "audit-fan-out",
      maxConcurrency: 2,
      failurePolicy: "fail_soft",
      subagents: [
        {
          profileId: "reviewer-profile",
          prompt: "Review the delegated tool slice.",
          correlationId: "fan-out-review",
        },
        {
          profileId: "packet-auditor-profile",
          prompt: "Audit completion packet evidence.",
          correlationId: "fan-out-packet",
        },
      ],
    });
    await this.callTool("fan_out_subagents_md", {
      markdown: `---
group_id: md-fan-out
max_concurrency: 2
failure_policy: fail_soft
priority: normal
---

## reviewer-profile
correlation: md-review

Review the markdown delegation tool.

## packet-auditor-profile
correlation: md-packet

Audit markdown handoff evidence.`,
    });
    await this.callTool(
      "fan_out_subagents_md",
      {
        markdown: `---
group_id: md-fan-out-invalid
failure_policy: keep_going_anyway
---

## reviewer-profile

Review the markdown delegation tool.`,
      },
      false,
    );
    await this.callTool("scout_codebase", {
      goal: "Find the brain action submission path.",
      paths: ["ts/packages/brain-island/src", "crates/core"],
    });
    await this.callTool("summarize_files", {
      files: ["README.md", "docs/delegation-request-contract.md"],
      focus: "Delegation model",
    });
    await this.callTool("find_relevant_paths", {
      query: "Where are ToolProfile descriptors selected?",
      roots: ["ts/packages/brain-island/src"],
    });
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}

  private async callTool(
    name: string,
    params: Record<string, unknown>,
    expectedOk = true,
  ) {
    const tool = this.options.initialState?.tools?.find(
      (candidate) => candidate.name === name,
    );
    assert.ok(tool, `${name} should be selected`);
    const result = await tool.execute(`${name}-call`, params);
    assert.equal((result.details as { ok?: boolean }).ok, expectedOk);
  }
}

let plannerSawToolActions: readonly BrainAction[] = [];
const brain = createPiAgentBrain({
  createAgent: (options) => new DelegationToolFakeAgent(options),
  resolveTools: resolveDelegationTools,
  toolProfile: selection.toolProfile,
  planActions: ({ toolActions }) => {
    plannerSawToolActions = toolActions ?? [];
    return [];
  },
});

const result = await brain.wake({
  wakeId: "delegation-tools-wake",
  sessionId,
  systemPrompt: "system",
  roleAssembly: { instructions: "invoke delegation tools" },
  state: {
    session: {
      handle: 1 as SessionHandle,
      sessionId,
      agentId,
      profileId,
      kind: "full",
      resourceLimits: {
        workdir: "/home/dev/rusty-crew",
        maxDurationMs: 30_000,
        maxDelegationDepth: 2,
      },
      toolProfile: selection.toolProfile,
      status: "idle",
      brainTurnCount: 0,
      createdAt: "2026-06-21T00:00:00Z",
      lastActiveAt: "2026-06-21T00:00:00Z",
    },
    pendingMessages: [],
    recentEvents: [],
    childCompletions: [],
    fanOutGroups: [],
    deltaPolicy: defaultBodyDeltaPolicy,
  },
});

assert.equal(result.actions.length, 9);
assert.equal(plannerSawToolActions.length, 9);
assert.ok(
  result.actions.every((action) => action.type === "request_delegation"),
);
const [
  spawn,
  spawnMd,
  fanOutFirst,
  fanOutSecond,
  fanOutMdFirst,
  fanOutMdSecond,
  scout,
  summarize,
  findPaths,
] = result.actions.filter(
  (action): action is Extract<BrainAction, { type: "request_delegation" }> =>
    action.type === "request_delegation",
);
assert.equal(spawn?.profileId, "coder-profile");
assert.equal(spawn?.correlationId, "spawn-proof");
assert.equal(spawnMd?.correlationId, "spawn-md-proof");
assert.equal(spawnMd?.taskId, "3877");
assert.match(spawnMd?.expectedOutput ?? "", /Completion packet/);
assert.equal(fanOutFirst?.fanOutGroupId, "audit-fan-out");
assert.equal(fanOutSecond?.fanOutFailurePolicy, "fail_soft");
assert.equal(fanOutMdFirst?.fanOutGroupId, "md-fan-out");
assert.equal(fanOutMdFirst?.correlationId, "md-review");
assert.equal(fanOutMdSecond?.profileId, "packet-auditor-profile");
assert.equal(scout?.resourceLimits?.maxDelegationDepth, 0);
assert.match(summarize?.prompt ?? "", /README\.md/);
assert.match(findPaths?.prompt ?? "", /ToolProfile/);

console.log(
  JSON.stringify(
    {
      selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
      queuedActions: result.actions.length,
      fanOutGroupId: fanOutFirst?.fanOutGroupId,
      childDepths: result.actions.map((action) =>
        action.type === "request_delegation"
          ? action.resourceLimits?.maxDelegationDepth
          : undefined,
      ),
    },
    null,
    2,
  ),
);
