export type DenMemoryFetch = typeof fetch;

export interface DenMemoryClientOptions {
  baseUrl: string;
  bearerToken?: string;
  fetchImpl?: DenMemoryFetch;
  timeoutMs?: number;
  paths?: Partial<DenMemoryClientPaths>;
  apiMode?: DenMemoryApiMode;
}

export type DenMemoryApiMode = "v1" | "den-memories-v0";

export interface DenMemoryClientPaths {
  read: string;
  search: string;
  recall: string;
  store: string;
  propose: string;
}

export interface DenMemorySourceRef {
  kind: string;
  ref: string;
  label?: string;
}

export interface DenMemoryRuntimeContext {
  projectId?: string;
  taskId?: string | number;
  sessionId?: string;
  agentId?: string;
  profileId?: string;
  runId?: string;
}

export interface DenMemoryScope {
  audience?: readonly string[];
  role?: string;
  mode?: "personal" | "project" | "shared" | string;
}

export interface DenMemoryRecord {
  id: string;
  slug?: string;
  title?: string;
  summary?: string;
  bodyMarkdown?: string;
  score?: number;
  audience?: readonly string[];
  role?: string;
  mode?: string;
  sourceRefs?: readonly DenMemorySourceRef[];
  metadata?: Record<string, unknown>;
  createdAt?: string;
  updatedAt?: string;
}

export interface DenMemoryReadRequest {
  id?: string;
  slug?: string;
  context?: DenMemoryRuntimeContext;
}

export interface DenMemorySearchRequest extends DenMemoryScope {
  query: string;
  limit?: number;
  context?: DenMemoryRuntimeContext;
  sourceRefs?: readonly DenMemorySourceRef[];
  metadata?: Record<string, unknown>;
}

export interface DenMemoryRecallRequest extends DenMemoryScope {
  prompt: string;
  limit?: number;
  context?: DenMemoryRuntimeContext;
  sourceRefs?: readonly DenMemorySourceRef[];
  metadata?: Record<string, unknown>;
}

export interface DenMemoryStoreRequest extends DenMemoryScope {
  title?: string;
  summary?: string;
  bodyMarkdown: string;
  context?: DenMemoryRuntimeContext;
  sourceRefs?: readonly DenMemorySourceRef[];
  metadata?: Record<string, unknown>;
}

export interface DenMemoryProposeRequest extends DenMemoryStoreRequest {
  proposalKind?: "store" | "update" | "delete" | string;
  targetMemoryId?: string;
}

export interface DenMemoryListResponse {
  memories: DenMemoryRecord[];
  total?: number;
  nextCursor?: string;
}

export interface DenMemoryMutationResponse {
  accepted: boolean;
  memory?: DenMemoryRecord;
  proposalId?: string;
  reasonCode?: string;
  message?: string;
}

export interface DenMemoryClient {
  read(request: DenMemoryReadRequest): Promise<DenMemoryRecord>;
  search(request: DenMemorySearchRequest): Promise<DenMemoryListResponse>;
  recall(request: DenMemoryRecallRequest): Promise<DenMemoryListResponse>;
  store(request: DenMemoryStoreRequest): Promise<DenMemoryMutationResponse>;
  propose(request: DenMemoryProposeRequest): Promise<DenMemoryMutationResponse>;
}

export class DenMemoryClientError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly options: {
      status?: number;
      reasonCode?: string;
      retryable: boolean;
      cause?: unknown;
    },
  ) {
    super(message);
    this.name = "DenMemoryClientError";
  }
}

const defaultPaths = {
  read: "/v1/memories/read",
  search: "/v1/memories/search",
  recall: "/v1/memories/recall",
  store: "/v1/memories/store",
  propose: "/v1/memories/propose",
} satisfies DenMemoryClientPaths;

export function createDenMemoryClient(
  options: DenMemoryClientOptions,
): DenMemoryClient {
  if (options.apiMode === "den-memories-v0") {
    return new DenMemoryV0Client(options);
  }
  const transport = new DenMemoryHttpTransport(options);
  return {
    read: (request) => transport.post<DenMemoryRecord>("read", request),
    search: (request) =>
      transport.post<DenMemoryListResponse>("search", request),
    recall: (request) =>
      transport.post<DenMemoryListResponse>("recall", request),
    store: (request) =>
      transport.post<DenMemoryMutationResponse>("store", request),
    propose: (request) =>
      transport.post<DenMemoryMutationResponse>("propose", request),
  };
}

class DenMemoryV0Client implements DenMemoryClient {
  private readonly transport: DenMemoryHttpTransport;

  constructor(options: DenMemoryClientOptions) {
    this.transport = new DenMemoryHttpTransport({
      ...options,
      paths: {
        recall: "/api/recall",
        read: "/api/memory-entries",
        search: "/api/memory-entries/search",
        store: "/api/memory-entries",
        propose: "/api/candidates",
        ...options.paths,
      },
    });
  }

  async read(request: DenMemoryReadRequest): Promise<DenMemoryRecord> {
    const slug = request.slug ?? request.id;
    if (!slug) {
      throw new DenMemoryClientError(
        "invalid_request",
        "Den Memories v0 read requires id or slug",
        { retryable: false },
      );
    }
    const entry = await this.transport.request<DenMemoryV0Record>(
      "GET",
      `${this.transport.path("read")}/${encodeURIComponent(slug)}`,
    );
    return memoryRecordFromV0(entry);
  }

  async search(
    request: DenMemorySearchRequest,
  ): Promise<DenMemoryListResponse> {
    const response = await this.transport.post<DenMemoryV0Record[]>("search", {
      query: request.query,
      limit: request.limit,
      runtime_context: runtimeContextV0(request.context, request),
    });
    const records = Array.isArray(response) ? response : [];
    return {
      memories: records.map(memoryRecordFromV0),
      total: records.length,
    };
  }

  async recall(
    request: DenMemoryRecallRequest,
  ): Promise<DenMemoryListResponse> {
    const packet = await this.transport.post<DenMemoryV0RecallPacket>(
      "recall",
      {
        query: request.prompt,
        runtime_context: runtimeContextV0(request.context, request),
        audience: request.audience,
        mode: request.mode,
        budget_tokens: request.metadata?.budgetTokens,
      },
    );
    const rootMatches = Array.isArray(packet.root_matches)
      ? packet.root_matches
      : [];
    const packetMarkdown = stringValue(packet.packet_md) ?? "";
    return {
      memories: [
        {
          id: stringValue(packet.packet_id) ?? "den-memory-recall-packet",
          title: "Den Memories recall packet",
          summary: packetMarkdown,
          bodyMarkdown: packetMarkdown,
          metadata: {
            packet,
            rootMatchCount: rootMatches.length,
          },
        },
      ],
      total: rootMatches.length,
    };
  }

  async store(
    request: DenMemoryStoreRequest,
  ): Promise<DenMemoryMutationResponse> {
    const entry = await this.transport.post<DenMemoryV0Record>("store", {
      ...entryPayloadV0(request),
      created_by: request.context?.agentId ?? request.context?.profileId,
    });
    return {
      accepted: true,
      memory: memoryRecordFromV0(entry),
    };
  }

  async propose(
    request: DenMemoryProposeRequest,
  ): Promise<DenMemoryMutationResponse> {
    const candidate = await this.transport.post<DenMemoryV0Record>("propose", {
      ...entryPayloadV0(request),
      proposer_identity: request.context?.agentId ?? request.context?.profileId,
      proposer_kind: "rusty_crew",
      proposed_kind: request.proposalKind ?? "fact",
    });
    return {
      accepted: true,
      proposalId: stringValue(candidate.id) ?? stringValue(candidate.slug),
      memory: memoryRecordFromV0(candidate),
    };
  }
}

class DenMemoryHttpTransport {
  private readonly baseUrl: URL;
  private readonly fetchImpl: DenMemoryFetch;
  private readonly timeoutMs: number;
  private readonly bearerToken?: string;
  private readonly paths: DenMemoryClientPaths;

  constructor(options: DenMemoryClientOptions) {
    this.baseUrl = new URL(options.baseUrl);
    this.fetchImpl = options.fetchImpl ?? fetch;
    this.timeoutMs = options.timeoutMs ?? 5_000;
    this.bearerToken = options.bearerToken;
    this.paths = { ...defaultPaths, ...options.paths };
  }

  async post<T>(
    pathKey: keyof DenMemoryClientPaths,
    body: unknown,
  ): Promise<T> {
    return this.request<T>("POST", this.paths[pathKey], body);
  }

  async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const response = await this.fetchImpl(this.url(path), {
        method,
        signal: controller.signal,
        headers: this.headers(),
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      return await decodeMemoryResponse<T>(response);
    } catch (error) {
      throw normalizeMemoryError(error, "den_memory_transport_failed");
    } finally {
      clearTimeout(timeout);
    }
  }

  path(pathKey: keyof DenMemoryClientPaths): string {
    return this.paths[pathKey];
  }

  private url(path: string): string {
    return new URL(path, this.baseUrl).toString();
  }

  private headers(): Record<string, string> {
    return {
      accept: "application/json",
      "content-type": "application/json",
      ...(this.bearerToken
        ? { authorization: `Bearer ${this.bearerToken}` }
        : {}),
    };
  }
}

interface DenMemoryV0RecallPacket {
  packet_id?: unknown;
  packet_md?: unknown;
  root_matches?: unknown;
  included_nodes?: unknown;
  skipped?: unknown;
  warnings?: unknown;
  provenance?: unknown;
}

type DenMemoryV0Record = Record<string, unknown>;

function runtimeContextV0(
  context: DenMemoryRuntimeContext | undefined,
  scope: DenMemoryScope,
): Record<string, unknown> {
  return stripUndefined({
    runtime: "pi_crew",
    agent_identity: context?.agentId,
    profile_id: context?.profileId ?? context?.agentId,
    agent_instance_id: context?.agentId
      ? `${context.agentId}@rusty-crew`
      : undefined,
    session_id: context?.sessionId,
    session_key: context?.sessionId,
    session_kind: "durable_agent",
    project_id: context?.projectId,
    task_id: context?.taskId,
    run_id: context?.runId,
    role: scope.role ?? "runner",
    audience: scope.audience,
    mode: scope.mode ?? "general",
    source_surface: "rusty_crew",
  });
}

function entryPayloadV0(
  request: DenMemoryStoreRequest | DenMemoryProposeRequest,
): Record<string, unknown> {
  return stripUndefined({
    slug: request.title ? slugify(request.title) : undefined,
    title: request.title ?? "Rusty Crew memory",
    summary: request.summary,
    body_md: request.bodyMarkdown,
    proposed_kind:
      "proposalKind" in request ? (request.proposalKind ?? "fact") : "fact",
    kind: "fact",
    scope_kind: request.mode ?? "project",
    scope_id: request.context?.projectId,
    authority_scope_kind: request.mode ?? "project",
    authority_scope_id: request.context?.projectId,
    discovery_scope: "same_project",
    claim_strength: "observation",
    audience: request.audience,
    source_refs: request.sourceRefs?.map(sourceRefV0),
    runtime_context: runtimeContextV0(request.context, request),
  });
}

function sourceRefV0(ref: DenMemorySourceRef): Record<string, unknown> {
  return stripUndefined({
    source_kind: ref.kind,
    source_id: ref.ref,
    source_summary: ref.label,
  });
}

function memoryRecordFromV0(record: DenMemoryV0Record): DenMemoryRecord {
  return {
    id: String(record.id ?? record.slug ?? "den-memory-v0-record"),
    slug: stringValue(record.slug),
    title: stringValue(record.title),
    summary: stringValue(record.summary),
    bodyMarkdown:
      stringValue(record.body_md) ?? stringValue(record.bodyMarkdown),
    audience:
      stringArrayValue(record.audience_json) ??
      stringArrayValue(record.audience),
    role: stringValue(record.role),
    mode: stringValue(record.scope_kind),
    metadata: {
      v0: record,
    },
    createdAt: stringValue(record.created_at),
    updatedAt: stringValue(record.updated_at),
  };
}

function stripUndefined(
  input: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(input).filter(([, value]) => value !== undefined),
  );
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
}

function stringArrayValue(value: unknown): string[] | undefined {
  if (Array.isArray(value)) {
    return value.filter((item): item is string => typeof item === "string");
  }
  if (typeof value === "string") {
    try {
      const parsed = JSON.parse(value) as unknown;
      return stringArrayValue(parsed);
    } catch {
      return undefined;
    }
  }
  return undefined;
}

async function decodeMemoryResponse<T>(response: Response): Promise<T> {
  let body: unknown;
  try {
    body = await response.json();
  } catch (error) {
    throw new DenMemoryClientError(
      "invalid_json",
      `Den Memories returned non-JSON response with status ${response.status}`,
      {
        status: response.status,
        retryable: response.status >= 500,
        cause: error,
      },
    );
  }

  if (response.ok) {
    return unwrapMemoryData<T>(body);
  }

  const error = memoryErrorBody(body);
  throw new DenMemoryClientError(error.code, error.message, {
    status: response.status,
    reasonCode: error.reasonCode,
    retryable: error.retryable ?? response.status >= 500,
  });
}

function unwrapMemoryData<T>(body: unknown): T {
  if (isRecord(body) && body.ok === true && "data" in body) {
    return body.data as T;
  }
  return body as T;
}

function memoryErrorBody(body: unknown): {
  code: string;
  reasonCode?: string;
  message: string;
  retryable?: boolean;
} {
  if (isRecord(body) && body.ok === false && isRecord(body.error)) {
    return {
      code: stringValue(body.error.code) ?? "den_memory_error",
      reasonCode:
        stringValue(body.error.reason_code) ??
        stringValue(body.error.reasonCode),
      message: stringValue(body.error.message) ?? "Den Memories request failed",
      retryable:
        typeof body.error.retryable === "boolean"
          ? body.error.retryable
          : undefined,
    };
  }
  if (isRecord(body) && isRecord(body.error)) {
    return {
      code: stringValue(body.error.code) ?? "den_memory_error",
      reasonCode:
        stringValue(body.error.reason_code) ??
        stringValue(body.error.reasonCode),
      message: stringValue(body.error.message) ?? "Den Memories request failed",
      retryable:
        typeof body.error.retryable === "boolean"
          ? body.error.retryable
          : undefined,
    };
  }
  return {
    code: "den_memory_error",
    message: "Den Memories request failed",
  };
}

function normalizeMemoryError(
  error: unknown,
  code: string,
): DenMemoryClientError {
  if (error instanceof DenMemoryClientError) {
    return error;
  }
  const aborted =
    error instanceof Error &&
    (error.name === "AbortError" || error.message.includes("aborted"));
  return new DenMemoryClientError(
    aborted ? "timeout" : code,
    error instanceof Error ? error.message : "Den Memories request failed",
    {
      retryable: true,
      cause: error,
    },
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
