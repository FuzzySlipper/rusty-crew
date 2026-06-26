import type {
  NativeBridgeModule,
  NativeProfileMemoryQuery,
  NativeRuntimeCounterQuery,
  NativeRuntimeSearchQuery,
  NativeRuntimeModuleSchemaRegistryDiagnostics,
  NativeSimpleKvQuery,
  NativeRuntimeStorageDiagnostics,
} from "@rusty-crew/native-bridge";
import { Type, type Static } from "typebox";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";

export type StorageQueryId =
  | "conversation.branches"
  | "profile.memory"
  | "runtime.counters"
  | "runtime.search"
  | "simple_kv.entries"
  | "storage.schema"
  | "storage.table_counts";

export interface StorageQueryParameter {
  name: string;
  type: "boolean" | "enum" | "integer" | "string";
  required: boolean;
  description: string;
  enumValues?: readonly string[];
  defaultValue?: unknown;
}

export interface StorageQueryDescriptor {
  id: StorageQueryId;
  title: string;
  description: string;
  owner: "rust_coordination";
  readOnly: true;
  backendAgnostic: true;
  resultShape: string;
  parameters: readonly StorageQueryParameter[];
  module?: StorageQueryModuleMetadata;
}

export interface StorageQueryModuleMetadata {
  moduleId: string;
  schemaVersion: number;
  logicalStore: string;
  ownerCrate: string;
  ownerModule: string;
}

export interface StorageQueryCatalog {
  schema_version: 1;
  source: "rust_bridge_read_model";
  items: StorageQueryDescriptor[];
  total: number;
}

export interface StorageQueryRouteRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId?: string;
}

export interface StorageQueryContext {
  bridge: Pick<
    NativeBridgeModule,
    | "listProfileMemory"
    | "queryConversationBranches"
    | "queryRuntimeCounters"
    | "searchRuntime"
    | "listSimpleKv"
    | "storageSchema"
    | "storageDiagnostics"
  >;
}

export interface StorageQueryResult<TItem = unknown, TData = unknown> {
  query_id: StorageQueryId;
  read_only: true;
  source: "rust_bridge_read_model";
  items?: TItem[];
  total?: number;
  data?: TData;
  limit?: number;
  offset?: number;
  nextOffset?: number;
}

export type StorageQueryExecuteToolDetails =
  | StorageQueryResult
  | {
      ok: false;
      reason_code: "unknown_storage_query_id";
      message: string;
    };

const MAX_LIMIT = 100;

const STORAGE_QUERY_DESCRIPTORS = [
  {
    id: "simple_kv.entries",
    title: "Simple KV entries",
    description:
      "List simple_kv module entries by scope, key prefix, and expiry status through the Rust repository.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "module.simple_kv.entry.v1",
    module: {
      moduleId: "simple_kv",
      schemaVersion: 1,
      logicalStore: "entries",
      ownerCrate: "core_persistence",
      ownerModule: "simple_kv",
    },
    parameters: [
      parameter("scopeType", "string", true, "Simple KV scope type."),
      parameter("scopeId", "string", true, "Simple KV scope id."),
      parameter("keyPrefix", "string", false, "Optional key prefix filter."),
      enumParameter("expiryStatus", false, "Expiry filter.", [
        "active",
        "expired",
        "all",
      ]),
      parameter("now", "string", false, "ISO timestamp used for expiry checks."),
      parameter("limit", "integer", false, "Maximum rows to return.", 25),
      parameter("offset", "integer", false, "Rows to skip.", 0),
    ],
  },
  {
    id: "storage.schema",
    title: "Storage module schema registry",
    description:
      "Read registered module schemas, installed versions, backend capability status, and physical ownership declarations.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "storage.module_schema_registry.v1",
    parameters: [],
  },
  {
    id: "storage.table_counts",
    title: "Storage table row counts",
    description:
      "List Rust-owned storage table row counts and backend capacity metadata.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "storage.table_counts.v1",
    parameters: [
      parameter("table", "string", false, "Optional exact table name filter."),
      parameter("limit", "integer", false, "Maximum rows to return.", 100),
      parameter("offset", "integer", false, "Rows to skip.", 0),
    ],
  },
  {
    id: "runtime.search",
    title: "Runtime text search",
    description:
      "Search sessions, agent messages, and queued messages through Rust-owned indexed runtime search.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "runtime.search_result.v1",
    parameters: [
      parameter("query", "string", true, "Search text."),
      enumParameter("rowType", false, "Optional row type filter.", [
        "message",
        "queue_message",
        "session",
      ]),
      parameter("sessionId", "string", false, "Optional session id filter."),
      parameter("agentId", "string", false, "Optional agent id filter."),
      parameter("eventKind", "string", false, "Optional event kind filter."),
      parameter("recordedAfter", "string", false, "Optional ISO lower bound."),
      parameter("recordedBefore", "string", false, "Optional ISO upper bound."),
      parameter("limit", "integer", false, "Maximum rows to return.", 25),
    ],
  },
  {
    id: "profile.memory",
    title: "Profile memory records",
    description:
      "List dense profile memory records by profile, profile target, or user target.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "runtime.profile_memory_record.v1",
    parameters: [
      parameter("profileId", "string", true, "Profile id."),
      enumParameter("targetType", false, "Optional memory target type.", [
        "profile",
        "user",
      ]),
      parameter("targetId", "string", false, "Required for user targets."),
      parameter("limit", "integer", false, "Maximum rows to return.", 25),
      parameter("offset", "integer", false, "Rows to skip.", 0),
    ],
  },
  {
    id: "conversation.branches",
    title: "Conversation branches",
    description:
      "List transcript branch records, optionally scoped to one runtime session.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "runtime.conversation_branch_record.v1",
    parameters: [
      parameter("sessionId", "string", false, "Optional session id filter."),
      parameter(
        "parentBranchId",
        "string",
        false,
        "Optional parent branch id filter.",
      ),
      parameter("limit", "integer", false, "Maximum rows to return.", 25),
      parameter("offset", "integer", false, "Rows to skip.", 0),
    ],
  },
  {
    id: "runtime.counters",
    title: "Runtime counters",
    description:
      "List derived runtime counters by runtime, agent, instance, or session scope.",
    owner: "rust_coordination",
    readOnly: true,
    backendAgnostic: true,
    resultShape: "runtime.counter_record.v1",
    parameters: [
      enumParameter("scopeType", false, "Optional counter scope type.", [
        "runtime",
        "agent",
        "instance",
        "session",
      ]),
      parameter("scopeId", "string", false, "Optional scope id filter."),
      parameter("counterName", "string", false, "Optional counter name."),
      parameter("limit", "integer", false, "Maximum rows to return.", 25),
      parameter("offset", "integer", false, "Rows to skip.", 0),
    ],
  },
] as const satisfies readonly StorageQueryDescriptor[];

export function storageQueryCatalog(): StorageQueryCatalog {
  return {
    schema_version: 1,
    source: "rust_bridge_read_model",
    items: STORAGE_QUERY_DESCRIPTORS.map((descriptor) => ({
      ...descriptor,
      parameters: descriptor.parameters.map((item) => ({ ...item })),
    })),
    total: STORAGE_QUERY_DESCRIPTORS.length,
  };
}

export async function handleStorageQueryRequest(
  request: StorageQueryRouteRequest,
  context: StorageQueryContext,
): Promise<AdminRouteResult> {
  const requestId = request.requestId ?? "storage-query";
  const url = new URL(request.url, "http://rusty-crew.local");
  const method = request.method.toUpperCase();

  if (url.pathname === "/v1/admin/storage/query-catalog") {
    if (method !== "GET") {
      return routeFailure(405, requestId, {
        reason_code: "storage_query_catalog_read_only",
        message: "storage query catalog only supports GET",
      });
    }
    return routeSuccess(requestId, storageQueryCatalog());
  }

  if (url.pathname === "/v1/admin/storage/schema") {
    if (method !== "GET") {
      return routeFailure(405, requestId, {
        reason_code: "storage_schema_read_only",
        message: "storage schema diagnostics only support GET",
      });
    }
    return routeSuccess(requestId, await context.bridge.storageSchema());
  }

  const queryId = decodeStorageQueryId(url.pathname);
  if (!queryId) {
    return routeFailure(404, requestId, {
      reason_code: "unknown_storage_query_route",
      message: `unknown storage query route ${url.pathname}`,
    });
  }
  if (method !== "POST") {
    return routeFailure(405, requestId, {
      reason_code: "storage_query_execute_requires_post",
      message: "storage query execution requires POST",
    });
  }

  try {
    return routeSuccess(
      requestId,
      await executeStorageQuery(queryId, request.body, context),
    );
  } catch (error) {
    return routeFailure(400, requestId, {
      reason_code:
        error instanceof StorageQueryInputError
          ? error.reasonCode
          : "invalid_storage_query_input",
      message:
        error instanceof Error ? error.message : "invalid storage query input",
    });
  }
}

export async function executeStorageQuery(
  queryId: StorageQueryId,
  input: unknown,
  context: StorageQueryContext,
): Promise<StorageQueryResult> {
  const body = recordBody(input);
  switch (queryId) {
    case "storage.table_counts":
      return storageTableCounts(
        body,
        await context.bridge.storageDiagnostics(),
      );
    case "storage.schema":
      return storageSchema(await context.bridge.storageSchema());
    case "simple_kv.entries":
      return simpleKvEntries(body, context);
    case "runtime.search":
      return runtimeSearch(body, context);
    case "profile.memory":
      return profileMemory(body, context);
    case "conversation.branches":
      return conversationBranches(body, context);
    case "runtime.counters":
      return runtimeCounters(body, context);
  }
}

const catalogToolParameters = Type.Object({});
type CatalogToolParams = Static<typeof catalogToolParameters>;

export function storageQueryCatalogTool(): BrainTool<
  typeof catalogToolParameters,
  StorageQueryCatalog
> {
  return {
    name: "storage_query_catalog",
    label: "Storage query catalog",
    description:
      "List curated read-only Rusty Crew storage queries. This tool never accepts raw SQL.",
    parameters: catalogToolParameters,
    execute: async (_toolCallId, _params: CatalogToolParams) =>
      toolResult(storageQueryCatalog()),
  };
}

const executeToolParameters = Type.Object({
  queryId: Type.String({ minLength: 1 }),
  input: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
});
type ExecuteToolParams = Static<typeof executeToolParameters>;

export function storageQueryExecuteTool(
  context: StorageQueryContext,
): BrainTool<typeof executeToolParameters, StorageQueryExecuteToolDetails> {
  return {
    name: "storage_query_execute",
    label: "Run storage query",
    description:
      "Execute one curated read-only Rusty Crew storage query by id. Raw SQL is not supported.",
    parameters: executeToolParameters,
    execute: async (_toolCallId, params: ExecuteToolParams) => {
      const queryId = parseStorageQueryId(params.queryId);
      if (!queryId) {
        return toolResult({
          ok: false,
          reason_code: "unknown_storage_query_id",
          message: `unknown storage query id ${params.queryId}`,
        });
      }
      return toolResult(
        await executeStorageQuery(queryId, params.input, context),
      );
    },
  };
}

function storageSchema(
  diagnostics: NativeRuntimeModuleSchemaRegistryDiagnostics,
): StorageQueryResult<never, NativeRuntimeModuleSchemaRegistryDiagnostics> {
  return {
    query_id: "storage.schema",
    read_only: true,
    source: "rust_bridge_read_model",
    data: diagnostics,
  };
}

function storageTableCounts(
  body: Record<string, unknown>,
  diagnostics: NativeRuntimeStorageDiagnostics,
): StorageQueryResult {
  const filter = optionalString(body, "table");
  if (filter !== undefined && !/^[a-zA-Z0-9_]+$/.test(filter)) {
    throw new StorageQueryInputError(
      "invalid_table_filter",
      "table must contain only letters, numbers, and underscores",
    );
  }
  const { limit, offset } = pageInput(body);
  const rows = diagnostics.tableCounts.filter((row) =>
    filter ? row.table === filter : true,
  );
  return pageResult("storage.table_counts", rows, limit, offset, {
    data: {
      backend: diagnostics.backend,
      backendLabel: diagnostics.backendLabel,
      pressure: diagnostics.pressure,
      size: diagnostics.size,
    },
  });
}

async function simpleKvEntries(
  body: Record<string, unknown>,
  context: StorageQueryContext,
): Promise<StorageQueryResult> {
  const scopeType = boundedString(body, "scopeType", 64, true);
  const scopeId = boundedString(body, "scopeId", 256, true);
  const keyPrefix = boundedString(body, "keyPrefix", 256, false);
  const expiryStatus =
    optionalEnum(body, "expiryStatus", ["active", "expired", "all"] as const) ??
    "active";
  const now = boundedString(body, "now", 64, false);
  if (expiryStatus === "expired" && now === undefined) {
    throw new StorageQueryInputError(
      "now_required_for_expired_entries",
      "now is required when expiryStatus is expired",
    );
  }
  const { limit, offset } = pageInput(body, 25);
  const query: NativeSimpleKvQuery = compactRecord({
    scopeType,
    scopeId,
    keyPrefix,
    includeExpired: expiryStatus === "all" || expiryStatus === "expired",
    expiredOnly: expiryStatus === "expired",
    now,
    limit,
    offset,
  }) as unknown as NativeSimpleKvQuery;
  const items = await context.bridge.listSimpleKv(query);
  return {
    query_id: "simple_kv.entries",
    read_only: true,
    source: "rust_bridge_read_model",
    items,
    total: items.length,
    limit,
    offset,
    data: {
      module: {
        moduleId: "simple_kv",
        logicalStore: "entries",
        schemaVersion: 1,
      },
      scopeType,
      scopeId,
      expiryStatus,
    },
  };
}

async function runtimeSearch(
  body: Record<string, unknown>,
  context: StorageQueryContext,
): Promise<StorageQueryResult> {
  const query = requiredString(body, "query");
  const limit = integer(body, "limit", 25, 1, MAX_LIMIT);
  const rowType = optionalEnum(body, "rowType", [
    "message",
    "queue_message",
    "session",
  ] as const);
  const request: NativeRuntimeSearchQuery = compactRecord({
    query,
    rowType,
    sessionId: optionalString(body, "sessionId"),
    agentId: optionalString(body, "agentId"),
    instanceId: optionalString(body, "instanceId"),
    taskId: optionalString(body, "taskId"),
    eventKind: optionalString(body, "eventKind"),
    recordedAfter: optionalString(body, "recordedAfter"),
    recordedBefore: optionalString(body, "recordedBefore"),
    limit,
  }) as unknown as NativeRuntimeSearchQuery;
  const items = await context.bridge.searchRuntime(request);
  return {
    query_id: "runtime.search",
    read_only: true,
    source: "rust_bridge_read_model",
    items,
    total: items.length,
    limit,
    offset: 0,
  };
}

async function profileMemory(
  body: Record<string, unknown>,
  context: StorageQueryContext,
): Promise<StorageQueryResult> {
  const profileId = requiredString(body, "profileId");
  const targetType = optionalEnum(body, "targetType", [
    "profile",
    "user",
  ] as const);
  const targetId = optionalString(body, "targetId");
  if (targetType === "user" && targetId === undefined) {
    throw new StorageQueryInputError(
      "target_id_required",
      "targetId is required when targetType is user",
    );
  }
  const { limit, offset } = pageInput(body, 25);
  const query: NativeProfileMemoryQuery = compactRecord({
    profileId,
    targetType,
    targetId,
    limit,
    offset,
  }) as unknown as NativeProfileMemoryQuery;
  const items = await context.bridge.listProfileMemory(query);
  return {
    query_id: "profile.memory",
    read_only: true,
    source: "rust_bridge_read_model",
    items,
    total: items.length,
    limit,
    offset,
  };
}

async function conversationBranches(
  body: Record<string, unknown>,
  context: StorageQueryContext,
): Promise<StorageQueryResult> {
  const { limit, offset } = pageInput(body, 25);
  const query = compactRecord({
    session_id: optionalString(body, "sessionId"),
    parent_branch_id: optionalString(body, "parentBranchId"),
    page: { limit, offset },
  });
  const items = await context.bridge.queryConversationBranches(query);
  return {
    query_id: "conversation.branches",
    read_only: true,
    source: "rust_bridge_read_model",
    items: items as unknown[],
    total: (items as unknown[]).length,
    limit,
    offset,
  };
}

async function runtimeCounters(
  body: Record<string, unknown>,
  context: StorageQueryContext,
): Promise<StorageQueryResult> {
  const { limit, offset } = pageInput(body, 25);
  const scopeType = optionalEnum(body, "scopeType", [
    "runtime",
    "agent",
    "instance",
    "session",
  ] as const);
  const query: NativeRuntimeCounterQuery = compactRecord({
    scopeType,
    scopeId: optionalString(body, "scopeId"),
    counterName: optionalString(body, "counterName"),
    limit,
    offset,
  }) as NativeRuntimeCounterQuery;
  const items = await context.bridge.queryRuntimeCounters(query);
  return {
    query_id: "runtime.counters",
    read_only: true,
    source: "rust_bridge_read_model",
    items,
    total: items.length,
    limit,
    offset,
  };
}

function pageResult<T>(
  queryId: StorageQueryId,
  rows: readonly T[],
  limit: number,
  offset: number,
  extra: Omit<
    Partial<StorageQueryResult<T, unknown>>,
    | "items"
    | "limit"
    | "nextOffset"
    | "offset"
    | "query_id"
    | "read_only"
    | "source"
    | "total"
  > = {},
): StorageQueryResult<T> {
  const items = rows.slice(offset, offset + limit);
  return {
    query_id: queryId,
    read_only: true,
    source: "rust_bridge_read_model",
    items,
    total: rows.length,
    limit,
    offset,
    ...(offset + items.length < rows.length
      ? { nextOffset: offset + items.length }
      : {}),
    ...extra,
  };
}

function decodeStorageQueryId(pathname: string): StorageQueryId | undefined {
  const prefix = "/v1/admin/storage/query/";
  if (!pathname.startsWith(prefix)) return undefined;
  return parseStorageQueryId(decodeURIComponent(pathname.slice(prefix.length)));
}

function parseStorageQueryId(value: string): StorageQueryId | undefined {
  return STORAGE_QUERY_DESCRIPTORS.some((descriptor) => descriptor.id === value)
    ? (value as StorageQueryId)
    : undefined;
}

function parameter(
  name: string,
  type: StorageQueryParameter["type"],
  required: boolean,
  description: string,
  defaultValue?: unknown,
): StorageQueryParameter {
  return {
    name,
    type,
    required,
    description,
    ...(defaultValue === undefined ? {} : { defaultValue }),
  };
}

function enumParameter(
  name: string,
  required: boolean,
  description: string,
  enumValues: readonly string[],
): StorageQueryParameter {
  return {
    name,
    type: "enum",
    required,
    description,
    enumValues,
  };
}

function routeSuccess<T>(requestId: string, data: T): AdminRouteResult<T> {
  return {
    status: 200,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data,
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

function routeFailure(
  status: number,
  requestId: string,
  input: { reason_code: string; message: string },
): AdminRouteResult {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error: {
        code:
          status === 404
            ? "not_found"
            : status === 405
              ? "method_not_allowed"
              : "invalid_input",
        reason_code: input.reason_code,
        message: input.message,
        retryable: false,
      },
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

function recordBody(input: unknown): Record<string, unknown> {
  if (input === undefined || input === null) return {};
  if (typeof input !== "object" || Array.isArray(input)) {
    throw new StorageQueryInputError(
      "storage_query_body_must_be_object",
      "storage query body must be a JSON object",
    );
  }
  return input as Record<string, unknown>;
}

function requiredString(body: Record<string, unknown>, key: string): string {
  const value = optionalString(body, key);
  if (value === undefined) {
    throw new StorageQueryInputError(
      "required_parameter_missing",
      `${key} is required`,
    );
  }
  return value;
}

function optionalString(
  body: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || value.trim() === "") {
    throw new StorageQueryInputError(
      "invalid_string_parameter",
      `${key} must be a non-empty string`,
    );
  }
  return value.trim();
}

function boundedString(
  body: Record<string, unknown>,
  key: string,
  maxBytes: number,
  required: true,
): string;
function boundedString(
  body: Record<string, unknown>,
  key: string,
  maxBytes: number,
  required: false,
): string | undefined;
function boundedString(
  body: Record<string, unknown>,
  key: string,
  maxBytes: number,
  required: boolean,
): string | undefined {
  const value = required ? requiredString(body, key) : optionalString(body, key);
  if (value === undefined) return undefined;
  if (Buffer.byteLength(value, "utf8") > maxBytes) {
    throw new StorageQueryInputError(
      "string_parameter_too_long",
      `${key} must be ${maxBytes} bytes or less`,
    );
  }
  if (value.includes("\0")) {
    throw new StorageQueryInputError(
      "invalid_string_parameter",
      `${key} must not contain NUL bytes`,
    );
  }
  return value;
}

function optionalEnum<const T extends readonly string[]>(
  body: Record<string, unknown>,
  key: string,
  values: T,
): T[number] | undefined {
  const value = optionalString(body, key);
  if (value === undefined) return undefined;
  if (!values.includes(value)) {
    throw new StorageQueryInputError(
      "invalid_enum_parameter",
      `${key} must be one of ${values.join(", ")}`,
    );
  }
  return value;
}

function pageInput(
  body: Record<string, unknown>,
  defaultLimit = 100,
): { limit: number; offset: number } {
  return {
    limit: integer(body, "limit", defaultLimit, 1, MAX_LIMIT),
    offset: integer(body, "offset", 0, 0, Number.MAX_SAFE_INTEGER),
  };
}

function integer(
  body: Record<string, unknown>,
  key: string,
  fallback: number,
  min: number,
  max: number,
): number {
  const value = body[key];
  if (value === undefined || value === null) return fallback;
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < min ||
    value > max
  ) {
    throw new StorageQueryInputError(
      "invalid_integer_parameter",
      `${key} must be an integer between ${min} and ${max}`,
    );
  }
  return value;
}

function compactRecord(
  value: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(
      ([, entry]) => entry !== undefined && entry !== null,
    ),
  );
}

function toolResult<T>(details: T): BrainToolResult<T> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

class StorageQueryInputError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
    this.name = "StorageQueryInputError";
  }
}
