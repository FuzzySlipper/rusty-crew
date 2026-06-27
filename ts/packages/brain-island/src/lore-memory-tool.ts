import type {
  NativeBridgeModule,
  NativeLoreRecallResult,
  NativeRoleplayChatLayerRecord,
  NativeRoleplayLoreLayerEntryJoin,
  NativeRoleplayLoreRecord,
} from "@rusty-crew/native-bridge";
import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export type LoreMemoryToolOperation =
  | "recall_lore"
  | "capture_lore_fact"
  | "promote_lore_entry"
  | "search_lore"
  | "list_lore_layers"
  | "manage_lore_layers"
  | "get_lore_layer_config";

export interface LoreMemoryToolContext {
  client?: Pick<
    NativeBridgeModule,
    | "archiveLoreLayer"
    | "captureLoreFact"
    | "createLoreLayer"
    | "getChatLayers"
    | "getLoreLayerConfig"
    | "listEntriesByLayer"
    | "listLoreLayers"
    | "promoteLoreEntry"
    | "queryLoreEntries"
    | "recallLore"
    | "reorderChatLayers"
    | "toggleChatLayer"
    | "updateLoreLayer"
  >;
  session?: {
    profileId?: string;
    sessionId?: string;
  };
  profileId?: string;
  now?: () => string;
}

export interface LoreMemoryToolDetails {
  ok: boolean;
  operation: LoreMemoryToolOperation;
  action: "read" | "written" | "denied" | "failed";
  reasonCode?: string;
  result?: unknown;
}

const pageSchema = Type.Object({
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  offset: Type.Optional(Type.Number({ minimum: 0 })),
});

const evidenceRefSchema = Type.Object({
  evidenceType: Type.Union([
    Type.Literal("wake"),
    Type.Literal("event"),
    Type.Literal("tool_call"),
    Type.Literal("transcript"),
    Type.Literal("user_correction"),
    Type.Literal("source_document"),
    Type.Literal("den_memory"),
    Type.Literal("import"),
    Type.Literal("migration"),
    Type.Literal("ui"),
    Type.Literal("other"),
  ]),
  refId: Type.String({ minLength: 1 }),
  label: Type.Optional(Type.String()),
});

const canonStatusSchema = Type.Union([
  Type.Literal("canon"),
  Type.Literal("draft"),
  Type.Literal("contested"),
  Type.Literal("deprecated"),
]);

const visibilitySchema = Type.Union([
  Type.Literal("public"),
  Type.Literal("private"),
  Type.Literal("gm_only"),
  Type.Literal("tool_only"),
]);

const layerPurposeSchema = Type.Union([
  Type.Literal("world"),
  Type.Literal("story"),
  Type.Literal("characters"),
  Type.Literal("factions"),
  Type.Literal("mixed"),
]);

const layerWritePolicySchema = Type.Union([
  Type.Literal("manual"),
  Type.Literal("auto_capture"),
  Type.Literal("readonly"),
]);

const loreContentSchema = Type.Record(Type.String(), Type.Unknown());

const recallLoreParameters = Type.Object({
  chatId: Type.String({ minLength: 1 }),
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
  queryText: Type.Optional(Type.String()),
  activeSubjects: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  excludedSubjects: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  tokenBudget: Type.Optional(Type.Number({ minimum: 1 })),
  traceId: Type.Optional(Type.String({ minLength: 1 })),
  recordTrace: Type.Optional(Type.Boolean()),
  now: Type.Optional(Type.String({ minLength: 1 })),
});

const captureLoreFactParameters = Type.Object({
  layerId: Type.String({ minLength: 1 }),
  recordId: Type.String({ minLength: 1 }),
  worldId: Type.String({ minLength: 1 }),
  entityId: Type.Optional(Type.String({ minLength: 1 })),
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
  branchId: Type.Optional(Type.String({ minLength: 1 })),
  shapeId: Type.String({ minLength: 1 }),
  shapeVersion: Type.Optional(Type.Number({ minimum: 1 })),
  canonStatus: Type.Optional(canonStatusSchema),
  visibility: Type.Optional(visibilitySchema),
  title: Type.String({ minLength: 1 }),
  body: Type.String({ minLength: 1 }),
  content: Type.Optional(loreContentSchema),
  evidenceRefs: Type.Optional(Type.Array(evidenceRefSchema)),
  confidence: Type.Optional(Type.Number({ minimum: 0, maximum: 1 })),
  durabilityRationale: Type.Optional(Type.String({ minLength: 1 })),
  isConstant: Type.Optional(Type.Boolean()),
  priority: Type.Optional(Type.Number()),
  captureReason: Type.Optional(Type.String()),
  now: Type.Optional(Type.String({ minLength: 1 })),
});

const promoteLoreEntryParameters = Type.Object({
  sourceLayerId: Type.String({ minLength: 1 }),
  sourceRecordId: Type.String({ minLength: 1 }),
  targetLayerId: Type.String({ minLength: 1 }),
  newRecordId: Type.String({ minLength: 1 }),
  isConstant: Type.Optional(Type.Boolean()),
  priority: Type.Optional(Type.Number()),
  now: Type.Optional(Type.String({ minLength: 1 })),
});

const searchLoreParameters = Type.Object({
  query: Type.String({ minLength: 1 }),
  worldId: Type.Optional(Type.String({ minLength: 1 })),
  chatId: Type.Optional(Type.String({ minLength: 1 })),
  layerIds: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  entityId: Type.Optional(Type.String({ minLength: 1 })),
  canonStatus: Type.Optional(canonStatusSchema),
  visibility: Type.Optional(visibilitySchema),
  shapeId: Type.Optional(Type.String({ minLength: 1 })),
  includeSuperseded: Type.Optional(Type.Boolean()),
  includeTombstoned: Type.Optional(Type.Boolean()),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  offset: Type.Optional(Type.Number({ minimum: 0 })),
});

const listLoreLayersParameters = Type.Object({
  profileId: Type.Optional(Type.String({ minLength: 1 })),
});

const manageLoreLayersParameters = Type.Object({
  action: Type.Union([
    Type.Literal("create"),
    Type.Literal("update"),
    Type.Literal("archive"),
    Type.Literal("toggle"),
    Type.Literal("reorder"),
  ]),
  profileId: Type.Optional(Type.String({ minLength: 1 })),
  layerId: Type.Optional(Type.String({ minLength: 1 })),
  name: Type.Optional(Type.String({ minLength: 1 })),
  description: Type.Optional(Type.Union([Type.String(), Type.Null()])),
  purpose: Type.Optional(layerPurposeSchema),
  writePolicy: Type.Optional(layerWritePolicySchema),
  chatId: Type.Optional(Type.String({ minLength: 1 })),
  enabled: Type.Optional(Type.Boolean()),
  layerIds: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  now: Type.Optional(Type.String({ minLength: 1 })),
});

const getLoreLayerConfigParameters = Type.Object({
  layerId: Type.String({ minLength: 1 }),
});

type RecallLoreParams = Static<typeof recallLoreParameters>;
type CaptureLoreFactParams = Static<typeof captureLoreFactParameters>;
type PromoteLoreEntryParams = Static<typeof promoteLoreEntryParameters>;
type SearchLoreParams = Static<typeof searchLoreParameters>;
type ListLoreLayersParams = Static<typeof listLoreLayersParameters>;
type ManageLoreLayersParams = Static<typeof manageLoreLayersParameters>;
type GetLoreLayerConfigParams = Static<typeof getLoreLayerConfigParameters>;

export function createLoreMemoryToolResolver(
  context: LoreMemoryToolContext,
): BrainToolResolver {
  return (input) =>
    resolveLoreMemoryTools({
      ...context,
      session: context.session ?? input.wake.state.session,
    });
}

export function resolveLoreMemoryTools(
  context: LoreMemoryToolContext,
): BrainTool[] {
  return [
    recallLoreTool(context),
    captureLoreFactTool(context),
    promoteLoreEntryTool(context),
    searchLoreTool(context),
    listLoreLayersTool(context),
    manageLoreLayersTool(context),
    getLoreLayerConfigTool(context),
  ];
}

export function recallLoreTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof recallLoreParameters, LoreMemoryToolDetails> {
  return {
    name: "recall_lore",
    label: "Recall lore",
    description:
      "Recall scored roleplay lore from the active layers for a chat.",
    parameters: recallLoreParameters,
    executionMode: "parallel",
    execute: async (_callId, params) =>
      runLoreTool("recall_lore", context, "read", async (client) =>
        client.recallLore({
          chat_id: params.chatId,
          session_id: params.sessionId ?? context.session?.sessionId,
          query_text: params.queryText,
          active_subjects: params.activeSubjects ?? [],
          excluded_subjects: params.excludedSubjects ?? [],
          token_budget: params.tokenBudget,
          trace_id: params.traceId,
          record_trace: params.recordTrace ?? false,
          now: params.now ?? now(context),
        }),
      ),
  };
}

export function captureLoreFactTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof captureLoreFactParameters, LoreMemoryToolDetails> {
  return {
    name: "capture_lore_fact",
    label: "Capture lore fact",
    description:
      "Record a roleplay fact into an auto-capture lore layer for later recall or promotion.",
    parameters: captureLoreFactParameters,
    executionMode: "sequential",
    execute: async (_callId, params) =>
      runLoreTool("capture_lore_fact", context, "written", async (client) =>
        client.captureLoreFact({
          layer_id: params.layerId,
          write: loreWriteFromParams(params, "capture_producer", context),
          is_constant: params.isConstant ?? false,
          priority: params.priority ?? 0,
          capture_reason: params.captureReason,
        }),
      ),
  };
}

export function promoteLoreEntryTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof promoteLoreEntryParameters, LoreMemoryToolDetails> {
  return {
    name: "promote_lore_entry",
    label: "Promote lore entry",
    description:
      "Promote an existing captured lore entry into another lore layer.",
    parameters: promoteLoreEntryParameters,
    executionMode: "sequential",
    execute: async (_callId, params) =>
      runLoreTool("promote_lore_entry", context, "written", async (client) =>
        client.promoteLoreEntry({
          source_layer_id: params.sourceLayerId,
          source_record_id: params.sourceRecordId,
          target_layer_id: params.targetLayerId,
          new_record_id: params.newRecordId,
          is_constant: params.isConstant ?? false,
          priority: params.priority ?? 0,
          now: params.now ?? now(context),
        }),
      ),
  };
}

export function searchLoreTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof searchLoreParameters, LoreMemoryToolDetails> {
  return {
    name: "search_lore",
    label: "Search lore",
    description:
      "Search roleplay lore entries and optionally narrow results to active chat layers or explicit layers.",
    parameters: searchLoreParameters,
    executionMode: "parallel",
    execute: async (_callId, params) =>
      runLoreTool("search_lore", context, "read", async (client) => {
        const records = (await client.queryLoreEntries({
          world_id: params.worldId,
          entity_id: params.entityId,
          canon_status: params.canonStatus,
          visibility: params.visibility,
          shape_id: params.shapeId,
          query: params.query,
          include_superseded: params.includeSuperseded ?? false,
          include_tombstoned: params.includeTombstoned ?? false,
          page: page(params),
        })) as NativeRoleplayLoreRecord[];
        const layerIds = await resolveSearchLayerIds(client, params);
        if (!layerIds) return { records };
        const recordIds = await recordIdsForLayers(client, layerIds);
        return {
          layerIds,
          records: records.filter((record) =>
            recordIds.has(String(record.record_id)),
          ),
        };
      }),
  };
}

export function listLoreLayersTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof listLoreLayersParameters, LoreMemoryToolDetails> {
  return {
    name: "list_lore_layers",
    label: "List lore layers",
    description: "List roleplay lore layers available to a profile.",
    parameters: listLoreLayersParameters,
    executionMode: "parallel",
    execute: async (_callId, params) =>
      runLoreTool("list_lore_layers", context, "read", async (client) =>
        client.listLoreLayers(profileId(params.profileId, context)),
      ),
  };
}

export function manageLoreLayersTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof manageLoreLayersParameters, LoreMemoryToolDetails> {
  return {
    name: "manage_lore_layers",
    label: "Manage lore layers",
    description:
      "Create, update, archive, toggle, or reorder roleplay lore layers.",
    parameters: manageLoreLayersParameters,
    executionMode: "sequential",
    execute: async (_callId, params) =>
      runLoreTool("manage_lore_layers", context, "written", async (client) => {
        switch (params.action) {
          case "create":
            return client.createLoreLayer({
              layer_id: required(params.layerId, "layer_id_missing"),
              profile_id: profileId(params.profileId, context),
              name: required(params.name, "layer_name_missing"),
              description: params.description ?? undefined,
              purpose: params.purpose ?? "mixed",
              write_policy: params.writePolicy ?? "manual",
              now: params.now ?? now(context),
            });
          case "update":
            return client.updateLoreLayer({
              layer_id: required(params.layerId, "layer_id_missing"),
              name: params.name,
              description:
                params.description === undefined
                  ? undefined
                  : params.description,
              purpose: params.purpose,
              write_policy: params.writePolicy,
              now: params.now ?? now(context),
            });
          case "archive":
            return client.archiveLoreLayer({
              layer_id: required(params.layerId, "layer_id_missing"),
              now: params.now ?? now(context),
            });
          case "toggle":
            await client.toggleChatLayer({
              chatId: required(params.chatId, "chat_id_missing"),
              layerId: required(params.layerId, "layer_id_missing"),
              enabled: requiredBoolean(params.enabled, "enabled_missing"),
            });
            return {
              chatId: params.chatId,
              layerId: params.layerId,
              enabled: params.enabled,
            };
          case "reorder":
            await client.reorderChatLayers({
              chatId: required(params.chatId, "chat_id_missing"),
              layerIds: requiredNonEmptyList(
                params.layerIds,
                "layer_ids_missing",
              ),
            });
            return { chatId: params.chatId, layerIds: params.layerIds };
        }
      }),
  };
}

export function getLoreLayerConfigTool(
  context: LoreMemoryToolContext,
): BrainTool<typeof getLoreLayerConfigParameters, LoreMemoryToolDetails> {
  return {
    name: "get_lore_layer_config",
    label: "Get lore layer config",
    description: "Read retrieval and capture settings for a lore layer.",
    parameters: getLoreLayerConfigParameters,
    executionMode: "parallel",
    execute: async (_callId, params) =>
      runLoreTool("get_lore_layer_config", context, "read", async (client) =>
        client.getLoreLayerConfig(params.layerId),
      ),
  };
}

async function resolveSearchLayerIds(
  client: NonNullable<LoreMemoryToolContext["client"]>,
  params: SearchLoreParams,
): Promise<string[] | undefined> {
  if (params.layerIds?.length) return params.layerIds;
  if (!params.chatId) return undefined;
  const layers = (await client.getChatLayers(
    params.chatId,
  )) as NativeRoleplayChatLayerRecord[];
  return layers
    .filter((layer) => layer.enabled !== false)
    .map((layer) => String(layer.layer_id));
}

async function recordIdsForLayers(
  client: NonNullable<LoreMemoryToolContext["client"]>,
  layerIds: readonly string[],
): Promise<Set<string>> {
  const records = await Promise.all(
    layerIds.map(
      (layerId) =>
        client.listEntriesByLayer(layerId) as Promise<
          NativeRoleplayLoreLayerEntryJoin[]
        >,
    ),
  );
  return new Set(
    records
      .flat()
      .map((entry) =>
        String(
          entry.record_id ??
            (isObject(entry.record) ? entry.record.record_id : undefined),
        ),
      ),
  );
}

function loreWriteFromParams(
  params: CaptureLoreFactParams,
  source: "capture_producer" | "in_wake_tool" | "ui",
  context: LoreMemoryToolContext,
): Record<string, unknown> {
  return {
    record_id: params.recordId,
    world_id: params.worldId,
    entity_id: params.entityId,
    session_id: params.sessionId ?? context.session?.sessionId,
    branch_id: params.branchId,
    shape: {
      shape_id: params.shapeId,
      version: params.shapeVersion ?? 1,
    },
    canon_status: params.canonStatus ?? "draft",
    visibility: params.visibility ?? "tool_only",
    title: params.title,
    body: params.body,
    content: params.content ?? {},
    evidence_refs: (params.evidenceRefs ?? []).map((ref) => ({
      evidence_type: ref.evidenceType,
      ref_id: ref.refId,
      label: ref.label,
    })),
    source,
    confidence: params.confidence ?? 0.7,
    durability_rationale:
      params.durabilityRationale ?? "Captured through roleplay lore tool.",
    supersedes_record_id: undefined,
    now: params.now ?? now(context),
  };
}

function page(params: { limit?: number; offset?: number }): unknown {
  if (params.limit === undefined && params.offset === undefined) {
    return undefined;
  }
  return {
    limit: params.limit,
    offset: params.offset,
  };
}

function profileId(
  requested: string | undefined,
  context: LoreMemoryToolContext,
): string {
  return required(
    requested ?? context.profileId ?? context.session?.profileId,
    "profile_id_missing",
  );
}

function now(context: LoreMemoryToolContext): string {
  return context.now?.() ?? new Date().toISOString();
}

async function runLoreTool(
  operation: LoreMemoryToolOperation,
  context: LoreMemoryToolContext,
  successAction: "read" | "written",
  callback: (
    client: NonNullable<LoreMemoryToolContext["client"]>,
  ) => Promise<unknown>,
): Promise<BrainToolResult<LoreMemoryToolDetails>> {
  if (!context.client) {
    return loreResult(operation, "failed", {
      ok: false,
      reasonCode: "lore_bridge_client_unavailable",
    });
  }
  try {
    const result = await callback(context.client);
    return loreResult(operation, successAction, {
      ok: true,
      result: normalizeResult(result),
    });
  } catch (error) {
    if (error instanceof LoreMemoryInputError) {
      return loreResult(operation, "denied", {
        ok: false,
        reasonCode: error.reasonCode,
      });
    }
    return loreResult(operation, "failed", {
      ok: false,
      reasonCode: "lore_tool_call_failed",
      result: error instanceof Error ? error.message : String(error),
    });
  }
}

function loreResult(
  operation: LoreMemoryToolOperation,
  action: LoreMemoryToolDetails["action"],
  details: {
    ok: boolean;
    reasonCode?: string;
    result?: unknown;
  },
): BrainToolResult<LoreMemoryToolDetails> {
  const result = {
    ok: details.ok,
    operation,
    action,
    reasonCode: details.reasonCode,
    result: details.result,
  };
  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function normalizeResult(result: unknown): unknown {
  if (isRecallResult(result)) {
    return {
      ...result,
      entries: result.entries.map((entry) => ({
        ...entry,
        record: normalizeLoreRecord(entry.record),
      })),
    };
  }
  if (Array.isArray(result)) return result.map(normalizeResult);
  if (isLayerEntryJoin(result)) {
    return { ...result, record: normalizeLoreRecord(result.record) };
  }
  if (isLoreRecord(result)) return normalizeLoreRecord(result);
  return result;
}

function normalizeLoreRecord(record: NativeRoleplayLoreRecord): unknown {
  return record;
}

function isLoreRecord(value: unknown): value is NativeRoleplayLoreRecord {
  return isObject(value) && typeof value.record_id === "string";
}

function isLayerEntryJoin(
  value: unknown,
): value is NativeRoleplayLoreLayerEntryJoin & {
  record: NativeRoleplayLoreRecord;
} {
  return isObject(value) && isObject(value.record);
}

function isRecallResult(value: unknown): value is NativeLoreRecallResult & {
  entries: Array<{ record: NativeRoleplayLoreRecord }>;
} {
  return isObject(value) && Array.isArray(value.entries);
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function required(value: string | undefined, reasonCode: string): string {
  if (!value) throw new LoreMemoryInputError(reasonCode);
  return value;
}

function requiredBoolean(
  value: boolean | undefined,
  reasonCode: string,
): boolean {
  if (value === undefined) throw new LoreMemoryInputError(reasonCode);
  return value;
}

function requiredNonEmptyList(
  value: string[] | undefined,
  reasonCode: string,
): string[] {
  if (!value?.length) throw new LoreMemoryInputError(reasonCode);
  return value;
}

class LoreMemoryInputError extends Error {
  constructor(readonly reasonCode: string) {
    super(reasonCode);
    this.name = "LoreMemoryInputError";
  }
}
