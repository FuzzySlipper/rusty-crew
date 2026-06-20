import { spawn } from "node:child_process";
import { readdir, readFile, stat, writeFile } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";
import type { AgentTool as PiAgentTool } from "@earendil-works/pi-agent-core";
import type { ResourceLimits } from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import { patchTool } from "./patch-tool.js";
import type { PiAgentToolResolver } from "./tool-session-selection.js";

const defaultMaxReadBytes = 256 * 1024;
const defaultMaxSearchFileBytes = 256 * 1024;
const defaultMaxCommandOutputBytes = 128 * 1024;
const defaultCommandTimeoutMs = 30_000;

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
  maxDurationMs?: number;
}

export interface LocalToolProcessResult {
  command: string;
  cwd: string;
  exitCode: number | null;
  signal: NodeJS.Signals | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

export const resolveLocalCodeTools: PiAgentToolResolver = ({ wake }) => {
  const context = localToolContext(wake.state.session.resourceLimits);
  return [
    readFileTool(context),
    writeFileTool(context),
    searchFilesTool(context),
    terminalTool(context),
    gitStatusTool(context),
    gitDiffTool(context),
    patchTool(context),
  ];
};

export function readFileTool(
  context: LocalToolContext,
): PiAgentTool<typeof readFileParameters> {
  return {
    name: "read_file",
    description: "Read a UTF-8 text file from the session workdir.",
    label: "Read file",
    parameters: readFileParameters,
    execute: async (_toolCallId, params: ReadFileParams) => {
      const target = scopedPath(context.workdir, params.path);
      const maxBytes = params.maxBytes ?? defaultMaxReadBytes;
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
): PiAgentTool<typeof writeFileParameters> {
  return {
    name: "write_file",
    description: "Write a bounded UTF-8 text file inside the session workdir.",
    label: "Write file",
    parameters: writeFileParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: WriteFileParams) => {
      const target = scopedPath(context.workdir, params.path);
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
): PiAgentTool<typeof searchFilesParameters> {
  return {
    name: "search_files",
    description:
      "Search file paths and UTF-8 file contents beneath the session workdir.",
    label: "Search files",
    parameters: searchFilesParameters,
    execute: async (_toolCallId, params: SearchFilesParams) => {
      const maxResults = params.maxResults ?? 50;
      const matches: Array<{ path: string; line?: number; preview: string }> =
        [];
      await searchDirectory(
        context.workdir,
        context.workdir,
        params.query,
        matches,
        maxResults,
      );
      const details = {
        query: params.query,
        matches,
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
): PiAgentTool<typeof terminalParameters> {
  return {
    name: "terminal",
    description: "Run a bounded shell command in the session workdir.",
    label: "Terminal",
    parameters: terminalParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: TerminalParams, signal) => {
      const timeoutMs = Math.min(
        params.timeoutMs ?? context.maxDurationMs ?? defaultCommandTimeoutMs,
        context.maxDurationMs ?? defaultCommandTimeoutMs,
      );
      const result = await runShellCommand(params.command, context.workdir, {
        signal,
        timeoutMs,
        maxOutputBytes: params.maxOutputBytes ?? defaultMaxCommandOutputBytes,
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
): PiAgentTool<typeof gitStatusParameters> {
  return {
    name: "git_status",
    description:
      "Return concise git working tree status for the session workdir.",
    label: "Git status",
    parameters: gitStatusParameters,
    execute: async (_toolCallId, _params, signal) => {
      const result = await runProcess(
        "git",
        ["status", "--short"],
        context.workdir,
        {
          signal,
          timeoutMs: context.maxDurationMs ?? defaultCommandTimeoutMs,
          maxOutputBytes: defaultMaxCommandOutputBytes,
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
): PiAgentTool<typeof gitDiffParameters> {
  return {
    name: "git_diff",
    description: "Return a git diff from the session workdir.",
    label: "Git diff",
    parameters: gitDiffParameters,
    execute: async (_toolCallId, params: GitDiffParams, signal) => {
      const scopedDiffPath = params.path
        ? relative(context.workdir, scopedPath(context.workdir, params.path))
        : undefined;
      const args = ["diff", "--", ...(scopedDiffPath ? [scopedDiffPath] : [])];
      const result = await runProcess("git", args, context.workdir, {
        signal,
        timeoutMs: context.maxDurationMs ?? defaultCommandTimeoutMs,
        maxOutputBytes: defaultMaxCommandOutputBytes,
      });
      return {
        content: [{ type: "text", text: formatProcessResult(result) }],
        details: result,
      };
    },
  };
}

function localToolContext(limits: ResourceLimits): LocalToolContext {
  return {
    workdir: resolve(limits.workdir ?? process.cwd()),
    maxDurationMs: limits.maxDurationMs,
  };
}

function scopedPath(workdir: string, path: string): string {
  const target = resolve(workdir, path);
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
): Promise<void> {
  if (matches.length >= maxResults) {
    return;
  }

  const entries = await readdir(current, { withFileTypes: true });
  for (const entry of entries) {
    if (matches.length >= maxResults || shouldSkipEntry(entry.name)) {
      continue;
    }
    const absolutePath = resolve(current, entry.name);
    const displayPath = relative(root, absolutePath);
    if (entry.isDirectory()) {
      await searchDirectory(root, absolutePath, query, matches, maxResults);
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
    );
  }
}

async function searchFileContent(
  absolutePath: string,
  displayPath: string,
  query: string,
  matches: Array<{ path: string; line?: number; preview: string }>,
  maxResults: number,
): Promise<void> {
  const metadata = await stat(absolutePath);
  if (metadata.size > defaultMaxSearchFileBytes) {
    return;
  }
  const text = await readFile(absolutePath, "utf8").catch(() => undefined);
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
  return name === ".git" || name === "node_modules" || name === "target";
}

function runShellCommand(
  command: string,
  cwd: string,
  options: {
    signal: AbortSignal | undefined;
    timeoutMs: number;
    maxOutputBytes: number;
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
  },
): Promise<LocalToolProcessResult> {
  if (command.includes("\0")) {
    throw new Error("command cannot contain null bytes");
  }

  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, [...args], {
      cwd,
      shell: options.shell ?? false,
      signal: options.signal,
    });
    let stdout = "";
    let stderr = "";
    let settled = false;
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, options.timeoutMs);

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
      reject(error);
    });
    child.on("close", (exitCode, signal) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolvePromise({
        command: [command, ...args].join(" "),
        cwd,
        exitCode,
        signal,
        stdout,
        stderr,
        timedOut,
      });
    });
  });
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
