import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentToolResult,
} from "@earendil-works/pi-agent-core";
import {
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  resolveLocalCodeTools,
  selectToolProfile,
} from "./index.js";

const workdir = mkdtempSync(join(tmpdir(), "rusty-crew-local-tools-"));
writeFileSync(join(workdir, "note.txt"), "hello from local tools\n", "utf8");

const sessionId = "local-tools-session" as SessionId;
const agentId = "local-tools-agent" as AgentId;
const selection = selectToolProfile({
  profileId: "local-tools-profile" as ProfileId,
  policy: {
    requestedTools: ["read_file", "terminal"],
  },
});

class ToolCallingFakeAgent {
  constructor(
    private readonly options: PiAgentOptions,
    private readonly results: Record<string, AgentToolResult<unknown>>,
  ) {}

  subscribe(
    _listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    return () => {};
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    const tools = this.options.initialState?.tools ?? [];
    const readFile = tools.find((tool) => tool.name === "read_file");
    const terminal = tools.find((tool) => tool.name === "terminal");
    assert.ok(readFile);
    assert.ok(terminal);

    this.results.read_file = await readFile.execute("read-file-call", {
      path: "note.txt",
    });
    this.results.terminal = await terminal.execute("terminal-call", {
      command: "printf local-tools-ok",
      timeoutMs: 5_000,
    });
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const toolResults: Record<string, AgentToolResult<unknown>> = {};
const brain = createPiAgentBrain({
  createAgent: (options) => new ToolCallingFakeAgent(options, toolResults),
  resolveTools: resolveLocalCodeTools,
});

try {
  await brain.wake({
    wakeId: "wake-local-tools",
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "invoke selected local tools" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId,
        profileId: "local-tools-profile" as ProfileId,
        kind: "full",
        resourceLimits: {
          workdir,
          maxDurationMs: 5_000,
        },
        toolProfile: selection.toolProfile,
        status: "idle",
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
  });

  assert.match(textResult(toolResults.read_file), /hello from local tools/);
  assert.match(textResult(toolResults.terminal), /local-tools-ok/);
  assert.equal(
    (toolResults.terminal.details as { exitCode: number }).exitCode,
    0,
  );

  console.log(
    JSON.stringify(
      {
        selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
        readFileText: textResult(toolResults.read_file).trim(),
        terminalExit: (toolResults.terminal.details as { exitCode: number })
          .exitCode,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(workdir, { force: true, recursive: true });
}

function textResult(result: AgentToolResult<unknown> | undefined): string {
  assert.ok(result);
  return result.content
    .flatMap((content) =>
      content.type === "text" && typeof content.text === "string"
        ? [content.text]
        : [],
    )
    .join("");
}
