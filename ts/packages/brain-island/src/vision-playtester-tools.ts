import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { isAbsolute, dirname, resolve } from "node:path";
import { promisify } from "node:util";
import { Type, type Static, type TSchema } from "typebox";
import type {
  BrainTool,
  BrainToolContent,
  BrainToolResult,
} from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";
import type { VisionPlaytestOutcome } from "./vision-playtester.js";

const execFileAsync = promisify(execFile);

const nonEmptyString = Type.String({ minLength: 1 });
const point = { x: Type.Number(), y: Type.Number() };
const playtestBudgetSchema = Type.Object({
  max_actions: Type.Integer({ minimum: 1 }),
  max_session_minutes: Type.Number({ exclusiveMinimum: 0 }),
  max_estimated_cost_usd: Type.Optional(Type.Number({ exclusiveMinimum: 0 })),
});
const visibleActionSchema = Type.Union([
  Type.Object({ type: Type.Literal("keyboard_press"), key: nonEmptyString }),
  Type.Object({ type: Type.Literal("keyboard_down"), key: nonEmptyString }),
  Type.Object({ type: Type.Literal("keyboard_up"), key: nonEmptyString }),
  Type.Object({ type: Type.Literal("mouse_move"), ...point }),
  Type.Object({ type: Type.Literal("mouse_click"), ...point }),
  Type.Object({ type: Type.Literal("mouse_down") }),
  Type.Object({ type: Type.Literal("mouse_up") }),
  Type.Object({
    type: Type.Literal("mouse_wheel"),
    deltaX: Type.Optional(Type.Number()),
    deltaY: Type.Number(),
  }),
  Type.Object({
    type: Type.Literal("wait"),
    ms: Type.Integer({ minimum: 1, maximum: 30_000 }),
  }),
]);

export const playtestStartParameters = Type.Object({
  project: nonEmptyString,
  repo_root: nonEmptyString,
  manifest_path: nonEmptyString,
  owner: Type.Optional(nonEmptyString),
  scenario: nonEmptyString,
  den_project_id: Type.Optional(nonEmptyString),
  den_task_id: Type.Optional(Type.Integer({ minimum: 1 })),
  budget: playtestBudgetSchema,
});

export const playtestObserveParameters = Type.Object({
  session_id: nonEmptyString,
  screenshot: Type.Optional(Type.Boolean()),
  label: Type.Optional(nonEmptyString),
  frameBurst: Type.Optional(
    Type.Object({
      count: Type.Integer({ minimum: 2, maximum: 12 }),
      intervalMs: Type.Integer({ minimum: 16, maximum: 2_000 }),
    }),
  ),
});

export const playtestActParameters = Type.Object({
  session_id: nonEmptyString,
  owner: Type.Optional(nonEmptyString),
  sequence: Type.Optional(Type.Integer({ minimum: 0 })),
  actions: Type.Array(visibleActionSchema, { minItems: 1, maxItems: 32 }),
});

export const playtestFinishParameters = Type.Object({
  session_id: nonEmptyString,
  outcome: Type.Union([
    Type.Literal("pass"),
    Type.Literal("fail"),
    Type.Literal("uncertain"),
    Type.Literal("infrastructure_error"),
  ]),
  annotation: nonEmptyString,
  assertions: Type.Optional(
    Type.Array(
      Type.Object({
        name: nonEmptyString,
        pass: Type.Boolean(),
        artifact: nonEmptyString,
      }),
    ),
  ),
});

export type PlaytestStartParams = Static<typeof playtestStartParameters>;
export type PlaytestObserveParams = Static<typeof playtestObserveParameters>;
export type PlaytestActParams = Static<typeof playtestActParameters>;
export type PlaytestFinishParams = Static<typeof playtestFinishParameters>;

export type VisionPlaytesterOperation = "start" | "observe" | "act" | "finish";

export interface VisionPlaytesterCommandResult {
  ok: boolean;
  operation: VisionPlaytesterOperation;
  value?: unknown;
  error?: string;
  stdout?: string;
  stderr?: string;
  imagePaths: string[];
}

export interface VisionPlaytesterRuntime {
  execute(
    operation: VisionPlaytesterOperation,
    request: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<VisionPlaytesterCommandResult>;
}

export interface VisionPlaytesterCliRuntimeOptions {
  cliPath?: string;
  configPath?: string;
  operationTimeoutMs?: number;
}

interface ActivePlaytestBudget {
  maxActions: number;
  maxSessionMinutes: number;
  maxEstimatedCostUsd?: number;
  actionsUsed: number;
  startedAtMs: number;
}

export function createVisionPlaytesterCliRuntime(
  options: VisionPlaytesterCliRuntimeOptions = {},
): VisionPlaytesterRuntime {
  const cliPath =
    options.cliPath ?? process.env.RUSTY_CREW_PLAYTEST_CLI ?? "den-playwright";
  const configPath =
    options.configPath ?? process.env.RUSTY_CREW_PLAYTEST_CONFIG;
  const operationTimeoutMs = options.operationTimeoutMs ?? 120_000;
  const cliRuntime: VisionPlaytesterRuntime = {
    async execute(operation, request, signal) {
      if (!configPath?.trim()) {
        return {
          ok: false,
          operation,
          error:
            "playtest infrastructure is not configured: RUSTY_CREW_PLAYTEST_CONFIG is missing",
          imagePaths: [],
        };
      }
      const args = cliArguments(operation, request, configPath);
      try {
        const result = await execFileAsync(cliPath, args, {
          signal,
          timeout: operationTimeoutMs,
          maxBuffer: 16 * 1024 * 1024,
          encoding: "utf8",
        });
        const stdout = String(result.stdout);
        const value = parseCommandOutput(stdout);
        return {
          ok: true,
          operation,
          value,
          stdout,
          stderr: String(result.stderr),
          imagePaths: imagePathsFromResult(value),
        };
      } catch (error) {
        const detail = processError(error);
        return {
          ok: false,
          operation,
          error: detail.message,
          stdout: detail.stdout,
          stderr: detail.stderr,
          imagePaths: [],
        };
      }
    },
  };
  return withVisionPlaytesterBudgets(cliRuntime);
}

export function withVisionPlaytesterBudgets(
  runtime: VisionPlaytesterRuntime,
  now: () => number = Date.now,
): VisionPlaytesterRuntime {
  const sessions = new Map<string, ActivePlaytestBudget>();
  return {
    async execute(operation, request, signal) {
      if (operation === "start") {
        const result = await runtime.execute(operation, request, signal);
        if (!result.ok) return result;
        const sessionId = findStringField(result.value, "session_id");
        const budget = playtestBudget(request.budget);
        if (!sessionId || !budget) {
          return {
            ok: false,
            operation,
            error:
              "playtest start did not return a session id or a valid delegated budget",
            imagePaths: result.imagePaths,
          };
        }
        sessions.set(sessionId, {
          ...budget,
          actionsUsed: 0,
          startedAtMs: now(),
        });
        return withBudgetStatus(
          result,
          sessionId,
          sessions.get(sessionId)!,
          now(),
        );
      }

      const sessionId = String(request.session_id ?? "");
      const budget = sessions.get(sessionId);
      if (!budget) {
        return {
          ok: false,
          operation,
          error:
            "delegated playtest budget state is unavailable for this session; report infrastructure_error instead of starting over",
          imagePaths: [],
        };
      }
      if (
        operation !== "finish" &&
        elapsedMinutes(budget, now()) > budget.maxSessionMinutes
      ) {
        return budgetFailure(operation, sessionId, budget, now(), "session");
      }
      if (operation === "act") {
        const actionCount = Array.isArray(request.actions)
          ? request.actions.length
          : 0;
        if (budget.actionsUsed + actionCount > budget.maxActions) {
          return budgetFailure(operation, sessionId, budget, now(), "actions");
        }
        // Attempts count even when the browser operation fails or is only
        // partially applied, so retrying cannot evade the delegated budget.
        budget.actionsUsed += actionCount;
      }
      const result = await runtime.execute(operation, request, signal);
      const withStatus = withBudgetStatus(result, sessionId, budget, now());
      if (operation === "finish" && result.ok) sessions.delete(sessionId);
      return withStatus;
    },
  };
}

export function createVisionPlaytesterToolResolver(
  runtime: VisionPlaytesterRuntime = createVisionPlaytesterCliRuntime(),
): BrainToolResolver {
  return () => visionPlaytesterTools(runtime);
}

export function visionPlaytesterTools(
  runtime: VisionPlaytesterRuntime,
): BrainTool[] {
  return [
    playtestStartTool(runtime),
    playtestObserveTool(runtime),
    playtestActTool(runtime),
    playtestFinishTool(runtime),
  ];
}

export function playtestStartTool(runtime: VisionPlaytesterRuntime): BrainTool {
  return playtestTool(
    "playtest_start",
    "Start playtest",
    "Start the supplied exact-revision project and retain the returned playtest session and evidence handles.",
    "start",
    playtestStartParameters,
    runtime,
  );
}

export function playtestObserveTool(
  runtime: VisionPlaytesterRuntime,
): BrainTool {
  return playtestTool(
    "playtest_observe",
    "Observe playtest",
    "Observe the ordinary visible page through a screenshot or bounded frame burst. Returned screenshots are attached as vision inputs.",
    "observe",
    playtestObserveParameters,
    runtime,
  );
}

export function playtestActTool(runtime: VisionPlaytesterRuntime): BrainTool {
  return playtestTool(
    "playtest_act",
    "Act in playtest",
    "Use genuine keyboard, mouse, wheel, or wait input against the visible page.",
    "act",
    playtestActParameters,
    runtime,
  );
}

export function playtestFinishTool(
  runtime: VisionPlaytesterRuntime,
): BrainTool {
  return playtestTool(
    "playtest_finish",
    "Finish playtest",
    "Finalize evidence with pass, fail, uncertain, or infrastructure_error and best-effort cleanup.",
    "finish",
    playtestFinishParameters,
    runtime,
  );
}

function playtestTool<TParameters extends TSchema>(
  name: string,
  label: string,
  description: string,
  operation: VisionPlaytesterOperation,
  parameters: TParameters,
  runtime: VisionPlaytesterRuntime,
): BrainTool<TParameters, VisionPlaytesterCommandResult> {
  return {
    name,
    label,
    description,
    parameters,
    executionMode: "sequential",
    async execute(_callId, params, signal) {
      const result = await runtime.execute(
        operation,
        params as Record<string, unknown>,
        signal,
      );
      return toolResult(result);
    },
  };
}

async function toolResult(
  result: VisionPlaytesterCommandResult,
): Promise<BrainToolResult<VisionPlaytesterCommandResult>> {
  const content: BrainToolContent[] = [
    {
      type: "text",
      text: JSON.stringify(
        result.ok
          ? result.value
          : {
              ok: false,
              operation: result.operation,
              error: result.error,
              stderr: result.stderr,
            },
        null,
        2,
      ),
    },
  ];
  for (const path of result.imagePaths) {
    try {
      const bytes = await readFile(path);
      content.push({
        type: "image",
        data: bytes.toString("base64"),
        mimeType: "image/png",
      });
    } catch {
      // The text evidence still reports the artifact path. A missing image is
      // itself useful infrastructure evidence and must not trigger repair.
    }
  }
  return { content, details: result };
}

function cliArguments(
  operation: VisionPlaytesterOperation,
  request: Record<string, unknown>,
  configPath: string,
): string[] {
  if (operation !== "start") {
    const sessionId = String(request.session_id ?? "");
    const payload = { ...request };
    delete payload.session_id;
    return [
      "playtest",
      operation,
      sessionId,
      "-config",
      configPath,
      "-request",
      JSON.stringify(payload),
    ];
  }
  const args = [
    "playtest",
    "start",
    String(request.project),
    "-config",
    configPath,
    "-repo",
    String(request.repo_root),
    "-manifest",
    String(request.manifest_path),
    "-scenario",
    String(request.scenario),
  ];
  optionalArg(args, "-owner", request.owner);
  optionalArg(args, "-den-project", request.den_project_id);
  optionalArg(args, "-den-task", request.den_task_id);
  return args;
}

function optionalArg(args: string[], flag: string, value: unknown): void {
  if (value !== undefined && value !== null && String(value).trim()) {
    args.push(flag, String(value));
  }
}

function parseCommandOutput(stdout: string): unknown {
  const trimmed = stdout.trim();
  if (!trimmed) return {};
  return JSON.parse(trimmed);
}

function playtestBudget(
  value: unknown,
): Omit<ActivePlaytestBudget, "actionsUsed" | "startedAtMs"> | undefined {
  if (!isRecord(value)) return undefined;
  const maxActions = value.max_actions;
  const maxSessionMinutes = value.max_session_minutes;
  const maxEstimatedCostUsd = value.max_estimated_cost_usd;
  if (
    !Number.isInteger(maxActions) ||
    Number(maxActions) < 1 ||
    typeof maxSessionMinutes !== "number" ||
    !Number.isFinite(maxSessionMinutes) ||
    maxSessionMinutes <= 0 ||
    (maxEstimatedCostUsd !== undefined &&
      (typeof maxEstimatedCostUsd !== "number" ||
        !Number.isFinite(maxEstimatedCostUsd) ||
        maxEstimatedCostUsd <= 0))
  ) {
    return undefined;
  }
  return {
    maxActions: Number(maxActions),
    maxSessionMinutes,
    ...(typeof maxEstimatedCostUsd === "number" ? { maxEstimatedCostUsd } : {}),
  };
}

function withBudgetStatus(
  result: VisionPlaytesterCommandResult,
  sessionId: string,
  budget: ActivePlaytestBudget,
  nowMs: number,
): VisionPlaytesterCommandResult {
  const budgetStatus = {
    session_id: sessionId,
    actions_used: budget.actionsUsed,
    actions_remaining: Math.max(0, budget.maxActions - budget.actionsUsed),
    elapsed_session_minutes: elapsedMinutes(budget, nowMs),
    max_session_minutes: budget.maxSessionMinutes,
    ...(budget.maxEstimatedCostUsd === undefined
      ? {}
      : { max_estimated_cost_usd: budget.maxEstimatedCostUsd }),
  };
  return {
    ...result,
    value: isRecord(result.value)
      ? { ...result.value, delegated_budget: budgetStatus }
      : { result: result.value, delegated_budget: budgetStatus },
  };
}

function budgetFailure(
  operation: VisionPlaytesterOperation,
  sessionId: string,
  budget: ActivePlaytestBudget,
  nowMs: number,
  dimension: "actions" | "session",
): VisionPlaytesterCommandResult {
  const elapsed = elapsedMinutes(budget, nowMs);
  return {
    ok: false,
    operation,
    error:
      dimension === "actions"
        ? `delegated action budget exhausted for ${sessionId}: ${budget.actionsUsed}/${budget.maxActions} actions used`
        : `delegated session budget exhausted for ${sessionId}: ${elapsed.toFixed(3)}/${budget.maxSessionMinutes} minutes elapsed`,
    imagePaths: [],
    value: {
      session_id: sessionId,
      budget_exhausted: dimension,
      actions_used: budget.actionsUsed,
      max_actions: budget.maxActions,
      elapsed_session_minutes: elapsed,
      max_session_minutes: budget.maxSessionMinutes,
    },
  };
}

function elapsedMinutes(budget: ActivePlaytestBudget, nowMs: number): number {
  return Math.max(0, nowMs - budget.startedAtMs) / 60_000;
}

function imagePathsFromResult(value: unknown): string[] {
  const indexPath = findStringField(value, "index_path");
  const root = indexPath ? dirname(indexPath) : undefined;
  const paths = new Set<string>();
  collectImagePathValues(value, paths);
  return [...paths].map((path) =>
    isAbsolute(path) || root === undefined ? path : resolve(root, path),
  );
}

function collectImagePathValues(value: unknown, paths: Set<string>): void {
  if (typeof value === "string") {
    if (/\.png$/i.test(value)) paths.add(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) collectImagePathValues(item, paths);
    return;
  }
  if (isRecord(value)) {
    for (const item of Object.values(value))
      collectImagePathValues(item, paths);
  }
}

function findStringField(value: unknown, field: string): string | undefined {
  if (isRecord(value) && typeof value[field] === "string") return value[field];
  if (isRecord(value)) {
    for (const nested of Object.values(value)) {
      const found = findStringField(nested, field);
      if (found) return found;
    }
  }
  return undefined;
}

function processError(error: unknown): {
  message: string;
  stdout?: string;
  stderr?: string;
} {
  if (!isRecord(error)) return { message: String(error) };
  return {
    message:
      typeof error.message === "string"
        ? error.message
        : "playtest command failed",
    stdout: typeof error.stdout === "string" ? error.stdout : undefined,
    stderr: typeof error.stderr === "string" ? error.stderr : undefined,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

export function visionPlaytestOutcome(
  value: string,
): VisionPlaytestOutcome | undefined {
  return value === "pass" ||
    value === "fail" ||
    value === "uncertain" ||
    value === "infrastructure_error"
    ? value
    : undefined;
}
