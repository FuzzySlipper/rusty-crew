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
  resolveCompletionTools,
  selectToolProfile,
} from "./index.js";

const sessionId = "completion-tools-session" as SessionId;
const agentId = "completion-tools-agent" as AgentId;
const profileId = "completion-tools-profile" as ProfileId;

const selection = selectToolProfile({
  profileId,
  policy: { requestedTools: ["deliver_completion_md"] },
});

assert.deepEqual(
  selection.toolProfile.tools.map((tool) => tool.name),
  ["deliver_completion_md"],
);

class CompletionToolFakeAgent {
  constructor(private readonly options: PiAgentOptions) {}

  subscribe(
    _listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    return () => {};
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    await this.callTool("deliver_completion_md", {
      markdown: `---
status: completed
---

## Summary

Implemented markdown completion packet submission.

## Evidence

- Smoke: completion tools`,
    });
    await this.callTool(
      "deliver_completion_md",
      {
        markdown: `---
status: maybe_done
---

## Summary

This should be rejected.`,
      },
      false,
    );
    await this.callTool(
      "deliver_completion_md",
      {
        markdown: `---
status: blocked
---

`,
      },
      false,
    );
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
  createAgent: (options) => new CompletionToolFakeAgent(options),
  resolveTools: resolveCompletionTools,
  toolProfile: selection.toolProfile,
  planActions: ({ toolActions }) => {
    plannerSawToolActions = toolActions ?? [];
    return [];
  },
});

const result = await brain.wake({
  wakeId: "completion-tools-wake",
  sessionId,
  systemPrompt: "system",
  roleAssembly: { instructions: "invoke completion tool" },
  state: {
    session: {
      handle: 1 as SessionHandle,
      sessionId,
      agentId,
      profileId,
      kind: "delegated",
      resourceLimits: {},
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

assert.equal(result.actions.length, 1);
assert.equal(plannerSawToolActions.length, 1);
const [completion] = result.actions.filter(
  (action): action is Extract<BrainAction, { type: "deliver_completion" }> =>
    action.type === "deliver_completion",
);
assert.equal(completion?.packet.sessionId, sessionId);
assert.equal(completion?.packet.status, "completed");
assert.match(completion?.packet.summary ?? "", /markdown completion/);

console.log(
  JSON.stringify(
    {
      selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
      queuedActions: result.actions.length,
      completionStatus: completion?.packet.status,
      completionSummary: completion?.packet.summary,
    },
    null,
    2,
  ),
);
