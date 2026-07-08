import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export interface SceneStateToolContext {
  client?: Pick<
    NativeBridgeModule,
    | "listSimpleKv"
    | "putSimpleKv"
    | "readRoleplaySceneState"
    | "planRoleplaySceneStateUpdate"
  >;
  session?: {
    sessionId?: string;
  };
  now?: () => string;
}

export interface RoleplaySceneState {
  sessionId: string;
  location?: string;
  charactersPresent: string[];
  activeThreads: string[];
  notes?: string;
  updatedAt?: string;
}

export interface SceneStateToolDetails {
  ok: boolean;
  operation: "get_scene_state" | "update_scene_state";
  action: "read" | "written" | "denied" | "failed";
  reasonCode?: string;
  state?: RoleplaySceneState;
  revision?: number;
  result?: unknown;
}

interface RoleplaySceneStateReadOutput {
  state: RoleplaySceneState;
  revision?: number;
}

interface RoleplaySceneStateUpdatePlan {
  state: RoleplaySceneState;
  value_json: string;
  now: string;
}

const getSceneStateParameters = Type.Object({
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
});

const updateSceneStateParameters = Type.Object({
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
  location: Type.Optional(Type.Union([Type.String(), Type.Null()])),
  charactersPresent: Type.Optional(Type.Array(Type.String())),
  activeThreads: Type.Optional(Type.Array(Type.String())),
  notes: Type.Optional(Type.Union([Type.String(), Type.Null()])),
});

type GetSceneStateParams = Static<typeof getSceneStateParameters>;
type UpdateSceneStateParams = Static<typeof updateSceneStateParameters>;

const SCENE_STATE_SCOPE_TYPE = "roleplay_scene_state";
const SCENE_STATE_KEY = "current";

export function createSceneStateToolResolver(
  context: SceneStateToolContext,
): BrainToolResolver {
  return (input) =>
    resolveSceneStateTools({
      ...context,
      session: context.session ?? input.wake.state.session,
    });
}

export function resolveSceneStateTools(
  context: SceneStateToolContext,
): BrainTool[] {
  return [getSceneStateTool(context), updateSceneStateTool(context)];
}

export function getSceneStateTool(
  context: SceneStateToolContext,
): BrainTool<typeof getSceneStateParameters, SceneStateToolDetails> {
  return {
    name: "get_scene_state",
    label: "Get scene state",
    description:
      "Read lightweight roleplay scene state for the current session.",
    parameters: getSceneStateParameters,
    executionMode: "parallel",
    execute: async (_callId, params) =>
      runSceneStateTool("get_scene_state", context, "read", async (client) => {
        const sessionId = resolveSessionId(params.sessionId, context);
        const record = await readSceneStateRecord(client, sessionId, context);
        const fallback = (await client.readRoleplaySceneState({
          session_id: sessionId,
        })) as RoleplaySceneStateReadOutput;
        return {
          state: record?.state ?? fallback.state,
          revision: record?.revision,
        };
      }),
  };
}

export function updateSceneStateTool(
  context: SceneStateToolContext,
): BrainTool<typeof updateSceneStateParameters, SceneStateToolDetails> {
  return {
    name: "update_scene_state",
    label: "Update scene state",
    description:
      "Update lightweight roleplay scene state for the current session.",
    parameters: updateSceneStateParameters,
    executionMode: "sequential",
    execute: async (_callId, params) =>
      runSceneStateTool(
        "update_scene_state",
        context,
        "written",
        async (client) => {
          const sessionId = resolveSessionId(params.sessionId, context);
          const existing =
            (await readSceneStateRecord(client, sessionId, context))?.state ??
            (
              (await client.readRoleplaySceneState({
                session_id: sessionId,
              })) as RoleplaySceneStateReadOutput
            ).state;
          const plan = (await client.planRoleplaySceneStateUpdate({
            session_id: sessionId,
            current: existing,
            now: now(context),
            body: params,
          })) as RoleplaySceneStateUpdatePlan;
          const record = await client.putSimpleKv({
            scopeType: SCENE_STATE_SCOPE_TYPE,
            scopeId: sessionId,
            key: SCENE_STATE_KEY,
            valueJson: plan.value_json,
            now: plan.now,
          });
          return {
            state: plan.state,
            revision: record.revision,
          };
        },
      ),
  };
}

async function readSceneStateRecord(
  client: NonNullable<SceneStateToolContext["client"]>,
  sessionId: string,
  context: SceneStateToolContext,
): Promise<{ state: RoleplaySceneState; revision?: number } | undefined> {
  const [record] = await client.listSimpleKv({
    scopeType: SCENE_STATE_SCOPE_TYPE,
    scopeId: sessionId,
    keyPrefix: SCENE_STATE_KEY,
    now: now(context),
    limit: 1,
  });
  if (!record) return undefined;
  return (await client.readRoleplaySceneState({
    session_id: sessionId,
    record_value_json: record.valueJson,
    record_updated_at: record.updatedAt,
    revision: record.revision,
  })) as RoleplaySceneStateReadOutput;
}

async function runSceneStateTool(
  operation: SceneStateToolDetails["operation"],
  context: SceneStateToolContext,
  successAction: "read" | "written",
  callback: (
    client: NonNullable<SceneStateToolContext["client"]>,
  ) => Promise<{ state: RoleplaySceneState; revision?: number }>,
): Promise<BrainToolResult<SceneStateToolDetails>> {
  if (!context.client) {
    return sceneStateResult(operation, "failed", {
      ok: false,
      reasonCode: "scene_state_client_unavailable",
    });
  }
  try {
    const result = await callback(context.client);
    return sceneStateResult(operation, successAction, {
      ok: true,
      state: result.state,
      revision: result.revision,
    });
  } catch (error) {
    if (error instanceof SceneStateInputError) {
      return sceneStateResult(operation, "denied", {
        ok: false,
        reasonCode: error.reasonCode,
      });
    }
    const reasonCode = sceneStateDomainReasonCode(error);
    if (reasonCode !== undefined) {
      return sceneStateResult(operation, "denied", {
        ok: false,
        reasonCode,
        result: error instanceof Error ? error.message : String(error),
      });
    }
    return sceneStateResult(operation, "failed", {
      ok: false,
      reasonCode: "scene_state_call_failed",
      result: error instanceof Error ? error.message : String(error),
    });
  }
}

function sceneStateResult(
  operation: SceneStateToolDetails["operation"],
  action: SceneStateToolDetails["action"],
  details: {
    ok: boolean;
    reasonCode?: string;
    state?: RoleplaySceneState;
    revision?: number;
    result?: unknown;
  },
): BrainToolResult<SceneStateToolDetails> {
  const result = {
    ok: details.ok,
    operation,
    action,
    reasonCode: details.reasonCode,
    state: details.state,
    revision: details.revision,
    result: details.result,
  };
  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function resolveSessionId(
  requested: string | undefined,
  context: SceneStateToolContext,
): string {
  const sessionId = requested ?? context.session?.sessionId;
  if (!sessionId) {
    throw new SceneStateInputError("session_id_missing");
  }
  return sessionId;
}

function now(context: SceneStateToolContext): string {
  return context.now?.() ?? new Date().toISOString();
}

function sceneStateDomainReasonCode(error: unknown): string | undefined {
  const message = error instanceof Error ? error.message : String(error);
  const reasonCode = message.split(":", 1)[0]?.trim();
  return reasonCode?.startsWith("roleplay_") ? reasonCode : undefined;
}

class SceneStateInputError extends Error {
  constructor(readonly reasonCode: string) {
    super(reasonCode);
    this.name = "SceneStateInputError";
  }
}
