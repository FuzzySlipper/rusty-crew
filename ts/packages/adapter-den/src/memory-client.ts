export type DenMemoryFetch = typeof fetch;

export interface DenMemoryClientOptions {
  baseUrl: string;
  bearerToken?: string;
  fetchImpl?: DenMemoryFetch;
  timeoutMs?: number;
  paths?: Partial<DenMemoryClientPaths>;
}

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
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const response = await this.fetchImpl(this.url(this.paths[pathKey]), {
        method: "POST",
        signal: controller.signal,
        headers: this.headers(),
        body: JSON.stringify(body),
      });
      return await decodeMemoryResponse<T>(response);
    } catch (error) {
      throw normalizeMemoryError(error, "den_memory_transport_failed");
    } finally {
      clearTimeout(timeout);
    }
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
