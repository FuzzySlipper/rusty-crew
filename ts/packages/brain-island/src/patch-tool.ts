import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { promisify } from "node:util";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import { Type, type Static } from "typebox";
import type { LocalToolContext } from "./local-code-tools.js";

const execFileAsync = promisify(execFile);

const patchParameters = Type.Object({
  mode: Type.Optional(
    Type.Union([Type.Literal("replace"), Type.Literal("patch")]),
  ),
  path: Type.Optional(Type.String()),
  old_string: Type.Optional(Type.String()),
  new_string: Type.Optional(Type.String()),
  replace_all: Type.Optional(Type.Boolean()),
  patch: Type.Optional(Type.String()),
});

type PatchParams = Static<typeof patchParameters>;

interface PatchToolDetails {
  ok: boolean;
  path?: string;
  replacements?: number;
  filesApplied?: number;
  errors?: number;
  diff?: string;
  error?: string;
}

interface MatchResult {
  indices: readonly number[];
  unique: boolean;
  count: number;
  baseContent: string;
  replace(content: string, replacement: string): string;
}

interface V4AFile {
  path: string;
  hunks: V4AHunk[];
}

interface V4AHunk {
  context: string;
  removals: string[];
  additions: string[];
}

export function patchTool(
  context: LocalToolContext,
): BrainTool<typeof patchParameters, PatchToolDetails> {
  return {
    name: "patch",
    label: "Patch",
    description:
      "Apply bounded find-and-replace edits or V4A multi-file patches inside the session workdir and return a unified diff.",
    parameters: patchParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: PatchParams) => {
      if ((params.mode ?? "replace") === "patch") {
        return executePatchMode(context, params);
      }
      return executeReplaceMode(context, params);
    },
  };
}

async function executeReplaceMode(
  context: LocalToolContext,
  params: PatchParams,
): Promise<BrainToolResult<PatchToolDetails>> {
  const path = params.path ?? "";
  const oldString = params.old_string ?? "";
  const newString = params.new_string ?? "";
  const replaceAll = params.replace_all === true;

  if (path.length === 0) {
    return errorResult("path is required in replace mode");
  }
  if (oldString.length === 0) {
    return errorResult("old_string is required in replace mode");
  }

  let absolutePath: string;
  try {
    absolutePath = scopedPath(context.workdir, path);
  } catch {
    return errorResult(`path escapes session workdir: ${path}`);
  }

  const originalContent = await fs
    .readFile(absolutePath, "utf8")
    .catch((error: unknown) =>
      errorResult(`Cannot read file ${path}: ${errorMessage(error)}`),
    );
  if (typeof originalContent !== "string") {
    return originalContent;
  }

  const matchResult = findBestMatch(originalContent, oldString, replaceAll);
  if (!matchResult) {
    return errorResult(
      `Could not find a unique match for old_string in ${path}. Use search_files to find the exact text first.`,
    );
  }
  if (!matchResult.unique && !replaceAll) {
    return errorResult(
      `old_string matched ${matchResult.count} times in ${path}. Set replace_all=true or narrow old_string.`,
    );
  }

  const patchedContent = matchResult.replace(
    matchResult.baseContent,
    newString,
  );
  const diff = buildDiff(path, matchResult.baseContent, patchedContent).join(
    "\n",
  );
  const rollback = originalContent;

  try {
    await fs.mkdir(dirname(absolutePath), { recursive: true });
    await fs.writeFile(absolutePath, patchedContent, "utf8");
  } catch (error: unknown) {
    return errorResult(`Failed to write ${path}: ${errorMessage(error)}`);
  }

  const syntaxResult = await checkSyntax(absolutePath);
  if (!syntaxResult.ok) {
    await fs.writeFile(absolutePath, rollback, "utf8").catch(() => undefined);
    return errorResult(
      `Syntax check failed after edit; file rolled back.\n${syntaxResult.error}`,
    );
  }

  return textResult(diff, {
    ok: true,
    path,
    replacements: matchResult.count,
    diff,
  });
}

async function executePatchMode(
  context: LocalToolContext,
  params: PatchParams,
): Promise<BrainToolResult<PatchToolDetails>> {
  const patchBlock = params.patch ?? "";
  if (patchBlock.length === 0) {
    return errorResult("patch field is required in patch mode");
  }

  const files = parseV4APatch(patchBlock);
  if (files.length === 0) {
    return errorResult(
      "Could not parse any file blocks from the patch content. Expected *** Begin Patch / *** Update File / @@ / +/- lines.",
    );
  }

  const diffs: string[] = [];
  const errors: string[] = [];
  for (const file of files) {
    await applyPatchFile(context, file, diffs, errors);
  }

  const output = [
    ...(diffs.length > 0 ? [`Applied ${diffs.length} file(s)`, ...diffs] : []),
    ...(errors.length > 0 ? [`Errors (${errors.length}):`, ...errors] : []),
  ].join("\n");

  return textResult(output, {
    ok: errors.length === 0,
    filesApplied: diffs.length,
    errors: errors.length,
    diff: output,
  });
}

async function applyPatchFile(
  context: LocalToolContext,
  file: V4AFile,
  diffs: string[],
  errors: string[],
): Promise<void> {
  let absolutePath: string;
  try {
    absolutePath = scopedPath(context.workdir, file.path);
  } catch {
    errors.push(`path escapes session workdir: ${file.path}`);
    return;
  }

  const originalContent = await fs
    .readFile(absolutePath, "utf8")
    .catch((error: unknown) => {
      errors.push(`Cannot read ${file.path}: ${errorMessage(error)}`);
      return undefined;
    });
  if (originalContent === undefined) {
    return;
  }

  const patchedContent = applyV4AHunks(originalContent, file.hunks);
  if (patchedContent === originalContent) {
    errors.push(
      `No changes applied to ${file.path}; context lines did not match`,
    );
    return;
  }

  await fs.mkdir(dirname(absolutePath), { recursive: true });
  await fs.writeFile(absolutePath, patchedContent, "utf8");

  const syntaxResult = await checkSyntax(absolutePath);
  if (!syntaxResult.ok) {
    await fs
      .writeFile(absolutePath, originalContent, "utf8")
      .catch(() => undefined);
    errors.push(
      `${file.path}: syntax check failed; rolled back: ${syntaxResult.error}`,
    );
    return;
  }

  diffs.push(buildDiff(file.path, originalContent, patchedContent).join("\n"));
}

function findBestMatch(
  content: string,
  needle: string,
  allowMultiple: boolean,
): MatchResult | undefined {
  const exactIndices = allIndices(content, needle);
  if (exactIndices.length > 0) {
    return buildMatchResult(content, needle, exactIndices, allowMultiple);
  }

  const trimmedContent = content
    .split("\n")
    .map((line) => line.trimEnd())
    .join("\n");
  const trimmedNeedle = needle
    .split("\n")
    .map((line) => line.trimEnd())
    .join("\n");
  const trimmedIndices = allIndices(trimmedContent, trimmedNeedle);
  if (trimmedIndices.length > 0) {
    return buildMatchResult(
      trimmedContent,
      trimmedNeedle,
      trimmedIndices,
      allowMultiple,
    );
  }

  const fullyTrimmedContent = content
    .split("\n")
    .map((line) => line.trim())
    .join("\n");
  const fullyTrimmedNeedle = needle
    .split("\n")
    .map((line) => line.trim())
    .join("\n");
  const fullTrimIndices = allIndices(fullyTrimmedContent, fullyTrimmedNeedle);
  if (fullTrimIndices.length > 0) {
    return buildMatchResult(
      fullyTrimmedContent,
      fullyTrimmedNeedle,
      fullTrimIndices,
      allowMultiple,
    );
  }

  return undefined;
}

function allIndices(content: string, needle: string): number[] {
  const indices: number[] = [];
  let startIndex = 0;
  while (true) {
    const index = content.indexOf(needle, startIndex);
    if (index === -1) {
      return indices;
    }
    indices.push(index);
    startIndex = index + 1;
  }
}

function buildMatchResult(
  content: string,
  needle: string,
  indices: readonly number[],
  allowMultiple: boolean,
): MatchResult | undefined {
  if (indices.length === 0) {
    return undefined;
  }
  if (indices.length > 1 && !allowMultiple) {
    return {
      indices,
      unique: false,
      count: indices.length,
      baseContent: content,
      replace: () => content,
    };
  }

  return {
    indices,
    unique: indices.length === 1,
    count: indices.length,
    baseContent: content,
    replace: (base, replacement) => {
      let result = base;
      for (const index of [...indices].sort((left, right) => right - left)) {
        result =
          result.slice(0, index) +
          replacement +
          result.slice(index + needle.length);
      }
      return result;
    },
  };
}

function parseV4APatch(patchBlock: string): V4AFile[] {
  const files: V4AFile[] = [];
  let currentFile: V4AFile | undefined;
  let currentHunk: V4AHunk | undefined;

  for (const line of patchBlock.split("\n")) {
    if (/^\*\*\*\s*Begin\s+Patch\s*$/iu.test(line)) {
      continue;
    }
    if (/^\*\*\*\s*End\s+Patch\s*$/iu.test(line)) {
      break;
    }

    const fileMatch = /^\*\*\*\s*Update\s+File:\s+(.+?)\s*$/iu.exec(line);
    if (fileMatch) {
      currentFile = { path: fileMatch[1]!.trim(), hunks: [] };
      files.push(currentFile);
      currentHunk = undefined;
      continue;
    }

    const hunkMatch = /^@@\s*(.*?)\s*@@$/iu.exec(line);
    if (hunkMatch && currentFile) {
      currentHunk = {
        context: hunkMatch[1]!.trim(),
        removals: [],
        additions: [],
      };
      currentFile.hunks.push(currentHunk);
      continue;
    }

    if (!currentHunk) {
      continue;
    }
    if (line.startsWith("-") && !line.startsWith("--")) {
      currentHunk.removals.push(line.slice(1));
    } else if (line.startsWith("+") && !line.startsWith("++")) {
      currentHunk.additions.push(line.slice(1));
    }
  }

  return files.filter((file) => file.hunks.length > 0);
}

function applyV4AHunks(content: string, hunks: readonly V4AHunk[]): string {
  let result = content;
  for (const hunk of hunks) {
    const removal = hunk.removals.join("\n");
    const addition = hunk.additions.join("\n");
    if (removal.length === 0) {
      const lines = result.split("\n");
      const contextIndex = lines.findIndex((line) =>
        line.includes(hunk.context),
      );
      if (contextIndex !== -1) {
        lines.splice(contextIndex + 1, 0, ...hunk.additions);
        result = lines.join("\n");
      }
      continue;
    }
    const index = result.indexOf(removal);
    if (index !== -1) {
      result =
        result.slice(0, index) +
        addition +
        result.slice(index + removal.length);
    }
  }
  return result;
}

function buildDiff(path: string, original: string, patched: string): string[] {
  const originalLines = original.split("\n");
  const patchedLines = patched.split("\n");
  const lines = [
    `--- ${path}`,
    `+++ ${path}`,
    `@@ -1,${originalLines.length} +1,${patchedLines.length} @@`,
  ];
  const max = Math.max(originalLines.length, patchedLines.length);
  for (let index = 0; index < max; index += 1) {
    const before = originalLines[index];
    const after = patchedLines[index];
    if (before === after) {
      lines.push(` ${before ?? ""}`);
    } else {
      if (before !== undefined) {
        lines.push(`-${before}`);
      }
      if (after !== undefined) {
        lines.push(`+${after}`);
      }
    }
  }
  return lines;
}

async function checkSyntax(
  filePath: string,
): Promise<{ ok: boolean; error?: string }> {
  if (filePath.endsWith(".json")) {
    return checkJsonSyntax(filePath);
  }
  if (filePath.endsWith(".ts") || filePath.endsWith(".tsx")) {
    return checkTypeScriptSyntax(filePath);
  }
  return { ok: true };
}

async function checkTypeScriptSyntax(
  filePath: string,
): Promise<{ ok: boolean; error?: string }> {
  try {
    const { stdout, stderr } = await execFileAsync(
      "npx",
      ["tsc", "--noEmit", "--pretty", "false", filePath],
      { timeout: 15_000, maxBuffer: 512_000 },
    );
    const output = [stdout, stderr].filter(Boolean).join("\n").trim();
    return output ? { ok: false, error: output } : { ok: true };
  } catch (error: unknown) {
    if (error instanceof Error && "stderr" in error) {
      return {
        ok: false,
        error: String((error as { stderr: unknown }).stderr),
      };
    }
    return { ok: false, error: errorMessage(error) };
  }
}

async function checkJsonSyntax(
  filePath: string,
): Promise<{ ok: boolean; error?: string }> {
  try {
    JSON.parse(await fs.readFile(filePath, "utf8"));
    return { ok: true };
  } catch (error: unknown) {
    return { ok: false, error: errorMessage(error) };
  }
}

function scopedPath(workdir: string, path: string): string {
  const target = resolve(workdir, path);
  const scopedRelative = relative(workdir, target);
  if (
    scopedRelative === ".." ||
    scopedRelative.startsWith(`..${sep}`) ||
    scopedRelative.startsWith("/")
  ) {
    throw new Error(`path escapes session workdir: ${path}`);
  }
  return target;
}

function textResult(
  text: string,
  details: PatchToolDetails,
): BrainToolResult<PatchToolDetails> {
  return { content: [{ type: "text", text }], details };
}

function errorResult(message: string): BrainToolResult<PatchToolDetails> {
  return {
    content: [{ type: "text", text: message }],
    details: { ok: false, error: message },
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
