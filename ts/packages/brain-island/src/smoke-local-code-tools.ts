import assert from "node:assert/strict";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
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
const outsideDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-local-tools-outside-"),
);
const outsideReadPath = join(outsideDir, "outside-note.txt");
const outsideWritePath = join(outsideDir, "outside-write.txt");
writeFileSync(join(workdir, "note.txt"), "hello from local tools\n", "utf8");
writeFileSync(outsideReadPath, "hello from outside local tools\n", "utf8");

const sessionId = "local-tools-session" as SessionId;
const agentId = "local-tools-agent" as AgentId;
const selection = selectToolProfile({
  profileId: "local-tools-profile" as ProfileId,
  policy: {
    requestedTools: ["read_file", "write_file", "terminal", "worker_write"],
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
    const writeFile = tools.find((tool) => tool.name === "write_file");
    const terminal = tools.find((tool) => tool.name === "terminal");
    const workerWrite = tools.find((tool) => tool.name === "worker_write");
    assert.ok(readFile);
    assert.ok(writeFile);
    assert.ok(terminal);
    assert.ok(workerWrite);

    this.results.read_file = await readFile.execute("read-file-call", {
      path: "note.txt",
    });
    this.results.read_file_absolute = await readFile.execute(
      "read-file-absolute-call",
      {
        path: outsideReadPath,
      },
    );
    this.results.write_file_absolute = await writeFile.execute(
      "write-file-absolute-call",
      {
        path: outsideWritePath,
        content: "written outside workdir\n",
      },
    );
    try {
      this.results.worker_write_outside = await workerWrite.execute(
        "worker-write-outside-call",
        {
          path: outsideWritePath,
          content: "should not write\n",
        },
      );
    } catch (error) {
      this.results.worker_write_outside = {
        content: [
          {
            type: "text",
            text: error instanceof Error ? error.message : String(error),
          },
        ],
        details: {
          error: error instanceof Error ? error.message : String(error),
        },
      };
    }
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
  assert.match(
    textResult(toolResults.read_file_absolute),
    /hello from outside local tools/,
  );
  assert.equal(
    readFileSync(outsideWritePath, "utf8"),
    "written outside workdir\n",
  );
  assert.match(
    textResult(toolResults.worker_write_outside),
    /path escapes session workdir/,
  );
  assert.equal(
    existsSync(join(workdir, outsideWritePath.replace(/^\/+/, ""))),
    false,
  );
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
        absoluteReadText: textResult(toolResults.read_file_absolute).trim(),
        absoluteWriteText: readFileSync(outsideWritePath, "utf8").trim(),
        workerWriteDenied: /path escapes session workdir/.test(
          textResult(toolResults.worker_write_outside),
        ),
        terminalExit: (toolResults.terminal.details as { exitCode: number })
          .exitCode,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(workdir, { force: true, recursive: true });
  rmSync(outsideDir, { force: true, recursive: true });
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
