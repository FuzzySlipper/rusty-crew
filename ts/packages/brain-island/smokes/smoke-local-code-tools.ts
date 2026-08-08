import assert from "node:assert/strict";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  ProfileId,
  RuntimeActivityBegin,
  RuntimeActivityFinish,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  AgentEvent as ChatCompletionsEvent,
  AgentMessage as ChatCompletionsMessage,
  AgentOptions as ChatCompletionsOptions,
  AgentToolResult,
} from "./support/chat-completions-test-harness.js";
import {
  createLocalCodeToolResolver,
  defaultBodyDeltaPolicy,
  defaultLocalCodeResourcePolicy,
  selectToolProfile,
  workerWriteTool,
} from "../src/index.js";
import { createChatCompletionsBrain } from "./support/chat-completions-test-harness.js";

const workdir = mkdtempSync(join(tmpdir(), "rusty-crew-local-tools-"));
const secondWorkdir = mkdtempSync(
  join(tmpdir(), "rusty-crew-local-tools-second-"),
);
const outsideDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-local-tools-outside-"),
);
const outsideReadPath = join(outsideDir, "outside-note.txt");
const outsideWritePath = join(outsideDir, "outside-write.txt");
writeFileSync(join(workdir, "note.txt"), "hello from local tools\n", "utf8");
mkdirSync(join(workdir, "nested"), { recursive: true });
writeFileSync(
  join(workdir, "nested", "search-note.txt"),
  "needle appears in a nested file\n",
  "utf8",
);
writeFileSync(
  join(workdir, "large-skip.txt"),
  `${"x".repeat(270 * 1024)}needle after large payload\n`,
  "utf8",
);
writeFileSync(outsideReadPath, "hello from outside local tools\n", "utf8");
writeFileSync(
  join(secondWorkdir, "note.txt"),
  "hello from second workspace\n",
  "utf8",
);
mkdirSync(join(secondWorkdir, "nested"), { recursive: true });
writeFileSync(
  join(secondWorkdir, "nested", "search-note.txt"),
  "needle appears in the second workspace\n",
  "utf8",
);

const sessionId = "local-tools-session" as SessionId;
const agentId = "local-tools-agent" as AgentId;
const selection = selectToolProfile({
  profileId: "local-tools-profile" as ProfileId,
  policy: {
    requestedTools: ["read_file", "write_file", "terminal", "worker_write"],
  },
});
const searchSelection = selectToolProfile({
  profileId: "local-tools-profile" as ProfileId,
  policy: {
    requestedTools: ["search_files"],
  },
});

class ToolCallingFakeAgent {
  constructor(
    private readonly options: ChatCompletionsOptions,
    private readonly results: Record<string, AgentToolResult<unknown>>,
  ) {}

  subscribe(
    _listener: (event: ChatCompletionsEvent, signal: AbortSignal) => void,
  ): () => void {
    return () => {};
  }

  async prompt(
    _input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
  ): Promise<void> {
    const tools = this.options.initialState?.tools ?? [];
    const readFile = tools.find((tool) => tool.name === "read_file");
    const writeFile = tools.find((tool) => tool.name === "write_file");
    const searchFiles = tools.find((tool) => tool.name === "search_files");
    const terminal = tools.find((tool) => tool.name === "terminal");
    const workerWrite = tools.find((tool) => tool.name === "worker_write");
    assert.ok(readFile);
    assert.ok(writeFile);
    assert.ok(searchFiles);
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
    this.results.search_files = await searchFiles.execute("search-files-call", {
      query: "needle",
      root: ".",
      maxResults: 20,
    });
    try {
      this.results.worker_write_outside = await workerWrite.execute(
        "worker-write-outside-call",
        {
          path: outsideWritePath,
          content: "full session worker tool remains unrestricted\n",
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
const activityBegins: RuntimeActivityBegin[] = [];
const activityFinishes: RuntimeActivityFinish[] = [];
const policyBackedResolver = createLocalCodeToolResolver({
  resourcePolicy: {
    ...defaultLocalCodeResourcePolicy,
    maxReadBytes: 64,
    maxSearchFileBytes: 1_024,
    maxCommandOutputBytes: 64,
  },
  bridge: {
    beginRuntimeActivity: async (input) => {
      activityBegins.push(input);
      return input as never;
    },
    finishRuntimeActivity: async (input) => {
      activityFinishes.push(input);
      return input as never;
    },
  },
});
const brain = createChatCompletionsBrain({
  createAgent: (options) => new ToolCallingFakeAgent(options, toolResults),
  resolveTools: policyBackedResolver,
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
        workspace: {
          cwd: workdir,
          revision: 1,
          updatedAt: "2026-06-20T00:00:00Z",
        },
        resourceLimits: {
          maxDurationMs: 5_000,
        },
        toolProfile: {
          tools: [
            ...selection.toolProfile.tools,
            ...searchSelection.toolProfile.tools,
          ],
        },
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
    "full session worker tool remains unrestricted\n",
  );
  const searchDetails = toolResults.search_files.details as {
    matches: Array<{ path: string; preview: string }>;
    skipped: Array<{ path: string; reason: string }>;
    skippedCount: number;
  };
  assert.ok(
    searchDetails.matches.some(
      (match) =>
        match.path === "nested/search-note.txt" &&
        match.preview.includes("needle"),
    ),
  );
  assert.ok(
    searchDetails.skipped.some(
      (skip) =>
        skip.path === "large-skip.txt" && skip.reason === "file_too_large",
    ),
  );
  assert.equal(searchDetails.skippedCount, searchDetails.skipped.length);
  assert.equal(
    readFileSync(outsideWritePath, "utf8"),
    "full session worker tool remains unrestricted\n",
  );
  const constrainedWorkerWrite = workerWriteTool({
    workdir,
    delegatedWorkspaceConstraint: workdir,
    maxReadBytes: defaultLocalCodeResourcePolicy.maxReadBytes,
    maxSearchFileBytes: defaultLocalCodeResourcePolicy.maxSearchFileBytes,
    maxCommandOutputBytes: defaultLocalCodeResourcePolicy.maxCommandOutputBytes,
    commandTimeoutMs: defaultLocalCodeResourcePolicy.commandTimeoutMs,
    resourcePolicy: defaultLocalCodeResourcePolicy,
  });
  await assert.rejects(
    constrainedWorkerWrite.execute("constrained-worker-write", {
      path: outsideWritePath,
      content: "delegated constraint must reject this\n",
    }),
    /path escapes session workdir/,
  );
  const escapedCreationPath = join(outsideDir, "escaped-created.txt");
  symlinkSync(outsideDir, join(workdir, "escape-directory"), "dir");
  await assert.rejects(
    constrainedWorkerWrite.execute("constrained-worker-write-dir-symlink", {
      path: "escape-directory/escaped-created.txt",
      content: "must remain inside the delegated constraint\n",
    }),
    /delegated workspace path contains symlink/,
  );
  assert.equal(existsSync(escapedCreationPath), false);

  const outsideBeforeFileSymlink = readFileSync(outsideWritePath, "utf8");
  symlinkSync(outsideWritePath, join(workdir, "escape-file"), "file");
  await assert.rejects(
    constrainedWorkerWrite.execute("constrained-worker-write-file-symlink", {
      path: "escape-file",
      content: "must not replace the outside file\n",
    }),
    /delegated workspace path contains symlink/,
  );
  assert.equal(
    readFileSync(outsideWritePath, "utf8"),
    outsideBeforeFileSymlink,
  );

  await constrainedWorkerWrite.execute("constrained-worker-write-new-file", {
    path: "nested/delegated-created.txt",
    content: "created inside constraint\n",
  });
  assert.equal(
    readFileSync(join(workdir, "nested", "delegated-created.txt"), "utf8"),
    "created inside constraint\n",
  );
  assert.match(textResult(toolResults.terminal), /local-tools-ok/);
  assert.equal(
    (toolResults.terminal.details as { exitCode: number }).exitCode,
    0,
  );
  const terminalProcess = activityBegins.find(
    (activity) => activity.toolName === "terminal",
  );
  assert.ok(terminalProcess);
  assert.equal(terminalProcess.kind, "subprocess");
  assert.equal(terminalProcess.sessionId, sessionId);
  assert.equal(
    terminalProcess.parentActivityId,
    "tool:wake-local-tools:terminal-call",
  );
  assert.equal(terminalProcess.summary, "terminal child process");
  assert.equal(
    activityFinishes.find(
      (activity) => activity.activityId === terminalProcess.activityId,
    )?.status,
    "completed",
  );

  await brain.wake({
    wakeId: "wake-local-tools-second-workspace",
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
        workspace: {
          cwd: secondWorkdir,
          revision: 2,
          updatedAt: "2026-06-20T00:01:00Z",
        },
        resourceLimits: { maxDurationMs: 5_000 },
        toolProfile: {
          tools: [
            ...selection.toolProfile.tools,
            ...searchSelection.toolProfile.tools,
          ],
        },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-20T00:00:00Z",
        lastActiveAt: "2026-06-20T00:01:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    },
  });
  assert.match(
    textResult(toolResults.read_file),
    /hello from second workspace/,
  );

  console.log(
    JSON.stringify(
      {
        selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
        readFileText: textResult(toolResults.read_file).trim(),
        absoluteReadText: textResult(toolResults.read_file_absolute).trim(),
        absoluteWriteText: readFileSync(outsideWritePath, "utf8").trim(),
        searchMatches: searchDetails.matches.length,
        searchSkipped: searchDetails.skipped,
        fullSessionWorkerWriteUnrestricted: true,
        delegatedConstraintDeniedEscape: true,
        delegatedConstraintDeniedSymlinkEscape: true,
        terminalExit: (toolResults.terminal.details as { exitCode: number })
          .exitCode,
        runtimeActivity: terminalProcess.activityId,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(workdir, { force: true, recursive: true });
  rmSync(secondWorkdir, { force: true, recursive: true });
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
