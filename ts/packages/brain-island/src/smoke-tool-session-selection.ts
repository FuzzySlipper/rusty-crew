import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentTool as PiAgentTool,
} from "@earendil-works/pi-agent-core";
import {
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  resolveToolSession,
} from "./index.js";

const sessionId = "tool-session" as SessionId;
const agentId = "tool-agent" as AgentId;
const wake = {
  wakeId: "wake-tool-session",
  sessionId,
  systemPrompt: "system",
  roleAssembly: { instructions: "tools are selected by ToolProfile" },
  state: {
    session: {
      handle: 1 as SessionHandle,
      sessionId,
      agentId,
      profileId: "prime-coder" as ProfileId,
      kind: "full" as const,
      resourceLimits: {},
      toolProfile: {
        tools: [
          { name: "git_status", description: "Read git status" },
          { name: "read_file", description: "Read files" },
          { name: "patch", description: "Apply patches" },
        ],
      },
      status: "idle" as const,
      brainTurnCount: 0,
      createdAt: "2026-06-20T00:00:00Z",
      lastActiveAt: "2026-06-20T00:00:00Z",
    },
    pendingMessages: [],
    recentEvents: [],
    childCompletions: [],
    fanOutGroups: [],
    deltaPolicy: defaultBodyDeltaPolicy,
  },
};

const selection = resolveToolSession({
  wake,
  resolveTools: () => [
    fakeTool("read_file"),
    fakeTool("dangerous_shell"),
    fakeTool("git_status"),
  ],
});

assert.deepEqual(
  selection.tools.map((tool) => tool.name),
  ["git_status", "read_file"],
);
assert.deepEqual(
  selection.items.map((item) => [item.name, item.status]),
  [
    ["git_status", "callable"],
    ["read_file", "callable"],
    ["patch", "implementation_missing"],
    ["dangerous_shell", "not_requested"],
  ],
);

class FakeAgent {
  subscribe(
    _listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    return () => {};
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {}

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

let capturedToolNames: string[] = [];
const brain = createPiAgentBrain({
  createAgent: (options) => {
    capturedToolNames = (options.initialState?.tools ?? []).map(
      (tool) => tool.name,
    );
    return new FakeAgent();
  },
  resolveTools: () => [
    fakeTool("read_file"),
    fakeTool("dangerous_shell"),
    fakeTool("git_status"),
  ],
});

await brain.wake(wake);

assert.deepEqual(capturedToolNames, ["git_status", "read_file"]);

console.log(
  JSON.stringify(
    {
      callableTools: selection.tools.map((tool) => tool.name),
      inventory: selection.items.map((item) => ({
        name: item.name,
        status: item.status,
      })),
      brainTools: capturedToolNames,
    },
    null,
    2,
  ),
);

function fakeTool(name: string): PiAgentTool {
  return {
    name,
    description: `${name} description`,
    label: name,
    parameters: {} as PiAgentTool["parameters"],
    execute: async () => ({
      content: [{ type: "text", text: `${name} result` }],
      details: {},
    }),
  };
}
