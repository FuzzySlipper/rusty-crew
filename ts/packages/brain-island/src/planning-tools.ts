import type {
  NativeBridgeModule,
  NativeRuntimeCounterRecord,
  NativeRuntimeCounterScopeType,
  NativeRuntimeCounterSummary,
  NativeRuntimeSearchResult,
} from "@rusty-crew/native-bridge";
import type {
  AgentTool as PiAgentTool,
  AgentToolResult,
} from "@earendil-works/pi-agent-core";
import { Type, type Static } from "typebox";

export type TodoStatus =
  | "pending"
  | "in_progress"
  | "done"
  | "blocked"
  | "cancelled";

export interface TodoItem {
  id: string;
  title: string;
  status: TodoStatus;
  notes?: string;
  updatedAt?: string;
}

export interface SessionTodoState {
  sessionId: string;
  items: readonly TodoItem[];
  updatedAt?: string;
  expiresAt?: string;
}

export interface SessionTodoStore {
  read(sessionId: string): SessionTodoState;
  replace(
    sessionId: string,
    items: readonly TodoItem[],
    ttlMs?: number,
  ): SessionTodoState;
  merge(
    sessionId: string,
    items: readonly TodoItem[],
    ttlMs?: number,
  ): SessionTodoState;
}

export interface MemorySessionTodoStoreOptions {
  now?: () => Date;
  maxItems?: number;
}

export class MemorySessionTodoStore implements SessionTodoStore {
  private readonly states = new Map<string, SessionTodoState>();
  private readonly now: () => Date;
  private readonly maxItems: number;

  constructor(options: MemorySessionTodoStoreOptions = {}) {
    this.now = options.now ?? (() => new Date());
    this.maxItems = options.maxItems ?? 50;
  }

  read(sessionId: string): SessionTodoState {
    const state = this.states.get(sessionId);
    if (!state || this.isExpired(state)) {
      this.states.delete(sessionId);
      return { sessionId, items: [] };
    }
    return state;
  }

  replace(
    sessionId: string,
    items: readonly TodoItem[],
    ttlMs?: number,
  ): SessionTodoState {
    const state = this.nextState(sessionId, items, ttlMs);
    this.states.set(sessionId, state);
    return state;
  }

  merge(
    sessionId: string,
    items: readonly TodoItem[],
    ttlMs?: number,
  ): SessionTodoState {
    const existing = new Map(
      this.read(sessionId).items.map((item) => [item.id, item]),
    );
    for (const item of items) {
      existing.set(item.id, { ...existing.get(item.id), ...item });
    }
    return this.replace(sessionId, [...existing.values()], ttlMs);
  }

  private nextState(
    sessionId: string,
    items: readonly TodoItem[],
    ttlMs: number | undefined,
  ): SessionTodoState {
    if (items.length > this.maxItems) {
      throw new TodoInputError("todo_too_many_items");
    }
    const now = this.now();
    return {
      sessionId,
      items: items.map(normalizeTodoItem),
      updatedAt: now.toISOString(),
      expiresAt: ttlMs
        ? new Date(now.getTime() + ttlMs).toISOString()
        : undefined,
    };
  }

  private isExpired(state: SessionTodoState): boolean {
    return Boolean(
      state.expiresAt && state.expiresAt <= this.now().toISOString(),
    );
  }
}

export interface TodoToolContext {
  store: SessionTodoStore;
  sessionId?: string;
  maxItems?: number;
}

export interface TodoToolDetails {
  ok: boolean;
  operation: "read" | "replace" | "merge";
  reasonCode?: string;
  state?: SessionTodoState;
}

const todoStatusSchema = Type.Union([
  Type.Literal("pending"),
  Type.Literal("in_progress"),
  Type.Literal("done"),
  Type.Literal("blocked"),
  Type.Literal("cancelled"),
]);

const todoItemSchema = Type.Object({
  id: Type.String({ minLength: 1 }),
  title: Type.String({ minLength: 1 }),
  status: todoStatusSchema,
  notes: Type.Optional(Type.String()),
});

const todoParameters = Type.Object({
  action: Type.Union([
    Type.Literal("read"),
    Type.Literal("replace"),
    Type.Literal("merge"),
  ]),
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
  items: Type.Optional(Type.Array(todoItemSchema)),
  ttlMs: Type.Optional(Type.Number({ minimum: 1 })),
});

type TodoParams = Static<typeof todoParameters>;

export interface SessionSearchToolContext {
  client?: Pick<NativeBridgeModule, "searchRuntime">;
  maxBodyChars?: number;
}

export type CounterResetTriggerType =
  | "manual"
  | "maintenance"
  | "governance_review";

export interface CounterResetToolContext {
  client?: Pick<
    NativeBridgeModule,
    "queryRuntimeCounters" | "runtimeSummary" | "resetRuntimeCounters"
  >;
  allowReset?: boolean;
}

export interface CounterResetToolDetails {
  ok: boolean;
  operation: "query" | "summary" | "reset";
  reasonCode?: string;
  triggerType?: CounterResetTriggerType;
  resetRows?: number;
  records?: readonly NativeRuntimeCounterRecord[];
  summary?: NativeRuntimeCounterSummary;
}

export interface SessionSearchToolDetails {
  ok: boolean;
  operation: "search";
  reasonCode?: string;
  results?: readonly SessionSearchResult[];
}

export interface SessionSearchResult {
  rowType: "message" | "queue_message" | "session";
  rowKey: string;
  sequence?: number;
  sessionId?: string;
  agentId?: string;
  instanceId?: string;
  taskId?: string;
  eventKind?: string;
  recordedAt: string;
  title: string;
  bodySnippet: string;
  truncated: boolean;
}

const sessionSearchParameters = Type.Object({
  query: Type.String({ minLength: 1 }),
  rowType: Type.Optional(
    Type.Union([
      Type.Literal("message"),
      Type.Literal("queue_message"),
      Type.Literal("session"),
    ]),
  ),
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
  agentId: Type.Optional(Type.String({ minLength: 1 })),
  instanceId: Type.Optional(Type.String({ minLength: 1 })),
  taskId: Type.Optional(Type.String({ minLength: 1 })),
  eventKind: Type.Optional(Type.String({ minLength: 1 })),
  recordedAfter: Type.Optional(Type.String({ minLength: 1 })),
  recordedBefore: Type.Optional(Type.String({ minLength: 1 })),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
});

type SessionSearchParams = Static<typeof sessionSearchParameters>;

const runtimeCounterScopeTypeSchema = Type.Union([
  Type.Literal("runtime"),
  Type.Literal("agent"),
  Type.Literal("instance"),
  Type.Literal("session"),
]);

const counterResetTriggerTypeSchema = Type.Union([
  Type.Literal("manual"),
  Type.Literal("maintenance"),
  Type.Literal("governance_review"),
]);

const counterResetParameters = Type.Object({
  action: Type.Union([
    Type.Literal("query"),
    Type.Literal("summary"),
    Type.Literal("reset"),
  ]),
  scopeType: Type.Optional(runtimeCounterScopeTypeSchema),
  scopeId: Type.Optional(Type.String({ minLength: 1 })),
  counterName: Type.Optional(Type.String({ minLength: 1 })),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  offset: Type.Optional(Type.Number({ minimum: 0 })),
  triggerType: Type.Optional(counterResetTriggerTypeSchema),
  reason: Type.Optional(Type.String({ minLength: 1 })),
  confirm: Type.Optional(Type.Boolean()),
});

type CounterResetParams = Static<typeof counterResetParameters>;

export function todoTool(
  context: TodoToolContext,
): PiAgentTool<typeof todoParameters, TodoToolDetails> {
  return {
    name: "todo",
    label: "Session todo",
    description:
      "Read or update bounded session-local planning notes. This is not Den task truth.",
    parameters: todoParameters,
    execute: async (_toolCallId, params: TodoParams) => {
      const sessionId = params.sessionId ?? context.sessionId;
      if (!sessionId) {
        return todoResult({
          ok: false,
          operation: params.action,
          reasonCode: "todo_session_id_missing",
        });
      }
      try {
        switch (params.action) {
          case "read":
            return todoResult({
              ok: true,
              operation: "read",
              state: context.store.read(sessionId),
            });
          case "replace":
            return todoResult({
              ok: true,
              operation: "replace",
              state: context.store.replace(
                sessionId,
                params.items ?? [],
                params.ttlMs,
              ),
            });
          case "merge":
            return todoResult({
              ok: true,
              operation: "merge",
              state: context.store.merge(
                sessionId,
                params.items ?? [],
                params.ttlMs,
              ),
            });
        }
      } catch (error) {
        return todoResult({
          ok: false,
          operation: params.action,
          reasonCode:
            error instanceof TodoInputError
              ? error.reasonCode
              : "todo_update_failed",
        });
      }
    },
  };
}

export function renderSessionTodoContext(
  state: SessionTodoState | undefined,
): string | undefined {
  if (!state || state.items.length === 0) return undefined;
  return [
    "# Session Todo",
    "Session-local planning notes only. These are not Den tasks or durable project truth.",
    ...state.items.map((item) =>
      [
        `- [${item.status}] ${item.id}: ${item.title}`,
        item.notes ? `  Notes: ${item.notes}` : undefined,
      ]
        .filter((line): line is string => Boolean(line))
        .join("\n"),
    ),
  ].join("\n");
}

export function sessionSearchTool(
  context: SessionSearchToolContext,
): PiAgentTool<typeof sessionSearchParameters, SessionSearchToolDetails> {
  return {
    name: "session_search",
    label: "Search runtime sessions",
    description:
      "Search Rust-owned runtime session and message history. Does not search Den product data.",
    parameters: sessionSearchParameters,
    execute: async (_toolCallId, params: SessionSearchParams) => {
      if (!context.client) {
        return result({
          ok: false,
          operation: "search",
          reasonCode: "runtime_search_client_unavailable",
        });
      }
      try {
        const rows = await context.client.searchRuntime({
          query: params.query,
          rowType: params.rowType,
          sessionId: params.sessionId,
          agentId: params.agentId,
          instanceId: params.instanceId,
          taskId: params.taskId,
          eventKind: params.eventKind,
          recordedAfter: params.recordedAfter,
          recordedBefore: params.recordedBefore,
          limit: params.limit,
        });
        return result({
          ok: true,
          operation: "search",
          results: rows.map((row) => normalizeResult(row, context)),
        });
      } catch (error) {
        return result({
          ok: false,
          operation: "search",
          reasonCode: "runtime_search_failed",
          results: [
            {
              rowType: "message",
              rowKey: "error",
              recordedAt: "",
              title: "Runtime search failed",
              bodySnippet:
                error instanceof Error ? error.message : String(error),
              truncated: false,
            },
          ],
        });
      }
    },
  };
}

export function counterResetTool(
  context: CounterResetToolContext,
): PiAgentTool<typeof counterResetParameters, CounterResetToolDetails> {
  return {
    name: "counter_reset",
    label: "Runtime counters",
    description:
      "Query, summarize, or explicitly reset Rust-owned derived runtime counters.",
    parameters: counterResetParameters,
    execute: async (_toolCallId, params: CounterResetParams) => {
      if (!context.client) {
        return counterResult({
          ok: false,
          operation: params.action,
          reasonCode: "runtime_counter_client_unavailable",
        });
      }

      const scopeType = params.scopeType ?? "runtime";
      const invalidScope = validateCounterScope(scopeType, params.scopeId);
      if (invalidScope) {
        return counterResult({
          ok: false,
          operation: params.action,
          reasonCode: invalidScope,
        });
      }

      try {
        if (params.action === "summary") {
          return counterResult({
            ok: true,
            operation: "summary",
            triggerType: params.triggerType ?? "manual",
            summary: await context.client.runtimeSummary({
              scopeType,
              scopeId: params.scopeId,
            }),
          });
        }

        const query = {
          scopeType,
          scopeId: params.scopeId,
          counterName: params.counterName,
          limit: params.limit,
          offset: params.offset,
        };

        if (params.action === "query") {
          return counterResult({
            ok: true,
            operation: "query",
            triggerType: params.triggerType ?? "manual",
            records: await context.client.queryRuntimeCounters(query),
          });
        }

        if (!context.allowReset) {
          return counterResult({
            ok: false,
            operation: "reset",
            reasonCode: "runtime_counter_reset_disabled",
          });
        }
        if (params.confirm !== true) {
          return counterResult({
            ok: false,
            operation: "reset",
            reasonCode: "runtime_counter_reset_confirmation_required",
          });
        }
        if (!params.triggerType) {
          return counterResult({
            ok: false,
            operation: "reset",
            reasonCode: "runtime_counter_reset_trigger_required",
          });
        }
        if (!params.reason?.trim()) {
          return counterResult({
            ok: false,
            operation: "reset",
            reasonCode: "runtime_counter_reset_reason_required",
          });
        }

        const resetRows = await context.client.resetRuntimeCounters(query);
        return counterResult({
          ok: true,
          operation: "reset",
          triggerType: params.triggerType,
          resetRows,
          records: await context.client.queryRuntimeCounters(query),
        });
      } catch (error) {
        return counterResult({
          ok: false,
          operation: params.action,
          reasonCode: "runtime_counter_operation_failed",
          records: [
            {
              scopeType,
              scopeId: params.scopeId ?? "_global",
              counterName: "error",
              value: 0,
              updatedAt: error instanceof Error ? error.message : String(error),
            },
          ],
        });
      }
    },
  };
}

function todoResult(
  details: TodoToolDetails,
): AgentToolResult<TodoToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function normalizeTodoItem(item: TodoItem): TodoItem {
  if (!item.id.trim() || !item.title.trim()) {
    throw new TodoInputError("todo_item_invalid");
  }
  return {
    ...item,
    id: item.id.trim(),
    title: item.title.trim(),
  };
}

class TodoInputError extends Error {
  constructor(readonly reasonCode: string) {
    super(reasonCode);
    this.name = "TodoInputError";
  }
}

function normalizeResult(
  row: NativeRuntimeSearchResult,
  context: SessionSearchToolContext,
): SessionSearchResult {
  const maxBodyChars = context.maxBodyChars ?? 2_000;
  return {
    rowType: row.rowType,
    rowKey: row.rowKey,
    sequence: row.sequence,
    sessionId: row.sessionId,
    agentId: row.agentId,
    instanceId: row.instanceId,
    taskId: row.taskId,
    eventKind: row.eventKind,
    recordedAt: row.recordedAt,
    title: row.title,
    bodySnippet: row.body.slice(0, maxBodyChars),
    truncated: row.body.length > maxBodyChars,
  };
}

function validateCounterScope(
  scopeType: NativeRuntimeCounterScopeType,
  scopeId: string | undefined,
): string | undefined {
  if (scopeType === "runtime") return undefined;
  if (!scopeId?.trim()) return "runtime_counter_scope_id_required";
  return undefined;
}

function counterResult(
  details: CounterResetToolDetails,
): AgentToolResult<CounterResetToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function result(
  details: SessionSearchToolDetails,
): AgentToolResult<SessionSearchToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}
