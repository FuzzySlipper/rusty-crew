import { spawn } from "node:child_process";
import { readdir, readFile, stat, writeFile } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";
import type { BrainTool } from "./brain-tool.js";
import type { SessionState } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeLocalCodeResourcePolicyPlan,
} from "@rusty-crew/native-bridge";
import { Type, type Static } from "typebox";
import { patchTool } from "./patch-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export const defaultLocalCodeResourcePolicy: NativeLocalCodeResourcePolicyPlan =
  {
    commandTimeoutMs: 30_000,
    maxReadBytes: 256 * 1024,
    maxSearchFileBytes: 256 * 1024,
    maxCommandOutputBytes: 128 * 1024,
    tools: [
      {
        toolName: "read_file",
        filesystemScope: "unrestricted",
        writesFiles: false,
        executesProcess: false,
        executionMode: "parallel",
        outputShape: "text_with_file_read_details",
      },
      {
        toolName: "write_file",
        filesystemScope: "unrestricted",
        writesFiles: true,
        executesProcess: false,
        executionMode: "sequential",
        outputShape: "json_file_write_details",
      },
      {
        toolName: "search_files",
        filesystemScope: "unrestricted",
        writesFiles: false,
        executesProcess: false,
        executionMode: "parallel",
        outputShape: "json_search_matches_with_skips",
      },
      {
        toolName: "terminal",
        filesystemScope: "unrestricted",
        writesFiles: false,
        executesProcess: true,
        executionMode: "sequential",
        outputShape: "process_result_text_and_details",
      },
      {
        toolName: "git_status",
        filesystemScope: "unrestricted",
        writesFiles: false,
        executesProcess: true,
        executionMode: "parallel",
        outputShape: "process_result_text_and_details",
      },
      {
        toolName: "git_diff",
        filesystemScope: "unrestricted",
        writesFiles: false,
        executesProcess: true,
        executionMode: "parallel",
        outputShape: "process_result_text_and_details",
      },
      {
        toolName: "patch",
        filesystemScope: "unrestricted",
        writesFiles: true,
        executesProcess: true,
        executionMode: "sequential",
        outputShape: "patch_diff_with_apply_details",
      },
      {
        toolName: "worker_write",
        filesystemScope: "workdir",
        writesFiles: true,
        executesProcess: false,
        executionMode: "sequential",
        outputShape: "json_file_write_details",
      },
      {
        toolName: "worker_patch",
        filesystemScope: "workdir",
        writesFiles: true,
        executesProcess: true,
        executionMode: "sequential",
        outputShape: "patch_diff_with_apply_details",
      },
    ],
    denialReasonCodes: [
      "path_escape",
      "read_dir_failed",
      "read_file_failed",
      "stat_failed",
      "file_too_large",
      "write_failed",
      "command_invalid",
      "command_failed",
      "command_timeout",
      "patch_parse_failed",
      "patch_no_match",
      "patch_non_unique_match",
      "syntax_check_failed",
    ],
  };

const readFileParameters = Type.Object({
  path: Type.String({ minLength: 1 }),
  maxBytes: Type.Optional(Type.Number({ minimum: 1 })),
});

const writeFileParameters = Type.Object({
  path: Type.String({ minLength: 1 }),
  content: Type.String(),
});

const searchFilesParameters = Type.Object({
  query: Type.String({ minLength: 1 }),
  root: Type.Optional(Type.String({ minLength: 1 })),
  maxResults: Type.Optional(Type.Number({ minimum: 1 })),
});

const terminalParameters = Type.Object({
  command: Type.String({ minLength: 1 }),
  timeoutMs: Type.Optional(Type.Number({ minimum: 1 })),
  maxOutputBytes: Type.Optional(Type.Number({ minimum: 1 })),
});

const gitStatusParameters = Type.Object({});
const gitDiffParameters = Type.Object({
  path: Type.Optional(Type.String({ minLength: 1 })),
});

type ReadFileParams = Static<typeof readFileParameters>;
type WriteFileParams = Static<typeof writeFileParameters>;
type SearchFilesParams = Static<typeof searchFilesParameters>;
type TerminalParams = Static<typeof terminalParameters>;
type GitDiffParams = Static<typeof gitDiffParameters>;

export interface LocalToolContext {
  workdir: string;
  maxReadBytes: number;
  maxSearchFileBytes: number;
  maxCommandOutputBytes: number;
  commandTimeoutMs: number;
  maxDurationMs?: number;
  resourcePolicy: NativeLocalCodeResourcePolicyPlan;
  runtimeActivity?: {
    bridge: Pick<
      NativeBridgeModule,
      "beginRuntimeActivity" | "finishRuntimeActivity"
    >;
    wakeId: string;
    session: Pick<SessionState, "agentId" | "profileId" | "sessionId">;
  };
}

type FilesystemScope = "unrestricted" | "workdir";

export interface LocalToolProcessResult {
  command: string;
  cwd: string;
  exitCode: number | null;
  signal: NodeJS.Signals | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

const processOutputDrainGraceMs = 50;
const processTerminationGraceMs = 250;

export function createLocalCodeToolResolver(
  input: {
    resourcePolicy?: NativeLocalCodeResourcePolicyPlan;
    bridge?: Pick<
      NativeBridgeModule,
      "beginRuntimeActivity" | "finishRuntimeActivity"
    >;
  } = {},
): BrainToolResolver {
  return ({ wake }) => {
    const context = localToolContext(
      input.resourcePolicy ?? defaultLocalCodeResourcePolicy,
      wake.state.session,
    );
    if (input.bridge !== undefined) {
      context.runtimeActivity = {
        bridge: input.bridge,
        wakeId: wake.wakeId,
        session: wake.state.session,
      };
    }
    return [
      readFileTool(context),
      writeFileTool(context),
      searchFilesTool(context),
      terminalTool(context),
      gitStatusTool(context),
      gitDiffTool(context),
      patchTool(context, {
        filesystemScope: localToolFilesystemScope(context, "patch"),
      }),
      workerWriteTool(context),
      workerPatchTool(context),
    ];
  };
}

export const resolveLocalCodeTools = createLocalCodeToolResolver();

export function readFileTool(
  context: LocalToolContext,
): BrainTool<typeof readFileParameters> {
  return {
    name: "read_file",
    description:
      "Read a UTF-8 text file. Relative paths resolve from the session workdir; absolute paths are allowed.",
    label: "Read file",
    parameters: readFileParameters,
    execute: async (_toolCallId, params: ReadFileParams) => {
      const target = resolveToolPath(
        context.workdir,
        params.path,
        "unrestricted",
      );
      const maxBytes = params.maxBytes ?? context.maxReadBytes;
      const data = await readFile(target);
      const truncated = data.byteLength > maxBytes;
      const text = data.subarray(0, maxBytes).toString("utf8");
      const details = {
        path: params.path,
        absolutePath: target,
        bytesRead: Math.min(data.byteLength, maxBytes),
        totalBytes: data.byteLength,
        truncated,
      };
      return {
        content: [{ type: "text", text }],
        details,
      };
    },
  };
}

export function writeFileTool(
  context: LocalToolContext,
): BrainTool<typeof writeFileParameters> {
  return {
    name: "write_file",
    description:
      "Write a bounded UTF-8 text file. Relative paths resolve from the session workdir; absolute paths are allowed.",
    label: "Write file",
    parameters: writeFileParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: WriteFileParams) => {
      const target = resolveToolPath(
        context.workdir,
        params.path,
        "unrestricted",
      );
      await writeFile(target, params.content, "utf8");
      const details = {
        path: params.path,
        absolutePath: target,
        bytesWritten: Buffer.byteLength(params.content, "utf8"),
      };
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(details, null, 2),
          },
        ],
        details,
      };
    },
  };
}

export function searchFilesTool(
  context: LocalToolContext,
): BrainTool<typeof searchFilesParameters> {
  return {
    name: "search_files",
    description:
      "Search file paths and UTF-8 file contents beneath a root. Relative roots resolve from the session workdir; absolute roots are allowed.",
    label: "Search files",
    parameters: searchFilesParameters,
    execute: async (_toolCallId, params: SearchFilesParams) => {
      const maxResults = params.maxResults ?? 50;
      const matches: Array<{ path: string; line?: number; preview: string }> =
        [];
      const skipped: SearchSkippedPath[] = [];
      const root = resolveToolPath(
        context.workdir,
        params.root ?? ".",
        "unrestricted",
      );
      await searchDirectory(root, root, params.query, matches, maxResults, {
        skipped,
        maxSearchFileBytes: context.maxSearchFileBytes,
      });
      const details = {
        query: params.query,
        root,
        matches,
        skipped,
        skippedCount: skipped.length,
        truncated: matches.length >= maxResults,
      };
      return {
        content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
        details,
      };
    },
  };
}

export function terminalTool(
  context: LocalToolContext,
): BrainTool<typeof terminalParameters> {
  return {
    name: "terminal",
    description: "Run a bounded shell command in the session workdir.",
    label: "Terminal",
    parameters: terminalParameters,
    executionMode: "sequential",
    execute: async (toolCallId, params: TerminalParams, signal) => {
      const timeoutMs = Math.min(
        params.timeoutMs ?? context.maxDurationMs ?? context.commandTimeoutMs,
        context.maxDurationMs ?? context.commandTimeoutMs,
      );
      const result = await runShellCommand(params.command, context.workdir, {
        signal,
        timeoutMs,
        maxOutputBytes: params.maxOutputBytes ?? context.maxCommandOutputBytes,
        runtimeActivity: processRuntimeActivity(
          context,
          toolCallId,
          "terminal",
        ),
      });
      return {
        content: [{ type: "text", text: formatProcessResult(result) }],
        details: result,
      };
    },
  };
}

export function gitStatusTool(
  context: LocalToolContext,
): BrainTool<typeof gitStatusParameters> {
  return {
    name: "git_status",
    description:
      "Return concise git working tree status for the session workdir.",
    label: "Git status",
    parameters: gitStatusParameters,
    execute: async (toolCallId, _params, signal) => {
      const result = await runProcess(
        "git",
        ["status", "--short"],
        context.workdir,
        {
          signal,
          timeoutMs: context.maxDurationMs ?? context.commandTimeoutMs,
          maxOutputBytes: context.maxCommandOutputBytes,
          runtimeActivity: processRuntimeActivity(
            context,
            toolCallId,
            "git_status",
          ),
        },
      );
      return {
        content: [{ type: "text", text: formatProcessResult(result) }],
        details: result,
      };
    },
  };
}

export function gitDiffTool(
  context: LocalToolContext,
): BrainTool<typeof gitDiffParameters> {
  return {
    name: "git_diff",
    description: "Return a git diff from the session workdir.",
    label: "Git diff",
    parameters: gitDiffParameters,
    execute: async (toolCallId, params: GitDiffParams, signal) => {
      const scopedDiffPath = params.path
        ? relative(
            context.workdir,
            resolveToolPath(context.workdir, params.path, "unrestricted"),
          )
        : undefined;
      const args = ["diff", "--", ...(scopedDiffPath ? [scopedDiffPath] : [])];
      const result = await runProcess("git", args, context.workdir, {
        signal,
        timeoutMs: context.maxDurationMs ?? context.commandTimeoutMs,
        maxOutputBytes: context.maxCommandOutputBytes,
        runtimeActivity: processRuntimeActivity(
          context,
          toolCallId,
          "git_diff",
        ),
      });
      return {
        content: [{ type: "text", text: formatProcessResult(result) }],
        details: result,
      };
    },
  };
}

export function workerWriteTool(
  context: LocalToolContext,
): BrainTool<typeof writeFileParameters> {
  return {
    name: "worker_write",
    description:
      "Write a bounded UTF-8 text file inside the delegated worker workdir.",
    label: "Worker write",
    parameters: writeFileParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: WriteFileParams) => {
      const target = resolveToolPath(
        context.workdir,
        params.path,
        localToolFilesystemScope(context, "worker_write"),
      );
      await writeFile(target, params.content, "utf8");
      const details = {
        path: params.path,
        absolutePath: target,
        bytesWritten: Buffer.byteLength(params.content, "utf8"),
      };
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(details, null, 2),
          },
        ],
        details,
      };
    },
  };
}

export function workerPatchTool(context: LocalToolContext): BrainTool {
  return patchTool(context, {
    name: "worker_patch",
    label: "Worker patch",
    description:
      "Apply bounded find-and-replace edits or V4A multi-file patches inside the delegated worker workdir and return a unified diff.",
    filesystemScope: localToolFilesystemScope(context, "worker_patch"),
  });
}

function localToolContext(
  policy: NativeLocalCodeResourcePolicyPlan,
  session: SessionState,
): LocalToolContext {
  const workspace = session.workspace;
  if (workspace == null) {
    throw new Error(
      `session_workspace_missing: session ${session.sessionId} has no canonical workspace`,
    );
  }
  const limits = session.resourceLimits;
  const maxDurationMs = limits.maxDurationMs ?? policy.maxDurationMs;
  return {
    workdir: resolve(workspace.cwd),
    maxReadBytes: policy.maxReadBytes,
    maxSearchFileBytes: policy.maxSearchFileBytes,
    maxCommandOutputBytes: policy.maxCommandOutputBytes,
    commandTimeoutMs: maxDurationMs ?? policy.commandTimeoutMs,
    ...(maxDurationMs === undefined ? {} : { maxDurationMs }),
    resourcePolicy: policy,
  };
}

function localToolFilesystemScope(
  context: LocalToolContext,
  toolName: string,
): FilesystemScope {
  return (
    context.resourcePolicy.tools.find((tool) => tool.toolName === toolName)
      ?.filesystemScope ?? "unrestricted"
  );
}

export function resolveToolPath(
  workdir: string,
  path: string,
  scope: FilesystemScope = "unrestricted",
): string {
  const target = resolve(workdir, path);
  if (scope === "unrestricted") {
    return target;
  }
  const scopedRelative = relative(workdir, target);
  if (
    scopedRelative === ".." ||
    scopedRelative.startsWith(`..${sep}`) ||
    scopedRelative === "" ||
    scopedRelative.startsWith("/")
  ) {
    if (scopedRelative !== "") {
      throw new Error(`path escapes session workdir: ${path}`);
    }
  }
  return target;
}

async function searchDirectory(
  root: string,
  current: string,
  query: string,
  matches: Array<{ path: string; line?: number; preview: string }>,
  maxResults: number,
  state: SearchState,
): Promise<void> {
  if (matches.length >= maxResults) {
    return;
  }

  const entries = await readdir(current, { withFileTypes: true }).catch(
    (error: unknown) => {
      recordSkippedPath(state, root, current, "read_dir_failed", error);
      return undefined;
    },
  );
  if (!entries) return;
  for (const entry of entries) {
    if (matches.length >= maxResults || shouldSkipEntry(entry.name)) {
      continue;
    }
    const absolutePath = resolve(current, entry.name);
    const displayPath = relative(root, absolutePath);
    if (entry.isDirectory()) {
      await searchDirectory(
        root,
        absolutePath,
        query,
        matches,
        maxResults,
        state,
      );
      continue;
    }
    if (!entry.isFile()) {
      continue;
    }
    if (displayPath.includes(query)) {
      matches.push({ path: displayPath, preview: displayPath });
      continue;
    }
    await searchFileContent(
      absolutePath,
      displayPath,
      query,
      matches,
      maxResults,
      state,
      root,
    );
  }
}

interface SearchSkippedPath {
  path: string;
  reason: string;
  message?: string;
}

interface SearchState {
  skipped: SearchSkippedPath[];
  maxSearchFileBytes: number;
}

async function searchFileContent(
  absolutePath: string,
  displayPath: string,
  query: string,
  matches: Array<{ path: string; line?: number; preview: string }>,
  maxResults: number,
  state: SearchState,
  root: string,
): Promise<void> {
  const metadata = await stat(absolutePath).catch((error: unknown) => {
    recordSkippedPath(state, root, absolutePath, "stat_failed", error);
    return undefined;
  });
  if (!metadata) return;
  if (metadata.size > state.maxSearchFileBytes) {
    recordSkippedPath(state, root, absolutePath, "file_too_large");
    return;
  }
  const text = await readFile(absolutePath, "utf8").catch((error: unknown) => {
    recordSkippedPath(state, root, absolutePath, "read_file_failed", error);
    return undefined;
  });
  if (!text) {
    return;
  }
  const lines = text.split(/\r?\n/u);
  for (const [index, line] of lines.entries()) {
    if (matches.length >= maxResults) {
      return;
    }
    if (line.includes(query)) {
      matches.push({
        path: displayPath,
        line: index + 1,
        preview: line.trim().slice(0, 240),
      });
    }
  }
}

function shouldSkipEntry(name: string): boolean {
  return (
    name === ".git" ||
    name === "node_modules" ||
    name === "target" ||
    name === ".tmp" ||
    name.startsWith("systemd-private-")
  );
}

function recordSkippedPath(
  state: SearchState,
  root: string,
  absolutePath: string,
  reason: string,
  error?: unknown,
): void {
  if (state.skipped.length >= 200) return;
  state.skipped.push({
    path: relative(root, absolutePath) || ".",
    reason,
    message: error instanceof Error ? error.message : undefined,
  });
}

function runShellCommand(
  command: string,
  cwd: string,
  options: {
    signal: AbortSignal | undefined;
    timeoutMs: number;
    maxOutputBytes: number;
    runtimeActivity?: ProcessRuntimeActivity;
  },
): Promise<LocalToolProcessResult> {
  return runProcess(command, [], cwd, {
    ...options,
    shell: true,
  });
}

function runProcess(
  command: string,
  args: readonly string[],
  cwd: string,
  options: {
    signal: AbortSignal | undefined;
    timeoutMs: number;
    maxOutputBytes: number;
    shell?: boolean;
    runtimeActivity?: ProcessRuntimeActivity;
  },
): Promise<LocalToolProcessResult> {
  if (command.includes("\0")) {
    throw new Error("command cannot contain null bytes");
  }
  if (options.signal?.aborted) {
    return Promise.reject(processCancellationError(options.signal));
  }

  return new Promise((resolvePromise, reject) => {
    const ownsProcessGroup = process.platform !== "win32";
    const child = spawn(command, [...args], {
      cwd,
      shell: options.shell ?? false,
      detached: ownsProcessGroup,
    });
    const processId = child.pid;
    const runtimeActivity = options.runtimeActivity;
    const activityId =
      processId === undefined || runtimeActivity === undefined
        ? undefined
        : `subprocess:${runtimeActivity.wakeId}:${runtimeActivity.callId}:${processId}`;
    const activityStarted =
      activityId === undefined ||
      processId === undefined ||
      runtimeActivity === undefined
        ? Promise.resolve(false)
        : runtimeActivity.bridge
            .beginRuntimeActivity({
              activityId,
              parentActivityId: `tool:${runtimeActivity.wakeId}:${runtimeActivity.callId}`,
              kind: "subprocess",
              owner: "type_script_host",
              agentId: runtimeActivity.session.agentId,
              profileId: runtimeActivity.session.profileId,
              sessionId: runtimeActivity.session.sessionId,
              wakeId: runtimeActivity.wakeId,
              phase: "running",
              summary: `${runtimeActivity.toolName} child process`,
              toolName: runtimeActivity.toolName,
              processId,
            })
            .then(() => true)
            .catch(() => false);
    let stdout = "";
    let stderr = "";
    let settled = false;
    let timedOut = false;
    let cancelled = false;
    let terminationStarted = false;
    let exitResult:
      | { exitCode: number | null; signal: NodeJS.Signals | null }
      | undefined;
    let outputDrain: NodeJS.Timeout | undefined;
    let terminationEscalation: NodeJS.Timeout | undefined;

    const signalProcessTree = (signal: NodeJS.Signals): void => {
      const pid = child.pid;
      if (pid === undefined) return;
      if (ownsProcessGroup) {
        try {
          process.kill(-pid, signal);
          return;
        } catch (error) {
          if (
            !(error instanceof Error) ||
            !("code" in error) ||
            error.code !== "ESRCH"
          ) {
            throw error;
          }
        }
      }
      child.kill(signal);
    };

    const beginProcessTreeTermination = (): void => {
      if (terminationStarted) return;
      terminationStarted = true;
      try {
        signalProcessTree("SIGTERM");
      } catch {
        child.kill("SIGTERM");
      }
      terminationEscalation = setTimeout(() => {
        try {
          signalProcessTree("SIGKILL");
        } catch {
          child.kill("SIGKILL");
        }
      }, processTerminationGraceMs);
      terminationEscalation.unref();
    };

    const onAbort = (): void => {
      cancelled = true;
      clearTimeout(timeout);
      beginProcessTreeTermination();
    };

    const finish = (
      exitCode: number | null,
      signal: NodeJS.Signals | null,
    ): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (!timedOut && !cancelled) {
        clearTimeout(terminationEscalation);
      }
      clearTimeout(outputDrain);
      options.signal?.removeEventListener("abort", onAbort);
      child.stdout?.destroy();
      child.stderr?.destroy();
      const result = {
        command: [command, ...args].join(" "),
        cwd,
        exitCode,
        signal,
        stdout,
        stderr,
        timedOut,
      };
      const failed = timedOut || cancelled || exitCode !== 0;
      void finishProcessActivity(
        runtimeActivity,
        activityId,
        activityStarted,
        failed ? "failed" : "completed",
        timedOut
          ? "command_timeout"
          : cancelled
            ? "process_cancelled"
            : failed
              ? "command_failed"
              : undefined,
        failed
          ? cancelled
            ? "tool child process was cancelled"
            : "tool child process exited unsuccessfully"
          : "tool child process completed",
      ).finally(() => {
        if (cancelled) {
          reject(processCancellationError(options.signal));
          return;
        }
        resolvePromise(result);
      });
    };

    const timeout = setTimeout(() => {
      timedOut = true;
      beginProcessTreeTermination();
    }, options.timeoutMs);
    options.signal?.addEventListener("abort", onAbort, { once: true });
    if (options.signal?.aborted) onAbort();

    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stdout?.on("data", (chunk: string) => {
      stdout = boundedAppend(stdout, chunk, options.maxOutputBytes);
    });
    child.stderr?.on("data", (chunk: string) => {
      stderr = boundedAppend(stderr, chunk, options.maxOutputBytes);
    });
    child.on("error", (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      clearTimeout(outputDrain);
      clearTimeout(terminationEscalation);
      options.signal?.removeEventListener("abort", onAbort);
      void finishProcessActivity(
        runtimeActivity,
        activityId,
        activityStarted,
        "failed",
        "process_spawn_failed",
        "tool child process failed to start",
      ).finally(() => reject(error));
    });
    child.on("exit", (exitCode, signal) => {
      clearTimeout(timeout);
      exitResult = { exitCode, signal };
      outputDrain = setTimeout(
        () => finish(exitCode, signal),
        processOutputDrainGraceMs,
      );
    });
    child.on("close", (exitCode, signal) => {
      finish(exitResult?.exitCode ?? exitCode, exitResult?.signal ?? signal);
    });
  });
}

function processCancellationError(signal: AbortSignal | undefined): Error {
  return signal?.reason instanceof Error
    ? signal.reason
    : new DOMException("The operation was aborted", "AbortError");
}

interface ProcessRuntimeActivity {
  bridge: Pick<
    NativeBridgeModule,
    "beginRuntimeActivity" | "finishRuntimeActivity"
  >;
  wakeId: string;
  callId: string;
  toolName: string;
  session: Pick<SessionState, "agentId" | "profileId" | "sessionId">;
}

function processRuntimeActivity(
  context: LocalToolContext,
  callId: string,
  toolName: string,
): ProcessRuntimeActivity | undefined {
  if (context.runtimeActivity === undefined) return undefined;
  return { ...context.runtimeActivity, callId, toolName };
}

async function finishProcessActivity(
  activity: ProcessRuntimeActivity | undefined,
  activityId: string | undefined,
  started: Promise<boolean>,
  status: "completed" | "failed",
  reasonCode: string | undefined,
  summary: string,
): Promise<void> {
  if (activity === undefined || activityId === undefined || !(await started)) {
    return;
  }
  await activity.bridge
    .finishRuntimeActivity({
      activityId,
      status,
      phase: status,
      reasonCode,
      summary,
    })
    .catch(() => undefined);
}

function boundedAppend(
  current: string,
  chunk: string,
  maxBytes: number,
): string {
  const next = current + chunk;
  if (Buffer.byteLength(next, "utf8") <= maxBytes) {
    return next;
  }
  return next.slice(0, maxBytes);
}

function formatProcessResult(result: LocalToolProcessResult): string {
  return JSON.stringify(result, null, 2);
}
