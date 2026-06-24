import type {
  NativeBridgeModule,
  NativeProfileMemoryCaps,
  NativeProfileMemoryRecord,
} from "@rusty-crew/native-bridge";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { SessionState } from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import type { BrainToolResolver } from "./tool-session-selection.js";

export type DenseProfileMemoryMode = "off" | "read_only" | "read_write";
export type DenseProfileMemoryAction =
  | "list"
  | "read"
  | "add"
  | "replace"
  | "remove";

export interface DenseProfileMemoryToolContext {
  client?: Pick<
    NativeBridgeModule,
    | "listProfileMemory"
    | "getProfileMemory"
    | "addProfileMemory"
    | "replaceProfileMemory"
    | "removeProfileMemory"
  >;
  mode: DenseProfileMemoryMode;
  session?: Pick<SessionState, "profileId">;
  profileId?: string;
  descriptionOverride?: string;
  caps?: NativeProfileMemoryCaps;
}

export interface DenseProfileMemoryToolDetails {
  ok: boolean;
  operation: DenseProfileMemoryAction;
  mode: DenseProfileMemoryMode;
  action: "read" | "written" | "removed" | "denied" | "failed";
  reasonCode?: string;
  result?: unknown;
}

const targetType = Type.Optional(
  Type.Union([Type.Literal("profile"), Type.Literal("user")]),
);

const capsSchema = Type.Optional(
  Type.Object({
    maxRecordsPerProfile: Type.Optional(Type.Number({ minimum: 1 })),
    maxKeyBytes: Type.Optional(Type.Number({ minimum: 1 })),
    maxContentBytes: Type.Optional(Type.Number({ minimum: 1 })),
  }),
);

const parameters = Type.Object({
  action: Type.Union([
    Type.Literal("list"),
    Type.Literal("read"),
    Type.Literal("add"),
    Type.Literal("replace"),
    Type.Literal("remove"),
  ]),
  profileId: Type.Optional(Type.String({ minLength: 1 })),
  targetType,
  targetId: Type.Optional(Type.String({ minLength: 1 })),
  key: Type.Optional(Type.String({ minLength: 1 })),
  content: Type.Optional(Type.String({ minLength: 1 })),
  metadata: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
  expectedRevision: Type.Optional(Type.Number({ minimum: 1 })),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  offset: Type.Optional(Type.Number({ minimum: 0 })),
  caps: capsSchema,
});

type Params = Static<typeof parameters>;

export function createDenseProfileMemoryToolResolver(
  context: DenseProfileMemoryToolContext,
): BrainToolResolver {
  return () => [denseProfileMemoryTool(context)];
}

export function denseProfileMemoryTool(
  context: DenseProfileMemoryToolContext,
): BrainTool<typeof parameters, DenseProfileMemoryToolDetails> {
  return {
    name: "dense_profile_memory",
    label: "Dense profile memory",
    description:
      context.descriptionOverride ??
      "Read or update compact stable profile memory. Do not store task progress, todos, temporary outcomes, or Den product facts here.",
    parameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: Params) => {
      if (context.mode === "off") {
        return denseResult(params.action, context, "denied", {
          ok: false,
          reasonCode: "dense_profile_memory_disabled",
        });
      }
      if (!context.client) {
        return denseResult(params.action, context, "failed", {
          ok: false,
          reasonCode: "dense_profile_memory_client_unavailable",
        });
      }

      const profileId =
        params.profileId ?? context.profileId ?? context.session?.profileId;
      if (!profileId) {
        return denseResult(params.action, context, "denied", {
          ok: false,
          reasonCode: "profile_id_missing",
        });
      }

      const targetType = params.targetType ?? "profile";
      if (targetType === "user" && !params.targetId) {
        return denseResult(params.action, context, "denied", {
          ok: false,
          reasonCode: "target_id_missing",
        });
      }

      const writeDenied =
        context.mode !== "read_write" &&
        ["add", "replace", "remove"].includes(params.action);
      if (writeDenied) {
        return denseResult(params.action, context, "denied", {
          ok: false,
          reasonCode: "dense_profile_memory_read_only",
        });
      }

      try {
        switch (params.action) {
          case "list": {
            const result = await context.client.listProfileMemory({
              profileId,
              targetType: params.targetType,
              targetId: params.targetId,
              limit: params.limit,
              offset: params.offset,
            });
            return denseResult("list", context, "read", {
              ok: true,
              result: result.map(normalizeRecord),
            });
          }
          case "read": {
            const key = required(params.key, "memory_key_missing");
            const result = await context.client.getProfileMemory({
              profileId,
              targetType,
              targetId: params.targetId,
              key,
            });
            return denseResult("read", context, "read", {
              ok: true,
              result: result ? normalizeRecord(result) : undefined,
            });
          }
          case "add": {
            const key = required(params.key, "memory_key_missing");
            const content = required(params.content, "memory_content_missing");
            const result = await context.client.addProfileMemory({
              profileId,
              targetType,
              targetId: params.targetId,
              key,
              content,
              metadataJson: JSON.stringify(params.metadata ?? {}),
              caps: params.caps ?? context.caps,
            });
            return denseResult("add", context, "written", {
              ok: true,
              result: normalizeRecord(result),
            });
          }
          case "replace": {
            const key = required(params.key, "memory_key_missing");
            const content = required(params.content, "memory_content_missing");
            const expectedRevision = requiredRevision(params.expectedRevision);
            const result = await context.client.replaceProfileMemory({
              expectedRevision,
              write: {
                profileId,
                targetType,
                targetId: params.targetId,
                key,
                content,
                metadataJson: JSON.stringify(params.metadata ?? {}),
                caps: params.caps ?? context.caps,
              },
            });
            return denseResult("replace", context, "written", {
              ok: true,
              result: normalizeRecord(result),
            });
          }
          case "remove": {
            const key = required(params.key, "memory_key_missing");
            const expectedRevision = requiredRevision(params.expectedRevision);
            const result = await context.client.removeProfileMemory({
              profileId,
              targetType,
              targetId: params.targetId,
              key,
              expectedRevision,
            });
            return denseResult("remove", context, "removed", {
              ok: true,
              result: normalizeRecord(result),
            });
          }
        }
      } catch (error) {
        if (error instanceof DenseProfileMemoryInputError) {
          return denseResult(params.action, context, "denied", {
            ok: false,
            reasonCode: error.reasonCode,
          });
        }
        return denseResult(params.action, context, "failed", {
          ok: false,
          reasonCode: "dense_profile_memory_call_failed",
          result: error instanceof Error ? error.message : String(error),
        });
      }
    },
  };
}

function normalizeRecord(record: NativeProfileMemoryRecord): unknown {
  return {
    ...record,
    metadata: parseMetadata(record.metadataJson),
  };
}

function parseMetadata(metadataJson: string): unknown {
  try {
    return JSON.parse(metadataJson) as unknown;
  } catch {
    return {};
  }
}

function required(value: string | undefined, reasonCode: string): string {
  if (!value) throw new DenseProfileMemoryInputError(reasonCode);
  return value;
}

function requiredRevision(value: number | undefined): number {
  if (!value)
    throw new DenseProfileMemoryInputError("expected_revision_missing");
  return value;
}

class DenseProfileMemoryInputError extends Error {
  constructor(readonly reasonCode: string) {
    super(reasonCode);
    this.name = "DenseProfileMemoryInputError";
  }
}

function denseResult(
  operation: DenseProfileMemoryAction,
  context: DenseProfileMemoryToolContext,
  action: DenseProfileMemoryToolDetails["action"],
  details: {
    ok: boolean;
    reasonCode?: string;
    result?: unknown;
  },
): BrainToolResult<DenseProfileMemoryToolDetails> {
  const result = {
    ok: details.ok,
    operation,
    mode: context.mode,
    action,
    reasonCode: details.reasonCode,
    result: details.result,
  };
  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}
